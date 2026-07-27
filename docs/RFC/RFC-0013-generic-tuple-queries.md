# RFC-0013: Query tuple generik (arity & mutabilitas campuran)

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-28
- **Milestone:** M-12 (Generic Tuple Queries)
- **ADR terkait:** [ADR-0013](../ADR/ADR-0013-generic-tuple-queries.md)

## Ringkasan

Menggeneralisasi `QueryData` (RFC-0005) dari impl **konkret per-kombinasi** (`(&A, &B)`, `(&A, &mut B)`) menjadi impl **generik** untuk tuple sembarang-arity dengan **mutabilitas campuran** (`(&mut A, &mut B)`, `(&A, &B, &C)`, `(&A, &mut B, &mut C)`, …). Peminjaman kolom yang disjoint dilakukan **aman** lewat `slice::get_disjoint_mut` (stabil sejak Rust 1.86) — **tanpa `unsafe`**. Mengoreksi keputusan RFC-0005 §"alternatif" yang menyatakan komposisi tuple generik tak layak.

## Motivasi

RFC-0005 menulis tuple secara konkret karena "komposisi generik tak bisa *join*". Namun dengan (a) trait **term** yang memaparkan cara mengiterasi satu kolom, dan (b) `get_disjoint_mut` untuk banyak `&mut` kolom disjoint, tuple generik **bisa** dilakukan aman. Menambah tiap kombinasi baru secara konkret adalah ledakan (2ᴺ per arity); generik menyelesaikannya sekaligus.

## Usulan rinci

### 1. Trait `QueryTerm`

Satu elemen query (`&T` atau `&mut T`):

```rust
pub trait QueryTerm {
    type Item<'w>;
    fn access(access: &mut Access);            // baca/tulis komponen
    fn component_id(world: &World) -> Option<ComponentId>;
    fn iter(col: &mut Box<dyn Column>) -> impl Iterator<Item = Self::Item<'_>>;
}
```

- `&T` → `Item = &T`, iterasi `&[T]` (baca).
- `&mut T` → `Item = &mut T`, iterasi `&mut [T]` (tulis).

### 2. Impl tuple generik

Sebuah makro menghasilkan `impl QueryData` untuk `(T0, T1, …)` di mana tiap `Ti: QueryTerm`, arity **2–8**. Untuk tiap archetype yang cocok:

1. Cari indeks kolom tiap term (bila ada yang hilang → lewati archetype).
2. `arch.columns_disjoint_mut([i0, i1, …])` → `&mut` kolom yang **disjoint & aman** (`get_disjoint_mut`).
3. Iterasi tiap kolom (baca/tulis) secara **lockstep**, memancarkan tuple `(item0, item1, …)`.

### 3. Penolakan alias

Bila dua term merujuk komponen yang sama (mis. `(&mut A, &mut A)` atau `(&A, &mut A)`), `get_disjoint_mut` akan menolak indeks duplikat; kami mendeteksinya lebih awal dan **panik dengan pesan yang menyebut komponen** ([`EcsError::QueryConflict`], STD-0008).

### 4. Supersede

Impl konkret `(&A, &B)` & `(&A, &mut B)` dari M-4 **dihapus**, digantikan versi generik. Query satu-komponen (`&T`, `&mut T`) sebagai `QueryData` tetap. Method `World::query_pair`/`query_pair_ref` (M-1) tetap sebagai konvenience publik.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| Impl konkret per-kombinasi | Sederhana per-kasus | Ledakan 2ᴺ; tak skala ke arity 3+ | Tak memenuhi "arity 3+" |
| `unsafe` pointer aliasing | Fleksibel | UB tanpa verifikasi miri | `get_disjoint_mut` aman & cukup |
| Iterator via nested-zip + flatten makro | Elegan | Makro flatten rumit | Iterasi lockstep `.next()` lebih sederhana |

## Dampak

- **Kompatibilitas / migrasi:** API `System::each::<Q>` tak berubah; kini `Q` boleh tuple arity/mut apa pun (2–8).
- **Keamanan:** tetap tanpa `unsafe` (memakai `get_disjoint_mut`); alias ditolak berkonteks.
- **Konsekuensi pada invarian:** memperkuat *ergonomis = cepat* (jalur aman untuk query multi-mut) & determinisme (urutan archetype/baris tak berubah).

## Pertanyaan terbuka

- Filter `With<T>`/`Without<T>` → milestone berikutnya (M-13).
- Arity > 8 → tambah baris makro bila dibutuhkan.
- `Entity` sebagai term query (mengembalikan handle) → lanjutan.

## Keputusan

Diterima. Lihat [ADR-0013](../ADR/ADR-0013-generic-tuple-queries.md).
