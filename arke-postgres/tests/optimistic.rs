//! Uji optimistic-lock `PgStore::update_entity` vs Postgres nyata.
//! Dilewati bila `DATABASE_URL` tak diset.

use arke::{Entity, World};
use arke_postgres::{PgComponent, PgStore, UpdateError};

#[derive(PgComponent, PartialEq, Debug)]
struct Score {
    value: i32,
}

async fn store() -> Option<PgStore> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let mut s = PgStore::connect(&url).await.expect("connect");
    s.register::<Score>();
    Some(s)
}

fn world_with(e_idx_gen: (u32, u32), value: i32) -> (World, Entity) {
    let mut w = World::new();
    let e = w.spawn_at(e_idx_gen.0, e_idx_gen.1);
    w.insert(e, Score { value });
    (w, e)
}

#[tokio::test]
async fn optimistic_lock_mendeteksi_konflik() {
    let Some(mut store) = store().await else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    store.migrate().await.unwrap();

    // Seed: satu entity, Score(10). Baseline versi 0.
    let mut seed = World::new();
    let e = seed.spawn();
    seed.insert(e, Score { value: 10 });
    store.save(&seed).await.unwrap();
    assert_eq!(store.entity_version(e).await.unwrap(), Some(0));

    // Writer B (segar, versi 0) menulis Score(20) → sukses, versi 1.
    let (wb, _) = world_with((e.index(), e.generation()), 20);
    assert_eq!(store.update_entity(&wb, e, 0).await.unwrap(), 1);
    assert_eq!(store.entity_version(e).await.unwrap(), Some(1));

    // Writer A (basi, masih kira versi 0) → KONFLIK terdeteksi.
    let (wa, _) = world_with((e.index(), e.generation()), 99);
    match store.update_entity(&wa, e, 0).await {
        Err(UpdateError::Conflict) => {}
        other => panic!("harusnya Conflict, dapat {other:?}"),
    }

    // Nilai DB tetap dari writer B (A tak menimpa).
    let mut loaded = World::new();
    store.load(&mut loaded).await.unwrap();
    assert_eq!(loaded.get::<Score>(e), Some(&Score { value: 20 }));

    // Writer A re-baca versi (1) lalu retry → sukses, versi 2.
    let v = store.entity_version(e).await.unwrap().unwrap();
    assert_eq!(v, 1);
    assert_eq!(store.update_entity(&wa, e, v).await.unwrap(), 2);

    let mut after = World::new();
    store.load(&mut after).await.unwrap();
    assert_eq!(after.get::<Score>(e), Some(&Score { value: 99 }));
}
