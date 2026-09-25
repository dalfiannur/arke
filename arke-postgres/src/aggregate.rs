//! Agregasi typed di atas [`Query`] (`SUM`/`MIN`/`MAX`/`AVG`/`COUNT`/
//! `COUNT(DISTINCT)` + `GROUP BY` satu/multi-kunci + `HAVING`) — hasilnya
//! skalar, bukan entity.
//!
//! ```ignore
//! let total: Option<i64> = store.query::<Booking>()
//!     .filter(Booking::room().eq(7))
//!     .sum::<i64>(Booking::minutes()).await?;
//! let per_room: Vec<(i64, u64)> = store.query::<Booking>()
//!     .group_by(Booking::room()).count().await?;
//! ```
//!
//! Tipe hasil `A` ([`FromPgScalar`]) menentukan **cast SQL eksplisit**
//! (`SUM(x)::bigint`), sehingga aturan Postgres `SUM(int)→bigint`,
//! `SUM(bigint)→numeric`, `AVG→numeric` tidak bocor ke pemanggil. Untuk nilai
//! eksak di luar `i64`/`f64` (uang di NUMERIC), minta `String` (`::text`).
//! Agregat atas nol baris → `None`.
//!
//! ```ignore
//! use arke_postgres::aggregate as agg;
//! // Per (room, kind) yang total menitnya ≥ 200:
//! let rows: Vec<((i64, String), Option<i64>)> = store.query::<Booking>()
//!     .group_by((Booking::room(), Booking::kind()))
//!     .having(agg::sum(Booking::minutes()).gte(200))
//!     .sum::<i64>(Booking::minutes()).await?;
//! ```
//!
//! Window function tidak ada — pakai `load_where`/SQL.

use sqlx::postgres::PgRow;
use sqlx::{Row, ValueRef};

use std::marker::PhantomData;

use crate::query::{Field, IntoPgValue, Query, fetch_scalar_rows, renumber};
use crate::{PgComponent, PgType, PgValue, quote_ident};

/// Tipe hasil agregat/kunci grup yang dapat dibaca dari satu kolom hasil.
pub trait FromPgScalar: Sized {
    /// Cast SQL yang dipaksakan pada ekspresi (`"::bigint"`), agar tipe wire
    /// deterministik.
    const CAST: &'static str;
    /// Baca kolom `col` dari `row`.
    fn from_row(row: &PgRow, col: &str) -> Result<Self, sqlx::Error>;
}

macro_rules! scalar_via {
    ($cast:literal, $wire:ty => $($t:ty),*) => { $(
        impl FromPgScalar for $t {
            const CAST: &'static str = $cast;
            fn from_row(row: &PgRow, col: &str) -> Result<Self, sqlx::Error> {
                let v: $wire = row.try_get(col)?;
                <$t>::try_from(v).map_err(|e| sqlx::Error::Decode(Box::new(e)))
            }
        }
    )* };
}
scalar_via!("::bigint", i64 => i8, i16, i32, i64, u8, u16, u32, u64, usize);

macro_rules! scalar_direct {
    ($cast:literal => $($t:ty),*) => { $(
        impl FromPgScalar for $t {
            const CAST: &'static str = $cast;
            fn from_row(row: &PgRow, col: &str) -> Result<Self, sqlx::Error> {
                row.try_get(col)
            }
        }
    )* };
}
scalar_direct!("::float8" => f64);
scalar_direct!("::text" => String);
scalar_direct!("::boolean" => bool);
impl FromPgScalar for f32 {
    const CAST: &'static str = "::float8";
    fn from_row(row: &PgRow, col: &str) -> Result<Self, sqlx::Error> {
        let v: f64 = row.try_get(col)?;
        Ok(v as f32)
    }
}

/// Kolom komponen `T` yang tipenya di-*erase* — argumen agregat, supaya
/// `sum::<i64>(T::col())` cukup satu turbofish. Diimplementasi oleh
/// [`Field<T, V>`] untuk `V` apa pun.
pub trait ColumnOf<T> {
    /// Nama kolom.
    fn column(&self) -> &'static str;
}
impl<T, V> ColumnOf<T> for Field<T, V> {
    fn column(&self) -> &'static str {
        self.column
    }
}

/// Fungsi agregat yang didukung.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Agg {
    Sum,
    Min,
    Max,
    Avg,
}

impl Agg {
    fn sql(self) -> &'static str {
        match self {
            Agg::Sum => "SUM",
            Agg::Min => "MIN",
            Agg::Max => "MAX",
            Agg::Avg => "AVG",
        }
    }
}

