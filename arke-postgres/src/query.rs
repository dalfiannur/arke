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
//! Total tanpa paginasi: `.count()` (`SELECT COUNT(*)` dengan `WHERE` yang sama;
//! `Filter` dapat di-`clone()` untuk dipakai ulang). Field array (`Vec<V>` →
//! JSONB): `Tags::ids().contains(7)` / `.contains_all([1, 2])` → `@>`.
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

use arke::{Entity, World};
use sqlx::Row;

use crate::tx::PgTx;
use crate::{PgComponent, PgStore, PgType, PgValue, quote_ident};

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
#[cfg(feature = "uuid")]
impl IntoPgValue for uuid::Uuid {
    fn into_pg_value(self) -> PgValue {
        PgValue::Text(self.to_string())
    }
}
#[cfg(feature = "chrono")]
impl IntoPgValue for chrono::DateTime<chrono::Utc> {
    fn into_pg_value(self) -> PgValue {
        PgValue::Text(crate::__private::ts_to_text(&self))
    }
}
/// Field array (`Vec<V>` → JSONB): nilai di-encode via `arke::Serialize` —
/// representasi sama dengan yang ditulis `to_params`. Dipakai `set` massal.
impl<T: arke::Serialize> IntoPgValue for Vec<T> {
    fn into_pg_value(self) -> PgValue {
        let list = arke::Value::List(self.iter().map(|v| v.to_value()).collect());
        PgValue::Json(list.to_json())
    }
}
/// Field nullable (`Option<V>`): `Some(v)` → nilai `v`, `None` → `NULL`.
/// Ingat semantik SQL: `col = NULL` tak pernah benar — untuk WHERE pakai
/// [`Field::is_null`]; `None` berguna untuk `set` (mengosongkan kolom).
impl<T: IntoPgValue> IntoPgValue for Option<T> {
    fn into_pg_value(self) -> PgValue {
        match self {
            Some(v) => v.into_pg_value(),
            None => PgValue::Null,
        }
    }
}

/// Token field typed untuk komponen `C`, bertipe nilai `V` (di-generate derive).
pub struct Field<C, V> {
    pub(crate) column: &'static str,
    pub(crate) ty: PgType,
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

    /// Nama kolom ter-quote untuk SQL (kata kunci/huruf besar aman).
    pub(crate) fn col(&self) -> std::borrow::Cow<'static, str> {
        quote_ident(self.column)
    }

    /// Cast placeholder yang dibutuhkan tipe kolom (tipe yang di-bind teks).
    pub(crate) fn cast(&self) -> &'static str {
        self.ty.bind_cast()
    }
}

/// Predikat `WHERE` typed atas komponen `C`. Bangun dari operator [`Field`],
/// gabung dengan [`Filter::and`]/[`Filter::or`]/[`Filter::not`].
pub struct Filter<C> {
    /// Fragmen SQL dengan placeholder `?` (dinomori ulang jadi `$n` saat rakit).
    pub(crate) sql: String,
    /// Nilai ter-bind, urut sesuai kemunculan `?`.
    pub(crate) params: Vec<(PgType, PgValue)>,
    _pd: PhantomData<fn() -> C>,
}

impl<C> Clone for Filter<C> {
    fn clone(&self) -> Self {
        Self::raw(self.sql.clone(), self.params.clone())
    }
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
            format!("{} {} ?{}", self.col(), op, self.cast()),
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
            format!("{} BETWEEN ?{cast} AND ?{cast}", self.col()),
            vec![(self.ty, lo.into_pg_value()), (self.ty, hi.into_pg_value())],
        )
    }

    /// `col IS NULL` (untuk field `Option<T>`).
    pub fn is_null(self) -> Filter<C> {
        Filter::raw(format!("{} IS NULL", self.col()), vec![])
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
        Filter::raw(format!("{} IN ({placeholders})", self.col()), params)
    }

    /// **Semi-join by value** (tanpa relasi Entity): `col IN (SELECT other FROM
    /// cmp_R WHERE f)` — cocok bila nilai kolom ini ada di kolom `other` milik
    /// komponen `R` yang memenuhi `f`. Kedua kolom bertipe `V` (dicek compiler).
    /// Mis. booking yang `room_code`-nya menunjuk ruang berkapasitas > 10:
    /// `Booking::room_code().in_where(Room::code(), Room::capacity().gt(10))`.
    pub fn in_where<R: PgComponent>(self, other: Field<R, V>, f: Filter<R>) -> Filter<C> {
        Filter::raw(
            format!(
                "{} IN (SELECT {} FROM {} WHERE {})",
                self.col(),
                other.col(),
                quote_ident(R::TABLE),
                f.sql
            ),
            f.params,
        )
    }
}

/// `LIKE` hanya untuk field teks (type-safety): `Health::hp().like(..)` (integer)
/// tak dapat dikompilasi.
impl<C: PgComponent> Field<C, String> {
    /// `col LIKE pattern` (mis. `"a%"`).
    pub fn like(self, pattern: impl Into<String>) -> Filter<C> {
        Filter::raw(
            format!("{} LIKE ?", self.col()),
            vec![(PgType::Text, PgValue::Text(pattern.into()))],
        )
    }

    /// **Full-text search**: `to_tsvector(cfg, col) @@ websearch_to_tsquery(cfg, ?)`.
    /// Sintaks `query` ala mesin pencari: kata = AND, `OR`, `-kata` = NOT,
    /// `"frasa"`. `cfg` = `#[pg(fts = "…")]` field ini (indeks GIN dibuat
    /// `migrate`); field tanpa atribut tetap boleh (`simple`, tanpa indeks).
    /// Padukan dengan [`Query::order_by_rank`] untuk urutan relevansi.
    pub fn search(self, query: impl Into<String>) -> Filter<C> {
        let cfg = fts_config::<C>(self.column);
        Filter::raw(
            format!(
                "{} @@ {}",
                tsvector_expr(cfg, &self.col()),
                tsquery_expr(cfg)
            ),
            vec![(PgType::Text, PgValue::Text(query.into()))],
        )
    }
}

/// Operator *containment* JSONB (`@>`) untuk field array (`Vec<V>` → kolom JSONB
/// via derive). Nilai dibangun lewat [`arke::Serialize`] agar representasinya
/// identik dengan yang ditulis `to_params`. Memanfaatkan index GIN bila ada.
macro_rules! jsonb_contains {
    ($($ty:ty),*) => { $(
        impl<C: PgComponent, V: arke::Serialize> Field<C, $ty> {
            /// `col @> '[v]'` — array memuat elemen `v`.
            pub fn contains(self, v: V) -> Filter<C> {
                self.contains_all([v])
            }

            /// `col @> '[v1, v2, …]'` — array memuat **semua** elemen `vals`
            /// (urutan bebas). Iterator kosong → `@> '[]'` → selalu benar untuk
            /// baris non-NULL.
            pub fn contains_all<I: IntoIterator<Item = V>>(self, vals: I) -> Filter<C> {
                let list = arke::Value::List(vals.into_iter().map(|v| v.to_value()).collect());
                Filter::raw(
                    format!("{} @> ?::jsonb", self.col()),
                    vec![(PgType::Jsonb, PgValue::Json(list.to_json()))],
                )
            }
        }
    )* };
}
jsonb_contains!(Vec<V>, Option<Vec<V>>);

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

// ---------------------------------------------------------------------------
// Paginasi keyset (RFC-0030 lanjutan): kursor opaque + kondisi `WHERE` keyset.
// ---------------------------------------------------------------------------

/// Kursor **keyset** opaque: nilai kunci `ORDER BY` + `pid` baris tepi halaman.
/// Dibuat oleh [`Query::load_page`] (`next`/`prev`), diteruskan ke
/// [`Query::after`]/[`Query::before`]. Stabil sebagai string aman-URL lewat
/// [`Cursor::encode`]/[`Cursor::decode`] (juga `Display`/`FromStr`); isinya
/// **bukan** rahasia (bukan tanda tangan) — validasi kecocokan kunci dilakukan
/// saat query dijalankan ([`CursorError::Mismatch`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Cursor {
    keys: Vec<PgValue>,
    pid: i64,
}

/// Kegagalan membaca/mencocokkan [`Cursor`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorError {
    /// Token bukan hasil [`Cursor::encode`] (base64url/JSON/skema tak dikenal).
    Malformed,
    /// Kursor tak cocok dengan `order_by` query ini (jumlah/tipe kunci), atau
    /// kunci tak dapat dipakai keyset (`NULL`, JSONB, referensi entity).
    Mismatch(String),
}

impl std::fmt::Display for CursorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CursorError::Malformed => write!(f, "kursor keyset rusak/tak dikenal"),
            CursorError::Mismatch(why) => write!(f, "kursor keyset tak cocok: {why}"),
        }
    }
}

impl std::error::Error for CursorError {}

/// Kegagalan [`Query::load_page`].
#[derive(Debug)]
pub enum PageError {
    /// `load_page` butuh [`Query::limit`] (ukuran halaman).
    MissingLimit,
    /// [`Query::offset`] tak bermakna bersama kursor — pakai salah satu.
    OffsetWithCursor,
    /// Kursor tak cocok/rusak.
    Cursor(CursorError),
    /// Galat database.
    Db(sqlx::Error),
}

