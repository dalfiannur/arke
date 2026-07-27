# RFC-0015: Penyimpanan kolom berbasis `UnsafeCell`

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-28
- **Milestone:** M-14 (UnsafeCell Storage)
- **ADR terkait:** [ADR-0015](../ADR/ADR-0015-unsafecell-column-storage.md)
- **Menggraduasi:** [RN-0001](../RN/RN-0001-unsafecell-column-storage.md)

## Ringkasan

Mengubah penyimpanan kolom `TypedColumn<T>(Vec<T>)` menjadi `TypedColumn<T>(UnsafeCell<Vec<T>>)`, memungkinkan pembentukan `&mut [T]` interior dari referensi `World` **bersama** — prasyarat eksekutor paralel tingkat-sistem (RFC-0006). Ini memperkenalkan **`unsafe` internal pertama** proyek, **dikurung di modul `storage`** dan diverifikasi **miri** di CI. Refactor ini **mempertahankan perilaku**: seluruh 65 tes tetap hijau; belum ada fitur paralel baru (itu milestone berikutnya).

## Motivasi

RFC-0006 menyimpulkan bahwa paralelisme tingkat-sistem yang sound menuntut kemampuan membentuk `&mut` ke data kolom dari `&World` bersama, yang di Rust **hanya sah lewat `UnsafeCell`**. Prasyarat #2 (miri di CI) telah terpenuhi. RFC ini menggarap prasyarat #1.

## Usulan rinci

### 1. Perubahan penyimpanan

```rust
pub(crate) struct TypedColumn<T>(UnsafeCell<Vec<T>>);
```

### 2. Antarmuka akses (unsafe dikurung di `storage`)

```rust
impl<T> TypedColumn<T> {
    fn data_mut(&mut self) -> &mut Vec<T>;              // AMAN (via UnsafeCell::get_mut)
    /// # Safety: tak ada akses `&mut` lain ke data ini yang aktif.
    unsafe fn data(&self) -> &Vec<T>;                    // baca-bersama
    /// # Safety: pemanggil menjamin akses eksklusif ke data ini.
    unsafe fn data_mut_shared(&self) -> &mut Vec<T>;     // tulis lewat &self (jalur paralel)
}
```

- Jalur ber-`&mut self` (insert/remove/move/`query_mut`) memakai `data_mut` — **tetap aman**.
- Jalur baca ber-`&self` (`query`, `get`) memakai `unsafe { data() }` — sound dalam eksekusi serial (tak ada penulis konkuren).
- `data_mut_shared` disiapkan untuk eksekutor paralel (M-15); belum dipakai di M-14.

### 3. Invarian keamanan (didokumentasikan per-fungsi)

Setiap blok `unsafe` menyertakan argumen: dalam eksekusi **serial**, hanya satu akses aktif pada satu waktu → pembentukan `&[T]`/`&mut [T]` dari sel tak beralias. Dalam eksekusi **paralel** (M-15), penjadwal menjamin akses disjoint.

### 4. Kebijakan lint

Lint crate diubah dari `unsafe_code = "deny"` menjadi izin **terkurung**: `#[allow(unsafe_code)]` hanya pada modul `storage` (dan kelak eksekutor). Kode pengguna & modul lain tetap `deny` → STD-0004 (jalur pengguna tanpa `unsafe`) **utuh**.

### 5. Verifikasi

Job `miri` CI (`cargo miri test -p arke --lib`) wajib hijau. Setiap `unsafe` yang lolos harus mempertahankannya.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| Raw pointer di eksekutor saja (tanpa UnsafeCell) | Jalur baca tetap aman | Navigasi raw tanpa membentuk `&World` sangat rapuh | UnsafeCell adalah pola sound baku (mudah diverifikasi miri) |
| Tetap serial (tanpa storage berubah) | Nol unsafe | Tak ada paralelisme tingkat-sistem | Tujuan Phase 2 |

## Dampak

- **Kompatibilitas / migrasi:** perilaku identik; API publik tak berubah. Perubahan internal.
- **Keamanan:** memperkenalkan `unsafe` **terkurung di `storage`**, diverifikasi miri. STD-0004 tetap.
- **Identitas:** cerita "0 `unsafe`" menjadi "`unsafe` terkurung, miri-verified"; STD-0004 (jalur pengguna) tetap utuh.

## Pertanyaan terbuka

- Pembungkus `Sync` untuk berbagi `&World` lintas-thread → M-15 (eksekutor paralel).

## Keputusan

Diterima. Lihat [ADR-0015](../ADR/ADR-0015-unsafecell-column-storage.md).
