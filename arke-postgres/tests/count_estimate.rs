//! Uji integrasi `Query::count_estimate` (estimasi planner via `EXPLAIN`)
//! terhadap Postgres nyata. Dilewati bila `DATABASE_URL` tak diset. Komponen
//! `EstRow` unik ke berkas ini (tabel `cmp_estrow`).

use arke::World;
use arke_postgres::{PgComponent, PgStore};

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct EstRow {
    bucket: i32,
}

#[tokio::test]
async fn count_estimate_dekat_count_setelah_analyze() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    let mut store = PgStore::connect(&url).await.expect("connect");
    store.register::<EstRow>();
    store.migrate().await.unwrap();
    store.save(&World::new()).await.unwrap(); // slate bersih

    // 2000 baris, bucket 0..10 merata → filter `bucket < 3` ≈ 600.
    let mut world = World::new();
    for i in 0..2000 {
        let e = world.spawn();
        world.insert(e, EstRow { bucket: i % 10 });
    }
    store.save(&world).await.unwrap();
    sqlx::query("ANALYZE cmp_estrow")
        .execute(&pool)
        .await
        .unwrap();

    // Tanpa filter.
    let exact = store.query::<EstRow>().count().await.unwrap();
    let est = store.query::<EstRow>().count_estimate().await.unwrap();
    assert_eq!(exact, 2000);
    assert!(
        (1000..=4000).contains(&est),
        "estimasi total {est} terlalu jauh dari {exact}"
    );

    // Dengan filter ter-parameterisasi (statistik histogram/MCV).
    let f = EstRow::bucket().lt(3);
    let exact = store
        .query::<EstRow>()
        .filter(f.clone())
        .count()
        .await
        .unwrap();
    let est = store
        .query::<EstRow>()
        .filter(f)
        .count_estimate()
        .await
        .unwrap();
    assert_eq!(exact, 600);
    assert!(
        (300..=1200).contains(&est),
        "estimasi ber-filter {est} terlalu jauh dari {exact}"
    );

    // Tabel kosong → estimasi kecil (planner memberi ≥ 1 untuk tabel tanpa
    // statistik; tak boleh gagal).
    store.save(&World::new()).await.unwrap();
    sqlx::query("ANALYZE cmp_estrow")
        .execute(&pool)
        .await
        .unwrap();
    let est = store.query::<EstRow>().count_estimate().await.unwrap();
    assert!(est <= 10, "tabel kosong: estimasi {est}");
}
