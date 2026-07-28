# ADR-0022: Bundle komponen (spawn/insert tuple)

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-28
- **RFC terkait:** [RFC-0022](../RFC/RFC-0022-component-bundles.md)

## Konteks

Menyisipkan komponen satu-per-satu memindahkan entity antar-archetype tiap
`insert` (`N` komponen → `N` pindah + archetype antara sia-sia). Tuple sudah
menjadi `Component` (blanket impl), sehingga `insert(e, tuple)` tak bisa dipakai
ulang untuk arti "bundle".

## Keputusan

Kami memilih:

1. **Trait `Bundle`** (`#[doc(hidden)]`), diimplementasikan untuk **bentuk tuple**
   `(A,)`…`(A,…,H)` dari `Component` distinct — **bukan** `T: Component` generik
   (menghindari tumpang-tindih dengan tuple-sebagai-Component).
2. **`World::insert_bundle`/`spawn_bundle`** (nama berbeda dari `insert`/`spawn`,
   sebab tuple valid sebagai komponen tunggal): menghitung archetype tujuan sekali
   (`lama ∪ ids`, terurut), memindahkan baris **sekali**, mendorong tiap komponen.
3. **Kontrak**: komponen bundle distinct + belum dimiliki → **panic** menyebut
   komponen (selaras `assert_no_alias`). Mencegah kolom rusak.

## Konsekuensi

**Positif:**

- Lebih ringkas **dan** lebih cepat (satu pindah archetype) → memperkuat
  *ergonomis = cepat*.
- Hasil **identik** dengan `insert` berurutan (archetype & nilai) — determinisme
  terjaga (id terurut, STD-0005).
- **Tanpa `unsafe` baru**; memakai operasi archetype aman yang ada.

**Negatif / biaya:**

- Nama baru (`insert_bundle`/`spawn_bundle`) alih-alih overload `insert`.
- Kontrak "komponen baru" (overwrite via bundle ditunda).

**Netral / catatan:**

- `Bundle` untuk tuple 1–8; bare `T` bukan bundle (pakai `insert`).
- Bundle di `CommandBuffer` & `remove_bundle` → follow-up.

## Alternatif yang ditolak

- **Overload `insert(e, bundle)`** — ambigu/breaking (tuple *adalah* Component).
- **`Bundle` untuk `T: Component` + tuple** — impl tumpang-tindih.
- **Overwrite komponen yang sudah ada** — butuh set-in-place bertipe; ditunda.

Rincian pertimbangan ada di [RFC-0022](../RFC/RFC-0022-component-bundles.md).
