//! Regresi dari audit 0.16: jembatan pid↔entity ber-generation, muat aditif
//! (`Query::load` berkali-kali ke satu World), jembatan tak berubah saat commit
//! gagal, `fetch` mengisi jembatan, namespace cache ber-fingerprint skema, dan
//! `Ref` membawa generation. Dilewati bila `DATABASE_URL` tak diset. Nama
//! komponen unik ke berkas ini (`Audit*`), kecuali `Thing` yang sengaja
//! didefinisikan dua kali (v1/v2) untuk mensimulasikan evolusi skema.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use arke::{Entity, QueryData, World};
use arke_postgres::{ComponentCache, PgComponent, PgStore};
use async_trait::async_trait;

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct AuditHp {
    hp: i64,
}

#[derive(PgComponent, PartialEq, Debug, Clone)]
#[pg(check = "score >= 0")]
struct AuditScore {
    score: i64,
}

#[derive(PgComponent, PartialEq, Debug)]
struct AuditFollower {
    target: Entity,
}

/// Satu test sekuensial (konvensi berkas ini): sub-skenario berbagi
/// `arke_entities` dan masing-masing mulai dari tabel kosong.
#[tokio::test]
async fn regresi_audit_sekuensial() {
    if std::env::var("DATABASE_URL").is_err() {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    }
    query_load_aditif_tak_menggandakan_saat_save_incremental().await;
    slot_terdaur_ulang_tak_mewarisi_pid_lama().await;
    commit_gagal_tak_mengubah_jembatan().await;
    fetch_mengisi_jembatan().await;
    relasi_me_resolve_pada_world_ber_generation().await;
    cache_tak_menyajikan_baris_skema_lama().await;
}

async fn connect() -> Option<PgStore> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let mut store = PgStore::connect(&url).await.expect("connect Postgres");
    store
        .register::<AuditHp>()
        .register::<AuditScore>()
        .register::<AuditFollower>();
    store.migrate().await.unwrap();
    // Bersihkan seluruh tabel entity (cascade) agar tiap tes mulai dari nol.
    store.save(&World::new()).await.unwrap();
    Some(store)
}

fn count<T: arke::Component>(w: &mut World) -> usize {
    let mut n = 0;
    <&T>::each(w, |_| n += 1);
    n
}

/// #9: dua `Query::load` ke World yang sama tak boleh menggandakan entity di
/// DB saat `save_incremental`; entity dari muat pertama tetap tertaut ke pid-nya.
async fn query_load_aditif_tak_menggandakan_saat_save_incremental() {
    let Some(mut store) = connect().await else {
        return;
    };
    let mut seed = World::new();
    for hp in [1, 2, 3] {
        let e = seed.spawn();
        seed.insert(e, AuditHp { hp });
    }
    store.save(&seed).await.unwrap();

    let mut store = store.fork();
    let mut w = World::new();
    let first = store
        .query::<AuditHp>()
        .filter(AuditHp::hp().eq(1))
        .load_pids(&mut w)
        .await
        .unwrap();
    let second = store
        .query::<AuditHp>()
        .filter(AuditHp::hp().eq(2))
        .load_pids(&mut w)
        .await
        .unwrap();
    let (pid1, e1) = first[0];
    let (_, e2) = second[0];
    assert_eq!(
        store.pid_of(e1),
        Some(pid1),
        "muat kedua tak boleh melupakan e1"
    );
    assert_eq!(store.entity_of(pid1), Some(e1));

    // Ubah keduanya → UPDATE, bukan INSERT baru.
    w.insert(e1, AuditHp { hp: 10 });
    w.insert(e2, AuditHp { hp: 20 });
    let stats = store.save_incremental(&w).await.unwrap();
    assert_eq!((stats.written, stats.deleted), (2, 0));
    assert_eq!(store.query::<AuditHp>().count().await.unwrap(), 3);

    // Muat ulang pid yang sama ke World yang sama → refresh di tempat, bukan duplikat.
    let again = store
        .query::<AuditHp>()
        .filter(AuditHp::hp().eq(10))
        .load_pids(&mut w)
        .await
        .unwrap();
    assert_eq!(again, vec![(pid1, e1)]);
    assert_eq!(count::<AuditHp>(&mut w), 2);
}

