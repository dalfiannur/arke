# Milestone 11 — Derive: rename_all & atribut level-tipe

> Disalin dari [MILESTONE_TEMPLATE.md](MILESTONE_TEMPLATE.md). Lihat [RFC-0012](RFC/RFC-0012-derive-rename-all.md) / [ADR-0012](ADR/ADR-0012-derive-rename-all.md).

## Tujuan

Menambahkan atribut level-tipe `#[serialize(rename_all = "...")]` untuk menyesuaikan konvensi penamaan seluruh field/varian sekaligus, tetap ditulis tangan tanpa dependensi crates.io.

## Ruang lingkup

**Termasuk:**

- `#[serialize(rename_all = "...")]` pada struct & enum.
- Konvensi: lowercase, UPPERCASE, snake_case, SCREAMING_SNAKE_CASE, kebab-case, SCREAMING-KEBAB-CASE, camelCase, PascalCase (konverter tulis-tangan).
- Diterapkan ke kunci field & nama varian; `rename` per-field/varian menang; `skip` tetap.
- `#[serialize(rename = "...")]` pada varian enum.

**Tidak termasuk (sengaja ditunda):**

- `rename_all` terpisah field-vs-varian; atribut `default` eksplisit.

## Artefak yang dihasilkan

| Artefak | Bentuk |
| --- | --- |
| RFC-0012 / ADR-0012 | Proposal & keputusan |
| Kode + tes | Perluasan `arke-derive` + integration test |

## Kriteria selesai (Definition of Done)

- [x] `rename_all` menerapkan konvensi (camelCase, snake_case, SCREAMING_SNAKE_CASE, kebab-case) pada kunci field & nama varian — teruji round-trip.
- [x] `rename` per-field/varian menang atas `rename_all` — teruji.
- [x] Nilai `rename_all` tak dikenal → `compile_error!` (via `parse_case` → Err).
- [x] `arke-derive` tetap **0 dependensi crates.io**; core tetap tanpa `unsafe`.
- [x] RFC-0012 & ADR-0012 ditulis serta konsisten dengan kode.
- [x] Semua tes hijau (57 tes).

## Ketergantungan

- **Butuh selesai lebih dulu:** M-10 (derive enum & atribut field).

## Pertanyaan terbuka

- `rename_all` field-vs-varian terpisah; `default` eksplisit → lanjutan.
