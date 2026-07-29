//! Uji integrasi relasi BERTIPE `Ref<T>` + path builder `.through().load_all()`
//! (RFC-0032 fase 2). Dilewati bila `DATABASE_URL` tak diset. Tabel unik.

use arke::World;
use arke_postgres::{PgComponent, PgStore, Ref};

#[derive(PgComponent, PartialEq, Debug)]
struct Weapon {
    power: i32,
}
#[derive(PgComponent, PartialEq, Debug)]
struct Unit {
    weapon: Ref<Weapon>,
}
#[derive(PgComponent, PartialEq, Debug)]
struct Squad {
    leader: Ref<Unit>,
}

#[tokio::test]
#[ignore = "relasi menanti desain ulang berbasis pid (RFC-0034 Amandemen 2 / opsi 1); kolom _id/_gen masih menyimpan indeks World ephemeral, tak kompatibel dengan skema pid 0.12.0"]
async fn path_bertipe_3_deep_load_all() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let mut store = PgStore::connect(&url).await.expect("connect");
    store
        .register::<Weapon>()
        .register::<Unit>()
        .register::<Squad>();
    store.migrate().await.unwrap();
    store.save(&World::new()).await.unwrap();

    // w1(30) w2(90); u1→w1, u2→w2; s1→u1, s2→u2.
    let mut world = World::new();
    let w1 = world.spawn();
    world.insert(w1, Weapon { power: 30 });
    let w2 = world.spawn();
    world.insert(w2, Weapon { power: 90 });
    let u1 = world.spawn();
    world.insert(
        u1,
        Unit {
            weapon: Ref::new(w1),
        },
    );
    let u2 = world.spawn();
    world.insert(
        u2,
        Unit {
            weapon: Ref::new(w2),
        },
    );
    let s1 = world.spawn();
    world.insert(
        s1,
        Squad {
            leader: Ref::new(u1),
        },
    );
    let s2 = world.spawn();
    world.insert(
        s2,
        Squad {
            leader: Ref::new(u2),
        },
    );
    store.save(&world).await.unwrap();

    // Round-trip Ref: leader bertahan.
    let mut w0 = World::new();
    store.load(&mut w0).await.unwrap();
    assert_eq!(w0.get::<Squad>(s2).map(|s| s.leader), Some(Ref::new(u2)));

    // Path type-safe: Squad→leader(Unit)→weapon(Weapon).power > 50 → hanya s2.
    // load_all memuat Squad + Unit + Weapon sepanjang path cocok.
    let mut w = World::new();
    let n = store
        .query::<Squad>()
        .through(Squad::leader())
        .through(Unit::weapon())
        .where_(Weapon::power().gt(50))
        .load_all(&mut w)
        .await
        .unwrap();
    assert_eq!(n, 1);
    assert!(w.contains(s2) && !w.contains(s1));
    // Entity sepanjang path dimuat:
    assert_eq!(w.get::<Unit>(u2).map(|u| u.weapon), Some(Ref::new(w2)));
    assert_eq!(w.get::<Weapon>(w2).map(|x| x.power), Some(90));
    // Di luar path tak dimuat:
    assert!(w.get::<Unit>(u1).is_none() && w.get::<Weapon>(w1).is_none());
}
