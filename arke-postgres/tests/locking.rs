//! Uji **penguncian baris** (RFC-0040): `Query::for_update()` dengan
//! `skip_locked`/`nowait`, hanya lewat transaksi (`pids_in`). Pola worker
//! antrean: klaim satu job `pending` tanpa menunggu worker lain.
//! Dilewati bila `DATABASE_URL` tak diset.

use arke::World;
use arke_postgres::{Dir, PgComponent, PgStore};

#[derive(PgComponent, PartialEq, Debug)]
struct LkJob {
    status: String,
    n: i32,
}

#[tokio::test]
async fn skip_locked_dan_nowait() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let pool = sqlx::PgPool::connect(&url).await.expect("pool");
    sqlx::query("DROP TABLE IF EXISTS cmp_lkjob")
        .execute(&pool)
        .await
        .unwrap();
    let mut store = PgStore::connect(&url).await.unwrap();
    store.register::<LkJob>();
    store.migrate().await.unwrap();
    let mut pids = Vec::new();
    for n in 1..=3 {
        let mut w = World::new();
        let e = w.spawn();
        w.insert(
            e,
            LkJob {
                status: "pending".into(),
                n,
            },
        );
        let staged = store.stage_insert(&w, e);
        pids.push(store.commit_insert(staged).await.unwrap());
    }

    // Worker 1 mengklaim job pertama.
    let mut tx1 = store.begin().await.unwrap();
    let got1 = store
        .query::<LkJob>()
        .filter(LkJob::status().eq("pending".to_string()))
        .order_by(LkJob::n(), Dir::Asc)
        .limit(1)
        .for_update()
        .skip_locked()
        .pids_in(&mut tx1)
        .await
        .unwrap();
    assert_eq!(got1, vec![pids[0]]);

    // Worker 2 melompati yang terkunci, tanpa menunggu.
    let mut tx2 = store.begin().await.unwrap();
    let got2 = store
        .query::<LkJob>()
        .filter(LkJob::status().eq("pending".to_string()))
        .order_by(LkJob::n(), Dir::Asc)
        .limit(1)
        .for_update()
        .skip_locked()
        .pids_in(&mut tx2)
        .await
        .unwrap();
    assert_eq!(got2, vec![pids[1]]);

    // NOWAIT pada baris terkunci → galat 55P03 seketika.
    let mut tx3 = store.begin().await.unwrap();
    let err = store
        .query::<LkJob>()
        .filter(LkJob::n().eq(1))
        .for_update()
        .nowait()
        .pids_in(&mut tx3)
        .await
        .expect_err("baris terkunci");
    let code = err
        .as_database_error()
        .and_then(|e| e.code())
        .map(|c| c.to_string());
    assert_eq!(code.as_deref(), Some("55P03"), "{err}");
    tx3.rollback().await.unwrap();

    // Worker 1 menandai job-nya lalu commit; kunci lepas.
    store
        .update_where::<LkJob>()
        .filter(LkJob::n().eq(1))
        .set(LkJob::status(), "running".to_string())
        .execute_in(&mut tx1)
        .await
        .unwrap();
    tx1.commit().await.unwrap();

    // Worker 3: job 1 bukan pending lagi, job 2 masih dikunci worker 2 → job 3.
    let mut tx4 = store.begin().await.unwrap();
    let got4 = store
        .query::<LkJob>()
        .filter(LkJob::status().eq("pending".to_string()))
        .order_by(LkJob::n(), Dir::Asc)
        .limit(1)
        .for_update()
        .skip_locked()
        .pids_in(&mut tx4)
        .await
        .unwrap();
    assert_eq!(got4, vec![pids[2]]);
    tx4.rollback().await.unwrap();
    tx2.rollback().await.unwrap();
}
