//! Bukti STD-0004: seluruh API publik dapat dipakai dari kode pengguna yang
//! **melarang `unsafe` sepenuhnya**. Bila berkas ini gagal dikompilasi, invarian
//! "ergonomis = cepat" (jalur pengguna tanpa `unsafe`) telah dilanggar.

#![forbid(unsafe_code)]

use arke::{CommandBuffer, World};

#[derive(Debug, PartialEq)]
struct Position(i32, i32);
#[derive(Debug, PartialEq)]
struct Velocity(i32, i32);

fn main() {
    let mut world = World::new();

    let a = world.spawn();
    world.insert(a, Position(0, 0));
    world.insert(a, Velocity(1, 2));

    let b = world.spawn();
    world.insert(b, Position(10, 10)); // tanpa Velocity

    // "Sistem gerak": position += velocity, hanya untuk entity yang punya keduanya.
    for (vel, pos) in world.query_pair::<Velocity, Position>() {
        pos.0 += vel.0;
        pos.1 += vel.1;
    }

    // Baca semua Position.
    let total: i32 = world.query::<Position>().map(|p| p.0 + p.1).sum();
    assert_eq!(world.get::<Position>(a), Some(&Position(1, 2)));
    assert_eq!(world.get::<Position>(b), Some(&Position(10, 10)));
    assert_eq!(total, 3 + 20);

    // Hapus komponen dan entity.
    assert_eq!(world.remove::<Velocity>(a), Some(Velocity(1, 2)));
    world.despawn(b);
    assert!(world.contains(a));
    assert!(!world.contains(b));

    // Mutasi struktural tertunda via command buffer (RFC-0019).
    let mut cmd = CommandBuffer::new();
    cmd.spawn().insert(Position(5, 5)).insert(Velocity(1, 1));
    cmd.despawn(a);
    cmd.apply(&mut world);
    assert!(!world.contains(a)); // a ter-despawn (tertunda)
    assert_eq!(world.query::<Position>().count(), 1); // hanya entity baru

    println!("no_unsafe: OK (total={total})");
}
