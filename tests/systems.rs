//! Sistem berbasis-tipe end-to-end (RFC-0005), lewat API publik dan di bawah
//! `forbid(unsafe_code)` (memperkuat STD-0004).

#![forbid(unsafe_code)]

use rust_ecs::{Schedule, System, World};

#[derive(PartialEq, Debug)]
struct Position(i32);
#[derive(PartialEq, Debug)]
struct Velocity(i32);

#[test]
fn sistem_bertipe_query_tuple_berjalan_dalam_schedule() {
    let mut world = World::new();
    let moving = world.spawn();
    world.insert(moving, Position(0));
    world.insert(moving, Velocity(5));
    let still = world.spawn();
    world.insert(still, Position(100)); // tanpa Velocity → tak tersentuh

    let mut schedule = Schedule::new();
    // Sistem "gerak": posisi += kecepatan. Akses (baca Velocity, tulis Position)
    // disimpulkan otomatis dari tipe query.
    schedule.add(System::each::<(&Velocity, &mut Position)>(|(v, p)| {
        p.0 += v.0;
    }));

    schedule.run(&mut world);
    schedule.run(&mut world);

    assert_eq!(world.get::<Position>(moving), Some(&Position(10)));
    assert_eq!(world.get::<Position>(still), Some(&Position(100)));
}

#[test]
fn dua_sistem_pembaca_berbeda_berbagi_satu_stage() {
    let mut schedule = Schedule::new();
    schedule.add(System::each::<&Position>(|_| {}));
    schedule.add(System::each::<&Velocity>(|_| {}));
    // Akses berbeda dan keduanya membaca → tak konflik → satu stage (dapat paralel).
    assert_eq!(schedule.stages(), vec![vec![0, 1]]);
}
