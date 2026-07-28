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
| `filter` | `load_where::<Health>("hp < 20")` | `Query().with(Health, {filters:[hp<20]})` |
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

Sweep konkurensi, N=8000, Ryzen 5 8645HS (12 core), Postgres lokal — ms rata-rata:

```
  save                   C=1      C=4      C=8     C=16    scaling
  arke-postgres       8192.2   3265.2   1708.5   1165.4     7.03×
  bunsane            10991.3   4854.3   2894.7   2872.6     3.83×

  incremental            C=1      C=4      C=8     C=16    scaling
  arke-postgres        663.3    273.6    149.7    383.1*    1.73×
  bunsane             1183.0    425.2    272.3    305.2     3.88×

  load  (query tunggal, ~datar)     arke ≈ 21 ms   bunsane ≈ 50 ms
  filter (query tunggal, ~datar)    arke ≈ 5 ms    bunsane ≈ 13 ms
```

Bacaan:
- **Tulis** (`save`/`incremental`): keduanya skala baik dgn konkurensi. arke lebih
  cepat di tiap level & scaling `save` lebih tinggi (7× vs 3.8×); BunSane plateau
  setelah C≈8. (`*` C=16 arke `incremental` = noise ukur; puncak di C=8 ≈ 4.4×.)
- **Baca** (`load`/`filter`): datar terhadap konkurensi (query tunggal), arke ~2×
  lebih cepat — overhead per-op lebih rendah (kolom typed vs JSONB).

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