impl std::fmt::Display for PageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PageError::MissingLimit => write!(f, "load_page butuh limit (ukuran halaman)"),
            PageError::OffsetWithCursor => write!(f, "offset tak dapat dipakai bersama kursor"),
            PageError::Cursor(e) => write!(f, "{e}"),
            PageError::Db(e) => write!(f, "database: {e}"),
        }
    }
}

impl std::error::Error for PageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PageError::Cursor(e) => Some(e),
            PageError::Db(e) => Some(e),
            _ => None,
        }
    }
}

impl From<sqlx::Error> for PageError {
    fn from(e: sqlx::Error) -> Self {
        PageError::Db(e)
    }
}

impl From<CursorError> for PageError {
    fn from(e: CursorError) -> Self {
        PageError::Cursor(e)
    }
}

/// Satu halaman hasil [`Query::load_page`].
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    /// `(pid, Entity)` urut sesuai `ORDER BY` query (arah maju), ≤ `limit`.
    pub items: Vec<(i64, Entity)>,
    /// Kursor halaman **berikut** (`Some` bila masih ada baris setelah halaman
    /// ini) — teruskan ke [`Query::after`].
    pub next: Option<Cursor>,
    /// Kursor halaman **sebelum** (`Some` bila halaman ini dicapai lewat kursor,
    /// atau `before` menemukan baris lebih awal) — teruskan ke [`Query::before`].
    pub prev: Option<Cursor>,
}

/// Arah kursor pada [`Query`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bound {
    After,
    Before,
}

impl Cursor {
    /// Encode ke token aman-URL (`[A-Za-z0-9_-]`): JSON `{"v":1,"k":[[tag,val]…],"p":pid}`
    /// di-base64url tanpa padding.
    pub fn encode(&self) -> String {
        use arke::Value;
        let keys: Vec<Value> = self
            .keys
            .iter()
            .map(|v| {
                let (tag, val) = match v {
                    PgValue::Int(i) => ("i", Value::Int(*i)),
                    PgValue::Float(f) => ("f", Value::Float(*f)),
                    PgValue::Bool(b) => ("b", Value::Bool(*b)),
                    PgValue::Text(t) => ("t", Value::Text(t.clone())),
                    PgValue::Numeric(n) => ("n", Value::Text(n.clone())),
                    // Ditolak saat validasi (`keyset_where`); tak pernah sampai sini
                    // lewat `load_page`. Dipetakan agar `encode` total.
                    PgValue::Json(j) => ("j", Value::Text(j.clone())),
                    PgValue::Ref(r) => ("r", Value::Int(*r)),
                    PgValue::Null => ("_", Value::Null),
                };
                Value::List(vec![Value::Text(tag.to_string()), val])
            })
            .collect();
        let doc = Value::Map(vec![
            ("v".to_string(), Value::Int(1)),
            ("k".to_string(), Value::List(keys)),
            ("p".to_string(), Value::Int(self.pid)),
        ]);
        base64url_encode(doc.to_json().as_bytes())
    }

    /// Kebalikan [`Cursor::encode`].
    pub fn decode(token: &str) -> Result<Cursor, CursorError> {
        use arke::Value;
        let bytes = base64url_decode(token).ok_or(CursorError::Malformed)?;
        let text = String::from_utf8(bytes).map_err(|_| CursorError::Malformed)?;
        let Some(Value::Map(fields)) = Value::from_json(&text) else {
            return Err(CursorError::Malformed);
        };
        let get = |name: &str| fields.iter().find(|(k, _)| k == name).map(|(_, v)| v);
        if get("v") != Some(&Value::Int(1)) {
            return Err(CursorError::Malformed);
        }
        let Some(Value::Int(pid)) = get("p") else {
            return Err(CursorError::Malformed);
        };
        let Some(Value::List(list)) = get("k") else {
            return Err(CursorError::Malformed);
        };
        let mut keys = Vec::with_capacity(list.len());
        for item in list {
            let Value::List(pair) = item else {
                return Err(CursorError::Malformed);
            };
            let [Value::Text(tag), val] = pair.as_slice() else {
                return Err(CursorError::Malformed);
            };
            keys.push(match (tag.as_str(), val) {
                ("i", Value::Int(i)) => PgValue::Int(*i),
                ("f", Value::Float(f)) => PgValue::Float(*f),
                ("f", Value::Int(i)) => PgValue::Float(*i as f64),
                ("b", Value::Bool(b)) => PgValue::Bool(*b),
                ("t", Value::Text(t)) => PgValue::Text(t.clone()),
                ("n", Value::Text(n)) => PgValue::Numeric(n.clone()),
                ("j", Value::Text(j)) => PgValue::Json(j.clone()),
                ("r", Value::Int(r)) => PgValue::Ref(*r),
                ("_", Value::Null) => PgValue::Null,
                _ => return Err(CursorError::Malformed),
            });
        }
        Ok(Cursor { keys, pid: *pid })
    }
}

impl std::fmt::Display for Cursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.encode())
    }
}

impl std::str::FromStr for Cursor {
    type Err = CursorError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Cursor::decode(s)
    }
}

const B64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// base64url tanpa padding (RFC 4648 §5) — 0 dependensi.
fn base64url_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let chars = [
            B64URL[(n >> 18) as usize & 63],
            B64URL[(n >> 12) as usize & 63],
            B64URL[(n >> 6) as usize & 63],
            B64URL[n as usize & 63],
        ];
        let keep = chunk.len() + 1;
        out.extend(chars[..keep].iter().map(|&c| c as char));
    }
    out
}

fn base64url_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| B64URL.iter().position(|&x| x == c).map(|p| p as u32);
    let bytes = s.as_bytes();
    if bytes.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut n: u32 = 0;
        for &c in chunk {
            n = (n << 6) | val(c)?;
        }
        n <<= 6 * (4 - chunk.len());
        let full = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&full[..chunk.len() - 1]);
    }
    Some(out)
}

/// SQL utama ter-parameterisasi + nilai bind + apakah urutan dibalik (`before`).
type BuiltSql = (String, Vec<(PgType, PgValue)>, bool);

/// Satu kunci `ORDER BY`: kolom biasa, atau **ekspresi** ter-parameterisasi
/// (mis. `ts_rank(...)` dari [`Query::order_by_rank`]) yang di-`SELECT` dengan
/// alias `column` saat membentuk kursor.
#[derive(Clone, Debug)]
struct OrderKey {
    /// Nama kolom, atau alias hasil bila `expr` ada.
    column: &'static str,
    ty: PgType,
    dir: Dir,
    /// Ekspresi SQL (placeholder `?`) + nilai bind; `None` → kolom biasa.
    expr: Option<(String, Vec<(PgType, PgValue)>)>,
}

impl OrderKey {
    fn column(column: &'static str, ty: PgType, dir: Dir) -> Self {
        Self {
            column,
            ty,
            dir,
            expr: None,
        }
    }

    /// SQL kunci (ekspresi atau kolom ter-quote) + param yang dibawanya.
    fn sql(&self) -> (String, &[(PgType, PgValue)]) {
        match &self.expr {
            Some((e, p)) => (e.clone(), p.as_slice()),
            None => (quote_ident(self.column).into_owned(), &[]),
        }
    }
}

/// Kunci efektif keyset: `order` + tiebreak `pid` (arah = arah kunci terakhir,
/// `Asc` bila tanpa `order_by`) — urutan total & deterministik.
fn keyset_keys(order: &[OrderKey]) -> Vec<OrderKey> {
    let mut keys = order.to_vec();
    let dir = order.last().map_or(Dir::Asc, |k| k.dir);
    keys.push(OrderKey::column("pid", PgType::BigInt, dir));
    keys
}

/// Klausa `ORDER BY` untuk `keys` (dibalik bila `reverse`) + param ekspresi
/// (urut tekstual).
fn order_by_sql(keys: &[OrderKey], reverse: bool) -> (String, Vec<(PgType, PgValue)>) {
    let mut params = Vec::new();
    let parts: Vec<String> = keys
        .iter()
        .map(|k| {
            let dir = match (k.dir, reverse) {
                (Dir::Asc, false) | (Dir::Desc, true) => "ASC",
                (Dir::Desc, false) | (Dir::Asc, true) => "DESC",
            };
            let (sql, p) = k.sql();
            params.extend_from_slice(p);
            format!("{sql} {dir}")
        })
        .collect();
    (parts.join(", "), params)
}

