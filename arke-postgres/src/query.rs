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

/// Predikat relasi (RFC-0031/0032) pada token relasi `Field<C, EntityRef>`.
impl<C: PgComponent> Field<C, EntityRef> {
    /// Cocok bila entity yang ditunjuk kolom relasi ini memenuhi `f` atas komponen
    /// `R`: `<rel>_id IN (SELECT entity_id FROM cmp_R WHERE <f>)`.
    ///
    /// Menghasilkan `Filter<C>` → **bersarang** (argumen `f` boleh hasil `matches`
    /// lagi, relasi 3–4 deep) & digabung `and`/`or` (RFC-0032).
    pub fn matches<R: PgComponent>(self, f: Filter<R>) -> Filter<C> {
        Filter::raw(join_cond(self.column, R::TABLE, &f.sql), f.params)
    }
}

/// Predikat relasi **bertipe** (RFC-0032) pada token `Field<C, RelRef<Target>>`.
impl<C: PgComponent, Target: PgComponent> Field<C, RelRef<Target>> {
    /// Seperti [`Field::matches`] tetapi target `Target` sudah tersimpul → menerima
    /// `Filter<Target>` langsung (tanpa `::<R>`).
    pub fn matches(self, f: Filter<Target>) -> Filter<C> {
        Filter::raw(join_cond(self.column, Target::TABLE, &f.sql), f.params)
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

/// Marker tipe untuk token relasi (kolom FK `Entity`, RFC-0031). Tak punya
/// operator skalar — hanya dipakai sebagai argumen `relation` pada
/// [`Query::join`]/[`Query::join_load`].
pub struct EntityRef;

/// Marker token relasi **bertipe** (RFC-0032) untuk field `Ref<Target>`: token
/// jadi `Field<C, RelRef<Target>>`, target `Target` tersimpul → `matches`/`through`
/// tanpa anotasi tipe & hop salah-tipe gagal kompilasi.
pub struct RelRef<Target>(PhantomData<fn() -> Target>);

/// Satu klausa join antar-entity (RFC-0031): `<rel>_id IN (SELECT entity_id FROM
/// <tabel R> WHERE <filter>)`. Pendekatan sub-query menghindari ambiguitas alias.
struct JoinClause {
    rel_column: &'static str,
    related_table: &'static str,
    filter_sql: String,
    filter_params: Vec<(PgType, PgValue)>,
    /// `join_load` → muat juga entity target `R`.
    load: bool,
}

/// Builder query baca ber-filter (RFC-0030) + join antar-entity (RFC-0031).
/// Dibuat oleh [`PgStore::query`].
pub struct Query<'a, T: PgComponent> {
    store: &'a mut PgStore,
    filter: Option<Filter<T>>,
    joins: Vec<JoinClause>,
    order: Vec<(&'static str, Dir)>,
    limit: Option<i64>,
    offset: Option<i64>,
}

impl<'a, T: PgComponent> Query<'a, T> {
    pub(crate) fn new(store: &'a mut PgStore) -> Self {
        Self {
            store,
            filter: None,
            joins: Vec::new(),
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

    /// Join antar-entity (RFC-0031): saring `T` bila entity yang ditunjuk
    /// `relation` (kolom FK di `T`, mis. `Owner::of()`) memenuhi `filter` atas
    /// komponen `R`. Entity `R` **tidak** dimuat — pakai [`Self::join_load`] untuk itu.
    ///
    /// Gula untuk `filter(relation.matches(filter))` (RFC-0032); `filter` boleh
    /// bersarang (`matches`) untuk relasi 3–4 deep.
    pub fn join<R: PgComponent>(self, relation: Field<T, EntityRef>, filter: Filter<R>) -> Self {
        self.filter(relation.matches(filter))
    }

    /// Seperti [`Self::join`], **plus** memuat entity `R` yang menjadi target
    /// relasi dari `T` yang cocok (agar traversal handle langsung jalan).
    pub fn join_load<R: PgComponent>(
        self,
        relation: Field<T, EntityRef>,
        filter: Filter<R>,
    ) -> Self {
        self.push_join(relation, filter, true)
    }

    fn push_join<R: PgComponent>(
        mut self,
        relation: Field<T, EntityRef>,
        filter: Filter<R>,
        load: bool,
    ) -> Self {
        self.joins.push(JoinClause {
            rel_column: relation.column,
            related_table: R::TABLE,
            filter_sql: filter.sql,
            filter_params: filter.params,
            load,
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

    /// Klausa `WHERE` (filter dasar + sub-query join) dengan placeholder `?` +
    /// nilai bind-nya. `None` bila tak ada kondisi.
    fn where_clause(&self) -> (Option<String>, Vec<(PgType, PgValue)>) {
        let mut conds: Vec<String> = Vec::new();
        let mut params: Vec<(PgType, PgValue)> = Vec::new();
        if let Some(f) = &self.filter {
            conds.push(f.sql.clone());
            params.extend(f.params.iter().cloned());
        }
        for j in &self.joins {
            conds.push(join_cond(j.rel_column, j.related_table, &j.filter_sql));
            params.extend(j.filter_params.iter().cloned());
        }
        if conds.is_empty() {
            (None, params)
        } else {
            (Some(conds.join(" AND ")), params)
        }
    }

    /// Susun SQL utama ter-parameterisasi + nilai bind. Terpisah dari [`load`]
    /// agar dapat diuji tanpa DB.
    ///
    /// [`load`]: Self::load
    fn build(&self) -> (String, Vec<(PgType, PgValue)>) {
        let (where_opt, mut params) = self.where_clause();
        let mut sql = format!("SELECT entity_id FROM {}", T::TABLE);
        if let Some(w) = &where_opt {
            sql.push_str(" WHERE ");
            sql.push_str(w);
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

    /// SQL pemuat target `R` untuk `join_load`: id entity yang **ditunjuk** oleh
    /// `T` yang cocok (`SELECT DISTINCT <rel>_id ... WHERE <where>`).
    fn target_load_sql(&self, rel_column: &str) -> (String, Vec<(PgType, PgValue)>) {
        let (where_opt, params) = self.where_clause();
        let where_sql = where_opt.unwrap_or_else(|| "TRUE".to_string());
        let sql = format!(
            "SELECT DISTINCT {rel} AS entity_id FROM {tbl} WHERE ({where_sql}) AND {rel} IS NOT NULL",
            rel = rel_column,
            tbl = T::TABLE,
        );
        (renumber(&sql), params)
    }

    /// Jalankan query, materialisasi entity `T` yang cocok (+ seluruh komponennya)
    /// ke `world`; untuk tiap `join_load`, muat pula entity target `R`.
    /// Mengembalikan jumlah entity `T` dimuat.
    pub async fn load(self, world: &mut World) -> Result<usize, sqlx::Error> {
        // Susun semua SQL (pinjam-baca `self`) sebelum menyentuh `self.store`.
        let (main_sql, main_params) = self.build();
        let targets: Vec<(String, Vec<(PgType, PgValue)>)> = self
            .joins
            .iter()
            .filter(|j| j.load)
            .map(|j| self.target_load_sql(j.rel_column))
            .collect();

        let store = self.store;
        let n = store.load_by_query(main_sql, main_params, world).await?;
        for (sql, params) in targets {
            store.load_by_query(sql, params, world).await?;
        }
        Ok(n)
    }

    /// Mulai **path relasi bertipe** (RFC-0032): hop pertama `T →(rel)→ Next`.
    /// Lanjutkan dengan `.through()` lalu `.where_(...).load_all(...)`.
    pub fn through<Next: PgComponent>(self, rel: Field<T, RelRef<Next>>) -> PathQuery<'a, Next> {
        PathQuery {
            store: self.store,
            hops: vec![Hop {
                from_table: T::TABLE,
                rel_column: rel.column,
                to_table: Next::TABLE,
            }],
            _pd: PhantomData,
        }
    }
}

/// Satu hop path relasi (RFC-0032): dari `from_table` lewat `rel_column` ke `to_table`.
struct Hop {
    from_table: &'static str,
    rel_column: &'static str,
    to_table: &'static str,
}

/// Path relasi bertipe sedang dibangun; `Current` = tipe komponen di ujung path.
/// Lanjut `.through()` (hop lagi) atau akhiri `.where_(Filter<Current>)`.
pub struct PathQuery<'a, Current: PgComponent> {
    store: &'a mut PgStore,
    hops: Vec<Hop>,
    _pd: PhantomData<fn() -> Current>,
}

impl<'a, Current: PgComponent> PathQuery<'a, Current> {
    /// Tambah hop `Current →(rel)→ Next` (type-safe: hop salah-tipe gagal kompilasi).
    pub fn through<Next: PgComponent>(
        mut self,
        rel: Field<Current, RelRef<Next>>,
    ) -> PathQuery<'a, Next> {
        self.hops.push(Hop {
            from_table: Current::TABLE,
            rel_column: rel.column,
            to_table: Next::TABLE,
        });
        PathQuery {
            store: self.store,
            hops: self.hops,
            _pd: PhantomData,
        }
    }

    /// Tetapkan filter daun pada komponen ujung path → siap `.load_all(...)`.
    pub fn where_(self, leaf: Filter<Current>) -> PathLoad<'a> {
        PathLoad {
            store: self.store,
            hops: self.hops,
            leaf_sql: leaf.sql,
            leaf_params: leaf.params,
        }
    }
}

/// Path relasi siap dimuat (tipe di-erase). [`load_all`] memuat entity **root**
/// yang cocok **dan** entity di tiap hop sepanjang path yang cocok (RFC-0032).
///
/// [`load_all`]: Self::load_all
pub struct PathLoad<'a> {
    store: &'a mut PgStore,
    hops: Vec<Hop>,
    leaf_sql: String,
    leaf_params: Vec<(PgType, PgValue)>,
}

impl<'a> PathLoad<'a> {
    /// Filter bersarang pada tabel root: `r1 IN (SELECT … r2 IN (SELECT … <leaf>))`.
    fn root_filter(&self) -> String {
        let mut cur = self.leaf_sql.clone();
        for hop in self.hops.iter().rev() {
            cur = join_cond(hop.rel_column, hop.to_table, &cur);
        }
        cur
    }

    /// Muat entity root yang cocok + entity target di tiap hop sepanjang path
    /// yang cocok. Mengembalikan jumlah entity **root** dimuat.
    pub async fn load_all(self, world: &mut World) -> Result<usize, sqlx::Error> {
        let root_filter = self.root_filter();
        let root_table = self.hops[0].from_table;

        // Susun semua SQL (pinjam-baca) sebelum menyentuh store.
        let root_sql = renumber(&format!(
            "SELECT entity_id FROM {root_table} WHERE {root_filter} ORDER BY entity_id"
        ));
        // Query id entity yang cocok di level sebelumnya (level 0 = root).
        let mut matched_prev = format!("SELECT entity_id FROM {root_table} WHERE {root_filter}");
        let mut level_loads: Vec<String> = Vec::new();
        for hop in &self.hops {
            let targets = format!(
                "SELECT DISTINCT {rel} AS entity_id FROM {from} \
                 WHERE entity_id IN ({matched_prev}) AND {rel} IS NOT NULL",
                rel = hop.rel_column,
                from = hop.from_table,
            );
            level_loads.push(renumber(&targets));
            matched_prev = targets; // level ini jadi "sebelumnya" utk hop berikut
        }

        let store = self.store;
        let n = store
            .load_by_query(root_sql, self.leaf_params.clone(), world)
            .await?;
        for sql in level_loads {
            store
                .load_by_query(sql, self.leaf_params.clone(), world)
                .await?;
        }
        Ok(n)
    }
}

/// Kondisi join antar-entity (RFC-0031) sebagai sub-query (menghindari alias):
/// `<rel> IN (SELECT entity_id FROM <tbl> WHERE <filter>)`.
fn join_cond(rel_column: &str, related_table: &str, filter_sql: &str) -> String {
    format!("{rel_column} IN (SELECT entity_id FROM {related_table} WHERE {filter_sql})")
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
        const COLUMNS: &'static [ColumnDef] = &[ColumnDef::scalar("hp", PgType::Integer, false)];
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
        // Token relasi (untuk uji nesting RFC-0032).
        fn boss() -> Field<Self, EntityRef> {
            Field::new("boss_id", PgType::BigInt)
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

    #[test]
    fn join_subquery_terparameterisasi() {
        // Filter dasar pada T + join antar-entity → sub-query; placeholder `?`
        // dinomori **global** ($1, $2, …) melintasi filter dasar & sub-query.
        let base = Health::hp().gte(5); // "hp >= ?"
        let jc = join_cond("of_id", "cmp_health", &Health::hp().lt(20).sql);
        let where_sql = format!("{} AND {}", base.sql, jc);
        let sql = renumber(&format!(
            "SELECT entity_id FROM cmp_owner WHERE {where_sql} ORDER BY entity_id"
        ));
        assert_eq!(
            sql,
            "SELECT entity_id FROM cmp_owner WHERE hp >= $1 AND of_id IN \
             (SELECT entity_id FROM cmp_health WHERE hp < $2) ORDER BY entity_id"
        );
    }

    #[test]
    fn matches_bersarang_3_deep() {
        // boss → boss → hp (rantai relasi 3-deep, RFC-0032). Sub-query bersarang,
        // placeholder global.
        let f =
            Health::boss().matches::<Health>(Health::boss().matches::<Health>(Health::hp().lt(20)));
        let (sql, params) = built(Some(f), vec![], None, None);
        assert_eq!(
            sql,
            "SELECT entity_id FROM cmp_health WHERE boss_id IN \
             (SELECT entity_id FROM cmp_health WHERE boss_id IN \
             (SELECT entity_id FROM cmp_health WHERE hp < $1)) ORDER BY entity_id"
        );
        assert_eq!(params, vec![(PgType::Integer, PgValue::Int(20))]);
    }
}
