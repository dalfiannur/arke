# Rust ECS

ECS (Entity-Component-System) **standalone, deterministik, berbasis archetype** untuk Rust.

Dua pendirian yang membedakannya:

- **Jalur ergonomis adalah jalur cepat.** API aman ter-*compile* ke jalur panas optimal — kamu tidak perlu `unsafe` untuk mendapat performa.
- **Determinisme by construction.** Hasil yang sama setiap kali, apa pun jumlah thread atau penjadwalan.

> Status: **pra-M1** — fondasi sedang dibangun. Belum siap dipakai.

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
