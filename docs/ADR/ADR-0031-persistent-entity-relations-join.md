# ADR-0031: Relasi entity persisten + join builder (arke-postgres)

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-29
- **RFC terkait:** [RFC-0031](../RFC/RFC-0031-persistent-entity-relations-join.md)

## Konteks

Relasi antar-entity tak dapat dipersist (`Entity` bukan skalar/`Serialize`).
Struktur arke-postgres sudah relasional; kurang kolom relasi + join. Memperluas
query builder RFC-0030.

## Keputusan

1. **`Entity` sebagai field relasi** → dua kolom `<f>_id BIGINT REFERENCES
   arke_entities(entity_id)` + `<f>_gen BIGINT`. `Option<Entity>` → nullable.
2. **Prasyarat core additif**: `pub Entity::from_raw(u32, u32)` (untuk `from_params`
   merekonstruksi handle). Aman: handle basi ditolak saat pakai (STD-0007).
3. **Keamanan-basi**: simpan `(id, gen)`; dangling ditangani di **baca**
   (`world.get` → `None`), bukan `ON DELETE CASCADE`.
4. **Representasi FK**: tambah `ColumnDef.references: Option<&'static str>` (breaking
   kecil → arke-postgres **0.7.0**); `migrate` memancarkan `REFERENCES`.
   **KOREKSI saat implementasi:** kolom relasi `_id` **tanpa FK** (BIGINT polos) —
   FK penghalang tak kompatibel dgn reconcile-clear (`DELETE FROM arke_entities`),
   cascade/set-null salah semantik. `ColumnDef.references` tetap sbg mekanisme
   umum, tak dipakai relasi. Integritas by-construction + generation saat baca.
5. **Join builder**: `join(rel, filter)` (filter-saja) **dan** `join_load(rel,
   filter)` (muat target `R` juga). Token relasi `T::field()` eksplisit.
6. **Lingkup v1**: arah `T → R` saja; **self-join & reverse ditunda**.

## Konsekuensi

**Positif:**

- Relasi entity persisten + join tradisional; generation-aware.
- Memperluas RFC-0030 secara konsisten (typed, parameterized, deterministik).

**Negatif / biaya:**

- `ColumnDef` bertambah field → **breaking kecil** (arke-postgres 0.7.0).
- Butuh rilis core patch (`Entity::from_raw`) lebih dulu.
- Dua kolom per relasi (`_id`+`_gen`).

**Netral:**

- Pola relasi in-memory (`struct Owner(Entity)` + traversal) sudah ada — RFC ini
  hanya menambah **persistensi + join**.

## Alternatif yang ditolak

- **`const RELATIONS`** alih-alih `ColumnDef.references` — memilih yang terintegrasi
  dengan pipeline kolom/migrate yang ada.
- **`ON DELETE CASCADE`** — menghapus semantik generation ECS.
- **JSONB simpan Entity** — tak dapat di-join / di-FK secara relasional.

Rincian di [RFC-0031](../RFC/RFC-0031-persistent-entity-relations-join.md).
