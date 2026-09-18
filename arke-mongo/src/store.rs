//! [`MongoStore`]: persistensi async `World` ↔ MongoDB (RFC-0035 §5).
//!
//! Satu-satunya modul yang menyentuh I/O; seluruh logika pemetaan hidup di
//! `bson_map` dan `registry` sebagai fungsi murni.

use std::collections::{HashMap, HashSet};

use arke::{Entity, QueryData, World, WorldId};
use futures_util::TryStreamExt;
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
///
/// # Satu store, satu `World`
///
/// `pid_of`/`entity_of` mengunci `Entity` — handle yang cuma bermakna di
/// dalam **satu** `World` — ke `pid` yang persisten (RFC-0035 §3). Store
/// **menautkan diri ke `World` pertama** yang dilayaninya
/// (`create`/`fetch`/`load`/`save`/`update`/`update_checked`), dikenali lewat
/// [`arke::World::id`]; operasi dengan `World` lain ditolak dengan
/// [`MongoError::WorldMismatch`] alih-alih merusak data diam-diam. Tiga mode
/// salah-pakai yang dulu lolos tanpa penjaga (v0.1 pra-`WorldId`):
///
/// - **`fetch` ke World scratch merebut `pid`** — `fetch(&mut scratch, pid)`
///   menaut ulang `pid` ke entity di `scratch`; `save` berikutnya atas World
///   asli mencetak pid baru dan menghapus dokumen lama. Kini: `Err(WorldMismatch)`.
/// - **Handle `Entity` bertabrakan antar-World** — dua World independen
///   men-spawn `Entity` identik; `save(&w2)` menimpa dokumen entity `w1`.
///   Kini: `Err(WorldMismatch)`.
/// - **`save` dengan World berbeda** menghapus dokumen milik World lama. Kini:
///   `Err(WorldMismatch)`.
///
/// Untuk melayani `World` lain (mis. World sekali-pakai untuk inspeksi, atau
/// pola World per-request), pakai [`Self::fork`]: store baru dengan
/// klien/database/registry yang sama dan jembatan kosong.
pub struct MongoStore {
    db: Database,
    entities: Collection<Document>,
    reg: Registry,
    /// Jembatan Entity (indeks ephemeral) → `pid` persisten (RFC-0034 §2).
    pid_of: HashMap<Entity, Pid>,
    /// Jembatan `pid` → Entity (handle lokal working-set).
    entity_of: HashMap<Pid, Entity>,
    /// `World` yang ditautkan (lihat "Satu store, satu `World`"); `None`
    /// sebelum operasi ber-`&World` pertama.
    world_id: Option<WorldId>,
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
            world_id: None,
        })
    }

    /// Store baru dengan klien, database, dan registry yang **sama** tetapi
    /// jembatan `pid_of`/`entity_of` **kosong** dan belum tertaut ke `World`
    /// mana pun — untuk melayani `World` lain (World sekali-pakai, atau pola
    /// World per-request). Murah: `Database` adalah handle ber-`Arc`.
    pub fn fork(&self) -> Self {
        Self {
            db: self.db.clone(),
            entities: self.entities.clone(),
            reg: self.reg.clone(),
            pid_of: HashMap::new(),
            entity_of: HashMap::new(),
            world_id: None,
        }
    }

    /// `World` yang ditautkan store ini, bila sudah ada.
    pub fn bound_world(&self) -> Option<WorldId> {
        self.world_id
    }

    /// Menautkan store ke `world` (pada operasi pertama) atau menolak bila
    /// `world` bukan `World` yang sudah ditautkan.
    fn bind_world(&mut self, world: &World) -> Result<(), MongoError> {
        match self.world_id {
            None => {
                self.world_id = Some(world.id());
                Ok(())
            }
            Some(bound) if bound == world.id() => Ok(()),
            Some(bound) => Err(MongoError::WorldMismatch {
                bound,
                given: world.id(),
            }),
        }
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
    ///
    /// Bukan `async fn`: parameter `async fn` hidup di state future sampai
    /// selesai, sehingga `&World` tertangkap melewati `.await` dan future
    /// pemanggil menjadi `!Send` (`World` bukan `Sync`). `world` selesai dibaca
    /// di sini; future yang dikembalikan hanya memegang `&mut self` + BSON owned
    /// dan dijamin `Send` (sejajar `arke-postgres` ≥ 0.17.1). Galat fase sinkron
    /// (`WorldMismatch`, validasi nama field) dilaporkan lewat future, sebelum
    /// satu pun tulis.
    pub fn create<'s>(
        &'s mut self,
        world: &World,
        entity: Entity,
    ) -> impl std::future::Future<Output = Result<Pid, MongoError>> + Send + 's {
        let staged = self
            .bind_world(world)
            .and_then(|()| self.reg.cmp_doc(world, entity));
        async move {
            let cmp = staged?;
            let pid = Pid::new();
            self.entities
                .insert_one(doc! { "_id": pid.0, "version": 0i64, "cmp": cmp })
                .await?;
            self.bind(entity, pid);
            Ok(pid)
        }
    }

    /// Memuat dokumen `pid` ke `world` sebagai entity baru; `None` bila dokumen
    /// tak ada.
    ///
    /// Lihat [`Self::materialize`] untuk perlakuan `cmp` absen vs. korup dan
    /// jaminan despawn-on-error. `World` lain dari yang ditautkan store →
    /// [`MongoError::WorldMismatch`] (pakai [`Self::fork`]) — lihat "Satu
    /// store, satu `World`" pada [`MongoStore`].
    pub async fn fetch(
        &mut self,
        world: &mut World,
        pid: Pid,
    ) -> Result<Option<Entity>, MongoError> {
        self.bind_world(world)?;
        let Some(document) = self.entities.find_one(doc! { "_id": pid.0 }).await? else {
            return Ok(None);
        };
        Ok(Some(self.materialize(world, pid, &document)?))
    }

    /// Memuat seluruh koleksi ke `world` sebagai working-set.
    ///
    /// Diurutkan `_id` menaik supaya urutan materialisasi identik antar-run
    /// (STD-0005) — sejajar `ORDER BY pid` di `arke-postgres`.
    ///
    /// Memuat **seluruh** koleksi tanpa paging; materialisasi parsial
    /// (`load_where`) ditunda ke RFC lanjutan.
    ///
    /// `World` lain dari yang ditautkan store → [`MongoError::WorldMismatch`]
    /// (pakai [`Self::fork`]) — lihat "Satu store, satu `World`" pada
    /// [`MongoStore`].
    ///
    /// **`load` menambah, bukan mengganti.** [`Self::materialize`] selalu
    /// `world.spawn()` entity baru; memanggil `load` dua kali ke `World`
    /// yang sama menggandakan tiap entity — panggilan kedua men-`bind` ulang
    /// tiap `pid` ke entity barunya, sehingga salinan pertama jadi entity
    /// yatim (masih ada di `World`, tapi tak lagi tertaut ke `pid` mana pun
    /// di `pid_of`/`entity_of`). `load` mengasumsikan `world` kosong atau
    /// berisi entity yang memang bukan milik store ini.
    pub async fn load(&mut self, world: &mut World) -> Result<(), MongoError> {
        self.bind_world(world)?;
        let mut cursor = self.entities.find(doc! {}).sort(doc! { "_id": 1 }).await?;
        while let Some(document) = cursor.try_next().await? {
            let Ok(oid) = document.get_object_id("_id") else {
                continue; // dokumen dengan `_id` non-ObjectId bukan milik arke
            };
            self.materialize(world, Pid(oid), &document)?;
        }
        Ok(())
    }

    /// Memuat komponen dari `document` ke entity baru di `world`, menautkannya
    /// ke `pid`. Entity dibuang lagi bila dokumen gagal di-decode, sehingga
    /// `world` tak pernah menyimpan entity separuh terisi.
    ///
    /// Dipakai bersama oleh [`Self::fetch`] dan [`Self::load`] — satu-satunya
    /// tempat urutan spawn / decode `cmp` / apply / despawn-on-error / bind
    /// hidup, agar keduanya tak lagi bisa hanyut berbeda (lih. RFC-0035 §6).
    /// Field `cmp` yang **absen** berarti entity legit tanpa komponen
    /// terdaftar (lanjut, `Ok`); `cmp` yang **ada tapi bukan sub-dokumen**
    /// berarti dokumen korup — bukan sesuatu yang boleh diperlakukan sebagai
    /// "entity tanpa komponen" secara diam-diam.
    fn materialize(
        &mut self,
        world: &mut World,
        pid: Pid,
        document: &Document,
    ) -> Result<Entity, MongoError> {
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
        Ok(entity)
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
    ///
    /// # Err bila dokumen sudah tak ada
    ///
    /// `update` tak meng-upsert: bila `pid` tak lagi cocok dengan dokumen
    /// mana pun (mis. dihapus penulis lain di antara `fetch` dan `update`
    /// ini), tulisan tak mengenai apa pun dan hasilnya
    /// `Err(MongoError::Missing { pid })` — bukan `Ok(())` yang diam-diam
    /// membuang tulisan. Ini beda perlakuan sengaja dari [`Self::remove`],
    /// yang idempoten terhadap dokumen yang sudah tak ada (delete memang
    /// wajar dipanggil dua kali); `update` bukan kasus yang sama karena
    /// pemanggil datang membawa data yang mengira akan tersimpan.
    pub async fn update(
        &mut self,
        world: &World,
        entity: Entity,
        pid: Pid,
    ) -> Result<(), MongoError> {
        self.bind_world(world)?;
        let ops = self.reg.update_ops(world, entity)?;
        let result = self.entities.update_one(doc! { "_id": pid.0 }, ops).await?;
        if result.matched_count == 0 {
            return Err(MongoError::Missing { pid });
        }
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
        self.bind_world(world)?;
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
    ///
    /// `None` sebenarnya menutupi tiga keadaan: dokumen tak ada, field
    /// `version` absen, atau `version` bukan Int64. Dokumen yang ditulis
    /// `arke-mongo` sendiri (`create`/`save`) selalu punya `version` bertipe
    /// Int64, jadi dua kemungkinan terakhir hanya muncul pada dokumen asing
    /// (ditulis service lain, atau dikorupsi) — untuk dokumen milik
    /// `arke-mongo`, `None` di sini memang berarti "dokumen tak ada".
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
    /// **Versi awal berbeda dari [`Self::create`]:** `create` menyisip
    /// `version: 0` secara eksplisit, sedangkan upsert `$inc` di sini atas
    /// dokumen yang belum ada dimulai dari nol dan menaikkannya ke `1` dalam
    /// operasi yang sama — jadi entity yang lahir lewat `save` mulai dari
    /// `version: 1`, bukan `0`. Tak terlihat dari luar sampai dikombinasikan
    /// dengan [`Self::update_checked`]: `save(&w)` diikuti
    /// `update_checked(&w, e, pid, 0)` akan selalu `Conflict` (`expected: 0`
    /// vs. `actual: Some(1)`) walau belum ada penulis lain yang menyentuhnya.
    /// Pemanggil yang mencampur `save` dan `update_checked` atas entity yang
    /// sama perlu membaca versi lewat [`Self::version_of`] dulu, bukan
    /// mengasumsikan `0`.
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
    /// tautan bila baru).
    ///
    /// `save` dengan `World` yang berbeda dari yang ditautkan store →
    /// [`MongoError::WorldMismatch`] (dulu: menghapus dokumen milik `World`
    /// lama) — lihat "Satu store, satu `World`" pada [`MongoStore`].
    ///
    /// Bukan `async fn` (lihat [`create`](Self::create)): seluruh pembacaan
    /// `world` terjadi secara sinkron sebelum kembali, future yang dikembalikan
    /// tak memegang `&World` dan `Send`.
    pub fn save<'s>(
        &'s mut self,
        world: &World,
    ) -> impl std::future::Future<Output = Result<(), MongoError>> + Send + 's {
        let staged = self.stage_save(world);
        async move {
            let ops = staged?;
            self.commit_save(ops).await
        }
    }

    /// Fase sinkron [`save`](Self::save): kumpulkan operasi tulis per entity
    /// hidup. `update_ops` bisa gagal dengan `InvalidName`/`DuplicateField`
    /// (validasi nama field, RFC-0035 Am. 2); mengumpulkan semua operasi dulu
    /// berarti galat semacam itu membatalkan seluruh `save` **sebelum** satu
    /// dokumen pun tertulis — bukan meninggalkan separuh entity tertulis dan
    /// separuh gagal di tengah jalan. Biayanya: BSON seluruh entity hidup ditahan
    /// di memori sekaligus sebelum satu pun ditulis.
    fn stage_save(
        &mut self,
        world: &World,
    ) -> Result<Vec<(Entity, Option<Pid>, Document)>, MongoError> {
        self.bind_world(world)?;
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
        Ok(ops)
    }

    /// Fase async [`save`](Self::save): upsert tiap entity, lalu hapus dokumen
    /// entity yang hilang dari World. **Tidak menyentuh `World`.**
    async fn commit_save(
        &mut self,
        ops: Vec<(Entity, Option<Pid>, Document)>,
    ) -> Result<(), MongoError> {
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
