//! Uji integrasi **paginasi keyset** (`Query::after`/`before`/`load_page`)
//! terhadap Postgres nyata. Dilewati bila `DATABASE_URL` tak diset. Komponen
//! `Ks*` unik ke berkas ini (tabel `cmp_ks_*`) agar tak balapan dengan berkas
//! uji lain. Satu fungsi uji: tabel dibagi & `seed` mengosongkan `arke_entities`.

use arke::World;
use arke_postgres::{Cursor, CursorError, Dir, PageError, PgComponent, PgStore};

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct KsItem {
    score: i32,
    name: String,
}

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct KsExtra {
    note: String,
}

async fn seed(url: &str) -> PgStore {
    let mut store = PgStore::connect(url).await.expect("connect");
    store.register::<KsItem>().register::<KsExtra>();
    store.migrate().await.unwrap();
    store.save(&World::new()).await.unwrap(); // slate bersih

    // Skor berulang agar tiebreak `pid` benar-benar teruji.
    let rows = [
        (5, "e"),
        (3, "c"),
        (5, "a"),
        (1, "g"),
        (3, "b"),
        (5, "d"),
        (2, "f"),
    ];
    let mut world = World::new();
    for (score, name) in rows {
        let e = world.spawn();
        world.insert(
            e,
            KsItem {
                score,
                name: name.to_string(),
            },
        );
        world.insert(
            e,
            KsExtra {
                note: format!("n{score}"),
            },
        );
    }
    store.save(&world).await.unwrap();
    store
}

/// Jalan maju dari awal sampai `next` habis; kembalikan pid per halaman.
async fn walk_forward(
    store: &mut PgStore,
    limit: u64,
    mk: impl Fn(&mut PgStore) -> arke_postgres::Query<'_, KsItem>,
) -> Vec<Vec<i64>> {
    let mut pages = Vec::new();
    let mut cursor: Option<Cursor> = None;
    loop {
        let mut w = World::new();
        let mut q = mk(store).limit(limit);
        if let Some(c) = cursor.take() {
            q = q.after(c);
        }
        let page = q.load_page(&mut w).await.unwrap();
        assert!(page.items.len() as u64 <= limit);
        pages.push(page.items.iter().map(|(pid, _)| *pid).collect());
        match page.next {
            Some(c) => cursor = Some(c),
            None => break,
        }
        assert!(pages.len() < 50, "loop tak berujung");
    }
    pages
}

