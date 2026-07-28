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

#[derive(PgComponent, PartialEq, Debug)]
struct Target {
    id: Option<i64>,
    note: Option<String>,
}

#[derive(arke::Serialize, PartialEq, Debug, Clone)]
struct Waypoint {
    x: i32,
    y: i32,
}

#[derive(PgComponent, PartialEq, Debug)]
struct Path {
    kind: String,
    home: Waypoint,         // non-skalar → JSONB
    next: Option<Waypoint>, // Option non-skalar → JSONB nullable
}

async fn connect() -> Option<PgStore> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let mut store = PgStore::connect(&url).await.expect("connect Postgres");
    store
        .register::<Position>()
        .register::<Label>()
        .register::<Target>()
        .register::<Path>();
    Some(store)
}

fn entity_count(world: &mut World) -> usize {
    let mut n = 0;
    <Entity>::each(world, |_| n += 1);
    n
}

#[tokio::test]
async fn pgstore_save_load_round_trip_dan_overwrite() {
    let Some(mut store) = connect().await else {
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
    world.insert(
        e1,
        Target {
            id: Some(7),
            note: Some("locked".to_string()),
        },
    );
    world.insert(
        e1,
        Path {
            kind: "patrol".to_string(),
            home: Waypoint { x: 1, y: 1 },
            next: Some(Waypoint { x: 2, y: 3 }),
        },
    );
    let e2 = world.spawn();
    world.insert(e2, Position { x: -5.0, y: 0.5 }); // tanpa Label
    world.insert(
        e2,
        Target {
            id: None,
            note: None,
        },
    ); // kolom NULL
    world.insert(
        e2,
        Path {
            kind: "idle".to_string(),
            home: Waypoint { x: 0, y: 0 },
            next: None, // JSONB NULL
        },
    );

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
    // Option: Some round-trip, None → NULL → None.
    assert_eq!(
        loaded.get::<Target>(e1),
        Some(&Target {
            id: Some(7),
            note: Some("locked".to_string()),
        })
    );
    assert_eq!(
        loaded.get::<Target>(e2),
        Some(&Target {
            id: None,
            note: None
        })
    );
    // JSONB: nested struct round-trip + Option nested (Some & None → NULL).
    assert_eq!(
        loaded.get::<Path>(e1),
        Some(&Path {
            kind: "patrol".to_string(),
            home: Waypoint { x: 1, y: 1 },
            next: Some(Waypoint { x: 2, y: 3 }),
        })
    );
    assert_eq!(
        loaded.get::<Path>(e2),
        Some(&Path {
            kind: "idle".to_string(),
            home: Waypoint { x: 0, y: 0 },
            next: None,
        })
    );

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
