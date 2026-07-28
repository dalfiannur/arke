# Milestone 29 — Ref<T> bertipe + path builder + deep join_load (RFC-0032 fase 2)

> Relasi **type-safe** (`Ref<T>`) + **path builder** `.through()` yang memuat entity
> sepanjang path (deep `join_load`). Menuntaskan penyaringan+pemuatan relasi 3–4
> deep secara type-safe.

## Ruang lingkup

**Termasuk:**

- `Ref<T>` (Entity + tipe target) — dipakai sebagai field relasi; `Entity` polos tetap.
- derive: field `Ref<T>`/`Option<Ref<T>>` → 2 kolom (`_id`+`_gen`) + token relasi
  **bertipe** `Field<C, RelRef<T>>`; to/from_params via `Ref::new`/`Entity::from_raw`.
- `RelRef<T>` marker + `matches(Filter<T>)` typed (target tersimpul, tanpa `::<R>`).
- `Query::through(rel)` → `PathQuery`; `.where_(Filter)`; `.load_all(world)` memuat
  Root + entity tiap hop sepanjang path yang cocok.

**Tidak termasuk:** recursive CTE (M-30).

## Definition of Done

- [ ] `Ref<T>` round-trip save→load; token relasi bertipe di-generate.
- [ ] Unit SQL-gen: path `.through().where_()` → filter bersarang + query per-level.
- [ ] Integrasi DB: path 3-deep menyaring benar; `load_all` memuat tiap level.
- [ ] fmt/clippy/miri/CI hijau; API RFC-0030/0031/0032-fase1 tak berubah.
