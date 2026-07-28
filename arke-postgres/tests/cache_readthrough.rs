//! Uji read-through + invalidasi cache (RFC-0033) memakai cache IN-MEMORY (tanpa
//! Redis). Verifikasi: hit di muat kedua, & tulis tak menyajikan data basi.
//! Dilewati bila `DATABASE_URL` tak diset.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use arke::World;
use arke_postgres::{ComponentCache, PgComponent, PgStore};
use async_trait::async_trait;

#[derive(PgComponent, PartialEq, Debug)]
struct Coin {
    value: i32,
}

#[derive(Default)]
struct MemCache {
    data: Mutex<HashMap<(String, i64), Vec<u8>>>,
    hits: AtomicUsize,
    misses: AtomicUsize,
}

#[async_trait]
impl ComponentCache for MemCache {
    async fn get_many(&self, table: &str, ids: &[i64]) -> Vec<Option<Vec<u8>>> {
        let d = self.data.lock().unwrap();
        ids.iter()
            .map(|id| {
                let v = d.get(&(table.to_string(), *id)).cloned();
                if v.is_some() {
                    self.hits.fetch_add(1, Ordering::Relaxed);
                } else {
                    self.misses.fetch_add(1, Ordering::Relaxed);
                }
                v
            })
            .collect()
    }
    async fn put_many(&self, table: &str, entries: &[(i64, Vec<u8>)]) {
        let mut d = self.data.lock().unwrap();
        for (id, bytes) in entries {
            d.insert((table.to_string(), *id), bytes.clone());
        }
    }
    async fn invalidate(&self, table: &str, ids: &[i64]) {
        let mut d = self.data.lock().unwrap();
        for id in ids {
            d.remove(&(table.to_string(), *id));
        }
    }
    async fn clear(&self) {
        self.data.lock().unwrap().clear();
    }
}

#[tokio::test]
async fn read_through_hit_dan_invalidasi_tak_basi() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let mut store = PgStore::connect(&url).await.expect("connect");
    store.register::<Coin>();
    store.migrate().await.unwrap();
    let cache = Arc::new(MemCache::default());
    let mut store = store.with_cache(cache.clone());

    // 3 coin.
    let mut world = World::new();
    let ids: Vec<_> = (0..3)
        .map(|i| {
            let e = world.spawn();
            world.insert(e, Coin { value: i * 10 });
            e
        })
        .collect();
    store.save(&world).await.unwrap(); // clear cache (kosong)

    // Muat #1 → semua miss (isi cache).
    let mut w1 = World::new();
    store.load(&mut w1).await.unwrap();
    assert_eq!(cache.hits.load(Ordering::Relaxed), 0);
    assert!(cache.misses.load(Ordering::Relaxed) >= 3);
    assert_eq!(w1.get::<Coin>(ids[1]), Some(&Coin { value: 10 }));

    // Muat #2 → semua HIT (dari cache), data tetap benar.
    let h0 = cache.hits.load(Ordering::Relaxed);
    let mut w2 = World::new();
    store.load(&mut w2).await.unwrap();
    assert!(
        cache.hits.load(Ordering::Relaxed) >= h0 + 3,
        "muat kedua harus hit"
    );
    assert_eq!(w2.get::<Coin>(ids[1]), Some(&Coin { value: 10 }));

    // Ubah coin[1] (nilai 10 → 999) in-place → save_incremental → invalidate →
    // muat #3 harus NILAI BARU (bukan basi dari cache).
    for c in world.query_mut::<Coin>() {
        if c.value == 10 {
            c.value = 999;
        }
    }
    store.save_incremental(&world).await.unwrap();
    let mut w3 = World::new();
    store.load(&mut w3).await.unwrap();
    assert_eq!(
        w3.get::<Coin>(ids[1]),
        Some(&Coin { value: 999 }),
        "cache tak boleh menyajikan nilai basi setelah tulis"
    );
}
