//! Adapter PostgreSQL untuk ECS [`arke`](https://docs.rs/arke): persistensi
//! **relasional berkolom-tipe** menjadikan Postgres **sumber kebenaran**
//! (RFC-0021 / ADR-0021).
//!
//! Fase v1: skema kolom-tipe dari [`PgComponent`] (via `#[derive(PgComponent)]`),
//! pemetaan tipe Rust → SQL, dan pembangun `CREATE TABLE`. Integrasi `sqlx`
//! (`PgStore`: `connect`/`load`/`save`) menyusul.
//!
//! Core `arke` tetap **0-dependensi** (STD-0003); crate adapter inilah gerbang
//! dependensi DB.

/// Derive `#[derive(PgComponent)]` — menurunkan skema kolom-tipe dari struct.
pub use arke_postgres_derive::PgComponent;

mod store;
pub use store::{PgStore, SyncStats, UpdateError};

pub mod query;
pub use query::{Dir, EntityRef, Field, Filter, IntoPgValue, Query};

/// Tipe kolom SQL yang dipetakan dari tipe field Rust (RFC-0021 §2/§3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PgType {
    /// `INTEGER` (i8/i16/i32/u8/u16).
    Integer,
    /// `BIGINT` (i64/isize/u32).
    BigInt,
    /// `NUMERIC(20)` (u64/usize — melampaui `BIGINT`).
    Numeric,
    /// `REAL` (f32).
    Real,
    /// `DOUBLE PRECISION` (f64).
    DoublePrecision,
    /// `BOOLEAN` (bool).
    Boolean,
    /// `TEXT` (String).
    Text,
    /// `JSONB` (fallback field non-skalar).
    Jsonb,
}

impl PgType {
    /// Nama tipe SQL Postgres.
    pub fn sql(self) -> &'static str {
        match self {
            PgType::Integer => "INTEGER",
            PgType::BigInt => "BIGINT",
            PgType::Numeric => "NUMERIC(20)",
            PgType::Real => "REAL",
            PgType::DoublePrecision => "DOUBLE PRECISION",
            PgType::Boolean => "BOOLEAN",
            PgType::Text => "TEXT",
            PgType::Jsonb => "JSONB",
        }
    }
}

/// Definisi satu kolom tabel komponen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnDef {
    /// Nama kolom (dari nama field).
    pub name: &'static str,
    /// Tipe SQL kolom.
    pub ty: PgType,
    /// Apakah `NULL` diizinkan (dari `Option<T>`).
    pub nullable: bool,
    /// Referensi FK (`REFERENCES <target>`), atau `None`. Diisi untuk kolom `_id`
    /// relasi entity → `Some("arke_entities(entity_id)")` (RFC-0031).
    pub references: Option<&'static str>,
}

impl ColumnDef {
    /// Kolom skalar biasa (tanpa FK) — pembantu ringkas untuk mengurangi churn.
    pub const fn scalar(name: &'static str, ty: PgType, nullable: bool) -> Self {
        Self {
            name,
            ty,
            nullable,
            references: None,
        }
    }
}

/// Definisi indeks kustom pada sebuah kolom (dari `#[pg(index)]`/`#[pg(unique)]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexDef {
    /// Kolom yang di-index.
    pub column: &'static str,
    /// Apakah indeks `UNIQUE`.
    pub unique: bool,
}

/// Nilai kolom yang portabel (batas antara komponen ↔ driver DB).
///
/// `to_params` menghasilkannya; layer `sqlx` (nanti) mem-bind-nya ke query.
#[derive(Debug, Clone, PartialEq)]
pub enum PgValue {
    /// Bilangan bulat (INTEGER/BIGINT).
    Int(i64),
    /// Bilangan lebar sebagai desimal-teks (NUMERIC).
    Numeric(String),
    /// Titik-mengambang (REAL/DOUBLE PRECISION).
    Float(f64),
    /// Boolean.
    Bool(bool),
    /// Teks (TEXT).
    Text(String),
    /// JSON (JSONB) — teks JSON dari `arke::Serialize` (fallback field non-skalar).
    Json(String),
    /// `NULL`.
    Null,
}

/// Komponen yang dipetakan ke tabel Postgres berkolom-tipe (RFC-0021).
///
/// Diturunkan via `#[derive(PgComponent)]`; jangan diimplementasi manual kecuali
/// perlu. Round-trip `to_params` → baris DB → `from_params` setia (STD-0002).
pub trait PgComponent {
    /// Nama tabel komponen (mis. `cmp_position`).
    const TABLE: &'static str;
    /// Definisi kolom, urut sesuai field.
    const COLUMNS: &'static [ColumnDef];
    /// Indeks kustom (`#[pg(index)]`/`#[pg(unique)]`); kosong bila tak ada.
    const INDEXES: &'static [IndexDef] = &[];
    /// Ekspresi `CHECK` level-tabel (`#[pg(check = "…")]`); kosong bila tak ada.
    const CHECKS: &'static [&'static str] = &[];
    /// Nilai kolom untuk baris ini, urut sesuai [`Self::COLUMNS`].
    fn to_params(&self) -> Vec<PgValue>;
    /// Rekonstruksi dari nilai kolom (urut sesuai [`Self::COLUMNS`]); `None`
    /// bila bentuk/tipe tak cocok.
    fn from_params(values: &[PgValue]) -> Option<Self>
    where
        Self: Sized;
}

/// Membangun pernyataan `CREATE TABLE` untuk komponen `T` (pembangun `migrate`).
///
/// Kolom `entity_id` merujuk `arke_entities` dengan `ON DELETE CASCADE`.
pub fn create_table_sql<T: PgComponent>() -> String {
    create_table_sql_from(T::TABLE, T::COLUMNS)
}

/// Seperti [`create_table_sql`] tetapi dari nama tabel + kolom (dipakai runtime,
/// mis. oleh `PgStore` yang menyimpan skema type-erased).
pub fn create_table_sql_from(table: &str, columns: &[ColumnDef]) -> String {
    let mut cols = String::from(
        "entity_id BIGINT PRIMARY KEY REFERENCES arke_entities(entity_id) ON DELETE CASCADE",
    );
    for col in columns {
        cols.push_str(", ");
        cols.push_str(col.name);
        cols.push(' ');
        cols.push_str(col.ty.sql());
        if !col.nullable {
            cols.push_str(" NOT NULL");
        }
        if let Some(target) = col.references {
            // Kolom relasi `_id` → FK ke arke_entities (RFC-0031). Tanpa CASCADE:
            // dangling ditangani saat baca (generation), bukan penghapusan berantai.
            cols.push_str(" REFERENCES ");
            cols.push_str(target);
        }
    }
    format!("CREATE TABLE IF NOT EXISTS {table} ({cols})")
}
