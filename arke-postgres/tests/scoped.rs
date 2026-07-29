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
    for i in 0..4 {
        let e = world.spawn();
        world.insert(e, Health { hp: 5 + i * 10 });
        world.insert(e, Position { x: (i * 10) as f32 });
    }
    store.save(&world).await.unwrap();

    // RFC-0034: indeks World ephemeral → handle sisi-simpan tak lestari lintas
    // `load_where`; verifikasi subset + kelengkapan komponen lewat isi world muat.
    let sorted_i32 = |w: &mut World| {
        let mut v: Vec<i32> = w.query::<Health>().map(|h| h.hp).collect();
        v.sort();
        v
    };
    let sorted_f32 = |w: &mut World| {
        let mut v: Vec<i32> = w.query::<Position>().map(|p| p.x as i32).collect();
        v.sort();
        v
    };

    // Muat hanya yang "sekarat" (hp < 20) → hp 5 & 15; tiap match membawa Position.
    let mut hurt = World::new();
    let n = store
        .load_where::<Health>(&mut hurt, "hp < 20")
        .await
        .unwrap();
    assert_eq!(n, 2);
    assert_eq!(sorted_i32(&mut hurt), vec![5, 15]);
    assert_eq!(sorted_f32(&mut hurt), vec![0, 10]);

    // Predikat atas komponen lain (Position) juga bisa → x 20 & 30 (+ Health).
    let mut far = World::new();
    let n = store
        .load_where::<Position>(&mut far, "x >= 20")
        .await
        .unwrap();
    assert_eq!(n, 2);
    assert_eq!(sorted_f32(&mut far), vec![20, 30]);
    assert_eq!(sorted_i32(&mut far), vec![25, 35]);
}
