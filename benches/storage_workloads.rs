//! Benchmark beban-kerja penyimpanan arke (RN-0002).
//!
//! **Harness 0-dependensi**: `harness = false` di `Cargo.toml` → `main()` ini
//! yang berjalan, memakai [`std::time::Instant`] + [`std::hint::black_box`] —
//! tanpa `criterion` (menjaga STD-0003, core standalone). Urutan akan-acak
//! memakai LCG deterministik (tanpa crate `rand`) → hasil reproducible.
//!
//! Menjalankan:  `cargo bench --bench storage_workloads`
//! Smoke cepat:  `ARKE_BENCH_QUICK=1 cargo bench --bench storage_workloads`
//!
//! Lima beban dipilih untuk **membedakan** penyimpanan archetype (kini) dari
//! sparse-set hybrid (kandidat, RN-0002):
//!
//! - **W1** iterasi satu-komponen (padat) — dasar.
//! - **W2** query dua-komponen — keunggulan archetype (kolom kolokasi).
//! - **W3** akses acak per-entity (`get`) — keunggulan teoretis sparse-set (O(1)).
//! - **W4** churn struktural (insert/remove → pindah archetype) — kelemahan archetype.
//! - **W5** iterasi terfragmentasi (banyak archetype) — uji fragmentasi + query-cache.
//!
//! Tiga beban tambahan (audit 0.7.0) menjaga jalur struktural & paralel:
//!
//! - **W6** churn di world ber-64-archetype — resolusi archetype tujuan (indeks hash).
//! - **W7** overhead `run_parallel` 16 sistem kecil — kolam thread persisten.
//! - **W8** `par_for_each` terfragmentasi — pekerja terbatas di kolam.

use std::hint::black_box;
use std::time::Instant;

use arke::{QueryData, World};

// `y`/`z` memberi komponen ukuran realistis (12 byte); hanya `x` dibaca hot-loop.
#[derive(Clone, Copy)]
#[allow(dead_code)]
struct Position {
    x: f32,
    y: f32,
    z: f32,
}
#[derive(Clone, Copy)]
#[allow(dead_code)]
struct Velocity {
    x: f32,
    y: f32,
    z: f32,
}
struct Tag;
// Penanda untuk fragmentasi W5 (tipe berbeda → archetype berbeda).
struct M0;
struct M1;
struct M2;
struct M3;

/// LCG deterministik (Numerical Recipes) — pengganti `rand`, hasil reproducible.
struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed)
    }
    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// Harness: warmup lalu ukur, laporkan ns/op & juta-op/detik.
///
/// Mode **check** (`ARKE_BENCH_CHECK`, RFC/regresi-guard): tiap workload
/// dibandingkan dengan **ceiling** ns/op; melampaui → gagal (exit ≠ 0). Ceiling
/// **longgar** (≥~5× baseline lokal) — hanya menangkap regresi **besar/
/// algoritmik** (mis. loop lockstep balik, iterasi O(archetype×baris)), bukan
/// fluktuasi kecil di runner CI yang bervariasi & bising.
struct Bench {
    quick: bool,
    check: bool,
    failed: std::cell::Cell<u32>,
}
impl Bench {
    fn size(&self, full: usize, quick: usize) -> usize {
        if self.quick { quick } else { full }
    }

    /// Ukur `f` (mengembalikan akumulator agar tak ter-optimasi habis). `ops`
    /// = jumlah operasi logis per pemanggilan `f`. `ceiling` = batas ns/op (mode
    /// check); dilewati → gagal. Di mode check diambil **min beberapa pass** agar
    /// robust terhadap spike penjadwalan.
    fn run(&self, name: &str, ops: u64, ceiling: f64, mut f: impl FnMut() -> u64) {
        let (warmup, iters) = if self.quick { (1, 3) } else { (3, 25) };
        for _ in 0..warmup {
            black_box(f());
        }
        let passes = if self.check { 5 } else { 1 };
        let mut best = f64::INFINITY;
        for _ in 0..passes {
            let start = Instant::now();
            let mut acc = 0u64;
            for _ in 0..iters {
                acc = acc.wrapping_add(f());
            }
            let elapsed = start.elapsed();
            black_box(acc);
            let ns_per_op = elapsed.as_nanos() as f64 / (ops * iters as u64) as f64;
            best = best.min(ns_per_op);
        }

        if self.check {
            let status = if best > ceiling { "FAIL" } else { "ok" };
            if best > ceiling {
                self.failed.set(self.failed.get() + 1);
            }
            println!("{name:<44} {best:>9.2} ns/op  (ceiling {ceiling:.0}) [{status}]");
        } else {
            let mops = 1000.0 / best;
            println!("{name:<44} {best:>9.2} ns/op  {mops:>8.1} Mop/s");
        }
    }
}

