//! Uji integrasi filter **lintas komponen** pada satu entity
//! (`Query::with`/`with_where`/`without`) terhadap Postgres nyata. Dilewati bila
//! `DATABASE_URL` tak diset. Komponen `Wc*` unik ke berkas ini. Satu fungsi uji.

use arke::World;
use arke_postgres::{Dir, PgComponent, PgStore};

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct WcOrder {
    status: String,
    total: i64,
}

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct WcCustomer {
    tier: String,
}

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct WcNote {
    text: String,
}

#[tokio::test]
async fn with_component_end_to_end() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let mut store = PgStore::connect(&url).await.expect("connect");
    store
        .register::<WcOrder>()
        .register::<WcCustomer>()
        .register::<WcNote>();
    store.migrate().await.unwrap();
    store.save(&World::new()).await.unwrap(); // slate bersih

    // (status, total, tier, note?)
    let rows: [(&str, i64, Option<&str>, Option<&str>); 6] = [
        ("open", 100, Some("gold"), Some("rush")),
        ("open", 250, Some("gold"), None),
        ("open", 300, Some("silver"), Some("gift")),
        ("closed", 900, Some("gold"), None),
        ("open", 50, None, Some("no customer")),
        ("open", 700, Some("gold"), Some("vip")),
    ];
    let mut world = World::new();
    for (status, total, tier, note) in rows {
        let e = world.spawn();
        world.insert(
            e,
            WcOrder {
                status: status.to_string(),
                total,
            },
        );
        if let Some(t) = tier {
            world.insert(
                e,
                WcCustomer {
                    tier: t.to_string(),
                },
            );
        }
        if let Some(n) = note {
            world.insert(
                e,
                WcNote {
                    text: n.to_string(),
                },
            );
        }
    }
    store.save(&world).await.unwrap();

    let totals = |w: &World, pids: &[(i64, arke::Entity)]| -> Vec<i64> {
        pids.iter()
            .map(|(_, e)| w.get::<WcOrder>(*e).unwrap().total)
            .collect()
    };

    // ---- "open orders untuk customer gold, total > 200, urut total" ----
    // Ini query INTERSECT + 3× EXISTS di BunSane; di sini satu SELECT + 1 semi-join.
    let mut w = World::new();
    let got = store
        .query::<WcOrder>()
        .filter(
            WcOrder::status()
                .eq("open".into())
                .and(WcOrder::total().gt(200)),
        )
        .with_where(WcCustomer::tier().eq("gold".into()))
        .order_by(WcOrder::total(), Dir::Desc)
        .load_pids(&mut w)
        .await
        .unwrap();
    assert_eq!(totals(&w, &got), vec![700, 250]);
    // Entity termuat lengkap (customer ikut), bukan hanya WcOrder.
    assert_eq!(w.query::<WcCustomer>().count(), 2);

    // ---- with::<R>() = kehadiran; without::<R>() = ketiadaan ----
    let mut w = World::new();
    let got = store
        .query::<WcOrder>()
        .with::<WcNote>()
        .order_by(WcOrder::total(), Dir::Asc)
        .load_pids(&mut w)
        .await
        .unwrap();
    assert_eq!(totals(&w, &got), vec![50, 100, 300, 700]);
    let mut w = World::new();
    let got = store
        .query::<WcOrder>()
        .without::<WcCustomer>()
        .load_pids(&mut w)
        .await
        .unwrap();
    assert_eq!(totals(&w, &got), vec![50]);

    // ---- gabungan: gold, tanpa note ----
    let mut w = World::new();
    let got = store
        .query::<WcOrder>()
        .with_where(WcCustomer::tier().eq("gold".into()))
        .without::<WcNote>()
        .order_by(WcOrder::total(), Dir::Asc)
        .load_pids(&mut w)
        .await
        .unwrap();
    assert_eq!(totals(&w, &got), vec![250, 900]);

    // ---- count/exists/count_estimate ikut kondisi lintas komponen ----
    let n = store
        .query::<WcOrder>()
        .with_where(WcCustomer::tier().eq("gold".into()))
        .count()
        .await
        .unwrap();
    assert_eq!(n, 4);
    assert!(
        !store
            .query::<WcOrder>()
            .with_where(WcCustomer::tier().eq("platinum".into()))
            .exists()
            .await
            .unwrap()
    );
    let est = store
        .query::<WcOrder>()
        .with_where(WcCustomer::tier().eq("gold".into()))
        .count_estimate()
        .await
        .unwrap();
    assert!(est >= 1);

    // ---- keyset load_page di atas kondisi lintas komponen ----
    let mut w = World::new();
    let p1 = store
        .query::<WcOrder>()
        .with_where(WcCustomer::tier().eq("gold".into()))
        .order_by(WcOrder::total(), Dir::Asc)
        .limit(3)
        .load_page(&mut w)
        .await
        .unwrap();
    let p2 = store
        .query::<WcOrder>()
        .with_where(WcCustomer::tier().eq("gold".into()))
        .order_by(WcOrder::total(), Dir::Asc)
        .limit(3)
        .after(p1.next.expect("halaman 2"))
        .load_page(&mut w)
        .await
        .unwrap();
    let mut all = totals(&w, &p1.items);
    all.extend(totals(&w, &p2.items));
    assert_eq!(all, vec![100, 250, 700, 900]);
    assert!(p2.next.is_none());

    // ---- filter R dengan or/not tetap terkurung di sub-query ----
    let mut w = World::new();
    let got = store
        .query::<WcOrder>()
        .with_where(
            WcCustomer::tier()
                .eq("silver".into())
                .or(WcCustomer::tier().eq("gold".into()))
                .not(),
        )
        .load_pids(&mut w)
        .await
        .unwrap();
    assert!(got.is_empty(), "tak ada tier selain silver/gold");
}
