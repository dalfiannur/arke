//! Lapis 2 (RFC-0035 §7): uji `MongoStore` terhadap MongoDB nyata.
//!
//! Dilewati (skip) bila `MONGODB_URI` tak diset — sehingga CI tanpa MongoDB
//! tetap hijau; job `mongo` di CI menyetel env ini. Tiap tes memakai database
//! sendiri agar tak saling mengganggu.

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
fn uri() -> Option<String> {
    std::env::var("MONGODB_URI").ok()
}

/// Store bersih pada database bernama `db_name` (di-drop lebih dulu).
async fn store(db_name: &str) -> Option<MongoStore> {
    let uri = uri()?;
    let mut s = MongoStore::connect(&uri, db_name).await.unwrap();
    s.drop_database().await.unwrap();
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
    s.ensure_indexes().await.unwrap();
    s.ensure_indexes().await.unwrap();
}
