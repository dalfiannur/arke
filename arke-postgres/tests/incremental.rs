//! Uji `PgStore::save_incremental` — hanya entity berubah yang ditulis.
//! Dilewati bila `DATABASE_URL` tak diset.

use arke::{Entity, QueryData, World};
use arke_postgres::{PgComponent, PgStore, SyncStats};

#[derive(PgComponent, PartialEq, Debug)]
struct Pos {
    x: i32,
}

fn entity_count(world: &mut World) -> usize {
    let mut n = 0;
    <Entity>::each(world, |_| n += 1);
    n
}

#[tokio::test]
async fn save_incremental_hanya_menulis_yang_berubah() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let mut store = PgStore::connect(&url).await.expect("connect");
    store.register::<Pos>();
    store.migrate().await.unwrap();
    // Mulai bersih agar rekam internal & DB sinkron.
    store.save(&World::new()).await.unwrap();

    // Sinkron awal: 3 entity → semua ditulis.
    let mut world = World::new();
    let mut ids = Vec::new();
    for i in 0..3 {
        let e = world.spawn();
        world.insert(e, Pos { x: i });
        ids.push(e);
    }
    // Rekam internal masih kosong (baru save() World kosong) → 3 baru.
    assert_eq!(
        store.save_incremental(&world).await.unwrap(),
        SyncStats {
            written: 3,
            deleted: 0
        }
    );

    // Tak ada perubahan → tak menulis apa pun.
    assert_eq!(
        store.save_incremental(&world).await.unwrap(),
        SyncStats {
            written: 0,
            deleted: 0
        }
    );

    // Ubah SATU entity (yang x==1, yakni ids[1]) → hanya 1 ditulis.
    for p in world.query_mut::<Pos>() {
        if p.x == 1 {
            p.x = 99;
        }
    }
    assert_eq!(
        store.save_incremental(&world).await.unwrap(),
        SyncStats {
            written: 1,
            deleted: 0
        }
    );
    // Versi entity yang ditulis naik jadi 1; yang lain tetap 0.
    assert_eq!(store.entity_version(ids[1]).await.unwrap(), Some(1));
    assert_eq!(store.entity_version(ids[0]).await.unwrap(), Some(0));

    // Despawn satu → 1 dihapus, 0 ditulis.
    world.despawn(ids[2]);
    assert_eq!(
        store.save_incremental(&world).await.unwrap(),
        SyncStats {
            written: 0,
            deleted: 1
        }
    );

    // Muat: 2 entity tersisa; mutasi (x=1→99) bertahan, yang di-despawn (x=2)
    // hilang. RFC-0034: handle sisi-simpan tak lestari → verifikasi lewat isi.
    let mut loaded = World::new();
    store.load(&mut loaded).await.unwrap();
    assert_eq!(entity_count(&mut loaded), 2);
    let mut xs: Vec<i32> = loaded.query::<Pos>().map(|p| p.x).collect();
    xs.sort();
    assert_eq!(xs, vec![0, 99]);
}
