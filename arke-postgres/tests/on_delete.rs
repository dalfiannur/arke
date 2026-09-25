//! Uji **aksi hapus relasi** (`#[pg(on_delete = "…")]`, RFC-0039): cascade
//! level-entity (rekursif, siklus aman), set_null, restrict (ditolak saat
//! commit), overwrite penuh `save` tetap jalan, dan `migrate` idempoten.
//! Dilewati bila `DATABASE_URL` tak diset.

use arke::World;
use arke_postgres::{OnDelete, PgComponent, PgStore, Ref};
use sqlx::PgPool;

#[derive(PgComponent, PartialEq, Debug)]
struct OdParent {
    name: String,
}

#[derive(PgComponent, PartialEq, Debug)]
struct OdChild {
    #[pg(on_delete = "cascade")]
    parent: Ref<OdParent>,
}

#[derive(PgComponent, PartialEq, Debug)]
struct OdGrand {
    #[pg(on_delete = "cascade")]
    child: Ref<OdChild>,
}

#[derive(PgComponent, PartialEq, Debug)]
struct OdNullable {
    #[pg(on_delete = "set_null")]
    parent: Option<Ref<OdParent>>,
}

#[derive(PgComponent, PartialEq, Debug)]
struct OdStrict {
    #[pg(on_delete = "restrict")]
    parent: Ref<OdParent>,
}

#[derive(PgComponent, PartialEq, Debug)]
struct OdNode {
    #[pg(on_delete = "cascade")]
    next: Option<Ref<OdNode>>,
}

async fn count(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

async fn exists(pool: &PgPool, pid: i64) -> bool {
    count(
        pool,
        &format!("SELECT count(*) FROM arke_entities WHERE pid = {pid}"),
    )
    .await
        == 1
}

#[test]
fn derive_menurunkan_aksi_hapus() {
    let d = OdChild::ON_DELETE;
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].column, "parent_id");
    assert_eq!(d[0].action, OnDelete::Cascade);
    assert_eq!(OdNullable::ON_DELETE[0].action, OnDelete::SetNull);
    assert_eq!(OdStrict::ON_DELETE[0].action, OnDelete::Restrict);
    assert!(OdParent::ON_DELETE.is_empty());
}

#[tokio::test]
async fn cascade_set_null_restrict() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let pool = PgPool::connect(&url).await.expect("pool");
    sqlx::query(
        "DROP TABLE IF EXISTS cmp_odparent, cmp_odchild, cmp_odgrand, cmp_odnullable, \
         cmp_odstrict, cmp_odnode",
    )
    .execute(&pool)
    .await
    .unwrap();

    let mut store = PgStore::connect(&url).await.unwrap();
    store
        .register::<OdParent>()
        .register::<OdChild>()
        .register::<OdGrand>()
        .register::<OdNullable>()
        .register::<OdStrict>()
        .register::<OdNode>();
    store.migrate().await.unwrap();

    // migrate idempoten: jumlah constraint & trigger arke tak bertambah.
    let objs = "SELECT (SELECT count(*) FROM pg_constraint WHERE conname LIKE 'afk\\_%') \
                + (SELECT count(*) FROM pg_trigger WHERE tgname LIKE 'arke\\_ondel\\_%')";
    let before = count(&pool, objs).await;
    store.migrate().await.unwrap();
    assert_eq!(count(&pool, objs).await, before);

    let mut world = World::new();
    let spawn_parent = |w: &mut World, name: &str| {
        let e = w.spawn();
        w.insert(e, OdParent { name: name.into() });
        e
    };
    let p1 = spawn_parent(&mut world, "p1");
    let p2 = spawn_parent(&mut world, "p2");
    let p3 = spawn_parent(&mut world, "p3");
    let c1 = world.spawn();
    world.insert(
        c1,
        OdChild {
            parent: Ref::new(p1),
        },
    );
    let g1 = world.spawn();
    world.insert(
        g1,
        OdGrand {
            child: Ref::new(c1),
        },
    );
    let n1 = world.spawn();
    world.insert(
        n1,
        OdNullable {
            parent: Some(Ref::new(p2)),
        },
    );
    let r1 = world.spawn();
    world.insert(
        r1,
        OdStrict {
            parent: Ref::new(p3),
        },
    );
    // Siklus a → b → a.
    let a = world.spawn();
    let b = world.spawn();
    world.insert(
        a,
        OdNode {
            next: Some(Ref::new(b)),
        },
    );
    world.insert(
        b,
        OdNode {
            next: Some(Ref::new(a)),
        },
    );

    store.save(&world).await.unwrap();
    // Overwrite penuh kedua kali (DELETE semua entity) tetap jalan.
    store.save(&world).await.unwrap();

    let pid = |e| store.pid_of(e).unwrap();
    let (p1, p2, p3, c1, g1, n1, r1, a, b) = (
        pid(p1),
        pid(p2),
        pid(p3),
        pid(c1),
        pid(g1),
        pid(n1),
        pid(r1),
        pid(a),
        pid(b),
    );

    // cascade rekursif: entity anak & cucu hilang utuh (bukan yatim).
    store.remove(p1).await.unwrap();
    assert!(!exists(&pool, c1).await);
    assert!(!exists(&pool, g1).await);

    // set_null: entity perujuk tetap, kolomnya NULL.
    store.remove(p2).await.unwrap();
    assert!(exists(&pool, n1).await);
    let parent: Option<i64> = sqlx::query_scalar(&format!(
        "SELECT parent_id FROM cmp_odnullable WHERE pid = {n1}"
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(parent, None);

    // restrict: ditolak, target tetap ada.
    assert!(store.remove(p3).await.is_err());
    assert!(exists(&pool, p3).await);
    assert!(exists(&pool, r1).await);
    // Hapus perujuk dulu → target boleh dihapus.
    store.remove(r1).await.unwrap();
    store.remove(p3).await.unwrap();

    // Siklus: satu hapus membawa keduanya, tanpa galat.
    store.remove(a).await.unwrap();
    assert!(!exists(&pool, a).await);
    assert!(!exists(&pool, b).await);

    // Integritas saat tulis: rujukan ke pid yang tak ada ditolak.
    let fresh: i64 =
        sqlx::query_scalar("INSERT INTO arke_entities (version) VALUES (0) RETURNING pid")
            .fetch_one(&pool)
            .await
            .unwrap();
    let bad = sqlx::query(&format!(
        "INSERT INTO cmp_odchild (pid, parent_id) VALUES ({fresh}, -42)"
    ))
    .execute(&pool)
    .await;
    assert!(bad.is_err(), "FK harus menolak pid yang tak ada");

    // Tabel perujuk di-DROP (komponen dibuang): trigger sisa tak boleh membuat
    // setiap DELETE entity gagal.
    sqlx::query("DROP TABLE cmp_odgrand")
        .execute(&pool)
        .await
        .unwrap();
    let mut w = World::new();
    let p = w.spawn();
    w.insert(p, OdParent { name: "x".into() });
    let staged = store.stage_insert(&w, p);
    let p = store.commit_insert(staged).await.unwrap();
    store.remove(p).await.unwrap();
    assert!(!exists(&pool, p).await);
}
