//! Uji evolusi skema: `migrate` merekonsiliasi tabel lama (field ditambah/
//! dihapus) secara non-destruktif. Dilewati bila `DATABASE_URL` tak diset.

use arke::World;
use arke_postgres::{PgComponent, PgStore};
use sqlx::{PgPool, Row};

// Skema BARU: `old_field` sudah dihapus, `new_field` ditambahkan.
#[derive(PgComponent, PartialEq, Debug)]
struct Thing {
    new_field: i32,
}

#[tokio::test]
async fn migrate_merekonsiliasi_skema_lama() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let pool = PgPool::connect(&url).await.expect("pool");

    // Slate bersih + skema LAMA cmp_thing (old_field, tanpa new_field).
    sqlx::query("DROP TABLE IF EXISTS cmp_thing, arke_entities CASCADE")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE arke_entities \
         (entity_id BIGINT PRIMARY KEY, generation BIGINT NOT NULL, version BIGINT NOT NULL DEFAULT 0)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TABLE cmp_thing \
         (entity_id BIGINT PRIMARY KEY REFERENCES arke_entities(entity_id) ON DELETE CASCADE, \
          old_field INTEGER NOT NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO arke_entities (entity_id, generation, version) VALUES (0, 0, 0)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO cmp_thing (entity_id, old_field) VALUES (0, 7)")
        .execute(&pool)
        .await
        .unwrap();

    // Rekonsiliasi ke skema BARU.
    let mut store = PgStore::connect(&url).await.unwrap();
    store.register::<Thing>();
    store.migrate().await.unwrap();

    // Kolom baru ada; kolom usang jadi nullable.
    let has_new: Option<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_name = 'cmp_thing' AND column_name = 'new_field'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(has_new.is_some(), "kolom new_field harus ditambahkan");

    let old_nullable: String = sqlx::query_scalar(
        "SELECT is_nullable FROM information_schema.columns \
         WHERE table_name = 'cmp_thing' AND column_name = 'old_field'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(old_nullable, "YES", "old_field harusnya jadi nullable");

    // Data lama selamat (non-destruktif); new_field di-backfill 0.
    let row = sqlx::query("SELECT old_field, new_field FROM cmp_thing WHERE entity_id = 0")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.try_get::<i32, _>("old_field").unwrap(), 7);
    assert_eq!(row.try_get::<i32, _>("new_field").unwrap(), 0);

    // INSERT baru (tanpa old_field) kini valid → save/load bekerja.
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Thing { new_field: 42 });
    store.save(&world).await.unwrap();

    let mut loaded = World::new();
    store.load(&mut loaded).await.unwrap();
    assert_eq!(loaded.get::<Thing>(e), Some(&Thing { new_field: 42 }));
}
