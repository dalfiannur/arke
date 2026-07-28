//! Benchmark **arke-postgres** (Rust) untuk perbandingan lintas-bahasa vs
//! **BunSane** (TS/Bun). Keduanya EC-store di atas Postgres → yang diukur adalah
//! overhead lapisan klien + pola query, bukan compute murni.
//!
//! Empat beban kerja atas `N` entity ber-`(Position, Health)`:
//!   1. `save`            — tulis penuh N entity ke Postgres
//!   2. `load`            — muat/scan seluruh N entity + komponennya
//!   3. `load_where`      — query terfilter (`Health.hp < 20`)
//!   4. `save_incremental`— tulis-balik hanya entity yang berubah (~10%)
//!
//! **Multi-core (`--concurrency C`).** Tulis (`save`/`incremental`) dijalankan
//! lewat `C` transaksi konkuren (buffer_unordered) di atas pool → `C` backend
//! Postgres paralel, meniru `save()` per-entity konkuren BunSane. `C=1`
//! menyetarai jalur sekuensial `PgStore::save`. Baca (`load`/`filter`) adalah
//! query tunggal → tak bergantung `C` (dilaporkan apa adanya).
//!
//! Jalankan:
//! ```sh
//! DATABASE_URL=postgres://postgres:postgres@localhost:5432/arke_bench \
//!   cargo run --release -- --n 20000 --iters 5 --concurrency 8
//! ```

use std::time::Instant;

use arke::World;
use arke_postgres::{PgComponent, PgStore};
use futures::stream::{self, StreamExt};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

#[derive(PgComponent, Clone, Copy)]
struct Position {
    x: f64,
    y: f64,
}

#[derive(PgComponent, Clone, Copy)]
struct Health {
    #[pg(index)]
    hp: i32,
}

/// Satu baris entity siap-tulis (bebas dari `World` → aman dikirim antar-task).
#[derive(Clone, Copy)]
struct Row {
    id: i64,
    x: f64,
    y: f64,
    hp: i32,
}

/// LCG deterministik (tanpa `rand`) — hp tersebar 0..100.
struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
}

fn build_rows(n: usize) -> Vec<Row> {
    let mut rng = Lcg::new(0xA42E_5EED);
    (0..n)
        .map(|i| Row {
            id: i as i64,
            x: i as f64,
            y: (n - i) as f64,
            hp: (rng.next_u32() % 100) as i32,
        })
        .collect()
}

