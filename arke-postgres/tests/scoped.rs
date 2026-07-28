//! Uji `PgStore::load_where` — materialisasi subset (query-scoped).
//! Dilewati bila `DATABASE_URL` tak diset.

use arke::World;
use arke_postgres::{PgComponent, PgStore};

#[derive(PgComponent, PartialEq, Debug)]
struct Health {
    hp: i32,
}

#[derive(PgComponent, PartialEq, Debug)]
struct Position {
    x: f32,
}

#[tokio::test]
async fn load_where_memuat_subset_beserta_semua_komponen() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let mut store = PgStore::connect(&url).await.expect("connect");
    store.register::<Health>().register::<Position>();
    store.migrate().await.unwrap();
    store.save(&World::new()).await.unwrap(); // slate bersih

    // 4 entity: hp 5,15,25,35 + Position.
    let mut world = World::new();
    let mut ids = Vec::new();
    for i in 0..4 {
        let e = world.spawn();
        world.insert(e, Health { hp: 5 + i * 10 });
        world.insert(e, Position { x: (i * 10) as f32 });
        ids.push(e);
    }
    store.save(&world).await.unwrap();

    // Muat hanya yang "sekarat" (hp < 20) → e0(hp5), e1(hp15).
    let mut hurt = World::new();
    let n = store
        .load_where::<Health>(&mut hurt, "hp < 20")
        .await
        .unwrap();
    assert_eq!(n, 2);

    // Yang cocok: seluruh komponennya dimuat (Health + Position).
    assert_eq!(hurt.get::<Health>(ids[0]), Some(&Health { hp: 5 }));
    assert_eq!(hurt.get::<Position>(ids[0]), Some(&Position { x: 0.0 }));
    assert_eq!(hurt.get::<Health>(ids[1]), Some(&Health { hp: 15 }));
    assert_eq!(hurt.get::<Position>(ids[1]), Some(&Position { x: 10.0 }));

    // Yang tak cocok: tak dimuat.
    assert!(!hurt.contains(ids[2]));
    assert!(!hurt.contains(ids[3]));

    // Predikat atas komponen lain (Position) juga bisa.
    let mut far = World::new();
    let n = store
        .load_where::<Position>(&mut far, "x >= 20")
        .await
        .unwrap();
    assert_eq!(n, 2); // e2(x20), e3(x30)
    assert!(far.contains(ids[2]));
    assert!(far.contains(ids[3]));
    assert!(!far.contains(ids[0]));
}
