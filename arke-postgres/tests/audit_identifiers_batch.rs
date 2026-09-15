//! Regresi audit 0.16: identifier SQL di-quote (kata kunci `order`/`user`,
//! nama camelCase, tabel kustom `#[pg(table = "…")]`) dan jalur tulis **batch**
//! (`commit`/`commit_incremental` memakai `UNNEST`, lintas batas batch, semua
//! tipe kolom termasuk NULL). Dilewati bila `DATABASE_URL` tak diset.

#![allow(non_snake_case)]

use arke::{Entity, QueryData, World};
use arke_postgres::{Dir, PgComponent, PgStore};

/// Field ber-kata-kunci SQL + camelCase; tabel kustom (nama campur huruf besar).
#[derive(PgComponent, PartialEq, Debug, Clone)]
#[pg(table = "Audit_Bookings")]
struct Booking {
    #[pg(index)]
    order: i64,
    user: String,
    startAt: i64,
    #[pg(unique)]
    select: String,
}

/// Semua tipe kolom, nullable & tidak, untuk jalur batch.
#[derive(PgComponent, PartialEq, Debug, Clone)]
struct AuditAllTypes {
    i: i32,
    b: i64,
    n: u64,
    r: f32,
    d: f64,
    f: bool,
    t: String,
    j: Vec<i64>,
    oi: Option<i32>,
    ob: Option<i64>,
    on: Option<u64>,
    or: Option<f32>,
    od: Option<f64>,
    of: Option<bool>,
    ot: Option<String>,
    oj: Option<Vec<i64>>,
    e: Option<Entity>,
}

#[tokio::test]
async fn identifier_quoting_dan_batch_sekuensial() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::query("DROP TABLE IF EXISTS \"Audit_Bookings\", cmp_auditalltypes")
        .execute(&pool)
        .await
        .unwrap();

    let mut store = PgStore::connect(&url).await.unwrap();
    store.register::<Booking>().register::<AuditAllTypes>();
    store.migrate().await.unwrap();
    store.migrate().await.unwrap(); // idempoten dengan identifier ter-quote
    store.save(&World::new()).await.unwrap();

    identifier_quoting(&mut store).await;
    batch_semua_tipe_lintas_batas(&mut store).await;
}

async fn identifier_quoting(store: &mut PgStore) {
    assert_eq!(Booking::TABLE, "Audit_Bookings");
    let mut w = World::new();
    for i in 0..3 {
        let e = w.spawn();
        w.insert(
            e,
            Booking {
                order: i,
                user: format!("u{i}"),
                startAt: 100 + i,
                select: format!("s{i}"),
            },
        );
    }
    store.save(&w).await.unwrap();

    // Query builder: filter/order pada kolom kata-kunci & camelCase.
    let mut store2 = store.fork();
    let mut w2 = World::new();
    let n = store2
        .query::<Booking>()
        .filter(Booking::order().gte(1).and(Booking::user().like("u%")))
        .order_by(Booking::startAt(), Dir::Desc)
        .load(&mut w2)
        .await
        .unwrap();
    assert_eq!(n, 2);
    let mut got: Vec<i64> = Vec::new();
    <&Booking>::each(&mut w2, |b| got.push(b.order));
    got.sort();
    assert_eq!(got, vec![1, 2]);
    assert_eq!(store2.query::<Booking>().count().await.unwrap(), 3);

    // Agregat & mutasi massal pada kolom kata-kunci.
    let total = store2
        .query::<Booking>()
        .sum::<i64>(Booking::order())
        .await
        .unwrap();
    assert_eq!(total, Some(3));
    let changed = store2
        .update_where::<Booking>()
        .filter(Booking::select().eq("s0".to_string()))
        .set(Booking::user(), "root".to_string())
        .execute()
        .await
        .unwrap();
    assert_eq!(changed, 1);
    let deleted = store2
        .delete_where::<Booking>()
        .filter(Booking::order().eq(2))
        .execute()
        .await
        .unwrap();
    assert_eq!(deleted, 1);

    // Optimistic update & load penuh tetap benar.
    let mut w3 = World::new();
    store2.load(&mut w3).await.unwrap();
    let mut users: Vec<String> = Vec::new();
    <&Booking>::each(&mut w3, |b| users.push(b.user.clone()));
    users.sort();
    assert_eq!(users, vec!["root".to_string(), "u1".to_string()]);
}

