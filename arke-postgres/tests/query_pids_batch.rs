//! Uji `query_pids` memuat banyak entity dalam SATU batch per tabel komponen.
//! Dilewati bila `DATABASE_URL` tak diset.
//!
//! Berkas terpisah dengan tipe komponen sendiri (bukan menumpang `store.rs`):
//! berkas itu sengaja berisi satu test karena tabelnya global, dan menambah test
//! kedua di sana membuat dua `migrate()` berlomba membuat tabel yang sama.

use arke::World;
use arke_postgres::{PgComponent, PgStore};

#[derive(PgComponent, PartialEq, Debug)]
struct QpMark {
    tag: String,
    level: i32,
}

#[derive(PgComponent, PartialEq, Debug)]
struct QpSide {
    note: String,
}

async fn connect() -> Option<PgStore> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let mut store = PgStore::connect(&url).await.expect("connect Postgres");
    store.register::<QpMark>().register::<QpSide>();
    Some(store)
}

/// Sebelumnya jalur ini memanggil `fetch` sekali per pid, jadi biayanya
/// `baris × (1 + jumlah komponen terdaftar)` round-trip. Test ini menjaga hal-hal
/// yang bisa diam-diam rusak saat pindah ke `materialize`: setiap pid yang cocok
/// harus muncul (bukan hanya yang pertama), komponen tiap entity harus utuh dan
/// tidak tertukar antar baris, urutannya tetap `ORDER BY pid`, dan pid yang
/// barisnya sudah dihapus tetap tidak ikut terbawa.
#[tokio::test]
async fn query_pids_memuat_banyak_entity_sekaligus() {
    let Some(mut store) = connect().await else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    store.migrate().await.unwrap();

    let tag = format!("qp-{}", std::process::id());
    let mut seed = World::new();
    let mut pids = Vec::new();
    for i in 0..3 {
        let e = seed.spawn();
        seed.insert(e, QpMark { tag: tag.clone(), level: i });
        seed.insert(e, QpSide { note: format!("n{i}") });
        let staged = store.stage_insert(&seed, e);
        pids.push(store.commit_insert(staged).await.unwrap());
    }
    pids.sort();

    let mut world = World::new();
    let pred = format!("tag = '{tag}'");
    let pairs = store.query_pids::<QpMark>(&mut world, Some(&pred)).await.unwrap();

    assert_eq!(pairs.len(), 3, "semua baris yang cocok ikut termuat");
    let got: Vec<i64> = pairs.iter().map(|(pid, _)| *pid).collect();
    assert_eq!(got, pids, "urutan tetap ORDER BY pid");

    // Komponen tiap entity utuh dan tidak tertukar antar pid — inilah yang paling
    // mudah rusak saat baris banyak pid datang dari satu query bersama.
    for (i, (pid, e)) in pairs.iter().enumerate() {
        let mark = world.get::<QpMark>(*e).expect("QpMark termuat");
        assert_eq!(mark.level, i as i32, "pid {pid} membawa QpMark miliknya sendiri");
        let side = world.get::<QpSide>(*e).expect("komponen kedua ikut termuat");
        assert_eq!(side.note, format!("n{i}"), "pid {pid} membawa QpSide miliknya sendiri");
    }

    // Pid tanpa baris `arke_entities` dilewati, sama seperti jalur `fetch` lama.
    store.remove(pids[1]).await.unwrap();
    let mut world2 = World::new();
    let after = store.query_pids::<QpMark>(&mut world2, Some(&pred)).await.unwrap();
    let got2: Vec<i64> = after.iter().map(|(pid, _)| *pid).collect();
    assert_eq!(got2, vec![pids[0], pids[2]], "pid yang sudah dihapus tidak ikut");

    // Predikat tanpa hasil tidak boleh error (jalur `ids` kosong).
    let mut world3 = World::new();
    let none = store
        .query_pids::<QpMark>(&mut world3, Some("tag = 'qp-tidak-ada'"))
        .await
        .unwrap();
    assert!(none.is_empty(), "predikat tanpa hasil mengembalikan kosong");

    for pid in [pids[0], pids[2]] {
        store.remove(pid).await.unwrap();
    }
}
