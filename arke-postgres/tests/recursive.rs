//! Uji integrasi rekursi same-type (WITH RECURSIVE) — RFC-0032 fase 3. Dilewati
//! bila `DATABASE_URL` tak diset. Tabel unik (cmp_employee).

use arke::{Entity, QueryData, World};
use arke_postgres::{PgComponent, PgStore, Ref};

/// Self-ref: `manager` menunjuk atasan (None untuk puncak). `id` untuk identifikasi
/// stabil (RFC-0034 Am.3: handle tak lestari lintas-load).
#[derive(PgComponent, PartialEq, Debug)]
struct Employee {
    id: i32,
    manager: Option<Ref<Employee>>,
}

/// Muat **penuh** ke World segar → `pid_of` memetakan seluruh entity; kembalikan
/// handle Employee ber-`id` (root untuk query rekursif berikutnya). World dibuang,
/// tapi `pid_of` tetap valid sampai `max_depth` membaca seed.
async fn root_by_id(store: &mut PgStore, id: i32) -> Entity {
    let mut src = World::new();
    store.load(&mut src).await.unwrap();
    let mut hs = Vec::new();
    <Entity>::each(&mut src, |e| hs.push(e));
    hs.into_iter()
        .find(|&e| src.get::<Employee>(e).map(|x| x.id) == Some(id))
        .expect("employee dengan id dimaksud ada")
}

/// `id` semua Employee di `world`, terurut.
fn ids(w: &mut World) -> Vec<i32> {
    let mut v: Vec<i32> = w.query::<Employee>().map(|e| e.id).collect();
    v.sort();
    v
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

    // Rantai boss → a → b → c → d (manager menunjuk ke atas), id 0..4.
    let mut world = World::new();
    let boss = world.spawn();
    world.insert(
        boss,
        Employee {
            id: 0,
            manager: None,
        },
    );
    let a = world.spawn();
    world.insert(
        a,
        Employee {
            id: 1,
            manager: Some(Ref::new(boss)),
        },
    );
    let b = world.spawn();
    world.insert(
        b,
        Employee {
            id: 2,
            manager: Some(Ref::new(a)),
        },
    );
    let c = world.spawn();
    world.insert(
        c,
        Employee {
            id: 3,
            manager: Some(Ref::new(b)),
        },
    );
    let d = world.spawn();
    world.insert(
        d,
        Employee {
            id: 4,
            manager: Some(Ref::new(c)),
        },
    );
    store.save(&world).await.unwrap();

    // descendants(boss) dalam → a,b,c,d (id 1..4); boss (0) tak termasuk.
    let boss_h = root_by_id(&mut store, 0).await;
    let mut w = World::new();
    let n = store
        .query::<Employee>()
        .descendants_of(boss_h, Employee::manager())
        .max_depth(10)
        .load(&mut w)
        .await
        .unwrap();
    assert_eq!(n, 4);
    assert_eq!(ids(&mut w), vec![1, 2, 3, 4]);

    // max_depth membatasi: 1 → a (base) + b (satu iterasi) = id {1,2}.
    let boss_h = root_by_id(&mut store, 0).await;
    let mut w1 = World::new();
    let n1 = store
        .query::<Employee>()
        .descendants_of(boss_h, Employee::manager())
        .max_depth(1)
        .load(&mut w1)
        .await
        .unwrap();
    assert_eq!(n1, 2);
    assert_eq!(ids(&mut w1), vec![1, 2]);

    // ancestors(d) → c,b,a,boss (id {0,1,2,3}); d (4) tak termasuk.
    let d_h = root_by_id(&mut store, 4).await;
    let mut wa = World::new();
    let na = store
        .query::<Employee>()
        .ancestors_of(d_h, Employee::manager())
        .max_depth(10)
        .load(&mut wa)
        .await
        .unwrap();
    assert_eq!(na, 4);
    assert_eq!(ids(&mut wa), vec![0, 1, 2, 3]);
}
