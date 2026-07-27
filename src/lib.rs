//! # Rust ECS
//!
//! ECS (Entity-Component-System) **standalone, deterministik, berbasis archetype**
//! untuk Rust.
//!
//! Dua invarian yang membentuk seluruh desain (lihat `docs/ARCHITECTURE_BIBLE.md` §2):
//!
//! - **Ergonomis = cepat** — API aman ter-*compile* ke jalur panas optimal; kode
//!   pengguna tidak pernah butuh `unsafe`.
//! - **Determinisme by construction** — hasil yang sama setiap kali, apa pun
//!   penjadwalannya.
//!
//! Arsitektur penyimpanan inti diputuskan di RFC-0002 / ADR-0002.
//!
//! ## Peta modul (Milestone M-1)
//!
//! | Modul | Tanggung jawab | Status |
//! | --- | --- | --- |
//! | [`entity`] | `Entity` sebagai generational index | kontrak tersedia |
//! | [`component`] | Tipe komponen & identitasnya | kontrak tersedia |
//! | [`world`] | Otoritas atas entity, komponen, resource | dalam pengerjaan |
//! | `storage` | Kolom kontigu bertipe (`unsafe` internal dikurung di sini) | menyusul |
//! | `archetype` | Tabel per-kombinasi-komponen | menyusul |
//! | `query` | Akses berpola & terverifikasi atas komponen | menyusul |
//!
//! Modul yang berstatus "menyusul" ditambahkan saat implementasi mencapainya
//! (dikembangkan secara test-first).

pub mod component;
pub mod entity;
pub mod world;

pub use component::{Component, ComponentId};
pub use entity::Entity;
pub use world::World;
