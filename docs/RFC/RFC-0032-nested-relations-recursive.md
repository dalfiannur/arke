# RFC-0032: Relasi bersarang (nested) + rekursif untuk arke-postgres

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-29
- **Crate:** `arke-postgres` (+ `arke-postgres-derive`)
- **Memperluas:** [RFC-0031](RFC-0031-persistent-entity-relations-join.md) (relasi + join)
- **ADR terkait:** [ADR-0032](../ADR/ADR-0032-nested-relations-recursive.md)

## Keputusan (2026-07-29)

- **Q1 → `Ref<T>` bertipe** (Entity + tipe target). Path & relasi fully type-safe;
  hop salah-tipe gagal kompilasi. `Entity` polos tetap didukung (path untyped).
- **Q4 → `max_depth` WAJIB** pada rekursi (guard siklus; relasi tanpa FK bisa menyimpang).

### Rencana bertahap (implementasi)

| Fase | Isi | Butuh `Ref<T>`? |
| --- | --- | --- |
| **1 — M-28** | `matches` nesting (rantai heterogen, **filter-saja** N-deep) | Tidak — jalan dgn `Entity` relasi yang ada |
| **2 — M-29** | `Ref<T>` bertipe + path builder `.through()` + **deep `join_load`** | Ya |
| **3 — M-30** | Rekursi same-type `WITH RECURSIVE` (`descendants`/`ancestors` + `max_depth`) | Memakai `Ref<T>` |

Fase 1 sendiri sudah menuntaskan **penyaringan 3–4 deep**; fase 2–3 menambah
pemuatan sepanjang path & rekursi.

## Ringkasan

Mendukung relasi **3–4 deep** dalam dua bentuk:

- **A. Rantai heterogen** (A→B→C→D, tipe berbeda, kedalaman tetap) — via
  `relation.matches(Filter<R>)` yang **bersarang** + pemuatan sepanjang path.
- **B. Rekursi same-type** (hierarki `parent→parent→…`, kedalaman tak-tentu) — via
  `WITH RECURSIVE` Postgres (`ancestors`/`descendants`).

Aditif di atas RFC-0031 (sub-query sudah menggeneralisasi ke N-deep).

## Motivasi

RFC-0031 `join` hanya **satu hop**. Domain nyata (game: squad→leader→weapon→enchant;
org: employee→manager→…) butuh **beberapa hop**. Untungnya pendekatan sub-query
RFC-0031 **bersarang secara alami** — hanya perlu API yang mengekspos itu.

## A. Rantai heterogen

### A1. Predikat relasi bersarang — `matches`

```rust
impl<C: PgComponent> Field<C, EntityRef> {
    /// `<rel>_id IN (SELECT entity_id FROM cmp_R WHERE <f>)`. Karena mengembalikan
    /// `Filter<C>` dan menerima `Filter<R>`, ia bersarang tanpa batas.
    pub fn matches<R: PgComponent>(self, f: Filter<R>) -> Filter<C>;
}
```

```rust
store.query::<Squad>()
    .filter(Squad::leader().matches(
        Unit::weapon().matches(
            Weapon::enchant().matches(Enchant::power().gt(100)))))
    .load(&mut world).await?;
```

SQL bersarang (ter-parameterisasi):

```sql
SELECT entity_id FROM cmp_squad WHERE leader_id IN (
  SELECT entity_id FROM cmp_unit WHERE weapon_id IN (
    SELECT entity_id FROM cmp_weapon WHERE enchant_id IN (
      SELECT entity_id FROM cmp_enchant WHERE power > $1)))
```

`join(rel, f)` menjadi **gula**: `filter(rel.matches(f))`. **Filter-saja**, memuat
`T` saja.

### A2. Deep `join_load` — memuat entity sepanjang path

Memuat `T` **dan** target di tiap hop. Butuh **path eksplisit** (bukan `Filter`
opak). Usulan API builder path bertipe:

```rust
store.query::<Squad>()
    .through::<Unit>(Squad::leader())      // PathQuery<Squad, Unit>
    .through::<Weapon>(Unit::weapon())     // PathQuery<Squad, Weapon>
    .through::<Enchant>(Weapon::enchant()) // PathQuery<Squad, Enchant>
    .where_(Enchant::power().gt(100))
    .load_all(&mut world).await?;          // muat Squad+Unit+Weapon+Enchant sepanjang path cocok
```

`.through::<Next>(Field<Cur, EntityRef>)` merekam hop `(Cur::TABLE, rel_col,
Next::TABLE)` & mentransisikan tipe path. `load_all` menjalankan, untuk tiap
kedalaman `k`, query "entity di level-k dari path yang cocok" lalu materialisasi:

```sql
-- level 1 (Unit): SELECT DISTINCT leader_id FROM cmp_squad WHERE <path filter>
-- level 2 (Weapon): SELECT DISTINCT weapon_id FROM cmp_unit
--                   WHERE entity_id IN (SELECT leader_id FROM cmp_squad WHERE …)
-- … dst.
```

## B. Rekursi same-type — `WITH RECURSIVE`

Hierarki tipe sama, kedalaman **tak-tentu** (mis. `Employee.manager: Entity`):

```rust
// Semua bawahan (transitif) dari `boss`:
store.query::<Employee>()
    .descendants_of(boss, Employee::manager())   // manager menunjuk ATASAN
    .max_depth(4)                                 // opsional; guard siklus
    .load(&mut world).await?;
```

```sql
WITH RECURSIVE sub AS (
  SELECT entity_id, 0 AS depth FROM cmp_employee WHERE manager_id = $1
  UNION ALL
  SELECT e.entity_id, sub.depth+1 FROM cmp_employee e
  JOIN sub ON e.manager_id = sub.entity_id
  WHERE sub.depth < $2)
SELECT entity_id FROM sub
```

`ancestors_of` = arah sebaliknya. `max_depth` menjaga dari siklus (relasi bisa
menyimpang; tanpa FK, integritas tak dijamin DB).

## Pertanyaan terbuka (untuk review)

1. **Typed entity refs?** Path `.through::<Next>(token)` butuh anotasi tipe target
   manual karena `Entity` **tak bertipe**. Alternatif: perkenalkan **`Ref<T>`**
   (Entity bertipe) → `pet: Ref<Beast>` → path/relasi **fully type-safe** (tak perlu
   `::<Next>`). Perubahan lebih besar (tipe baru + derive) — masuk RFC ini atau
   terpisah?
2. **Ergonomi path** — `.through::<T>()` rantai vs makro `path![…]` vs `Ref<T>`.
3. **Deduplikasi load** — level dalam bisa menunjuk entity berulang; materialisasi
   idempoten (spawn_at) — cukup?
4. **Siklus rekursi** — `max_depth` wajib, atau deteksi siklus (via array path)?
5. **Batas kedalaman nesting `matches`** — Postgres punya batas praktis; dokumentasikan.

## Yang **tidak** termasuk

- Agregasi lintas relasi (COUNT anak, dsb.), graph traversal umum.
- Relasi many-to-many (butuh tabel jembatan) — mungkin RFC lanjutan.

## Rencana verifikasi (TDD, saat Accepted)

- Unit SQL-gen: nesting `matches` 3–4 deep; recursive CTE; path `load_all`.
- Integrasi DB: rantai 3-deep menyaring benar; `load_all` memuat tiap level;
  recursive descendants/ancestors + `max_depth`; siklus tak hang.

## Keputusan

**Draft — menunggu review.** Terutama Pertanyaan #1 (Typed `Ref<T>`) yang paling
memengaruhi bentuk API.
