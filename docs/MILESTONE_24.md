# Milestone 24 — Deprecate `query_pair`/`query_pair_ref` (menuju 1.0)

> Langkah bentuk-API kedua menuju 1.0 (RN-0004 §2). Menandai API query khusus
> arity-2 sebagai usang, mengarahkan ke jalur `QueryData` generik. Non-breaking
> (peringatan) di 0.6.0; dihapus di 1.0.

## Tujuan

Mengimplementasikan [RFC-0027](RFC/RFC-0027-deprecate-query-pair.md): `#[deprecated]`
pada `World::query_pair` & `query_pair_ref`, migrasi pemakai internal ke jalur
generik, diverifikasi doc-test `compile_fail` + `#![deny(deprecated)]`.

## Ruang lingkup

**Termasuk:**

- `#[deprecated(since = "0.6.0", note = …)]` pada kedua method (implementasi tetap).
- Migrasi pemakai internal (contoh `no_unsafe`, doc crate) ke `<(...)>::each`.
- `#[allow(deprecated)]` pada uji perilaku yang masih menguji method.
- Doc-test `compile_fail` (RED→GREEN) membuktikan atribut aktif.

**Tidak termasuk:**

- Menghapus method (itu di 1.0).
- Deprecate `query`/`query_mut` (dipertahankan).

## Definition of Done

- [ ] Kedua method `#[deprecated]`; doc-test `compile_fail` + `deny(deprecated)` lolos.
- [ ] RED disaksikan: `compile_fail` gagal sebelum atribut ditambah.
- [ ] Nol peringatan di seluruh workspace (CI `-D warnings` hijau); miri hijau.
- [ ] Perilaku tak berubah (method deprecated tetap berfungsi; uji existing hijau).
