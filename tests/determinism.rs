//! Bukti STD-0005: urutan operasi yang sama menghasilkan keadaan yang identik,
//! tak bergantung pada timing thread atau alamat memori.
//!
//! Dua `World` independen dijalankan dengan urutan operasi yang persis sama —
//! termasuk despawn (memicu pemakaian ulang slot & `swap_remove` archetype) dan
//! remove (memicu perpindahan archetype). Snapshot query keduanya harus identik.

#![forbid(unsafe_code)]

use rust_ecs::World;

#[derive(Debug, PartialEq)]
struct Health(u32);
#[derive(Debug, PartialEq)]
struct Shield(u32);

/// Menjalankan satu urutan operasi tetap dan mengembalikan snapshot query.
fn snapshot() -> (Vec<u32>, Vec<u32>) {
    let mut world = World::new();
    let mut entities = Vec::new();

    for i in 0..10 {
        let e = world.spawn();
        world.insert(e, Health(i));
        if i % 2 == 0 {
            world.insert(e, Shield(i * 10));
        }
        entities.push(e);
    }

    // Despawn beberapa → free-list terisi, baris archetype ter-swap_remove.
    world.despawn(entities[2]);
    world.despawn(entities[7]);

    // Remove komponen → perpindahan archetype.
    world.remove::<Shield>(entities[4]);

    // Spawn lagi → memakai ulang slot yang dibebaskan.
    for i in 100..104 {
        let e = world.spawn();
        world.insert(e, Health(i));
    }

    let health: Vec<u32> = world.query::<Health>().map(|h| h.0).collect();
    let shield: Vec<u32> = world.query::<Shield>().map(|s| s.0).collect();
    (health, shield)
}

#[test]
fn keadaan_deterministik_antar_run() {
    assert_eq!(snapshot(), snapshot());
}
