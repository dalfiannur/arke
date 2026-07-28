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
//! Harness sama dgn sisi BunSane: wall-clock, beberapa iterasi, lapor
//! ms & entity/detik. Output JSON (`--json`) agar bisa digabung skrip pembanding.
//!
//! Jalankan:
//! ```sh
//! DATABASE_URL=postgres://postgres:postgres@localhost:5432/arke_bench \
//!   cargo run --release -- --n 20000 --iters 5
//! ```

use std::time::Instant;

use arke::World;
use arke_postgres::{PgComponent, PgStore};

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

/// Bangun `World` berisi `n` entity ber-(Position, Health). hp: 0..100.
fn build_world(n: usize) -> World {
    let mut w = World::new();
    let mut rng = Lcg::new(0xA42E_5EED);
    for i in 0..n {
        let e = w.spawn();
        w.insert(
            e,
            Position {
                x: i as f64,
                y: (n - i) as f64,
            },
        );
        w.insert(
            e,
            Health {
                hp: (rng.next_u32() % 100) as i32,
            },
        );
    }
    w
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
/// `Vec<f64>` ms. Blok dijalankan sekuensial → pinjaman `&mut store` aman.
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
    let mut json = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--n" => n = args.next().and_then(|s| s.parse().ok()).unwrap_or(n),
            "--iters" => iters = args.next().and_then(|s| s.parse().ok()).unwrap_or(iters),
            "--json" => json = true,
            _ => {}
        }
    }

    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/arke_bench".into());

    let mut store = PgStore::connect(&url).await?;
    store.register::<Position>().register::<Health>();
    store.migrate().await?;

    let world = build_world(n);

    if !json {
        println!("arke-postgres — N = {n}, iters = {iters}\n");
    }
    let mut stats = Vec::new();

    // 1) save: tulis penuh N entity.
    let s = bench!(iters, {
        store.save(&world).await.unwrap();
    });
    stats.push(summarize("save", n, &s));

    // Pastikan DB terisi utk load/query berikutnya.
    store.save(&world).await?;

    // 2) load: scan seluruh state ke World segar.
    let s = bench!(iters, {
        let mut fresh = World::new();
        store.load(&mut fresh).await.unwrap();
    });
    stats.push(summarize("load", n, &s));

    // 3) load_where: query terfilter (hp < 20 ≈ 20% baris).
    let matched = {
        let mut w = World::new();
        store.load_where::<Health>(&mut w, "hp < 20").await?
    };
    let s = bench!(iters, {
        let mut w = World::new();
        store.load_where::<Health>(&mut w, "hp < 20").await.unwrap();
    });
    stats.push(summarize("filter", matched, &s));

    // 4) save_incremental: ubah hp ~10% entity lalu tulis-balik diff.
    // Reset save penuh + muat baseline tiap iterasi agar diff konsisten (≈10%).
    let s = bench!(iters, {
        store.save(&world).await.unwrap();
        let mut w = World::new();
        store.load(&mut w).await.unwrap();
        let mut i = 0usize;
        for h in w.query_mut::<Health>() {
            if i % 10 == 0 {
                h.hp = h.hp.wrapping_add(1);
            }
            i += 1;
        }
        store.save_incremental(&w).await.unwrap();
    });
    stats.push(summarize("incremental", n / 10, &s));

    if json {
        println!("{{");
        println!("  \"engine\": \"arke-postgres\",");
        println!("  \"n\": {n},");
        println!("  \"iters\": {iters},");
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
