# ADR-0032: Relasi bersarang + rekursif (arke-postgres)

- **Status:** Accepted
- **Tanggal:** 2026-07-29
- **RFC terkait:** [RFC-0032](../RFC/RFC-0032-nested-relations-recursive.md)

## Konteks

RFC-0031 `join` satu-hop. Domain nyata butuh relasi 3–4 deep (rantai heterogen)
& hierarki rekursif same-type. Sub-query RFC-0031 bersarang alami.

## Keputusan

1. **`matches(Filter<R>) -> Filter<C>`** pada token relasi → predikat bersarang
   N-deep (filter-saja). `join` = gula `filter(rel.matches(f))`.
2. **`Ref<T>` bertipe** (Entity + tipe target) untuk path/relasi fully type-safe;
   `Entity` polos tetap didukung.
3. **`max_depth` WAJIB** pada rekursi (guard siklus; relasi tanpa FK).
4. **Rekursi via `WITH RECURSIVE`** (`descendants_of`/`ancestors_of`).
5. **Bertahap**: M-28 matches nesting → M-29 Ref<T>+path+deep load → M-30 recursive.

## Konsekuensi

- **Positif**: relasi dalam type-safe, komposabel, ter-parameterisasi; aditif.
- **Biaya**: `Ref<T>` + path builder + CTE = permukaan API besar → dibagi milestone.
- **Netral**: fase 1 (matches) sudah menuntaskan penyaringan 3–4 deep.

## Alternatif ditolak

- **JOIN + alias** — subquery lebih bersih, dioptimasi setara (semi-join).
- **Entity + `::<Next>` manual** — verbose & salah-tipe jadi bug runtime; `Ref<T>` type-safe.

Rincian di [RFC-0032](../RFC/RFC-0032-nested-relations-recursive.md).
