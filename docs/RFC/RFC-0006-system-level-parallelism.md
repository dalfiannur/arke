# RFC-0006: Paralelisme tingkat-sistem (analisis & penundaan)

- **Status:** Accepted <!-- keputusan: TUNDA implementasi; prasyarat direkam -->
- **Tanggal:** 2026-07-27
- **Milestone:** M-5 (Deferred)
- **ADR terkait:** [ADR-0006](../ADR/ADR-0006-defer-system-parallelism.md)

> Keputusan RFC ini adalah **menunda implementasi** eksekusi paralel tingkat-sistem sampai prasyaratnya terpenuhi. Isi utamanya adalah **analisis soundness** yang menjelaskan mengapa. Ini contoh nyata prinsip documentation-first: merekam sebuah "tidak (belum)" beserta alasannya agar tak diperdebatkan ulang tanpa informasi baru.

## Ringkasan

Menjalankan sistem-sistem satu *stage* (yang aksesnya sudah dijamin disjoint oleh analisis konflik M-2 + akses tersimpul M-4) di beberapa thread akan memberi percepatan tingkat-sistem. Namun implementasi yang **sound** menuntut prasyarat yang belum ada di proyek ini. RFC ini merekam analisisnya dan **menunda** implementasi hingga: (1) penyimpanan kolom berbasis `UnsafeCell`, dan (2) verifikasi `miri` di CI. Sampai saat itu, crate tetap **0 `unsafe`**.

## Motivasi

ARCHITECTURE_BIBLE §2 mencantumkan *paralelisme yang aman* sebagai invarian, dan §3.2 menempatkan penjadwal sebagai penjalan sistem tak-konflik secara bersamaan. M-2–M-4 menyiapkan seluruh prasyarat analitis (stage tak-konflik, akses tersimpul dari tipe). Yang tersisa adalah **eksekusi** paralelnya.

## Analisis soundness

### Masalah inti

Sistem dalam satu stage memiliki akses komponen **disjoint**. Untuk menjalankannya paralel, tiap thread perlu:

- **membaca struktur** `World` yang sama (Vec archetype, metadata kolom) — bersama; dan
- **menulis data kolom** komponen miliknya — eksklusif.

`Schedule` menyimpan sistem heterogen (`System` seragam, tipe query ter-erase). Karena itu tiap thread harus mengakses `World` lewat referensi bersama, lalu membentuk `&mut [T]` ke kolomnya.

### Mengapa jalur naif adalah UB

Membentuk `&mut [T]` dari `&Vec<T>` (referensi **bersama** ke data kolom, yang diperoleh lewat `&World` bersama) adalah **undefined behavior** di bawah model aliasing Rust (Stacked/Tree Borrows): sebuah `&T` menandai lokasi *read-only*; menulis lewat pointer turunannya melanggar itu. Ini berlaku meski secara logika aksesnya eksklusif.

Catatan penting: data kolom (`Vec<T>` di balik `Box<dyn Column>`) berada di **alokasi berbeda** dari buffer `Vec<Archetype>`. Karena itu membaca struktur (yang tak dimutasi selama fase paralel) bersamaan dengan menulis data kolom di alokasi lain **tidak** saling meng-alias secara memori — masalahnya murni pada *bagaimana* `&mut` dibentuk.

### Jalur yang sound

Membentuk `&mut` dari referensi bersama hanya sah lewat **`UnsafeCell`**. Maka penyimpanan kolom harus menjadi `TypedColumn<T>(UnsafeCell<Vec<T>>)`. Konsekuensinya:

1. Jalur **baca** (`query`, `query_pair_ref`, `get`) kini butuh `unsafe { &*cell.get() }` — `unsafe` menyebar ke kode yang saat ini aman.
2. `UnsafeCell` bersifat `!Sync` → `World: !Sync` → berbagi `&World` lintas-thread butuh pembungkus dengan `unsafe impl Sync`, yang kesahihannya bergantung pada jaminan disjoint dari analisis stage.
3. Kontrak baru: **tak ada perubahan struktural** (spawn/despawn/insert/remove) selama fase paralel — kalau tidak, buffer archetype bisa berpindah saat dibaca thread lain.

