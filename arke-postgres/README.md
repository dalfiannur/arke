# arke-postgres

[![crates.io](https://img.shields.io/crates/v/arke-postgres.svg)](https://crates.io/crates/arke-postgres)
[![docs.rs](https://img.shields.io/docsrs/arke-postgres)](https://docs.rs/arke-postgres)

Adapter **PostgreSQL** untuk ECS [`arke`](https://crates.io/crates/arke): menjadikan
Postgres **sumber kebenaran (source of truth)** yang durable bagi keadaan ECS,
dengan **pemetaan relasional berkolom-tipe** yang bisa di-query SQL biasa
(join lintas-komponen, index, analitik, dibaca/ditulis service lain).

Lihat [RFC-0021](../docs/RFC/RFC-0021-arke-postgres-adapter.md) untuk desain lengkap.

> **Isolasi dependensi.** Core `arke` tetap **0 dependensi crates.io** (STD-0003).
> Crate adapter inilah gerbang dependensi DB (`sqlx`).

## Model

- **`World` = *working set* in-memory**; Postgres memegang data otoritatif.
- Sinkronisasi terjadi di **titik terkendali** (muat saat mulai, tulis-balik saat
  checkpoint) — **bukan** per-tick (ECS in-memory tak cocok disinkronkan tiap frame).
- Tiap tipe komponen `#[derive(PgComponent)]` → **satu tabel** `cmp_<nama>`, tiap
  field → **kolom SQL nyata ber-tipe**.

## Contoh singkat

```rust,no_run
use arke::World;
use arke_postgres::{PgComponent, PgStore};

#[derive(PgComponent)]
struct Position { x: f32, y: f32 }

#[derive(PgComponent)]
struct Health { hp: i32, shield: Option<i32> }   // Option → kolom nullable

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let mut store = PgStore::connect("postgres://user:pass@localhost/db").await?;
    store.register::<Position>().register::<Health>();
    store.migrate().await?;                        // CREATE TABLE per komponen

    // ---- ECS → Postgres ----
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Position { x: 1.0, y: 2.0 });
    world.insert(e, Health { hp: 100, shield: None });
    store.save(&world).await?;                     // overwrite penuh (transaksional)

    // ---- Postgres → ECS (handle identik) ----
    let mut restored = World::new();
    store.load(&mut restored).await?;
    assert!(restored.contains(e));
    Ok(())
}
```

Skema yang dihasilkan dapat langsung di-query & di-join:

```sql
SELECT p.entity_id, p.x, p.y, h.hp
FROM cmp_position p JOIN cmp_health h USING (entity_id)
WHERE h.hp < 20;
```

## Tiga mode tulis

| Metode | Semantik | Kapan |
| --- | --- | --- |
| `save(world)` | **Overwrite penuh** transaksional (reset versi) | Inisialisasi / single-writer / checkpoint sederhana |
| `save_incremental(world)` | **Diff** vs sinkron-terakhir → hanya entity baru/berubah ditulis, yang hilang di-DELETE | World besar, checkpoint berkala (hemat I/O) |
| `update_entity(world, e, ver)` | **Optimistic-lock** per-entity: menulis hanya bila versi & identitas cocok | Multi-writer (service lain juga menulis) |

`load(world)` me-materialize `World` dari Postgres, merekonstruksi entity dengan
**handle identik** (deterministik, `ORDER BY entity_id`).

### Optimistic-lock (multi-writer)

`arke_entities.version` naik tiap tulis-balik; `update_entity` gagal dengan
[`UpdateError::Conflict`] bila writer lain telah mengubah entity itu.

```rust,no_run
# use arke::{Entity, World};
# use arke_postgres::{PgStore, UpdateError};
# async fn f(store: &PgStore, world: &World, e: Entity) -> Result<(), sqlx::Error> {
let v = store.entity_version(e).await?.expect("entity ada");
match store.update_entity(world, e, v).await {
    Ok(new_version) => { /* tersimpan */ }
    Err(UpdateError::Conflict) => { /* re-baca versi, merge/retry */ }
    Err(UpdateError::Db(e)) => return Err(e),
}
# Ok(())
# }
```

> **Catatan:** `generation` arke hanya naik saat despawn/respawn (mendeteksi
> konflik **identitas**); kolom `version` terpisah mendeteksi konflik **nilai**.

## Pemetaan tipe Rust → SQL

| Rust | SQL |
| --- | --- |
| `i8`/`i16`/`i32`, `u8`/`u16` | `INTEGER` |
| `i64`/`isize`, `u32` | `BIGINT` |
| `u64`/`usize` | `NUMERIC(20)` (di luar jangkauan `BIGINT`) |
| `f32` / `f64` | `REAL` / `DOUBLE PRECISION` |
| `bool` | `BOOLEAN` |
| `String` | `TEXT` |
| `Option<T>` | kolom `T` **nullable** |
| lainnya (nested/enum/`Vec`) | `JSONB` (via `#[derive(arke::Serialize)]`) |

Field non-skalar butuh `#[derive(arke::Serialize)]` pada tipenya; hanya komponen
ber-`#[derive(PgComponent)]` yang dipersist.

## Menjalankan uji

Uji integrasi butuh Postgres nyata; di-*skip* bila `DATABASE_URL` tak diset:

```sh
docker run -d -e POSTGRES_PASSWORD=arke -e POSTGRES_USER=arke -e POSTGRES_DB=arke_test \
  -p 5432:5432 postgres:16
DATABASE_URL=postgres://arke:arke@localhost:5432/arke_test cargo test -p arke-postgres
```

Contoh end-to-end: [`examples/persist.rs`](examples/persist.rs)
(`DATABASE_URL=… cargo run -p arke-postgres --example persist`).

## Lisensi

MIT.
