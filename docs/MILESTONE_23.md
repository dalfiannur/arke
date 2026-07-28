# Milestone 23 — Seal trait ekstensi (menuju 1.0)

> Langkah bentuk-API pertama menuju 1.0 (RN-0004 §1). Menutup trait ekstensi agar
> hanya `arke` yang mengimplementasikannya — mengeluarkan detail implementasi dari
> kontrak publik **sebelum** dibekukan. Breaking (untuk 0.6.0), tapi berdampak nol
> secara praktik.

## Tujuan

Mengimplementasikan [RFC-0026](RFC/RFC-0026-seal-extension-traits.md): seal
`Bundle`, `QueryData`, `QueryTerm`, `QueryFilter` via supertrait penanda di
`pub(crate) mod sealed`, diverifikasi doc-test `compile_fail` (0 dependensi).

## Ruang lingkup

**Termasuk:**

- `pub(crate) mod sealed` dengan empat trait penanda.
- Supertrait penanda pada tiap trait publik + impl penanda paralel untuk semua
  tipe yang di-impl crate (via makro tuple yang ada + impl skalar).
- Doc-test `compile_fail` (RED→GREEN) untuk `QueryFilter` & `QueryData` (dua trait
  yang benar-benar terbuka), memakai impl downstream **lengkap** agar RED bermakna.

**Tidak termasuk:**

- Seal `Component`/`Serialize` (sengaja tak disegel).
- Deprecate API query (RFC-0027, milestone terpisah).
- CHANGELOG/policy (RFC-0028, milestone terpisah).

## Definition of Done

- [ ] Keempat trait tak dapat diimpl di luar crate (doc-test `compile_fail` lolos).
- [ ] RED disaksikan: uji `compile_fail` gagal sebelum seal (impl downstream lengkap kompilasi).
- [ ] Semua uji existing hijau; `cargo test --doc` + miri + CI hijau.
- [ ] Tak ada perubahan perilaku/performa (murni visibilitas).
