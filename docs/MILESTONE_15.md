# Milestone 15 — Parallel Executor

> Disalin dari [MILESTONE_TEMPLATE.md](MILESTONE_TEMPLATE.md). Lihat [RFC-0016](RFC/RFC-0016-parallel-executor.md) / [ADR-0016](ADR/ADR-0016-parallel-executor.md).

## Tujuan

Menjalankan sistem-sistem satu stage secara paralel (`Schedule::run_parallel`), mengaktifkan STD-0006 tingkat-sistem. `unsafe` terkurung, miri-verified.

## Ruang lingkup

**Termasuk:**

- Jalur query berbagi `QueryData::each_filtered_shared(&World)`; `each_filtered` = wrapper eksklusif.
- `TypedColumn::data_mut_shared` (unsafe) + `QueryTerm::iter_shared`.
- Runner `Exclusive`/`Shared`; sistem bertipe → `Shared`.
- `SyncWorld` (`unsafe impl Sync`) + `Schedule::run_parallel` via `std::thread::scope`.

**Tidak termasuk (sengaja ditunda):**

- Sistem resource/opaque paralel; thread pool; command buffer.

## Kriteria selesai (Definition of Done)

- [ ] Jalur query diseragamkan ke `each_filtered_shared`; perilaku serial identik (65+ tes hijau).
- [ ] `run_parallel` menjalankan stage `Shared` di beberapa thread; stage dengan `Exclusive` → serial.
- [ ] **Paralel = serial** untuk sistem disjoint — teruji (STD-0006 tingkat-sistem).
- [ ] `unsafe` terkurung di `storage`/`query`/`schedule`; jalur pengguna tetap tanpa `unsafe`.
- [ ] Job **miri** CI hijau (termasuk uji paralel).
- [ ] RFC-0016 & ADR-0016 ditulis serta konsisten dengan kode.

## Ketergantungan

- **Butuh selesai lebih dulu:** M-14 (UnsafeCell storage), gerbang miri.
- **Membuka jalan bagi:** sistem resource paralel; command buffer.

## Pertanyaan terbuka

- Resource paralel; thread pool → lanjutan.
