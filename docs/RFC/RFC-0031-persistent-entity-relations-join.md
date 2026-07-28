# RFC-0031: Relasi entity persisten + join builder (arke-postgres)

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-29
- **Crate:** `arke-postgres` (+ `arke-postgres-derive`) & **prasyarat core** kecil
- **Memperluas:** [RFC-0030](RFC-0030-arke-postgres-typed-query-builder.md) (query builder)
- **ADR terkait:** [ADR-0031](../ADR/ADR-0031-persistent-entity-relations-join.md)

## Ringkasan

Menjadikan **`Entity` dapat dipersist sebagai relasi** (FK ke `arke_entities`) dan
menambah **join antar-entity** ke query builder RFC-0030:

```rust
#[derive(PgComponent)]
struct Owner { of: Entity }   // relasi: kolom FK ke arke_entities

// "muat pemilik yang meng-owner entity ber-Health < 20":
store.query::<Owner>()
    .join(Owner::of(), Health::hp().lt(20))
    .load(&mut world).await?;
// → SELECT o.entity_id FROM cmp_owner o
//   JOIN cmp_health h ON o.of_id = h.entity_id WHERE h.hp < $1
```

Menyimpan `(entity_id, generation)` → keamanan-basi (STD-0007) terbawa ke
persistensi. **Bukan** relasi ECS first-class (in-memory) — itu jalur pasca-1.0
terpisah.

## Motivasi

Relasi antar-entity kini **tak dapat dipersist**: `Entity` bukan skalar & bukan
`arke::Serialize`, jadi field `Entity` gagal dipetakan (bukan kolom, bukan JSONB).
Padahal struktur arke-postgres **sudah relasional** (`arke_entities` + tabel/komponen
ber-`entity_id`) — hanya kurang (a) kolom relasi dan (b) join lintas relasi.

## Prasyarat core (additif — aman pasca-freeze 1.0)

`Entity::new(index, generation)` saat ini `pub(crate)`; `from_params` (tanpa akses
`World`) tak bisa merekonstruksi handle relasi. Butuh:

```rust
impl Entity {
    /// Rekonstruksi handle dari nilai mentah (mis. deserialisasi). Handle yang
    /// "basi" tetap ditolak saat dipakai (`World::get` cek generation, STD-0007).
    pub fn from_raw(index: u32, generation: u32) -> Entity;
}
```

Additif (metode baru) → tak melanggar freeze bentuk-API 1.0. Forging handle sudah
mungkin via `spawn_at`/snapshot; keamanan tetap dari validasi-saat-pakai.

## Usulan rinci

### 1. `Entity` sebagai field relasi (derive)

Field `Entity` (atau `Option<Entity>`) → **dua kolom**:

```sql
<field>_id  BIGINT [NOT NULL] REFERENCES arke_entities(entity_id),
<field>_gen BIGINT [NOT NULL]
```

- `to_params`: `Entity` → `[Int(index), Int(generation)]`.
- `from_params`: `[Int(id), Int(gen)]` → `Entity::from_raw(id, gen)`.
- `Option<Entity>` → kedua kolom `NULL`-able (relasi opsional).

Perlu jalur kolom-FK di derive + `migrate`. Opsi representasi (untuk diputuskan):
`ColumnDef` diberi `references: Option<&'static str>`, atau `PgComponent` diberi
`const RELATIONS: &[RelationDef]`. (Menambah field `ColumnDef` = breaking kecil bagi
konstruksi manual → arke-postgres 0.7.)

> **Koreksi implementasi (2026-07-29): kolom relasi TANPA FK.** Rencana awal
> memberi `_id` FK `REFERENCES arke_entities` (via `ColumnDef.references`). Saat
> implementasi tersingkap **konflik**: `save`/`migrate` mengosongkan state via
> `DELETE FROM arke_entities` — sebuah **FK penghalang** (tanpa cascade) **memblokir**
> penghapusan itu (baris `cmp_<rel>` masih menunjuk). Sedangkan `ON DELETE CASCADE`
> menghapus **baris komponen keeper** (salah semantik ECS) dan `SET NULL` mustahil
> untuk `Entity` non-`Option`. Karena keputusan sudah **"dangling ditangani saat
> baca, bukan cascade"**, resolusi yang konsisten adalah **kolom `_id`/`_gen` =
> `BIGINT` polos tanpa FK**. Integritas by-construction (`save` menulis
> `arke_entities` lebih dulu) + keamanan-basi via generation saat baca. Field
> `ColumnDef.references` tetap ada (mekanisme FK umum), hanya **tak dipakai** relasi.

