//! # Arke
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
//! | Modul | Tanggung jawab |
//! | --- | --- |
//! | [`entity`] | `Entity` sebagai generational index |
//! | [`component`] | Tipe komponen & identitasnya (registrasi otomatis) |
//! | [`world`] | Otoritas atas entity/komponen/resource + query + `par_for_each` |
//! | [`query`] | `QueryData` tuple generik + filter `With`/`Without` + `Access` (M-4/12/13); `QueryState` cache inkremental (M-16) |
//! | [`schedule`] | `System` + `Schedule` (M-2); `each` bertipe (M-4); resources (M-9); `run_parallel` (M-15) |
//! | [`serialize`] | `Value` + trait `Serialize` + JSON tulis-tangan (M-6) |
//! | [`snapshot`] | `Snapshot` World berversi, round-trip setia (M-6) |
//! | [`error`] | `EcsError` berkonteks yang menyebut komponen (M-7) |
//! | `storage` (privat) | Kolom kontigu bertipe (`TypedColumn`) |
//! | `archetype` (privat) | Tabel per-kombinasi-komponen |
//!
//! Query M-1 hadir sebagai method pada [`World`] ([`World::query`],
//! [`World::query_mut`], [`World::query_pair`]). Seluruh implementasi M-1
//! **bebas `unsafe`**: pemisahan kolom disjoint memakai `split_at_mut`. Trait
//! `QueryData` generik atas tuple sembarang-arity direncanakan untuk milestone
//! berikutnya.

pub mod component;
pub mod entity;
pub mod error;
pub mod query;
pub mod schedule;
pub mod serialize;
pub mod snapshot;
pub mod world;

mod archetype;
mod storage;

pub use component::{Component, ComponentId};
pub use entity::Entity;
pub use error::EcsError;
pub use query::{Access, QueryData, QueryFilter, QueryState, With, Without};
pub use schedule::{Schedule, System};
pub use serialize::{Serialize, Value};
pub use snapshot::Snapshot;
pub use world::World;

/// Derive macro `#[derive(Serialize)]` (RFC-0009). Berbagi nama dengan trait
/// [`Serialize`]; `use arke::Serialize;` membawa keduanya.
pub use arke_derive::Serialize;
