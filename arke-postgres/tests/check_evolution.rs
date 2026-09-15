//! `#[pg(check)]` ber-nama-stabil (content-addressed): mengubah/menghapus
//! ekspresi CHECK antar-deploy direkonsiliasi `migrate` (yang lama di-drop,
//! yang baru dipasang), dan `migrate` berulang dengan definisi sama tak
//! menyentuh constraint. Dilewati bila `DATABASE_URL` tak diset.

use arke::World;
use arke_postgres::{PgComponent, PgStore};
use sqlx::PgPool;

mod v1 {
    use arke_postgres::PgComponent;
    #[derive(PgComponent, PartialEq, Debug)]
    #[pg(table = "audit_checked")]
    #[pg(check = "hp >= 0")]
    #[pg(check = "mp >= 0")]
    pub struct Checked {
        pub hp: i64,
        pub mp: i64,
    }
}
mod v2 {
    use arke_postgres::PgComponent;
    /// `hp >= 0` diperketat jadi `hp >= 1`; `mp >= 0` dihapus.
    #[derive(PgComponent, PartialEq, Debug)]
    #[pg(table = "audit_checked")]
    #[pg(check = "hp >= 1")]
    pub struct Checked {
        pub hp: i64,
        pub mp: i64,
    }
}

/// Nama constraint `chk_%` pada tabel, terurut.
async fn check_names(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT conname FROM pg_constraint \
         WHERE conrelid = 'audit_checked'::regclass AND contype = 'c' ORDER BY conname",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn check_direkonsiliasi_saat_ekspresi_berubah() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let pool = PgPool::connect(&url).await.unwrap();
    sqlx::query("DROP TABLE IF EXISTS audit_checked")
        .execute(&pool)
        .await
        .unwrap();

    // Deploy v1: dua CHECK; migrate dua kali → nama stabil, tak berubah.
    let mut s1 = PgStore::connect(&url).await.unwrap();
    s1.register::<v1::Checked>();
    s1.migrate().await.unwrap();
    let names_a = check_names(&pool).await;
    assert_eq!(names_a.len(), 2, "{names_a:?}");
    s1.migrate().await.unwrap();
    assert_eq!(
        check_names(&pool).await,
        names_a,
        "migrate ulang tak boleh mengubah nama"
    );
    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, v1::Checked { hp: 5, mp: 0 });
    s1.save(&w).await.unwrap();

    // Deploy v2: `hp >= 0` → `hp >= 1`, `mp >= 0` dihapus.
    let mut s2 = PgStore::connect(&url).await.unwrap();
    s2.register::<v2::Checked>();
    s2.migrate().await.unwrap();
    let names_b = check_names(&pool).await;
    assert_eq!(names_b.len(), 1, "{names_b:?}");
    assert!(
        !names_a.contains(&names_b[0]),
        "ekspresi berubah → nama baru"
    );

    // Ditegakkan: hp=0 kini ditolak, mp negatif kini boleh.
    let mut w2 = World::new();
    let e2 = w2.spawn();
    w2.insert(e2, v2::Checked { hp: 0, mp: -1 });
    assert!(
        s2.save_incremental(&w2).await.is_err(),
        "hp >= 1 harus ditegakkan"
    );
    w2.insert(e2, v2::Checked { hp: 1, mp: -1 });
    s2.save_incremental(&w2).await.unwrap();

    // Data yang melanggar CHECK baru membuat `migrate` gagal **keras** (bukan
    // diam): kembali ke v1 memasang lagi `mp >= 0`, tapi ada baris `mp = -1`.
    let mut s3 = PgStore::connect(&url).await.unwrap();
    s3.register::<v1::Checked>();
    assert!(s3.migrate().await.is_err());
}
