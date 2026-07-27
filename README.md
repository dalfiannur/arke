# Arke

ECS (Entity-Component-System) **standalone, deterministik, berbasis archetype** untuk Rust.

Dua pendirian yang membedakannya:

- **Jalur ergonomis adalah jalur cepat.** API aman ter-*compile* ke jalur panas optimal — kamu tidak perlu `unsafe` untuk mendapat performa.
- **Determinisme by construction.** Hasil yang sama setiap kali, apa pun jumlah thread atau penjadwalan.

> Status: **0.1.0** — fondasi inti selesai (M-1…M-7): entity/komponen archetype,
> query, scheduler deterministik, iterasi data-parallel, sistem berbasis-tipe,
> snapshot/serialisasi berversi, dan error berkonteks. **0 `unsafe`, 0 dependensi
> eksternal.** Paralelisme tingkat-sistem ([M-5](docs/MILESTONE_5.md)) sengaja
> ditunda. API masih dapat berubah sebelum 1.0.

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
