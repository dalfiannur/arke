//! Uji agregasi lanjutan: `count_distinct`, `group_by` multi-kunci, dan
//! `HAVING` typed. Dilewati bila `DATABASE_URL` tak diset. Komponen `Ag*`
//! unik ke berkas ini. Satu fungsi uji.

use arke::World;
use arke_postgres::aggregate as agg;
use arke_postgres::{PgComponent, PgStore};

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct AgBooking {
    room: i64,
    kind: String,
    minutes: i32,
}

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct AgVip {
    level: i32,
}

#[tokio::test]
async fn agregasi_lanjutan() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let mut store = PgStore::connect(&url).await.expect("connect");
    store.register::<AgBooking>().register::<AgVip>();
    store.migrate().await.unwrap();
    store.save(&World::new()).await.unwrap();

    // (room, kind, minutes, vip?)
    let rows: [(i64, &str, i32, bool); 7] = [
        (1, "meeting", 30, true),
        (1, "meeting", 60, false),
        (1, "call", 15, true),
        (2, "meeting", 120, false),
        (2, "call", 45, false),
        (2, "call", 45, true),
        (3, "workshop", 240, false),
    ];
    let mut world = World::new();
    for (room, kind, minutes, vip) in rows {
        let e = world.spawn();
        world.insert(
            e,
            AgBooking {
                room,
                kind: kind.to_string(),
                minutes,
            },
        );
        if vip {
            world.insert(e, AgVip { level: 1 });
        }
    }
    store.save(&world).await.unwrap();

    // ---- count_distinct global & per kunci ----
    let kinds = store
        .query::<AgBooking>()
        .count_distinct(AgBooking::kind())
        .await
        .unwrap();
    assert_eq!(kinds, 3);
    let per_room = store
        .query::<AgBooking>()
        .group_by(AgBooking::room())
        .count_distinct(AgBooking::kind())
        .await
        .unwrap();
    assert_eq!(per_room, vec![(1i64, 2u64), (2, 2), (3, 1)]);

    // ---- group_by multi-kunci (room, kind) ----
    let per_room_kind = store
        .query::<AgBooking>()
        .group_by((AgBooking::room(), AgBooking::kind()))
        .sum::<i64>(AgBooking::minutes())
        .await
        .unwrap();
    assert_eq!(
        per_room_kind,
        vec![
            ((1i64, "call".to_string()), Some(15i64)),
            ((1, "meeting".to_string()), Some(90)),
            ((2, "call".to_string()), Some(90)),
            ((2, "meeting".to_string()), Some(120)),
            ((3, "workshop".to_string()), Some(240)),
        ]
    );
    // Satu kunci lama tetap jalan.
    let per_room = store
        .query::<AgBooking>()
        .group_by(AgBooking::room())
        .count()
        .await
        .unwrap();
    assert_eq!(per_room, vec![(1i64, 3u64), (2, 3), (3, 1)]);

    // ---- HAVING: COUNT(*) > 1 → room 1 & 2 ----
    let busy = store
        .query::<AgBooking>()
        .group_by(AgBooking::room())
        .having(agg::count::<AgBooking>().gt(1))
        .count()
        .await
        .unwrap();
    assert_eq!(busy, vec![(1i64, 3u64), (2, 3)]);

    // ---- HAVING: SUM(minutes) >= 200 → room 2 (210), 3 (240) ----
    let long = store
        .query::<AgBooking>()
        .group_by(AgBooking::room())
        .having(agg::sum(AgBooking::minutes()).gte(200))
        .sum::<i64>(AgBooking::minutes())
        .await
        .unwrap();
    assert_eq!(long, vec![(2i64, Some(210i64)), (3, Some(240))]);

    // ---- HAVING gabungan and/or/not + WHERE + with_where ----
    // Booking VIP saja (with_where), per room, yang punya ≥ 2 kind berbeda ATAU
    // total menit > 100 — VIP: room1 {meeting30, call15}, room2 {call45}.
    let got = store
        .query::<AgBooking>()
        .with::<AgVip>()
        .group_by(AgBooking::room())
        .having(
            agg::count_distinct(AgBooking::kind())
                .gte(2)
                .or(agg::sum(AgBooking::minutes()).gt(100)),
        )
        .count()
        .await
        .unwrap();
    assert_eq!(got, vec![(1i64, 2u64)]);
    let got = store
        .query::<AgBooking>()
        .filter(AgBooking::minutes().lt(100))
        .group_by((AgBooking::room(), AgBooking::kind()))
        .having(agg::avg(AgBooking::minutes()).lt(40.0).not())
        .avg::<f64>(AgBooking::minutes())
        .await
        .unwrap();
    // minutes<100: (1,meeting)=[30,60] avg45; (1,call)=15; (2,call)=[45,45] avg45.
    assert_eq!(
        got,
        vec![
            ((1i64, "meeting".to_string()), Some(45.0)),
            ((2, "call".to_string()), Some(45.0)),
        ]
    );
    // HAVING tanpa hasil → kosong, bukan galat.
    let none = store
        .query::<AgBooking>()
        .group_by(AgBooking::kind())
        .having(agg::max(AgBooking::minutes()).eq(999))
        .count()
        .await
        .unwrap();
    assert!(none.is_empty());
}