/// Sisipkan satu entity (arke_entities + kedua komponen) dalam 1 transaksi —
/// meniru granularitas `Entity.save()` BunSane.
async fn insert_one(pool: &PgPool, r: &Row) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO arke_entities (entity_id, generation, version) VALUES ($1, 0, 0)")
        .bind(r.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO cmp_position (entity_id, x, y) VALUES ($1, $2, $3)")
        .bind(r.id)
        .bind(r.x)
        .bind(r.y)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO cmp_health (entity_id, hp) VALUES ($1, $2)")
        .bind(r.id)
        .bind(r.hp)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

/// Tulis penuh N entity dgn `c` transaksi konkuren.
async fn save_concurrent(pool: &PgPool, rows: &[Row], c: usize) {
    sqlx::query("TRUNCATE arke_entities CASCADE")
        .execute(pool)
        .await
        .unwrap();
    stream::iter(rows.iter())
        .map(|r| insert_one(pool, r))
        .buffer_unordered(c)
        .for_each(|res| async { res.unwrap() })
        .await;
}

/// Tulis-balik: `UPDATE cmp_health SET hp = hp + 1` utk tiap `id`, `c` konkuren.
async fn update_concurrent(pool: &PgPool, ids: &[i64], c: usize) {
    stream::iter(ids.iter())
        .map(|&id| {
            sqlx::query("UPDATE cmp_health SET hp = hp + 1 WHERE entity_id = $1")
                .bind(id)
                .execute(pool)
        })
        .buffer_unordered(c)
        .for_each(|res| async {
            res.unwrap();
        })
        .await;
}

/// Statistik ringkas satu beban kerja (ms rata-rata + entity/detik).
struct Stat {
    label: &'static str,
    n: usize,
    ms_avg: f64,
    ms_min: f64,
    per_sec: f64,
}

impl Stat {
    fn print_human(&self) {
        println!(
            "  {:<18} {:>9.2} ms  (min {:>7.2})  {:>12.0} ent/s",
            self.label, self.ms_avg, self.ms_min, self.per_sec
        );
    }
    fn print_json(&self, last: bool) {
        print!(
            "    {{\"workload\":{:?},\"n\":{},\"ms_avg\":{:.4},\"ms_min\":{:.4},\"per_sec\":{:.1}}}{}",
            self.label,
            self.n,
            self.ms_avg,
            self.ms_min,
            self.per_sec,
            if last { "\n" } else { ",\n" }
        );
    }
}

/// Ukur blok async `$block` selama `$iters` iterasi (1 warm-up dibuang) →
/// `Vec<f64>` ms. Blok dijalankan sekuensial → pinjaman aman.
macro_rules! bench {
    ($iters:expr, $block:block) => {{
        $block // warm-up: koneksi pool, prepared stmt, cache OS
        let mut samples = Vec::with_capacity($iters);
        for _ in 0..$iters {
            let __t = Instant::now();
            $block
            samples.push(__t.elapsed().as_secs_f64() * 1000.0);
        }
        samples
    }};
}

fn summarize(label: &'static str, n: usize, samples: &[f64]) -> Stat {
    let ms_avg = samples.iter().sum::<f64>() / samples.len() as f64;
    let ms_min = samples.iter().cloned().fold(f64::INFINITY, f64::min);
    Stat {
        label,
        n,
        ms_avg,
        ms_min,
        per_sec: n as f64 / (ms_avg / 1000.0),
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut n = 20_000usize;
    let mut iters = 5usize;
    let mut concurrency = 1usize;
    let mut json = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--n" => n = args.next().and_then(|s| s.parse().ok()).unwrap_or(n),
            "--iters" => iters = args.next().and_then(|s| s.parse().ok()).unwrap_or(iters),
            "--concurrency" => {
                concurrency = args.next().and_then(|s| s.parse().ok()).unwrap_or(concurrency)
            }
            "--json" => json = true,
            _ => {}
        }
    }
    let concurrency = concurrency.max(1);

    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/arke_bench".into());

    // Pool cukup besar utk `concurrency` transaksi paralel (+ margin).
    let pool = PgPoolOptions::new()
        .max_connections((concurrency as u32 + 4).max(8))
        .connect(&url)
        .await?;

    let mut store = PgStore::from_pool(pool.clone());
    store.register::<Position>().register::<Health>();
    store.migrate().await?;

    let rows = build_rows(n);
    let ids_10pct: Vec<i64> = rows.iter().step_by(10).map(|r| r.id).collect();

    if !json {
        println!("arke-postgres — N = {n}, iters = {iters}, concurrency = {concurrency}\n");
    }
    let mut stats = Vec::new();

    // 1) save: tulis penuh N entity (konkuren C).
    let s = bench!(iters, {
        save_concurrent(&pool, &rows, concurrency).await;
    });
    stats.push(summarize("save", n, &s));

    // Pastikan DB terisi utk load/query berikutnya.
    save_concurrent(&pool, &rows, concurrency).await;

    // 2) load: scan seluruh state ke World segar (query tunggal — C-independen).
    let s = bench!(iters, {
        let mut fresh = World::new();
        store.load(&mut fresh).await.unwrap();
    });
    stats.push(summarize("load", n, &s));

    // 3) load_where: query terfilter (hp < 20 ≈ 20% baris; C-independen).
    let matched = {
        let mut w = World::new();
        store.load_where::<Health>(&mut w, "hp < 20").await?
    };
    let s = bench!(iters, {
        let mut w = World::new();
        store.load_where::<Health>(&mut w, "hp < 20").await.unwrap();
    });
    stats.push(summarize("filter", matched, &s));

    // 4) incremental: UPDATE hp pada ~10% entity, `C` konkuren.
    let s = bench!(iters, {
        update_concurrent(&pool, &ids_10pct, concurrency).await;
    });
    stats.push(summarize("incremental", ids_10pct.len(), &s));

    if json {
        println!("{{");
        println!("  \"engine\": \"arke-postgres\",");
        println!("  \"n\": {n},");
        println!("  \"iters\": {iters},");
        println!("  \"concurrency\": {concurrency},");
        println!("  \"results\": [");
        for (i, st) in stats.iter().enumerate() {
            st.print_json(i == stats.len() - 1);
        }
        println!("  ]");
        println!("}}");
    } else {
        for st in &stats {
            st.print_human();
        }
    }

    Ok(())
}
