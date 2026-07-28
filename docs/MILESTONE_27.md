# Milestone 27 — Relasi entity persisten + join (RFC-0031)

> Persistensi relasi antar-entity (Entity → kolom FK) + join builder untuk
> arke-postgres. Memperluas RFC-0030. Prasyarat: core `Entity::from_raw` (additif).

## Ruang lingkup

**Termasuk:**

- **core**: `pub Entity::from_raw(u32, u32)` (additif) → rilis arke patch.
- **arke-postgres**: `ColumnDef.references: Option<&'static str>`; `migrate`
  memancarkan `REFERENCES`.
- **derive**: field `Entity`/`Option<Entity>` → dua kolom (`_id` FK + `_gen`) +
  to/from_params + token relasi `T::field() -> Field<T, EntityRef>`.
- **query builder**: `join` (filter-saja) + `join_load` (muat target R).

**Tidak termasuk:** self-join, arah reverse, relasi ECS in-memory first-class.

## Definition of Done

- [ ] Unit SQL-gen join (tanpa DB) + compile check token relasi.
- [ ] Integrasi DB: relasi save→load round-trip; `join` filter benar; `join_load`
      memuat target; handle basi (target di-despawn) → `world.get` = `None`.
- [ ] fmt/clippy/miri/CI hijau; API skalar RFC-0030 tak berubah.
