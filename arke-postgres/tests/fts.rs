//! Uji integrasi **full-text search** (`#[pg(fts)]`, `Field::search`,
//! `Query::order_by_rank`) terhadap Postgres nyata. Dilewati bila
//! `DATABASE_URL` tak diset. Komponen `Fts*` unik ke berkas ini. Satu fungsi
//! uji: tabel dibagi & `seed` mengosongkan `arke_entities`.

use arke::World;
use arke_postgres::{Dir, PgComponent, PgStore};

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct FtsDoc {
    /// Stemming Inggris: "running" ≈ "run", "shoes" ≈ "shoe".
    #[pg(fts = "english")]
    title: String,
    /// Default `simple`: tanpa stemming, hanya tokenisasi + lowercase.
    #[pg(fts)]
    body: String,
    /// Tanpa `#[pg(fts)]`: `search` tetap boleh (tanpa indeks).
    note: Option<String>,
    score: i32,
}

/// Komponen kedua dengan nama tabel sama seperti `FtsDoc` tapi config lain —
/// untuk menguji indeks dibuat ulang saat config berubah (tabel dibagi).
#[derive(PgComponent, PartialEq, Debug, Clone)]
#[pg(table = "cmp_ftsdoc")]
struct FtsDocV2 {
    #[pg(fts = "simple")]
    title: String,
    #[pg(fts)]
    body: String,
    note: Option<String>,
    score: i32,
}

