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