/// Kondisi `WHERE` keyset "baris setelah `cursor`" dalam urutan `keys`
/// (`bound = After`), atau "sebelum" (`Before`), dengan placeholder `?`.
/// Arah seragam → row-value compare `(k1, k2, pid) > (?, ?, ?)` (ramah index);
/// arah campur → bentuk OR-expanded. Memvalidasi kursor terhadap `keys`.
fn keyset_where(
    keys: &[OrderKey],
    cursor: &Cursor,
    bound: Bound,
) -> Result<(String, Vec<(PgType, PgValue)>), CursorError> {
    // `keys` sudah termasuk `pid` di ujung; kursor menyimpan kunci tanpa pid.
    let n = keys.len() - 1;
    if cursor.keys.len() != n {
        return Err(CursorError::Mismatch(format!(
            "kursor membawa {} kunci, order_by query {n}",
            cursor.keys.len()
        )));
    }
    // Nilai kursor per kunci (validasi tipe); param SQL dirakit di bawah dalam
    // urutan tekstual (param ekspresi kunci ⇢ nilai kursor).
    let mut values: Vec<(PgType, PgValue)> = Vec::with_capacity(keys.len());
    for (k, v) in keys.iter().zip(
        cursor
            .keys
            .iter()
            .chain(std::iter::once(&PgValue::Int(cursor.pid))),
    ) {
        let ok = match (k.ty, v) {
            (PgType::Integer | PgType::BigInt, PgValue::Int(_)) => true,
            (PgType::Real | PgType::DoublePrecision, PgValue::Float(_)) => true,
            (PgType::Boolean, PgValue::Bool(_)) => true,
            (PgType::Text, PgValue::Text(_)) => true,
            (PgType::Numeric, PgValue::Numeric(_)) => true,
            (PgType::Uuid | PgType::TimestampTz, PgValue::Text(_)) => true,
            (_, PgValue::Null) => {
                return Err(CursorError::Mismatch(format!(
                    "kunci `{}` NULL — keyset butuh kunci non-NULL",
                    k.column
                )));
            }
            (PgType::Jsonb, _) => {
                return Err(CursorError::Mismatch(format!(
                    "kunci `{}` JSONB tak dapat dipakai keyset",
                    k.column
                )));
            }
            _ => false,
        };
        if !ok {
            return Err(CursorError::Mismatch(format!(
                "tipe kunci `{}` ({:?}) tak cocok nilai kursor",
                k.column, k.ty
            )));
        }
        values.push((k.ty, v.clone()));
    }

    let op = |k: &OrderKey| match (k.dir, bound) {
        (Dir::Asc, Bound::After) | (Dir::Desc, Bound::Before) => ">",
        (Dir::Desc, Bound::After) | (Dir::Asc, Bound::Before) => "<",
    };
    let ph = |k: &OrderKey| format!("?{}", cast_of(k.ty));
    let uniform = keys.iter().all(|k| k.dir == keys[0].dir);
    let mut params: Vec<(PgType, PgValue)> = Vec::new();
    let sql = if uniform {
        // (k1, k2, pid) op (?, ?, ?) — param ekspresi kunci dulu, lalu nilai kursor.
        let mut cols = Vec::with_capacity(keys.len());
        for k in keys {
            let (sql, p) = k.sql();
            params.extend_from_slice(p);
            cols.push(sql);
        }
        let phs: Vec<String> = keys.iter().map(ph).collect();
        params.extend(values.iter().cloned());
        format!(
            "({}) {} ({})",
            cols.join(", "),
            op(&keys[0]),
            phs.join(", ")
        )
    } else {
        // (k1 op ?) OR (k1 = ? AND k2 op ?) OR (k1 = ? AND k2 = ? AND pid op ?)
        // Placeholder diulang per cabang → params diduplikasi sesuai urutan.
        let mut branches = Vec::with_capacity(keys.len());
        for i in 0..keys.len() {
            let mut conds = Vec::with_capacity(i + 1);
            for (j, k) in keys.iter().enumerate().take(i + 1) {
                let o = if j < i { "=" } else { op(k) };
                let (sql, p) = k.sql();
                params.extend_from_slice(p);
                conds.push(format!("{sql} {o} {}", ph(k)));
                params.push(values[j].clone());
            }
            branches.push(format!("({})", conds.join(" AND ")));
        }
        branches.join(" OR ")
    };
    Ok((sql, params))
}

/// Ekspresi `to_tsvector('<cfg>', <col_sql>)` — **satu** definisi dipakai indeks
/// GIN (`migrate`), `search`, dan `order_by_rank`, agar planner mencocokkan
/// indeks ekspresi.
pub(crate) fn tsvector_expr(config: &str, col_sql: &str) -> String {
    format!("to_tsvector('{config}', {col_sql})")
}

/// Ekspresi `websearch_to_tsquery('<cfg>', ?)` (teks pencarian di-bind).
fn tsquery_expr(config: &str) -> String {
    format!("websearch_to_tsquery('{config}', ?)")
}

/// Config FTS kolom `column` pada komponen `C` (`#[pg(fts = …)]`), atau
/// `simple` bila kolom tak ditandai (search tetap valid, tanpa indeks).
fn fts_config<C: PgComponent>(column: &str) -> &'static str {
    C::FTS
        .iter()
        .find(|f| f.column == column)
        .map_or("simple", |f| f.config)
}

/// Cast placeholder untuk tipe kolom (tipe yang di-bind sebagai teks).
fn cast_of(ty: PgType) -> &'static str {
    ty.bind_cast()
}

/// Himpunan komponen untuk **hidrasi selektif** ([`Query::only`]): satu
/// komponen (`only::<Health>()`) atau tuple 1–8 (`only::<(Health, Position)>()`).
pub trait ComponentSet {
    /// Nama tabel tiap komponen dalam himpunan.
    const TABLES: &'static [&'static str];
}

impl<C: PgComponent> ComponentSet for C {
    const TABLES: &'static [&'static str] = &[C::TABLE];
}

macro_rules! component_set_tuple {
    ($($name:ident),+) => {
        impl<$($name: PgComponent),+> ComponentSet for ($($name,)+) {
            const TABLES: &'static [&'static str] = &[$($name::TABLE),+];
        }
    };
}
component_set_tuple!(A);
component_set_tuple!(A, B);
component_set_tuple!(A, B, C);
component_set_tuple!(A, B, C, D);
component_set_tuple!(A, B, C, D, E);
component_set_tuple!(A, B, C, D, E, F);
component_set_tuple!(A, B, C, D, E, F, G);
component_set_tuple!(A, B, C, D, E, F, G, H);

