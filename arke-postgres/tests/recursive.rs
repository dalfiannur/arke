//! Uji integrasi rekursi same-type (WITH RECURSIVE) — RFC-0032 fase 3. Dilewati
//! bila `DATABASE_URL` tak diset. Tabel unik (cmp_employee).

use arke::World;
use arke_postgres::{PgComponent, PgStore, Ref};

/// Self-ref: `manager` menunjuk atasan (None untuk puncak).
#[derive(PgComponent, PartialEq, Debug)]
struct Employee {
    manager: Option<Ref<Employee>>,
}

#[tokio::test]
async fn descendants_ancestors_dan_max_depth() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let mut store = PgStore::connect(&url).await.expect("connect");
    store.register::<Employee>();
    store.migrate().await.unwrap();
    store.save(&World::new()).await.unwrap();

    // Rantai boss → a → b → c → d (manager menunjuk ke atas).
    let mut world = World::new();
    let boss = world.spawn();
    world.insert(boss, Employee { manager: None });
    let a = world.spawn();
    world.insert(
        a,
        Employee {
            manager: Some(Ref::new(boss)),
        },
    );
    let b = world.spawn();
    world.insert(
        b,
        Employee {
            manager: Some(Ref::new(a)),
        },
    );
    let c = world.spawn();
    world.insert(
        c,
        Employee {
            manager: Some(Ref::new(b)),
        },
    );
    let d = world.spawn();
    world.insert(
        d,
        Employee {
            manager: Some(Ref::new(c)),
        },
    );
    store.save(&world).await.unwrap();

    // descendants(boss) dalam → a,b,c,d (4); boss sendiri tak termasuk.
    let mut w = World::new();
    let n = store
        .query::<Employee>()
        .descendants_of(boss, Employee::manager())
        .max_depth(10)
        .load(&mut w)
        .await
        .unwrap();
    assert_eq!(n, 4);
    assert!(w.contains(a) && w.contains(d) && !w.contains(boss));

    // max_depth membatasi: 1 → a (base) + b (satu iterasi) = 2.
    let mut w1 = World::new();
    let n1 = store
        .query::<Employee>()
        .descendants_of(boss, Employee::manager())
        .max_depth(1)
        .load(&mut w1)
        .await
        .unwrap();
    assert_eq!(n1, 2);
    assert!(w1.contains(a) && w1.contains(b) && !w1.contains(c));

    // ancestors(d) → c,b,a,boss (4).
    let mut wa = World::new();
    let na = store
        .query::<Employee>()
        .ancestors_of(d, Employee::manager())
        .max_depth(10)
        .load(&mut wa)
        .await
        .unwrap();
    assert_eq!(na, 4);
    assert!(wa.contains(boss) && wa.contains(a) && !wa.contains(d));
}
