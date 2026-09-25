//! Uji kolom `UUID` (`uuid::Uuid`), `TIMESTAMPTZ` (`chrono::DateTime<Utc>`) dan
//! `#[pg(text)]` (enum via `Display`/`FromStr`) round-trip vs Postgres, beserta
//! filter & keyset di atasnya. Dilewati tanpa `DATABASE_URL`.
#![cfg(all(feature = "uuid", feature = "chrono"))]

use std::fmt;
use std::str::FromStr;

use arke::World;
use arke_postgres::{IntoPgValue, PgComponent, PgStore, PgValue};
use chrono::{DateTime, TimeZone, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Status {
    Open,
    Done,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Status::Open => "open",
            Status::Done => "done",
        })
    }
}

impl FromStr for Status {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "open" => Ok(Status::Open),
            "done" => Ok(Status::Done),
            _ => Err(()),
        }
    }
}

impl IntoPgValue for Status {
    fn into_pg_value(self) -> PgValue {
        PgValue::Text(self.to_string())
    }
}

#[derive(PgComponent, PartialEq, Debug, Clone)]
#[pg(table = "typed_ticket", check = "status IN ('open', 'done')")]
struct Ticket {
    #[pg(unique)]
    public_id: Uuid,
    owner: Option<Uuid>,
    created_at: DateTime<Utc>,
    closed_at: Option<chrono::DateTime<chrono::Utc>>,
    #[pg(text)]
    status: Status,
    #[pg(text)]
    prev: Option<Status>,
}

fn ts(secs: i64, micros: u32) -> DateTime<Utc> {
    Utc.timestamp_opt(secs, micros * 1_000).unwrap()
}

#[tokio::test]
async fn uuid_timestamptz_text_round_trip_filter_dan_keyset() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    // Zona sesi non-UTC tak boleh menggeser nilai yang dibaca: berlaku untuk
    // koneksi baru, jadi dipasang sebelum store tersambung.
    let pool = sqlx::PgPool::connect(&url).await.expect("connect");
    let db: String = sqlx::query_scalar("SELECT current_database()::text")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(&format!(
        "ALTER DATABASE \"{db}\" SET timezone = 'Asia/Jakarta'"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let mut store = PgStore::connect(&url).await.expect("connect");
    store.register::<Ticket>();
    store.migrate().await.unwrap();
    sqlx::query("TRUNCATE typed_ticket")
        .execute(&pool)
        .await
        .unwrap();

    // Kolom benar-benar bertipe (bukan TEXT) — bisa di-query service lain.
    let types: Vec<(String, String)> = sqlx::query_as(
        "SELECT column_name::text, data_type::text FROM information_schema.columns \
         WHERE table_name = 'typed_ticket' ORDER BY ordinal_position",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let ty = |c: &str| types.iter().find(|(n, _)| n == c).unwrap().1.clone();
    assert_eq!(ty("public_id"), "uuid");
    assert_eq!(ty("owner"), "uuid");
    assert_eq!(ty("created_at"), "timestamp with time zone");
    assert_eq!(ty("status"), "text");

    let a = Ticket {
        public_id: Uuid::from_u128(0x0192_0000_0000_7000_8000_0000_0000_0001),
        owner: Some(Uuid::from_u128(42)),
        created_at: ts(1_758_800_000, 123_456),
        closed_at: Some(ts(1_758_900_000, 0)),
        status: Status::Done,
        prev: Some(Status::Open),
    };
    let b = Ticket {
        public_id: Uuid::from_u128(0x0192_0000_0000_7000_8000_0000_0000_0002),
        owner: None,
        created_at: ts(1_758_800_001, 0),
        closed_at: None,
        status: Status::Open,
        prev: None,
    };
    let mut world = World::new();
    for t in [a.clone(), b.clone()] {
        let e = world.spawn();
        world.insert(e, t);
    }
    store.save(&world).await.unwrap();

    let mut loaded = World::new();
    store.load(&mut loaded).await.unwrap();
    let mut got: Vec<Ticket> = loaded.query::<Ticket>().cloned().collect();
    got.sort_by_key(|t| t.created_at);
    assert_eq!(got, vec![a.clone(), b.clone()]);

    // Filter per tipe baru.
    let by_id = store
        .query::<Ticket>()
        .filter(Ticket::public_id().eq(a.public_id))
        .count()
        .await
        .unwrap();
    assert_eq!(by_id, 1);
    let after = store
        .query::<Ticket>()
        .filter(Ticket::created_at().gt(ts(1_758_800_000, 500_000)))
        .count()
        .await
        .unwrap();
    assert_eq!(after, 1);
    let open = store
        .query::<Ticket>()
        .filter(Ticket::status().eq(Status::Open))
        .count()
        .await
        .unwrap();
    assert_eq!(open, 1);

    // Keyset di atas kunci TIMESTAMPTZ (paginasi pesan urut waktu).
    let first = store
        .query::<Ticket>()
        .order_by(Ticket::created_at(), arke_postgres::Dir::Asc)
        .limit(1)
        .load_page(&mut World::new())
        .await
        .unwrap();
    let next = first.next.expect("halaman berikut");
    let mut w2 = World::new();
    let second = store
        .query::<Ticket>()
        .order_by(Ticket::created_at(), arke_postgres::Dir::Asc)
        .after(next)
        .limit(1)
        .load_page(&mut w2)
        .await
        .unwrap();
    assert_eq!(second.items.len(), 1);
    let t = w2.get::<Ticket>(second.items[0].1).unwrap();
    assert_eq!(t.public_id, b.public_id);

    sqlx::query(&format!("ALTER DATABASE \"{db}\" RESET timezone"))
        .execute(&pool)
        .await
        .unwrap();
}
