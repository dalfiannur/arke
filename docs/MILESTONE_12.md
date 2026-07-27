# Milestone 12 — Generic Tuple Queries

> Disalin dari [MILESTONE_TEMPLATE.md](MILESTONE_TEMPLATE.md). Lihat [RFC-0013](RFC/RFC-0013-generic-tuple-queries.md) / [ADR-0013](ADR/ADR-0013-generic-tuple-queries.md).

## Tujuan

Menggeneralisasi `QueryData` ke tuple sembarang-arity dengan mutabilitas campuran (`(&mut A, &mut B)`, arity 3+), aman via `get_disjoint_mut`, menggantikan impl konkret M-4.

## Ruang lingkup

**Termasuk:**

- Trait `QueryTerm` untuk `&T`/`&mut T`.
- Impl `QueryData` generik untuk tuple arity 2–8 (makro), mutabilitas campuran.
- `Archetype::columns_disjoint_mut` (via `get_disjoint_mut`).
- Penolakan alias berkonteks (`EcsError::QueryConflict`).
- Menghapus impl konkret `(&A, &B)`/`(&A, &mut B)`.

**Tidak termasuk (sengaja ditunda):**

- Filter `With<T>`/`Without<T>` (M-13).
- `Entity` sebagai term query; arity > 8.

## Artefak yang dihasilkan

| Artefak | Bentuk |
| --- | --- |
| RFC-0013 / ADR-0013 | Proposal & keputusan |
| Kode + tes | `QueryTerm`, makro tuple, `columns_disjoint_mut` + tes |

## Kriteria selesai (Definition of Done)

- [x] `(&mut A, &mut B)` mengiterasi & memutasi kedua komponen — teruji.
- [x] Tuple arity 3 & 4 (baca & mutabilitas campuran) — teruji (`tests/queries.rs`).
- [x] Hanya entity yang memiliki **semua** komponen tuple yang diiterasi — teruji.
- [x] Alias (`(&mut A, &mut A)`) → panik menyebut komponen (STD-0008) — teruji.
- [x] Bekerja lewat `System::each::<Q>` untuk tuple arity 3 & 4 — teruji.
- [x] Tetap **tanpa `unsafe`** (`get_disjoint_mut`); 0 dependensi eksternal. *(MSRV dinaikkan ke 1.86 untuk `get_disjoint_mut`.)*
- [x] RFC-0013 & ADR-0013 ditulis serta konsisten dengan kode.
- [x] Semua tes hijau (61 tes).

## Ketergantungan

- **Butuh selesai lebih dulu:** M-4 (QueryData).
- **Membuka jalan bagi:** filter query (M-13).

## Pertanyaan terbuka

- Filter `With`/`Without`; `Entity`-as-term → lanjutan.
