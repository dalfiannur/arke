# ADR-0033: Cache adapter read-through (Redis-compatible)

- **Status:** Accepted
- **Tanggal:** 2026-07-29
- **RFC terkait:** [RFC-0033](../RFC/RFC-0033-cache-adapter.md)

## Konteks

Baca Postgres bisa jadi bottleneck bila **remote + point-read-berat** (gate diukur:
Redis ~3× lokal, 20–60× lintas-jaringan; bulk-load justru lebih cepat via batch SQL).
Deployment pemilik proyek cocok syarat → implementasi.

## Keputusan

1. **Hook `ComponentCache`** (trait async, `async-trait`) di `arke-postgres`;
   `PgStore::with_cache`. Read-through di `materialize` (MGET batch), invalidate saat
   `save_incremental`/`update_entity`, `clear()` saat `save` penuh.
2. **Kunci `(table, entity_id)`**; encode baris biner ringkas **0-serde** (di
   `arke-postgres`, trait berurusan `Vec<u8>` → `arke-cache` bebas format).
3. **Crate `arke-cache`** (Redis-compatible: Redis/Dragonfly/KeyDB via `redis` crate).
   `FLUSHDB` di `clear` → asumsi DB terdedikasi. TTL jaring pengaman.
4. **Resilien**: kegagalan cache di-swallow → degradasi ke Postgres.
5. **Postgres tetap sumber kebenaran**; cache lapisan transparan-performa.

## Konsekuensi

- **Positif**: baca point cepat (remote); read-through transparan; core tetap 0-dep.
- **Biaya**: `async-trait` dep di arke-postgres; konsistensi bergantung "semua tulis
  lewat store" + TTL untuk tulis luar-proses (didokumentasikan).
- **Netral**: opsional (`None` → jalur lama). Query-result-set cache ditunda.

## Alternatif ditolak

- **Cache core ECS** — `World` sudah cache RAM.
- **FLUSHDB tanpa asumsi DB terdedikasi** — SCAN+DEL lebih aman tapi lambat; v1 pilih
  FLUSHDB + dokumentasi.

Rincian di [RFC-0033](../RFC/RFC-0033-cache-adapter.md).