/// Builder query baca ber-filter (RFC-0030) + join antar-entity (RFC-0031).
/// Dibuat oleh [`PgStore::query`].
pub struct Query<'a, T: PgComponent> {
    pub(crate) store: &'a mut PgStore,
    filter: Option<Filter<T>>,
    joins: Vec<JoinClause>,
    order: Vec<OrderKey>,
    limit: Option<i64>,
    offset: Option<i64>,
    /// Hidrasi selektif: tabel komponen yang dimuat (`None` = semua terdaftar).
    only: Option<&'static [&'static str]>,
    /// Kursor keyset (`after`/`before`).
    cursor: Option<(Cursor, Bound)>,
    /// Kondisi **lintas komponen** pada entity yang sama
    /// (`with`/`with_where`/`without`): SQL + param.
    archetype: Vec<(String, Vec<(PgType, PgValue)>)>,
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
            only: None,
            cursor: None,
            archetype: Vec::new(),
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

    /// Saring entity yang **juga memiliki** komponen `R` (kehadiran, apa pun
    /// nilainya) — padanan `With<R>` arke core di sisi Postgres:
    /// `pid IN (SELECT pid FROM cmp_r)`. Lihat [`Self::with_where`] untuk
    /// predikat atas `R`, [`Self::without`] untuk ketiadaan.
    pub fn with<R: PgComponent>(mut self) -> Self {
        self.archetype
            .push((archetype_cond(R::TABLE, None, false), Vec::new()));
        self
    }

    /// Saring entity yang memiliki komponen `R` yang **memenuhi** `filter`
    /// (join lintas komponen pada entity yang sama, bukan relasi FK):
    /// `pid IN (SELECT pid FROM cmp_r WHERE <filter>)`. Satu semi-join per
    /// komponen — pengganti `INTERSECT` + `EXISTS` pada model JSONB. Berlaku
    /// juga untuk `count`/`exists`/`count_estimate`/`load_page`. Boleh
    /// berkali-kali (digabung `AND`).
    pub fn with_where<R: PgComponent>(mut self, filter: Filter<R>) -> Self {
        self.archetype.push((
            archetype_cond(R::TABLE, Some(&filter.sql), false),
            filter.params,
        ));
        self
    }

    /// Saring entity yang **tidak** memiliki komponen `R` — padanan `Without<R>`:
    /// `pid NOT IN (SELECT pid FROM cmp_r)`.
    pub fn without<R: PgComponent>(mut self) -> Self {
        self.archetype
            .push((archetype_cond(R::TABLE, None, true), Vec::new()));
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

    /// Tambah kunci `ORDER BY` (bisa berkali-kali untuk multi-kunci). `pid`
    /// selalu ditambahkan sebagai kunci pengikat terakhir (arah mengikuti kunci
    /// terakhir) → urutan total & deterministik, prasyarat paginasi keyset.
    pub fn order_by<V>(mut self, field: Field<T, V>, dir: Dir) -> Self {
        self.order
            .push(OrderKey::column(field.column, field.ty, dir));
        self
    }

    /// Urutkan berdasarkan **relevansi full-text** (`ts_rank`, menurun) kolom
    /// `field` terhadap `query` (sintaks `websearch_to_tsquery`); biasanya
    /// dipadukan dengan `.filter(field.search(query))`. Berlaku sebagai kunci
    /// `ORDER BY` biasa: bisa ditumpuk dengan `order_by` dan dipakai
    /// [`load_page`](Self::load_page) (kursor membawa nilai rank).
    pub fn order_by_rank(mut self, field: Field<T, String>, query: impl Into<String>) -> Self {
        let cfg = fts_config::<T>(field.column);
        let expr = format!(
            "ts_rank({}, {})",
            tsvector_expr(cfg, &field.col()),
            tsquery_expr(cfg)
        );
        // Alias statis per posisi kunci (tak boleh bentrok dengan kolom komponen).
        const ALIASES: [&str; 8] = [
            "__rank0", "__rank1", "__rank2", "__rank3", "__rank4", "__rank5", "__rank6", "__rank7",
        ];
        let alias = ALIASES[self.order.len().min(ALIASES.len() - 1)];
        self.order.push(OrderKey {
            column: alias,
            ty: PgType::Real,
            dir: Dir::Desc,
            expr: Some((expr, vec![(PgType::Text, PgValue::Text(query.into()))])),
        });
        self
    }

    /// **Keyset**: hanya baris **setelah** `cursor` dalam urutan `order_by`
    /// (kursor dari [`Page::next`]). Berlaku untuk `load`/`load_pids`/`load_page`;
    /// `count()` mengabaikannya (total keseluruhan). Kursor harus berasal dari
    /// query dengan `order_by` yang sama — bila tidak, [`CursorError::Mismatch`].
    pub fn after(mut self, cursor: Cursor) -> Self {
        self.cursor = Some((cursor, Bound::After));
        self
    }

    /// **Keyset**: hanya baris **sebelum** `cursor` (kursor dari [`Page::prev`]);
    /// hasil tetap dikembalikan dalam urutan maju. Lihat [`Self::after`].
    pub fn before(mut self, cursor: Cursor) -> Self {
        self.cursor = Some((cursor, Bound::Before));
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

    /// **Hidrasi selektif**: muat hanya komponen dalam `S` untuk entity yang
    /// cocok — satu komponen (`only::<Health>()`) atau tuple
    /// (`only::<(Health, Position)>()`). Round-trip turun dari `2 + R` (R = semua
    /// komponen terdaftar) menjadi `2 + |S|`. `S` **tidak** harus memuat `T`.
    ///
    /// Komponen di luar `S` tak disentuh di `World` (tak dimuat, tak dilepas).
    /// Entity yang **baru** dimuat lewat jalur ini dicatat **parsial**: pada
    /// `update_entity`/`save_incremental`, tabel yang tak dimuat untuknya
    /// **dilewati** (tidak di-DELETE), sehingga komponen yang tak dimuat tetap
    /// utuh di DB. Status parsial dilepas bila entity itu kemudian dimuat penuh
    /// (tanpa `only`) ke World yang sama. `save()` (overwrite penuh) **tidak**
    /// dijaga — sama seperti working-set parsial `load_where`, pakai
    /// `save_incremental`/`update_entity`.
    pub fn only<S: ComponentSet>(mut self) -> Self {
        self.only = Some(S::TABLES);
        self
    }

    /// Klausa `WHERE` (filter dasar + sub-query join) dengan placeholder `?` +
    /// nilai bind-nya. `None` bila tak ada kondisi.
    pub(crate) fn where_clause(&self) -> (Option<String>, Vec<(PgType, PgValue)>) {
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
        for (sql, p) in &self.archetype {
            conds.push(sql.clone());
            params.extend(p.iter().cloned());
        }
        if conds.is_empty() {
            (None, params)
        } else {
            (Some(conds.join(" AND ")), params)
        }
    }

    /// Susun SQL utama ter-parameterisasi + nilai bind, dengan `limit` eksplisit
    /// dan, bila `select_keys`,
    /// kolom kunci `ORDER BY` ikut di-`SELECT` (untuk membentuk kursor tepi
    /// halaman). Mengembalikan pula apakah urutan SQL **dibalik** (`before`) —
    /// pemanggil membalik hasil di memori.
    fn build_with(&self, limit: Option<i64>, select_keys: bool) -> Result<BuiltSql, CursorError> {
        let keys = keyset_keys(&self.order);
        // Param dirakit dalam **urutan tekstual** SQL (SELECT → WHERE → ORDER BY
        // → LIMIT/OFFSET) karena `renumber` menomori `?` berurutan.
        let mut params: Vec<(PgType, PgValue)> = Vec::new();

        let mut select = String::from("pid");
        if select_keys {
            // NUMERIC/UUID/TIMESTAMPTZ dibaca sebagai teks (`read_expr`, lihat
            // `read_typed`); JSONB ditolak `keyset_where`. Kunci ekspresi
            // di-alias dengan `column`-nya.
            for k in &self.order {
                let (ksql, p) = k.sql();
                params.extend_from_slice(p);
                let alias = quote_ident(k.column);
                // JSONB tetap dibaca apa adanya (ditolak `keyset_where`).
                let text = (k.ty != PgType::Jsonb)
                    .then(|| k.ty.read_expr(&ksql))
                    .flatten();
                match (text, &k.expr) {
                    (Some(expr), _) => select.push_str(&format!(", {expr} AS {alias}")),
                    (None, Some(_)) => select.push_str(&format!(", {ksql} AS {alias}")),
                    (None, None) => select.push_str(&format!(", {ksql}")),
                }
            }
        }

        let (where_opt, where_params) = self.where_clause();
        let mut conds: Vec<String> = where_opt.into_iter().map(|w| format!("({w})")).collect();
        params.extend(where_params);
        let mut reversed = false;
        if let Some((cursor, bound)) = &self.cursor {
            let (ks, ks_params) = keyset_where(&keys, cursor, *bound)?;
            conds.push(format!("({ks})"));
            params.extend(ks_params);
            reversed = *bound == Bound::Before;
        }

        let mut sql = format!("SELECT {select} FROM {}", quote_ident(T::TABLE));
        if !conds.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conds.join(" AND "));
        }
        // Urutan total & deterministik (STD-0005 mirror pada sisi Postgres):
        // kunci `order_by` + tiebreak `pid`.
        let (order_sql, order_params) = order_by_sql(&keys, reversed);
        sql.push_str(" ORDER BY ");
        sql.push_str(&order_sql);
        params.extend(order_params);

        if let Some(l) = limit {
            sql.push_str(" LIMIT ?");
            params.push((PgType::BigInt, PgValue::Int(l)));
        }
        if let Some(o) = self.offset {
            sql.push_str(" OFFSET ?");
            params.push((PgType::BigInt, PgValue::Int(o)));
        }

        Ok((renumber(&sql), params, reversed))
    }

    /// SQL pemuat target `R` untuk `join_load`: id entity yang **ditunjuk** oleh
    /// `T` yang cocok (`SELECT DISTINCT <rel>_id ... WHERE <where>`).
    fn target_load_sql(&self, rel_column: &str) -> (String, Vec<(PgType, PgValue)>) {
        let (where_opt, params) = self.where_clause();
        let where_sql = where_opt.unwrap_or_else(|| "TRUE".to_string());
        let sql = format!(
            "SELECT DISTINCT {rel} AS pid FROM {tbl} WHERE ({where_sql}) AND {rel} IS NOT NULL",
            rel = quote_ident(rel_column),
            tbl = quote_ident(T::TABLE),
        );
        (renumber(&sql), params)
    }

    /// Hitung jumlah entity `T` yang cocok (`SELECT COUNT(*)`) memakai `WHERE`
    /// yang sama dengan [`load`](Self::load); `order_by`/`limit`/`offset`
    /// **diabaikan** — cocok untuk total halaman: bangun satu `Filter`, `clone()`
    /// untuk `count()`, sisanya untuk `load()` berpaginasi.
    pub async fn count(self) -> Result<u64, sqlx::Error> {
        let (sql, params) = self.count_query();
        let n = fetch_count(self.store.pool(), &sql, &params).await?;
        Ok(u64::try_from(n).unwrap_or(0))
    }

    /// **Estimasi** jumlah baris yang cocok dari planner Postgres
    /// (`EXPLAIN` → `rows=`), **tanpa** memindai tabel — O(1) terhadap ukuran
    /// tabel, cocok untuk "≈ N hasil"/total halaman kasar di list screen
    /// (padukan dengan [`load_page`](Self::load_page) yang sudah memberi
    /// `next`). Akurasinya bergantung statistik (`ANALYZE`/autovacuum) dan bisa
    /// meleset jauh untuk predikat berkorelasi; tabel kosong/baru bisa memberi
    /// ≥ 1. Untuk angka eksak pakai [`count`](Self::count). `WHERE` sama dengan
    /// `count()`; `order_by`/`limit`/`offset`/kursor diabaikan.
    pub async fn count_estimate(self) -> Result<u64, sqlx::Error> {
        let (where_opt, params) = self.where_clause();
        let sql = renumber(&explain_sql(&quote_ident(T::TABLE), where_opt.as_deref()));
        let rows = fetch_scalar_rows(self.store.pool(), &sql, &params).await?;
        let first: String = rows
            .first()
            .ok_or_else(|| sqlx::Error::Protocol("EXPLAIN tanpa baris".into()))?
            .try_get(0)?;
        parse_explain_rows(&first)
            .ok_or_else(|| sqlx::Error::Protocol(format!("EXPLAIN tanpa `rows=`: {first}")))
    }

    /// Seperti [`count`](Self::count) tetapi lewat koneksi transaksi `tx` milik
    /// pemanggil — melihat baris yang belum di-commit oleh `tx` itu (lihat
    /// [`crate::tx`]).
    pub async fn count_in(self, tx: &mut PgTx<'_>) -> Result<u64, sqlx::Error> {
        let (sql, params) = self.count_query();
        let n = fetch_count(tx.conn(), &sql, &params).await?;
        Ok(u64::try_from(n).unwrap_or(0))
    }

    /// Ada minimal satu entity `T` yang cocok? `SELECT EXISTS(SELECT 1 …)` —
    /// berhenti di baris pertama, lebih murah dari `count() > 0`.
    pub async fn exists(self) -> Result<bool, sqlx::Error> {
        let (sql, params) = self.exists_query();
        let store = self.store;
        fetch_exists(store.pool(), &sql, &params).await
    }

    /// Seperti [`exists`](Self::exists) tetapi lewat transaksi `tx` milik
    /// pemanggil — inti pola "lock → cek → tulis" (lihat [`crate::tx`]).
    pub async fn exists_in(self, tx: &mut PgTx<'_>) -> Result<bool, sqlx::Error> {
        let (sql, params) = self.exists_query();
        fetch_exists(tx.conn(), &sql, &params).await
    }

    fn count_query(&self) -> (String, Vec<(PgType, PgValue)>) {
        let (where_opt, params) = self.where_clause();
        (
            renumber(&count_sql(&quote_ident(T::TABLE), where_opt.as_deref())),
            params,
        )
    }

    /// Kelompokkan atas `key` — satu `Field` atau tuple 2–4 `Field`
    /// (`group_by((T::a(), T::b()))`) → terminal `count()`/`sum()`/… per kunci,
    /// opsional `.having(..)` (lihat [`crate::aggregate`]).
    pub fn group_by<G: crate::aggregate::GroupKey<T>>(
        self,
        key: G,
    ) -> crate::aggregate::Grouped<'a, T, G> {
        crate::aggregate::Grouped {
            query: self,
            key,
            having: None,
        }
    }

    fn exists_query(&self) -> (String, Vec<(PgType, PgValue)>) {
        let (where_opt, params) = self.where_clause();
        (
            renumber(&exists_sql(&quote_ident(T::TABLE), where_opt.as_deref())),
            params,
        )
    }

    /// Jalankan query, materialisasi entity `T` yang cocok (+ seluruh komponennya)
    /// ke `world`; untuk tiap `join_load`, muat pula entity target `R`.
    /// Mengembalikan jumlah entity `T` dimuat.
    pub async fn load(self, world: &mut World) -> Result<usize, sqlx::Error> {
        Ok(self.load_pids(world).await?.len())
    }

    /// Kunci baris `T` yang cocok (`SELECT … FOR UPDATE`, RFC-0040). Hasilnya
    /// hanya dapat dijalankan di transaksi ([`Locked::pids_in`]) — kunci di luar
    /// transaksi lepas seketika dan tak berarti.
    pub fn for_update(self) -> Locked<'a, T> {
        Locked {
            query: self,
            wait: LockWait::Wait,
        }
    }

    /// Seperti [`load`](Self::load) tetapi mengembalikan pasangan `(pid, Entity)`
    /// entity `T` yang dimuat (urut `ORDER BY` query) — id persisten untuk
    /// `remove(pid)`/`commit_update(pid)`/respons API, tanpa `load_where`.
    pub async fn load_pids(self, world: &mut World) -> Result<Vec<(i64, Entity)>, sqlx::Error> {
        // Kursor tak cocok di jalur non-`load_page` → galat protokol (`sqlx::Error`),
        // agar tanda tangan lama tetap; `load_page` memberi `PageError` bertipe.
        let (main_sql, main_params, reversed) = self
            .build_with(self.limit, false)
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
        let mut loaded = self.run_load(main_sql, main_params, world).await?;
        if reversed {
            loaded.reverse();
        }
        Ok(loaded)
    }

    /// Jalankan SQL utama (+ target `join_load`) dan materialisasi ke `world`.
    /// Urutan hasil = urutan baris SQL utama.
    async fn run_load(
        self,
        main_sql: String,
        main_params: Vec<(PgType, PgValue)>,
        world: &mut World,
    ) -> Result<Vec<(i64, Entity)>, sqlx::Error> {
        // Susun semua SQL (pinjam-baca `self`) sebelum menyentuh `self.store`.
        let targets: Vec<(String, Vec<(PgType, PgValue)>)> = self
            .joins
            .iter()
            .filter(|j| j.load)
            .map(|j| self.target_load_sql(j.rel_column))
            .collect();

        let store = self.store;
        let only = self.only;
        // Muat **aditif**: jembatan pid↔entity tidak di-reset, sehingga beberapa
        // `load` ke satu World (pola per-request: `fork()` → beberapa query →
        // `save_incremental`) saling melengkapi; pid yang sudah termuat di-refresh
        // di tempat (`materialize`). Satu store ↔ satu World; untuk World baru
        // pakai `fork()`.
        // Target dulu (RFC-0034 Am.3): entity utama me-resolve relasinya ke target
        // yang sudah ter-materialize di `entity_of`. Filter-saja (tanpa target) →
        // relasi utama menggantung (handle sentinel), entity tetap termuat.
        for (sql, params) in targets {
            store.load_by_query(sql, params, world, only).await?;
        }
        store
            .load_by_query(main_sql, main_params, world, only)
            .await
    }

    /// **Satu halaman keyset** tanpa `COUNT(*)`: mengambil `limit + 1` baris
    /// untuk tahu ada-tidaknya halaman berikut, memuat ≤ `limit` entity ke
    /// `world`, dan membentuk kursor tepi ([`Page::next`]/[`Page::prev`]).
    /// Butuh [`limit`](Self::limit); tak boleh bersama [`offset`](Self::offset).
    /// Halaman pertama: tanpa `after`/`before`. Kunci `order_by` harus non-NULL
    /// pada baris tepi (NULL → [`CursorError::Mismatch`]); baris ber-kunci NULL
    /// tak terjangkau keyset — beri `Option` default atau filter `is_null().not()`.
    pub async fn load_page(self, world: &mut World) -> Result<Page, PageError> {
        let Some(limit) = self.limit else {
            return Err(PageError::MissingLimit);
        };
        if self.offset.is_some() && self.cursor.is_some() {
            return Err(PageError::OffsetWithCursor);
        }
        let bound = self.cursor.as_ref().map(|(_, b)| *b);
        let (sql, params, reversed) = self.build_with(Some(limit + 1), true)?;
        let order: Vec<OrderKey> = self.order.clone();

        // Baris tepi + kunci: baca dulu dari SQL utama (ikut `only`/join lewat
        // `run_load` dengan daftar pid yang sudah dipangkas).
        let rows = self.store.fetch_rows(&sql, &params).await?;
        let mut edges: Vec<Cursor> = Vec::with_capacity(rows.len());
        for row in &rows {
            let pid: i64 = row.try_get("pid")?;
            let mut keys = Vec::with_capacity(order.len());
            for k in &order {
                keys.push(crate::store::read_typed(row, k.column, k.ty)?);
            }
            edges.push(Cursor { keys, pid });
        }
        let has_more = edges.len() as i64 > limit;
        edges.truncate(limit as usize);
        if reversed {
            edges.reverse();
        }

        // Muat entity halaman (urut maju) lewat daftar pid literal.
        let pids: Vec<i64> = edges.iter().map(|c| c.pid).collect();
        let items = if pids.is_empty() {
            Vec::new()
        } else {
            let list: Vec<String> = pids.iter().map(|p| p.to_string()).collect();
            let page_sql = format!(
                "SELECT pid FROM {} WHERE pid IN ({}) ORDER BY array_position(ARRAY[{}]::bigint[], pid)",
                quote_ident(T::TABLE),
                list.join(", "),
                list.join(", ")
            );
            self.run_load(page_sql, Vec::new(), world).await?
        };

        let first = edges.first().cloned();
        let last = edges.last().cloned();
        let (next, prev) = match bound {
            // Halaman pertama / maju: next bila masih ada; prev bila dicapai lewat kursor.
            None => (has_more.then_some(last).flatten(), None),
            Some(Bound::After) => (has_more.then_some(last).flatten(), first),
            // Mundur: prev bila masih ada baris lebih awal; next selalu (kita
            // datang dari sana).
            Some(Bound::Before) => (last, has_more.then_some(first).flatten()),
        };
        Ok(Page { items, next, prev })
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

    /// **Turunan transitif** dari `root` mengikuti relasi **self-ref** `rel` (RFC-0032):
    /// mis. `manager` menunjuk atasan → descendants = semua bawahan (rekursif). Wajib
    /// `.max_depth(n)` sebelum `.load(...)`.
    pub fn descendants_of(self, root: Entity, rel: Field<T, RelRef<T>>) -> Recursive<'a> {
        Recursive {
            store: self.store,
            table: T::TABLE,
            rel_column: rel.column,
            root,
            dir: RecurDir::Descendants,
        }
    }

    /// **Leluhur transitif** dari `start` mengikuti relasi self-ref `rel` (rantai
    /// `rel` ke atas). Wajib `.max_depth(n)`.
    pub fn ancestors_of(self, start: Entity, rel: Field<T, RelRef<T>>) -> Recursive<'a> {
        Recursive {
            store: self.store,
            table: T::TABLE,
            rel_column: rel.column,
            root: start,
            dir: RecurDir::Ancestors,
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
        let root_table = quote_ident(self.hops[0].from_table);

        // Susun semua SQL (pinjam-baca) sebelum menyentuh store.
        let root_sql = renumber(&format!(
            "SELECT pid FROM {root_table} WHERE {root_filter} ORDER BY pid"
        ));
        // Query id entity yang cocok di level sebelumnya (level 0 = root).
        let mut matched_prev = format!("SELECT pid FROM {root_table} WHERE {root_filter}");
        let mut level_loads: Vec<String> = Vec::new();
        for hop in &self.hops {
            let targets = format!(
                "SELECT DISTINCT {rel} AS pid FROM {from} \
                 WHERE pid IN ({matched_prev}) AND {rel} IS NOT NULL",
                rel = quote_ident(hop.rel_column),
                from = quote_ident(hop.from_table),
            );
            level_loads.push(renumber(&targets));
            matched_prev = targets; // level ini jadi "sebelumnya" utk hop berikut
        }

        let store = self.store;
        // Muat aditif (jembatan tak di-reset) — lihat `Query::load_pids`.
        // Muat **terdalam-dulu** (RFC-0034 Am.3): tiap entity me-resolve relasi ke
        // hop berikutnya yang sudah ter-materialize di `entity_of`; root dimuat
        // terakhir agar `leader`/dst me-resolve. `n` = jumlah entity **root**.
        for sql in level_loads.into_iter().rev() {
            store
                .load_by_query(sql, self.leaf_params.clone(), world, None)
                .await?;
        }
        let n = store
            .load_by_query(root_sql, self.leaf_params.clone(), world, None)
            .await?;
        Ok(n.len())
    }
}

