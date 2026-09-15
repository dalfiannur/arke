//! Uji integrasi RedisCache terhadap backend Redis-compatible NYATA (RFC-0033).
//! Butuh DATABASE_URL (Postgres) + REDIS_URL (default redis://localhost:6379).
//! Dilewati bila DATABASE_URL tak diset atau Redis tak terjangkau.

use std::sync::Arc;

use arke::World;
use arke_cache::RedisCache;
use arke_postgres::{ComponentCache, PgComponent, PgStore};

#[derive(PgComponent, PartialEq, Debug)]
struct CacheProbe {
    value: i32,
}

#[tokio::test]
async fn redis_cache_end_to_end() {
    let Ok(db) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".into());
    let Ok(cache) = RedisCache::connect(&redis_url, 60).await else {
        eprintln!("skip: Redis tak terjangkau di {redis_url}");
        return;
    };
    let cache = Arc::new(cache);
    cache.clear().await; // slate bersih

    let mut store = PgStore::connect(&db).await.expect("connect pg");
    store.register::<CacheProbe>();
    store.migrate().await.unwrap();
    let mut store = store.with_cache(cache.clone());

    let mut world = World::new();
    let e0 = world.spawn();
    world.insert(e0, CacheProbe { value: 7 });
    let e1 = world.spawn();
    world.insert(e1, CacheProbe { value: 8 });
    store.save(&world).await.unwrap(); // clear cache
    let pid0 = store.pid_of(e0).expect("pid teralokasi");

    // Pembaca memakai `fork()` (pool + cache sama, jembatan sendiri) agar `store`
    // tetap tertaut ke `world` untuk `save_incremental` di bawah.
    let mut reader = store.fork();

    // Muat #1 → isi cache Redis.
    let mut w1 = World::new();
    reader.load(&mut w1).await.unwrap();
    assert_eq!(w1.get::<CacheProbe>(e0), Some(&CacheProbe { value: 7 }));

    // Bukti: kunci terisi di Redis (MGET langsung via cache) — di bawah namespace
    // ber-fingerprint skema dan `pid` persisten (bukan indeks World).
    let ns = store
        .cache_namespace(CacheProbe::TABLE)
        .expect("tabel terdaftar")
        .to_string();
    let probe = cache.get_many(&ns, &[pid0]).await;
    assert!(probe[0].is_some(), "load harus mengisi cache Redis");

    // Muat #2 → dilayani cache; data tetap benar.
    let mut w2 = World::new();
    reader.load(&mut w2).await.unwrap();
    assert_eq!(w2.get::<CacheProbe>(e1), Some(&CacheProbe { value: 8 }));

    // Ubah e0 (7→77) → save_incremental → invalidate → muat #3 = NILAI BARU.
    for c in world.query_mut::<CacheProbe>() {
        if c.value == 7 {
            c.value = 77;
        }
    }
    store.save_incremental(&world).await.unwrap();
    let mut w3 = World::new();
    reader.load(&mut w3).await.unwrap();
    assert_eq!(
        w3.get::<CacheProbe>(e0),
        Some(&CacheProbe { value: 77 }),
        "cache tak boleh basi setelah tulis"
    );
}
