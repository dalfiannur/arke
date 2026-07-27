# Milestone 7 — Contextual Errors

> Disalin dari [MILESTONE_TEMPLATE.md](MILESTONE_TEMPLATE.md). Lihat [RFC-0008](RFC/RFC-0008-contextual-errors.md) / [ADR-0008](ADR/ADR-0008-contextual-errors.md).

## Tujuan

Membuat kegagalan runtime **menyebut komponen yang terlibat**, mengaktifkan STD-0008 dan mewujudkan prinsip "pesan error mengajari" (Philosophy §3). Tanpa `unsafe`, tanpa dependensi eksternal.

## Ruang lingkup

**Termasuk:**

- Tipe `EcsError` (`Display` + `std::error::Error`) dengan varian `QueryConflict` & `ComponentNotRegistered`.
- Konflik borrow query: pesan `panic` menyebut tipe komponen (`type_name`).
- `World::try_snapshot() -> Result<Snapshot, EcsError>` yang menyebut komponen tak-terdaftar.
- `ComponentRegistry` menyimpan nama tipe tiap komponen.

**Tidak termasuk (sengaja ditunda):**

- Mengubah `insert`/`remove`/`get` menjadi `Result`.
- Menyertakan `Entity` dalam error (belum ada operasi ber-entity yang gagal).

## Artefak yang dihasilkan

| Artefak | Bentuk |
| --- | --- |
| RFC-0008 / ADR-0008 | Proposal & keputusan error berkonteks |
| Kode + tes | `EcsError`, `try_snapshot`, pesan konflik query + unit/integration test |

## Kriteria selesai (Definition of Done)

- [x] `EcsError` mengimplementasikan `Display` & `std::error::Error`; pesannya menyebut nama tipe komponen.
- [x] Konflik borrow query (`query_pair::<A, A>`) panik dengan pesan yang menyebut tipe komponen (bukti STD-0008) — disertai tes.
- [x] `try_snapshot` mengembalikan `Err(ComponentNotRegistered { component })` yang menyebut namanya bila ada komponen hidup tak-terdaftar (bukti STD-0008) — `tests/snapshot.rs`.
- [x] `try_snapshot` mengembalikan `Ok` bila semua komponen hidup terdaftar; setara `snapshot()`.
- [x] Tetap **tanpa `unsafe`** & bebas dependensi eksternal (STD-0003).
- [x] RFC-0008 & ADR-0008 ditulis serta konsisten dengan kode.
- [x] Semua tes hijau (39 tes) secara lokal.

## Ketergantungan

- **Butuh selesai lebih dulu:** M-1 (query), M-6 (snapshot).
- **Membuka jalan bagi:** —

## Pertanyaan terbuka

- `insert`/`remove` ber-`Result` untuk entity mati → RN bila jadi sumber bug.
