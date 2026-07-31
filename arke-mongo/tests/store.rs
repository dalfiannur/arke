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

use arke::{Entity, World};
use arke_mongo::bson::doc;
use arke_mongo::{IndexDef, MongoError, MongoStore, mongo_component};

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

#[tokio::test]
async fn fetch_cmp_bertipe_salah_gagal_bukan_diam_diam() {
    let Some(mut s) = store("arke_test_cmp_rusak").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };
    // Dokumen rusak: `cmp` bukan sub-dokumen. Bisa ditulis service lain atau
    // hasil korupsi. Membacanya sebagai "entity tanpa komponen" akan
    // menghilangkan data diam-diam (RFC-0035 §6).
    let pid = arke_mongo::Pid::new();
    let raw = mongodb::Client::with_uri_str(uri().expect("MONGODB_URI"))
        .await
        .expect("klien driver mentah untuk setup tes");
    raw.database("arke_test_cmp_rusak")
        .collection::<mongodb::bson::Document>("arke_entities")
        .insert_one(mongodb::bson::doc! { "_id": pid.0, "version": 0i64, "cmp": "bukan dokumen" })
        .await
        .expect("tulis dokumen rusak");

    // `w` baru: `fetch` men-spawn tepat satu entity baru, yang karenanya
    // dijamin `Entity::from_raw(0, 0)` (spawn pertama pada World kosong).
    // Bila `fetch` gagal dan membuang entity itu lewat `despawn`, generasi
    // slot 0 naik ke 1 sehingga handle lama (gen 0) tak lagi hidup.
    let mut w = World::new();
    let hasil = s.fetch(&mut w, pid).await;
    assert!(
        hasil.is_err(),
        "cmp bertipe salah harus Err, dapat {hasil:?}"
    );
    assert!(
        !w.contains(Entity::from_raw(0, 0)),
        "fetch yang gagal harus membuang entity yang telanjur di-spawn \
         (despawn tidak terjadi)"
    );
}

#[tokio::test]
async fn fetch_tanpa_field_cmp_mengembalikan_entity_tanpa_komponen() {
    let Some(mut s) = store("arke_test_cmp_absen").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };
    // Dokumen legit: entity yang belum/tak pernah punya komponen terdaftar
    // tak menulis field `cmp` sama sekali — ini bukan korupsi.
    let pid = arke_mongo::Pid::new();
    let raw = mongodb::Client::with_uri_str(uri().expect("MONGODB_URI"))
        .await
        .expect("klien driver mentah untuk setup tes");
    raw.database("arke_test_cmp_absen")
        .collection::<mongodb::bson::Document>("arke_entities")
        .insert_one(mongodb::bson::doc! { "_id": pid.0, "version": 0i64 })
        .await
        .expect("tulis dokumen tanpa cmp");

    let mut w = World::new();
    let e = s
        .fetch(&mut w, pid)
        .await
        .expect("fetch tanpa cmp harus Ok")
        .expect("entity harus ada");
    assert_eq!(w.get::<Position>(e), None);
    assert_eq!(w.get::<Health>(e), None);
}

#[tokio::test]
async fn fetch_cmp_field_bertipe_salah_gagal_decode_dan_membuang_entity() {
    let Some(mut s) = store("arke_test_field_rusak").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };
    // `cmp` itu sendiri sub-dokumen yang sah, tapi salah satu field komponen
    // di dalamnya bertipe salah (`x` string, bukan angka) — path
    // `Registry::apply` -> `MongoError::Decode`, bukan `get("cmp")` yang
    // ditutupi FIX 1.
    let pid = arke_mongo::Pid::new();
    let raw = mongodb::Client::with_uri_str(uri().expect("MONGODB_URI"))
        .await
        .expect("klien driver mentah untuk setup tes");
    raw.database("arke_test_field_rusak")
        .collection::<mongodb::bson::Document>("arke_entities")
        .insert_one(mongodb::bson::doc! {
            "_id": pid.0,
            "version": 0i64,
            "cmp": { "position": { "x": "bukan angka", "y": 1.0 } },
        })
        .await
        .expect("tulis dokumen dengan field cmp rusak");

    // Sama seperti tes sebelumnya: `w` baru, jadi entity yang di-spawn
    // `fetch` di dalam dijamin `Entity::from_raw(0, 0)`.
    let mut w = World::new();
    let hasil = s.fetch(&mut w, pid).await;
    assert!(
        matches!(hasil, Err(arke_mongo::MongoError::Decode { .. })),
        "field cmp bertipe salah harus Err(Decode), dapat {hasil:?}"
    );
    assert!(
        !w.contains(Entity::from_raw(0, 0)),
        "fetch yang gagal decode harus membuang entity yang telanjur \
         di-spawn"
    );
}

#[tokio::test]
async fn create_dua_kali_entity_sama_menjaga_bijeksi_pid_of_entity_of() {
    let Some(mut s) = store("arke_test_bind_create").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };
    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, Position { x: 0.0, y: 0.0 });

    let pid1 = s.create(&w, e).await.unwrap();
    let pid2 = s.create(&w, e).await.unwrap();

    assert_ne!(pid1, pid2);
    assert_eq!(s.pid_of(e), Some(pid2));
    // Tautan lama (pid1 -> e) di `entity_of` seharusnya sudah dibersihkan
    // oleh `bind` saat `pid_of[e]` dipindah ke pid2 — pid1 tak boleh lagi
    // mengklaim entity ini.
    assert_eq!(
        s.entity_of(pid1),
        None,
        "entity_of[pid1] harus dibersihkan begitu e dipetakan ulang ke pid2"
    );
    assert_eq!(s.entity_of(pid2), Some(e));
}

