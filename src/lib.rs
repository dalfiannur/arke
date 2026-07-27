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
//! | Modul | Tanggung jawab |
//! | --- | --- |
//! | [`entity`] | `Entity` sebagai generational index |
//! | [`component`] | Tipe komponen & identitasnya (registrasi otomatis) |
//! | [`world`] | Otoritas atas entity/komponen + `spawn`/`insert`/`get`/`remove`/query |
//! | [`schedule`] | `System` + `Schedule` deterministik (M-2) |
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
pub mod schedule;
pub mod world;

mod archetype;
mod storage;

pub use component::{Component, ComponentId};
pub use entity::Entity;
pub use schedule::{Schedule, System};
pub use world::World;
