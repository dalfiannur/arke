# arke-mongo

Adapter MongoDB untuk ECS [`arke`](https://crates.io/crates/arke): persistensi
**satu dokumen per entity**, dengan MongoDB sebagai **sumber kebenaran**
(RFC-0035).

> **Isolasi dependensi.** Core `arke` tetap **0 dependensi crates.io**
> (STD-0003); crate adapter inilah gerbang dependensi driver (`mongodb`, `bson`).

## Model data

Satu koleksi `arke_entities`, satu dokumen per entity:

```js
{
  _id: ObjectId("..."),   // pid — identitas persisten
  version: 3,             // optimistic lock
  cmp: {
    position: { x: 1.0, y: 2.0 },
    health:   { hp: 80 }
  }
}
```

Query lintas-komponen cukup satu `find`, tanpa join:

```js
db.arke_entities.find({ "cmp.position.x": { $gt: 100 }, "cmp.health.hp": { $lt: 20 } })
```

## Satu store, satu `World`

`MongoStore` mengunci `Entity` — handle yang **hanya bermakna di dalam satu
`World`** — ke `pid` yang persisten. Store **menautkan diri ke `World`
pertama** yang dilayaninya (dikenali lewat `World::id()`); operasi
`create`/`fetch`/`load`/`save`/`update`/`update_checked` dengan `World` lain
ditolak dengan `MongoError::WorldMismatch` — bukan merusak data diam-diam.
Untuk `World` lain (inspeksi lewat `World` sekali-pakai, atau pola World
per-request) pakai **`store.fork()`**: klien, database, dan registry sama,
jembatan `pid↔entity` kosong.

```rust,no_run
# use arke::World;
# use arke_mongo::MongoStore;
# async fn f(store: &mut MongoStore, pid: arke_mongo::Pid) -> Result<(), arke_mongo::MongoError> {
let mut scratch = World::new();
// store.fetch(&mut scratch, pid).await?;      // Err(WorldMismatch) bila store sudah tertaut World lain
let mut reader = store.fork();
reader.fetch(&mut scratch, pid).await?;        // OK: store sendiri untuk `scratch`
# Ok(())
# }
```

## Pemakaian

```rust
use arke::World;
use arke_mongo::{IndexDef, MongoStore, mongo_component};

#[derive(arke::Serialize)]
struct Position { x: f32, y: f32 }
mongo_component!(Position => "position");

#[derive(arke::Serialize)]
struct Health { hp: i64 }
mongo_component!(Health => "health", indexes: [IndexDef::asc("hp")]);

# async fn contoh() -> Result<(), arke_mongo::MongoError> {
// connect() ping server-nya dulu: URI ke host yang tak terjangkau gagal di
// sini, bukan diam-diam Ok lalu timeout buram di operasi pertama.
let mut store = MongoStore::connect("mongodb://localhost:27017", "game").await?;
store.register::<Position>();
store.register::<Health>();
store.ensure_indexes().await?;

let mut world = World::new();
let e = world.spawn();
world.insert(e, Position { x: 1.0, y: 2.0 });

let pid = store.create(&world, e).await?;      // satu round-trip
store.update(&world, e, pid).await?;           // last-write-wins
store.save(&world).await?;                     // seluruh working-set
# Ok(())
# }
```

Tak ada derive khusus: `#[derive(arke::Serialize)]` sudah menghasilkan bentuk
yang dibutuhkan BSON; `mongo_component!` hanya melengkapi nama dan indeks.

## `create` vs `save`: versi awal berbeda

`create` menyisip `version: 0` secara eksplisit. Entity yang lahir lewat
`save` (upsert `$inc` atas dokumen yang belum ada) mulai dari `version: 1`,
bukan `0`. Bila kamu mencampur `save` dan `update_checked` atas entity yang
sama, baca versi lewat `version_of` dulu — jangan asumsikan `0`.

`update`, berbeda dari `save`, **tidak** meng-upsert: bila dokumen `pid` sudah
tak ada (mis. dihapus penulis lain di antara `fetch` dan `update`), hasilnya
`Err(MongoError::Missing)` — bukan `Ok(())` yang diam-diam membuang tulisan.

`load` **menambah** ke `World`, bukan mengganti: ia selalu `world.spawn()`
entity baru untuk tiap dokumen. Memanggil `load` dua kali ke `World` yang sama
menggandakan tiap entity; pakai `World` kosong, atau `World` yang isinya
memang bukan milik store ini.

## Atomisitas — baca ini

`save` memakai **operasi per-dokumen berurutan tanpa transaksi**: atomik
**per-entity**, bukan per-World. Kegagalan di tengah meninggalkan sebagian
entity tertulis.

Ini konsekuensi sadar agar `mongod` **standalone** cukup — transaksi
multi-dokumen MongoDB menuntut replica set. Bila kamu butuh `save` yang
all-or-nothing, pakai [`arke-postgres`](https://crates.io/crates/arke-postgres),
yang transaksional penuh.

Mutasi **satu** entity (`create`/`update`/`update_checked`/`remove`) tetap
atomik, karena MongoDB menjamin atomisitas per-dokumen.

## Error

`MongoError` punya enam varian:

| Varian | Kapan |
| --- | --- |
| `Driver` | Kegagalan mentah dari driver MongoDB. |
| `Conflict` | `update_checked` gagal: `version` dokumen sudah bergeser. |
| `Decode` | Sub-dokumen komponen tak bisa direkonstruksi ke tipe Rust-nya. |
| `InvalidName` | Nama field komponen melanggar batas BSON (`.` atau berawalan `$`). |
| `DuplicateField` | Dua field komponen memetakan ke nama BSON yang sama. |
| `Missing` | `update` menyasar dokumen yang sudah tak ada. |

## Batasan v1

Belum ada: query builder bertipe, relasi entity, nested/recursive load, cache
read-through, `save_incremental`, transaksi multi-dokumen, dan strategi evolusi
skema. Semuanya ditunda ke RFC lanjutan — lihat RFC-0035 §8. `MongoStore::collection()`
membuka koleksi mentah sebagai jalan keluar sementara.

Tak ada `drop_database` atau metode destruktif lain — sengaja tak ada di API
publik yang dipublikasikan.

Menambah field non-`Option` ke komponen yang sudah punya dokumen lama akan
menghasilkan `MongoError::Decode`. Untuk sekarang: pakai `Option<T>` pada field
baru, atau jalankan skrip backfill.

## Menjalankan uji

Uji integrasi butuh MongoDB nyata; di-*skip* bila `MONGODB_URI` tak diset:

```sh
docker run -d -p 27017:27017 mongo:7
MONGODB_URI=mongodb://127.0.0.1:27017 cargo test -p arke-mongo
```

## Lisensi

MIT
