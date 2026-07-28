//! Uji `PgStore` round-trip terhadap Postgres nyata.
//!
//! Dilewati (skip) bila `DATABASE_URL` tak diset — sehingga CI tanpa Postgres
//! tetap hijau; job Postgres di CI menyetel env ini. Satu test sekuensial
//! karena berbagi tabel global yang sama.

use arke::{Entity, QueryData, World};
use arke_postgres::{PgComponent, PgStore};

#[derive(PgComponent, PartialEq, Debug)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(PgComponent, PartialEq, Debug)]
struct Label {
    name: String,
    level: i32,
    hp: i64,
    active: bool,
}

async fn connect() -> Option<PgStore> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let mut store = PgStore::connect(&url).await.expect("connect Postgres");
    store.register::<Position>().register::<Label>();
    Some(store)
}

fn entity_count(world: &mut World) -> usize {
    let mut n = 0;
    <Entity>::each(world, |_| n += 1);
    n
}

#[tokio::test]
async fn pgstore_save_load_round_trip_dan_overwrite() {
    let Some(store) = connect().await else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    store.migrate().await.unwrap();

    // --- Round-trip setia ---
    let mut world = World::new();
    let e1 = world.spawn();
    world.insert(e1, Position { x: 1.0, y: 2.0 });
    world.insert(
        e1,
        Label {
            name: "hero".to_string(),
            level: 3,
            hp: -100,
            active: true,
        },
    );
    let e2 = world.spawn();
    world.insert(e2, Position { x: -5.0, y: 0.5 }); // tanpa Label

    store.save(&world).await.unwrap();

    // Muat ke World segar → handle identik + komponen setia.
    let mut loaded = World::new();
    store.load(&mut loaded).await.unwrap();

    assert!(loaded.contains(e1));
    assert!(loaded.contains(e2));
    assert_eq!(
        loaded.get::<Position>(e1),
        Some(&Position { x: 1.0, y: 2.0 })
    );
    assert_eq!(
        loaded.get::<Label>(e1),
        Some(&Label {
            name: "hero".to_string(),
            level: 3,
            hp: -100,
            active: true,
        })
    );
    assert_eq!(
        loaded.get::<Position>(e2),
        Some(&Position { x: -5.0, y: 0.5 })
    );
    assert_eq!(loaded.get::<Label>(e2), None);

    // --- Save kedua menimpa state lama ---
    let mut b = World::new();
    let only = b.spawn();
    b.insert(only, Position { x: 99.0, y: 99.0 });
    store.save(&b).await.unwrap();

    let mut reloaded = World::new();
    store.load(&mut reloaded).await.unwrap();
    assert_eq!(entity_count(&mut reloaded), 1);
    assert_eq!(
        reloaded.get::<Position>(only),
        Some(&Position { x: 99.0, y: 99.0 })
    );
}