#[tokio::test]
async fn keyset_end_to_end() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let mut store = seed(&url).await;

    // ---- urutan acuan: load_pids mengikuti ORDER BY query (+ tiebreak pid) ----
    let mut w = World::new();
    let all: Vec<i64> = store
        .query::<KsItem>()
        .order_by(KsItem::score(), Dir::Desc)
        .load_pids(&mut w)
        .await
        .unwrap()
        .into_iter()
        .map(|(pid, _)| pid)
        .collect();
    assert_eq!(all.len(), 7);
    let scores: Vec<i32> = all
        .iter()
        .map(|pid| {
            let (_, e) = store_lookup(&store, &w, *pid);
            w.get::<KsItem>(e).unwrap().score
        })
        .collect();
    assert_eq!(scores, vec![5, 5, 5, 3, 3, 2, 1], "urut skor DESC");
    // Tiebreak pid mengikuti arah kunci terakhir (DESC) → pid menurun di dalam skor sama.
    assert!(
        all[0] > all[1] && all[1] > all[2],
        "tiebreak pid DESC: {all:?}"
    );

    // ---- maju: 3 halaman (3,3,1), gabungan == urutan acuan ----
    let pages = walk_forward(&mut store, 3, |s| {
        s.query::<KsItem>().order_by(KsItem::score(), Dir::Desc)
    })
    .await;
    assert_eq!(
        pages.iter().map(Vec::len).collect::<Vec<_>>(),
        vec![3, 3, 1]
    );
    assert_eq!(pages.concat(), all);

    // ---- mundur dari halaman terakhir: prev → halaman 2 persis → halaman 1 ----
    let mut w = World::new();
    let mut c = None;
    for _ in 0..2 {
        let mut q = store
            .query::<KsItem>()
            .order_by(KsItem::score(), Dir::Desc)
            .limit(3);
        if let Some(cur) = c.take() {
            q = q.after(cur);
        }
        c = q.load_page(&mut w).await.unwrap().next;
    }
    let last = store
        .query::<KsItem>()
        .order_by(KsItem::score(), Dir::Desc)
        .limit(3)
        .after(c.unwrap())
        .load_page(&mut w)
        .await
        .unwrap();
    assert_eq!(last.items.len(), 1);
    assert!(last.next.is_none());
    let prev = last.prev.expect("ada halaman sebelumnya");

    let p2 = store
        .query::<KsItem>()
        .order_by(KsItem::score(), Dir::Desc)
        .limit(3)
        .before(prev)
        .load_page(&mut w)
        .await
        .unwrap();
    assert_eq!(
        p2.items.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
        pages[1],
        "before → halaman 2 persis, urutan maju"
    );
    assert!(p2.next.is_some(), "datang dari belakang → next ada");
    let p1 = store
        .query::<KsItem>()
        .order_by(KsItem::score(), Dir::Desc)
        .limit(3)
        .before(p2.prev.expect("prev halaman 2"))
        .load_page(&mut w)
        .await
        .unwrap();
    assert_eq!(
        p1.items.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
        pages[0]
    );
    assert!(p1.prev.is_none(), "halaman pertama: tak ada prev");

    // ---- kursor stabil sebagai string (round-trip Display/FromStr) ----
    let tok = p2.next.clone().unwrap().to_string();
    let back: Cursor = tok.parse().unwrap();
    assert_eq!(back, p2.next.unwrap());
    assert!(
        tok.chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'),
        "token aman-URL: {tok}"
    );

    // ---- arah campur (score DESC, name ASC): bentuk OR-expanded ----
    let mut w = World::new();
    let all_mixed: Vec<i64> = store
        .query::<KsItem>()
        .order_by(KsItem::score(), Dir::Desc)
        .order_by(KsItem::name(), Dir::Asc)
        .load_pids(&mut w)
        .await
        .unwrap()
        .into_iter()
        .map(|(pid, _)| pid)
        .collect();
    let names: Vec<String> = all_mixed
        .iter()
        .map(|pid| {
            let (_, e) = store_lookup(&store, &w, *pid);
            w.get::<KsItem>(e).unwrap().name.clone()
        })
        .collect();
    assert_eq!(names, vec!["a", "d", "e", "b", "c", "f", "g"]);
    let pages = walk_forward(&mut store, 2, |s| {
        s.query::<KsItem>()
            .order_by(KsItem::score(), Dir::Desc)
            .order_by(KsItem::name(), Dir::Asc)
    })
    .await;
    assert_eq!(pages.concat(), all_mixed);
    assert_eq!(pages.len(), 4);

    // ---- tanpa order_by: keyset atas pid saja; + `only` ----
    let pages = walk_forward(&mut store, 4, |s| s.query::<KsItem>().only::<KsItem>()).await;
    assert_eq!(pages.len(), 2);
    let mut sorted = pages.concat();
    let mut expect = all.clone();
    sorted.sort_unstable();
    expect.sort_unstable();
    assert_eq!(sorted, expect);
    let mut w = World::new();
    store
        .query::<KsItem>()
        .only::<KsItem>()
        .limit(4)
        .load_page(&mut w)
        .await
        .unwrap();
    assert_eq!(
        w.query::<KsExtra>().count(),
        0,
        "only dihormati oleh load_page"
    );

    // ---- `after` juga berlaku untuk load()/load_pids() biasa ----
    let mut w = World::new();
    let first = store
        .query::<KsItem>()
        .order_by(KsItem::score(), Dir::Desc)
        .limit(3)
        .load_page(&mut w)
        .await
        .unwrap();
    let rest = store
        .query::<KsItem>()
        .order_by(KsItem::score(), Dir::Desc)
        .after(first.next.unwrap())
        .load_pids(&mut w)
        .await
        .unwrap();
    assert_eq!(
        rest.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
        all[3..].to_vec()
    );

    // ---- galat: kursor rusak, kursor tak cocok kunci, tanpa limit, offset+kursor ----
    assert!(matches!(
        Cursor::decode("bukan-kursor"),
        Err(CursorError::Malformed)
    ));
    let mut w = World::new();
    let by_score = store
        .query::<KsItem>()
        .order_by(KsItem::score(), Dir::Desc)
        .limit(2)
        .load_page(&mut w)
        .await
        .unwrap()
        .next
        .unwrap();
    let err = store
        .query::<KsItem>()
        .order_by(KsItem::name(), Dir::Asc)
        .limit(2)
        .after(by_score.clone())
        .load_page(&mut w)
        .await
        .unwrap_err();
    assert!(
        matches!(err, PageError::Cursor(CursorError::Mismatch(_))),
        "{err:?}"
    );
    let err = store
        .query::<KsItem>()
        .order_by(KsItem::score(), Dir::Desc)
        .load_page(&mut w)
        .await
        .unwrap_err();
    assert!(matches!(err, PageError::MissingLimit), "{err:?}");
    let err = store
        .query::<KsItem>()
        .order_by(KsItem::score(), Dir::Desc)
        .limit(2)
        .offset(1)
        .after(by_score)
        .load_page(&mut w)
        .await
        .unwrap_err();
    assert!(matches!(err, PageError::OffsetWithCursor), "{err:?}");
}

/// `(pid, Entity)` untuk `pid` lewat jembatan store (entity sudah termuat di `w`).
fn store_lookup(store: &PgStore, _w: &World, pid: i64) -> (i64, arke::Entity) {
    (pid, store.entity_of(pid).expect("pid termuat"))
}
