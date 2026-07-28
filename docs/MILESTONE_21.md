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

- [ ] `spawn_bundle((A, B))` membuat entity ber-A,B (nilai benar) — teruji.
- [ ] `insert_bundle` pada entity ber-komponen menambah bundle (satu archetype tujuan) — teruji.
- [ ] Hasil **identik** dengan `insert(A); insert(B)` (archetype & query sama) — teruji.
- [ ] Komponen duplikat/sudah-ada → panic menyebut komponen — teruji (should_panic).
- [ ] Arity beragam (1–8) berfungsi — teruji.
- [ ] Determinisme (id terurut) & tanpa `unsafe` baru.
- [ ] RFC-0022 & ADR-0022 konsisten dengan kode.
- [ ] Semua tes + miri hijau.

## Ketergantungan

- **Butuh selesai lebih dulu:** M-1 (archetype storage, `insert`).
- **Membuka jalan bagi:** bundle di command buffer; `remove_bundle`.

## Pertanyaan terbuka

- Bundle di `CommandBuffer`; overwrite; `remove_bundle` → lanjutan.