#[tokio::test]
async fn fetch_dua_kali_pid_sama_menjaga_bijeksi_pid_of_entity_of() {
    let Some(mut s) = store("arke_test_bind_fetch").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };
    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, Position { x: 0.0, y: 0.0 });
    let pid = s.create(&w, e).await.unwrap();

    let mut w2 = World::new();
    let e1 = s.fetch(&mut w2, pid).await.unwrap().expect("fetch pertama");
    let e2 = s.fetch(&mut w2, pid).await.unwrap().expect("fetch kedua");

    assert_ne!(e1, e2);
    // Tautan lama (e1 -> pid) di `pid_of` seharusnya sudah dibersihkan
    // oleh `bind` saat `entity_of[pid]` dipindah ke e2 — e1 tak boleh lagi
    // mengklaim pid ini.
    assert_eq!(s.pid_of(e1), None);
    assert_eq!(s.pid_of(e2), Some(pid));
}

#[tokio::test]
async fn update_menulis_nilai_baru_dan_menaikkan_version() {
    let Some(mut s) = store("arke_test_update").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };

    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, Position { x: 0.0, y: 0.0 });
    let pid = s.create(&w, e).await.unwrap();
    assert_eq!(s.version_of(pid).await.unwrap(), Some(0));

    w.insert(e, Position { x: 9.0, y: 9.0 });
    s.update(&w, e, pid).await.unwrap();
    assert_eq!(s.version_of(pid).await.unwrap(), Some(1));

    let mut w2 = World::new();
    let e2 = s.fetch(&mut w2, pid).await.unwrap().unwrap();
    assert_eq!(w2.get::<Position>(e2), Some(&Position { x: 9.0, y: 9.0 }));
}

#[tokio::test]
async fn update_checked_mendeteksi_konflik_versi() {
    let Some(mut s) = store("arke_test_conflict").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };

    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, Health { hp: 10 });
    let pid = s.create(&w, e).await.unwrap();

    // Penulis lain menaikkan versi lebih dulu.
    s.update(&w, e, pid).await.unwrap();

    // Kita masih memegang harapan versi 0 → konflik.
    match s.update_checked(&w, e, pid, 0).await {
        Err(MongoError::Conflict {
            expected, actual, ..
        }) => {
            assert_eq!(expected, 0);
            assert_eq!(actual, Some(1));
        }
        other => panic!("harus Conflict, dapat {other:?}"),
    }
}

#[tokio::test]
async fn update_checked_sukses_mengembalikan_versi_baru() {
    let Some(mut s) = store("arke_test_checked_ok").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };

    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, Health { hp: 1 });
    let pid = s.create(&w, e).await.unwrap();

    assert_eq!(s.update_checked(&w, e, pid, 0).await.unwrap(), 1);
}

#[tokio::test]
async fn remove_menghapus_dokumen() {
    let Some(mut s) = store("arke_test_remove").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };

    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, Health { hp: 3 });
    let pid = s.create(&w, e).await.unwrap();

    s.remove(pid).await.unwrap();
    assert_eq!(s.version_of(pid).await.unwrap(), None);
}

#[tokio::test]
async fn save_menulis_entity_baru_dan_menghapus_yang_despawn() {
    let Some(mut s) = store("arke_test_save").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };

    let mut w = World::new();
    let a = w.spawn();
    w.insert(a, Health { hp: 1 });
    let b = w.spawn();
    w.insert(b, Health { hp: 2 });
    s.save(&w).await.unwrap();

    let pid_b = s.pid_of(b).expect("b harus punya pid setelah save");
    w.despawn(b);
    s.save(&w).await.unwrap();

    assert_eq!(s.version_of(pid_b).await.unwrap(), None, "b harus terhapus");
    let pid_a = s.pid_of(a).unwrap();
    assert!(
        s.version_of(pid_a).await.unwrap().is_some(),
        "a harus tetap"
    );
}

#[tokio::test]
async fn save_tidak_menghapus_komponen_yang_tak_terdaftar() {
    let Some(mut s) = store("arke_test_foreign").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };

    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, Health { hp: 5 });
    let pid = s.create(&w, e).await.unwrap();

    // Service lain menulis komponennya sendiri ke dokumen yang sama.
    s.collection()
        .update_one(
            doc! { "_id": pid.0 },
            doc! { "$set": { "cmp.dari_service_lain": { "v": 1i64 } } },
        )
        .await
        .unwrap();

    w.insert(e, Health { hp: 6 });
    s.save(&w).await.unwrap();

    let d = s
        .collection()
        .find_one(doc! { "_id": pid.0 })
        .await
        .unwrap()
        .unwrap();
    assert!(
        d.get_document("cmp")
            .unwrap()
            .contains_key("dari_service_lain"),
        "save tak boleh menghapus komponen milik penulis lain"
    );
}
