//! Uji integrasi relasi BERTIPE `Ref<T>` + path builder `.through().load_all()`
//! (RFC-0032 fase 2). Dilewati bila `DATABASE_URL` tak diset. Tabel unik.

use arke::{Entity, QueryData, World};
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

/// Untuk tiap Squad di `world`, telusuri rantai bertipe
/// `Squad.leader(Unit) → Unit.weapon(Weapon).power` (resolusi **intra-world**,
/// RFC-0034 Am.3). Terurut; rantai yang tak lengkap termuat dilewati.
fn squad_powers(w: &mut World) -> Vec<i32> {
    let mut hs = Vec::new();
    <Entity>::each(w, |e| hs.push(e));
    let squads: Vec<Entity> = hs
        .into_iter()
        .filter(|&e| w.get::<Squad>(e).is_some())
        .collect();
    let mut powers: Vec<i32> = squads
        .iter()
        .filter_map(|&s| {
            let leader = w.get::<Squad>(s)?.leader.entity();
            let weapon = w.get::<Unit>(leader)?.weapon.entity();
            w.get::<Weapon>(weapon).map(|x| x.power)
        })
        .collect();
    powers.sort();
    powers
}

#[tokio::test]
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

    // Round-trip: muat penuh → tiap rantai Squad→Unit→Weapon me-resolve intra-world.
    // 2 squad → power {30, 90}.
    let mut w0 = World::new();
    store.load(&mut w0).await.unwrap();
    assert_eq!(squad_powers(&mut w0), vec![30, 90]);

    // Path type-safe: Squad→leader(Unit)→weapon(Weapon).power > 50 → hanya s2.
    // load_all memuat Squad + Unit + Weapon sepanjang path cocok (terdalam-dulu →
    // rantai me-resolve).
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
    assert_eq!(w.query::<Squad>().count(), 1);
    // Hanya rantai s2 termuat: 1 Unit, Weapon power {90}; s1/u1/w1 (power 30) tidak.
    assert_eq!(w.query::<Unit>().count(), 1);
    let weapons: Vec<i32> = w.query::<Weapon>().map(|x| x.power).collect();
    assert_eq!(weapons, vec![90]);
    // Rantai s2 me-resolve penuh ke power 90.
    assert_eq!(squad_powers(&mut w), vec![90]);
}
