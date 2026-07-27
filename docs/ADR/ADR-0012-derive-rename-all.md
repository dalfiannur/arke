# ADR-0012: `derive(Serialize)` — `rename_all` & atribut level-tipe

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-28
- **RFC terkait:** [RFC-0012](../RFC/RFC-0012-derive-rename-all.md)

## Konteks

Snapshot sering harus cocok dengan konvensi penamaan eksternal. Menandai tiap field dengan `rename` (M-10) melelahkan. Konversi case yang robust biasanya pakai pustaka eksternal — yang melanggar janji 0-dependensi.

## Keputusan

Kami memilih:

1. Atribut level-tipe **`#[serialize(rename_all = "...")]`** yang menerapkan konvensi pada semua kunci field & nama varian.
2. Mendukung set konvensi standar (lowercase, UPPERCASE, snake_case, SCREAMING_SNAKE_CASE, kebab-case, SCREAMING-KEBAB-CASE, camelCase, PascalCase) dengan **konversi ditulis tangan** (split kata + rakit), 0-dep.
3. **`rename` per-field/varian menang** atas `rename_all`; `skip` tetap berlaku.
4. Menambahkan `#[serialize(rename = "...")]` pada varian enum.

## Konsekuensi

**Positif:**

- Penyesuaian konvensi penamaan ringkas (satu atribut untuk seluruh tipe).
- Tetap 0 dependensi crates.io, tanpa `unsafe`.

**Negatif / biaya:**

- Konverter case tulis-tangan menambah kode (split kata + 8 mode).
- `rename_all` terpisah field-vs-varian belum ada.

**Netral / catatan:**

- Nilai `rename_all` tak dikenal → `compile_error!`.
- Format hasil adalah bagian snapshot berversi.

## Alternatif yang ditolak

- **Hanya `rename` per-field** — melelahkan.
- **Pustaka case eksternal** — dependensi eksternal.

Rincian pertimbangan ada di [RFC-0012](../RFC/RFC-0012-derive-rename-all.md).
