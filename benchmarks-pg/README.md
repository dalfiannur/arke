# Benchmark: arke-postgres vs BunSane

Benchmark **kompetitif lintas-bahasa** untuk dua *Entity–Component store di atas
PostgreSQL*:

| | Bahasa/runtime | Model penyimpanan |
|---|---|---|
| [`arke-postgres`](../arke-postgres) | Rust + `sqlx` + tokio | 1 tabel typed-column per komponen (`cmp_<nama>`), FK ke `arke_entities` |
| [BunSane](https://github.com/yaaruu/bunsane) `0.5.7` | TypeScript + Bun | tabel `components` **JSONB, partitioned by `type_id`** (1 partisi/komponen) + `entities` |

> Ini **bukan** perbandingan in-memory game-ECS (hecs/bevy_ecs) — untuk itu lihat
> [`../benchmarks`](../benchmarks). BunSane adalah EC-store persisten, jadi lawan
> yang setara adalah `arke-postgres`, bukan `arke` core.

## Menjalankan

Butuh: **Postgres** jalan, **Rust** toolchain, **Bun**. Skrip membuat dua database
terpisah (`arke_bench`, `bunsane_bench`) — pastikan sudah ada (lihat di bawah).

```sh
./run.sh [N] [ITERS] [C]      # default: 20000 5 8
# contoh:
./run.sh 10000 5 8
```

`C` = konkurensi tulis, **disamakan kedua sisi** (arke `--concurrency C`, BunSane
`SAVE_CONCURRENCY=C`) → perbandingan apel-ke-apel. Tanpa penyamaan ini, default
tiap sisi berbeda (arke C=1 sekuensial vs BunSane C=20) sehingga `save` timpang &
menyesatkan. Untuk **kurva scaling** lintas beberapa `C`, pakai `sweep.sh`.

Env opsional: `PGHOST PGPORT PGUSER PGPASS` (default `postgres:postgres@localhost:5432`).

### Sweep konkurensi (multi-core)

Karena tak satu sisi pun CPU-bound (keduanya round-trip Postgres), "multi-core"
di sini = **berapa transaksi tulis konkuren** → berapa backend Postgres paralel.
`sweep.sh` menjalankan kedua engine pada beberapa level konkurensi dan mencetak
kurva scaling:

```sh
./sweep.sh [N] [ITERS] [LEVELS...]     # default: 20000 5 "1 4 8 16"
./sweep.sh 10000 5 1 4 8 16
```

- arke: `--concurrency C` → `C` transaksi per-entity konkuren (`buffer_unordered`).
  `C=1` = sekuensial.
- BunSane: `SAVE_CONCURRENCY=C` → `C` `Entity.save()` konkuren.
- Baca (`load`/`filter`) adalah query tunggal → ~datar terhadap `C`.

Membuat database sekali (bila belum ada):

```sh
psql postgres://postgres:postgres@localhost:5432/postgres \
  -c 'CREATE DATABASE arke_bench' -c 'CREATE DATABASE bunsane_bench'
```

Menjalankan tiap sisi manual:

```sh
# arke (Rust)
DATABASE_URL=postgres://postgres:postgres@localhost:5432/arke_bench \
  cargo run --release --manifest-path arke/Cargo.toml -- --n 20000 --iters 5

# BunSane (Bun) — catatan: env-nya DB_CONNECTION_URL, BUKAN DATABASE_URL
cd bunsane && DB_CONNECTION_URL=postgres://postgres:postgres@localhost:5432/bunsane_bench \
  BUNSANE_DEFAULT_QUERY_LIMIT=100000000 LOG_LEVEL=silent \
  bun bench.ts --n 20000 --iters 5
```

## Beban kerja (identik kedua sisi, N entity ber-`(Position, Health)`)

| workload | arke-postgres | BunSane |
|---|---|---|
| `save` | INSERT per-entity, `C` transaksi konkuren (`--concurrency`) | `Entity.save()` per entity, `C` konkuren (`SAVE_CONCURRENCY`) |
| `load` | `PgStore::load` — muat seluruh state | `Query().with(...).eagerLoad(...).exec()` |
| `filter` | `load_where::<Health>("hp < 20")` — memuat **semua** komponen | `Query().with(Health, {filters:[hp<20]})` — hanya Health |
| `filter_only` | `query::<Health>().filter(hp.lt(20)).only::<Health>()` — hanya Health (**apel-ke-apel** dengan `filter` BunSane) | — |
| `incremental` | `UPDATE cmp_health` ~10% entity, `C` konkuren | `set()` + `save()` ~10% entity, `C` konkuren |

## Membaca hasil — PENTING

- **Micro-benchmark, satu mesin, lintas-bahasa** → angka **RELATIF**, bukan absolut.
  Keduanya didominasi round-trip ke Postgres yang sama; yang diukur adalah overhead
  lapisan klien + pola query, bukan compute murni.
- **Konkurensi tulis = dimensi multi-core.** Tak satu sisi pun CPU-bound; "multi-
  core" praktis = berapa transaksi tulis konkuren → berapa backend Postgres
  paralel. Kedua sisi kini punya jalur tulis konkuren yg setara (arke
  `--concurrency C`, BunSane `SAVE_CONCURRENCY=C`) → pakai `sweep.sh` utk kurva
  scaling apel-ke-apel, bukan satu titik. `C=1` = sekuensial murni.
- **Model skema beda** memengaruhi hasil: arke = kolom typed (scan sempit,
  filter ber-indeks btree biasa); BunSane = JSONB + partition-pruning per
  `type_id` (filter via indeks ekspresi `data->>'field'`).
- **BunSane storage-layer saja** — GraphQL/HTTP tidak diukur (sesuai desain).

## Contoh keluaran (spesifik-mesin, jangan dikutip sbg klaim absolut)

Sweep konkurensi, **N=20000, iters=5**, Ryzen 5 8645HS (12 core), Postgres 17
lokal — ms rata-rata (konkurensi tulis disamakan kedua sisi):

```
  save                   C=1       C=4      C=8     C=16    scaling
  arke-postgres      26660.0   12351.2   6783.3   3409.3     7.82×
  bunsane            37093.0   15191.1   8249.2   5007.5     7.41×

  incremental            C=1       C=4      C=8     C=16    scaling
  arke-postgres       2918.3    1062.0    749.7    287.5    10.15×
  bunsane             3863.3    1398.5    885.8    448.8     8.61×

  load        (query tunggal, ~datar)   arke ≈ 49-56 ms   bunsane ≈ 104-109 ms
  filter      (query tunggal, ~datar)   arke ≈ 12-14 ms   bunsane ≈ 33 ms
  filter_only (query tunggal, ~datar)   arke ≈  8-12 ms   (bunsane: = `filter`)
```

Bacaan:
- **Tulis** (`save`/`incremental`): keduanya skala baik dgn konkurensi. arke lebih
  cepat di **tiap** level, & scaling lebih tinggi (`save` 7.8× vs 7.4×,
  `incremental` 10.2× vs 8.6×); di C=16 `save` 3409 vs 5008 ms.
- **Baca** (`load`/`filter`): datar terhadap konkurensi (query tunggal), arke ~2×
  (`load`) dan ~2.5× (`filter`) lebih cepat — overhead per-op lebih rendah
  (kolom typed vs JSONB) — **padahal `filter` arke memuat semua komponen**,
  sedangkan `filter` BunSane hanya Health. `filter_only` (hidrasi selektif
  `only::<Health>()`, apel-ke-apel) ≈ 8-12 ms → **~3-4× lebih cepat** dari BunSane.

## Struktur

```
benchmarks-pg/
  arke/            crate Rust standalone (di-exclude dari workspace inti)
  bunsane/         proyek Bun (bench.ts + node_modules)
  compare.ts       gabung 2 JSON → tabel perbandingan (1 level)
  sweep_report.ts  susun hasil sweep → tabel scaling
  run.sh           orkestrator 1-level: build + run + compare
  sweep.sh         orkestrator sweep konkurensi: build + run tiap C + report
```