/// `SELECT F(col)cast AS v FROM table [WHERE …]` (placeholder `?`, belum dinomori).
pub(crate) fn agg_sql(
    table: &str,
    where_sql: Option<&str>,
    agg: Agg,
    col: &str,
    cast: &str,
) -> String {
    let w = where_sql.map(|w| format!(" WHERE {w}")).unwrap_or_default();
    format!(
        "SELECT {}({}){cast} AS v FROM {}{w}",
        agg.sql(),
        quote_ident(col),
        quote_ident(table)
    )
}

/// `SELECT k0cast AS k0, k1cast AS k1, <expr> AS v FROM table [WHERE …]
/// GROUP BY k0, k1 [HAVING …] ORDER BY k0, k1`. `expr` = `COUNT(*)::bigint`
/// atau `F(col)cast`. Placeholder `?` belum dinomori; param `having` mengikuti
/// param `where` (urutan tekstual).
pub(crate) fn group_sql(
    table: &str,
    where_sql: Option<&str>,
    keys: &[(&str, &str)],
    expr: &str,
    having_sql: Option<&str>,
) -> String {
    let w = where_sql.map(|w| format!(" WHERE {w}")).unwrap_or_default();
    let h = having_sql
        .map(|h| format!(" HAVING {h}"))
        .unwrap_or_default();
    let selects: Vec<String> = keys
        .iter()
        .enumerate()
        .map(|(i, (col, cast))| format!("{}{cast} AS k{i}", quote_ident(col)))
        .collect();
    let groups: Vec<String> = keys
        .iter()
        .map(|(col, _)| quote_ident(col).into_owned())
        .collect();
    let groups = groups.join(", ");
    format!(
        "SELECT {}, {expr} AS v FROM {}{w} GROUP BY {groups}{h} ORDER BY {groups}",
        selects.join(", "),
        quote_ident(table)
    )
}

/// Kunci `GROUP BY`: satu [`Field`] atau tuple 2–4 `Field` komponen `T`.
/// `Out` = tipe nilai kunci yang dibaca per baris (`K` atau tuple `K`).
pub trait GroupKey<T> {
    /// Nilai kunci per baris hasil.
    type Out;
    /// `(kolom, cast)` tiap kunci, urut.
    fn columns(&self) -> Vec<(&'static str, &'static str)>;
    /// Baca kunci dari kolom `k<base>`, `k<base+1>`, ….
    fn read(row: &PgRow, base: usize) -> Result<Self::Out, sqlx::Error>;
}

impl<T, K: FromPgScalar> GroupKey<T> for Field<T, K> {
    type Out = K;
    fn columns(&self) -> Vec<(&'static str, &'static str)> {
        vec![(self.column, K::CAST)]
    }
    fn read(row: &PgRow, base: usize) -> Result<K, sqlx::Error> {
        K::from_row(row, &format!("k{base}"))
    }
}

macro_rules! group_key_tuple {
    ($($f:ident : $k:ident => $i:tt),+) => {
        impl<T, $($k: FromPgScalar),+> GroupKey<T> for ($(Field<T, $k>,)+) {
            type Out = ($($k,)+);
            fn columns(&self) -> Vec<(&'static str, &'static str)> {
                vec![$((self.$i.column, $k::CAST)),+]
            }
            fn read(row: &PgRow, base: usize) -> Result<Self::Out, sqlx::Error> {
                Ok(($($k::from_row(row, &format!("k{}", base + $i))?,)+))
            }
        }
    };
}
group_key_tuple!(a: K0 => 0, b: K1 => 1);
group_key_tuple!(a: K0 => 0, b: K1 => 1, c: K2 => 2);
group_key_tuple!(a: K0 => 0, b: K1 => 1, c: K2 => 2, d: K3 => 3);

/// Predikat `HAVING` typed atas agregat komponen `T` (lihat [`count`],
/// [`sum`], …); gabung dengan [`Having::and`]/[`Having::or`]/[`Having::not`].
pub struct Having<T> {
    pub(crate) sql: String,
    pub(crate) params: Vec<(PgType, PgValue)>,
    _pd: PhantomData<fn() -> T>,
}

impl<T> Having<T> {
    fn raw(sql: String, params: Vec<(PgType, PgValue)>) -> Self {
        Self {
            sql,
            params,
            _pd: PhantomData,
        }
    }
    /// `(self) AND (other)`.
    pub fn and(mut self, other: Having<T>) -> Having<T> {
        self.sql = format!("({}) AND ({})", self.sql, other.sql);
        self.params.extend(other.params);
        self
    }
    /// `(self) OR (other)`.
    pub fn or(mut self, other: Having<T>) -> Having<T> {
        self.sql = format!("({}) OR ({})", self.sql, other.sql);
        self.params.extend(other.params);
        self
    }
    /// `NOT (self)`.
    // Metode fluent sengaja (rantai `.and().or().not()`), bukan trait `Not`.
    #[allow(clippy::should_implement_trait)]
    pub fn not(mut self) -> Having<T> {
        self.sql = format!("NOT ({})", self.sql);
        self
    }
}

