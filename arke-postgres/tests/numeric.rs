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

    // RFC-0034: indeks World ephemeral → handle sisi-simpan (e1/e2) tak lestari;
    // verifikasi round-trip NUMERIC lewat himpunan isi world muat.
    let mut got: Vec<(u64, usize, Option<u64>)> = loaded
        .query::<Wallet>()
        .map(|w| (w.coins, w.slot, w.debt))
        .collect();
    got.sort();
    let mut want = vec![(big, 42usize, Some(1_000u64)), (0, 0, None)];
    want.sort();
    assert_eq!(got, want);
}
