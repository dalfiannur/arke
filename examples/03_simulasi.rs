//! # 03 — Simulasi mini end-to-end
//!
//! Menyatukan semua ke satu loop simulasi kecil namun lengkap: **hujan meteor**.
//! Tiap tick, meteor jatuh, umurnya berkurang, dan yang habis umur **menghapus
//! dirinya sendiri**. Menampilkan fitur yang tak muncul di contoh sebelumnya:
//!
//! - **Resource** — keadaan global (`Tick`) yang dibaca/ditulis sistem.
//! - **Command buffer** — mutasi *struktural* (despawn) tak boleh terjadi saat
//!   query sedang berjalan; ia direkam & di-*apply* aman di akhir run.
//! - **`Entity` sebagai term query** — sistem tahu entity mana yang sedang
//!   diproses, memungkinkan pola *despawn-self* (RFC-0020).
//!
//! Semua deterministik: keadaan akhir dipastikan lewat `assert_eq!`.
//!
//! Jalankan: `cargo run --example 03_simulasi`

#![forbid(unsafe_code)]

use arke::{Entity, Schedule, System, World};

#[derive(Debug)]
struct Position(i32, i32);
#[derive(Debug)]
struct Velocity(i32, i32);
/// Sisa umur meteor dalam tick. Nol → dihapus.
#[derive(Debug)]
struct Life(u32);

/// Resource global: nomor tick berjalan.
struct Tick(u32);

const TICKS: u32 = 8;

fn main() {
    let mut world = World::new();
    world.insert_resource(Tick(0));

    // Sebar 5 meteor dengan umur berbeda (2..=6) via bundle.
    for i in 0..5u32 {
        world.spawn_bundle((
            Position(i as i32 * 10, 100),
            Velocity(0, -(i as i32 + 1)), // makin cepat makin ke bawah
            Life(2 + i),                  // umur 2,3,4,5,6
        ));
    }

    let mut schedule = Schedule::new();

    // Sistem 1 — maju tick (resource, serial). Sekali per run.
    schedule.add(System::resource::<Tick>(|t| t.0 += 1));

    // Sistem 2 — gerak: Position += Velocity (paralel-mampu).
    schedule.add(System::each::<(&Velocity, &mut Position)>(|(v, p)| {
        p.0 += v.0;
        p.1 += v.1;
    }));

    // Sistem 3 — luruh umur: Life -= 1 (paralel-mampu).
    schedule.add(System::each::<&mut Life>(|life| {
        life.0 = life.0.saturating_sub(1);
    }));

    // Sistem 4 — despawn-self bila umur habis. Mutasi struktural direkam ke
    //   CommandBuffer lewat `each_cmd` & di-apply aman di akhir run.
    schedule.add(System::each_cmd::<(Entity, &Life)>(|(e, life), cmds| {
        if life.0 == 0 {
            cmds.despawn(e);
        }
    }));

    // Loop simulasi.
    for _ in 0..TICKS {
        schedule.run(&mut world);
        let tick = world.resource::<Tick>().unwrap().0;
        let hidup = world.query::<Life>().count();
        println!("tick {tick}: {hidup} meteor tersisa");
    }

    // --- Verifikasi diri (deterministik) ---
    // Umur awal 2,3,4,5,6. Urutan per tick: luruh DULU lalu despawn saat 0.
    //   Meteor umur-u dihapus di akhir tick ke-u. Setelah 8 tick, semua (u<=6)
    //   sudah lenyap → 0 tersisa.
    let tersisa = world.query::<Life>().count();
    assert_eq!(tersisa, 0, "semua meteor mestinya sudah meluruh");
    assert_eq!(world.resource::<Tick>().unwrap().0, TICKS);

    println!("OK — simulasi selesai: {tersisa} meteor, {TICKS} tick.");
}