### 2. Keamanan-basi (keputusan: simpan id+gen)

Menyimpan `(id, gen)`; `from_raw` menghasilkan handle ber-generation. Saat
di-`world.get::<T>(handle)`, generation divalidasi → target mati/daur-ulang →
`None` (mirror STD-0007). FK `_id` menjaga integritas dasar (target ada di
`arke_entities`); penghapusan target ditangani di **baca** (dangling → None), bukan
`ON DELETE CASCADE`, agar semantik generation ECS terjaga.

### 3. Join builder

```rust
impl<'a, T: PgComponent> Query<'a, T> {
    /// Filter `T`: cocok bila entity yang ditunjuk `relation` (kolom FK di `T`)
    /// memenuhi `filter` atas komponen `R`. Target `R` **tak** dimuat.
    pub fn join<R: PgComponent>(self, relation: Field<T, EntityRef>, filter: Filter<R>) -> Self;

    /// Seperti `join`, **plus** memuat entity `R` yang cocok ke `world` (agar
    /// traversal handle relasi langsung jalan).
    pub fn join_load<R: PgComponent>(self, relation: Field<T, EntityRef>, filter: Filter<R>) -> Self;
}
```

- `relation` = token field `Entity` di `T` (mis. `Owner::of()`) → menentukan kolom
  FK `of_id`.
- SQL: `... FROM cmp_<T> t JOIN cmp_<R> r ON t.<rel>_id = r.entity_id WHERE <filter>`.
- Placeholder tetap ter-parameterisasi (RFC-0030); `ORDER BY t.entity_id`
  (determinisme).

## Keputusan pertanyaan terbuka (2026-07-29)

1. **Eager-load target — SEDIAKAN KEDUANYA.** `join(relation, filter)` = filter `T`
   saja (target `R` tak dimuat). `join_load(relation, filter)` = filter `T` **dan**
   memuat entity `R` yang cocok ke `world` (agar traversal handle langsung jalan).
2. **Representasi FK — `ColumnDef.references: Option<&'static str>`.** Kolom `_id`
   → `references = Some("arke_entities(entity_id)")`; `migrate` memancarkan
   `REFERENCES …`. Menambah field `ColumnDef` = **breaking kecil** (konstruksi
   manual) → **arke-postgres 0.7.0**.
3. **Self-join — NANTI** (bukan v1). Token `relation` eksplisit sudah
   men-disambiguasi multi-relasi; self-join (alias tabel) ditunda.
4. **Arah — hanya "T menunjuk R" di v1.** Reverse ("R yang ditunjuk T") ditunda.

## Yang **tidak** termasuk

- Relasi ECS **first-class in-memory** (gaya Flecs, `Query<(.., Rel<ChildOf>)>`) —
  fitur core besar, pasca-1.0.
- Join N-arah / agregasi / GROUP BY — di luar lingkup v1.
- Pola relasi in-memory **sudah bisa hari ini**: `struct Owner(Entity)` + traversal
  `world.get` (didokumentasikan sebagai panduan, tanpa API baru).

## Dampak

- **Kompatibilitas:** aditif kecuali representasi FK (bila `ColumnDef` diperluas →
  arke-postgres 0.7). Core hanya menambah `Entity::from_raw` (additif).
- **Keamanan:** generation-aware; jalur pengguna tetap bebas `unsafe`.

## Rencana verifikasi (TDD, saat Accepted)

- Unit SQL-gen join (tanpa DB): builder → SQL join + params.
- Integrasi DB: relasi disimpan+dimuat; `join` menyaring benar; handle basi (target
  di-despawn) → `world.get` = `None`.
- Round-trip: `Entity` field bertahan save→load.

## Keputusan

Diterima. Lihat [ADR-0031](../ADR/ADR-0031-persistent-entity-relations-join.md).
Urutan rilis: **arke patch** (`Entity::from_raw`, additif) → **arke-postgres 0.7.0**
(`ColumnDef.references` breaking-kecil + relasi + join).
