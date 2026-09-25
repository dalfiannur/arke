//! Uji **upsert** (RFC-0038): `PgStore::upsert::<T>(staged)` dengan target
//! konflik indeks unik komposit, aksi DO NOTHING / DO UPDATE / coalesce, pid
//! alokasi yang dilepas saat konflik, komponen lain hanya ditulis untuk baris
//! baru, konkurensi, dan varian transaksi. Dilewati tanpa `DATABASE_URL`.

use arke::World;
use arke_postgres::{PgComponent, PgStore};
use sqlx::PgPool;

#[derive(PgComponent, PartialEq, Debug, Clone)]
#[pg(unique(ws, chat))]
struct UContact {
    ws: i64,
    chat: String,
    push: Option<String>,
    avatar: Option<String>,
}

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct UTag {
    n: i32,
}

fn contact(chat: &str, push: Option<&str>, avatar: Option<&str>) -> UContact {
    UContact {
        ws: 1,
        chat: chat.into(),
        push: push.map(Into::into),
        avatar: avatar.map(Into::into),
    }
}

/// Stage satu entity baru berisi `c` (+ `UTag` bila `tag`).
fn staged(store: &PgStore, c: UContact, tag: Option<i32>) -> arke_postgres::StagedInsert {
    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, c);
    if let Some(n) = tag {
        w.insert(e, UTag { n });
    }
    store.stage_insert(&w, e)
}

/// Tes di berkas ini menghitung `arke_entities` global → dijalankan bergiliran.
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn scalar(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

async fn setup(url: &str) -> (PgPool, PgStore) {
    let pool = PgPool::connect(url).await.expect("pool");
    sqlx::query("DROP TABLE IF EXISTS cmp_ucontact, cmp_utag")
        .execute(&pool)
        .await
        .unwrap();
    let mut store = PgStore::connect(url).await.unwrap();
    store.register::<UContact>().register::<UTag>();
    store.migrate().await.unwrap();
    (pool, store)
}

#[tokio::test]
async fn upsert_do_nothing_dan_do_update() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let _serial = SERIAL.lock().await;
    let (pool, store) = setup(&url).await;
    let entities_before = scalar(&pool, "SELECT count(*) FROM arke_entities").await;

    // Baris baru: inserted, komponen lain ikut tertulis.
    let first = store
        .upsert::<UContact>(staged(&store, contact("a", Some("Ani"), None), Some(7)))
        .on(UContact::ws())
        .on(UContact::chat())
        .execute()
        .await
        .unwrap();
    assert!(first.inserted);

    // Konflik + DO NOTHING: pid lama, tak ada pid bocor, UTag tak digandakan.
    let again = store
        .upsert::<UContact>(staged(&store, contact("a", Some("X"), None), Some(9)))
        .on(UContact::ws())
        .on(UContact::chat())
        .execute()
        .await
        .unwrap();
    assert_eq!((again.pid, again.inserted), (first.pid, false));
    assert_eq!(
        scalar(&pool, "SELECT count(*) FROM arke_entities").await,
        entities_before + 1
    );
    assert_eq!(scalar(&pool, "SELECT count(*) FROM cmp_utag").await, 1);
    let push: String = sqlx::query_scalar("SELECT push FROM cmp_ucontact WHERE chat = 'a'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(push, "Ani", "DO NOTHING tak mengubah baris");

    // DO UPDATE: `push` coalesce (None tak menimpa), `avatar` ditimpa.
    let version_before = scalar(
        &pool,
        &format!(
            "SELECT version FROM arke_entities WHERE pid = {}",
            first.pid
        ),
    )
    .await;
    let upd = store
        .upsert::<UContact>(staged(&store, contact("a", None, Some("u1")), None))
        .on(UContact::ws())
        .on(UContact::chat())
        .update_coalesce(UContact::push())
        .update(UContact::avatar())
        .execute()
        .await
        .unwrap();
    assert_eq!((upd.pid, upd.inserted), (first.pid, false));
    let row: (Option<String>, Option<String>) =
        sqlx::query_as("SELECT push, avatar FROM cmp_ucontact WHERE chat = 'a'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row, (Some("Ani".into()), Some("u1".into())));
    assert_eq!(
        scalar(
            &pool,
            &format!(
                "SELECT version FROM arke_entities WHERE pid = {}",
                first.pid
            )
        )
        .await,
        version_before + 1,
        "DO UPDATE menaikkan version (optimistic lock)"
    );
    assert_eq!(
        scalar(&pool, "SELECT count(*) FROM arke_entities").await,
        entities_before + 1
    );

    // Staged tanpa komponen T → galat, bukan insert diam-diam.
    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, UTag { n: 1 });
    let err = store
        .upsert::<UContact>(store.stage_insert(&w, e))
        .on(UContact::chat())
        .execute()
        .await;
    assert!(err.is_err());

    // Varian transaksi: rollback → tak ada jejak.
    let mut tx = store.begin().await.unwrap();
    let r = store
        .upsert::<UContact>(staged(&store, contact("tx", None, None), None))
        .on(UContact::ws())
        .on(UContact::chat())
        .execute_in(&mut tx)
        .await
        .unwrap();
    assert!(r.inserted);
    tx.rollback().await.unwrap();
    assert_eq!(
        scalar(&pool, "SELECT count(*) FROM cmp_ucontact WHERE chat = 'tx'").await,
        0
    );
}

/// Komponen sendiri untuk uji paralel, terpisah dari tabel yang di-DROP `setup`.
#[derive(PgComponent, PartialEq, Debug, Clone)]
#[pg(unique(chat))]
struct RContact {
    chat: String,
}

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct RTag {
    n: i32,
}

#[tokio::test]
async fn upsert_paralel_satu_pemenang() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let _serial = SERIAL.lock().await;
    let pool = PgPool::connect(&url).await.expect("pool");
    sqlx::query("DROP TABLE IF EXISTS cmp_rcontact, cmp_rtag")
        .execute(&pool)
        .await
        .unwrap();
    let mut store = PgStore::connect(&url).await.unwrap();
    store.register::<RContact>().register::<RTag>();
    store.migrate().await.unwrap();

    let store = std::sync::Arc::new(store);
    let mut handles = Vec::new();
    for i in 0..8 {
        let store = store.clone();
        handles.push(tokio::spawn(async move {
            let mut w = World::new();
            let e = w.spawn();
            w.insert(
                e,
                RContact {
                    chat: "race".into(),
                },
            );
            w.insert(e, RTag { n: 100 + i });
            let staged = store.stage_insert(&w, e);
            store
                .upsert::<RContact>(staged)
                .on(RContact::chat())
                .execute()
                .await
                .unwrap()
        }));
    }
    let mut results = Vec::new();
    for h in handles {
        results.push(h.await.unwrap());
    }
    let pid = results[0].pid;
    assert!(results.iter().all(|r| r.pid == pid), "{results:?}");
    assert_eq!(results.iter().filter(|r| r.inserted).count(), 1);
    assert_eq!(
        scalar(
            &pool,
            &format!("SELECT count(*) FROM cmp_rtag WHERE pid = {pid}")
        )
        .await,
        1
    );
}
