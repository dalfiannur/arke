# Milestone 21 — Component Bundles

> Disalin dari [MILESTONE_TEMPLATE.md](MILESTONE_TEMPLATE.md). Lihat [RFC-0022](RFC/RFC-0022-component-bundles.md) / [ADR-0022](ADR/ADR-0022-component-bundles.md).

## Tujuan

Menyisipkan beberapa komponen sekaligus lewat tuple (`spawn_bundle`/`insert_bundle`) dalam **satu** perpindahan archetype — lebih ringkas dan lebih cepat (tesis *ergonomis = cepat*). Hasil identik dengan `insert` berurutan.

## Ruang lingkup

**Termasuk:**

- Trait `Bundle` untuk tuple arity 1–8 dari `Component` distinct.
- `World::insert_bundle` (satu pindah archetype) + `spawn_bundle`.
- Kontrak: komponen distinct + baru → panic menyebut komponen.

**Tidak termasuk (sengaja ditunda):**

- Overwrite komponen yang sudah ada via bundle; bundle di `CommandBuffer`; `remove_bundle`.

## Artefak yang dihasilkan

| Artefak | Bentuk |
| --- | --- |
| RFC-0022 / ADR-0022 | Proposal + keputusan bundle |
| kode + tes | `Bundle`, `insert_bundle`, `spawn_bundle` |

## Kriteria selesai (Definition of Done)

- [x] `spawn_bundle((A, B))` membuat entity ber-A,B (nilai benar) — teruji (`spawn_bundle_dan_insert_bundle_setara_insert_berurutan`).
- [x] `insert_bundle` pada entity ber-komponen menambah bundle — teruji (idem, `(C,)` ke entity ber-A,B).
- [x] Hasil **identik** dengan `insert(A); insert(B)` (nilai & komponen sama) — teruji (idem, world sekuensial pembanding).
- [x] Komponen duplikat/sudah-ada → panic menyebut komponen — teruji (`insert_bundle_komponen_sudah_ada_panik`, `spawn_bundle_tipe_duplikat_panik`).
- [x] Arity beragam (1–8) berfungsi — teruji arity 1/3/5 (`bundle_arity_lima_berfungsi`); impl 1–8.
- [x] Determinisme (id terurut) & tanpa `unsafe` baru — `ids.sort_unstable()`; hanya operasi archetype aman. Contoh `no_unsafe` memakai `spawn_bundle` di bawah `forbid(unsafe_code)`.
- [x] RFC-0022 & ADR-0022 konsisten dengan kode.
- [x] Semua tes + miri hijau (miri di CI).

## Ketergantungan

- **Butuh selesai lebih dulu:** M-1 (archetype storage, `insert`).
- **Membuka jalan bagi:** bundle di command buffer; `remove_bundle`.

## Pertanyaan terbuka

- Bundle di `CommandBuffer`; overwrite; `remove_bundle` → lanjutan.
