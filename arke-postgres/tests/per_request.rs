//! Pola **World per-request** (REST/axum): satu `PgStore` template di state,
//! `fork()` per handler; id publik = kolom UUID `#[pg(unique)]`, bukan `Entity`
//! maupun `pid`. Dilewati bila `DATABASE_URL` tak diset. Komponen `Ticket` unik
//! ke berkas ini.

use arke::World;
use arke_postgres::{PgComponent, PgStore, UpdateError};

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct Ticket {
    /// Id publik (UUID string) — lookup REST lewat kolom ini.
    #[pg(unique)]
    ticket_id: String,
    title: String,
    priority: i32,
}

/// POST: World kosong + spawn → `save_incremental` = INSERT murni.
async fn create(tpl: &PgStore, t: Ticket) -> i64 {
    let mut store = tpl.fork();
    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, t);
    let stats = store.save_incremental(&w).await.unwrap();
    assert_eq!((stats.written, stats.deleted), (1, 0));
    store.pid_of(e).expect("pid teralokasi setelah save")
}

/// GET by uuid: builder → `load_pids` → `(pid, Entity)`.
async fn get(tpl: &PgStore, id: &str) -> Option<(i64, Ticket)> {
    let mut store = tpl.fork();
    let mut w = World::new();
    let hits = store
        .query::<Ticket>()
        .filter(Ticket::ticket_id().eq(id.to_string()))
        .load_pids(&mut w)
        .await
        .unwrap();
    let &(pid, e) = hits.first()?;
    assert_eq!(store.pid_of(e), Some(pid));
    assert_eq!(store.entity_of(pid), Some(e));
    Some((pid, w.get::<Ticket>(e).unwrap().clone()))
}

#[tokio::test]
async fn world_per_request_lewat_fork() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    // Tes lain men-DROP `arke_entities CASCADE` → FK `cmp_ticket` sisa run
    // sebelumnya hilang → `remove` tak cascade. Buat ulang agar FK segar.
    let pool = sqlx::PgPool::connect(&url).await.expect("pool");
    sqlx::query("DROP TABLE IF EXISTS cmp_ticket")
        .execute(&pool)
        .await
        .unwrap();
    let mut tpl = PgStore::connect(&url).await.expect("connect");
    tpl.register::<Ticket>();
    tpl.migrate().await.unwrap();
    tpl.save(&World::new()).await.unwrap(); // slate bersih

    let mk = |id: &str, p: i32| Ticket {
        ticket_id: id.to_string(),
        title: format!("t-{id}"),
        priority: p,
    };
    let pid_a = create(&tpl, mk("aaaa-1", 1)).await;
    let pid_b = create(&tpl, mk("bbbb-2", 2)).await;
    assert_ne!(pid_a, pid_b);
    // UNIQUE: uuid ganda ditolak.
    {
        let mut store = tpl.fork();
        let mut w = World::new();
        let e = w.spawn();
        w.insert(e, mk("aaaa-1", 9));
        assert!(store.save_incremental(&w).await.is_err());
    }

    // GET by uuid.
    let (pid, t) = get(&tpl, "bbbb-2").await.expect("ada");
    assert_eq!(pid, pid_b);
    assert_eq!(t.priority, 2);
    assert!(get(&tpl, "zzzz").await.is_none());

    // PATCH: fork → load subset → ubah → save_incremental hanya menyentuh subset.
    {
        let mut store = tpl.fork();
        let mut w = World::new();
        let hits = store
            .query::<Ticket>()
            .filter(Ticket::ticket_id().eq("aaaa-1".to_string()))
            .load_pids(&mut w)
            .await
            .unwrap();
        let (_, e) = hits[0];
        // Core belum punya `get_mut`; `insert` = upsert di tempat.
        let mut t = w.get::<Ticket>(e).unwrap().clone();
        t.priority = 5;
        w.insert(e, t);
        let stats = store.save_incremental(&w).await.unwrap();
        assert_eq!((stats.written, stats.deleted), (1, 0));
    }
    assert_eq!(get(&tpl, "aaaa-1").await.unwrap().1.priority, 5);
    assert_eq!(
        get(&tpl, "bbbb-2").await.unwrap().1.priority,
        2,
        "di luar subset tak tersentuh"
    );

    // PATCH ber-optimistic-lock: dua request memuat versi sama → yang kedua 409.
    {
        let mut s1 = tpl.fork();
        let mut w1 = World::new();
        let e1 = s1
            .query::<Ticket>()
            .filter(Ticket::ticket_id().eq("bbbb-2".to_string()))
            .load_pids(&mut w1)
            .await
            .unwrap()[0]
            .1;
        let v1 = s1.entity_version(e1).await.unwrap().unwrap();

        let mut s2 = tpl.fork();
        let mut w2 = World::new();
        let e2 = s2
            .query::<Ticket>()
            .filter(Ticket::ticket_id().eq("bbbb-2".to_string()))
            .load_pids(&mut w2)
            .await
            .unwrap()[0]
            .1;
        let v2 = s2.entity_version(e2).await.unwrap().unwrap();
        assert_eq!(v1, v2);

        let mut t1 = w1.get::<Ticket>(e1).unwrap().clone();
        t1.priority = 7;
        w1.insert(e1, t1);
        s1.update_entity(&w1, e1, v1).await.unwrap();
        let mut t2 = w2.get::<Ticket>(e2).unwrap().clone();
        t2.priority = 8;
        w2.insert(e2, t2);
        assert!(matches!(
            s2.update_entity(&w2, e2, v2).await,
            Err(UpdateError::Conflict)
        ));
    }
    assert_eq!(get(&tpl, "bbbb-2").await.unwrap().1.priority, 7);

    // DELETE by uuid: pid dari load_pids → remove.
    {
        let (pid, _) = get(&tpl, "aaaa-1").await.unwrap();
        tpl.remove(pid).await.unwrap();
    }
    assert!(get(&tpl, "aaaa-1").await.is_none());
    let mut store = tpl.fork();
    assert_eq!(store.query::<Ticket>().count().await.unwrap(), 1);

    // Template tak pernah menyentuh World: jembatannya tetap kosong.
    assert_eq!(tpl.entity_of(pid_b), None);
}
