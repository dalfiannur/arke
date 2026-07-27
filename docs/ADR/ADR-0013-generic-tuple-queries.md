# ADR-0013: Query tuple generik (arity & mutabilitas campuran)

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-28
- **RFC terkait:** [RFC-0013](../RFC/RFC-0013-generic-tuple-queries.md)

## Konteks

`QueryData` (RFC-0005) ditulis konkret per-kombinasi karena komposisi tuple generik dianggap tak layak. Menambah `(&mut A, &mut B)` dan arity 3+ secara konkret adalah ledakan kombinatorial. `slice::get_disjoint_mut` (stabil Rust 1.86) memungkinkan banyak `&mut` kolom disjoint secara **aman**, sehingga tuple generik kini layak.

## Keputusan

Kami memilih:

1. Trait **`QueryTerm`** (`&T`/`&mut T`) yang memaparkan `access`, `component_id`, dan `iter(col) -> impl Iterator`.
2. **Impl `QueryData` generik** untuk tuple `(T0…Tn)` (arity 2–8) via makro; tiap archetype cocok memakai **`get_disjoint_mut`** untuk kolom `&mut` disjoint, lalu iterasi **lockstep** (`.next()`) memancarkan tuple item.
3. **Penolakan alias berkonteks**: term komponen-sama → panik `EcsError::QueryConflict` yang menyebut komponen.
4. **Menghapus** impl konkret `(&A, &B)`/`(&A, &mut B)` (digantikan generik); `&T`/`&mut T` tunggal & `query_pair*` tetap.

## Konsekuensi

**Positif:**

- Mendukung tuple arity & mutabilitas campuran sembarang, tanpa ledakan impl.
- Tetap **tanpa `unsafe`** (`get_disjoint_mut`), 0 dependensi eksternal.
- Mengoreksi asumsi RFC-0005 dengan pendekatan yang terbukti aman.

**Negatif / biaya:**

- Makro impl tuple menambah kompleksitas; arity dibatasi (2–8, bisa diperluas).
- Perlu akses `pub(crate)` baru pada `World`/`Archetype`.

**Netral / catatan:**

- Filter `With`/`Without` dan `Entity`-as-term ditunda.
- Determinisme (urutan archetype/baris) tak berubah.

## Alternatif yang ditolak

- **Impl konkret per-kombinasi** — tak skala ke arity 3+.
- **`unsafe` pointer aliasing** — UB tanpa verifikasi miri.
- **Makro nested-zip + flatten** — lebih rumit dari iterasi lockstep.

Rincian pertimbangan ada di [RFC-0013](../RFC/RFC-0013-generic-tuple-queries.md).
