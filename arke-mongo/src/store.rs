//! [`MongoStore`]: persistensi async `World` ↔ MongoDB (RFC-0035 §5).
//!
//! Satu-satunya modul yang menyentuh I/O; seluruh logika pemetaan hidup di
//! `bson_map` dan `registry` sebagai fungsi murni.

use std::collections::HashMap;

use arke::{Entity, World};
use mongodb::bson::{Document, doc};
use mongodb::{Client, Collection, Database};

use crate::registry::Registry;
use crate::{MongoComponent, MongoError, Pid};

/// Versi format dokumen (STD-0001), disimpan di koleksi `arke_meta`.
const SCHEMA_VERSION: i64 = 1;

/// Nama koleksi utama.
const ENTITIES: &str = "arke_entities";

/// Nama koleksi metadata skema.
const META: &str = "arke_meta";

/// Penyimpan MongoDB untuk keadaan ECS (RFC-0035).
///
/// Daftarkan tiap tipe komponen via [`Self::register`], panggil
/// [`Self::ensure_indexes`], lalu pakai jalur per-operasi
/// (`create`/`fetch`/`update`/`remove`) atau jalur seluruh World
/// (`save`/`load`).
pub struct MongoStore {
    db: Database,
    entities: Collection<Document>,
    reg: Registry,
    /// Jembatan Entity (indeks ephemeral) → `pid` persisten (RFC-0034 §2).
    pid_of: HashMap<Entity, Pid>,
    /// Jembatan `pid` → Entity (handle lokal working-set).
    entity_of: HashMap<Pid, Entity>,
}

impl MongoStore {
    /// Menyambung ke MongoDB pada `uri`, memakai database `db_name`.
    ///
    /// Memverifikasi keterjangkauan server (`ping`) sebelum mengembalikan —
    /// `Client::with_uri_str` sendiri malas (tak pernah membuka koneksi), jadi
    /// tanpa `ping` ini, `uri` yang menunjuk host mati akan lolos sebagai
    /// `Ok` dan baru gagal ~30 detik kemudian di operasi pertama, sebagai
    /// dump server-selection-timeout yang buram. Sejajar dengan
    /// `PgStore::connect`, yang gagal cepat lewat pool `connect()`-nya
    /// (RFC-0035 §5).
    pub async fn connect(uri: &str, db_name: &str) -> Result<Self, MongoError> {
        let client = Client::with_uri_str(uri).await?;
        let db = client.database(db_name);
        db.run_command(doc! { "ping": 1 }).await?;
        let entities = db.collection::<Document>(ENTITIES);
        Ok(Self {
            db,
            entities,
            reg: Registry::new(),
            pid_of: HashMap::new(),
            entity_of: HashMap::new(),
        })
    }

    /// Mendaftarkan tipe komponen `T` untuk dipersist.
    ///
    /// # Panics
    ///
    /// Panic bila `T::NAME` sudah dipakai komponen lain, atau bukan nama field
    /// BSON yang sah (RFC-0035 Am. 1 & Am. 2).
    pub fn register<T: MongoComponent>(&mut self) -> &mut Self {
        self.reg.push::<T>();
        self
    }

    /// Membuat indeks untuk seluruh komponen terdaftar dan mencatat
    /// `schema_version` (STD-0001).
    ///
    /// Idempoten **untuk definisi yang tak berubah** — memanggilnya berkali-
    /// kali dengan `IndexDef` yang sama aman di tiap start-up. Bila sebuah
    /// `IndexDef` berubah antar-deploy (mis. field sama, `unique` dibalik)
    /// sementara indeks lama atas nama yang sama masih ada, driver
    /// mengembalikan error MongoDB `IndexKeySpecsConflict` (kode 86) — ini
    /// **bukan** direkonsiliasi otomatis. Operator harus menjatuhkan indeks
    /// usang secara manual (mis. lewat `mongosh`) sebelum deploy berikutnya
    /// bisa jalan. Bandingkan dengan `PgStore::migrate`, yang memang
    /// merekonsiliasi skema kolom secara otomatis — `ensure_indexes` tidak.
    pub async fn ensure_indexes(&self) -> Result<(), MongoError> {
        let models = self.reg.index_models();
        if !models.is_empty() {
            self.entities.create_indexes(models).await?;
        }
        self.db
            .collection::<Document>(META)
            .update_one(
                doc! { "_id": "schema" },
                doc! { "$set": { "schema_version": SCHEMA_VERSION } },
            )
            .upsert(true)
            .await?;
        Ok(())
    }

    /// Menyimpan `entity` sebagai dokumen baru dan mengembalikan `pid`-nya.
    ///
    /// `pid` dialokasikan di sisi klien, jadi operasi ini cukup satu
    /// round-trip dan aman untuk multi-replica (RFC-0035 §3).
    pub async fn create(&mut self, world: &World, entity: Entity) -> Result<Pid, MongoError> {
        let cmp = self.reg.cmp_doc(world, entity)?;
        let pid = Pid::new();
        self.entities
            .insert_one(doc! { "_id": pid.0, "version": 0i64, "cmp": cmp })
            .await?;
        self.pid_of.insert(entity, pid);
        self.entity_of.insert(pid, entity);
        Ok(pid)
    }

    /// Memuat dokumen `pid` ke `world` sebagai entity baru; `None` bila dokumen
    /// tak ada.
    ///
    /// Bila sebuah komponen gagal di-decode, entity yang telanjur di-spawn
    /// dibuang lagi supaya `world` tak meninggalkan entity separuh terisi.
    pub async fn fetch(
        &mut self,
        world: &mut World,
        pid: Pid,
    ) -> Result<Option<Entity>, MongoError> {
        let Some(document) = self.entities.find_one(doc! { "_id": pid.0 }).await? else {
            return Ok(None);
        };
        let entity = world.spawn();
        if let Ok(cmp) = document.get_document("cmp")
            && let Err(e) = self.reg.apply(world, entity, pid, cmp)
        {
            world.despawn(entity);
            return Err(e);
        }
        self.pid_of.insert(entity, pid);
        self.entity_of.insert(pid, entity);
        Ok(Some(entity))
    }
}
