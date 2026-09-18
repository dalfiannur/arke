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
- Tiap tipe komponen `#[derive(PgComponent)]` → **satu tabel** `cmp_<nama>` (atau
  `#[pg(table = "…")]`), tiap field → **kolom SQL nyata ber-tipe**. Identifier
  di-quote bila perlu: field bernama `order`/`user`/`end` atau camelCase aman.

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

`load(world)` me-materialize **seluruh** `World` dari Postgres, merekonstruksi
entity dengan **handle identik** (deterministik, `ORDER BY entity_id`).

`load_where::<T>(world, predicate)` memuat **subset** (working-set parsial):
entity yang cocok predikat SQL atas kolom komponen `T`, beserta seluruh
komponennya — mis. muat hanya yang "sekarat":

```rust,no_run
# use arke::World;
# use arke_postgres::{PgComponent, PgStore};
# #[derive(PgComponent)] struct Health { hp: i32 }
# async fn f(store: &mut PgStore, world: &mut World) -> Result<(), sqlx::Error> {
let n = store.load_where::<Health>(world, "hp < 20").await?; // predikat = SQL mentah, tepercaya
# Ok(())
# }
```

Aman dikombinasi dengan `save_incremental` (entity tak-dimuat tak tersentuh).

Untuk filter **typed** (dicek compiler, ter-parameterisasi) pakai `query::<T>()`
— termasuk paginasi berikut total halamannya dan *containment* array JSONB:

```rust,no_run
# use arke::World;
# use arke_postgres::{Dir, PgComponent, PgStore};
# #[derive(PgComponent)] struct Meeting { start: i64, participants: Vec<i64> }
# async fn f(store: &mut PgStore, world: &mut World, uid: i64) -> Result<(), sqlx::Error> {
let f = Meeting::participants().contains(uid).and(Meeting::start().gte(1_700_000_000));
let total = store.query::<Meeting>().filter(f.clone()).count().await?; // COUNT(*), tanpa LIMIT
let n = store.query::<Meeting>().filter(f)
    .order_by(Meeting::start(), Dir::Asc).limit(20).offset(40)
    .load(world).await?;
# Ok(())
# }
```

**Hidrasi selektif** — `load` memuat *seluruh* komponen terdaftar untuk tiap
entity yang cocok (1 round-trip per tabel). Bila endpoint hanya butuh sebagian,
`.only::<(A, B)>()` (atau `.only::<A>()`) membatasi ke tabel itu saja; komponen
lain tak disentuh di `World`, dan `update_entity`/`save_incremental` **tidak**
menghapus komponen yang tak dimuat (kecuali kamu menyisipkannya sendiri di
World — itu ditulis). `save()` overwrite penuh tetap tak dijaga, sama seperti
working-set parsial `load_where`.

```rust,no_run
# use arke::World;
# use arke_postgres::{PgComponent, PgStore};
# #[derive(PgComponent)] struct Meeting { start: i64 }
# #[derive(PgComponent)] struct Room { code: String }
# async fn f(store: &mut PgStore, world: &mut World) -> Result<(), sqlx::Error> {
let n = store.query::<Meeting>().filter(Meeting::start().gte(0))
    .only::<(Meeting, Room)>()   // 2 + 2 round-trip, bukan 2 + R
    .load(world).await?;
# Ok(())
# }
```

### World per-request (REST/axum)

`PgStore` memegang jembatan pid↔entity dan rekam `save_incremental` yang
**per-World**, sehingga metode baca/tulisnya `&mut self`. Satu store melayani
**satu `World`**: bila `World` lain datang (dikenali lewat `World::id()`),
jembatan dan rekamnya di-reset — handle `Entity` tak bermakna lintas-World.
Jangan dibagi lewat `Mutex` antar-handler: simpan satu store **template**
(sudah `register` + `migrate`) di state, lalu **`fork()`** per request — pool
(`Arc`), registry, dan cache dibagi, jembatannya kosong. Beberapa
`query().load*()` ke satu `World` bersifat **aditif** (pid yang sudah termuat
di-refresh di tempat, bukan digandakan). Id publik = kolom `#[pg(unique)]`
(mis. UUID string), bukan `Entity` (ephemeral) maupun `pid` (integer
sekuensial).

```rust,no_run
# use arke::World;
# use arke_postgres::{PgComponent, PgStore, UpdateError};
# #[derive(PgComponent, Clone)] struct Booking { #[pg(unique)] booking_id: String, title: String }
# async fn handlers(tpl: &PgStore, id: String, title: String) -> Result<(), sqlx::Error> {
// POST: World kosong + spawn → save_incremental = INSERT murni.
let mut store = tpl.fork();
let mut w = World::new();
let e = w.spawn();
w.insert(e, Booking { booking_id: id.clone(), title });
store.save_incremental(&w).await?;
let pid = store.pid_of(e);                      // Some(pid) setelah tersimpan

// GET/PATCH by id: fork → muat subset → ubah → save_incremental hanya menyentuh subset.
let mut store = tpl.fork();
let mut w = World::new();
let hits = store.query::<Booking>().filter(Booking::booking_id().eq(id)).load_pids(&mut w).await?;
if let Some(&(pid, e)) = hits.first() {
    let mut b = w.get::<Booking>(e).unwrap().clone();
    b.title = "diubah".into();
    w.insert(e, b);                             // insert = upsert di tempat
    store.save_incremental(&w).await?;          // last-writer-wins
    // …atau ber-optimistic-lock (→ 409 pada konflik):
    // let v = store.entity_version(e).await?.unwrap();
    // match store.update_entity(&w, e, v).await { Err(UpdateError::Conflict) => …, r => … }
    // DELETE:
    // tpl.remove(pid).await?;
}
# Ok(())
# }
```

