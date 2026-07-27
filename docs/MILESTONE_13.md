# Milestone 13 — Query Filters (With / Without)

> Disalin dari [MILESTONE_TEMPLATE.md](MILESTONE_TEMPLATE.md). Lihat [RFC-0014](RFC/RFC-0014-query-filters.md) / [ADR-0014](ADR/ADR-0014-query-filters.md).

## Tujuan

Menambahkan filter `With<T>`/`Without<T>` untuk menyaring entity berdasarkan kehadiran komponen tanpa mengambil datanya. Tetap tanpa `unsafe`, 0 dependensi.

## Ruang lingkup

**Termasuk:**

- Penanda `With<T>`, `Without<T>`.
- Trait `QueryFilter` (impl untuk `With`, `Without`, `()`, tuple).
- `QueryData::each_filtered::<F>` + `each` = default `each_filtered::<()>`.
- `System::each_filtered::<Q, F>`.
- Helper archetype `contains`/`contains_all`/`contains_none`.

**Tidak termasuk (sengaja ditunda):**

- `Or<...>`, `Changed`/`Added` (deteksi perubahan); `Entity`-as-term.

## Artefak yang dihasilkan

| Artefak | Bentuk |
| --- | --- |
| RFC-0014 / ADR-0014 | Proposal & keputusan |
| Kode + tes | `QueryFilter`, `With`/`Without`, `each_filtered` + tes |

## Kriteria selesai (Definition of Done)

- [ ] `Without<T>` hanya memproses entity yang **tak** memiliki `T` — teruji.
- [ ] `With<T>` hanya memproses entity yang memiliki `T` (tanpa mengambil datanya) — teruji.
- [ ] Tuple filter `(With<A>, Without<B>)` bekerja (AND) — teruji.
- [ ] Filter tak memengaruhi konflik scheduler (tak menyumbang `Access`) — teruji.
- [ ] `each` (tanpa filter) tetap setara `each_filtered::<()>` — teruji.
- [ ] Tetap **tanpa `unsafe`**; 0 dependensi eksternal.
- [ ] RFC-0014 & ADR-0014 ditulis serta konsisten dengan kode.
- [ ] Semua tes hijau.

## Ketergantungan

- **Butuh selesai lebih dulu:** M-12 (query tuple generik).
- **Membuka jalan bagi:** deteksi perubahan (`Changed`/`Added`), `Or`.

## Pertanyaan terbuka

- `Or`, `Changed`/`Added`; `Entity`-as-term → lanjutan.
