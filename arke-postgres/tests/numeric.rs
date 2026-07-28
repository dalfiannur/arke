//! Uji kolom `NUMERIC` (u64/usize) round-trip vs Postgres — termasuk nilai
//! melampaui `i64::MAX` (yang tak muat di `BIGINT`). Dilewati tanpa `DATABASE_URL`.

use arke::World;
use arke_postgres::{PgComponent, PgStore};

#[derive(PgComponent, PartialEq, Debug)]
struct Wallet {
    coins: u64,
    slot: usize,
    debt: Option<u64>,
}

#[tokio::test]
async fn numeric_round_trip_termasuk_di_atas_i64_max() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let mut store = PgStore::connect(&url).await.expect("connect");
    store.register::<Wallet>();
    store.migrate().await.unwrap();

    let big = u64::MAX - 3; // > i64::MAX → wajib NUMERIC, bukan BIGINT
    let mut world = World::new();
    let e1 = world.spawn();
    world.insert(
        e1,
        Wallet {
            coins: big,
            slot: 42,
            debt: Some(1_000),
        },
    );
    let e2 = world.spawn();
    world.insert(
        e2,
        Wallet {
            coins: 0,
            slot: 0,
            debt: None, // NUMERIC NULL
        },
    );

    store.save(&world).await.unwrap();

    let mut loaded = World::new();
    store.load(&mut loaded).await.unwrap();

    assert_eq!(
        loaded.get::<Wallet>(e1),
        Some(&Wallet {
            coins: big,
            slot: 42,
            debt: Some(1_000),
        })
    );
    assert_eq!(
        loaded.get::<Wallet>(e2),
        Some(&Wallet {
            coins: 0,
            slot: 0,
            debt: None,
        })
    );
}
