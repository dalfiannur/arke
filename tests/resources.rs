//! Resources sebagai parameter sistem end-to-end (RFC-0010), API publik, di
//! bawah `forbid(unsafe_code)`.

#![forbid(unsafe_code)]

use arke::{Schedule, System, World};

struct DeltaTime(i64);
#[derive(PartialEq, Debug)]
struct Position(i64);
#[derive(PartialEq, Debug)]
struct Velocity(i64);

#[test]
fn sistem_gerak_membaca_resource_delta_saat_iterasi_query() {
    let mut world = World::new();
    world.insert_resource(DeltaTime(2));

    let e = world.spawn();
    world.insert(e, Position(0));
    world.insert(e, Velocity(5));

    let mut schedule = Schedule::new();
    // position += velocity * dt — membaca resource DeltaTime sambil mengiterasi
    // tuple query (&Velocity, &mut Position).
    schedule.add(System::each_res::<DeltaTime, (&Velocity, &mut Position)>(
        |dt, (v, p)| p.0 += v.0 * dt.0,
    ));

    schedule.run(&mut world);
    schedule.run(&mut world);

    assert_eq!(world.get::<Position>(e), Some(&Position(20))); // 2 run × (5×2)
    // Resource tetap ada setelah dipakai berkali-kali.
    assert!(world.contains_resource::<DeltaTime>());
}

/// `System::each_res` melepas resource sementara selama iterasi; panic di
/// dalam closure tak boleh menghilangkan resource dari World.
#[test]
fn each_res_mengembalikan_resource_walau_closure_panic() {
    struct Cfg(i32);
    struct N;
    let mut w = World::new();
    w.insert_resource(Cfg(5));
    let e = w.spawn();
    w.insert(e, N);
    let mut s = Schedule::new();
    s.add(System::each_res::<Cfg, &N>(|_cfg, _n| panic!("boom")));
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| s.run(&mut w)));
    assert!(r.is_err());
    assert_eq!(w.resource::<Cfg>().map(|c| c.0), Some(5));
}
