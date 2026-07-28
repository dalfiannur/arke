//! Uji indeks & constraint kustom (`#[pg(index/unique/check)]`) dibuat oleh
//! `migrate` dan ditegakkan Postgres. Dilewati bila `DATABASE_URL` tak diset.

use arke::World;
use arke_postgres::{PgComponent, PgStore};
use sqlx::PgPool;

#[derive(PgComponent, PartialEq, Debug)]
#[pg(check = "level >= 0")]
struct Guard {
    #[pg(index)]
    level: i32,
    #[pg(unique)]
    code: i32,
}

#[tokio::test]
async fn index_dan_check_dibuat_dan_ditegakkan() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let pool = PgPool::connect(&url).await.expect("pool");
    sqlx::query("DROP TABLE IF EXISTS cmp_guard, arke_entities CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let mut store = PgStore::connect(&url).await.unwrap();
    store.register::<Guard>();
    store.migrate().await.unwrap();

    // Indeks dibuat.
    let idx: Option<String> = sqlx::query_scalar(
        "SELECT indexname FROM pg_indexes \
         WHERE tablename = 'cmp_guard' AND indexname = 'idx_cmp_guard_level'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(idx.is_some(), "indeks level harus dibuat");

    let uniq: Option<String> = sqlx::query_scalar(
        "SELECT indexname FROM pg_indexes \
         WHERE tablename = 'cmp_guard' AND indexname = 'idx_cmp_guard_code'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(uniq.is_some(), "indeks unik code harus dibuat");

    // Constraint CHECK dibuat.
    let chk: Option<String> =
        sqlx::query_scalar("SELECT conname FROM pg_constraint WHERE conname = 'chk_cmp_guard_0'")
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert!(chk.is_some(), "constraint CHECK harus dibuat");

    // migrate idempoten (jalankan lagi tanpa error).
    store.migrate().await.unwrap();

    // Save valid → sukses.
    let mut ok = World::new();
    let e = ok.spawn();
    ok.insert(e, Guard { level: 5, code: 1 });
    store.save(&ok).await.unwrap();

    // CHECK menolak level negatif.
    let mut bad = World::new();
    let e = bad.spawn();
    bad.insert(e, Guard { level: -1, code: 2 });
    assert!(
        store.save(&bad).await.is_err(),
        "CHECK harus menolak level < 0"
    );

    // UNIQUE menolak code duplikat.
    let mut dup = World::new();
    let a = dup.spawn();
    dup.insert(a, Guard { level: 1, code: 9 });
    let b = dup.spawn();
    dup.insert(b, Guard { level: 2, code: 9 });
    assert!(
        store.save(&dup).await.is_err(),
        "UNIQUE harus menolak code duplikat"
    );
}