/// Arah traversal rekursif self-ref (RFC-0032).
enum RecurDir {
    Descendants,
    Ancestors,
}

/// Query rekursif self-ref (RFC-0032) sedang dibangun. **Wajib** `.max_depth(n)`
/// sebelum memuat (guard siklus dienforce di tingkat tipe: tanpa `max_depth`, tak
/// ada `load`).
pub struct Recursive<'a> {
    store: &'a mut PgStore,
    table: &'static str,
    rel_column: &'static str,
    root: Entity,
    dir: RecurDir,
}

impl<'a> Recursive<'a> {
    /// Batas kedalaman rekursi (WAJIB) → siap `.load(...)`. Mencegah hang dari
    /// siklus (relasi tanpa FK bisa menyimpang).
    pub fn max_depth(self, depth: u32) -> RecursiveLoad<'a> {
        // Seed = `pid` root (RFC-0034 Am.3); root harus ada di working-set. Hitung
        // sebelum memindah `self.store` ke struct.
        let root_pid = self.store.pid_of(self.root).unwrap_or(0);
        let sql = renumber(&recursive_sql(self.table, self.rel_column, self.dir));
        RecursiveLoad {
            store: self.store,
            sql,
            params: vec![
                (PgType::BigInt, PgValue::Int(root_pid)),
                (PgType::BigInt, PgValue::Int(i64::from(depth))),
            ],
        }
    }
}

