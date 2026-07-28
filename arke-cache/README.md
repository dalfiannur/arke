# arke-cache

Cache **read-through** berbasis **Redis / DragonflyDB / KeyDB** (protokol RESP)
untuk [`arke-postgres`](https://crates.io/crates/arke-postgres) — [RFC-0033](https://github.com/dalfiannur/arke/blob/main/docs/RFC/RFC-0033-cache-adapter.md).

```rust
use std::sync::Arc;
use arke_postgres::PgStore;
use arke_cache::RedisCache;

let cache = RedisCache::connect("redis://localhost:6379", 300).await?; // TTL 300 dtk
let mut store = PgStore::connect("postgres://…").await?.with_cache(Arc::new(cache));
// Baca komponen kini read-through: hit dari cache, miss dari Postgres (lalu diisi).
```

- **Postgres tetap sumber kebenaran** — cache lapisan transparan-performa; tulis
  meng-*invalidate* (`save_incremental`/`update_entity`) atau `clear` (`save` penuh).
- **Resilien** — kegagalan cache tak memutus app (degradasi ke Postgres).
- **Konsistensi** — `clear()` = `FLUSHDB`, jadi **pakai DB/instance Redis
  terdedikasi** untuk cache arke. TTL membatasi basi dari tulis luar-proses.
- **Kapan dipakai** — hanya bila baca Postgres jadi bottleneck (Postgres *remote* +
  pola *point-read* berulang). Untuk load-sekali-lalu-in-memory, `World` sudah cache.

MIT.
