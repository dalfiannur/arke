//! Query builder typed & fluent untuk baca ber-filter (RFC-0030).
//!
//! Terinspirasi *fluent query builder* Laravel Eloquent — **bukan** Active
//! Record-nya. Token field (`Health::hp()`) di-generate `#[derive(PgComponent)]`
//! → operator dicek compiler; SQL **ter-parameterisasi** (anti-injeksi).
//!
//! ```ignore
//! store.query::<Health>()
//!     .filter(Health::hp().lt(20).and(Health::hp().gte(5)))
//!     .order_by(Health::hp(), Dir::Desc)
//!     .limit(100).offset(200)
//!     .load(&mut world).await?;
//! ```
//!
//! `load_where::<T>(w, "sql")` string tetap ada sebagai escape-hatch.
//!
//! # Type-safety (dicek compiler)
//!
//! Nilai operator harus cocok tipe field:
//!
//! ```compile_fail
//! #[derive(arke_postgres::PgComponent)]
//! struct Health { hp: i32 }
//! // `hp` adalah i32 → membandingkan dengan teks tak dapat dikompilasi.
//! let _ = Health::hp().lt("teks");
//! ```
//!
//! `like` hanya untuk field teks:
//!
//! ```compile_fail
//! #[derive(arke_postgres::PgComponent)]
//! struct Health { hp: i32 }
//! // `like` hanya ada pada Field<C, String>; `hp` adalah i32.
//! let _ = Health::hp().like("a%");
//! ```

use std::marker::PhantomData;

use arke::World;

use crate::{PgComponent, PgStore, PgType, PgValue};

/// Konversi nilai skalar → [`PgValue`] untuk *bind* ter-parameterisasi.
pub trait IntoPgValue {
    /// Ubah `self` menjadi [`PgValue`] yang di-bind ke query.
    fn into_pg_value(self) -> PgValue;
}

macro_rules! int_val {
    ($($t:ty),*) => { $(impl IntoPgValue for $t {
        fn into_pg_value(self) -> PgValue { PgValue::Int(self as i64) }
    })* };
}
int_val!(i8, i16, i32, u8, u16, i64, isize, u32);

macro_rules! numeric_val {
    ($($t:ty),*) => { $(impl IntoPgValue for $t {
        fn into_pg_value(self) -> PgValue { PgValue::Numeric(self.to_string()) }
    })* };
}
numeric_val!(u64, usize);

impl IntoPgValue for f32 {
    fn into_pg_value(self) -> PgValue {
        PgValue::Float(f64::from(self))
    }
}
impl IntoPgValue for f64 {
    fn into_pg_value(self) -> PgValue {
        PgValue::Float(self)
    }
}
impl IntoPgValue for bool {
    fn into_pg_value(self) -> PgValue {
        PgValue::Bool(self)
    }
}
impl IntoPgValue for String {
    fn into_pg_value(self) -> PgValue {
        PgValue::Text(self)
    }
}
impl IntoPgValue for &str {
    fn into_pg_value(self) -> PgValue {
        PgValue::Text(self.to_string())
    }
}

/// Token field typed untuk komponen `C`, bertipe nilai `V` (di-generate derive).
pub struct Field<C, V> {
    column: &'static str,
    ty: PgType,
    _pd: PhantomData<fn() -> (C, V)>,
}

impl<C, V> Field<C, V> {
    /// Dibuat oleh `#[derive(PgComponent)]`; jarang dipanggil manual.
    pub fn new(column: &'static str, ty: PgType) -> Self {
        Self {
            column,
            ty,
            _pd: PhantomData,
        }
    }

    /// Cast placeholder yang dibutuhkan tipe kolom (`NUMERIC`/`JSONB` di-bind teks).
    fn cast(&self) -> &'static str {
        match self.ty {
            PgType::Numeric => "::numeric",
            PgType::Jsonb => "::jsonb",
            _ => "",
        }
    }
}

/// Predikat `WHERE` typed atas komponen `C`. Bangun dari operator [`Field`],
/// gabung dengan [`Filter::and`]/[`Filter::or`]/[`Filter::not`].
pub struct Filter<C> {
    /// Fragmen SQL dengan placeholder `?` (dinomori ulang jadi `$n` saat rakit).
    sql: String,
    /// Nilai ter-bind, urut sesuai kemunculan `?`.
    params: Vec<(PgType, PgValue)>,
    _pd: PhantomData<fn() -> C>,
}

