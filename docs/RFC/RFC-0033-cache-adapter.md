# RFC-0033: Cache adapter (read-through, Redis-compatible) untuk arke-postgres

- **Status:** Draft (konsep) <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-29
- **Crate:** **baru** `arke-cache` (adapter; core `arke` tetap 0-dependensi, STD-0003)
- **Memperluas:** [RFC-0021](RFC-0021-arke-postgres-adapter.md) (arke-postgres)
- **ADR terkait:** (menyusul bila di-Accept)

## Ringkasan

Lapisan **cache read-through** opsional di depan `arke-postgres`, berbasis backend
**Redis-wire-compatible** (Redis / **DragonflyDB** / KeyDB). Mengurangi round-trip
Postgres pada baca berulang (`load` / `load_where` / `query` / `join`), sambil
menjaga **Postgres tetap sumber kebenaran** & determinisme lewat **invalidasi
write-through**. Cache adalah lapisan *transparan-performa* (hasil identik, hanya
lebih cepat) — **bukan** sumber kebenaran kedua.

## ⚠️ Gate: ukur dulu (pelajaran RN-0003/RFC-0029)

Untuk banyak pemakaian ECS, **`World` sendiri sudah cache** (seluruh state di RAM,
`get` ~11 ns) — memuat sekali lalu jalan in-memory. Cache eksternal hanya relevan
di **boundary persistensi** dengan **baca berulang dari Postgres** (mis. reload
subset sering, banyak pembaca konkuren, hasil query/join mahal dipakai ulang).

**RFC ini graduate ke implementasi hanya bila ada sinyal terukur** bahwa *read*
Postgres jadi bottleneck. Tanpa itu, ini **YAGNI** — jangan tambah dependensi +
kompleksitas invalidasi untuk kemenangan tak-terukur.

## Non-goals

- **Cache untuk core ECS in-memory** — `World` sudah memegang state; cache jaringan
  malah lebih lambat dari akses memori.
- **Cache sebagai sumber kebenaran** — Postgres tetap otoritas (RFC-0021).
- **Query-result-set cache** (daftar entity_id per filter) di v1 — invalidasinya
  jauh lebih sulit (tulis apa pun ke tabel membatalkan banyak set). Ditunda.

## Usulan rinci

### 1. Crate adapter terpisah `arke-cache`

Core `arke` tetap 0-dependensi (STD-0003). Backend cache = **dependensi adapter**,
sama seperti `sqlx` di `arke-postgres`. Memakai klien Redis async (`redis`/`fred`).
Karena protokol RESP, **Dragonfly/Redis/KeyDB drop-in** — pilihan backend jadi
keputusan *operasional*, bukan arsitektural.

### 2. Yang di-cache: baris komponen per-entity

Kunci: `arke:{table}:{entity_id}` (mis. `arke:cmp_health:42`). Nilai: baris
komponen ter-serialisasi (nilai kolom). Granularitas ini transparan & invalidasinya
sederhana (per `(table, entity_id)`).

### 3. Read-through pada `materialize`

`PgStore` memperoleh **hook cache** aditif:

```rust
// arke-postgres (aditif):
pub trait ComponentCache: Send + Sync {
    async fn get(&self, table: &str, id: i64) -> Option<Vec<u8>>;
    async fn put(&self, table: &str, id: i64, bytes: &[u8]);
    async fn invalidate(&self, table: &str, ids: &[i64]);
}
impl PgStore { pub fn with_cache(self, cache: Arc<dyn ComponentCache>) -> Self; }
```

`materialize`: untuk tiap `(table, id)` → `cache.get` (hit → deserialisasi, lewati
Postgres) / miss → query Postgres → `cache.put`. Backend Redis-nya di `arke-cache`.

### 4. Konsistensi: invalidate-on-write + TTL

- **`save`/`save_incremental`/`update_entity`** → `cache.invalidate(table, changed_ids)`
  (write-through *invalidate*; baca berikutnya miss → Postgres segar). Write-through
  *update* (isi ulang) = optimasi lanjutan.
- **TTL** per kunci sebagai jaring pengaman (staleness terbatas walau invalidasi
  terlewat, mis. penulis lain menyentuh Postgres langsung).
- **Determinisme**: konsisten selama **semua tulis lewat store ber-cache**. Tulis
  luar-proses langsung ke Postgres tak terlihat cache → TTL membatasi basi.
  **Didokumentasikan sebagai batasan** (bukan bug).

## DragonflyDB — kelebihan (bersyarat)

vs Redis: **multi-threaded** (semua core, tanpa cluster), **efisiensi memori**
(dashtable), **snapshot tanpa fork**. Terasa **pada skala** (throughput/memori
tinggi). Wire-compatible → adalah *backend*, bukan API terpisah; adapter menargetkan
antarmuka Redis, Dragonfly menjadi swap-in.

## Alternatif yang dipertimbangkan

| Alternatif | Kenapa tidak (v1) |
| --- | --- |
| Cache di core ECS | `World` sudah cache; jaringan lebih lambat dari RAM |
| Query-result-set cache | Invalidasi jauh lebih sulit; baris-komponen lebih sederhana |
| Cache sbg sumber kebenaran | Melanggar "Postgres sumber kebenaran" + risiko basi |
| Write-through *update* (bukan invalidate) | Optimasi; v1 invalidate lebih sederhana & benar |
| Kunci ke Redis langsung (tanpa trait) | Trait `ComponentCache` menjaga `arke-postgres` bebas dependensi cache |

## Pertanyaan terbuka

1. **Serialisasi baris cache** — reuse `arke::Serialize`/`Value`→JSON, atau format
   biner ringkas? (JSON sederhana; biner lebih padat.)
2. **Async trait object** — `async fn` di `dyn ComponentCache` butuh pola boxed-future
   / generic; putuskan mekanismenya.
3. **Write-through update vs invalidate** — mulai invalidate; ukur apakah update sepadan.
4. **Query-result-set cache** — RFC lanjutan bila baca-query terbukti hot.
5. **Nama crate** — `arke-cache` (backend-agnostic) vs `arke-redis`.

## Rencana verifikasi (bila Accept)

- Unit: kunci/serialisasi; logika read-through (hit/miss) dgn cache tiruan (in-memory).
- Integrasi (CI, Redis/Dragonfly service container): load→cache-populate→reload
  hit; save→invalidate→reload segar; TTL kedaluwarsa.
- Benchmark: reload subset dengan vs tanpa cache (kuantifikasi kemenangan **sebelum**
  klaim — gate ukur-dulu).

## Keputusan

**Draft konsep — menunggu (a) sinyal terukur read-pressure Postgres, lalu (b) review
desain.** Bila tak ada sinyal, tetap Draft (tercatat, tak diimplementasi).
