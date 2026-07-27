# ADR-0015: Penyimpanan kolom berbasis `UnsafeCell`

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-28
- **RFC terkait:** [RFC-0015](../RFC/RFC-0015-unsafecell-column-storage.md)

## Konteks

RFC-0006 menetapkan bahwa paralelisme tingkat-sistem yang sound menuntut pembentukan `&mut` ke data kolom dari `&World` bersama — hanya sah lewat `UnsafeCell`. Prasyarat miri-di-CI (ADR-0006) kini terpenuhi. Ini menggarap prasyarat penyimpanan, memperkenalkan `unsafe` internal pertama proyek.

## Keputusan

Kami memilih:

1. `TypedColumn<T>(UnsafeCell<Vec<T>>)`.
2. Antarmuka akses dengan `unsafe` **dikurung di modul `storage`**: `data_mut` (aman, `&mut self`), `unsafe data` (baca-bersama), `unsafe data_mut_shared` (tulis lewat `&self`, untuk jalur paralel).
3. Lint `unsafe_code` diubah dari `deny` global menjadi izin **terkurung** via `#[allow(unsafe_code)]` hanya di modul yang membutuhkannya; sisanya tetap `deny`. STD-0004 (jalur pengguna tanpa `unsafe`) tetap.
4. **miri di CI wajib hijau** untuk setiap `unsafe`.
5. Refactor **mempertahankan perilaku** (65 tes tetap hijau); fitur paralel adalah milestone berikutnya.

## Konsekuensi

**Positif:**

- Membuka jalan eksekutor paralel tingkat-sistem yang sound.
- `unsafe` terkurung & diverifikasi miri, bukan sekadar diklaim.
- STD-0004 tetap utuh (jalur pengguna aman).

**Negatif / biaya:**

- Jalur baca (`query`, `get`) kini mengandung `unsafe` internal.
- Cerita "0 `unsafe`" menjadi "`unsafe` terkurung, miri-verified" — perubahan identitas.
- Pengembangan `unsafe` bergantung verifikasi miri di CI (lokal tanpa nightly).

**Netral / catatan:**

- Pembungkus `Sync` & eksekutor paralel adalah M-15.
- Menggraduasi RN-0001.

## Alternatif yang ditolak

- **Raw pointer di eksekutor saja** — navigasi tanpa membentuk `&World` sangat rapuh.
- **Tetap serial** — tak memenuhi tujuan Phase 2.

Rincian pertimbangan ada di [RFC-0015](../RFC/RFC-0015-unsafecell-column-storage.md).