async fn batch_semua_tipe_lintas_batas(store: &mut PgStore) {
    let n = 2_500; // > satu batch UNNEST (2_000)
    let mut w = World::new();
    let anchor = w.spawn();
    w.insert(anchor, AuditAllTypes::sample(0, None));
    let mut all = vec![anchor];
    for i in 1..n {
        let e = w.spawn();
        let e_ref = if i % 2 == 0 { Some(anchor) } else { None };
        w.insert(e, AuditAllTypes::sample(i, e_ref));
        all.push(e);
    }
    store.save(&w).await.unwrap();
    assert_eq!(
        store.query::<AuditAllTypes>().count().await.unwrap(),
        n as u64
    );

    // Muat penuh → nilai identik (termasuk NULL, NUMERIC besar, JSONB, relasi).
    let mut store2 = store.fork();
    let mut w2 = World::new();
    store2.load(&mut w2).await.unwrap();
    let mut rows: Vec<AuditAllTypes> = Vec::new();
    <&AuditAllTypes>::each(&mut w2, |a| rows.push(a.clone()));
    assert_eq!(rows.len(), n);
    rows.sort_by_key(|a| a.b);
    for (i, a) in rows.iter().enumerate() {
        let mut expect = AuditAllTypes::sample(i, a.e); // relasi dibandingkan terpisah
        expect.e = a.e;
        assert_eq!(a, &expect, "baris {i}");
        if i > 0 && i % 2 == 0 {
            let target = a.e.expect("relasi ke anchor");
            assert_eq!(w2.get::<AuditAllTypes>(target).map(|t| t.b), Some(0));
        } else {
            assert_eq!(a.e, None);
        }
    }

    // Inkremental batch: ubah 1.500 entity, hapus 300, tambah 100 baru.
    for (i, &e) in all.iter().enumerate().take(1_500) {
        let mut v = w.get::<AuditAllTypes>(e).unwrap().clone();
        v.i += 1;
        v.ot = Some(format!("x{i}"));
        w.insert(e, v);
    }
    for &e in &all[2_000..2_300] {
        w.despawn(e);
    }
    for i in 0..100 {
        let e = w.spawn();
        w.insert(e, AuditAllTypes::sample(10_000 + i, Some(anchor)));
    }
    let stats = store.save_incremental(&w).await.unwrap();
    assert_eq!((stats.written, stats.deleted), (1_600, 300));
    assert_eq!(
        store.query::<AuditAllTypes>().count().await.unwrap(),
        (n - 300 + 100) as u64
    );
    let mut w3 = World::new();
    let mut store3 = store.fork();
    let m = store3
        .query::<AuditAllTypes>()
        .filter(AuditAllTypes::ot().like("x%".to_string()))
        .load(&mut w3)
        .await
        .unwrap();
    assert_eq!(m, 1_500);
}

impl AuditAllTypes {
    fn sample(i: usize, e: Option<Entity>) -> Self {
        let odd = i % 2 == 1;
        Self {
            i: i as i32,
            b: i as i64,
            n: u64::MAX - i as u64,
            r: i as f32 * 0.5,
            d: i as f64 * 0.25,
            f: odd,
            t: format!("t{i}"),
            j: vec![i as i64, -1],
            oi: odd.then_some(i as i32),
            ob: odd.then_some(i as i64),
            on: odd.then_some(u64::MAX - 1),
            or: odd.then_some(1.5),
            od: odd.then_some(2.5),
            of: odd.then_some(true),
            ot: odd.then_some(format!("o{i}")),
            oj: odd.then_some(vec![7]),
            e,
        }
    }
}