/// Query rekursif siap dimuat (RFC-0032).
pub struct RecursiveLoad<'a> {
    store: &'a mut PgStore,
    sql: String,
    params: Vec<(PgType, PgValue)>,
}

impl<'a> RecursiveLoad<'a> {
    /// Jalankan CTE `WITH RECURSIVE` & materialisasi entity hasil ke `world`.
    /// Mengembalikan jumlah entity dimuat.
    pub async fn load(self, world: &mut World) -> Result<usize, sqlx::Error> {
        // Muat aditif: root sudah ada di working-set (seed pid diambil dari
        // jembatan) dan di-refresh di tempat, bukan digandakan.
        Ok(self
            .store
            .load_by_query(self.sql, self.params, world, None)
            .await?
            .len())
    }
}

/// SQL `WITH RECURSIVE` untuk descendants/ancestors. Placeholder `?`: root id, max_depth.
fn recursive_sql(table: &str, rel: &str, dir: RecurDir) -> String {
    let table = quote_ident(table);
    let rel = quote_ident(rel);
    match dir {
        RecurDir::Descendants => format!(
            "WITH RECURSIVE rec AS (\
               SELECT pid, 0 AS depth FROM {table} WHERE {rel} = ? \
               UNION ALL \
               SELECT t.pid, rec.depth + 1 FROM {table} t \
               JOIN rec ON t.{rel} = rec.pid WHERE rec.depth < ?) \
             SELECT DISTINCT pid FROM rec"
        ),
        RecurDir::Ancestors => format!(
            "WITH RECURSIVE rec AS (\
               SELECT {rel} AS pid, 0 AS depth FROM {table} \
               WHERE pid = ? AND {rel} IS NOT NULL \
               UNION ALL \
               SELECT t.{rel}, rec.depth + 1 FROM {table} t \
               JOIN rec ON t.pid = rec.pid \
               WHERE t.{rel} IS NOT NULL AND rec.depth < ?) \
             SELECT DISTINCT pid FROM rec"
        ),
    }
}

/// Kondisi join antar-entity (RFC-0031) sebagai sub-query (menghindari alias):
/// `<rel> IN (SELECT entity_id FROM <tbl> WHERE <filter>)`.
/// Kondisi lintas komponen: `pid [NOT] IN (SELECT pid FROM <table> [WHERE f])`.
/// `pid` di tabel komponen tak pernah NULL (PK/FK), jadi `NOT IN` aman.
fn archetype_cond(table: &str, filter_sql: Option<&str>, negate: bool) -> String {
    let not = if negate { "NOT " } else { "" };
    match filter_sql {
        Some(f) => format!(
            "pid {not}IN (SELECT pid FROM {} WHERE {f})",
            quote_ident(table)
        ),
        None => format!("pid {not}IN (SELECT pid FROM {})", quote_ident(table)),
    }
}

fn join_cond(rel_column: &str, related_table: &str, filter_sql: &str) -> String {
    format!(
        "{} IN (SELECT pid FROM {} WHERE {filter_sql})",
        quote_ident(rel_column),
        quote_ident(related_table)
    )
}

/// SQL `COUNT(*)` atas `table` dengan `WHERE` opsional (placeholder `?`, belum
/// dinomori). Dipisah dari [`Query::count`] agar dapat diuji tanpa DB.
fn count_sql(table: &str, where_sql: Option<&str>) -> String {
    match where_sql {
        Some(w) => format!("SELECT COUNT(*) AS n FROM {table} WHERE {w}"),
        None => format!("SELECT COUNT(*) AS n FROM {table}"),
    }
}

/// SQL `EXPLAIN` (format teks, tanpa eksekusi) atas `table` dengan `WHERE`
/// opsional — baris pertama memuat `rows=<estimasi>` simpul akar.
fn explain_sql(table: &str, where_sql: Option<&str>) -> String {
    match where_sql {
        Some(w) => format!("EXPLAIN SELECT pid FROM {table} WHERE {w}"),
        None => format!("EXPLAIN SELECT pid FROM {table}"),
    }
}

/// Ambil `rows=<n>` dari satu baris teks `EXPLAIN`
/// (`Seq Scan on t  (cost=0.00..35.50 rows=850 width=8)`).
fn parse_explain_rows(line: &str) -> Option<u64> {
    let i = line.find("rows=")? + "rows=".len();
    let digits: String = line[i..].chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// SQL `EXISTS` atas `table` dengan `WHERE` opsional (placeholder `?`, belum
/// dinomori). Dipisah dari [`Query::exists`] agar dapat diuji tanpa DB.
fn exists_sql(table: &str, where_sql: Option<&str>) -> String {
    match where_sql {
        Some(w) => format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE {w}) AS e"),
        None => format!("SELECT EXISTS(SELECT 1 FROM {table}) AS e"),
    }
}

/// Jalankan SQL `SELECT COUNT(*) AS n …` pada executor `ex` (pool atau koneksi tx).
async fn fetch_count<'e, E>(
    ex: E,
    sql: &str,
    params: &[(PgType, PgValue)],
) -> Result<i64, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let mut q = sqlx::query(sql);
    for (ty, val) in params {
        q = crate::store::bind_value(q, *ty, val);
    }
    q.fetch_one(ex).await?.try_get("n")
}

/// Jalankan `sql` ter-parameterisasi pada `ex`, kembalikan semua baris mentah
/// (dipakai agregasi).
pub(crate) async fn fetch_scalar_rows<'e, E>(
    ex: E,
    sql: &str,
    params: &[(PgType, PgValue)],
) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let mut q = sqlx::query(sql);
    for (ty, val) in params {
        q = crate::store::bind_value(q, *ty, val);
    }
    q.fetch_all(ex).await
}

/// Jalankan SQL `SELECT EXISTS(…) AS e` pada executor `ex`.
async fn fetch_exists<'e, E>(
    ex: E,
    sql: &str,
    params: &[(PgType, PgValue)],
) -> Result<bool, sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let mut q = sqlx::query(sql);
    for (ty, val) in params {
        q = crate::store::bind_value(q, *ty, val);
    }
    q.fetch_one(ex).await?.try_get("e")
}