/// Ekspresi agregat untuk `HAVING` (`COUNT(*)`, `SUM(col)`, …); bandingkan
/// dengan nilai lewat `gt`/`gte`/`lt`/`lte`/`eq`/`ne` → [`Having`].
pub struct AggExpr<T> {
    sql: String,
    _pd: PhantomData<fn() -> T>,
}

impl<T> AggExpr<T> {
    fn cmp(self, op: &str, v: impl IntoPgValue) -> Having<T> {
        let v = v.into_pg_value();
        let ty = bind_type(&v);
        Having::raw(
            format!("{} {op} ?{}", self.sql, cast_for(ty)),
            vec![(ty, v)],
        )
    }
    /// `expr > v`.
    pub fn gt(self, v: impl IntoPgValue) -> Having<T> {
        self.cmp(">", v)
    }
    /// `expr >= v`.
    pub fn gte(self, v: impl IntoPgValue) -> Having<T> {
        self.cmp(">=", v)
    }
    /// `expr < v`.
    pub fn lt(self, v: impl IntoPgValue) -> Having<T> {
        self.cmp("<", v)
    }
    /// `expr <= v`.
    pub fn lte(self, v: impl IntoPgValue) -> Having<T> {
        self.cmp("<=", v)
    }
    /// `expr = v`.
    pub fn eq(self, v: impl IntoPgValue) -> Having<T> {
        self.cmp("=", v)
    }
    /// `expr <> v`.
    pub fn ne(self, v: impl IntoPgValue) -> Having<T> {
        self.cmp("<>", v)
    }
}

/// Tipe bind untuk nilai literal `HAVING` (tak ada kolom acuan).
fn bind_type(v: &PgValue) -> PgType {
    match v {
        PgValue::Int(_) | PgValue::Ref(_) => PgType::BigInt,
        PgValue::Float(_) => PgType::DoublePrecision,
        PgValue::Numeric(_) => PgType::Numeric,
        PgValue::Bool(_) => PgType::Boolean,
        PgValue::Text(_) | PgValue::Null => PgType::Text,
        PgValue::Json(_) => PgType::Jsonb,
    }
}

fn cast_for(ty: PgType) -> &'static str {
    ty.bind_cast()
}

/// `COUNT(*)` untuk `HAVING`.
pub fn count<T>() -> AggExpr<T> {
    AggExpr {
        sql: "COUNT(*)".to_string(),
        _pd: PhantomData,
    }
}
/// `COUNT(DISTINCT field)` untuk `HAVING`.
pub fn count_distinct<T>(field: impl ColumnOf<T>) -> AggExpr<T> {
    AggExpr {
        sql: format!("COUNT(DISTINCT {})", quote_ident(field.column())),
        _pd: PhantomData,
    }
}
macro_rules! having_fn {
    ($($name:ident => $sql:literal),*) => { $(
        #[doc = concat!("`", $sql, "(field)` untuk `HAVING`.")]
        pub fn $name<T>(field: impl ColumnOf<T>) -> AggExpr<T> {
            AggExpr {
                sql: format!("{}({})", $sql, quote_ident(field.column())),
                _pd: PhantomData,
            }
        }
    )* };
}
having_fn!(sum => "SUM", min => "MIN", max => "MAX", avg => "AVG");

