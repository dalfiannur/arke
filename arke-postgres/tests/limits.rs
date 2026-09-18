//! Uji integrasi batas operasional (`ConnectOptions`: acquire/statement/lock
//! timeout, `FailureKind`, `PoolStats`) terhadap Postgres nyata. Dilewati bila
//! `DATABASE_URL` tak diset. Komponen `LimRow` unik ke berkas ini.

use std::time::Duration;

use arke::World;
use arke_postgres::{ConnectOptions, FailureKind, PgComponent, PgStore, failure_kind};

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct LimRow {
    v: i32,
}

#[tokio::test]
async fn batas_operasional() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };

    // ---- statement_timeout: query lambat dibatalkan server → StatementTimeout ----
    let opts = ConnectOptions {
        max_connections: 2,
        statement_timeout: Some(Duration::from_millis(200)),
        application_name: Some("arke-limits-test".into()),
        ..ConnectOptions::default()
    };
    let mut store = PgStore::connect_with(&url, &opts).await.expect("connect");
    store.register::<LimRow>();
    store.migrate().await.unwrap();
    // Satu baris agar predikat (dan `pg_sleep`) benar-benar dievaluasi.
    let mut seed = World::new();
    let e = seed.spawn();
    seed.insert(e, LimRow { v: 1 });
    store.save(&seed).await.unwrap();
    let mut w = World::new();
    let err = store
        .load_where::<LimRow>(&mut w, "pg_sleep(2) IS NOT NULL")
        .await
        .unwrap_err();
    assert_eq!(failure_kind(&err), FailureKind::StatementTimeout, "{err}");
    // Query cepat tetap jalan di pool yang sama (koneksi tak rusak).
    assert_eq!(store.query::<LimRow>().count().await.unwrap(), 1);
    let stats = store.pool_stats();
    assert_eq!(stats.max, 2);
    assert!(stats.size >= 1 && stats.size <= 2, "{stats:?}");

    // ---- acquire_timeout: pool penuh → shed at the door → PoolTimeout ----
    let opts = ConnectOptions {
        max_connections: 1,
        acquire_timeout: Duration::from_millis(200),
        ..ConnectOptions::default()
    };
    let mut store = PgStore::connect_with(&url, &opts).await.expect("connect");
    let tx = store.begin().await.unwrap(); // memegang satu-satunya koneksi
    let t0 = std::time::Instant::now();
    let err = store.query::<LimRow>().count().await.unwrap_err();
    assert_eq!(failure_kind(&err), FailureKind::PoolTimeout, "{err}");
    assert!(t0.elapsed() < Duration::from_secs(5), "harus gagal cepat");
    let stats = store.pool_stats();
    assert_eq!((stats.size, stats.idle, stats.max), (1, 0, 1), "{stats:?}");
    drop(tx); // rollback → koneksi kembali
    assert_eq!(store.query::<LimRow>().count().await.unwrap(), 1);

    // ---- lock_timeout: advisory lock bentrok → LockTimeout ----
    let opts = ConnectOptions {
        max_connections: 2,
        lock_timeout: Some(Duration::from_millis(200)),
        ..ConnectOptions::default()
    };
    let store = PgStore::connect_with(&url, &opts).await.expect("connect");
    let mut holder = store.begin().await.unwrap();
    holder.advisory_lock(424242).await.unwrap();
    let mut waiter = store.begin().await.unwrap();
    let err = waiter.advisory_lock(424242).await.unwrap_err();
    assert_eq!(failure_kind(&err), FailureKind::LockTimeout, "{err}");
    drop(waiter);
    holder.commit().await.unwrap();

    // ---- connect() lama = default: tanpa statement_timeout (query lambat lolos) ----
    let mut store = PgStore::connect(&url).await.expect("connect");
    store.register::<LimRow>();
    let mut w = World::new();
    store
        .load_where::<LimRow>(&mut w, "pg_sleep(0.3) IS NOT NULL")
        .await
        .unwrap();
}
