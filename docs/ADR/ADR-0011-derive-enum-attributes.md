# ADR-0011: `derive(Serialize)` untuk enum + atribut field

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-28
- **RFC terkait:** [RFC-0011](../RFC/RFC-0011-derive-enum-attributes.md)

## Konteks

`#[derive(Serialize)]` (M-8) hanya mendukung struct. Enum umum untuk state komponen, dan atribut field diperlukan untuk melewatkan field atau mengganti kunci. Perluasan ini harus tetap tanpa dependensi eksternal (`proc_macro` bawaan).

## Keputusan

Kami memilih:

1. **Enum externally-tagged**: unit → `Text(nama)`; tuple → `Map{nama: List}`; struct → `Map{nama: Map}`.
2. **Atribut field** pada struct field-bernama: `#[serialize(rename = "kunci")]` dan `#[serialize(skip)]` (field ber-`skip` memakai `Default::default()` saat deserialisasi).
3. Bentuk/atribut tak didukung memancarkan `compile_error!`.
4. Semua tetap ditulis tangan (`proc_macro` bawaan), 0 dependensi crates.io.

## Konsekuensi

**Positif:**

- Enum & kontrol field membuat derive berguna untuk lebih banyak tipe.
- Format enum eksplisit & seragam (nama varian selalu hadir).
- Tetap 0 dependensi eksternal, tanpa `unsafe`.

**Negatif / biaya:**

- Parser `TokenStream` tulis-tangan bertambah kompleks (varian enum, atribut).
- `skip` menuntut field `Default`.
- Atribut belum berlaku untuk tuple struct & field varian enum.

**Netral / catatan:**

- Format enum adalah v1; perubahannya butuh versi baru (RFC).
- `rename_all`, `default` eksplisit, atribut level-tipe ditunda.

## Alternatif yang ditolak

- **Enum internally/adjacently-tagged** — kurang seragam / lebih verbose.
- **`skip` tanpa `Default`** — tak bisa merekonstruksi.
- **Pustaka atribut (syn)** — dependensi eksternal.

Rincian pertimbangan ada di [RFC-0011](../RFC/RFC-0011-derive-enum-attributes.md).
