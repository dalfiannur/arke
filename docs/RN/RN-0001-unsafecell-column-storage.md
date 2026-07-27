# RN-0001: Penyimpanan kolom berbasis `UnsafeCell` untuk akses interior yang sound

- **Status:** Graduated to [RFC-0015](../RFC/RFC-0015-unsafecell-column-storage.md) <!-- Open | Investigating | Graduated to RFC-XXXX | Closed -->
- **Tanggal:** 2026-07-27
- **Dipicu oleh:** [RFC-0006](../RFC/RFC-0006-system-level-parallelism.md) (paralelisme tingkat-sistem)

## Pertanyaan

Bagaimana merancang penyimpanan kolom archetype agar `&mut [T]` ke data kolom dapat dibentuk secara **sound** dari `&World` bersama — tanpa membocorkan `unsafe` ke jalur pengguna, dan dengan biaya seminimal mungkin pada jalur baca/tulis M-1 yang saat ini 100% aman?

## Konteks

RFC-0006 menyimpulkan bahwa eksekusi paralel tingkat-sistem membutuhkan kemampuan membentuk `&mut` interior dari referensi `World` bersama. Di Rust, ini hanya sah lewat `UnsafeCell`. Redesign ini adalah prasyarat #1 untuk membuka M-5, dan cukup konsekuensial (menyentuh `storage`, `archetype`, dan seluruh jalur `query`/`get`) sehingga layak diselidiki terpisah sebelum diusulkan sebagai RFC.

## Yang sudah diketahui

- Membentuk `&mut [T]` dari `&Vec<T>` bersama = UB (Stacked/Tree Borrows); `UnsafeCell` adalah satu-satunya jalur sah.
- `TypedColumn<T>(UnsafeCell<Vec<T>>)` membuat jalur **baca** (`query`, `get`, `query_pair_ref`) memerlukan `unsafe { &*cell.get() }`.
- `UnsafeCell` bersifat `!Sync` → `World: !Sync` → berbagi `&World` lintas-thread menuntut pembungkus dengan `unsafe impl Sync` yang kesahihannya bersandar pada jaminan disjoint analisis stage (M-2/M-4).
- `slice::get_disjoint_mut` stabil di Rust 1.97 — memberi `&mut` disjoint aman untuk banyak kolom **satu** archetype, tetapi tak menyelesaikan distribusi lintas-archetype/lintas-sistem yang ter-erase tipe.
- Kebijakan proyek (ADR-0006): tak ada `unsafe` aliasing tanpa verifikasi `miri`.

## Arah yang dieksplorasi

| Arah | Catatan awal |
| --- | --- |
| `UnsafeCell<Vec<T>>` per kolom | Sound; menyebarkan `unsafe` (terkurung) ke jalur baca. Perlu akuntansi cermat get vs get_mut. |
| Akses interior hanya lewat metode ber-`unsafe` terdokumentasi | Kurung `unsafe` di `storage`; sediakan `get`/`get_mut` aman saat `&mut self`, `unsafe` saat `&self`. |
| Pembungkus `SyncWorldView` (`unsafe impl Sync`) | Membatasi apa yang boleh diakses lintas-thread ke komponen ter-deklarasi; soundness dari disjoint stage. |
| Tetap data-parallel saja (`par_for_each`) | Menghindari seluruh masalah; cukup untuk banyak beban; bukan paralelisme tingkat-sistem. |

## Kriteria graduation

RN ini **graduate menjadi RFC** ketika:

1. Ada rancangan `UnsafeCell` yang menempatkan seluruh `unsafe` di modul `storage` dengan invarian keamanan tertulis per fungsi; dan
2. `miri` tersedia di CI untuk memverifikasinya; dan
3. Biaya pada jalur baca/tulis M-1 terukur dapat diterima (benchmark).

## Catatan / temuan

- 2026-07-27: RN dibuka dari analisis RFC-0006. Prasyarat infrastruktur (nightly + miri di CI) belum ada di lingkungan pengembangan saat ini.
- 2026-07-28: **Prasyarat #2 (miri di CI) digarap** — job `miri` ditambahkan ke `.github/workflows/ci.yml` (`cargo miri test -p arke --lib` di toolchain nightly). Karena core saat ini 0-`unsafe`, run pertama memvalidasi bahwa test suite lolos miri sebagai *baseline*; setiap `unsafe` mendatang wajib mempertahankan status hijau job ini. Lingkungan lokal masih tanpa nightly/miri, jadi verifikasi berlangsung di CI.
