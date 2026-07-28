//! Benchmark **kompetitif**: arke vs hecs vs bevy_ecs pada beban ECS inti.
//!
//! Harness sama (hand-rolled `std::time::Instant` + `black_box`) membungkus tiap
//! pustaka → apel-ke-apel. Jalankan: `cargo run --release` (dari `benchmarks/`).
//!
//! **Peringatan:** micro-benchmark, satu mesin — angka **relatif**, bukan absolut.

use std::hint::black_box;
use std::time::Instant;

use bevy_ecs::prelude::Component;

#[derive(Component, Clone, Copy)]
struct Position(f32);
#[derive(Component, Clone, Copy)]
struct Velocity(f32);

/// LCG deterministik untuk urutan akses acak (tanpa `rand`).
struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    fn below(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % n
    }
}

/// Ukur `f` (kembalikan akumulator agar tak ter-optimasi). `ops` = op/panggilan.
fn bench(label: &str, ops: u64, mut f: impl FnMut() -> u64) {
    for _ in 0..3 {
        black_box(f());
    }
    let iters = 25u64;
    let start = Instant::now();
    let mut acc = 0u64;
    for _ in 0..iters {
        acc = acc.wrapping_add(f());
    }
    let elapsed = start.elapsed();
    black_box(acc);
    let ns_per_op = elapsed.as_nanos() as f64 / (ops * iters) as f64;
    println!("  {label:<32} {ns_per_op:>9.2} ns/op");
}

const N: usize = 100_000;

fn main() {
    println!("arke vs hecs vs bevy_ecs — N = {N}\n");

    println!("iter2: for each entity, position += velocity");
    arke_iter2();
    hecs_iter2();
    bevy_iter2();

    println!("\nspawn: buat {N} entity ber-(Position, Velocity)");
    arke_spawn();
    hecs_spawn();
    bevy_spawn();

    println!("\nget: akses acak position via entity handle");
    arke_get();
    hecs_get();
    bevy_get();
}

// ---------- iter2 ----------

fn arke_iter2() {
    use arke::{QueryData, QueryState, World};
    let mut w = World::new();
    for i in 0..N {
        w.spawn_bundle((Position(i as f32), Velocity(1.0)));
    }
    // Query cache persisten (seperti System::each) — adil vs query bevy yang di-cache.
    let mut state = QueryState::default();
    bench("arke", N as u64, || {
        let mut sum = 0u64;
        <(&mut Position, &Velocity)>::each_cached::<()>(&w, &mut state, |(p, v)| {
            p.0 += v.0;
            sum = sum.wrapping_add(p.0 as u64);
        });
        sum
    });
}

fn hecs_iter2() {
    let mut w = hecs::World::new();
    for i in 0..N {
        w.spawn((Position(i as f32), Velocity(1.0)));
    }
    bench("hecs", N as u64, || {
        let mut sum = 0u64;
        for (p, v) in w.query_mut::<(&mut Position, &Velocity)>() {
            p.0 += v.0;
            sum = sum.wrapping_add(p.0 as u64);
        }
        sum
    });
}

fn bevy_iter2() {
    let mut w = bevy_ecs::world::World::new();
    for i in 0..N {
        w.spawn((Position(i as f32), Velocity(1.0)));
    }
    let mut q = w.query::<(&mut Position, &Velocity)>();
    bench("bevy_ecs", N as u64, || {
        let mut sum = 0u64;
        for (mut p, v) in q.iter_mut(&mut w) {
            p.0 += v.0;
            sum = sum.wrapping_add(p.0 as u64);
        }
        sum
    });
}

// ---------- spawn ----------

fn arke_spawn() {
    use arke::World;
    bench("arke", N as u64, || {
        let mut w = World::new();
        for i in 0..N {
            w.spawn_bundle((Position(i as f32), Velocity(1.0)));
        }
        black_box(&w);
        N as u64
    });
}

fn hecs_spawn() {
    bench("hecs", N as u64, || {
        let mut w = hecs::World::new();
        for i in 0..N {
            w.spawn((Position(i as f32), Velocity(1.0)));
        }
        black_box(&w);
        N as u64
    });
}

fn bevy_spawn() {
    bench("bevy_ecs", N as u64, || {
        let mut w = bevy_ecs::world::World::new();
        for i in 0..N {
            w.spawn((Position(i as f32), Velocity(1.0)));
        }
        black_box(&w);
        N as u64
    });
}

// ---------- get (akses acak) ----------

fn order() -> Vec<usize> {
    let mut lcg = Lcg::new(0x1234);
    (0..N).map(|_| lcg.below(N)).collect()
}

fn arke_get() {
    use arke::{Entity, World};
    let mut w = World::new();
    let mut ents: Vec<Entity> = Vec::with_capacity(N);
    for i in 0..N {
        let e = w.spawn();
        w.insert(e, Position(i as f32));
        ents.push(e);
    }
    let ord = order();
    bench("arke", N as u64, || {
        let mut sum = 0u64;
        for &i in &ord {
            if let Some(p) = w.get::<Position>(ents[i]) {
                sum = sum.wrapping_add(p.0 as u64);
            }
        }
        sum
    });
}

fn hecs_get() {
    let mut w = hecs::World::new();
    let mut ents: Vec<hecs::Entity> = Vec::with_capacity(N);
    for i in 0..N {
        ents.push(w.spawn((Position(i as f32),)));
    }
    let ord = order();
    bench("hecs", N as u64, || {
        let mut sum = 0u64;
        for &i in &ord {
            if let Ok(p) = w.get::<&Position>(ents[i]) {
                sum = sum.wrapping_add(p.0 as u64);
            }
        }
        sum
    });
}

fn bevy_get() {
    let mut w = bevy_ecs::world::World::new();
    let mut ents: Vec<bevy_ecs::entity::Entity> = Vec::with_capacity(N);
    for i in 0..N {
        ents.push(w.spawn((Position(i as f32),)).id());
    }
    let ord = order();
    bench("bevy_ecs", N as u64, || {
        let mut sum = 0u64;
        for &i in &ord {
            if let Some(p) = w.get::<Position>(ents[i]) {
                sum = sum.wrapping_add(p.0 as u64);
            }
        }
        sum
    });
}