### Mengapa jalur "aman-penuh" juga terbentur

`slice::get_disjoint_mut` (stabil di Rust 1.97) dapat memberi `&mut` disjoint ke banyak kolom **satu** archetype secara aman. Tetapi sebuah sistem membentang **banyak** archetype, dan `Schedule` menyimpan sistem **heterogen**. Mendistribusikan slice yang dipinjam ke pekerjaan per-sistem menuntut menyimpan `&mut [T]`/`&[T]` ber-lifetime di dalam koleksi yang **ter-erase tipe** — yang tak dapat dinyatakan tanpa `unsafe` (raw pointer) atau desain sepenuhnya monomorfik (tak ada erasure, bertentangan dengan `Schedule` heterogen). Jadi jalur aman-penuh yang generik tidak tersedia tanpa redesign besar.

## Keputusan: tunda, dengan prasyarat

Implementasi eksekusi paralel tingkat-sistem **ditunda** sampai:

1. **Penyimpanan kolom berbasis `UnsafeCell`** — redesign `storage`/`archetype` sehingga `&mut` interior dari `&World` menjadi sah. Perubahan ini sendiri layak jadi RFC tersendiri karena menyentuh seluruh jalur baca/tulis M-1.
2. **`miri` di CI** — job yang menjalankan test suite di bawah `cargo +nightly miri test`, sehingga argumen soundness `unsafe` diverifikasi otomatis, bukan sekadar diklaim. Tanpa ini, `unsafe` aliasing tak boleh masuk (kebijakan proyek).

Sampai prasyarat terpenuhi, crate mempertahankan **0 `unsafe`** dan paralelisme yang tersedia adalah **data-parallel** (`World::par_for_each`, M-3), yang sudah aman dan mengaktifkan STD-0006.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih (sekarang) |
| --- | --- | --- | --- |
| Ship `unsafe` raw-pointer sekarang | Paralelisme sistem penuh | UB halus; tak terverifikasi tanpa miri | Menaruh risiko UB di "library produksi" tak dapat diterima tanpa verifikasi |
| Redesign `UnsafeCell` + implementasi sekarang | Sound secara buku-teks | Redesign besar M-1; tetap tak terverifikasi (miri absen) | Besar & tetap tak terverifikasi di lingkungan ini |
| Paralel data per-sistem (M-3 diterapkan ke sistem) | 100% aman, pakai banyak core | Bukan paralelisme tingkat-sistem sejati | Sudah tersedia sebagai `par_for_each`; bukan yang dimaksud M-5 |
| **Tunda + dokumentasikan (dipilih)** | Jujur; crate tetap 0-unsafe; analisis terekam | Tak ada percepatan tingkat-sistem kini | Menghormati invarian & menghindari UB tak-terverifikasi |

## Dampak

- **Kompatibilitas / migrasi:** tak ada perubahan kode. Prasyarat `UnsafeCell` kelak akan menjadi perubahan internal storage (lewat RFC tersendiri) yang tak mengubah API publik.
- **Keamanan / provenance:** menegakkan kebijakan "tak ada `unsafe` aliasing tanpa verifikasi miri".
- **Konsekuensi pada invarian:** menjaga *ergonomis = cepat* (STD-0004, jalur pengguna & core tetap tanpa `unsafe`). STD-0006 tetap aktif via data-parallelism M-3; paralelisme tingkat-sistem menunggu.

## Pertanyaan terbuka

- Desain rinci penyimpanan kolom `UnsafeCell` + pembungkus `Sync` → RFC tersendiri saat prasyarat digarap.
- Perlukah command buffer (mutasi struktural tertunda) agar sistem paralel tetap bisa spawn/despawn secara aman? → milestone terkait.
- Menyiapkan toolchain nightly + `miri` di CI → prasyarat infrastruktur.

## Keputusan

Diterima (sebagai penundaan berprasyarat). Lihat [ADR-0006](../ADR/ADR-0006-defer-system-parallelism.md).
