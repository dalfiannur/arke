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
