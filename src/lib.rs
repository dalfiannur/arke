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
//! ## Contoh
//!
//! ```
//! use arke::{Schedule, System, World};
//!
//! #[derive(Debug, PartialEq)]
//! struct Position(i32, i32);
//! #[derive(Debug, PartialEq)]
//! struct Velocity(i32, i32);
//!
//! let mut world = World::new();
//! // `spawn_bundle`: pasang beberapa komponen dalam satu pindah archetype.
//! let e = world.spawn_bundle((Position(0, 0), Velocity(1, 2)));
//!
//! // Sistem berbasis-tipe: akses (baca Velocity, tulis Position) disimpulkan
//! // dari tipe query — tanpa deklarasi manual, tanpa `unsafe`.
//! let mut schedule = Schedule::new();
//! schedule.add(System::each::<(&Velocity, &mut Position)>(|(v, p)| {
//!     p.0 += v.0;
//!     p.1 += v.1;
//! }));
//! schedule.run(&mut world);
//!
//! assert_eq!(world.get::<Position>(e), Some(&Position(1, 2)));
//! ```
//!
//! Contoh onboarding bertahap ada di direktori `examples/`.
//!
//! ## Peta modul (Milestone M-1)
//!
//! | Modul | Tanggung jawab |
//! | --- | --- |
//! | [`entity`] | `Entity` sebagai generational index |
//! | [`component`] | Tipe komponen & identitasnya (registrasi otomatis) |
//! | [`world`] | Otoritas atas entity/komponen/resource + query + `par_for_each` |
//! | [`query`] | `QueryData` tuple generik + filter `With`/`Without` + `Access` (M-4/12/13); `QueryState` cache inkremental (M-16); term `Entity` (M-19) |
//! | [`schedule`] | `System` + `Schedule` (M-2); `each` bertipe (M-4); resources (M-9); `run_parallel` (M-15); `each_cmd` (M-18) |
//! | [`command`] | `CommandBuffer` — mutasi struktural tertunda (M-18) |
//! | [`bundle`] | `Bundle` — sisip tuple komponen dalam satu pindah archetype (M-21) |
//! | [`serialize`] | `Value` + trait `Serialize` + JSON tulis-tangan (M-6) |
//! | [`snapshot`] | `Snapshot` World berversi, round-trip setia (M-6) |
//! | [`error`] | `EcsError` berkonteks yang menyebut komponen (M-7) |
//! | `storage` (privat) | Kolom kontigu bertipe (`TypedColumn`) |
//! | `archetype` (privat) | Tabel per-kombinasi-komponen |
//!
//! Query tersedia dua jalur: method langsung pada [`World`] ([`World::query`],
//! [`World::query_mut`], [`World::query_pair`]) untuk kasus sederhana, dan
//! [`QueryData`] generik atas tuple sembarang-arity (baca/tulis campuran) untuk
//! sistem. Iterasi dijalankan berkolom (RFC-0023); `unsafe`-nya **terkurung** di
//! [`query`] & [`world`], semuanya diverifikasi miri. Jalur pengguna tetap bebas
//! `unsafe` (STD-0004).

pub mod bundle;
pub mod command;
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

/// Penanda *sealed* untuk trait ekstensi (RFC-0026). Modul ini privat-crate,
/// jadi crate lain tak dapat menamai — apalagi mengimplementasikan — penanda di
/// dalamnya. Trait publik yang memakainya sebagai supertrait karenanya hanya
/// dapat diimpl oleh `arke`.
pub(crate) mod sealed {
    /// Penanda: hanya `arke` yang mengimpl [`crate::Bundle`].
    pub trait BundleSealed {}
    /// Penanda: hanya `arke` yang mengimpl [`crate::QueryData`].
    pub trait QueryDataSealed {}
    /// Penanda: hanya `arke` yang mengimpl [`crate::query::QueryTerm`].
    pub trait QueryTermSealed {}
    /// Penanda: hanya `arke` yang mengimpl [`crate::QueryFilter`].
    pub trait QueryFilterSealed {}
}

pub use bundle::Bundle;
pub use command::{CommandBuffer, EntityCommands};
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