/// #8: slot World terdaur-ulang (despawn + spawn di indeks sama) adalah entity
/// **baru** — pid lama dihapus, pid baru dicetak; bukan mewarisi pid lama.
async fn slot_terdaur_ulang_tak_mewarisi_pid_lama() {
    let Some(mut store) = connect().await else {
        return;
    };
    let mut w = World::new();
    let a = w.spawn();
    w.insert(a, AuditHp { hp: 1 });
    store.save_incremental(&w).await.unwrap();
    let pid_a = store.pid_of(a).unwrap();

    w.despawn(a);
    let b = w.spawn(); // indeks sama, generation naik
    assert_eq!(a.index(), b.index());
    w.insert(b, AuditHp { hp: 2 });
    let stats = store.save_incremental(&w).await.unwrap();
    assert_eq!((stats.written, stats.deleted), (1, 1));

    let pid_b = store.pid_of(b).expect("b tertaut ke pid");
    assert_ne!(pid_a, pid_b, "b harus pid baru");
    assert_eq!(store.pid_of(a), None, "handle basi tak tertaut");
    assert_eq!(store.query::<AuditHp>().count().await.unwrap(), 1);
}

/// #7: commit gagal (CHECK) → jembatan pid↔entity dan rekam sinkron **tak
/// berubah**; retry setelah data diperbaiki menulis dengan benar.
async fn commit_gagal_tak_mengubah_jembatan() {
    let Some(mut store) = connect().await else {
        return;
    };
    let mut w = World::new();
    let ok = w.spawn();
    w.insert(ok, AuditScore { score: 1 });
    store.save_incremental(&w).await.unwrap();
    let pid_ok = store.pid_of(ok).unwrap();

    let bad = w.spawn();
    w.insert(bad, AuditScore { score: -1 }); // melanggar CHECK
    w.insert(ok, AuditScore { score: 5 });
    assert!(store.save_incremental(&w).await.is_err());
    assert_eq!(
        store.pid_of(bad),
        None,
        "pid yang di-rollback tak boleh tersisa"
    );
    assert_eq!(store.pid_of(ok), Some(pid_ok));

    w.insert(bad, AuditScore { score: 0 });
    let stats = store.save_incremental(&w).await.unwrap();
    assert_eq!((stats.written, stats.deleted), (2, 0));
    assert_eq!(store.query::<AuditScore>().count().await.unwrap(), 2);
    let mut w2 = World::new();
    store.load(&mut w2).await.unwrap();
    let mut scores: Vec<i64> = Vec::new();
    <&AuditScore>::each(&mut w2, |s| scores.push(s.score));
    scores.sort();
    assert_eq!(scores, vec![0, 5]);
}

/// #9b: `fetch` mengisi jembatan seperti jalur muat lain, sehingga
/// `entity_version`/`update_entity` bekerja sesudahnya.
async fn fetch_mengisi_jembatan() {
    let Some(mut store) = connect().await else {
        return;
    };
    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, AuditHp { hp: 9 });
    store.save_incremental(&w).await.unwrap();
    let pid = store.pid_of(e).unwrap();

    let mut store = store.fork();
    let mut w2 = World::new();
    let f = store.fetch(&mut w2, pid).await.unwrap().expect("ada");
    assert_eq!(store.pid_of(f), Some(pid));
    assert_eq!(store.entity_of(pid), Some(f));
    assert_eq!(store.entity_version(f).await.unwrap(), Some(0));
    w2.insert(f, AuditHp { hp: 10 });
    assert_eq!(store.update_entity(&w2, f, 0).await.unwrap(), 1);
}