impl<C> Filter<C> {
    fn raw(sql: String, params: Vec<(PgType, PgValue)>) -> Self {
        Self {
            sql,
            params,
            _pd: PhantomData,
        }
    }

    /// `(self) AND (other)`.
    pub fn and(self, other: Filter<C>) -> Filter<C> {
        Self::raw(
            format!("({}) AND ({})", self.sql, other.sql),
            [self.params, other.params].concat(),
        )
    }

    /// `(self) OR (other)`.
    pub fn or(self, other: Filter<C>) -> Filter<C> {
        Self::raw(
            format!("({}) OR ({})", self.sql, other.sql),
            [self.params, other.params].concat(),
        )
    }

    /// `NOT (self)`.
    // Metode fluent sengaja (rantai `.and().or().not()`), bukan trait `Not`.
    #[allow(clippy::should_implement_trait)]
    pub fn not(self) -> Filter<C> {
        Self::raw(format!("NOT ({})", self.sql), self.params)
    }
}

impl<C: PgComponent, V: IntoPgValue> Field<C, V> {
    fn binop(self, op: &str, v: V) -> Filter<C> {
        Filter::raw(
            format!("{} {} ?{}", self.column, op, self.cast()),
            vec![(self.ty, v.into_pg_value())],
        )
    }

    /// `col = v`.
    pub fn eq(self, v: V) -> Filter<C> {
        self.binop("=", v)
    }
    /// `col <> v`.
    pub fn ne(self, v: V) -> Filter<C> {
        self.binop("<>", v)
    }
    /// `col < v`.
    pub fn lt(self, v: V) -> Filter<C> {
        self.binop("<", v)
    }
    /// `col <= v`.
    pub fn lte(self, v: V) -> Filter<C> {
        self.binop("<=", v)
    }
    /// `col > v`.
    pub fn gt(self, v: V) -> Filter<C> {
        self.binop(">", v)
    }
    /// `col >= v`.
    pub fn gte(self, v: V) -> Filter<C> {
        self.binop(">=", v)
    }

    /// `col BETWEEN lo AND hi`.
    pub fn between(self, lo: V, hi: V) -> Filter<C> {
        let cast = self.cast();
        Filter::raw(
            format!("{} BETWEEN ?{cast} AND ?{cast}", self.column),
            vec![(self.ty, lo.into_pg_value()), (self.ty, hi.into_pg_value())],
        )
    }

    /// `col IS NULL` (untuk field `Option<T>`).
    pub fn is_null(self) -> Filter<C> {
        Filter::raw(format!("{} IS NULL", self.column), vec![])
    }

    /// `col IN (a, b, …)`. Iterator kosong → `1 = 0` (tak cocok apa pun).
    pub fn in_<I: IntoIterator<Item = V>>(self, vals: I) -> Filter<C> {
        let cast = self.cast();
        let params: Vec<(PgType, PgValue)> = vals
            .into_iter()
            .map(|v| (self.ty, v.into_pg_value()))
            .collect();
        if params.is_empty() {
            return Filter::raw("1 = 0".to_string(), vec![]);
        }
        let placeholders = params
            .iter()
            .map(|_| format!("?{cast}"))
            .collect::<Vec<_>>()
            .join(", ");
        Filter::raw(format!("{} IN ({placeholders})", self.column), params)
    }
}

/// `LIKE` hanya untuk field teks (type-safety): `Health::hp().like(..)` (integer)
/// tak dapat dikompilasi.
impl<C: PgComponent> Field<C, String> {
    /// `col LIKE pattern` (mis. `"a%"`).
    pub fn like(self, pattern: impl Into<String>) -> Filter<C> {
        Filter::raw(
            format!("{} LIKE ?", self.column),
            vec![(PgType::Text, PgValue::Text(pattern.into()))],
        )
    }
}

/// Arah pengurutan `ORDER BY`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    /// Menaik (`ASC`).
    Asc,
    /// Menurun (`DESC`).
    Desc,
}