impl<T: PgComponent> Query<'_, T> {
    async fn agg_one<A: FromPgScalar>(
        self,
        agg: Agg,
        field: impl ColumnOf<T>,
    ) -> Result<Option<A>, sqlx::Error> {
        let (where_opt, params) = self.where_clause();
        let sql = renumber(&agg_sql(
            T::TABLE,
            where_opt.as_deref(),
            agg,
            field.column(),
            A::CAST,
        ));
        let rows = fetch_scalar_rows(self.store.pool(), &sql, &params).await?;
        let row = rows.first().ok_or(sqlx::Error::RowNotFound)?;
        // Agregat atas nol baris → NULL.
        let is_null = row.try_get_raw("v")?.is_null();
        if is_null {
            return Ok(None);
        }
        A::from_row(row, "v").map(Some)
    }

    /// `SUM(field)` atas entity yang cocok, dibaca sebagai `A`. `None` bila tak
    /// ada baris.
    pub async fn sum<A: FromPgScalar>(
        self,
        field: impl ColumnOf<T>,
    ) -> Result<Option<A>, sqlx::Error> {
        self.agg_one(Agg::Sum, field).await
    }
    /// `MIN(field)`; `None` bila tak ada baris.
    pub async fn min<A: FromPgScalar>(
        self,
        field: impl ColumnOf<T>,
    ) -> Result<Option<A>, sqlx::Error> {
        self.agg_one(Agg::Min, field).await
    }
    /// `MAX(field)`; `None` bila tak ada baris.
    pub async fn max<A: FromPgScalar>(
        self,
        field: impl ColumnOf<T>,
    ) -> Result<Option<A>, sqlx::Error> {
        self.agg_one(Agg::Max, field).await
    }
    /// `AVG(field)`; biasanya `A = f64`. `None` bila tak ada baris.
    pub async fn avg<A: FromPgScalar>(
        self,
        field: impl ColumnOf<T>,
    ) -> Result<Option<A>, sqlx::Error> {
        self.agg_one(Agg::Avg, field).await
    }

    /// `COUNT(DISTINCT field)` atas entity yang cocok (`NULL` tak dihitung).
    pub async fn count_distinct(self, field: impl ColumnOf<T>) -> Result<u64, sqlx::Error> {
        let (where_opt, params) = self.where_clause();
        let w = where_opt.map(|w| format!(" WHERE {w}")).unwrap_or_default();
        let sql = renumber(&format!(
            "SELECT COUNT(DISTINCT {})::bigint AS v FROM {}{w}",
            quote_ident(field.column()),
            quote_ident(T::TABLE)
        ));
        let rows = fetch_scalar_rows(self.store.pool(), &sql, &params).await?;
        let row = rows.first().ok_or(sqlx::Error::RowNotFound)?;
        let n: i64 = row.try_get("v")?;
        Ok(u64::try_from(n).unwrap_or(0))
    }
}

/// [`Query`] yang sudah dikelompokkan atas kunci `G` ([`GroupKey`]: satu
/// `Field` atau tuple `Field`, dari [`Query::group_by`]); terminalnya
/// mengembalikan `Vec<(G::Out, …)>` urut kunci. [`Self::having`] menyaring grup.
pub struct Grouped<'a, T: PgComponent, G: GroupKey<T>> {
    pub(crate) query: Query<'a, T>,
    pub(crate) key: G,
    pub(crate) having: Option<Having<T>>,
}

