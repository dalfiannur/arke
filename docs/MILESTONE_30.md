# Milestone 30 — Rekursi same-type via WITH RECURSIVE (RFC-0032 fase 3)

> Hierarki relasi **same-type** kedalaman **tak-tentu** (org-chart, tech-tree) via
> `WITH RECURSIVE` Postgres. `max_depth` **WAJIB** (guard siklus). Menuntaskan RFC-0032.

## Ruang lingkup

**Termasuk:**

- `Query::descendants_of(root, rel)` / `ancestors_of(start, rel)` — relasi self-ref
  bertipe `Field<T, RelRef<T>>` (mis. `Employee { manager: Ref<Employee> }`).
- `.max_depth(n)` **wajib** sebelum `.load(world)` (dienforce di tingkat tipe:
  `descendants_of` → `Recursive` (tanpa `load`) → `max_depth` → `RecursiveLoad`).
- SQL `WITH RECURSIVE` ter-parameterisasi (root id + max_depth); DISTINCT.

**Tidak termasuk:** deteksi-siklus via jejak path (dipilih `max_depth`), many-to-many.

## Definition of Done

- [ ] Unit SQL-gen: descendants & ancestors CTE ter-parameterisasi.
- [ ] Integrasi DB: org-chart descendants/ancestors benar; `max_depth` membatasi;
      siklus tak hang.
- [ ] `max_depth` wajib (tanpa itu tak ada `load`). fmt/clippy/CI hijau.
