# RFC-0005: Sistem berbasis-tipe dengan akses tersimpul

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-27
- **Milestone:** M-4 (Type-Based Systems)
- **ADR terkait:** [ADR-0005](../ADR/ADR-0005-type-based-systems.md)

## Ringkasan

Memperkenalkan trait **`QueryData`** yang menyimpulkan **akses** (baca/tulis) sebuah query dari **tipe** parameternya, dan konstruktor **`System::each::<Q>(f)`** yang membangun sistem dari `Q: QueryData` + closure per-entity — dengan akses otomatis tersimpul, menggantikan deklarasi `reads`/`writes` manual M-2 untuk sistem semacam ini. Ini mewujudkan model "System = fungsi atas Query" dari [ARCHITECTURE_BIBLE](../ARCHITECTURE_BIBLE.md) §3.1. Eksekusi tetap **serial** dan **100% aman**; paralelisme tingkat-sistem (yang menuntut `unsafe` terkurung) ditunda ke milestone berikutnya.

## Motivasi

M-2 membuat sistem menyatakan akses secara **eksplisit dan tak diverifikasi** (`.reads::<A>().writes::<B>()`), sehingga deklarasi yang salah adalah bug diam. ARCHITECTURE_BIBLE §3.1 menghendaki sistem menyatakan kebutuhan datanya lewat **tipe**. Menyimpulkan akses dari tipe query:

- menghilangkan celah deklarasi-salah (tipe query = akses sebenarnya);
- menjadi prasyarat paralelisme tingkat-sistem yang sound (param bertipe membatasi apa yang bisa disentuh sistem);
- membuat penulisan sistem lebih ergonomis.

## Usulan rinci

### 1. Trait `QueryData`

```rust
pub trait QueryData {
    type Item<'w>;
    /// Akses statis (baca/tulis) yang disimpulkan dari tipe.
    fn access() -> Access;
    /// Menjalankan `f` untuk setiap entity yang cocok (iterasi internal).
    fn each(world: &mut World, f: impl FnMut(Self::Item<'_>));
}
```

Iterasi **internal** (`each` memanggil `f` per item) dipilih daripada mengembalikan iterator — menghindari kerumitan lifetime/GAT pada tipe kembalian sekaligus tetap monomorfik (tanpa `dyn` di jalur panas).

### 2. Impl yang disediakan (arity ≤ 2)

| Tipe `Q` | `Item<'w>` | Akses |
| --- | --- | --- |
| `&T` | `&'w T` | baca `T` |
| `&mut T` | `&'w mut T` | tulis `T` |
| `(&A, &B)` | `(&'w A, &'w B)` | baca `A`, baca `B` |
| `(&A, &mut B)` | `(&'w A, &'w mut B)` | baca `A`, tulis `B` |

Impl tuple ditulis **konkret per-kombinasi** (bukan komposisi generik), memakai iterasi gabungan aman yang sudah ada (`query`/`query_mut`/`query_pair`, `split_at_mut`). Tuple `(&mut A, &mut B)` dan arity > 2 ditunda.

### 3. `System::each`

```rust
impl System {
    pub fn each<Q: QueryData>(f: impl FnMut(Q::Item<'_>) + 'static) -> System;
}
```

`each` membangun `System` dengan `access = Q::access()` (tersimpul) dan `run` yang memanggil `Q::each(world, &mut f)`. Sistem hasilnya masuk ke `Schedule` yang sama seperti M-2; penetapan stage kini memakai akses tersimpul.

Deklarasi eksplisit M-2 (`System::new(...).reads().writes()`) **tetap ada** untuk sistem `FnMut(&mut World)` yang butuh akses World penuh.

### 4. Lokasi `Access`

`Access` dipindah ke modul `query` sebagai tipe pakai-bersama; `schedule` mengimpornya. Aturan konflik dan penetapan stage M-2 tak berubah.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| Beberapa param `Query` (`fn(Query<&A>, Query<&mut B>)`) | Paling menyerupai bevy | Butuh akses World disjoint simultan → `unsafe` bahkan saat serial | Melanggar tujuan aman M-4; satu param tuple cukup |
| `QueryData` tuple generik via komposisi `A: QueryData, B: QueryData` | Elegan, variadic | Komposisi tak bisa melakukan *join* antar-komponen; HRTB/GAT rumit | Impl konkret per-kombinasi lebih sederhana & pasti kompilasi |
| Kembalikan iterator dari `Query<Q>` | Ergonomis (`for x in q`) | Lifetime/GAT pada tipe kembalian sangat rumit | Iterasi internal `each` menghindarinya tanpa kehilangan performa |
| Tetap deklarasi eksplisit M-2 | Tak ada kode baru | Celah deklarasi-salah tetap; tak menyiapkan paralel sound | Tujuan M-4 adalah akses tersimpul dari tipe |

## Dampak

- **Kompatibilitas / migrasi:** aditif; `Access` pindah modul (internal). API M-1/M-2/M-3 tak berubah.
- **Keamanan / izin / provenance:** tetap 100% aman; akses tersimpul menutup celah deklarasi-salah M-2.
- **Konsekuensi pada invarian:** mewujudkan model sistem §3.1; menyiapkan *paralelisme yang aman* tingkat-sistem (milestone berikutnya). Determinisme tak berubah (eksekusi serial).

## Pertanyaan terbuka

- Tuple `(&mut A, &mut B)` dan arity > 2 (via makro) → milestone berikutnya.
- Filter query (`With`/`Without`) sebagai bagian `QueryData` → milestone berikutnya.
- Eksekusi paralel tingkat-sistem memakai `Q::access()` untuk memberi tiap thread pandangan disjoint (butuh `unsafe` terkurung) → milestone tersendiri.

## Keputusan

Diterima. Lihat [ADR-0005](../ADR/ADR-0005-type-based-systems.md).