impl<T: PgComponent, G: GroupKey<T>> Grouped<'_, T, G> {
    /// Saring grup dengan predikat agregat (`HAVING`); dipanggil >1× →
    /// digabung `AND`. Bangun lewat [`count`]/[`count_distinct`]/[`sum`]/
    /// [`min`]/[`max`]/[`avg`] + `gt`/`gte`/…, gabung `and`/`or`/`not`.
    pub fn having(mut self, h: Having<T>) -> Self {
        self.having = Some(match self.having.take() {
            Some(existing) => existing.and(h),
            None => h,
        });
        self
    }

    async fn run<A: FromPgScalar>(
        self,
        expr: String,
    ) -> Result<Vec<(G::Out, Option<A>)>, sqlx::Error> {
        let (where_opt, mut params) = self.query.where_clause();
        let having_sql = self.having.as_ref().map(|h| h.sql.as_str());
        if let Some(h) = &self.having {
            params.extend(h.params.iter().cloned());
        }
        let sql = renumber(&group_sql(
            T::TABLE,
            where_opt.as_deref(),
            &self.key.columns(),
            &expr,
            having_sql,
        ));
        let rows = fetch_scalar_rows(self.query.store.pool(), &sql, &params).await?;
        rows.iter()
            .map(|row| {
                let k = G::read(row, 0)?;
                let v = if row.try_get_raw("v")?.is_null() {
                    None
                } else {
                    Some(A::from_row(row, "v")?)
                };
                Ok((k, v))
            })
            .collect()
    }

    /// `COUNT(*)` per kunci.
    pub async fn count(self) -> Result<Vec<(G::Out, u64)>, sqlx::Error> {
        let rows: Vec<(G::Out, Option<i64>)> = self.run("COUNT(*)::bigint".to_string()).await?;
        Ok(rows
            .into_iter()
            .map(|(k, v)| (k, u64::try_from(v.unwrap_or(0)).unwrap_or(0)))
            .collect())
    }
    /// `COUNT(DISTINCT field)` per kunci.
    pub async fn count_distinct(
        self,
        field: impl ColumnOf<T>,
    ) -> Result<Vec<(G::Out, u64)>, sqlx::Error> {
        let expr = format!("COUNT(DISTINCT {})::bigint", quote_ident(field.column()));
        let rows: Vec<(G::Out, Option<i64>)> = self.run(expr).await?;
        Ok(rows
            .into_iter()
            .map(|(k, v)| (k, u64::try_from(v.unwrap_or(0)).unwrap_or(0)))
            .collect())
    }
    /// `SUM(field)` per kunci.
    pub async fn sum<A: FromPgScalar>(
        self,
        field: impl ColumnOf<T>,
    ) -> Result<Vec<(G::Out, Option<A>)>, sqlx::Error> {
        self.run(format!("SUM({}){}", quote_ident(field.column()), A::CAST))
            .await
    }
    /// `MIN(field)` per kunci.
    pub async fn min<A: FromPgScalar>(
        self,
        field: impl ColumnOf<T>,
    ) -> Result<Vec<(G::Out, Option<A>)>, sqlx::Error> {
        self.run(format!("MIN({}){}", quote_ident(field.column()), A::CAST))
            .await
    }
    /// `MAX(field)` per kunci.
    pub async fn max<A: FromPgScalar>(
        self,
        field: impl ColumnOf<T>,
    ) -> Result<Vec<(G::Out, Option<A>)>, sqlx::Error> {
        self.run(format!("MAX({}){}", quote_ident(field.column()), A::CAST))
            .await
    }
    /// `AVG(field)` per kunci; biasanya `A = f64`.
    pub async fn avg<A: FromPgScalar>(
        self,
        field: impl ColumnOf<T>,
    ) -> Result<Vec<(G::Out, Option<A>)>, sqlx::Error> {
        self.run(format!("AVG({}){}", quote_ident(field.column()), A::CAST))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agg_dan_group_sql() {
        assert_eq!(
            renumber(&agg_sql(
                "cmp_b",
                Some("room = ?"),
                Agg::Sum,
                "minutes",
                "::bigint"
            )),
            "SELECT SUM(minutes)::bigint AS v FROM cmp_b WHERE room = $1"
        );
        assert_eq!(
            agg_sql("cmp_b", None, Agg::Avg, "minutes", "::float8"),
            "SELECT AVG(minutes)::float8 AS v FROM cmp_b"
        );
        assert_eq!(
            renumber(&group_sql(
                "cmp_b",
                Some("x > ?"),
                &[("room", "::bigint")],
                "COUNT(*)::bigint",
                None
            )),
            "SELECT room::bigint AS k0, COUNT(*)::bigint AS v FROM cmp_b WHERE x > $1 GROUP BY room ORDER BY room"
        );
        // Multi-kunci + HAVING: param HAVING dinomori setelah WHERE.
        assert_eq!(
            renumber(&group_sql(
                "cmp_b",
                Some("x > ?"),
                &[("room", "::bigint"), ("kind", "::text")],
                "SUM(minutes)::bigint",
                Some("SUM(minutes) >= ?")
            )),
            "SELECT room::bigint AS k0, kind::text AS k1, SUM(minutes)::bigint AS v FROM cmp_b \
             WHERE x > $1 GROUP BY room, kind HAVING SUM(minutes) >= $2 ORDER BY room, kind"
        );
    }

    #[test]
    fn having_builder_sql() {
        struct B;
        let h = count::<B>()
            .gt(1)
            .and(sum(Field::<B, i32>::new("minutes", PgType::Integer)).gte(200))
            .or(avg(Field::<B, i32>::new("minutes", PgType::Integer))
                .lt(40.0)
                .not());
        assert_eq!(
            h.sql,
            "((COUNT(*) > ?) AND (SUM(minutes) >= ?)) OR (NOT (AVG(minutes) < ?))"
        );
        assert_eq!(
            h.params,
            vec![
                (PgType::BigInt, PgValue::Int(1)),
                (PgType::BigInt, PgValue::Int(200)),
                (PgType::DoublePrecision, PgValue::Float(40.0)),
            ]
        );
        let d = count_distinct(Field::<B, String>::new("kind", PgType::Text)).eq(2);
        assert_eq!(d.sql, "COUNT(DISTINCT kind) = ?");
    }

    #[test]
    fn cast_per_tipe() {
        assert_eq!(<i32 as FromPgScalar>::CAST, "::bigint");
        assert_eq!(<f64 as FromPgScalar>::CAST, "::float8");
        assert_eq!(<String as FromPgScalar>::CAST, "::text");
    }
}