/// Ubah placeholder `?` berurutan menjadi `$1..$n` (dialek Postgres). `?` di
/// dalam identifier ter-quote (`"a?b"`) atau literal string (`'?'`) dibiarkan —
/// nama tabel/kolom kustom boleh memuat karakter apa pun, dan operator JSONB
/// `?`/`?|` (bila kelak ditambahkan) harus ditulis di luar kutip.
pub(crate) fn renumber(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len() + 8);
    let mut n = 1u32;
    let mut quote: Option<char> = None;
    for ch in sql.chars() {
        match quote {
            Some(q) => {
                if ch == q {
                    quote = None;
                }
                out.push(ch);
            }
            None => match ch {
                '"' | '\'' => {
                    quote = Some(ch);
                    out.push(ch);
                }
                '?' => {
                    out.push('$');
                    out.push_str(&n.to_string());
                    n += 1;
                }
                _ => out.push(ch),
            },
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ColumnDef;

    #[test]
    fn kursor_round_trip_dan_aman_url() {
        let c = Cursor {
            keys: vec![
                PgValue::Int(-7),
                PgValue::Float(1.5),
                PgValue::Bool(true),
                PgValue::Text("a?b&c=d ü".into()),
                PgValue::Numeric("123456789012345678901".into()),
            ],
            pid: 42,
        };
        let tok = c.encode();
        assert!(
            tok.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        );
        assert_eq!(Cursor::decode(&tok).unwrap(), c);
        assert_eq!(tok.parse::<Cursor>().unwrap(), c);
        assert_eq!(Cursor::decode("bukan-kursor"), Err(CursorError::Malformed));
        assert_eq!(Cursor::decode(""), Err(CursorError::Malformed));
        // Skema versi lain ditolak.
        let v2 = base64url_encode(br#"{"v":2,"k":[],"p":1}"#);
        assert_eq!(Cursor::decode(&v2), Err(CursorError::Malformed));
    }

    #[test]
    fn base64url_semua_panjang_sisa() {
        for n in 0..10 {
            let data: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
            let enc = base64url_encode(&data);
            assert!(!enc.contains('='));
            assert_eq!(base64url_decode(&enc).unwrap(), data, "n={n}");
        }
        assert_eq!(base64url_decode("A"), None, "sisa 1 karakter tak valid");
        assert_eq!(base64url_decode("A*"), None, "karakter di luar alfabet");
    }

    fn key(column: &'static str, ty: PgType, dir: Dir) -> OrderKey {
        OrderKey::column(column, ty, dir)
    }

    #[test]
    fn keyset_arah_seragam_pakai_row_value() {
        let keys = keyset_keys(&[key("hp", PgType::Integer, Dir::Desc)]);
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[1].column, "pid");
        assert_eq!(keys[1].dir, Dir::Desc, "tiebreak ikut arah kunci terakhir");
        let cur = Cursor {
            keys: vec![PgValue::Int(20)],
            pid: 9,
        };
        let (sql, params) = keyset_where(&keys, &cur, Bound::After).unwrap();
        assert_eq!(sql, "(hp, pid) < (?, ?)");
        assert_eq!(
            params,
            vec![
                (PgType::Integer, PgValue::Int(20)),
                (PgType::BigInt, PgValue::Int(9))
            ]
        );
        let (sql, _) = keyset_where(&keys, &cur, Bound::Before).unwrap();
        assert_eq!(sql, "(hp, pid) > (?, ?)");
        assert_eq!(order_by_sql(&keys, false).0, "hp DESC, pid DESC");
        assert_eq!(order_by_sql(&keys, true).0, "hp ASC, pid ASC");
    }

    #[test]
    fn keyset_arah_campur_pakai_or_expanded() {
        let keys = keyset_keys(&[
            key("hp", PgType::Integer, Dir::Desc),
            key("name", PgType::Text, Dir::Asc),
        ]);
        let cur = Cursor {
            keys: vec![PgValue::Int(5), PgValue::Text("m".into())],
            pid: 3,
        };
        let (sql, params) = keyset_where(&keys, &cur, Bound::After).unwrap();
        assert_eq!(
            sql,
            "(hp < ?) OR (hp = ? AND name > ?) OR (hp = ? AND name = ? AND pid > ?)"
        );
        assert_eq!(params.len(), 6, "param diduplikasi per cabang");
        assert_eq!(params[5], (PgType::BigInt, PgValue::Int(3)));
        // NUMERIC dapat cast pada placeholder.
        let keys = keyset_keys(&[key("amt", PgType::Numeric, Dir::Asc)]);
        let cur = Cursor {
            keys: vec![PgValue::Numeric("1.5".into())],
            pid: 1,
        };
        let (sql, _) = keyset_where(&keys, &cur, Bound::After).unwrap();
        assert_eq!(sql, "(amt, pid) > (?::numeric, ?)");
    }

    #[test]
    fn keyset_menolak_kursor_tak_cocok() {
        let keys = keyset_keys(&[key("hp", PgType::Integer, Dir::Asc)]);
        let salah_jumlah = Cursor {
            keys: vec![],
            pid: 1,
        };
        assert!(matches!(
            keyset_where(&keys, &salah_jumlah, Bound::After),
            Err(CursorError::Mismatch(_))
        ));
        let salah_tipe = Cursor {
            keys: vec![PgValue::Text("x".into())],
            pid: 1,
        };
        assert!(matches!(
            keyset_where(&keys, &salah_tipe, Bound::After),
            Err(CursorError::Mismatch(_))
        ));
        let null = Cursor {
            keys: vec![PgValue::Null],
            pid: 1,
        };
        assert!(matches!(
            keyset_where(&keys, &null, Bound::After),
            Err(CursorError::Mismatch(_))
        ));
        let keys = keyset_keys(&[key("extra", PgType::Jsonb, Dir::Asc)]);
        let json = Cursor {
            keys: vec![PgValue::Json("{}".into())],
            pid: 1,
        };
        assert!(matches!(
            keyset_where(&keys, &json, Bound::After),
            Err(CursorError::Mismatch(_))
        ));
    }

    #[test]
    fn fts_search_dan_rank_sql() {
        // `name` ber-`#[pg(fts = "english")]` → config dari FTS; ekspresi
        // identik dengan indeks GIN yang dibuat `migrate`.
        let f = Health::name().search("running shoe");
        assert_eq!(
            f.sql,
            "to_tsvector('english', name) @@ websearch_to_tsquery('english', ?)"
        );
        assert_eq!(
            f.params,
            vec![(PgType::Text, PgValue::Text("running shoe".into()))]
        );
        assert_eq!(
            tsvector_expr("english", "name"),
            "to_tsvector('english', name)"
        );
        // Kolom tanpa atribut → `simple`.
        assert_eq!(fts_config::<Health>("other"), "simple");

        // Kunci rank berekspresi: keyset row-value membawa param ekspresi
        // sebelum nilai kursor (urutan tekstual).
        let rank = OrderKey {
            column: "__rank0",
            ty: PgType::Real,
            dir: Dir::Desc,
            expr: Some((
                "ts_rank(to_tsvector('english', name), websearch_to_tsquery('english', ?))"
                    .to_string(),
                vec![(PgType::Text, PgValue::Text("shoe".into()))],
            )),
        };
        let keys = keyset_keys(&[rank]);
        let cur = Cursor {
            keys: vec![PgValue::Float(0.5)],
            pid: 7,
        };
        let (sql, params) = keyset_where(&keys, &cur, Bound::After).unwrap();
        assert_eq!(
            sql,
            "(ts_rank(to_tsvector('english', name), websearch_to_tsquery('english', ?)), pid) < (?, ?)"
        );
        assert_eq!(
            params,
            vec![
                (PgType::Text, PgValue::Text("shoe".into())),
                (PgType::Real, PgValue::Float(0.5)),
                (PgType::BigInt, PgValue::Int(7)),
            ]
        );
        let (order, oparams) = order_by_sql(&keys, false);
        assert!(order.starts_with("ts_rank(") && order.ends_with(" DESC, pid DESC"));
        assert_eq!(oparams.len(), 1);
    }

    #[test]
    fn archetype_cond_sql() {
        assert_eq!(
            archetype_cond("cmp_note", None, false),
            "pid IN (SELECT pid FROM cmp_note)"
        );
        assert_eq!(
            archetype_cond("cmp_note", None, true),
            "pid NOT IN (SELECT pid FROM cmp_note)"
        );
        assert_eq!(
            archetype_cond("cmp_customer", Some("tier = ?"), false),
            "pid IN (SELECT pid FROM cmp_customer WHERE tier = ?)"
        );
        // Kata kunci sebagai nama tabel di-quote.
        assert_eq!(
            archetype_cond("order", None, false),
            "pid IN (SELECT pid FROM \"order\")"
        );
    }

    #[test]
    fn explain_rows_terparse() {
        assert_eq!(
            parse_explain_rows("Seq Scan on cmp_health  (cost=0.00..35.50 rows=850 width=8)"),
            Some(850)
        );
        assert_eq!(
            parse_explain_rows("Index Only Scan using x on t  (cost=0.29..8.31 rows=1 width=8)"),
            Some(1)
        );
        assert_eq!(
            parse_explain_rows("Result  (cost=0.00..0.01 width=8)"),
            None
        );
        assert_eq!(
            explain_sql("cmp_health", Some("hp < ?")),
            "EXPLAIN SELECT pid FROM cmp_health WHERE hp < ?"
        );
    }

    #[test]
    fn renumber_mengabaikan_tanda_tanya_di_dalam_kutip() {
        assert_eq!(
            renumber(r#"SELECT pid FROM "t?b" WHERE "c?" = ? AND x = '?' AND y = ?"#),
            r#"SELECT pid FROM "t?b" WHERE "c?" = $1 AND x = '?' AND y = $2"#
        );
    }

    // Komponen uji manual (tanpa derive) — cukup untuk menguji generasi SQL.
    struct Health {
        _hp: i32,
    }
    impl PgComponent for Health {
        const TABLE: &'static str = "cmp_health";
        const FTS: &'static [crate::FtsDef] = &[crate::FtsDef {
            column: "name",
            config: "english",
        }];
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
        // Field array (non-skalar → JSONB via derive).
        fn tags() -> Field<Self, Vec<i64>> {
            Field::new("tags", PgType::Jsonb)
        }
        fn extra() -> Field<Self, Option<Vec<String>>> {
            Field::new("extra", PgType::Jsonb)
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
        let mut sql = format!("SELECT pid FROM {}", Health::TABLE);
        if let Some(f) = &filter {
            sql.push_str(" WHERE ");
            sql.push_str(&f.sql);
            params.extend(f.params.iter().cloned());
        }
        if order.is_empty() {
            sql.push_str(" ORDER BY pid");
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
        assert_eq!(sql, "SELECT pid FROM cmp_health WHERE hp < $1 ORDER BY pid");
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
            "SELECT pid FROM cmp_health WHERE ((hp >= $1) AND (hp < $2)) OR (NOT (hp = $3)) ORDER BY pid"
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
            "SELECT pid FROM cmp_health WHERE hp < $1 ORDER BY hp DESC LIMIT $2 OFFSET $3"
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
        assert_eq!(sql, "SELECT pid FROM cmp_health WHERE 1 = 0 ORDER BY pid");
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
            "SELECT pid FROM cmp_health WHERE (hp IN ($1, $2, $3)) AND (name LIKE $4) ORDER BY pid"
        );
        assert_eq!(params.len(), 4);
        assert_eq!(params[3], (PgType::Text, PgValue::Text("a%".to_string())));
    }

    #[test]
    fn contains_jsonb_containment() {
        // `Vec<V>` (JSONB) → `col @> ?::jsonb` dengan nilai array JSON; berlaku
        // pula untuk `Option<Vec<V>>` (NULL @> … → NULL → baris tak cocok).
        let (sql, params) = built(
            Some(
                Health::tags()
                    .contains(7)
                    .and(Health::extra().contains("x".to_string())),
            ),
            vec![],
            None,
            None,
        );
        assert_eq!(
            sql,
            "SELECT pid FROM cmp_health WHERE (tags @> $1::jsonb) AND (extra @> $2::jsonb) ORDER BY pid"
        );
        assert_eq!(
            params,
            vec![
                (PgType::Jsonb, PgValue::Json("[7]".to_string())),
                (PgType::Jsonb, PgValue::Json("[\"x\"]".to_string())),
            ]
        );
    }

    #[test]
    fn contains_all_semua_elemen() {
        let (sql, params) = built(
            Some(Health::tags().contains_all([1, 2, 3])),
            vec![],
            None,
            None,
        );
        assert_eq!(
            sql,
            "SELECT pid FROM cmp_health WHERE tags @> $1::jsonb ORDER BY pid"
        );
        assert_eq!(
            params,
            vec![(PgType::Jsonb, PgValue::Json("[1,2,3]".to_string()))]
        );
    }

    #[test]
    fn count_mengabaikan_order_limit_offset() {
        // COUNT(*) memakai WHERE yang sama, tanpa ORDER BY/LIMIT/OFFSET.
        let f = Health::hp().lt(20);
        assert_eq!(
            renumber(&count_sql(Health::TABLE, Some(&f.sql))),
            "SELECT COUNT(*) AS n FROM cmp_health WHERE hp < $1"
        );
        assert_eq!(
            count_sql(Health::TABLE, None),
            "SELECT COUNT(*) AS n FROM cmp_health"
        );
    }

    #[test]
    fn in_where_semi_join_by_value() {
        // Tanpa relasi Entity: `hp IN (SELECT hp FROM cmp_health WHERE name LIKE ?)`.
        // Kolom kanan bertipe sama (V) — dicek compiler.
        let f = Health::hp().in_where(Health::hp(), Health::name().like("a%"));
        let (sql, params) = built(Some(f), vec![], None, None);
        assert_eq!(
            sql,
            "SELECT pid FROM cmp_health WHERE hp IN (SELECT hp FROM cmp_health WHERE name LIKE $1) ORDER BY pid"
        );
        assert_eq!(
            params,
            vec![(PgType::Text, PgValue::Text("a%".to_string()))]
        );
    }

    #[test]
    fn exists_sql_bentuk() {
        let f = Health::hp().lt(20);
        assert_eq!(
            renumber(&exists_sql(Health::TABLE, Some(&f.sql))),
            "SELECT EXISTS(SELECT 1 FROM cmp_health WHERE hp < $1) AS e"
        );
        assert_eq!(
            exists_sql(Health::TABLE, None),
            "SELECT EXISTS(SELECT 1 FROM cmp_health) AS e"
        );
    }

    #[test]
    fn filter_clone_dipakai_ulang_untuk_count_dan_load() {
        let f = Health::hp().lt(20).and(Health::tags().contains(1));
        let g = f.clone();
        assert_eq!(f.sql, g.sql);
        assert_eq!(f.params, g.params);
    }

    #[test]
    fn join_subquery_terparameterisasi() {
        // Filter dasar pada T + join antar-entity → sub-query; placeholder `?`
        // dinomori **global** ($1, $2, …) melintasi filter dasar & sub-query.
        let base = Health::hp().gte(5); // "hp >= ?"
        let jc = join_cond("of_id", "cmp_health", &Health::hp().lt(20).sql);
        let where_sql = format!("{} AND {}", base.sql, jc);
        let sql = renumber(&format!(
            "SELECT pid FROM cmp_owner WHERE {where_sql} ORDER BY pid"
        ));
        assert_eq!(
            sql,
            "SELECT pid FROM cmp_owner WHERE hp >= $1 AND of_id IN \
             (SELECT pid FROM cmp_health WHERE hp < $2) ORDER BY pid"
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
            "SELECT pid FROM cmp_health WHERE boss_id IN \
             (SELECT pid FROM cmp_health WHERE boss_id IN \
             (SELECT pid FROM cmp_health WHERE hp < $1)) ORDER BY pid"
        );
        assert_eq!(params, vec![(PgType::Integer, PgValue::Int(20))]);
    }

    #[test]
    fn recursive_sql_descendants_dan_ancestors() {
        let d = renumber(&recursive_sql(
            "cmp_emp",
            "manager_id",
            RecurDir::Descendants,
        ));
        assert_eq!(
            d,
            "WITH RECURSIVE rec AS (SELECT pid, 0 AS depth FROM cmp_emp \
             WHERE manager_id = $1 UNION ALL SELECT t.pid, rec.depth + 1 \
             FROM cmp_emp t JOIN rec ON t.manager_id = rec.pid WHERE rec.depth < $2) \
             SELECT DISTINCT pid FROM rec"
        );
        let a = renumber(&recursive_sql("cmp_emp", "manager_id", RecurDir::Ancestors));
        assert!(a.contains("SELECT manager_id AS pid"));
        assert!(a.contains("t.manager_id IS NOT NULL AND rec.depth < $2"));
    }
}

/// Perilaku saat baris yang diminta sedang dikunci transaksi lain.
#[derive(Clone, Copy)]
enum LockWait {
    /// Tunggu hingga kunci lepas (bawaan Postgres).
    Wait,
    /// Lewati baris terkunci (`SKIP LOCKED`).
    SkipLocked,
    /// Gagal seketika, SQLSTATE `55P03` (`NOWAIT`).
    NoWait,
}

/// Query `T` yang mengunci baris hasilnya (`FOR UPDATE OF cmp_t`, RFC-0040).
/// Dibuat lewat [`Query::for_update`]; hanya punya terminal ber-transaksi.
///
/// Tanpa transaksi tidak dapat dijalankan:
///
/// ```compile_fail
/// # use arke_postgres::{PgComponent, PgStore};
/// #[derive(PgComponent)]
/// struct Job { n: i32 }
/// async fn f(store: &mut PgStore, w: &mut arke::World) {
///     store.query::<Job>().for_update().load(w).await;
/// }
/// ```
pub struct Locked<'a, T: PgComponent> {
    query: Query<'a, T>,
    wait: LockWait,
}

