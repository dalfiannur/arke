# Milestone 14 — UnsafeCell Column Storage

> Disalin dari [MILESTONE_TEMPLATE.md](MILESTONE_TEMPLATE.md). Lihat [RFC-0015](RFC/RFC-0015-unsafecell-column-storage.md) / [ADR-0015](ADR/ADR-0015-unsafecell-column-storage.md).

## Tujuan

Mengubah penyimpanan kolom ke `UnsafeCell` agar `&mut` interior dapat dibentuk dari `&World` bersama — prasyarat eksekutor paralel tingkat-sistem. `unsafe` dikurung di `storage`, diverifikasi miri. Perilaku tak berubah.

## Ruang lingkup

**Termasuk:**

- `TypedColumn<T>(UnsafeCell<Vec<T>>)` + antarmuka `data_mut`/`unsafe data`/`unsafe data_mut_shared`.
- Memperbarui seluruh akses kolom (storage, archetype, world) agar kompilasi & perilaku identik.
- Lint `unsafe_code`: `deny` global → izin terkurung di `storage`.
- Job miri CI hijau.

**Tidak termasuk (sengaja ditunda):**

- Pembungkus `Sync` & eksekutor paralel (M-15).

## Kriteria selesai (Definition of Done)

- [ ] `TypedColumn` memakai `UnsafeCell`; semua akses kolom terupdate.
- [ ] Perilaku identik: seluruh 65 tes tetap hijau.
- [ ] `unsafe` **hanya** di modul `storage`; modul lain & jalur pengguna tetap tanpa `unsafe` (STD-0004).
- [ ] Setiap blok `unsafe` menyertakan argumen keamanan.
- [ ] Job **miri** CI hijau.
- [ ] RFC-0015 & ADR-0015 ditulis; RN-0001 di-*graduate*.

## Ketergantungan

- **Butuh selesai lebih dulu:** gerbang miri CI (terpasang).
- **Membuka jalan bagi:** M-15 (eksekutor paralel tingkat-sistem).

## Pertanyaan terbuka

- Pembungkus `Sync` → M-15.
