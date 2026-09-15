//! Uji `update_where`/`delete_where` (mutasi massal ber-filter) dan semi-join
//! by-value `in_where`. Dilewati bila `DATABASE_URL` tak diset. Komponen `Job`
//! dan `Queue` unik ke berkas ini.

use arke::World;
use arke_postgres::{PgComponent, PgStore};

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct Job {
    queue_code: String,
    status: String,
    attempts: i32,
    tags: Vec<i64>,
}

/// Tak ada `Ref` ke `Job` — hubungan lewat nilai `code` ↔ `queue_code`.
#[derive(PgComponent, PartialEq, Debug, Clone)]
struct Queue {
    code: String,
    paused: bool,
}

#[tokio::test]
async fn bulk_update_delete_dan_semi_join() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let pool = sqlx::PgPool::connect(&url).await.expect("pool");
    sqlx::query("DROP TABLE IF EXISTS cmp_job, cmp_queue")
        .execute(&pool)
        .await
        .unwrap();
    let mut store = PgStore::connect(&url).await.expect("connect");
    store.register::<Job>().register::<Queue>();
    store.migrate().await.unwrap();
    store.save(&World::new()).await.unwrap();

    // 2 queue (q1 paused, q2 aktif); 5 job: 3 di q1, 2 di q2.
    let mut w = World::new();
    for (code, paused) in [("q1", true), ("q2", false)] {
        let e = w.spawn();
        w.insert(
            e,
            Queue {
                code: code.into(),
                paused,
            },
        );
    }
    for i in 0..5 {
        let e = w.spawn();
        w.insert(
            e,
            Job {
                queue_code: if i < 3 { "q1" } else { "q2" }.into(),
                status: "pending".into(),
                attempts: i,
                tags: vec![i as i64],
            },
        );
    }
    store.save(&w).await.unwrap();

    // A) in_where: job yang queue-nya paused → 3.
    let n = store
        .query::<Job>()
        .filter(Job::queue_code().in_where(Queue::code(), Queue::paused().eq(true)))
        .count()
        .await
        .unwrap();
    assert_eq!(n, 3);

    // B1) update_where: pending & attempts >= 1 → status = failed, attempts = 0.
    //     Versi entity terdampak naik; yang lain tidak.
    let ver_before: Vec<(i64, i64)> =
        sqlx::query_as("SELECT pid, version FROM arke_entities ORDER BY pid")
            .fetch_all(&pool)
            .await
            .unwrap();
    let changed = store
        .update_where::<Job>()
        .filter(
            Job::status()
                .eq("pending".to_string())
                .and(Job::attempts().gte(1)),
        )
        .set(Job::status(), "failed".to_string())
        .set(Job::attempts(), 0)
        .execute()
        .await
        .unwrap();
    assert_eq!(changed, 4); // attempts 1..=4
    let failed = store
        .query::<Job>()
        .filter(
            Job::status()
                .eq("failed".to_string())
                .and(Job::attempts().eq(0)),
        )
        .count()
        .await
        .unwrap();
    assert_eq!(failed, 4);
    let ver_after: Vec<(i64, i64)> =
        sqlx::query_as("SELECT pid, version FROM arke_entities ORDER BY pid")
            .fetch_all(&pool)
            .await
            .unwrap();
    let bumped = ver_before
        .iter()
        .zip(&ver_after)
        .filter(|(b, a)| a.1 == b.1 + 1)
        .count();
    assert_eq!(bumped, 4, "hanya entity terdampak yang versinya naik");

    // B2) set kolom JSONB + tanpa filter → semua baris.
    let all = store
        .update_where::<Job>()
        .set(Job::tags(), vec![9, 9])
        .execute()
        .await
        .unwrap();
    assert_eq!(all, 5);
    assert_eq!(
        store
            .query::<Job>()
            .filter(Job::tags().contains(9))
            .count()
            .await
            .unwrap(),
        5
    );

    // B3) tanpa `set` → no-op (0), bukan error.
    assert_eq!(
        store
            .update_where::<Job>()
            .filter(Job::attempts().eq(0))
            .execute()
            .await
            .unwrap(),
        0
    );

    // B4) delete_where menghapus ENTITY (cascade), bukan hanya komponen Job.
    let deleted = store
        .delete_where::<Job>()
        .filter(Job::queue_code().eq("q2".to_string()))
        .execute()
        .await
        .unwrap();
    assert_eq!(deleted, 2);
    assert_eq!(store.query::<Job>().count().await.unwrap(), 3);
    let ents: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM arke_entities")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(ents, 5, "2 queue + 3 job");

    // B5) execute_in bersama PgTx: rollback → tak ada perubahan.
    {
        let mut tx = store.begin().await.unwrap();
        let n = store
            .update_where::<Job>()
            .set(Job::status(), "x".to_string())
            .execute_in(&mut tx)
            .await
            .unwrap();
        assert_eq!(n, 3);
        tx.rollback().await.unwrap();
    }
    assert_eq!(
        store
            .query::<Job>()
            .filter(Job::status().eq("x".to_string()))
            .count()
            .await
            .unwrap(),
        0
    );
}
