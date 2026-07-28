# Arke

[![crates.io](https://img.shields.io/crates/v/arke.svg)](https://crates.io/crates/arke)
[![docs.rs](https://img.shields.io/docsrs/arke)](https://docs.rs/arke)
[![CI](https://github.com/dalfiannur/arke/actions/workflows/ci.yml/badge.svg)](https://github.com/dalfiannur/arke/actions/workflows/ci.yml)
[![license](https://img.shields.io/crates/l/arke.svg)](LICENSE-MIT)

ECS (Entity-Component-System) **standalone, deterministik, berbasis archetype** untuk Rust.

Dua pendirian yang membedakannya:

- **Jalur ergonomis adalah jalur cepat.** API aman ter-*compile* ke jalur panas optimal — kamu tidak perlu `unsafe` untuk mendapat performa.
- **Determinisme by construction.** Hasil yang sama setiap kali, apa pun jumlah thread atau penjadwalan.

> Status: **0.4.0** — fondasi inti (M-1…M-13): entity/komponen archetype, query
> tuple generik (arity & mutabilitas campuran) + filter `With`/`Without`,
> scheduler deterministik, iterasi data-parallel, sistem berbasis-tipe,
> **resources**, snapshot/serialisasi berversi + `#[derive(Serialize)]` (enum,
> `skip`/`rename`/`rename_all`), dan error berkonteks. **Baru di 0.4.0** (M-16…M-19):
> **query cache** inkremental ([M-16](docs/MILESTONE_16.md)), **eksekutor
> graf-ketergantungan** ([M-17](docs/MILESTONE_17.md)) menggantikan barrier stage,
> **command buffer** untuk mutasi struktural tertunda ([M-18](docs/MILESTONE_18.md)),
> dan **`Entity` sebagai term query** ([M-19](docs/MILESTONE_19.md)) — memungkinkan
> pola *despawn-self*. **0 `unsafe`, 0 dependensi eksternal** (bahkan derive-nya).
> **Jalur pengguna bebas `unsafe`** (STD-0004); `unsafe` internal **terkurung &
> diverifikasi miri** di CI (menopang paralelisme tingkat-sistem, hasil identik
> serial). Persistensi Postgres tersedia sebagai adapter terpisah
> [`arke-postgres`](arke-postgres/). Butuh **Rust 1.86+**. API masih dapat berubah
> sebelum 1.0.

## Contoh

```rust
use arke::{Schedule, System, World};

#[derive(Debug, PartialEq)]
struct Position(i32, i32);
#[derive(Debug, PartialEq)]
struct Velocity(i32, i32);

let mut world = World::new();
let e = world.spawn();
world.insert(e, Position(0, 0));
world.insert(e, Velocity(1, 2));

// Sistem berbasis-tipe: akses (baca Velocity, tulis Position) disimpulkan dari tipe.
let mut schedule = Schedule::new();
schedule.add(System::each::<(&Velocity, &mut Position)>(|(v, p)| {
    p.0 += v.0;
    p.1 += v.1;
}));
schedule.run(&mut world);

assert_eq!(world.get::<Position>(e), Some(&Position(1, 2)));

// Iterasi data-parallel yang aman (hasil = serial).
world.par_for_each::<Position>(|p| p.0 *= 10);
```

### Snapshot berversi (dengan `derive`)

```rust
use arke::{Serialize, World};

#[derive(Serialize, PartialEq, Debug)]
struct Health(u32);

let mut world = World::new();
world.register_serializable::<Health>();
let e = world.spawn();
world.insert(e, Health(100));

let json = world.snapshot().to_json(); // {"schema_version":1, ...}
let mut restored = World::new();
restored.register_serializable::<Health>();
restored.load_snapshot(&arke::Snapshot::from_json(&json).unwrap());
assert_eq!(restored.get::<Health>(e), Some(&Health(100)));
```

> `#[derive(Serialize)]` ditulis tangan dengan `proc_macro` bawaan — **tetap 0 dependensi crates.io.**

## Ekosistem

Integrasi eksternal hidup di **crate adapter terpisah** agar core `arke` tetap
**0 dependensi** (STD-0003):

| Crate | Versi | Isi |
| --- | --- | --- |
| [`arke-postgres`](arke-postgres/) | [![crates.io](https://img.shields.io/crates/v/arke-postgres.svg)](https://crates.io/crates/arke-postgres) | Persistensi PostgreSQL — Postgres sebagai **sumber kebenaran** relasional berkolom-tipe. Tulis: `save` / `save_incremental` (diff) / `update_entity` (optimistic-lock). Baca: `load` / `load_where::<T>` (query-scoped). Skema: `migrate` (reconciling) + `#[pg(index/unique/check)]`. Tipe: skalar / `Option` / `JSONB` / `NUMERIC`. Lihat [RFC-0021](docs/RFC/RFC-0021-arke-postgres-adapter.md). |

## Dokumentasi & tata-kelola

Proyek ini *documentation-first*. Arah dan keputusannya hidup di [`docs/`](docs/):

| Dokumen | Isi |
| --- | --- |
| [MANIFESTO](docs/MANIFESTO.md) | Identitas & keyakinan inti |
| [VISION](docs/VISION.md) | Masa depan yang dituju |
| [PHILOSOPHY](docs/PHILOSOPHY.md) | Prinsip pengambilan keputusan |
| [ARCHITECTURE_BIBLE](docs/ARCHITECTURE_BIBLE.md) | Invarian arsitektur (sumber kebenaran) |
| [STANDARDS](docs/STANDARDS.md) | Aturan yang dapat diuji mesin (STD-xxxx) |
| [RFC](docs/RFC/) · [ADR](docs/ADR/) | Proposal & keputusan arsitektur |
| [MILESTONE_1](docs/MILESTONE_1.md) | Lingkup kerja pertama: core storage & query |

Arsitektur inti diputuskan di [RFC-0002](docs/RFC/RFC-0002-core-storage-architecture.md) / [ADR-0002](docs/ADR/ADR-0002-core-storage-architecture.md).

## Pengembangan

```bash
cargo test          # jalankan tes
cargo clippy        # lint
cargo fmt           # format
```

## Lisensi

MIT — lihat [LICENSE-MIT](LICENSE-MIT).
