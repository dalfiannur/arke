//! Regresi: future `create`/`save` harus `Send` **tanpa** `World: Sync`, supaya
//! bisa di-await langsung dari handler async multi-thread (axum, `tokio::spawn`)
//! yang memegang `World` per-request. Dulu keduanya `async fn`, sehingga `&World`
//! tertangkap di state future sampai selesai → future pemanggil `!Send`.
//! Murni uji kompilasi: `MongoStore::connect` mem-`ping` server, jadi store tak
//! dibangun — cukup fungsi yang diperiksa tipe-nya (sejajar
//! `arke-postgres/tests/send_future.rs`). Selalu berjalan, tak butuh `MONGODB_URI`.

use arke::World;
use arke_mongo::{MongoError, MongoStore, mongo_component};

#[derive(arke::Serialize, PartialEq, Debug)]
struct SendProbe {
    n: i64,
}
mongo_component!(SendProbe => "send_probe");

fn assert_send<T: Send>(_: T) {}

/// Pola handler: World lokal, tulis, lalu `.await` — sebagai satu future yang
/// harus `Send` (mis. argumen `tokio::spawn`).
async fn handler_like(mut store: MongoStore) -> Result<(), MongoError> {
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, SendProbe { n: 1 });
    store.create(&world, e).await?;
    world.insert(e, SendProbe { n: 2 });
    store.save(&world).await
}

/// Tak pernah dipanggil; yang diuji adalah kompilasinya.
#[allow(dead_code)]
fn probe(store: MongoStore) {
    assert_send(handler_like(store));
}

#[test]
fn future_tulis_send_tanpa_world_sync() {
    // Bila berkas ini terkompilasi, `probe` lolos pemeriksaan `Send`.
}
