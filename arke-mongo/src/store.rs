//! [`MongoStore`]: persistensi async `World` ↔ MongoDB (RFC-0035 §5).
//!
//! Satu-satunya modul yang menyentuh I/O; seluruh logika pemetaan hidup di
//! `bson_map` dan `registry` sebagai fungsi murni.

use std::collections::{HashMap, HashSet};

use arke::{Entity, QueryData, World};
use mongodb::bson::{Document, doc};
use mongodb::options::ReturnDocument;
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
        self.bind(entity, pid);
        Ok(pid)
    }

    /// Memuat dokumen `pid` ke `world` sebagai entity baru; `None` bila dokumen
    /// tak ada.
    ///
    /// Bila sebuah komponen gagal di-decode, entity yang telanjur di-spawn
    /// dibuang lagi supaya `world` tak meninggalkan entity separuh terisi.
    /// Field `cmp` yang **absen** berarti entity legit tanpa komponen
    /// terdaftar (lanjut, `Ok`); `cmp` yang **ada tapi bukan sub-dokumen**
    /// berarti dokumen korup — bukan sesuatu yang boleh diperlakukan sebagai
    /// "entity tanpa komponen" secara diam-diam (RFC-0035 §6).
    pub async fn fetch(
        &mut self,
        world: &mut World,
        pid: Pid,
    ) -> Result<Option<Entity>, MongoError> {
        let Some(document) = self.entities.find_one(doc! { "_id": pid.0 }).await? else {
            return Ok(None);
        };
        let entity = world.spawn();
        match document.get("cmp") {
            None => {}
            Some(mongodb::bson::Bson::Document(cmp)) => {
                if let Err(e) = self.reg.apply(world, entity, pid, cmp) {
                    world.despawn(entity);
                    return Err(e);
                }
            }
            Some(_) => {
                world.despawn(entity);
                return Err(MongoError::Decode {
                    pid,
                    component: "cmp",
                });
            }
        }
        self.bind(entity, pid);
        Ok(Some(entity))
    }

    /// Tautan `entity` ↔ `pid` yang tercatat di store, bila ada.
    ///
    /// Aksesor sempit untuk Task 14 (`version_of`/`update_checked`), yang
    /// butuh menerjemahkan `Entity` lokal ke `pid` persisten untuk operasi
    /// per-op berikutnya.
    pub fn pid_of(&self, entity: Entity) -> Option<Pid> {
        self.pid_of.get(&entity).copied()
    }

    /// Tautan `pid` ↔ `entity` yang tercatat di store, bila ada.
    ///
    /// Sisi cermin dari [`Self::pid_of`]. `pid_of` sendiri tak cukup untuk
    /// menguji bijeksi `pid_of`/`entity_of` dari luar crate: kebocoran
    /// `bind` yang diperbaiki di sini (create dua kali untuk `entity` yang
    /// sama) hanya kelihatan sebagai tautan basi di sisi `entity_of`, tak
    /// pernah di `pid_of`. Karena itu diekspos di sini juga, bukan cuma
    /// untuk kenyamanan.
    pub fn entity_of(&self, pid: Pid) -> Option<Entity> {
        self.entity_of.get(&pid).copied()
    }

    /// Menulis keadaan `entity` ke dokumen `pid` (last-write-wins) dan
    /// menaikkan `version`.
    pub async fn update(
        &mut self,
        world: &World,
        entity: Entity,
        pid: Pid,
    ) -> Result<(), MongoError> {
        let ops = self.reg.update_ops(world, entity)?;
        self.entities.update_one(doc! { "_id": pid.0 }, ops).await?;
        Ok(())
    }

    /// Seperti [`Self::update`], tetapi hanya menulis bila `version` dokumen
    /// masih `expected` (optimistic-lock). Mengembalikan versi baru.
    ///
    /// Kebijakan resolusi konflik (retry / LWW / merge) diserahkan pemanggil —
    /// sejajar `PgStore::update_entity` (RFC-0035 §5).
    pub async fn update_checked(
        &mut self,
        world: &World,
        entity: Entity,
        pid: Pid,
        expected: i64,
    ) -> Result<i64, MongoError> {
        let ops = self.reg.update_ops(world, entity)?;
        let updated = self
            .entities
            .find_one_and_update(doc! { "_id": pid.0, "version": expected }, ops)
            .return_document(ReturnDocument::After)
            .await?;
        match updated {
            Some(document) => Ok(document.get_i64("version").unwrap_or(expected + 1)),
            None => Err(MongoError::Conflict {
                pid,
                expected,
                actual: self.version_of(pid).await?,
            }),
        }
    }

    /// Versi dokumen `pid` saat ini; `None` bila dokumen tak ada. Dipakai untuk
    /// retry setelah [`MongoError::Conflict`].
    pub async fn version_of(&self, pid: Pid) -> Result<Option<i64>, MongoError> {
        Ok(self
            .entities
            .find_one(doc! { "_id": pid.0 })
            .projection(doc! { "version": 1i32 })
            .await?
            .and_then(|d| d.get_i64("version").ok()))
    }

    /// Menghapus dokumen `pid` dan melepas pemetaannya dari working-set.
    pub async fn remove(&mut self, pid: Pid) -> Result<(), MongoError> {
        self.entities.delete_one(doc! { "_id": pid.0 }).await?;
        if let Some(entity) = self.entity_of.remove(&pid) {
            self.pid_of.remove(&entity);
        }
        Ok(())
    }

    /// Koleksi `arke_entities` mentah — jalan keluar untuk query yang belum
    /// dilayani API ini (query builder ditunda ke RFC lanjutan).
    pub fn collection(&self) -> &Collection<Document> {
        &self.entities
    }

    /// Menulis seluruh working-set: upsert tiap entity hidup, lalu hapus
    /// dokumen milik entity yang sudah tak ada di `world`.
    ///
    /// # Atomisitas
    ///
    /// Operasi per-dokumen berurutan **tanpa transaksi**: atomik **per-entity**,
    /// **bukan** per-World. Kegagalan di tengah meninggalkan sebagian entity
    /// tertulis. Ini konsekuensi sadar agar `mongod` standalone cukup — transaksi
    /// multi-dokumen MongoDB menuntut replica set. Janji ini **lebih lemah**
    /// daripada `arke-postgres::PgStore::save`, yang transaksional penuh
    /// (RFC-0035 §5, Amandemen 1).
    ///
    /// Bila `save` gagal di tengah, `pid_of`/`entity_of` tetap konsisten
    /// dengan MongoDB **untuk entity yang sudah diproses** sebelum kegagalan:
    /// `bind` dipanggil tepat setelah `update_one` sukses untuk entity itu,
    /// jadi tak ada tautan yang mengklaim entity yang belum tertulis. Entity
    /// yang belum sempat diproses tetap pada tautan lamanya (atau tanpa
    /// tautan bila baru). Peringatan terpisah: memanggil `save` dengan
    /// `world` yang berbeda dari yang dipakai `save`/`create`/`fetch`
    /// sebelumnya pada `MongoStore` yang sama akan menghapus dokumen milik
    /// `World` yang lama — `entity_of` tak tahu batas antar-`World`, ia hanya
    /// tahu "pid yang tak terlihat di panggilan `save` ini".
    pub async fn save(&mut self, world: &World) -> Result<(), MongoError> {
        // Kumpulkan entity hidup lebih dulu (sinkron) agar `&World` tak ditahan
        // melewati `.await`.
        let mut live: Vec<Entity> = Vec::new();
        <Entity>::each_filtered_shared::<()>(world, |e| live.push(e));

        let mut ops: Vec<(Entity, Option<Pid>, Document)> = Vec::with_capacity(live.len());
        for &entity in &live {
            ops.push((
                entity,
                self.pid_of.get(&entity).copied(),
                self.reg.update_ops(world, entity)?,
            ));
        }

        let mut still_live: HashSet<Pid> = HashSet::with_capacity(ops.len());
        for (entity, existing, update) in ops {
            let pid = existing.unwrap_or_else(Pid::new);
            self.entities
                .update_one(doc! { "_id": pid.0 }, update)
                .upsert(true)
                .await?;
            self.bind(entity, pid);
            still_live.insert(pid);
        }

        // Entity yang hilang dari World (despawn) → hapus dokumennya.
        let stale: Vec<Pid> = self
            .entity_of
            .keys()
            .copied()
            .filter(|pid| !still_live.contains(pid))
            .collect();
        if !stale.is_empty() {
            let ids: Vec<_> = stale.iter().map(|p| p.0).collect();
            self.entities
                .delete_many(doc! { "_id": { "$in": ids } })
                .await?;
            for pid in stale {
                if let Some(entity) = self.entity_of.remove(&pid) {
                    self.pid_of.remove(&entity);
                }
            }
        }
        Ok(())
    }

    /// Menautkan `entity` ↔ `pid`, menjaga `pid_of`/`entity_of` tetap bijektif:
    /// tautan lama pada salah satu sisi dibersihkan dari sisi lainnya.
    fn bind(&mut self, entity: Entity, pid: Pid) {
        if let Some(pid_lama) = self.pid_of.insert(entity, pid)
            && pid_lama != pid
        {
            self.entity_of.remove(&pid_lama);
        }
        if let Some(entity_lama) = self.entity_of.insert(pid, entity)
            && entity_lama != entity
        {
            self.pid_of.remove(&entity_lama);
        }
    }
}
