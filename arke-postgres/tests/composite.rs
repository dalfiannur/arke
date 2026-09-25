//! Uji indeks komposit level-tipe (`#[pg(index(a, b))]`/`#[pg(unique(a, b))]`,
//! RFC-0037): dibuat `migrate`, idempoten, ditegakkan Postgres, relasi dipetakan
//! ke `<name>_id`, dan indeks usang ber-awalan `cidx_<tabel>_` dibuang.
//! Dilewati bila `DATABASE_URL` tak diset.

use arke::{Entity, World};
use arke_postgres::{PgComponent, PgStore};
use sqlx::PgPool;

#[derive(PgComponent, PartialEq, Debug)]
#[pg(unique(channel, wa_id), index(channel, at))]
struct Msg {
    channel: Entity,
    wa_id: Option<String>,
    at: i64,
}

#[derive(PgComponent, PartialEq, Debug)]
struct Chan {
    name: String,
}

/// `(indexname, indexdef)` indeks `cidx_*` milik `cmp_msg`, urut nama.
async fn composites(pool: &PgPool) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT indexname::text, indexdef FROM pg_indexes \
         WHERE tablename = 'cmp_msg' AND indexname LIKE 'cidx\\_%' ORDER BY indexname",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

#[test]
fn derive_menurunkan_definisi_komposit() {
    let defs = Msg::COMPOSITE_INDEXES;
    assert_eq!(defs.len(), 2);
    assert_eq!(defs[0].columns, &["channel_id", "wa_id"]);
    assert!(defs[0].unique);
    assert_eq!(defs[1].columns, &["channel_id", "at"]);
    assert!(!defs[1].unique);
}

#[tokio::test]
async fn komposit_dibuat_idempoten_ditegakkan_dan_usang_dibuang() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let pool = PgPool::connect(&url).await.expect("pool");
    sqlx::query("DROP TABLE IF EXISTS cmp_msg CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let mut store = PgStore::connect(&url).await.unwrap();
    store.register::<Chan>().register::<Msg>();
    store.migrate().await.unwrap();

    let got = composites(&pool).await;
    assert_eq!(got.len(), 2, "{got:?}");
    let unique: Vec<_> = got.iter().filter(|(_, d)| d.contains("UNIQUE")).collect();
    assert_eq!(unique.len(), 1);
    assert!(
        unique[0].1.contains("(channel_id, wa_id)"),
        "{}",
        unique[0].1
    );
    assert!(
        got.iter()
            .any(|(_, d)| !d.contains("UNIQUE") && d.contains("(channel_id, at)")),
        "{got:?}"
    );

    // Idempoten: migrate ulang tak mengubah apa pun (nama sama).
    store.migrate().await.unwrap();
    assert_eq!(composites(&pool).await, got);

    // Indeks usang ber-awalan milik tabel ini dibuang; indeks lain tak disentuh.
    sqlx::query("CREATE INDEX cidx_cmp_msg_0000000000000000 ON cmp_msg (at)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE INDEX IF NOT EXISTS manual_cmp_msg_at ON cmp_msg (at)")
        .execute(&pool)
        .await
        .unwrap();
    store.migrate().await.unwrap();
    assert_eq!(composites(&pool).await, got);
    let manual: Option<String> = sqlx::query_scalar(
        "SELECT indexname::text FROM pg_indexes WHERE indexname = 'manual_cmp_msg_at'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(
        manual.is_some(),
        "indeks di luar awalan cidx_ tak boleh disentuh"
    );

    // Ditegakkan: pasangan (channel, wa_id) kembar ditolak; NULL tetap distinct.
    let mut world = World::new();
    let ch = world.spawn();
    world.insert(ch, Chan { name: "c".into() });
    let a = world.spawn();
    world.insert(
        a,
        Msg {
            channel: ch,
            wa_id: Some("w1".into()),
            at: 1,
        },
    );
    let n1 = world.spawn();
    world.insert(
        n1,
        Msg {
            channel: ch,
            wa_id: None,
            at: 2,
        },
    );
    let n2 = world.spawn();
    world.insert(
        n2,
        Msg {
            channel: ch,
            wa_id: None,
            at: 3,
        },
    );
    store.save(&world).await.expect("NULL wa_id boleh kembar");

    let b = world.spawn();
    world.insert(
        b,
        Msg {
            channel: ch,
            wa_id: Some("w1".into()),
            at: 4,
        },
    );
    let err = store
        .save(&world)
        .await
        .expect_err("duplikat harus ditolak");
    let code = err
        .as_database_error()
        .and_then(|e| e.code())
        .map(|c| c.to_string());
    assert_eq!(code.as_deref(), Some("23505"), "{err}");
}