async fn index_defs(pool: &sqlx::PgPool) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT indexname, indexdef FROM pg_indexes \
         WHERE schemaname = current_schema() AND tablename = 'cmp_ftsdoc' \
         AND indexname LIKE 'idx\\_%\\_fts' ORDER BY indexname",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn fts_end_to_end() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    let mut store = PgStore::connect(&url).await.expect("connect");
    store.register::<FtsDoc>();
    store.migrate().await.unwrap();
    store.save(&World::new()).await.unwrap(); // slate bersih

    // ---- migrate: indeks GIN ekspresi per kolom fts, config sesuai atribut ----
    let defs = index_defs(&pool).await;
    assert_eq!(
        defs.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        vec!["idx_cmp_ftsdoc_body_fts", "idx_cmp_ftsdoc_title_fts"]
    );
    assert!(
        defs[0].1.contains("USING gin") && defs[0].1.contains("'simple'::regconfig"),
        "{}",
        defs[0].1
    );
    assert!(
        defs[1].1.contains("USING gin") && defs[1].1.contains("'english'::regconfig"),
        "{}",
        defs[1].1
    );
    // Idempoten: migrate ulang tak mengubah definisi.
    store.migrate().await.unwrap();
    assert_eq!(index_defs(&pool).await, defs);

    // ---- seed ----
    let docs = [
        ("Running shoes for trail", "light and fast", 3),
        ("Best shoe care guide", "polish leather Shoes weekly", 1),
        ("Marathon training plan", "run long, run slow", 2),
        ("Kitchen knives", "sharpening steel", 9),
        (
            "Trail running: shoes and socks",
            "running running running shoes",
            5,
        ),
    ];
    let mut world = World::new();
    for (title, body, score) in docs {
        let e = world.spawn();
        world.insert(
            e,
            FtsDoc {
                title: title.to_string(),
                body: body.to_string(),
                note: None,
                score,
            },
        );
    }
    store.save(&world).await.unwrap();

    let titles = |w: &World, pids: &[(i64, arke::Entity)]| -> Vec<String> {
        pids.iter()
            .map(|(_, e)| w.get::<FtsDoc>(*e).unwrap().title.clone())
            .collect()
    };

    // ---- search dengan stemming english: "running shoe" → run & shoe ----
    let mut w = World::new();
    let got = store
        .query::<FtsDoc>()
        .filter(FtsDoc::title().search("running shoe"))
        .order_by(FtsDoc::score(), Dir::Asc)
        .load_pids(&mut w)
        .await
        .unwrap();
    assert_eq!(
        titles(&w, &got),
        vec!["Running shoes for trail", "Trail running: shoes and socks"]
    );

    // ---- websearch: `-trail` mengecualikan; `"shoe care"` frasa ----
    let mut w = World::new();
    let got = store
        .query::<FtsDoc>()
        .filter(FtsDoc::title().search("shoe -trail"))
        .load_pids(&mut w)
        .await
        .unwrap();
    assert_eq!(titles(&w, &got), vec!["Best shoe care guide"]);

    // ---- default simple: case-insensitive tapi tanpa stemming ----
    let mut w = World::new();
    let got = store
        .query::<FtsDoc>()
        .filter(FtsDoc::body().search("shoes"))
        .order_by(FtsDoc::score(), Dir::Asc)
        .load_pids(&mut w)
        .await
        .unwrap();
    assert_eq!(
        titles(&w, &got),
        vec!["Best shoe care guide", "Trail running: shoes and socks"],
        "simple: `Shoes`≈`shoes`, tapi `shoe` ≠ `shoes`"
    );
    let n = store
        .query::<FtsDoc>()
        .filter(FtsDoc::body().search("shoe"))
        .count()
        .await
        .unwrap();
    assert_eq!(n, 0, "simple tak men-stem `shoes` → `shoe`");

    // ---- field tanpa #[pg(fts)] (Option<String>) tetap bisa search ----
    let n = store
        .query::<FtsDoc>()
        .filter(FtsDoc::note().search("apa saja"))
        .count()
        .await
        .unwrap();
    assert_eq!(n, 0);

    // ---- order_by_rank: dokumen dengan lebih banyak kecocokan di atas ----
    let mut w = World::new();
    let got = store
        .query::<FtsDoc>()
        .filter(FtsDoc::body().search("running"))
        .order_by_rank(FtsDoc::body(), "running")
        .load_pids(&mut w)
        .await
        .unwrap();
    assert_eq!(titles(&w, &got), vec!["Trail running: shoes and socks"]);
    let mut w = World::new();
    let got = store
        .query::<FtsDoc>()
        .filter(FtsDoc::title().search("running OR shoe OR trail"))
        .order_by_rank(FtsDoc::title(), "running OR shoe OR trail")
        .load_pids(&mut w)
        .await
        .unwrap();
    assert_eq!(got.len(), 3);
    assert_eq!(
        titles(&w, &got)[0],
        "Trail running: shoes and socks",
        "tiga istilah cocok → rank tertinggi"
    );

    // ---- keyset di atas rank: halaman 1 (limit 2) + halaman 2, gabungan == urutan penuh ----
    let mut w = World::new();
    let p1 = store
        .query::<FtsDoc>()
        .filter(FtsDoc::title().search("running OR shoe OR trail"))
        .order_by_rank(FtsDoc::title(), "running OR shoe OR trail")
        .limit(2)
        .load_page(&mut w)
        .await
        .unwrap();
    assert_eq!(p1.items.len(), 2);
    let p2 = store
        .query::<FtsDoc>()
        .filter(FtsDoc::title().search("running OR shoe OR trail"))
        .order_by_rank(FtsDoc::title(), "running OR shoe OR trail")
        .limit(2)
        .after(p1.next.clone().expect("ada halaman 2"))
        .load_page(&mut w)
        .await
        .unwrap();
    assert_eq!(p2.items.len(), 1);
    assert!(p2.next.is_none());
    let mut joined = p1.items.clone();
    joined.extend(p2.items.iter().copied());
    assert_eq!(
        joined.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
        got.iter().map(|(p, _)| *p).collect::<Vec<_>>()
    );
    // Mundur dari halaman 2 → halaman 1 persis.
    let back = store
        .query::<FtsDoc>()
        .filter(FtsDoc::title().search("running OR shoe OR trail"))
        .order_by_rank(FtsDoc::title(), "running OR shoe OR trail")
        .limit(2)
        .before(p2.prev.expect("prev halaman 2"))
        .load_page(&mut w)
        .await
        .unwrap();
    assert_eq!(back.items, p1.items);

    // ---- config berubah (english → simple pada `title`) → indeks dibuat ulang ----
    let mut store2 = PgStore::connect(&url).await.expect("connect");
    store2.register::<FtsDocV2>();
    store2.migrate().await.unwrap();
    let defs = index_defs(&pool).await;
    assert!(
        defs[1].1.contains("'simple'::regconfig"),
        "config berubah harus dibuat ulang: {}",
        defs[1].1
    );
}
