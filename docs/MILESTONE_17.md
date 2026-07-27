# Milestone 17 — Dependency-Graph Executor

> Disalin dari [MILESTONE_TEMPLATE.md](MILESTONE_TEMPLATE.md). Lihat [RFC-0018](RFC/RFC-0018-dependency-graph-executor.md) / [ADR-0018](ADR/ADR-0018-dependency-graph-executor.md).

## Tujuan

Mengganti eksekutor paralel berbasis **stage** (barrier penuh) dengan eksekutor berbasis **graf-ketergantungan**: tiap sistem mulai segera setelah pendahulu yang berkonflik dengannya selesai, bukan menunggu seluruh stage. `run_parallel` memakainya transparan — API sama, hasil identik (STD-0006), paralelisme lebih tinggi. Tanpa `unsafe` baru.

## Ruang lingkup

**Termasuk:**

- `Schedule::dependencies() -> Vec<Vec<usize>>` (pendahulu berkonflik per-sistem, urutan registrasi).
- Eksekutor DAG (`std::thread::scope` + `Mutex<GraphState>` + `Condvar`, `Mutex<&mut System>`), untuk *run* sistem `Shared`.
- `run_parallel` di-reimplementasi: segmentasi pada batas `Exclusive` (Shared → DAG, Exclusive → serial).

**Tidak termasuk (sengaja ditunda):**

- Reduksi transitif graf; `Exclusive` sebagai simpul DAG; thread-pool persisten lintas-`run`.

## Artefak yang dihasilkan

| Artefak | Bentuk |
| --- | --- |
| RFC-0018 | Proposal eksekutor graf-ketergantungan |
| ADR-0018 | Keputusan reimplementasi `run_parallel` di atas DAG |
| kode + tes | `dependencies()`, eksekutor DAG, `run_parallel` tersegmentasi |

## Kriteria selesai (Definition of Done)

- [ ] `dependencies()` mengembalikan pendahulu berkonflik yang benar (urutan registrasi) — teruji.
- [ ] `run_parallel` (DAG) memberi hasil **identik** dengan `run` serial untuk skenario konflik campuran — teruji.
- [ ] Sistem tak-berkonflik yang dipisah barrier stage kini berjalan tanpa menunggu satu sama lain (mulai lebih awal) — teruji via struktur graf.
- [ ] Skedul dengan sistem `Exclusive` (resource) tetap benar & deterministik (segmentasi) — teruji.
- [ ] Determinisme & urutan efektif identik (STD-0005/0006).
- [ ] Tetap **tanpa `unsafe` baru**; jalur pengguna aman.
- [ ] RFC-0018 & ADR-0018 ditulis serta konsisten dengan kode.
- [ ] Semua tes + miri hijau.

## Ketergantungan

- **Butuh selesai lebih dulu:** M-15 (eksekutor paralel + `SyncWorld`), M-2/M-4 (analisis konflik).
- **Membuka jalan bagi:** command buffer (mutasi struktural saat paralel); reduksi transitif.

## Pertanyaan terbuka

- Reduksi transitif; `Exclusive` sebagai simpul; thread-pool persisten → lanjutan.