Diff `save_incremental` relatif ke **working set yang dimuat** (`last`), bukan
seluruh tabel: entity di luar subset tak tersentuh; yang di-`despawn` dari
subset di-DELETE; yang di-`spawn` baru di-INSERT.

### Semi-join by value, mutasi massal, agregasi

Tiga terminal yang hasilnya **bukan** entity termuat, tetap typed & ter-parameterisasi:

```rust,no_run
# use arke_postgres::{PgComponent, PgStore};
# #[derive(PgComponent)] struct Booking { room_code: String, status: String, minutes: i32, end_ts: i64 }
# #[derive(PgComponent)] struct Room { code: String, capacity: i32 }
# async fn f(store: &mut PgStore, now: i64) -> Result<(), sqlx::Error> {
// Semi-join lewat nilai kolom (tanpa relasi Entity): booking di ruang berkapasitas > 10.
let n = store.query::<Booking>()
    .filter(Booking::room_code().in_where(Room::code(), Room::capacity().gt(10)))
    .count().await?;

// UPDATE/DELETE massal ber-filter — versi entity terdampak naik, cache di-invalidate.
let expired = store.update_where::<Booking>()
    .filter(Booking::end_ts().lt(now))
    .set(Booking::status(), "expired".to_string())
    .execute().await?;                                   // jumlah baris
let purged = store.delete_where::<Booking>()
    .filter(Booking::status().eq("expired".to_string()))
    .execute().await?;                                   // menghapus ENTITY (cascade)

// Agregasi: tipe hasil = cast SQL eksplisit (SUM(x)::bigint), None bila tak ada baris.
let total: Option<i64> = store.query::<Booking>().sum::<i64>(Booking::minutes()).await?;
let per_room: Vec<(String, u64)> = store.query::<Booking>()
    .group_by(Booking::room_code()).count().await?;
let avg_per_room: Vec<(String, Option<f64>)> = store.query::<Booking>()
    .group_by(Booking::room_code()).avg::<f64>(Booking::minutes()).await?;
# Ok(())
# }
```

Yang sengaja **tidak** ada (tetap `load_where`/SQL): join berproyeksi kolom
lintas tabel, `HAVING`, multi-kunci `GROUP BY`, window function, UPSERT massal.

### Transaksi milik pemanggil (cek-lalu-tulis atomik)

Op per-entity (`commit_insert`/`commit_update`/`remove`) masing-masing
transaksional sendiri. Untuk pola **lock → cek → tulis** yang harus atomik
terhadap penulis lain — mis. tolak booking yang bentrok — pegang transaksinya
lewat `begin()` dan pakai varian `*_in(&mut tx, …)`:

```rust,no_run
# use arke::World;
# use arke_postgres::{PgComponent, PgStore};
# #[derive(PgComponent)] struct Slot { room: i64, start_ts: i64, end_ts: i64 }
# async fn f(store: &mut PgStore, world: &World, e: arke::Entity, room: i64, start: i64, end: i64)
# -> Result<Option<i64>, sqlx::Error> {
let staged = store.stage_insert(world, e);
let mut tx = store.begin().await?;
tx.advisory_lock(room).await?;                       // pg_advisory_xact_lock: serial per-ruang
let overlap = Slot::room().eq(room).and(Slot::start_ts().lt(end)).and(Slot::end_ts().gt(start));
if store.query::<Slot>().filter(overlap).exists_in(&mut tx).await? {
    tx.rollback().await?;
    return Ok(None);                                 // bentrok
}
let pid = store.commit_insert_in(&mut tx, staged).await?;
tx.commit().await?;
Ok(Some(pid))
# }
```

`count_in`/`exists_in` membaca lewat koneksi tx yang sama (melihat tulisan tx
itu yang belum di-commit); drop `PgTx` tanpa `commit` = rollback. `load`/`fetch`
sengaja tak punya versi tx — di dalam transaksi cukup cek keberadaan/jumlah.

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

## Index & constraint kustom

Atribut `#[pg(...)]` → `migrate` membuat indeks/constraint (idempoten):

```rust,no_run
# use arke_postgres::PgComponent;
#[derive(PgComponent)]
#[pg(table = "enemies")]           // nama tabel kustom (default `cmp_enemy`)
#[pg(check = "hp >= 0")]           // constraint CHECK level-tabel
struct Enemy {
    #[pg(index)]  kind: i32,       // btree index (mempercepat load_where)
    #[pg(unique)] tag: i64,        // UNIQUE index
    hp: i32,
}
```

- `#[pg(index)]` → `CREATE INDEX idx_<tabel>_<kolom>` btree (mempercepat filter
  `load_where`/builder); pada kolom **JSONB** (mis. `Vec<i64>`) → **GIN**, yang
  melayani `contains`/`contains_all` (`@>`). `migrate` mengganti btree lama di
  kolom JSONB dengan GIN. `#[pg(unique)]` selalu btree (GIN tak mendukung UNIQUE).
- `#[pg(unique)]` → `CREATE UNIQUE INDEX`.
- `#[pg(check = "…")]` (level-tipe, boleh banyak) → constraint `CHECK` bernama
  `chk_<tabel>_<hash ekspresi>`; mengubah/menghapus ekspresi direkonsiliasi
  `migrate` (yang lama di-drop). Baris lama yang melanggar ekspresi baru
  membuat `migrate` gagal — perbaiki datanya dulu.
- `#[pg(table = "…")]` (level-tipe) → nama tabel, dipakai **verbatim** (di-quote:
  huruf besar jadi case-sensitive). Dua struct bernama sama di modul berbeda
  butuh ini agar tak bertabrakan di `cmp_<nama>`.

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
