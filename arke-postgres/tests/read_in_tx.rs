//! Uji baca di dalam transaksi (RFC-0042): `Query::load_pids_in` dan
//! `PgStore::fetch_in` melihat tulisan transaksi yang belum di-commit, relasi
//! yang dimuat di sana resolve untuk tulisan berikutnya di transaksi yang sama,
//! dan pool di luar transaksi tetap tak melihatnya. Dilewati tanpa `DATABASE_URL`.

use arke::World;
use arke_postgres::{PgComponent, PgStore, Ref};

#[derive(PgComponent, PartialEq, Debug, Clone)]
#[pg(unique(key))]
struct TxParent {
    key: String,
}

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct TxChild {
    #[pg(on_delete = "cascade")]
    parent: Ref<TxParent>,
    note: String,
}

#[tokio::test]
async fn baca_dalam_transaksi_melihat_tulisan_sendiri() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let pool = sqlx::PgPool::connect(&url).await.expect("pool");
    sqlx::query("DROP TABLE IF EXISTS cmp_txparent, cmp_txchild")
        .execute(&pool)
        .await
        .unwrap();
    let mut tpl = PgStore::connect(&url).await.unwrap();
    tpl.register::<TxParent>().register::<TxChild>();
    tpl.migrate().await.unwrap();

    let mut store = tpl.fork();
    let mut tx = store.begin().await.unwrap();

    // Upsert induk di tx, lalu muat balik di tx yang sama.
    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, TxParent { key: "k1".into() });
    let up = store
        .upsert::<TxParent>(store.stage_insert(&w, e))
        .on(TxParent::key())
        .execute_in(&mut tx)
        .await
        .unwrap();

    let mut world = World::new();
    let found = store
        .query::<TxParent>()
        .filter(TxParent::key().eq("k1".to_string()))
        .load_pids_in(&mut tx, &mut world)
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, up.pid);
    let parent = found[0].1;

    // Tak terlihat dari luar transaksi.
    let mut other = tpl.fork();
    let outside = other
        .query::<TxParent>()
        .filter(TxParent::key().eq("k1".to_string()))
        .load_pids(&mut World::new())
        .await
        .unwrap();
    assert!(outside.is_empty());

    // Relasi ke entity yang dimuat di tx resolve saat menulis anak di tx.
    let c = world.spawn();
    world.insert(
        c,
        TxChild {
            parent: Ref::new(parent),
            note: "anak".into(),
        },
    );
    let child_pid = store
        .commit_insert_in(&mut tx, store.stage_insert(&world, c))
        .await
        .unwrap();

    // fetch_in + include di tx.
    let mut w2 = World::new();
    let fetched = store.fetch_in(&mut tx, &mut w2, child_pid).await.unwrap();
    assert!(fetched.is_some());
    let mut w3 = World::new();
    let mut s3 = tpl.fork();
    let got = s3
        .query::<TxChild>()
        .include(TxChild::parent())
        .load_pids_in(&mut tx, &mut w3)
        .await
        .unwrap();
    assert_eq!(got.len(), 1);
    let child = w3.get::<TxChild>(got[0].1).unwrap().clone();
    assert_eq!(child.note, "anak");
    assert_eq!(w3.get::<TxParent>(child.parent.entity()).unwrap().key, "k1");

    tx.commit().await.unwrap();

    // Setelah commit: terlihat dari luar.
    let after = tpl
        .fork()
        .query::<TxChild>()
        .load_pids(&mut World::new())
        .await
        .unwrap();
    assert_eq!(after.len(), 1);
}
