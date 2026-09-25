//! Uji `UpdateWhere::set_opt`: mengisi kolom nullable, dan `None` →
//! mengosongkannya (`NULL`). Dilewati tanpa `DATABASE_URL`.

use arke::World;
use arke_postgres::{PgComponent, PgStore};

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct SoRow {
    key: i32,
    note: Option<String>,
}

#[tokio::test]
async fn set_opt_mengisi_dan_mengosongkan() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let pool = sqlx::PgPool::connect(&url).await.expect("pool");
    sqlx::query("DROP TABLE IF EXISTS cmp_sorow")
        .execute(&pool)
        .await
        .unwrap();
    let mut store = PgStore::connect(&url).await.unwrap();
    store.register::<SoRow>();
    store.migrate().await.unwrap();
    let mut w = World::new();
    let e = w.spawn();
    w.insert(
        e,
        SoRow {
            key: 1,
            note: Some("awal".into()),
        },
    );
    store
        .commit_insert(store.stage_insert(&w, e))
        .await
        .unwrap();

    let note = || async {
        sqlx::query_scalar::<_, Option<String>>("SELECT note FROM cmp_sorow WHERE key = 1")
            .fetch_one(&pool)
            .await
            .unwrap()
    };
    let n = store
        .update_where::<SoRow>()
        .filter(SoRow::key().eq(1))
        .set_opt(SoRow::note(), Some("baru".to_string()))
        .execute()
        .await
        .unwrap();
    assert_eq!(n, 1);
    assert_eq!(note().await.as_deref(), Some("baru"));

    store
        .update_where::<SoRow>()
        .filter(SoRow::key().eq(1))
        .set_opt(SoRow::note(), None)
        .execute()
        .await
        .unwrap();
    assert_eq!(note().await, None);
}