/// Builder query baca ber-filter (RFC-0030). Dibuat oleh [`PgStore::query`].
pub struct Query<'a, T: PgComponent> {
    store: &'a mut PgStore,
    filter: Option<Filter<T>>,
    order: Vec<(&'static str, Dir)>,
    limit: Option<i64>,
    offset: Option<i64>,
}

impl<'a, T: PgComponent> Query<'a, T> {
    pub(crate) fn new(store: &'a mut PgStore) -> Self {
        Self {
            store,
            filter: None,
            order: Vec::new(),
            limit: None,
            offset: None,
        }
    }

    /// Tambah predikat `WHERE`. Dipanggil >1× → digabung dengan `AND`.
    pub fn filter(mut self, f: Filter<T>) -> Self {
        self.filter = Some(match self.filter.take() {
            Some(existing) => existing.and(f),
            None => f,
        });
        self
    }

    /// Tambah kunci `ORDER BY` (bisa berkali-kali untuk multi-kunci).
    pub fn order_by<V>(mut self, field: Field<T, V>, dir: Dir) -> Self {
        self.order.push((field.column, dir));
        self
    }

    /// Batasi jumlah baris (`LIMIT`).
    pub fn limit(mut self, n: u64) -> Self {
        self.limit = Some(n as i64);
        self
    }

    /// Lewati `n` baris pertama (`OFFSET`).
    pub fn offset(mut self, n: u64) -> Self {
        self.offset = Some(n as i64);
        self
    }

    /// Susun SQL ter-parameterisasi + urutan nilai bind. Terpisah dari [`load`]
    /// agar dapat diuji tanpa DB.
    ///
    /// [`load`]: Self::load
    fn build(&self) -> (String, Vec<(PgType, PgValue)>) {
        let mut params: Vec<(PgType, PgValue)> = Vec::new();
        let mut sql = format!("SELECT entity_id FROM {}", T::TABLE);

        if let Some(f) = &self.filter {
            sql.push_str(" WHERE ");
            sql.push_str(&f.sql);
            params.extend(f.params.iter().cloned());
        }

        if self.order.is_empty() {
            // Urutan deterministik default (STD-0005 mirror pada sisi Postgres).
            sql.push_str(" ORDER BY entity_id");
        } else {
            sql.push_str(" ORDER BY ");
            let parts: Vec<String> = self
                .order
                .iter()
                .map(|(c, d)| {
                    let dir = match d {
                        Dir::Asc => "ASC",
                        Dir::Desc => "DESC",
                    };
                    format!("{c} {dir}")
                })
                .collect();
            sql.push_str(&parts.join(", "));
        }

        if let Some(l) = self.limit {
            sql.push_str(" LIMIT ?");
            params.push((PgType::BigInt, PgValue::Int(l)));
        }
        if let Some(o) = self.offset {
            sql.push_str(" OFFSET ?");
            params.push((PgType::BigInt, PgValue::Int(o)));
        }

        (renumber(&sql), params)
    }

    /// Jalankan query, materialisasi entity yang cocok (+ seluruh komponennya) ke
    /// `world`. Mengembalikan jumlah entity dimuat.
    pub async fn load(self, world: &mut World) -> Result<usize, sqlx::Error> {
        let (sql, params) = self.build();
        self.store.load_by_query(sql, params, world).await
    }
}

