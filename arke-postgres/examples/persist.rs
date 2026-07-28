//! Contoh end-to-end `arke-postgres`: save/load, tulis-balik inkremental, dan
//! optimistic-lock terhadap Postgres nyata.
//!
//! Jalankan (butuh Postgres):
//! ```sh
//! DATABASE_URL=postgres://arke:arke@localhost:5432/arke_test \
//!   cargo run -p arke-postgres --example persist
//! ```

use std::error::Error;

use arke::World;
use arke_postgres::{PgComponent, PgStore, UpdateError};

#[derive(PgComponent, Debug)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(PgComponent, Debug)]
struct Score {
    value: i32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("Set DATABASE_URL untuk menjalankan contoh ini, mis.:");
        eprintln!("  DATABASE_URL=postgres://arke:arke@localhost:5432/arke_test \\");
        eprintln!("    cargo run -p arke-postgres --example persist");
        return Ok(());
    };

    let mut store = PgStore::connect(&url).await?;
    store.register::<Position>().register::<Score>();
    store.migrate().await?;

    // --- 1) ECS → Postgres (overwrite penuh) ---
    let mut world = World::new();
    let hero = world.spawn();
    world.insert(hero, Position { x: 0.0, y: 0.0 });
    world.insert(hero, Score { value: 10 });
    let mob = world.spawn();
    world.insert(mob, Position { x: 5.0, y: 5.0 });
    store.save(&world).await?;
    println!("save: 2 entity ditulis");

    // --- 2) Postgres → ECS (handle identik) ---
    let mut loaded = World::new();
    store.load(&mut loaded).await?;
    println!(
        "load: hero.contains={}, hero.score={:?}",
        loaded.contains(hero),
        loaded.get::<Score>(hero),
    );

    // --- 3) Tulis-balik inkremental: hanya yang berubah ditulis ---
    for s in world.query_mut::<Score>() {
        s.value += 1; // hero: 10 → 11 (mob tak punya Score)
    }
    let stats = store.save_incremental(&world).await?;
    println!(
        "save_incremental: written={}, deleted={}",
        stats.written, stats.deleted
    );

    // --- 4) Optimistic-lock: konflik multi-writer ---
    let v = store.entity_version(hero).await?.expect("hero ada");
    let new_v = store.update_entity(&world, hero, v).await?;
    println!("update_entity: sukses, versi {v} → {new_v}");

    // Coba tulis dengan versi basi → konflik.
    match store.update_entity(&world, hero, v).await {
        Err(UpdateError::Conflict) => {
            println!("update_entity (versi basi {v}): Conflict terdeteksi (benar)")
        }
        Ok(_) => println!("(tak terduga: mestinya konflik)"),
        Err(e) => return Err(e.into()),
    }

    println!("selesai.");
    Ok(())
}
