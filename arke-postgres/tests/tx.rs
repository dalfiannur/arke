//! Uji transaksi yang dipegang pemanggil (`PgStore::begin` → `PgTx`): pola
//! "lock → cek → tulis" atomik, mis. cek overlap booking lalu insert. Dilewati
//! bila `DATABASE_URL` tak diset. Komponen `Slot` unik ke berkas ini.

use arke::World;
use arke_postgres::{PgComponent, PgStore, PgTx};
use std::sync::Arc;
use tokio::sync::Barrier;

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct Slot {
    room: i64,
    start_ts: i64,
    end_ts: i64,
}

fn overlap(room: i64, start: i64, end: i64) -> arke_postgres::Filter<Slot> {
    Slot::room()
        .eq(room)
        .and(Slot::start_ts().lt(end))
        .and(Slot::end_ts().gt(start))
}

/// lock(room) → exists(overlap) → insert bila kosong → commit. `Ok(None)` = bentrok.
async fn book(store: &mut PgStore, slot: Slot) -> Result<Option<i64>, sqlx::Error> {
    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, slot.clone());
    let staged = store.stage_insert(&w, e);

    let mut tx: PgTx = store.begin().await?;
    tx.advisory_lock(slot.room).await?;
    let taken = store
        .query::<Slot>()
        .filter(overlap(slot.room, slot.start_ts, slot.end_ts))
        .exists_in(&mut tx)
        .await?;
    if taken {
        tx.rollback().await?;
        return Ok(None);
    }
    let pid = store.commit_insert_in(&mut tx, staged).await?;
    tx.commit().await?;
    Ok(Some(pid))
}

/// Store terdaftar tanpa `migrate` (skema disiapkan sekali oleh pemanggil —
/// `migrate` serentak dari banyak koneksi balapan di `CREATE TABLE IF NOT EXISTS`).
async fn store_for(url: &str) -> PgStore {
    let mut store = PgStore::connect(url).await.expect("connect");
    store.register::<Slot>();
    store
}

// Satu fungsi uji (bukan dua) supaya `migrate` & slate bersih tak balapan
// antar-test yang berbagi tabel `cmp_slot`.
#[tokio::test]
async fn lock_cek_insert_atomik_dan_serentak() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    // Tes lain (`constraints`/`migration`) men-DROP `arke_entities CASCADE`, yang
    // melucuti FK `cmp_slot` sisa run sebelumnya → `remove` tak lagi cascade.
    // Buat ulang tabel ini agar FK-nya segar.
    let pool = sqlx::PgPool::connect(&url).await.expect("pool");
    sqlx::query("DROP TABLE IF EXISTS cmp_slot")
        .execute(&pool)
        .await
        .unwrap();
    let mut store = store_for(&url).await;
    store.migrate().await.unwrap();
    store.save(&World::new()).await.unwrap(); // slate bersih

    // 1) Slot kosong → sukses; slot yang bentrok → None; slot lain → sukses.
    let a = book(
        &mut store,
        Slot {
            room: 1,
            start_ts: 10,
            end_ts: 20,
        },
    )
    .await
    .unwrap();
    assert!(a.is_some());
    let b = book(
        &mut store,
        Slot {
            room: 1,
            start_ts: 15,
            end_ts: 25,
        },
    )
    .await
    .unwrap();
    assert!(b.is_none(), "overlap harus ditolak");
    let c = book(
        &mut store,
        Slot {
            room: 2,
            start_ts: 15,
            end_ts: 25,
        },
    )
    .await
    .unwrap();
    assert!(c.is_some(), "ruang lain tak terpengaruh");
    assert_eq!(store.query::<Slot>().count().await.unwrap(), 2);

    // 2) count_in melihat baris yang di-insert di tx yang sama; drop tanpa commit
    //    = rollback → baris tak pernah ada di pool.
    {
        let mut w = World::new();
        let e = w.spawn();
        w.insert(
            e,
            Slot {
                room: 3,
                start_ts: 0,
                end_ts: 1,
            },
        );
        let staged = store.stage_insert(&w, e);
        let mut tx = store.begin().await.unwrap();
        store.commit_insert_in(&mut tx, staged).await.unwrap();
        let n_in = store
            .query::<Slot>()
            .filter(Slot::room().eq(3))
            .count_in(&mut tx)
            .await
            .unwrap();
        assert_eq!(n_in, 1, "terlihat di dalam tx");
        drop(tx);
    }
    let n_out = store
        .query::<Slot>()
        .filter(Slot::room().eq(3))
        .count()
        .await
        .unwrap();
    assert_eq!(n_out, 0, "drop tanpa commit = rollback");

    // 3) update_in + remove_in di satu tx.
    let pid_a = a.unwrap();
    {
        let mut w = World::new();
        let e = w.spawn();
        w.insert(
            e,
            Slot {
                room: 1,
                start_ts: 10,
                end_ts: 30,
            },
        );
        let staged = store.stage_update(&w, e);
        let mut tx = store.begin().await.unwrap();
        store
            .commit_update_in(&mut tx, pid_a, staged)
            .await
            .unwrap();
        store.remove_in(&mut tx, c.unwrap()).await.unwrap();
        tx.commit().await.unwrap();
    }
    let mut w = World::new();
    let e = store
        .fetch(&mut w, pid_a)
        .await
        .unwrap()
        .expect("pid_a ada");
    assert_eq!(w.get::<Slot>(e).unwrap().end_ts, 30);
    assert_eq!(store.query::<Slot>().count().await.unwrap(), 1);

    // 4) 8 task, ruang 7, slot sama, start bersamaan lewat barrier → tepat 1 sukses.
    let n = 8;
    let barrier = Arc::new(Barrier::new(n));
    let mut handles = Vec::new();
    for _ in 0..n {
        let url = url.clone();
        let barrier = barrier.clone();
        handles.push(tokio::spawn(async move {
            let mut store = store_for(&url).await;
            barrier.wait().await;
            book(
                &mut store,
                Slot {
                    room: 7,
                    start_ts: 100,
                    end_ts: 200,
                },
            )
            .await
            .unwrap()
        }));
    }
    let mut wins = 0;
    for h in handles {
        if h.await.unwrap().is_some() {
            wins += 1;
        }
    }
    assert_eq!(wins, 1, "advisory lock harus menserialkan cek+insert");
    let total = store
        .query::<Slot>()
        .filter(Slot::room().eq(7))
        .count()
        .await
        .unwrap();
    assert_eq!(total, 1);
}
