# Milestone 31 — Cache adapter read-through (RFC-0033)

> Cache opsional di depan arke-postgres (Redis/DragonflyDB). Gate "ukur dulu"
> terpenuhi (deployment remote-PG + point-read). Postgres tetap sumber kebenaran.

## Ruang lingkup

**Termasuk:**
- `arke-postgres`: trait `ComponentCache` + `PgStore::with_cache`; read-through di
  `materialize` (MGET batch); invalidate di `save_incremental`/`update_entity`;
  `clear` di `save`. Encode baris biner 0-serde.
- Crate baru `arke-cache`: `RedisCache` (Redis-compatible), resilien, TTL.
- CI: service container Redis + uji integrasi.

**Tidak termasuk:** query-result-set cache (invalidasi sulit — RFC lanjutan).

## Definition of Done
- [ ] Unit: encode/decode baris; read-through in-memory (hit + invalidasi tak basi).
- [ ] Integrasi DB+Redis: load isi cache, muat kedua hit, tulis → nilai segar.
- [ ] fmt/clippy/CI hijau (job Redis). Core arke tetap 0-dep.
