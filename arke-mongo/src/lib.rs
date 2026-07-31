//! Adapter MongoDB untuk ECS [`arke`](https://docs.rs/arke): persistensi
//! **satu dokumen per entity** yang menjadikan MongoDB **sumber kebenaran**
//! (RFC-0035).
//!
//! Seluruh komponen sebuah entity hidup sebagai sub-dokumen di bawah field
//! `cmp` pada koleksi `arke_entities`; identitas persisten (`pid`) adalah
//! `ObjectId` (RFC-0034 — indeks World tetap ephemeral).
//!
//! Core `arke` tetap **0-dependensi** (STD-0003); crate adapter inilah gerbang
//! dependensi driver.

/// Re-ekspor `bson` milik driver, agar pengguna tak perlu menambah dependensi
/// `bson` sendiri (dan tak bisa salah-versi).
pub use mongodb::bson;

use mongodb::bson::oid::ObjectId;

/// Identitas persisten sebuah entity di MongoDB (RFC-0034/RFC-0035 §3).
///
/// `ObjectId` dialokasikan **klien**, jadi `create` cukup satu round-trip dan
/// aman untuk multi-replica tanpa titik kontensi. Berbeda dari `pid` `i64`
/// milik `arke-postgres` — keduanya sumber kebenaran alternatif, bukan dua muka
/// dari satu penyimpanan.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Pid(pub ObjectId);

impl Pid {
    /// Mengalokasikan `pid` baru (klien-side).
    pub fn new() -> Self {
        Pid(ObjectId::new())
    }
}

impl Default for Pid {
    fn default() -> Self {
        Self::new()
    }
}

mod error;
pub use error::MongoError;

mod bson_map;
pub use bson_map::{bson_to_value, validate_names, value_to_bson};

mod registry;
pub use registry::Registry;

/// Arah urutan sebuah indeks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    /// Menaik (`1`).
    Asc,
    /// Menurun (`-1`).
    Desc,
}

impl Dir {
    /// Nilai arah sebagaimana dipakai spesifikasi indeks MongoDB.
    pub fn as_i32(self) -> i32 {
        match self {
            Dir::Asc => 1,
            Dir::Desc => -1,
        }
    }
}

/// Deklarasi satu indeks atas sebuah field komponen.
///
/// Indeks dibuat pada path bersarang `cmp.<NAME>.<field>`, sehingga bersifat
/// sparse secara alami: entity tanpa komponen itu tak masuk indeks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexDef {
    /// Nama field di dalam komponen.
    pub field: &'static str,
    /// Arah urutan.
    pub dir: Dir,
    /// Apakah indeks `unique`.
    pub unique: bool,
}

impl IndexDef {
    /// Indeks menaik atas `field`.
    pub const fn asc(field: &'static str) -> Self {
        Self {
            field,
            dir: Dir::Asc,
            unique: false,
        }
    }
    /// Indeks menurun atas `field`.
    pub const fn desc(field: &'static str) -> Self {
        Self {
            field,
            dir: Dir::Desc,
            unique: false,
        }
    }
    /// Menandai indeks ini `unique`.
    pub const fn unique(mut self) -> Self {
        self.unique = true;
        self
    }
}

/// Komponen yang dipersist ke MongoDB (RFC-0035 §2).
///
/// Tak ada derive: `#[derive(arke::Serialize)]` sudah menghasilkan pohon
/// [`arke::Value`] yang dibutuhkan BSON. Trait ini hanya membawa metadata yang
/// tak diketahui `Serialize`. Gunakan [`mongo_component!`] untuk mengisinya.
pub trait MongoComponent: arke::Serialize {
    /// Kunci komponen di bawah `cmp` (mis. `"position"`). Wajib eksplisit —
    /// sanitasi `type_name` tak bisa dilakukan di konteks `const`.
    const NAME: &'static str;
    /// Indeks atas path `cmp.<NAME>.<field>`; kosong bila tak ada.
    const INDEXES: &'static [IndexDef] = &[];
}

/// Mengimplementasikan [`MongoComponent`] untuk sebuah tipe.
///
/// ```
/// # use arke_mongo::{IndexDef, mongo_component};
/// #[derive(arke::Serialize)]
/// struct Position { x: f32, y: f32 }
/// mongo_component!(Position => "position");
///
/// #[derive(arke::Serialize)]
/// struct Health { hp: i64 }
/// mongo_component!(Health => "health", indexes: [IndexDef::asc("hp")]);
/// ```
#[macro_export]
macro_rules! mongo_component {
    ($ty:ty => $name:literal) => {
        impl $crate::MongoComponent for $ty {
            const NAME: &'static str = $name;
        }
    };
    ($ty:ty => $name:literal, indexes: [$($idx:expr),* $(,)?]) => {
        impl $crate::MongoComponent for $ty {
            const NAME: &'static str = $name;
            const INDEXES: &'static [$crate::IndexDef] = &[$($idx),*];
        }
    };
}