/// Ubah placeholder `?` berurutan menjadi `$1..$n` (dialek Postgres).
fn renumber(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len() + 8);
    let mut n = 1u32;
    for ch in sql.chars() {
        if ch == '?' {
            out.push('$');
            out.push_str(&n.to_string());
            n += 1;
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ColumnDef;

    // Komponen uji manual (tanpa derive) — cukup untuk menguji generasi SQL.
    struct Health {
        _hp: i32,
    }
    impl PgComponent for Health {
        const TABLE: &'static str = "cmp_health";
        const COLUMNS: &'static [ColumnDef] = &[ColumnDef {
            name: "hp",
            ty: PgType::Integer,
            nullable: false,
        }];
        fn to_params(&self) -> Vec<PgValue> {
            vec![PgValue::Int(i64::from(self._hp))]
        }
        fn from_params(v: &[PgValue]) -> Option<Self> {
            match v {
                [PgValue::Int(i)] => Some(Health { _hp: *i as i32 }),
                _ => None,
            }
        }
    }
    impl Health {
        fn hp() -> Field<Self, i32> {
            Field::new("hp", PgType::Integer)
        }
        fn name() -> Field<Self, String> {
            Field::new("name", PgType::Text)
        }
    }

    // Susun query lengkap tanpa store (uji `build` via helper).
    fn built(
        filter: Option<Filter<Health>>,
        order: Vec<(&'static str, Dir)>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> (String, Vec<(PgType, PgValue)>) {
        // Tiru `Query::build` tanpa `&mut PgStore` (yang butuh koneksi).
        let mut params: Vec<(PgType, PgValue)> = Vec::new();
        let mut sql = format!("SELECT entity_id FROM {}", Health::TABLE);
        if let Some(f) = &filter {
            sql.push_str(" WHERE ");
            sql.push_str(&f.sql);
            params.extend(f.params.iter().cloned());
        }
        if order.is_empty() {
            sql.push_str(" ORDER BY entity_id");
        } else {
            let parts: Vec<String> = order
                .iter()
                .map(|(c, d)| {
                    format!(
                        "{c} {}",
                        match d {
                            Dir::Asc => "ASC",
                            Dir::Desc => "DESC",
                        }
                    )
                })
                .collect();
            sql.push_str(&format!(" ORDER BY {}", parts.join(", ")));
        }
        if let Some(l) = limit {
            sql.push_str(" LIMIT ?");
            params.push((PgType::BigInt, PgValue::Int(l)));
        }
        if let Some(o) = offset {
            sql.push_str(" OFFSET ?");
            params.push((PgType::BigInt, PgValue::Int(o)));
        }
        (renumber(&sql), params)
    }

    #[test]
    fn predikat_sederhana_terparameterisasi() {
        let (sql, params) = built(Some(Health::hp().lt(20)), vec![], None, None);
        assert_eq!(
            sql,
            "SELECT entity_id FROM cmp_health WHERE hp < $1 ORDER BY entity_id"
        );
        assert_eq!(params, vec![(PgType::Integer, PgValue::Int(20))]);
    }

    #[test]
    fn and_or_not_dan_penomoran_placeholder() {
        let f = Health::hp()
            .gte(5)
            .and(Health::hp().lt(20))
            .or(Health::hp().eq(0).not());
        let (sql, params) = built(Some(f), vec![], None, None);
        assert_eq!(
            sql,
            "SELECT entity_id FROM cmp_health WHERE ((hp >= $1) AND (hp < $2)) OR (NOT (hp = $3)) ORDER BY entity_id"
        );
        assert_eq!(
            params,
            vec![
                (PgType::Integer, PgValue::Int(5)),
                (PgType::Integer, PgValue::Int(20)),
                (PgType::Integer, PgValue::Int(0)),
            ]
        );
    }

    #[test]
    fn order_limit_offset() {
        let (sql, params) = built(
            Some(Health::hp().lt(20)),
            vec![("hp", Dir::Desc)],
            Some(100),
            Some(200),
        );
        assert_eq!(
            sql,
            "SELECT entity_id FROM cmp_health WHERE hp < $1 ORDER BY hp DESC LIMIT $2 OFFSET $3"
        );
        assert_eq!(
            params,
            vec![
                (PgType::Integer, PgValue::Int(20)),
                (PgType::BigInt, PgValue::Int(100)),
                (PgType::BigInt, PgValue::Int(200)),
            ]
        );
    }

    #[test]
    fn in_kosong_jadi_selalu_salah() {
        let (sql, _) = built(
            Some(Health::hp().in_(Vec::<i32>::new())),
            vec![],
            None,
            None,
        );
        assert_eq!(
            sql,
            "SELECT entity_id FROM cmp_health WHERE 1 = 0 ORDER BY entity_id"
        );
    }

    #[test]
    fn in_dan_like() {
        let (sql, params) = built(
            Some(Health::hp().in_([1, 2, 3]).and(Health::name().like("a%"))),
            vec![],
            None,
            None,
        );
        assert_eq!(
            sql,
            "SELECT entity_id FROM cmp_health WHERE (hp IN ($1, $2, $3)) AND (name LIKE $4) ORDER BY entity_id"
        );
        assert_eq!(params.len(), 4);
        assert_eq!(params[3], (PgType::Text, PgValue::Text("a%".to_string())));
    }
}
