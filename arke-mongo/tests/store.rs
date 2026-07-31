//! Lapis 2 (RFC-0035 §7): uji `MongoStore` terhadap MongoDB nyata.
//!
//! Dilewati (skip) bila `MONGODB_URI` tak diset — sehingga CI tanpa MongoDB
//! tetap hijau; job `mongo` di CI menyetel env ini. Tiap tes memakai database
//! sendiri agar tak saling mengganggu.
//!
//! **Guard anti-silent-skip**: bila `MONGODB_URI` tak diset **dan**
//! `ARKE_REQUIRE_MONGO` diset, `uri()` panic alih-alih diam-diam melewati
//! tes. Tanpa ini, job CI `mongo` yang salah konfigurasi (nama env keliru,
//! container service gagal start) akan membuat seluruh lapis-2 ini hijau
//! tanpa mengetes apa pun, tanpa jejak apa pun di output. Job `mongo`
//! menyetel `ARKE_REQUIRE_MONGO=1` di samping `MONGODB_URI`; lokal dan job
//! CI lain yang tak menyetel keduanya tak terpengaruh — tes tetap skip
//! seperti biasa.

use arke::World;
use arke_mongo::{IndexDef, MongoStore, mongo_component};

#[derive(arke::Serialize, PartialEq, Debug)]
struct Position {
    x: f32,
    y: f32,
}
mongo_component!(Position => "position");

#[derive(arke::Serialize, PartialEq, Debug)]
struct Health {
    hp: i64,
}
mongo_component!(Health => "health", indexes: [IndexDef::asc("hp")]);

/// URI uji, atau `None` bila env tak diset (tes di-skip).
///
/// # Panics
///
/// Panic bila `MONGODB_URI` tak diset tapi `ARKE_REQUIRE_MONGO` diset — lihat
/// dokumentasi modul.
fn uri() -> Option<String> {
    match std::env::var("MONGODB_URI") {
        Ok(u) => Some(u),
        Err(_) => {
            assert!(
                std::env::var("ARKE_REQUIRE_MONGO").is_err(),
                "ARKE_REQUIRE_MONGO diset tapi MONGODB_URI tidak — job CI \
                 `mongo` salah konfigurasi (service container gagal start, \
                 atau nama env keliru): lapis-2 arke-mongo tidak boleh diam-\
                 diam dilewati di sini"
            );
            None
        }
    }
}

/// Store bersih pada database bernama `db_name` (di-drop lebih dulu lewat
/// klien driver mentah — `MongoStore` sengaja tak mengekspos operasi
/// destruktif macam drop-database di API publiknya).
async fn store(db_name: &str) -> Option<MongoStore> {
    let uri = uri()?;
    let raw = mongodb::Client::with_uri_str(&uri)
        .await
        .expect("klien driver mentah untuk setup tes");
    raw.database(db_name)
        .drop()
        .await
        .expect("drop database sebelum tes");
    let mut s = MongoStore::connect(&uri, db_name)
        .await
        .expect("connect MongoStore");
    s.register::<Position>();
    s.register::<Health>();
    Some(s)
}

#[tokio::test]
async fn ensure_indexes_idempoten() {
    let Some(s) = store("arke_test_indexes").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };
    s.ensure_indexes().await.expect("ensure_indexes pertama");

    let raw = mongodb::Client::with_uri_str(uri().expect("MONGODB_URI"))
        .await
        .expect("klien driver mentah untuk assert");
    let db = raw.database("arke_test_indexes");
    let entities = db.collection::<mongodb::bson::Document>("arke_entities");

    let names_first = entities
        .list_index_names()
        .await
        .expect("list_index_names setelah panggilan pertama");
    assert!(
        names_first.iter().any(|n| n.contains("cmp.health.hp")),
        "indeks atas `cmp.health.hp` tak ditemukan setelah ensure_indexes; \
         indeks yang ada: {names_first:?}"
    );

    s.ensure_indexes().await.expect("ensure_indexes kedua");

    let names_second = entities
        .list_index_names()
        .await
        .expect("list_index_names setelah panggilan kedua");
    assert_eq!(
        names_first.len(),
        names_second.len(),
        "jumlah indeks berubah setelah pemanggilan ulang — ensure_indexes \
         seharusnya idempoten untuk definisi yang tak berubah: {names_first:?} \
         vs {names_second:?}"
    );

    let meta = db
        .collection::<mongodb::bson::Document>("arke_meta")
        .find_one(mongodb::bson::doc! { "_id": "schema" })
        .await
        .expect("baca dokumen arke_meta")
        .expect("dokumen `{_id: \"schema\"}` harus ada setelah ensure_indexes");
    assert_eq!(
        meta.get_i64("schema_version"),
        Ok(1),
        "schema_version tak tersimpan/tak sesuai di arke_meta: {meta:?}"
    );
}

// Port 1 tak pernah melayani MongoDB. `connect` harus gagal di sini, bukan
// menunda kegagalan ke operasi pertama (sejajar `PgStore::connect`) — lihat
// FIX 2. Tes ini sengaja TIDAK memakai `uri()`/`store()`: ia tak butuh
// MongoDB nyata dan karenanya selalu berjalan, terlepas dari `MONGODB_URI`.
#[tokio::test]
async fn connect_ke_host_mati_gagal_saat_connect_bukan_nanti() {
    let r = MongoStore::connect("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1500", "x").await;
    assert!(r.is_err(), "connect ke host mati harus Err, dapat Ok");
}

#[tokio::test]
async fn create_lalu_fetch_round_trip_setia() {
    let Some(mut s) = store("arke_test_create_fetch").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };

    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, Position { x: 1.5, y: -2.5 });
    w.insert(e, Health { hp: 77 });
    let pid = s.create(&w, e).await.unwrap();

    // World baru: materialisasi dari MongoDB.
    let mut w2 = World::new();
    let e2 = s
        .fetch(&mut w2, pid)
        .await
        .unwrap()
        .expect("entity harus ada");

    assert_eq!(w2.get::<Position>(e2), Some(&Position { x: 1.5, y: -2.5 }));
    assert_eq!(w2.get::<Health>(e2), Some(&Health { hp: 77 }));
}

#[tokio::test]
async fn fetch_pid_tak_dikenal_mengembalikan_none() {
    let Some(mut s) = store("arke_test_fetch_none").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };
    let mut w = World::new();
    assert!(
        s.fetch(&mut w, arke_mongo::Pid::new())
            .await
            .unwrap()
            .is_none()
    );
}
