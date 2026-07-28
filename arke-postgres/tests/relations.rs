//! Uji integrasi relasi entity persisten + join (RFC-0031). Dilewati bila
//! `DATABASE_URL` tak diset. Komponen unik ke berkas ini (cmp_beast/cmp_keeper).

use arke::{Entity, World};
use arke_postgres::{PgComponent, PgStore};

#[derive(PgComponent, PartialEq, Debug)]
struct Beast {
    hp: i32,
}

/// Relasi: `pet` → FK ke arke_entities (kolom `pet_id` + `pet_gen`).
#[derive(PgComponent, PartialEq, Debug)]
struct Keeper {
    pet: Entity,
}

#[tokio::test]
async fn relasi_entity_persist_join_dan_join_load() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let mut store = PgStore::connect(&url).await.expect("connect");
    store.register::<Beast>().register::<Keeper>();
    store.migrate().await.unwrap();
    store.save(&World::new()).await.unwrap(); // slate bersih

    // 3 beast (hp 10/50/90) + 3 keeper yang meng-owner masing-masing.
    let mut world = World::new();
    let b_weak = world.spawn();
    world.insert(b_weak, Beast { hp: 10 });
    let b_mid = world.spawn();
    world.insert(b_mid, Beast { hp: 50 });
    let b_strong = world.spawn();
    world.insert(b_strong, Beast { hp: 90 });
    let k1 = world.spawn();
    world.insert(k1, Keeper { pet: b_weak });
    let k2 = world.spawn();
    world.insert(k2, Keeper { pet: b_mid });
    let k3 = world.spawn();
    world.insert(k3, Keeper { pet: b_strong });
    store.save(&world).await.unwrap();

    // 1) Round-trip: relasi `Keeper.pet` bertahan save→load (rekonstruksi via
    //    Entity::from_raw dari kolom pet_id/pet_gen).
    let mut w1 = World::new();
    store.load(&mut w1).await.unwrap();
    assert_eq!(w1.get::<Keeper>(k1).map(|k| k.pet), Some(b_weak));
    assert_eq!(w1.get::<Keeper>(k3).map(|k| k.pet), Some(b_strong));

    // 1b) Keamanan-basi (STD-0007 terbawa ke relasi): handle ber-generation salah
    //     → get = None.
    let stale = Entity::from_raw(b_weak.index(), b_weak.generation().wrapping_add(1));
    assert!(w1.get::<Beast>(stale).is_none());

    // 2) join (filter-saja): keeper yang pet-nya ber-hp < 60 → k1, k2. Target
    //    Beast TIDAK dimuat.
    let mut w2 = World::new();
    let n = store
        .query::<Keeper>()
        .join(Keeper::pet(), Beast::hp().lt(60))
        .load(&mut w2)
        .await
        .unwrap();
    assert_eq!(n, 2);
    assert!(w2.contains(k1) && w2.contains(k2) && !w2.contains(k3));
    assert!(w2.get::<Beast>(b_weak).is_none(), "join tak memuat target");

    // 3) join_load: memuat pula Beast target → traversal handle langsung jalan.
    let mut w3 = World::new();
    let n = store
        .query::<Keeper>()
        .join_load(Keeper::pet(), Beast::hp().lt(60))
        .load(&mut w3)
        .await
        .unwrap();
    assert_eq!(n, 2);
    let pet = w3.get::<Keeper>(k1).unwrap().pet;
    assert_eq!(w3.get::<Beast>(pet).map(|b| b.hp), Some(10));
    // Target di luar filter (b_strong) tak dimuat.
    assert!(w3.get::<Beast>(b_strong).is_none());
}
