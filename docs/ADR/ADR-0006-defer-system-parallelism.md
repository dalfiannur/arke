# ADR-0006: Menunda paralelisme tingkat-sistem hingga `UnsafeCell` + miri

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-27
- **RFC terkait:** [RFC-0006](../RFC/RFC-0006-system-level-parallelism.md)

## Konteks

M-2–M-4 menyiapkan seluruh prasyarat analitis untuk menjalankan sistem tak-konflik secara paralel (stage disjoint, akses tersimpul dari tipe). Namun implementasi yang **sound** menuntut membentuk `&mut [T]` ke data kolom dari `&World` bersama — yang hanya sah lewat penyimpanan berbasis `UnsafeCell`, sekaligus membuat `World: !Sync` (butuh pembungkus `unsafe impl Sync`). Jalur "aman-penuh" generik terbentur type-erasure slice yang dipinjam di `Schedule` heterogen. Rincian di [RFC-0006](../RFC/RFC-0006-system-level-parallelism.md).

Lingkungan pengembangan saat ini **tidak memiliki `miri`** (butuh nightly), sehingga argumen soundness `unsafe` aliasing tak dapat diverifikasi otomatis.

## Keputusan

Kami **menunda** implementasi eksekusi paralel tingkat-sistem. Ia baru boleh digarap setelah dua prasyarat terpenuhi:

1. **Penyimpanan kolom berbasis `UnsafeCell`** (lewat RFC tersendiri, karena menyentuh seluruh jalur baca/tulis M-1).
2. **`miri` berjalan di CI**, sehingga setiap `unsafe` aliasing terverifikasi, bukan sekadar diklaim.

Sampai itu, crate mempertahankan **0 `unsafe`**, dan satu-satunya paralelisme yang di-*ship* adalah **data-parallel** (`World::par_for_each`, ADR-0004), yang sudah aman.

Kebijakan yang ditetapkan: **tidak ada `unsafe` aliasing yang masuk ke kode tanpa verifikasi `miri`.**

## Konsekuensi

**Positif:**

- Menghindari men-*ship* *undefined behavior* yang tak terverifikasi ke "library produksi".
- Menjaga invarian *ergonomis = cepat* (STD-0004): jalur pengguna dan core tetap bebas `unsafe`.
- Analisis soundness terekam permanen; keputusan tak diperdebatkan ulang tanpa informasi baru (mis. miri tersedia).

**Negatif / biaya:**

- Belum ada percepatan tingkat-sistem; beban paralel harus memakai `par_for_each` (data-parallel).
- Fitur yang diminta ditunda, bukan dikirim.

**Netral / catatan:**

- STD-0006 tetap aktif via data-parallelism M-3.
- Ketika prasyarat siap, implementasi kemungkinan besar mengikuti pola bevy (storage `UnsafeCell` + executor multi-thread) dengan miri sebagai gerbang.

## Alternatif yang ditolak

- **Ship `unsafe` raw-pointer sekarang** — risiko UB tak terverifikasi (miri absen).
- **Redesign `UnsafeCell` + implementasi sekarang** — besar dan tetap tak terverifikasi tanpa miri.
- **Paralel data per-sistem sebagai pengganti** — sudah tersedia (`par_for_each`); bukan paralelisme tingkat-sistem yang dimaksud.

Rincian pertimbangan ada di [RFC-0006](../RFC/RFC-0006-system-level-parallelism.md).
