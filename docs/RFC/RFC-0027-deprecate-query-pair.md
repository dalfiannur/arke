# RFC-0027: Deprecate `query_pair`/`query_pair_ref` — konvergen ke `QueryData`

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-28
- **Graduasi dari:** [RN-0004](../RN/RN-0004-jalan-menuju-1.0.md)
- **ADR terkait:** [ADR-0027](../ADR/ADR-0027-deprecate-query-pair.md)

## Ringkasan

Menandai [`World::query_pair`] & [`World::query_pair_ref`] (**khusus arity-2**)
sebagai `#[deprecated]` di 0.6.0, mengarahkan pengguna ke jalur [`QueryData`]
generik (`<(&A, &mut B)>::each(...)`). Method dipertahankan sepanjang 0.6.x lalu
**dihapus di 1.0**. `query`/`query_mut` (arity-1, kasus paling umum)
**dipertahankan** sebagai shortcut.

## Motivasi

Menuju 1.0 (RN-0004), API query punya dua keluarga yang tumpang-tindih:

| API | Bentuk | Cakupan |
| --- | --- | --- |
| `query_pair::<A, B>()` | iterator `(&A, &mut B)` | **hanya** arity-2 |
| `query_pair_ref::<A, B>()` | iterator `(&A, &B)` | **hanya** arity-2 |
| `QueryData` generik | `<(...)>::each(w, \|..\|)` | arity **sembarang**, mutabilitas campuran |

`query_pair`/`_ref` adalah kasus khusus sempit dari jalur generik. Membekukan
keduanya di 1.0 = komit selamanya pada dua cara melakukan hal yang sama, salah
satunya tak-ortogonal (tak bisa arity-3+, tak bisa `Entity` sebagai term).
`query_pair_ref` bahkan **tanpa pemakai** di dalam repo.

`query`/`query_mut` **berbeda**: arity-1 adalah kasus paling umum dan iterator
tunggal-komponennya ergonomis; dipertahankan.

## Usulan rinci

```rust
#[deprecated(
    since = "0.6.0",
    note = "pakai QueryData generik: <(&A, &mut B)>::each(&mut world, |(a, b)| { … }). \
            Butuh `use arke::QueryData;`."
)]
pub fn query_pair<A: Component, B: Component>(&mut self) -> impl Iterator<Item = (&A, &mut B)> { … }

#[deprecated(since = "0.6.0", note = "pakai <(&A, &B)>::each(&mut world, |(a, b)| { … }).")]
pub fn query_pair_ref<A: Component, B: Component>(&self) -> impl Iterator<Item = (&A, &B)> { … }
```

Implementasi **tak berubah** — hanya atribut. Method tetap berfungsi selama
0.6.x agar migrasi mulus.

### Migrasi pemakai internal

CI menyetel `RUSTFLAGS="-D warnings"` → peringatan `deprecated` menggagalkan
build. Karena itu:

- **Contoh** (`no_unsafe`) & **doc crate**: dimigrasi ke jalur generik.
- **Uji perilaku** `query_pair` (masih menguji method sampai dihapus di 1.0):
  ditandai `#[allow(deprecated)]` pada scope uji.

## Verifikasi (TDD)

Deprecation adalah atribut waktu-kompilasi → diuji dengan **doc-test
`compile_fail`** ber-`#![deny(deprecated)]` (0 dependensi, pola sama RFC-0026):

- **RED:** sebelum atribut, pemakaian `query_pair` di bawah `deny(deprecated)`
  **kompilasi** → `compile_fail` gagal. Disaksikan.
- **GREEN:** setelah atribut, `deny(deprecated)` mengubah peringatan jadi error →
  `compile_fail` lolos.

## Dampak

- **Kompatibilitas:** **peringatan** (bukan breaking) di 0.6.0 — kode lama tetap
  jalan. **Breaking di 1.0** saat method dihapus. Jendela 0.6.x memberi waktu
  migrasi.
- **Perilaku/performa:** tak berubah (murni atribut).
- **Menuju 1.0:** API query 1.0 menjadi ortogonal — satu jalur tuple generik +
  shortcut arity-1.

## Alternatif yang dipertimbangkan

| Alternatif | Mengapa tidak |
| --- | --- |
| Pertahankan keduanya di 1.0 | Membekukan dua cara redundan; `_ref` bahkan tanpa pemakai |
| Hapus langsung di 0.6.0 (tanpa deprecate) | Breaking mendadak; deprecate memberi jendela migrasi |
| Deprecate `query`/`query_mut` juga | Arity-1 kasus umum & ergonomis; bukan redundansi yang sama |

## Keputusan

Diterima. Lihat [ADR-0027](../ADR/ADR-0027-deprecate-query-pair.md).

[`World::query_pair`]: ../../src/world.rs
[`World::query_pair_ref`]: ../../src/world.rs
[`QueryData`]: ../../src/query.rs
