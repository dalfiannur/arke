//! Bukti STD-0006: eksekusi paralel menghasilkan keadaan akhir yang identik
//! dengan eksekusi serial, untuk operasi per-elemen independen.
//!
//! Dua `World` dibangun identik. Yang satu ditransformasi secara serial
//! (`query_mut`), yang lain secara paralel (`par_for_each`). Snapshot query
//! keduanya harus sama persis.

#![forbid(unsafe_code)]

use rust_ecs::World;

#[derive(PartialEq, Debug)]
struct N(u64);

fn build() -> World {
    let mut world = World::new();
    for i in 0..10_000u64 {
        let e = world.spawn();
        world.insert(e, N(i));
    }
    world
}

/// Transformasi per-elemen yang murni (tak bergantung elemen lain).
fn transform(x: u64) -> u64 {
    x.wrapping_mul(2_654_435_761).rotate_left(13) ^ 0x9e37_79b9
}

#[test]
fn paralel_setara_serial() {
    let mut serial = build();
    for n in serial.query_mut::<N>() {
        n.0 = transform(n.0);
    }

    let mut parallel = build();
    parallel.par_for_each::<N>(|n| n.0 = transform(n.0));

    let s: Vec<u64> = serial.query::<N>().map(|n| n.0).collect();
    let p: Vec<u64> = parallel.query::<N>().map(|n| n.0).collect();
    assert_eq!(s, p);
}

#[test]
fn par_for_each_deterministik_antar_run() {
    fn run() -> Vec<u64> {
        let mut world = build();
        world.par_for_each::<N>(|n| n.0 = transform(n.0));
        world.query::<N>().map(|n| n.0).collect()
    }
    assert_eq!(run(), run());
}
