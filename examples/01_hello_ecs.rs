//! # 01 — Hello, ECS
//!
//! Contoh onboarding paling dasar. Ajari empat gerakan inti arke:
//!
//! 1. **Spawn** entity & pasang komponen (`insert` / `spawn_bundle`).
//! 2. **Query** komponen langsung (`query`) & tuple generik (`QueryData::each`).
//! 3. **System + Schedule** — logika berbasis-tipe yang aksesnya *disimpulkan*
//!    dari tipe query (tak ada deklarasi baca/tulis manual).
//! 4. **Get** — baca komponen satu entity lewat handle-nya.
//!
//! Jalankan: `cargo run --example 01_hello_ecs`
//! Contoh ini **memverifikasi dirinya sendiri** lewat `assert_eq!` — bila
//! panik, ada perilaku yang berubah.

#![forbid(unsafe_code)] // Jalur pengguna tak pernah butuh `unsafe` (STD-0004).

use arke::{Schedule, System, World};

#[derive(Debug, PartialEq)]
struct Position(i32, i32);

#[derive(Debug, PartialEq)]
struct Velocity(i32, i32);

fn main() {
    let mut world = World::new();

    // (1) Spawn. `spawn_bundle` memasang beberapa komponen dalam SATU pindah
    //     archetype — lebih murah daripada `insert` berkali-kali (RFC-0022).
    let pemain = world.spawn_bundle((Position(0, 0), Velocity(2, 1)));

    // Bisa juga bertahap: entity yang hanya punya Position (tanpa Velocity).
    let dinding = world.spawn();
    world.insert(dinding, Position(10, 10));

    // (2) Query langsung: berapa entity punya Position?
    let jumlah_ber_posisi = world.query::<Position>().count();
    println!("entity ber-Position: {jumlah_ber_posisi}");
    assert_eq!(jumlah_ber_posisi, 2);

    // (3) System + Schedule. Sistem "gerak" ini menulis Position & membaca
    //     Velocity — akses itu DISIMPULKAN dari tipe `(&Velocity, &mut Position)`.
    //     Hanya entity yang punya KEDUA komponen yang tersentuh: `dinding`
    //     (tanpa Velocity) otomatis terlewati.
    let mut schedule = Schedule::new();
    schedule.add(System::each::<(&Velocity, &mut Position)>(|(v, p)| {
        p.0 += v.0;
        p.1 += v.1;
    }));

    // Jalankan tiga "tick".
    for _ in 0..3 {
        schedule.run(&mut world);
    }

    // (4) Get: baca keadaan satu entity via handle-nya.
    let pos_pemain = world.get::<Position>(pemain).unwrap();
    let pos_dinding = world.get::<Position>(dinding).unwrap();
    println!("pemain  -> {pos_pemain:?}"); // bergerak 3× (2,1)
    println!("dinding -> {pos_dinding:?}"); // diam (tak punya Velocity)

    // Verifikasi diri: pemain = (0,0) + 3×(2,1) = (6,3); dinding tak berubah.
    assert_eq!(pos_pemain, &Position(6, 3));
    assert_eq!(pos_dinding, &Position(10, 10));

    println!("OK — fundamental ECS bekerja.");
}
