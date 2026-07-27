# Milestone 8 — Derive Serialize

> Disalin dari [MILESTONE_TEMPLATE.md](MILESTONE_TEMPLATE.md). Lihat [RFC-0009](RFC/RFC-0009-derive-serialize.md) / [ADR-0009](ADR/ADR-0009-derive-serialize.md).

## Tujuan

Menghilangkan boilerplate `Serialize` lewat `#[derive(Serialize)]`, tanpa mengorbankan janji "0 dependensi eksternal" — proc-macro ditulis tangan dengan `proc_macro` bawaan saja.

## Ruang lingkup

**Termasuk:**

- Impl `Serialize` untuk primitif (`i8..i64`, `u8..u64`, `usize`/`isize`, `f32`/`f64`, `bool`, `char`, `String`) + `Vec<T>` + `Option<T>`.
- Aksesor `Value` publik (`get`, `as_map`, `as_list`, `as_int`).
- Crate `arke-derive` (proc-macro, 0 dependensi crates.io) dengan `#[derive(Serialize)]`.
- Dukungan: struct field-bernama (→ `Map`), tuple struct (→ `List`), unit struct (→ `Null`).
- Workspace dua-crate; `arke` me-*re-export* derive.

**Tidak termasuk (sengaja ditunda):**

- Enum, generic, union (memancarkan `compile_error!`).
- Atribut field (`rename`, `skip`).

## Artefak yang dihasilkan

| Artefak | Bentuk |
| --- | --- |
| RFC-0009 / ADR-0009 | Proposal & keputusan derive |
| Kode + tes | Impl primitif, `arke-derive`, re-export + unit/integration test |

## Kriteria selesai (Definition of Done)

- [x] `Serialize` terimplementasi untuk primitif + `Vec`/`Option`; round-trip `to_value`/`from_value` teruji.
- [x] `#[derive(Serialize)]` bekerja untuk struct field-bernama, tuple, dan unit — round-trip teruji lewat integrasi (`tests/derive.rs`).
- [x] Bentuk tak didukung (enum/generic) memancarkan `compile_error!` yang jelas.
- [x] `arke-derive` **0 dependensi crates.io** (`cargo tree` hanya menampilkan dirinya).
- [x] Core `arke` tetap tanpa dependensi pihak-ketiga; pemeriksaan CI standalone diperbarui & hijau.
- [x] Tetap **tanpa `unsafe`**.
- [x] RFC-0009 & ADR-0009 ditulis serta konsisten dengan kode.
- [x] Semua tes hijau (45 tes).

## Ketergantungan

- **Butuh selesai lebih dulu:** M-6 (Serialize/Value).
- **Membuka jalan bagi:** derive enum/generic; atribut field.

## Pertanyaan terbuka

- Publikasi terkoordinasi: `arke-derive` lebih dulu, lalu `arke` bergantung padanya. → saat rilis versi berikutnya.
