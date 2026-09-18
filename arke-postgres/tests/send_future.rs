//! Regresi: future `save`/`save_incremental`/`update_entity` harus `Send`
//! **tanpa** `World: Sync`,
//! supaya bisa di-await langsung dari handler async multi-thread (axum, tokio
//! `spawn`) yang memegang `World` per-request. Dulu keduanya `async fn` sehingga
//! `&World` tertangkap di state future sampai selesai → future pemanggil
//! `!Send`. Murni uji kompilasi: pool dibuat lazy, tak ada koneksi DB.

use arke::World;
use arke_postgres::{PgComponent, PgStore};

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct SendProbe {
    n: i64,
}

fn assert_send<T: Send>(_: T) {}

/// Pola handler: World lokal, tulis-balik, lalu `.await` — sebagai satu future
/// yang harus `Send` (mis. argumen `tokio::spawn`).
async fn handler_like(mut store: PgStore) -> Result<(), sqlx::Error> {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, SendProbe { n: 1 });
    store.save_incremental(&world).await?;
    world.insert(e, SendProbe { n: 2 });
    if let Err(arke_postgres::UpdateError::Db(err)) = store.update_entity(&world, e, 1).await {
        return Err(err);
    }
    store.save(&world).await
}

/// `connect_lazy` butuh konteks runtime (sqlx), meski tak pernah menyambung.
#[tokio::test]
async fn future_tulis_balik_send_tanpa_world_sync() {
    let pool = sqlx::postgres::PgPool::connect_lazy("postgres://localhost/unused").unwrap();
    let mut store = PgStore::from_pool(pool);
    store.register::<SendProbe>();
    assert_send(handler_like(store));
}
