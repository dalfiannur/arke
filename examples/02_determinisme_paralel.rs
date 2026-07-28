//! # 02 — Determinisme by construction
//!
//! Pembeda utama arke: **hasil yang sama setiap kali, apa pun penjadwalan.**
//! `Schedule::run` (serial) dan `Schedule::run_parallel` (multi-thread) WAJIB
//! menghasilkan keadaan akhir yang identik — bukan "biasanya", tapi *dijamin*
//! oleh konstruksi (STD-0006). Scheduler menjalankan sistem yang aksesnya tak
//! bertumpang-tindih secara paralel; yang bertabrakan diserialkan.
//!
//! Contoh ini membangun DUA world identik, menjalankan satu serial & satu
//! paralel, lalu membuktikan setiap entity berakhir sama.
//!
//! Jalankan: `cargo run --example 02_determinisme_paralel`

#![forbid(unsafe_code)]

use arke::{Entity, Schedule, System, World};

#[derive(Debug, Clone, PartialEq)]
struct Position(i64, i64);
#[derive(Debug, Clone, PartialEq)]
struct Velocity(i64, i64);
#[derive(Debug, Clone, PartialEq)]
struct Energy(i64);

const N: u64 = 2_000;

/// Bangun world deterministik (tanpa RNG) berisi `N` entity, sekaligus
/// mengembalikan handle-nya. Konstruksi identik tiap pemanggilan → handle
/// (indeks+generasi) juga identik antar-world.
fn dunia_awal() -> (World, Vec<Entity>) {
    let mut world = World::new();
    let mut handles = Vec::with_capacity(N as usize);
    for i in 0..N as i64 {
        // Nilai berasal dari indeks → konstruksi identik tiap pemanggilan.
        let e = world.spawn_bundle((
            Position(i, -i),
            Velocity((i % 7) - 3, (i % 5) - 2),
            Energy(100 + (i % 50)),
        ));
        handles.push(e);
    }
    (world, handles)
}

/// Tiga sistem dengan akses SALING-LEPAS:
/// - gerak:  baca Velocity, tulis Position
/// - regen:  tulis Energy
/// - drift:  tulis Velocity
///
/// Karena himpunan tulis mereka tak bertumpang-tindih, `run_parallel`
/// menjalankannya di thread berbeda — namun hasilnya sama dengan serial.
fn schedule_baru() -> Schedule {
    let mut s = Schedule::new();
    s.add(System::each::<(&Velocity, &mut Position)>(|(v, p)| {
        p.0 += v.0;
        p.1 += v.1;
    }));
    s.add(System::each::<&mut Energy>(|e| {
        e.0 = (e.0 + 3).min(200);
    }));
    s.add(System::each::<&mut Velocity>(|v| {
        // "gesekan": tarik kecepatan menuju nol.
        v.0 -= v.0.signum();
        v.1 -= v.1.signum();
    }));
    s
}

fn main() {
    let (mut serial, handles) = dunia_awal();
    let (mut paralel, _) = dunia_awal();

    let mut jadwal_serial = schedule_baru();
    let mut jadwal_paralel = schedule_baru();

    println!("menjalankan {N} entity × 10 tick — serial vs paralel...");
    for _ in 0..10 {
        jadwal_serial.run(&mut serial); // 1 thread
        jadwal_paralel.run_parallel(&mut paralel); // banyak thread
    }

    // Bukti: setiap entity berakhir identik di kedua world.
    for (i, &e) in handles.iter().enumerate() {
        let p_s = serial.get::<Position>(e).unwrap();
        let p_p = paralel.get::<Position>(e).unwrap();
        let en_s = serial.get::<Energy>(e).unwrap();
        let en_p = paralel.get::<Energy>(e).unwrap();
        assert_eq!(p_s, p_p, "Position entity {i} berbeda serial vs paralel");
        assert_eq!(en_s, en_p, "Energy entity {i} berbeda serial vs paralel");
    }

    println!(
        "OK — {} entity IDENTIK antara run() dan run_parallel().",
        handles.len()
    );
    println!("determinisme by construction terbukti (STD-0006).");
}
