# Milestone 4 — Type-Based Systems

> Disalin dari [MILESTONE_TEMPLATE.md](MILESTONE_TEMPLATE.md). Lihat [RFC-0005](RFC/RFC-0005-type-based-systems.md) / [ADR-0005](ADR/ADR-0005-type-based-systems.md).

## Tujuan

Mewujudkan model "System = fungsi atas Query" (ARCHITECTURE_BIBLE §3.1): sistem menyatakan kebutuhan datanya lewat **tipe** query, dan scheduler menyimpulkan akses dari tipe itu (bukan deklarasi manual). Tetap serial dan 100% aman; ini prasyarat paralelisme tingkat-sistem yang sound.

## Ruang lingkup

**Termasuk:**

- Trait `QueryData`: `Item<'w>`, `access()` (baca/tulis tersimpul dari tipe), `each(world, f)` (iterasi internal).
- Impl untuk `&T`, `&mut T`, `(&A, &B)`, `(&A, &mut B)`.
- `System::each::<Q>(f)`: membangun sistem dengan akses tersimpul; masuk `Schedule` M-2.
- `Access` dipindah ke modul `query` sebagai tipe pakai-bersama.

**Tidak termasuk (sengaja ditunda):**

- Tuple `(&mut A, &mut B)` & arity > 2 (via makro).
- Filter query (`With`/`Without`).
- Eksekusi paralel tingkat-sistem (butuh `unsafe` terkurung) — milestone berikutnya.

## Artefak yang dihasilkan

| Artefak | Bentuk |
| --- | --- |
| RFC-0005 | Proposal sistem berbasis-tipe + akses tersimpul |
| ADR-0005 | Keputusan yang diterima dari RFC-0005 |
| Kode + tes | `QueryData`, `System::each` + unit test |

## Kriteria selesai (Definition of Done)

- [x] `QueryData::access()` menyimpulkan baca/tulis yang benar untuk `&T`, `&mut T`, `(&A, &B)`, `(&A, &mut B)`.
- [x] `QueryData::each` mengiterasi hanya entity yang cocok dan menghasilkan `Item` yang benar.
- [x] `System::each::<Q>(f)` membangun sistem yang, saat dijalankan, menerapkan `f` per entity cocok.
- [x] `Schedule` menempatkan sistem berbasis-tipe ke stage yang benar dari akses tersimpul (konflik tulis → stage berbeda; pembaca berbeda → satu stage). Lihat `tests/systems.rs`.
- [x] Implementasi tetap **tanpa `unsafe`** (dikompilasi di bawah `deny(unsafe_code)`) dan bebas dependensi (STD-0003).
- [x] RFC-0005 & ADR-0005 ditulis serta konsisten dengan kode.
- [x] Semua tes hijau (32 tes) secara lokal.

## Ketergantungan

- **Butuh selesai lebih dulu:** M-1 (query), M-2 (Schedule/stage).
- **Membuka jalan bagi:** eksekusi paralel tingkat-sistem (M-5), filter query.

## Pertanyaan terbuka

- `(&mut A, &mut B)`, arity > 2, filter query → milestone berikutnya.
- Paralel tingkat-sistem via `Q::access()` + pandangan disjoint (butuh `unsafe`) → milestone tersendiri.
