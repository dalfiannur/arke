//! Uji integrasi **hidrasi selektif** `Query::only` terhadap Postgres nyata:
//! hanya komponen dalam set yang dimuat, dan komponen yang **tak** dimuat tidak
//! terhapus oleh `update_entity`/`save_incremental`. Dilewati bila
//! `DATABASE_URL` tak diset. Komponen `Sel*` unik ke berkas ini (tabel
//! `cmp_sel_*`) agar tak balapan dengan berkas uji lain.

use arke::World;
use arke_postgres::{PgComponent, PgStore};

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct SelHealth {
    hp: i32,
}

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct SelPos {
    x: i32,
}

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct SelTag {
    label: String,
}

async fn seed(url: &str) -> PgStore {
    let mut store = PgStore::connect(url).await.expect("connect");
    store
        .register::<SelHealth>()
        .register::<SelPos>()
        .register::<SelTag>();
    store.migrate().await.unwrap();
    store.save(&World::new()).await.unwrap(); // slate bersih

    let mut world = World::new();
    for i in 0..3 {
        let e = world.spawn();
        world.insert(e, SelHealth { hp: 10 * (i + 1) });
        world.insert(e, SelPos { x: i });
        world.insert(
            e,
            SelTag {
                label: format!("t{i}"),
            },
        );
    }
    store.save(&world).await.unwrap();
    store
}

fn count<T: arke::Component>(w: &World) -> usize {
    w.query::<T>().count()
}

// Satu fungsi uji (bukan beberapa) karena tabel `cmp_sel_*` dibagi: tes dalam
// satu binary berjalan paralel dan `seed` mengosongkan `arke_entities`.
#[tokio::test]
async fn hidrasi_selektif_end_to_end() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };

    // ---- only_memuat_komponen_dalam_set_saja ----
    {
        let mut store = seed(&url).await;

        // Tuple: dua komponen dimuat, yang ketiga tidak.
        let mut w = World::new();
        let n = store
            .query::<SelHealth>()
            .only::<(SelHealth, SelPos)>()
            .load(&mut w)
            .await
            .unwrap();
        assert_eq!(n, 3);
        assert_eq!(count::<SelHealth>(&w), 3);
        assert_eq!(count::<SelPos>(&w), 3);
        assert_eq!(count::<SelTag>(&w), 0, "SelTag di luar set → tak dimuat");

        // Bentuk tunggal (bukan tuple).
        let mut w = World::new();
        store
            .query::<SelHealth>()
            .only::<SelHealth>()
            .load(&mut w)
            .await
            .unwrap();
        assert_eq!(count::<SelHealth>(&w), 3);
        assert_eq!(count::<SelPos>(&w), 0);
        assert_eq!(count::<SelTag>(&w), 0);

        // Tanpa `only` → seluruh komponen (perilaku lama tak berubah).
        let mut w = World::new();
        store.query::<SelHealth>().load(&mut w).await.unwrap();
        assert_eq!(count::<SelTag>(&w), 3);
    }

    // ---- update_entity_setelah_only_tak_menghapus_komponen_lain ----
    {
        let mut store = seed(&url).await;
        let mut w = World::new();
        let pids = store
            .query::<SelHealth>()
            .only::<SelHealth>()
            .load_pids(&mut w)
            .await
            .unwrap();
        let (_, e) = pids[0];
        let ver = store.entity_version(e).await.unwrap().unwrap();
        w.insert(e, SelHealth { hp: 999 }); // insert = upsert di tempat
        // Komponen di luar set yang **disisipkan** pemanggil tetap harus ditulis.
        w.insert(e, SelPos { x: 77 });
        store.update_entity(&w, e, ver).await.unwrap();

        // Muat penuh di World segar: SelPos & SelTag masih ada, SelHealth berubah.
        let mut full = World::new();
        store.load(&mut full).await.unwrap();
        assert_eq!(count::<SelPos>(&full), 3, "SelPos tak boleh terhapus");
        assert_eq!(count::<SelTag>(&full), 3, "SelTag tak boleh terhapus");
        assert!(
            full.query::<SelPos>().any(|p| p.x == 77),
            "SelPos yang disisipkan setelah muat parsial harus tertulis"
        );
        let mut hps: Vec<i32> = full.query::<SelHealth>().map(|h| h.hp).collect();
        hps.sort_unstable();
        assert_eq!(hps, vec![20, 30, 999]);
    }

    // ---- save_incremental_setelah_only_tak_menghapus_komponen_lain ----
    {
        let mut store = seed(&url).await;
        let mut w = World::new();
        let pids = store
            .query::<SelHealth>()
            .only::<SelHealth>()
            .load_pids(&mut w)
            .await
            .unwrap();
        // Ubah dua entity; satu dibiarkan. Pada yang pertama sisipkan juga
        // komponen di luar set — harus ikut tertulis.
        for &(_, e) in &pids[..2] {
            let hp = w.get::<SelHealth>(e).unwrap().hp;
            w.insert(e, SelHealth { hp: hp + 1 });
        }
        w.insert(pids[0].1, SelPos { x: 77 });
        let stats = store.save_incremental(&w).await.unwrap();
        assert_eq!(stats.written, 2);
        assert_eq!(stats.deleted, 0);

        let mut full = World::new();
        store.load(&mut full).await.unwrap();
        assert_eq!(count::<SelPos>(&full), 3, "SelPos tak boleh terhapus");
        assert_eq!(count::<SelTag>(&full), 3, "SelTag tak boleh terhapus");
        assert!(
            full.query::<SelPos>().any(|p| p.x == 77),
            "SelPos yang disisipkan setelah muat parsial harus tertulis"
        );
        let mut hps: Vec<i32> = full.query::<SelHealth>().map(|h| h.hp).collect();
        hps.sort_unstable();
        assert_eq!(hps, vec![11, 21, 30]);
    }

    // ---- muat_penuh_setelah_only_melepas_status_parsial ----
    {
        let mut store = seed(&url).await;
        let mut w = World::new();
        let pids = store
            .query::<SelHealth>()
            .only::<SelHealth>()
            .load_pids(&mut w)
            .await
            .unwrap();
        // Muat penuh ke World yang sama (aditif) → entity kini lengkap.
        store.query::<SelHealth>().load(&mut w).await.unwrap();
        assert_eq!(count::<SelTag>(&w), 3);

        // Lepas SelTag di World lalu tulis: kini penghapusan HARUS tembus ke DB,
        // karena entity sudah dimuat penuh (bukan lagi parsial).
        let (_, e) = pids[0];
        w.remove::<SelTag>(e);
        store.save_incremental(&w).await.unwrap();

        let mut full = World::new();
        store.load(&mut full).await.unwrap();
        assert_eq!(count::<SelTag>(&full), 2);
    }
}
