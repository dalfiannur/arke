//! Uji integrasi relasi bersarang `matches` 3-deep (RFC-0032 fase 1). Dilewati
//! bila `DATABASE_URL` tak diset. Berkas sendiri (tabel cmp_link) → tak balapan.

use arke::{Entity, World};
use arke_postgres::{PgComponent, PgStore};

/// Rantai (linked list) untuk uji nesting `matches` 3-deep.
#[derive(PgComponent, PartialEq, Debug)]
struct Link {
    v: i32,
    next: Option<Entity>,
}

#[tokio::test]
async fn matches_bersarang_3_deep_menyaring_benar() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let mut store = PgStore::connect(&url).await.expect("connect");
    store.register::<Link>();
    store.migrate().await.unwrap();
    store.save(&World::new()).await.unwrap();

    // Rantai a→b→c→d dengan v = 1,2,3,4.
    let mut world = World::new();
    let d = world.spawn();
    world.insert(d, Link { v: 4, next: None });
    let c = world.spawn();
    world.insert(
        c,
        Link {
            v: 3,
            next: Some(d),
        },
    );
    let b = world.spawn();
    world.insert(
        b,
        Link {
            v: 2,
            next: Some(c),
        },
    );
    let a = world.spawn();
    world.insert(
        a,
        Link {
            v: 1,
            next: Some(b),
        },
    );
    store.save(&world).await.unwrap();

    // "Link yang next→next→next-nya ber-v > 3" → hanya a (a→b→c→d, d.v=4).
    let mut w = World::new();
    let n = store
        .query::<Link>()
        .filter(Link::next().matches::<Link>(
            Link::next().matches::<Link>(Link::next().matches::<Link>(Link::v().gt(3))),
        ))
        .load(&mut w)
        .await
        .unwrap();
    assert_eq!(n, 1);
    // RFC-0034 Am.3: handle sisi-simpan tak lestari → verifikasi via isi. Hanya `a`
    // (v=1) yang next→next→next-nya (d, v=4) > 3.
    let vs: Vec<i32> = w.query::<Link>().map(|l| l.v).collect();
    assert_eq!(vs, vec![1]);
}
