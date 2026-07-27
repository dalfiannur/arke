# ADR-0005: Sistem berbasis-tipe dengan akses tersimpul

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-27
- **RFC terkait:** [RFC-0005](../RFC/RFC-0005-type-based-systems.md)

## Konteks

M-2 membuat sistem menyatakan akses secara eksplisit dan tak diverifikasi, sehingga deklarasi salah menjadi bug diam dan bukan model yang dikehendaki [ARCHITECTURE_BIBLE](../ARCHITECTURE_BIBLE.md) §3.1 ("System menyatakan kebutuhan datanya lewat tipe"). Menyimpulkan akses dari tipe query juga menjadi prasyarat paralelisme tingkat-sistem yang sound. Tantangannya: bentuk API yang menghindari `unsafe` dan kerumitan lifetime/GAT.

## Keputusan

Kami memilih:

1. Trait **`QueryData`** dengan `Item<'w>`, `access()` (baca/tulis tersimpul dari tipe), dan **iterasi internal** `each(world, f)`.
2. **Impl konkret per-kombinasi** untuk `&T`, `&mut T`, `(&A, &B)`, `(&A, &mut B)` (arity ≤ 2), memakai iterasi gabungan aman yang sudah ada (`split_at_mut`), **tanpa `unsafe`**.
3. **`System::each::<Q>(f)`** membangun sistem dengan akses tersimpul `Q::access()`; masuk ke `Schedule` M-2 yang sama.
4. Eksekusi tetap **serial**; paralelisme tingkat-sistem ditunda ke milestone berikutnya (menuntut `unsafe` terkurung).

## Konsekuensi

**Positif:**

- Akses sistem tersimpul dari tipe → celah deklarasi-salah M-2 tertutup.
- Model "System = fungsi atas Query" (§3.1) terwujud, lebih ergonomis.
- Tetap 100% aman & standalone; determinisme tak berubah.
- Menyiapkan paralelisme tingkat-sistem sound (akses tersimpul memberi rencana disjoint).

**Negatif / biaya:**

- Impl tuple ditulis per-kombinasi (bukan variadik generik) → menambah kombinasi berarti menambah impl.
- Iterasi internal (`each(f)`) alih-alih iterator yang dikembalikan — API sedikit kurang idiomatik, demi menghindari kerumitan lifetime.
- Belum ada percepatan tingkat-sistem (masih serial).

**Netral / catatan:**

- `Access` dipindah ke modul `query` sebagai tipe pakai-bersama.
- `(&mut A, &mut B)`, arity > 2, filter query, dan eksekusi paralel tingkat-sistem adalah pekerjaan berikutnya.

## Alternatif yang ditolak

- **Beberapa param `Query`** — butuh `unsafe` untuk akses disjoint simultan bahkan saat serial.
- **`QueryData` tuple generik via komposisi** — tak bisa *join* antar-komponen; HRTB/GAT rumit.
- **Mengembalikan iterator dari `Query<Q>`** — kerumitan lifetime/GAT pada tipe kembalian.

Rincian pertimbangan ada di [RFC-0005](../RFC/RFC-0005-type-based-systems.md).
