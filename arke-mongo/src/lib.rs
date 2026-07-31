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
