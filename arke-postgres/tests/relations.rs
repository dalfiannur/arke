//! Uji integrasi relasi entity persisten + join (RFC-0031). Dilewati bila
//! `DATABASE_URL` tak diset. Komponen unik ke berkas ini (cmp_beast/cmp_keeper).

use arke::{Entity, QueryData, World};
use arke_postgres::{PgComponent, PgStore};

#[derive(PgComponent, PartialEq, Debug)]
struct Beast {
    hp: i32,
}

/// Relasi: `pet` → satu kolom `pet_id` menyimpan **pid** Beast (RFC-0034 Am.3).
#[derive(PgComponent, PartialEq, Debug)]
struct Keeper {
    pet: Entity,
}

/// Handle semua entity di `world`.
fn handles(w: &mut World) -> Vec<Entity> {
    let mut v = Vec::new();
    <Entity>::each(w, |e| v.push(e));
    v
}

/// Untuk tiap Keeper di `world`, ikuti `.pet` → hp Beast yang ditunjuk
/// (resolusi **intra-world**, RFC-0034 Am.3: handle sisi-simpan tak lestari).
/// Terurut. Pet yang tak ikut termuat dilewati.
fn pet_hps(w: &mut World) -> Vec<i32> {
    let keepers: Vec<Entity> = handles(w)
        .into_iter()
        .filter(|&e| w.get::<Keeper>(e).is_some())
        .collect();
    let mut hps: Vec<i32> = keepers
        .iter()
        .filter_map(|&k| w.get::<Keeper>(k).map(|kp| kp.pet))
        .filter_map(|pet| w.get::<Beast>(pet).map(|b| b.hp))
        .collect();
    hps.sort();
    hps
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

    // 1) Round-trip: tiap `Keeper.pet` me-resolve ke Beast yang benar DALAM world
    //    muat penuh (pid pet_id → handle lokal via entity_of). 3 keeper → hp
    //    {10,50,90}.
    let mut w1 = World::new();
    store.load(&mut w1).await.unwrap();
    assert_eq!(pet_hps(&mut w1), vec![10, 50, 90]);

    // 2) join (filter-saja): keeper yang pet-nya ber-hp < 60 → 2 keeper. Target
    //    Beast TIDAK dimuat.
    let mut w2 = World::new();
    let n = store
        .query::<Keeper>()
        .join(Keeper::pet(), Beast::hp().lt(60))
        .load(&mut w2)
        .await
        .unwrap();
    assert_eq!(n, 2);
    assert_eq!(w2.query::<Keeper>().count(), 2);
    assert_eq!(w2.query::<Beast>().count(), 0, "join tak memuat target");

    // 3) join_load: memuat pula Beast target → traversal handle intra-world jalan.
    let mut w3 = World::new();
    let n = store
        .query::<Keeper>()
        .join_load(Keeper::pet(), Beast::hp().lt(60))
        .load(&mut w3)
        .await
        .unwrap();
    assert_eq!(n, 2);
    // Beast yang termuat = pet ber-hp < 60 → {10,50}; b_strong (90) tak dimuat.
    let mut beast_hps: Vec<i32> = w3.query::<Beast>().map(|b| b.hp).collect();
    beast_hps.sort();
    assert_eq!(beast_hps, vec![10, 50]);
    // Tiap keeper me-resolve pet-nya ke Beast termuat.
    assert_eq!(pet_hps(&mut w3), vec![10, 50]);
}
