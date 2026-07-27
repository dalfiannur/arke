# RFC-0004: Iterasi data-parallel yang aman

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-27
- **Milestone:** M-3 (Data-Parallel Iteration)
- **ADR terkait:** [ADR-0004](../ADR/ADR-0004-data-parallel-iteration.md)

## Ringkasan

Memperkenalkan iterasi komponen **data-parallel** yang **sepenuhnya aman**: `World::par_for_each::<T>(f)` menerapkan `f: Fn(&mut T) + Sync` pada setiap entity pemilik `T`, membagi baris antar-thread via `std::thread::scope` + `slice::chunks_mut`. Karena `f` memproses tiap elemen secara **independen**, keadaan akhir identik dengan eksekusi serial apa pun urutan/jumlah thread — mengaktifkan **STD-0006** (paralel setara serial) untuk operasi ini. Tanpa `unsafe`, tanpa dependensi eksternal.

## Motivasi

M-2 menghasilkan rencana paralel tingkat-sistem (stage), tetapi mengeksekusinya di thread secara **sound** menuntut sistem yang membatasi akses secara fisik (model berbasis-tipe) — kalau tidak, `FnMut(&mut World)` M-2 bisa menyentuh apa saja dan menimbulkan data race. Membangun model berbasis-tipe adalah pekerjaan besar tersendiri.

Sebaliknya, paralelisme **data** (membagi iterasi satu sistem atas banyak entity) dapat dibuat sound **tanpa `unsafe`**: `chunks_mut` menghasilkan sub-slice yang disjoint, dan `std::thread::scope` menjamin thread selesai sebelum pinjaman berakhir. Ini memberi percepatan nyata pada beban kerja per-entity yang umum (fisika, transformasi, integrasi) sambil menghormati invarian *ergonomis = cepat* (jalur pengguna tanpa `unsafe`, STD-0004) dan *standalone* (STD-0003).

## Usulan rinci

### 1. API

```rust
impl World {
    /// Menerapkan `f` pada komponen `T` setiap entity pemiliknya, secara paralel.
    ///
    /// `f` harus memproses tiap elemen secara independen (tanpa ketergantungan
    /// antar-elemen); dengan syarat itu, hasilnya deterministik & setara serial.
    pub fn par_for_each<T: Component>(&mut self, f: impl Fn(&mut T) + Sync);
}
```

Batas tipe: `T: Component` (`'static + Send`), `f: Fn(&mut T) + Sync`. `Send` pada `T` mengizinkan sub-slice `&mut [T]` berpindah ke thread lain; `Sync` pada `f` mengizinkan closure dibagi.

### 2. Mekanisme

```text
untuk setiap archetype yang memiliki kolom T:
    ambil &mut [T] (slice kontigu kolom itu)
    bagi menjadi chunk berukuran ~len / jumlah_thread
    std::thread::scope: spawn satu thread per chunk, tiap thread menerapkan f
    scope menjamin semua thread join sebelum pinjaman &mut berakhir
```

Jumlah thread dari `std::thread::available_parallelism()` (fallback 1). Chunk berasal dari `chunks_mut` → disjoint & aman. Tidak ada `unsafe`.

### 3. Determinisme (STD-0006)

`f` yang per-elemen independen menghasilkan keadaan akhir yang **tidak bergantung** pada urutan pemrosesan. Maka untuk himpunan entity yang sama, hasil paralel == hasil serial, apa pun jumlah thread. Ini kontrak yang didokumentasikan; operasi dengan efek antar-elemen (mis. menulis ke akumulator bersama) berada di luar kontrak `par_for_each`.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| Paralel tingkat-sistem (eksekusi stage M-2 di thread) | Mewujudkan rencana stage | Butuh model sistem berbasis-tipe agar sound; atau API `unsafe` (melanggar STD-0004) | Ditunda; data-parallel memberi nilai lebih dulu tanpa `unsafe` |
| `unsafe` berbagi `*mut World` untuk paralel sistem | Fleksibel | Soundness bergantung deklarasi akses yang tak ditegakkan | Melanggar semangat "jalur pengguna tanpa unsafe" |
| Thread pool (std, tanpa dep) | Menghindari spawn berulang | Kompleksitas & state global | Optimasi; `scope` spawn cukup untuk M-3, pool menyusul |
| rayon | Work-stealing matang | Dependensi eksternal | Melanggar standalone (STD-0003) |
| Kumpulkan-lalu-reduksi | Mendukung agregasi | Overhead pengumpulan & kompleksitas | Di luar lingkup M-3; kontrak per-elemen independen cukup |

## Dampak

- **Kompatibilitas / migrasi:** aditif; tak mengubah API M-1/M-2.
- **Keamanan / izin / provenance:** tetap 100% aman; tak memperkenalkan `unsafe`.
- **Konsekuensi pada invarian:** mengaktifkan **STD-0006** untuk `par_for_each`; memperkuat *paralelisme yang aman* dan *ergonomis = cepat*. Paralelisme tingkat-sistem tetap terbuka untuk milestone berbasis-tipe berikutnya.

## Pertanyaan terbuka

- Ambang ukuran di mana paralel mengalahkan serial (spawn thread punya overhead) → heuristik/`available_parallelism`; profil di kemudian hari. → RN bila perlu.
- Varian read-only `par_for_each` dan bentuk pasangan `(&A, &mut B)` paralel → milestone berikutnya.
- Paralelisme tingkat-sistem sound via sistem berbasis-tipe → milestone tersendiri.

## Keputusan

Diterima. Lihat [ADR-0004](../ADR/ADR-0004-data-parallel-iteration.md).
