//! Agregasi typed di atas [`Query`] (`SUM`/`MIN`/`MAX`/`AVG`/`COUNT` +
//! `GROUP BY` satu kunci) — hasilnya skalar, bukan entity.
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
//! Agregat atas nol baris → `None`. `HAVING`, multi-kunci, dan window function
//! tidak ada — pakai `load_where`/SQL.

use sqlx::postgres::PgRow;
use sqlx::{Row, ValueRef};

use crate::query::{Field, Query, fetch_scalar_rows, renumber};
use crate::{PgComponent, quote_ident};

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

/// `SELECT key AS k, <expr> AS v FROM table [WHERE …] GROUP BY key ORDER BY key`.
/// `expr` = `COUNT(*)::bigint` atau `F(col)cast`.
pub(crate) fn group_sql(
    table: &str,
    where_sql: Option<&str>,
    key: &str,
    key_cast: &str,
    expr: &str,
) -> String {
    let w = where_sql.map(|w| format!(" WHERE {w}")).unwrap_or_default();
    let key = quote_ident(key);
    let table = quote_ident(table);
    format!(
        "SELECT {key}{key_cast} AS k, {expr} AS v FROM {table}{w} GROUP BY {key} ORDER BY {key}"
    )
}

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
}

/// [`Query`] yang sudah dikelompokkan atas satu kunci `K` (dari
/// [`Query::group_by`]); terminalnya mengembalikan `Vec<(K, …)>` urut kunci.
pub struct Grouped<'a, T: PgComponent, K> {
    pub(crate) query: Query<'a, T>,
    pub(crate) key: Field<T, K>,
}

impl<T: PgComponent, K: FromPgScalar> Grouped<'_, T, K> {
    async fn run<A: FromPgScalar>(self, expr: String) -> Result<Vec<(K, Option<A>)>, sqlx::Error> {
        let (where_opt, params) = self.query.where_clause();
        let sql = renumber(&group_sql(
            T::TABLE,
            where_opt.as_deref(),
            self.key.column,
            K::CAST,
            &expr,
        ));
        let rows = fetch_scalar_rows(self.query.store.pool(), &sql, &params).await?;
        rows.iter()
            .map(|row| {
                let k = K::from_row(row, "k")?;
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
    pub async fn count(self) -> Result<Vec<(K, u64)>, sqlx::Error> {
        let rows: Vec<(K, Option<i64>)> = self.run("COUNT(*)::bigint".to_string()).await?;
        Ok(rows
            .into_iter()
            .map(|(k, v)| (k, u64::try_from(v.unwrap_or(0)).unwrap_or(0)))
            .collect())
    }
    /// `SUM(field)` per kunci.
    pub async fn sum<A: FromPgScalar>(
        self,
        field: impl ColumnOf<T>,
    ) -> Result<Vec<(K, Option<A>)>, sqlx::Error> {
        self.run(format!("SUM({}){}", quote_ident(field.column()), A::CAST))
            .await
    }
    /// `MIN(field)` per kunci.
    pub async fn min<A: FromPgScalar>(
        self,
        field: impl ColumnOf<T>,
    ) -> Result<Vec<(K, Option<A>)>, sqlx::Error> {
        self.run(format!("MIN({}){}", quote_ident(field.column()), A::CAST))
            .await
    }
    /// `MAX(field)` per kunci.
    pub async fn max<A: FromPgScalar>(
        self,
        field: impl ColumnOf<T>,
    ) -> Result<Vec<(K, Option<A>)>, sqlx::Error> {
        self.run(format!("MAX({}){}", quote_ident(field.column()), A::CAST))
            .await
    }
    /// `AVG(field)` per kunci; biasanya `A = f64`.
    pub async fn avg<A: FromPgScalar>(
        self,
        field: impl ColumnOf<T>,
    ) -> Result<Vec<(K, Option<A>)>, sqlx::Error> {
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
                "room",
                "::bigint",
                "COUNT(*)::bigint"
            )),
            "SELECT room::bigint AS k, COUNT(*)::bigint AS v FROM cmp_b WHERE x > $1 GROUP BY room ORDER BY room"
        );
    }

    #[test]
    fn cast_per_tipe() {
        assert_eq!(<i32 as FromPgScalar>::CAST, "::bigint");
        assert_eq!(<f64 as FromPgScalar>::CAST, "::float8");
        assert_eq!(<String as FromPgScalar>::CAST, "::text");
    }
}
