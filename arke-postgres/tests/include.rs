//! Uji `Query::include(T::rel())` (RFC-0041): muat entity target relasi
//! bertipe `Ref<T>`/`Option<Ref<T>>` untuk baris yang dimuat — hanya untuk
//! halaman hasil (limit/order/kursor), relasi resolve ke entity target, dan
//! `None` tetap `None`. Dilewati bila `DATABASE_URL` tak diset.

use arke::World;
use arke_postgres::{Dir, PgComponent, PgStore, Ref};

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct IncAuthor {
    name: String,
}

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct IncBook {
    n: i32,
    author: Ref<IncAuthor>,
    editor: Option<Ref<IncAuthor>>,
}

#[tokio::test]
async fn include_memuat_target_relasi_halaman_saja() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let pool = sqlx::PgPool::connect(&url).await.expect("pool");
    sqlx::query("DROP TABLE IF EXISTS cmp_incauthor, cmp_incbook")
        .execute(&pool)
        .await
        .unwrap();
    let mut tpl = PgStore::connect(&url).await.unwrap();
    tpl.register::<IncAuthor>().register::<IncBook>();
    tpl.migrate().await.unwrap();

    // a1..a3 penulis; b1(a1, editor a3), b2(a2, tanpa editor), b3(a3).
    let mut store = tpl.fork();
    let mut w = World::new();
    let mut authors = Vec::new();
    for name in ["a1", "a2", "a3"] {
        let e = w.spawn();
        w.insert(e, IncAuthor { name: name.into() });
        authors.push(e);
    }
    for (n, a, ed) in [(1, 0, Some(2)), (2, 1, None), (3, 2, None)] {
        let e = w.spawn();
        w.insert(
            e,
            IncBook {
                n,
                author: Ref::new(authors[a]),
                editor: ed.map(|i| Ref::new(authors[i])),
            },
        );
    }
    store.save_incremental(&w).await.unwrap();

    // Halaman 1 buku (n = 1): penulis & editornya ikut, penulis lain tidak.
    let mut store = tpl.fork();
    let mut world = World::new();
    let got = store
        .query::<IncBook>()
        .order_by(IncBook::n(), Dir::Asc)
        .limit(1)
        .include(IncBook::author())
        .include(IncBook::editor())
        .load_pids(&mut world)
        .await
        .unwrap();
    assert_eq!(got.len(), 1);
    let book = world.get::<IncBook>(got[0].1).unwrap().clone();
    assert_eq!(book.n, 1);
    let author = world
        .get::<IncAuthor>(book.author.entity())
        .expect("author termuat");
    assert_eq!(author.name, "a1");
    let editor = world
        .get::<IncAuthor>(book.editor.expect("editor").entity())
        .expect("editor termuat");
    assert_eq!(editor.name, "a3");
    let mut names: Vec<String> = world.query::<IncAuthor>().map(|a| a.name.clone()).collect();
    names.sort();
    assert_eq!(names, vec!["a1", "a3"], "hanya target halaman ini");

    // Filter + Option None: relasi kosong tetap None, tanpa galat.
    let mut store = tpl.fork();
    let mut world = World::new();
    let got = store
        .query::<IncBook>()
        .filter(IncBook::n().eq(2))
        .include(IncBook::author())
        .include(IncBook::editor())
        .load_pids(&mut world)
        .await
        .unwrap();
    let book = world.get::<IncBook>(got[0].1).unwrap().clone();
    assert_eq!(book.editor, None);
    assert_eq!(
        world.get::<IncAuthor>(book.author.entity()).unwrap().name,
        "a2"
    );

    // Tanpa include: target tak termuat (perilaku lama tetap).
    let mut store = tpl.fork();
    let mut world = World::new();
    let got = store
        .query::<IncBook>()
        .filter(IncBook::n().eq(3))
        .load_pids(&mut world)
        .await
        .unwrap();
    let book = world.get::<IncBook>(got[0].1).unwrap().clone();
    assert!(world.get::<IncAuthor>(book.author.entity()).is_none());
}