impl<T: PgComponent> Locked<'_, T> {
    /// `SKIP LOCKED`: baris yang dikunci transaksi lain dilewati — pola antrean
    /// kerja (klaim satu job tanpa menunggu worker lain).
    pub fn skip_locked(mut self) -> Self {
        self.wait = LockWait::SkipLocked;
        self
    }

    /// `NOWAIT`: gagal seketika (SQLSTATE `55P03`) bila ada baris terkunci.
    pub fn nowait(mut self) -> Self {
        self.wait = LockWait::NoWait;
        self
    }

    /// Kunci baris yang cocok di transaksi `tx` dan kembalikan `pid`-nya (urut
    /// `ORDER BY` query). Kunci bertahan hingga `tx` commit/rollback.
    pub async fn pids_in(self, tx: &mut PgTx<'_>) -> Result<Vec<i64>, sqlx::Error> {
        let (mut sql, params, reversed) = self
            .query
            .build_with(self.query.limit, false)
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
        sql.push_str(&format!(" FOR UPDATE OF {}", quote_ident(T::TABLE)));
        match self.wait {
            LockWait::Wait => {}
            LockWait::SkipLocked => sql.push_str(" SKIP LOCKED"),
            LockWait::NoWait => sql.push_str(" NOWAIT"),
        }
        let rows = fetch_scalar_rows(tx.conn(), &sql, &params).await?;
        let mut pids = rows
            .iter()
            .map(|r| r.try_get::<i64, _>("pid"))
            .collect::<Result<Vec<_>, _>>()?;
        if reversed {
            pids.reverse();
        }
        Ok(pids)
    }
}
