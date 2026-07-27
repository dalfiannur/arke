# ADR-0004: Iterasi data-parallel yang aman

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-27
- **RFC terkait:** [RFC-0004](../RFC/RFC-0004-data-parallel-iteration.md)

## Konteks

M-2 menghasilkan rencana paralel tingkat-sistem, tetapi mengeksekusinya di thread secara sound menuntut sistem yang membatasi akses secara fisik (model berbasis-tipe) — sistem `FnMut(&mut World)` M-2 bisa menyentuh apa saja, sehingga paralelisasi langsungnya tidak aman tanpa `unsafe` yang bocor ke pengguna (melanggar STD-0004). Paralelisme **data** — membagi iterasi satu operasi atas banyak entity — dapat dibuat sound tanpa `unsafe` dan memberi percepatan nyata untuk beban per-entity.

## Keputusan

Kami memilih menambahkan **`World::par_for_each::<T>(f)`** untuk iterasi komponen data-parallel:

1. Membagi kolom `&mut [T]` tiap archetype dengan `slice::chunks_mut` dan menjalankan tiap chunk di `std::thread::scope` — **tanpa `unsafe`**.
2. Membatasi kontrak ke `f: Fn(&mut T) + Sync` yang **per-elemen independen**, sehingga hasilnya deterministik dan setara serial (mengaktifkan **STD-0006**).
3. Memakai `std::thread::available_parallelism` untuk jumlah thread; **tanpa dependensi eksternal** (STD-0003).

## Konsekuensi

**Positif:**

- Percepatan paralel untuk beban per-entity, dengan jaminan paralel = serial (STD-0006).
- Tetap 100% aman (tak ada `unsafe`) dan standalone.
- Aditif; tak mengubah API M-1/M-2.

**Negatif / biaya:**

- `std::thread::scope` men-spawn thread per pemanggilan (tanpa pool) — overhead untuk beban kecil; pool adalah optimasi kemudian.
- Kontrak per-elemen independen adalah tanggung jawab pengguna; operasi dengan efek antar-elemen di luar cakupan.
- Belum memparalelkan stage scheduler (paralelisme tingkat-sistem) — itu menunggu model sistem berbasis-tipe.

**Netral / catatan:**

- Varian read-only dan pasangan `(&A, &mut B)` paralel, serta thread pool, adalah pekerjaan berikutnya.
- Paralelisme tingkat-sistem yang sound tetap memerlukan sistem berbasis-tipe (milestone tersendiri).

## Alternatif yang ditolak

- **Paralel tingkat-sistem via `unsafe` berbagi World** — soundness bergantung deklarasi akses tak-ditegakkan; mendorong `unsafe` ke pengguna.
- **rayon** — dependensi eksternal; melanggar standalone.
- **Thread pool sekarang** — kompleksitas; ditunda sebagai optimasi.

Rincian pertimbangan ada di [RFC-0004](../RFC/RFC-0004-data-parallel-iteration.md).