/// `Ref` membawa generation: relasi ke entity di slot ber-generation > 0 tetap
/// me-resolve setelah muat ke World yang slotnya sudah pernah didaur-ulang.
async fn relasi_me_resolve_pada_world_ber_generation() {
    let Some(mut store) = connect().await else {
        return;
    };
    let mut seed = World::new();
    let t = seed.spawn();
    seed.insert(t, AuditHp { hp: 42 });
    let f = seed.spawn();
    seed.insert(f, AuditFollower { target: t });
    store.save(&seed).await.unwrap();

    // World tujuan: slot 0 sudah pernah dipakai → entity termuat pertama ber-gen 1.
    let mut w = World::new();
    let scratch = w.spawn();
    w.despawn(scratch);
    let mut store = store.fork();
    store.load(&mut w).await.unwrap();
    let mut targets = Vec::new();
    <&AuditFollower>::each(&mut w, |fl| targets.push(fl.target));
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].generation(), 1);
    assert_eq!(w.get::<AuditHp>(targets[0]), Some(&AuditHp { hp: 42 }));
}

// ── #6: cache + evolusi skema ──────────────────────────────────────────────

#[derive(Default)]
struct MemCache {
    data: Mutex<HashMap<(String, i64), Vec<u8>>>,
}

#[async_trait]
impl ComponentCache for MemCache {
    async fn get_many(&self, ns: &str, ids: &[i64]) -> Vec<Option<Vec<u8>>> {
        let d = self.data.lock().unwrap();
        ids.iter()
            .map(|id| d.get(&(ns.to_string(), *id)).cloned())
            .collect()
    }
    async fn put_many(&self, ns: &str, entries: &[(i64, Vec<u8>)]) {
        let mut d = self.data.lock().unwrap();
        for (id, bytes) in entries {
            d.insert((ns.to_string(), *id), bytes.clone());
        }
    }
    async fn invalidate(&self, ns: &str, ids: &[i64]) {
        let mut d = self.data.lock().unwrap();
        for id in ids {
            d.remove(&(ns.to_string(), *id));
        }
    }
    async fn clear(&self) {
        self.data.lock().unwrap().clear();
    }
}

mod v1 {
    use arke_postgres::PgComponent;
    #[derive(PgComponent, PartialEq, Debug)]
    pub struct AuditThing {
        pub a: i64,
    }
}
mod v2 {
    use arke_postgres::PgComponent;
    #[derive(PgComponent, PartialEq, Debug)]
    pub struct AuditThing {
        pub a: i64,
        pub b: i64,
    }
}

/// Baris cache dari skema lama (v1: 1 kolom) tak boleh disajikan ke skema baru
/// (v2: 2 kolom) — sebelumnya `from_params` gagal diam-diam dan komponen hilang.
async fn cache_tak_menyajikan_baris_skema_lama() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        return;
    };
    let cache: Arc<MemCache> = Arc::new(MemCache::default());
    // Deploy v1: tulis, lalu muat (mengisi cache).
    let mut s1 = PgStore::connect(&url)
        .await
        .unwrap()
        .with_cache(cache.clone());
    s1.register::<v1::AuditThing>();
    // Tabel `cmp_auditthing` bisa tersisa dari run sebelumnya dengan kolom `b`;
    // buang agar skema v1 benar-benar 1 kolom.
    sqlx::query("DROP TABLE IF EXISTS cmp_auditthing")
        .execute(&sqlx::PgPool::connect(&url).await.unwrap())
        .await
        .unwrap();
    s1.migrate().await.unwrap();
    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, v1::AuditThing { a: 7 });
    s1.save(&w).await.unwrap();
    let pid = s1.pid_of(e).unwrap();
    let mut w1 = World::new();
    s1.load_ids(&mut w1, &[pid]).await.unwrap();
    assert!(
        !cache.data.lock().unwrap().is_empty(),
        "cache terisi oleh muat v1"
    );

    // Deploy v2 dengan cache yang sama: kolom `b` ditambah (backfill 0).
    let mut s2 = PgStore::connect(&url).await.unwrap().with_cache(cache);
    s2.register::<v2::AuditThing>();
    s2.migrate().await.unwrap();
    let mut w2 = World::new();
    let got = s2.load_ids(&mut w2, &[pid]).await.unwrap();
    assert_eq!(
        w2.get::<v2::AuditThing>(got[0]),
        Some(&v2::AuditThing { a: 7, b: 0 }),
        "komponen harus termuat dari Postgres, bukan hilang karena baris cache v1"
    );
}
