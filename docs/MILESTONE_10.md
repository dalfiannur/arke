# Milestone 10 — Derive: enum & atribut field

> Disalin dari [MILESTONE_TEMPLATE.md](MILESTONE_TEMPLATE.md). Lihat [RFC-0011](RFC/RFC-0011-derive-enum-attributes.md) / [ADR-0011](ADR/ADR-0011-derive-enum-attributes.md).

## Tujuan

Memperluas `#[derive(Serialize)]` untuk enum dan atribut field (`skip`/`rename`), tetap ditulis tangan tanpa dependensi crates.io.

## Ruang lingkup

**Termasuk:**

- Enum externally-tagged: unit → `Text`, tuple → `Map{nama:List}`, struct → `Map{nama:Map}`.
- Atribut field pada struct field-bernama: `#[serialize(rename = "...")]`, `#[serialize(skip)]` (pakai `Default` saat deserialisasi).
- `compile_error!` untuk atribut/bentuk tak didukung.

**Tidak termasuk (sengaja ditunda):**

- Atribut pada field tuple struct / varian enum.
- `rename_all`, `default` eksplisit, atribut level-tipe.
- Generic.

## Artefak yang dihasilkan

| Artefak | Bentuk |
| --- | --- |
| RFC-0011 / ADR-0011 | Proposal & keputusan |
| Kode + tes | Perluasan `arke-derive` + integration test |

## Kriteria selesai (Definition of Done)

- [x] Derive enum: varian unit, tuple, dan struct — round-trip teruji (enum campuran `Shape`).
- [x] Nama varian tak dikenal saat `from_value` → `None` (teruji).
- [x] `#[serialize(rename = "k")]` mengubah kunci `Map`; round-trip teruji.
- [x] `#[serialize(skip)]` menghilangkan field dari output & memakai `Default` saat masuk; round-trip teruji.
- [x] `arke-derive` tetap **0 dependensi crates.io**; core tetap tanpa `unsafe`.
- [x] RFC-0011 & ADR-0011 ditulis serta konsisten dengan kode.
- [x] Semua tes hijau (54 tes).

## Ketergantungan

- **Butuh selesai lebih dulu:** M-8 (derive struct).
- **Membuka jalan bagi:** `rename_all`, atribut level-tipe.

## Pertanyaan terbuka

- Atribut untuk tuple/varian enum; `rename_all` → lanjutan.