/// W1: iterasi satu-komponen (`&mut Position`) atas world padat.
fn w1_iter_single(b: &Bench) {
    let n = b.size(100_000, 1_000);
    let mut world = World::new();
    for i in 0..n {
        let e = world.spawn();
        world.insert(
            e,
            Position {
                x: i as f32,
                y: 0.0,
                z: 0.0,
            },
        );
    }
    b.run("W1 iter single (&mut Position)", n as u64, 8.0, || {
        let mut sum = 0u64;
        for p in world.query_mut::<Position>() {
            p.x += 1.0;
            sum = sum.wrapping_add(p.x as u64);
        }
        sum
    });
}

/// W2: query dua-komponen (`&Position, &mut Velocity`) — kolom kolokasi.
fn w2_query_two(b: &Bench) {
    let n = b.size(100_000, 1_000);
    let mut world = World::new();
    for i in 0..n {
        let e = world.spawn();
        world.insert(
            e,
            Position {
                x: i as f32,
                y: 0.0,
                z: 0.0,
            },
        );
        world.insert(
            e,
            Velocity {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
        );
    }
    b.run("W2 query two (&Pos, &mut Vel)", n as u64, 8.0, || {
        let mut sum = 0u64;
        <(&Position, &mut Velocity)>::each(&mut world, |(p, v)| {
            v.x += p.x;
            sum = sum.wrapping_add(v.x as u64);
        });
        sum
    });
}

/// W3: akses acak per-entity (`get::<Position>`) dalam urutan deterministik.
fn w3_random_get(b: &Bench) {
    let n = b.size(100_000, 1_000);
    let mut world = World::new();
    let mut entities = Vec::with_capacity(n);
    for i in 0..n {
        let e = world.spawn();
        world.insert(
            e,
            Position {
                x: i as f32,
                y: 0.0,
                z: 0.0,
            },
        );
        entities.push(e);
    }
    let mut lcg = Lcg::new(0x9E3779B97F4A7C15);
    let order: Vec<usize> = (0..n).map(|_| lcg.below(n)).collect();

    b.run("W3 random get::<Position>", n as u64, 80.0, || {
        let mut sum = 0u64;
        for &idx in &order {
            if let Some(p) = world.get::<Position>(entities[idx]) {
                sum = sum.wrapping_add(p.x.to_bits() as u64);
            }
        }
        sum
    });
}

/// W4: churn struktural — insert+remove `Tag` (round-trip pindah archetype).
fn w4_churn(b: &Bench) {
    let n = b.size(10_000, 200);
    let mut world = World::new();
    let mut entities = Vec::with_capacity(n);
    for i in 0..n {
        let e = world.spawn();
        world.insert(
            e,
            Position {
                x: i as f32,
                y: 0.0,
                z: 0.0,
            },
        );
        entities.push(e);
    }
    // Tiap iter: 2n pindah archetype (Position → {Position,Tag} → Position).
    b.run("W4 churn insert+remove Tag", (n * 2) as u64, 150.0, || {
        let mut sum = 0u64;
        for &e in &entities {
            world.insert(e, Tag);
            world.remove::<Tag>(e);
            sum = sum.wrapping_add(1);
        }
        sum
    });
}

/// W5: iterasi `Position` terfragmentasi di 16 archetype (subset penanda).
fn w5_fragmented(b: &Bench) {
    let n = b.size(100_000, 1_000);
    let mut world = World::new();
    for i in 0..n {
        let e = world.spawn();
        world.insert(
            e,
            Position {
                x: i as f32,
                y: 0.0,
                z: 0.0,
            },
        );
        let bits = i % 16; // subset {M0,M1,M2,M3} → hingga 16 archetype.
        if bits & 1 != 0 {
            world.insert(e, M0);
        }
        if bits & 2 != 0 {
            world.insert(e, M1);
        }
        if bits & 4 != 0 {
            world.insert(e, M2);
        }
        if bits & 8 != 0 {
            world.insert(e, M3);
        }
    }
    b.run(
        "W5 fragmented iter Position (16 arch)",
        n as u64,
        8.0,
        || {
            let mut sum = 0u64;
            for p in world.query::<Position>() {
                sum = sum.wrapping_add(p.x as u64);
            }
            sum
        },
    );
}

/// W6: churn struktural di world **ber-banyak archetype** (64): biaya resolusi
/// archetype tujuan (`find_or_create_archetype`) — linear-scan vs map.
fn w6_churn_many_archetypes(b: &Bench) {
    let n = b.size(10_000, 200);
    let mut world = World::new();
    // 64 archetype: subset {M0..M3} × {Tag} × {Velocity} → 4 bit + 2 bit.
    for i in 0..64usize {
        let e = world.spawn();
        world.insert(
            e,
            Position {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
        );
        if i & 1 != 0 {
            world.insert(e, M0);
        }
        if i & 2 != 0 {
            world.insert(e, M1);
        }
        if i & 4 != 0 {
            world.insert(e, M2);
        }
        if i & 8 != 0 {
            world.insert(e, M3);
        }
        if i & 16 != 0 {
            world.insert(e, Tag);
        }
        if i & 32 != 0 {
            world.insert(
                e,
                Velocity {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
            );
        }
    }
    // Entity churn hidup di archetype {Position, M0..M3} (dibuat belakangan).
    let mut entities = Vec::with_capacity(n);
    for i in 0..n {
        let e = world.spawn_bundle((
            Position {
                x: i as f32,
                y: 0.0,
                z: 0.0,
            },
            M0,
            M1,
            M2,
            M3,
        ));
        entities.push(e);
    }
    b.run(
        "W6 churn insert+remove Tag (64 arch)",
        (n * 2) as u64,
        300.0,
        || {
            let mut sum = 0u64;
            for &e in &entities {
                world.insert(e, Tag);
                world.remove::<Tag>(e);
                sum = sum.wrapping_add(1);
            }
            sum
        },
    );
}

/// W7: overhead eksekutor `run_parallel` — 16 sistem kecil (8 pasang
/// berkonflik, tiap pasang disjoint dari lainnya) atas world kecil; ns per
/// **sistem** ≈ biaya penjadwalan/thread, bukan kerja sistem.
fn w7_run_parallel_overhead(b: &Bench) {
    use arke::{Schedule, System};
    struct C0(u64);
    struct C1(u64);
    struct C2(u64);
    struct C3(u64);
    struct C4(u64);
    struct C5(u64);
    struct C6(u64);
    struct C7(u64);
    let mut world = World::new();
    for _ in 0..b.size(256, 16) {
        world.spawn_bundle((C0(0), C1(0), C2(0), C3(0), C4(0), C5(0), C6(0), C7(0)));
    }
    let mut s = Schedule::new();
    macro_rules! pair {
        ($t:ident) => {
            s.add(System::each::<&mut $t>(|c| c.0 = c.0.wrapping_add(1)));
            s.add(System::each::<&$t>(|c| {
                black_box(c.0);
            }));
        };
    }
    pair!(C0);
    pair!(C1);
    pair!(C2);
    pair!(C3);
    pair!(C4);
    pair!(C5);
    pair!(C6);
    pair!(C7);
    b.run(
        "W7 run_parallel 16 sistem (per sistem)",
        16,
        8_000.0,
        || {
            s.run_parallel(&mut world);
            1
        },
    );
}

/// W8: `par_for_each` atas world terfragmentasi (16 archetype) — biaya spawn
/// thread per chunk×archetype vs pekerja terbatas.
fn w8_par_for_each_fragmented(b: &Bench) {
    let n = b.size(100_000, 1_000);
    let mut world = World::new();
    for i in 0..n {
        let e = world.spawn();
        world.insert(
            e,
            Position {
                x: i as f32,
                y: 0.0,
                z: 0.0,
            },
        );
        let bits = i % 16;
        if bits & 1 != 0 {
            world.insert(e, M0);
        }
        if bits & 2 != 0 {
            world.insert(e, M1);
        }
        if bits & 4 != 0 {
            world.insert(e, M2);
        }
        if bits & 8 != 0 {
            world.insert(e, M3);
        }
    }
    b.run("W8 par_for_each Position (16 arch)", n as u64, 8.0, || {
        world.par_for_each::<Position>(|p| p.x += 1.0);
        1
    });
}

fn main() {
    let quick = std::env::var_os("ARKE_BENCH_QUICK").is_some();
    let check = std::env::var_os("ARKE_BENCH_CHECK").is_some();
    let b = Bench {
        quick,
        check,
        failed: std::cell::Cell::new(0),
    };
    let mode = if check {
        "  [CHECK/regresi-guard]"
    } else if quick {
        "  [QUICK/smoke]"
    } else {
        ""
    };
    println!("arke storage workloads (RN-0002){mode}");
    println!("{:-<64}", "");
    w1_iter_single(&b);
    w2_query_two(&b);
    w3_random_get(&b);
    w4_churn(&b);
    w5_fragmented(&b);
    w6_churn_many_archetypes(&b);
    w7_run_parallel_overhead(&b);
    w8_par_for_each_fragmented(&b);

    if check && b.failed.get() > 0 {
        eprintln!(
            "\nREGRESI PERFORMA: {} workload melampaui ceiling.",
            b.failed.get()
        );
        std::process::exit(1);
    }
}
