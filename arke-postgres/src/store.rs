//! [`PgStore`]: persistensi async `World` ↔ Postgres (RFC-0021 §4).
//!
//! `connect`/`register`/`migrate`/`save`/`load` penuh + `update_entity`
//! (optimistic-lock) + `save_incremental` (diff), semua transaksional.
//! `World` adalah *working set*; Postgres sumber kebenaran. Determinisme muat
//! dijaga dengan `ORDER BY entity_id` (STD-0005). Tipe kolom yang didukung:
//! INTEGER/BIGINT/NUMERIC (u64/usize)/REAL/DOUBLE PRECISION/BOOLEAN/TEXT +
//! `Option<T>` nullable + `JSONB` (field non-skalar via `arke::Serialize`).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use arke::{Component, Entity, QueryData, World};
use sqlx::{PgPool, Postgres, Row, postgres::PgArguments, postgres::PgPoolOptions, query::Query};

use crate::cache::{ComponentCache, decode_row, encode_row};
use crate::{ColumnDef, IndexDef, PgComponent, PgType, PgValue, create_table_sql_from};

/// Satu baris komponen yang di-dump: `(entity_id, nilai-kolom)`.
type ComponentRow = (i64, Vec<PgValue>);

/// Keadaan tersimpan satu entity untuk diff inkremental: `generation` + nilai
/// tiap komponen terdaftar (`None` bila entity tak punya komponen itu).
type EntityState = (i64, Vec<Option<Vec<PgValue>>>);

/// Operasi type-erased untuk satu tipe komponen terdaftar.
struct Registered {
    table: &'static str,
    columns: &'static [ColumnDef],
    indexes: &'static [IndexDef],
    checks: &'static [&'static str],
    /// Kumpulkan baris komponen dari `World`.
    dump: fn(&World) -> Vec<ComponentRow>,
    /// Nilai-kolom komponen `T` milik `entity`, bila ada.
    dump_one: fn(&World, Entity) -> Option<Vec<PgValue>>,
    /// Rekonstruksi komponen dari nilai-kolom lalu sisipkan ke `entity`.
    apply: fn(&mut World, Entity, &[PgValue]),
}

fn dump_of<T: PgComponent + Component>(world: &World) -> Vec<ComponentRow> {
    let mut out = Vec::new();
    <(Entity, &T)>::each_filtered_shared::<()>(world, |(e, c)| {
        out.push((i64::from(e.index()), c.to_params()));
    });
    out
}

fn dump_one_of<T: PgComponent + Component>(world: &World, entity: Entity) -> Option<Vec<PgValue>> {
    world.get::<T>(entity).map(PgComponent::to_params)
}

fn apply_of<T: PgComponent + Component>(world: &mut World, entity: Entity, values: &[PgValue]) {
    if let Some(component) = T::from_params(values) {
        world.insert(entity, component);
    }
}

/// Snapshot **owned** dari `World` yang siap di-[`commit`](PgStore::commit) secara
/// async — hasil [`PgStore::stage`]. Tidak memegang `&World`, sehingga `commit`
/// tak menahan World melewati `.await`: future-nya `Send` **tanpa** `World: Sync`.
/// Berguna untuk handler async multi-thread (mis. web server) yang menyimpan
/// `World` di balik mutex — snapshot diambil (sinkron) di bawah lock, lock dilepas,
/// lalu `commit` dijalankan tanpa menahan `World`.
pub struct StagedSave {
    /// **Indeks World** tiap entity yang punya ≥1 komponen (pid dialokasikan saat commit).
    entities: Vec<i64>,
    /// Baris komponen (keyed by indeks), sejajar urutan `registered` saat `stage`.
    components: Vec<Vec<ComponentRow>>,
    /// Rekam sinkron baru (untuk `save_incremental` berikutnya).
    next_state: HashMap<i64, EntityState>,
}

/// Diff **owned** (sync) dari sebuah `World` vs rekam sinkron internal, siap
/// di-[`commit_incremental`](PgStore::commit_incremental) secara async — hasil
/// [`PgStore::stage_incremental`]. Tak memegang `&World`.
pub struct StagedIncremental {
    /// entity yang hilang (ada di rekam, tak ada di world) → DELETE.
    deletes: Vec<i64>,
    /// entity baru/berubah → UPSERT: `(entity_id, (generation, params per-komponen))`.
    upserts: Vec<(i64, EntityState)>,
    /// Rekam sinkron baru setelah commit.
    next_state: HashMap<i64, EntityState>,
}

/// Komponen owned satu entity, siap di-`commit_insert` (RFC-0034). Tak memegang
/// `&World` → future commit `Send` tanpa `World: Sync`.
pub struct StagedInsert {
    rows: Vec<(usize, Vec<PgValue>)>, // (index registered, params)
}

/// Komponen owned satu entity untuk `commit_update` (RFC-0034). `None` = komponen
/// tak ada pada entity (baris komponen itu dihapus).
pub struct StagedUpdate {
    rows: Vec<(usize, Option<Vec<PgValue>>)>,
}

/// Penyimpan Postgres untuk keadaan ECS (RFC-0021).
///
/// Daftarkan tiap tipe komponen via [`Self::register`], `migrate`, lalu
/// `save`/`load`. Hanya komponen `#[derive(PgComponent)]` yang dipersist.
pub struct PgStore {
    pool: PgPool,
    registered: Vec<Registered>,
    /// Rekam keadaan sinkron terakhir (per **indeks World**) untuk `save_incremental`.
    /// Indeks stabil dalam satu sesi working-set (RFC-0034: indeks ephemeral, `pid`
    /// persisten — jembatan `pid_of`/`entity_of` di bawah).
    last: HashMap<i64, EntityState>,
    /// Jembatan **indeks World → `pid`** (RFC-0034). Diisi saat load/materialize &
    /// save; dipakai oleh jalur tulis untuk menulis di bawah `pid` persisten.
    pid_of: HashMap<u32, i64>,
    /// Jembatan **`pid` → Entity** (handle lokal working-set).
    entity_of: HashMap<i64, Entity>,
    /// Cache read-through opsional (RFC-0033); `None` → langsung Postgres.
    cache: Option<Arc<dyn ComponentCache>>,
}

impl PgStore {
    /// Menyambung ke Postgres pada `url` (pool koneksi).
    pub async fn connect(url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new().max_connections(5).connect(url).await?;
        Ok(Self::from_pool(pool))
    }

    /// Membangun dari `PgPool` yang sudah ada.
    pub fn from_pool(pool: PgPool) -> Self {
        Self {
            pool,
            registered: Vec::new(),
            last: HashMap::new(),
            pid_of: HashMap::new(),
            entity_of: HashMap::new(),
            cache: None,
        }
    }

    /// Memasang **cache read-through** (RFC-0033): baca komponen dilayani cache
    /// (hit) atau Postgres (miss, lalu isi cache); tulis meng-invalidate. Postgres
    /// tetap sumber kebenaran. Backend (Redis/Dragonfly) via crate `arke-cache`.
    pub fn with_cache(mut self, cache: Arc<dyn ComponentCache>) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Mendaftarkan tipe komponen `T` untuk dipersist.
    pub fn register<T: PgComponent + Component>(&mut self) -> &mut Self {
        self.registered.push(Registered {
            table: T::TABLE,
            columns: T::COLUMNS,
            indexes: T::INDEXES,
            checks: T::CHECKS,
            dump: dump_of::<T>,
            dump_one: dump_one_of::<T>,
            apply: apply_of::<T>,
        });
        self
    }

    /// Membuat/**merekonsiliasi** tabel `arke_entities` + satu tabel per komponen
    /// terdaftar, idempoten. Menangani **evolusi skema** komponen (RFC-0021 §7):
    ///
    /// - Field **ditambah** → `ALTER TABLE ADD COLUMN` (baris lama di-*backfill*
    ///   dengan default untuk kolom `NOT NULL`).
    /// - Field **dihapus** → kolom usang dijadikan **nullable** (`DROP NOT NULL`)
    ///   — **non-destruktif**: data lama tetap, `INSERT` baru yang tak mengisinya
    ///   jadi valid. (Drop kolom sepenuhnya diserahkan ke migrasi manual.)
    pub async fn migrate(&self) -> Result<(), sqlx::Error> {
        // RFC-0034: `pid` (BIGSERIAL, dialokasikan DB) = identitas persisten;
        // indeks World ephemeral (tak disimpan). `generation` dihapus dari skema.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS arke_entities \
             (pid BIGSERIAL PRIMARY KEY, version BIGINT NOT NULL DEFAULT 0)",
        )
        .execute(&self.pool)
        .await?;
        for r in &self.registered {
            self.reconcile_table(r).await?;
        }
        Ok(())
    }

    // ── API per-operasi berbasis `pid` (RFC-0034) ──────────────────────────────
    // `pid` (BIGSERIAL) = identitas persisten; indeks World ephemeral & lokal.
    // Write reads the entity's components sync (dump_one) before any `.await`, so
    // `&World` is not held across await → future `Send` tanpa `World: Sync`.

    /// **Fase 1 (sync)**: kumpulkan komponen `entity` jadi owned [`StagedInsert`]
    /// (tanpa await, tak menahan `&World`).
    pub fn stage_insert(&self, world: &World, entity: Entity) -> StagedInsert {
        let rows = self
            .registered
            .iter()
            .enumerate()
            .filter_map(|(ci, r)| (r.dump_one)(world, entity).map(|p| (ci, p)))
            .collect();
        StagedInsert { rows }
    }

    /// **Fase 2 (async)**: alokasi `pid` + tulis [`StagedInsert`]. Tak menyentuh World.
    pub async fn commit_insert(&self, staged: StagedInsert) -> Result<i64, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let pid: i64 =
            sqlx::query_scalar("INSERT INTO arke_entities (version) VALUES (0) RETURNING pid")
                .fetch_one(&mut *tx)
                .await?;
        for (ci, params) in &staged.rows {
            let r = &self.registered[*ci];
            let insert = insert_sql(r);
            let mut q = sqlx::query(&insert).bind(pid);
            for (v, col) in params.iter().zip(r.columns) {
                q = bind_value(q, col.ty, v);
            }
            q.execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(pid)
    }

    /// Muat komponen `pid` ke `world` sebagai entity lokal baru; kembalikan handle
    /// (atau `None` bila `pid` tak ada).
    pub async fn fetch(
        &self,
        world: &mut World,
        pid: i64,
    ) -> Result<Option<Entity>, sqlx::Error> {
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT pid FROM arke_entities WHERE pid = $1")
                .bind(pid)
                .fetch_optional(&self.pool)
                .await?;
        if exists.is_none() {
            return Ok(None);
        }
        let entity = world.spawn();
        for r in &self.registered {
            let row = sqlx::query(&select_sql(r, Some("pid = $1")))
                .bind(pid)
                .fetch_optional(&self.pool)
                .await?;
            if let Some(row) = row {
                let mut values = Vec::with_capacity(r.columns.len());
                for col in r.columns {
                    values.push(read_value(&row, col)?);
                }
                (r.apply)(world, entity, &values);
            }
        }
        Ok(Some(entity))
    }

    /// **Fase 1 (sync)**: kumpulkan komponen `entity` jadi owned [`StagedUpdate`].
    pub fn stage_update(&self, world: &World, entity: Entity) -> StagedUpdate {
        let rows = self
            .registered
            .iter()
            .enumerate()
            .map(|(ci, r)| (ci, (r.dump_one)(world, entity)))
            .collect();
        StagedUpdate { rows }
    }

    /// **Fase 2 (async)**: tulis-ulang komponen `pid` (versi naik) dari [`StagedUpdate`].
    pub async fn commit_update(&self, pid: i64, staged: StagedUpdate) -> Result<(), sqlx::Error> {
        let rows = &staged.rows;
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE arke_entities SET version = version + 1 WHERE pid = $1")
            .bind(pid)
            .execute(&mut *tx)
            .await?;
        for (ci, params) in rows {
            let r = &self.registered[*ci];
            sqlx::query(&format!("DELETE FROM {} WHERE pid = $1", r.table))
                .bind(pid)
                .execute(&mut *tx)
                .await?;
            if let Some(params) = params {
                let insert = insert_sql(r);
                let mut q = sqlx::query(&insert).bind(pid);
                for (v, col) in params.iter().zip(r.columns) {
                    q = bind_value(q, col.ty, v);
                }
                q.execute(&mut *tx).await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    /// Hapus `pid` (cascade ke tabel komponen).
    pub async fn remove(&self, pid: i64) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM arke_entities WHERE pid = $1")
            .bind(pid)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Muat entity yang cocok `predicate` (fragmen `WHERE` atas tabel `T`) ke
    /// `world`; kembalikan pasangan `(pid, Entity)`. `predicate` = SQL tepercaya.
    pub async fn query_pids<T: PgComponent>(
        &self,
        world: &mut World,
        predicate: Option<&str>,
    ) -> Result<Vec<(i64, Entity)>, sqlx::Error> {
        let where_c = predicate.map(|p| format!(" WHERE {p}")).unwrap_or_default();
        let sql = format!("SELECT pid FROM {}{} ORDER BY pid", T::TABLE, where_c);
        let pids: Vec<i64> = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(|r| r.try_get("pid"))
            .collect::<Result<_, _>>()?;
        let mut out = Vec::with_capacity(pids.len());
        for pid in pids {
            if let Some(e) = self.fetch(world, pid).await? {
                out.push((pid, e));
            }
        }
        Ok(out)
    }

    /// Membuat tabel komponen bila belum ada, lalu menyelaraskan kolomnya dengan
    /// [`PgComponent::COLUMNS`] terkini (tambah yang hilang; usang → nullable).
    async fn reconcile_table(&self, r: &Registered) -> Result<(), sqlx::Error> {
        sqlx::query(&create_table_sql_from(r.table, r.columns))
            .execute(&self.pool)
            .await?;

        // Tambah kolom yang hilang (field baru); backfill NOT NULL dgn default.
        for c in r.columns {
            let constraint = if c.nullable {
                String::new()
            } else {
                format!(" NOT NULL DEFAULT {}", default_sql(c.ty))
            };
            sqlx::query(&format!(
                "ALTER TABLE {} ADD COLUMN IF NOT EXISTS {} {}{}",
                r.table,
                c.name,
                c.ty.sql(),
                constraint
            ))
            .execute(&self.pool)
            .await?;
        }

        // Kolom usang (field dihapus) → jadikan nullable (non-destruktif).
        let desired: HashSet<&str> = r.columns.iter().map(|c| c.name).collect();
        let existing = sqlx::query(
            "SELECT column_name FROM information_schema.columns \
             WHERE table_schema = current_schema() AND table_name = $1 \
             AND column_name <> 'pid'",
        )
        .bind(r.table)
        .fetch_all(&self.pool)
        .await?;
        for row in existing {
            let name: String = row.try_get("column_name")?;
            if !desired.contains(name.as_str()) {
                sqlx::query(&format!(
                    "ALTER TABLE {} ALTER COLUMN {} DROP NOT NULL",
                    r.table, name
                ))
                .execute(&self.pool)
                .await?;
            }
        }

        // Indeks kustom (`#[pg(index)]`/`#[pg(unique)]`), idempoten.
        for idx in r.indexes {
            let unique = if idx.unique { "UNIQUE " } else { "" };
            sqlx::query(&format!(
                "CREATE {unique}INDEX IF NOT EXISTS idx_{table}_{col} ON {table} ({col})",
                table = r.table,
                col = idx.column
            ))
            .execute(&self.pool)
            .await?;
        }

        // Constraint `CHECK` (`#[pg(check = "…")]`), idempoten via nama + guard.
        for (i, expr) in r.checks.iter().enumerate() {
            sqlx::query(&format!(
                "DO $$ BEGIN \
                   IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'chk_{table}_{i}') THEN \
                     ALTER TABLE {table} ADD CONSTRAINT chk_{table}_{i} CHECK ({expr}); \
                   END IF; \
                 END $$;",
                table = r.table
            ))
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    /// Menulis seluruh working-set `world` ke Postgres dalam **satu transaksi**
    /// (overwrite penuh: hapus lalu tulis-ulang). Deterministik.
    ///
    /// Menyelaraskan rekam sinkron internal, jadi `save_incremental` berikutnya
    /// hanya menulis perubahan **setelah** `save` ini.
    /// **Fase 1 (sinkron)** persist dua-fase: baca seluruh `world` menjadi data
    /// *owned* ([`StagedSave`]) **tanpa `.await`**. Karena tak menahan `&World`
    /// melewati titik async, pemanggil boleh melepas lock `World` sebelum
    /// [`commit`](Self::commit) — membuat future `commit` `Send` tanpa `World: Sync`
    /// (ramah handler async multi-thread; RFC-0021 §4).
    pub fn stage(&self, world: &World) -> StagedSave {
        let mut entities: Vec<i64> = Vec::new();
        <Entity>::each_filtered_shared::<()>(world, |e| {
            entities.push(i64::from(e.index()));
        });
        let components: Vec<Vec<ComponentRow>> =
            self.registered.iter().map(|r| (r.dump)(world)).collect();
        let next_state = self.dump_state(world);
        StagedSave {
            entities,
            components,
            next_state,
        }
    }

    /// **Fase 2 (async)** persist dua-fase: tulis [`StagedSave`] ke Postgres dalam
    /// satu transaksi (overwrite penuh, versi baseline 0). **Tidak menyentuh
    /// `World`**, jadi future-nya tak butuh `World: Sync`.
    ///
    /// Prasyarat: urutan komponen `staged` sama dengan urutan registrasi saat
    /// [`stage`](Self::stage) (tidak ada `register` di antara stage & commit).
    pub async fn commit(&mut self, staged: StagedSave) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        // Overwrite penuh: DELETE meng-cascade ke tabel komponen.
        sqlx::query("DELETE FROM arke_entities")
            .execute(&mut *tx)
            .await?;

        // Alokasi `pid` (BIGSERIAL) tiap entity; catat jembatan indeks→pid (RFC-0034).
        self.pid_of.clear();
        self.entity_of.clear();
        for index in &staged.entities {
            let pid: i64 =
                sqlx::query_scalar("INSERT INTO arke_entities (version) VALUES (0) RETURNING pid")
                    .fetch_one(&mut *tx)
                    .await?;
            self.pid_of.insert(*index as u32, pid);
        }

        // Komponen ditulis di bawah `pid`.
        for (r, rows) in self.registered.iter().zip(&staged.components) {
            let insert = insert_sql(r);
            for (index, params) in rows {
                let pid = self.pid_of[&(*index as u32)];
                let mut q = sqlx::query(&insert).bind(pid);
                for (value, col) in params.iter().zip(r.columns) {
                    q = bind_value(q, col.ty, value);
                }
                q.execute(&mut *tx).await?;
            }
        }

        tx.commit().await?;
        // Overwrite penuh → kosongkan cache (RFC-0033).
        if let Some(c) = &self.cache {
            c.clear().await;
        }
        // Selaraskan rekam sinkron dengan keadaan yang baru ditulis.
        self.last = staged.next_state;
        Ok(())
    }

    /// Simpan **seluruh** `world` (overwrite penuh) dalam satu transaksi — kini
    /// = [`stage`](Self::stage) (sinkron) + [`commit`](Self::commit) (async). Karena
    /// semua pembacaan `world` terjadi sebelum `.await`, future `save` sendiri pun
    /// tak lagi menahan `&World` melewati titik async.
    pub async fn save(&mut self, world: &World) -> Result<(), sqlx::Error> {
        let staged = self.stage(world);
        self.commit(staged).await
    }

    /// Memuat (materialize) **seluruh** keadaan dari Postgres ke `world`,
    /// merekonstruksi entity dengan **handle identik** (via [`World::spawn_at`]).
    /// Ditujukan untuk `World` kosong/segar. Deterministik (`ORDER BY entity_id`).
    ///
    /// Menyelaraskan rekam sinkron internal → `save_incremental` berikutnya hanya
    /// menulis perubahan setelah muat ini.
    pub async fn load(&mut self, world: &mut World) -> Result<(), sqlx::Error> {
        let rows = sqlx::query("SELECT pid FROM arke_entities ORDER BY pid")
            .fetch_all(&self.pool)
            .await?;
        let ids: Vec<i64> = rows
            .iter()
            .map(|r| r.try_get("pid"))
            .collect::<Result<_, _>>()?;
        self.materialize(world, &ids).await?;
        self.last = self.dump_state(world);
        Ok(())
    }

    /// Memuat **subset** entity yang cocok `predicate` (fragmen SQL `WHERE` atas
    /// kolom tabel komponen `T`) beserta **seluruh** komponennya (RFC-0021 §7 v3).
    ///
    /// Contoh: `store.load_where::<Health>(&mut world, "hp < 20").await?` memuat
    /// entity ber-`Health.hp < 20`. Mengembalikan jumlah entity yang dimuat.
    /// Menyelaraskan rekam sinkron (working-set) → aman dikombinasi dengan
    /// `save_incremental` (entity tak-dimuat tak tersentuh).
    ///
    /// **Peringatan:** `predicate` adalah SQL mentah — untuk masukan tepercaya,
    /// bukan input pengguna-akhir (risiko injeksi).
    pub async fn load_where<T: PgComponent>(
        &mut self,
        world: &mut World,
        predicate: &str,
    ) -> Result<usize, sqlx::Error> {
        let sql = format!(
            "SELECT pid FROM {} WHERE {} ORDER BY pid",
            T::TABLE,
            predicate
        );
        let rows = sqlx::query(&sql).fetch_all(&self.pool).await?;
        let ids: Vec<i64> = rows
            .iter()
            .map(|r| r.try_get("pid"))
            .collect::<Result<_, _>>()?;
        self.materialize(world, &ids).await?;
        self.last = self.dump_state(world);
        Ok(ids.len())
    }

    /// Mulai **query builder typed** untuk komponen `T` (RFC-0030) — alternatif
    /// ergonomis & anti-injeksi untuk [`load_where`](Self::load_where).
    pub fn query<T: PgComponent>(&mut self) -> crate::Query<'_, T> {
        crate::Query::new(self)
    }

    /// Eksekutor query builder (RFC-0030): SQL **ter-parameterisasi** + nilai
    /// bind → materialisasi entity yang cocok ke `world`. Dipakai `Query::load`.
    pub(crate) async fn load_by_query(
        &mut self,
        sql: String,
        params: Vec<(PgType, PgValue)>,
        world: &mut World,
    ) -> Result<usize, sqlx::Error> {
        let mut q = sqlx::query(&sql);
        for (ty, val) in &params {
            q = bind_value(q, *ty, val);
        }
        let ids: Vec<i64> = q
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(|r| r.try_get("pid"))
            .collect::<Result<_, _>>()?;
        self.materialize(world, &ids).await?;
        self.last = self.dump_state(world);
        Ok(ids.len())
    }

    /// Rekonstruksi entity `ids` + seluruh komponennya ke `world`.
    async fn materialize(
        &mut self,
        world: &mut World,
        ids: &[i64],
    ) -> Result<Vec<Entity>, sqlx::Error> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        // `ids` = `pid`s. Spawn entity lokal baru (indeks ephemeral, RFC-0034) +
        // catat jembatan indeks↔pid.
        let rows = sqlx::query("SELECT pid FROM arke_entities WHERE pid = ANY($1) ORDER BY pid")
            .bind(ids)
            .fetch_all(&self.pool)
            .await?;
        let mut by_id: HashMap<i64, Entity> = HashMap::with_capacity(rows.len());
        let mut entities: Vec<Entity> = Vec::with_capacity(rows.len());
        for row in rows {
            let pid: i64 = row.try_get("pid")?;
            let entity = world.spawn();
            by_id.insert(pid, entity);
            entities.push(entity);
            self.pid_of.insert(entity.index(), pid);
            self.entity_of.insert(pid, entity);
        }

        for r in &self.registered {
            // Read-through cache (RFC-0033): layani hit dari cache, ambil miss dari
            // Postgres lalu isi cache. Tanpa cache → jalur langsung.
            let cached = match &self.cache {
                Some(c) => c.get_many(r.table, ids).await,
                None => vec![None; ids.len()],
            };
            let mut miss_ids: Vec<i64> = Vec::new();
            for (i, &id) in ids.iter().enumerate() {
                match cached
                    .get(i)
                    .and_then(|b| b.as_deref())
                    .and_then(decode_row)
                {
                    Some(values) => {
                        if let Some(&entity) = by_id.get(&id) {
                            (r.apply)(world, entity, &values);
                        }
                    }
                    None => miss_ids.push(id),
                }
            }
            if miss_ids.is_empty() {
                continue;
            }
            let rows = sqlx::query(&select_sql(r, Some("pid = ANY($1)")))
                .bind(&miss_ids)
                .fetch_all(&self.pool)
                .await?;
            let mut to_cache: Vec<(i64, Vec<u8>)> = Vec::new();
            for row in rows {
                let id: i64 = row.try_get("pid")?;
                let mut values = Vec::with_capacity(r.columns.len());
                for col in r.columns {
                    values.push(read_value(&row, col)?);
                }
                if self.cache.is_some() {
                    to_cache.push((id, encode_row(&values)));
                }
                if let Some(&entity) = by_id.get(&id) {
                    (r.apply)(world, entity, &values);
                }
            }
            if let Some(c) = &self.cache
                && !to_cache.is_empty()
            {
                c.put_many(r.table, &to_cache).await;
            }
        }
        Ok(entities)
    }

    /// Memuat entity **spesifik by id** (beserta seluruh komponennya) ke `world`,
    /// mengembalikan handle-nya (urut sesuai `entity_id`). Menyelaraskan rekam
    /// sinkron untuk working-set → aman dikombinasi dengan `save_incremental`.
    pub async fn load_ids(
        &mut self,
        world: &mut World,
        ids: &[i64],
    ) -> Result<Vec<Entity>, sqlx::Error> {
        let entities = self.materialize(world, ids).await?;
        self.last = self.dump_state(world);
        Ok(entities)
    }

    /// Versi optimistic-lock `entity` di DB, atau `None` bila entity tak ada.
    /// Identitas ke-`pid` diselesaikan via jembatan `pid_of` (RFC-0034); indeks
    /// World bersifat ephemeral sehingga identitas persisten = `pid`.
    pub async fn entity_version(&self, entity: Entity) -> Result<Option<i64>, sqlx::Error> {
        let Some(&pid) = self.pid_of.get(&entity.index()) else {
            return Ok(None);
        };
        let row = sqlx::query("SELECT version FROM arke_entities WHERE pid = $1")
            .bind(pid)
            .fetch_optional(&self.pool)
            .await?;
        match row {
            Some(row) => Ok(Some(row.try_get("version")?)),
            None => Ok(None),
        }
    }

    /// **Tulis-balik ber-optimistic-lock**: memperbarui baris `entity` (versi
    /// naik) beserta komponennya dari `world`, **hanya bila** versi DB masih
    /// `expected_version` (RFC-0021 §5). Identitas ke-`pid` via `pid_of`
    /// (RFC-0034): indeks World ephemeral → gerbang cukup pada versi.
    ///
    /// Mengembalikan versi baru bila sukses, atau [`UpdateError::Conflict`] bila
    /// writer lain telah mengubah entity ini (versi tak cocok / entity tak ada).
    /// Transaksional: pada konflik, tak ada perubahan.
    pub async fn update_entity(
        &self,
        world: &World,
        entity: Entity,
        expected_version: i64,
    ) -> Result<i64, UpdateError> {
        let Some(&pid) = self.pid_of.get(&entity.index()) else {
            return Err(UpdateError::Conflict);
        };
        let mut tx = self.pool.begin().await.map_err(UpdateError::Db)?;

        // Gerbang: naikkan versi hanya bila versi cocok.
        let new_version: Option<i64> = sqlx::query_scalar(
            "UPDATE arke_entities SET version = version + 1 \
             WHERE pid = $1 AND version = $2 \
             RETURNING version",
        )
        .bind(pid)
        .bind(expected_version)
        .fetch_optional(&mut *tx)
        .await
        .map_err(UpdateError::Db)?;

        let Some(new_version) = new_version else {
            // 0 baris → versi lain / entity tak ada → konflik.
            return Err(UpdateError::Conflict);
        };

        // Ganti komponen entity ini dengan keadaan `world` saat ini.
        for r in &self.registered {
            sqlx::query(&format!("DELETE FROM {} WHERE pid = $1", r.table))
                .bind(pid)
                .execute(&mut *tx)
                .await
                .map_err(UpdateError::Db)?;
            if let Some(params) = (r.dump_one)(world, entity) {
                let insert = insert_sql(r);
                let mut q = sqlx::query(&insert).bind(pid);
                for (value, col) in params.iter().zip(r.columns) {
                    q = bind_value(q, col.ty, value);
                }
                q.execute(&mut *tx).await.map_err(UpdateError::Db)?;
            }
        }

        tx.commit().await.map_err(UpdateError::Db)?;
        // Invalidate cache untuk entity ini di tiap tabel (RFC-0033).
        if let Some(c) = &self.cache {
            for r in &self.registered {
                c.invalidate(r.table, &[pid]).await;
            }
        }
        Ok(new_version)
    }

    /// Kumpulkan keadaan seluruh entity + komponen `world` (untuk diff).
    fn dump_state(&self, world: &World) -> HashMap<i64, EntityState> {
        let n = self.registered.len();
        let mut current: HashMap<i64, EntityState> = HashMap::new();
        <Entity>::each_filtered_shared::<()>(world, |e| {
            current.insert(
                i64::from(e.index()),
                (i64::from(e.generation()), vec![None; n]),
            );
        });
        for (ci, r) in self.registered.iter().enumerate() {
            for (id, params) in (r.dump)(world) {
                if let Some(state) = current.get_mut(&id) {
                    state.1[ci] = Some(params);
                }
            }
        }
        current
    }

    /// **Tulis-balik inkremental**: menulis (UPSERT, versi naik) hanya entity
    /// yang **baru atau berubah** sejak sinkron terakhir, dan meng-DELETE yang
    /// **hilang** — dengan mem-*diff* `world` terhadap rekam internal (RFC-0021 §7).
    ///
    /// Panggilan **pertama** (rekam kosong) menulis semua entity `world` (sinkron
    /// awal); baris DB pra-ada yang tak dikenal rekam **tak** dihapus. Satu
    /// transaksi. Cocok untuk checkpoint berkala world besar (hemat I/O).
    ///
    /// Catatan: diff berbasis-nilai (arke tak melacak perubahan otomatis), jadi
    /// `PgStore` menyimpan salinan keadaan terakhir (biaya memori per entity).
    pub async fn save_incremental(&mut self, world: &World) -> Result<SyncStats, sqlx::Error> {
        let staged = self.stage_incremental(world);
        self.commit_incremental(staged).await
    }

    /// **Fase 1 (sync)** incremental dua-fase: diff `world` vs rekam sinkron internal
    /// → [`StagedIncremental`] owned (tanpa await, tak menahan `&World`). Membuat
    /// `commit_incremental` `Send` tanpa `World: Sync` (ramah handler async).
    pub fn stage_incremental(&self, world: &World) -> StagedIncremental {
        let current = self.dump_state(world);
        let deletes: Vec<i64> = self
            .last
            .keys()
            .copied()
            .filter(|id| !current.contains_key(id))
            .collect();
        let upserts: Vec<(i64, EntityState)> = current
            .iter()
            .filter(|&(id, state)| self.last.get(id) != Some(state))
            .map(|(id, state)| (*id, state.clone()))
            .collect();
        StagedIncremental {
            deletes,
            upserts,
            next_state: current,
        }
    }

    /// **Fase 2 (async)** incremental dua-fase: terapkan diff (UPSERT versi-naik +
    /// DELETE hilang) dalam 1 transaksi, perbarui rekam sinkron + invalidate cache.
    /// **Tidak menyentuh `World`.**
    pub async fn commit_incremental(
        &mut self,
        staged: StagedIncremental,
    ) -> Result<SyncStats, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let mut stats = SyncStats {
            written: 0,
            deleted: 0,
        };
        // Entity yang tersentuh (dihapus/berubah) → invalidate cache (RFC-0033).
        // Kunci diff internal = **indeks** World (ephemeral, stabil dalam-sesi);
        // identitas DB = `pid` diselesaikan via `pid_of` (RFC-0034).
        let mut affected: Vec<i64> = Vec::new();

        // Hilang → DELETE (cascade ke tabel komponen).
        for index in &staged.deletes {
            if let Some(pid) = self.pid_of.remove(&(*index as u32)) {
                sqlx::query("DELETE FROM arke_entities WHERE pid = $1")
                    .bind(pid)
                    .execute(&mut *tx)
                    .await?;
                self.entity_of.remove(&pid);
                affected.push(pid);
                stats.deleted += 1;
            }
        }

        // Baru/berubah → alokasi `pid` untuk entity baru, UPSERT (versi naik) +
        // ganti baris komponen.
        for (index, state) in &staged.upserts {
            let pid = match self.pid_of.get(&(*index as u32)) {
                Some(&pid) => {
                    sqlx::query("UPDATE arke_entities SET version = version + 1 WHERE pid = $1")
                        .bind(pid)
                        .execute(&mut *tx)
                        .await?;
                    pid
                }
                None => {
                    let pid: i64 = sqlx::query_scalar(
                        "INSERT INTO arke_entities (version) VALUES (0) RETURNING pid",
                    )
                    .fetch_one(&mut *tx)
                    .await?;
                    self.pid_of.insert(*index as u32, pid);
                    pid
                }
            };
            for (ci, r) in self.registered.iter().enumerate() {
                sqlx::query(&format!("DELETE FROM {} WHERE pid = $1", r.table))
                    .bind(pid)
                    .execute(&mut *tx)
                    .await?;
                if let Some(params) = &state.1[ci] {
                    let insert = insert_sql(r);
                    let mut q = sqlx::query(&insert).bind(pid);
                    for (value, col) in params.iter().zip(r.columns) {
                        q = bind_value(q, col.ty, value);
                    }
                    q.execute(&mut *tx).await?;
                }
            }
            affected.push(pid);
            stats.written += 1;
        }

        tx.commit().await?;
        if let Some(c) = &self.cache
            && !affected.is_empty()
        {
            for r in &self.registered {
                c.invalidate(r.table, &affected).await;
            }
        }
        self.last = staged.next_state;
        Ok(stats)
    }
}

/// Ringkasan sinkron inkremental [`PgStore::save_incremental`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncStats {
    /// Jumlah entity yang ditulis (baru/berubah).
    pub written: usize,
    /// Jumlah entity yang dihapus (hilang sejak sinkron terakhir).
    pub deleted: usize,
}

/// Kegagalan [`PgStore::update_entity`].
#[derive(Debug)]
pub enum UpdateError {
    /// Versi/identitas DB tak cocok expektasi — writer lain telah mengubah entity.
    Conflict,
    /// Galat database.
    Db(sqlx::Error),
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::Conflict => write!(f, "optimistic-lock: versi/identitas entity berubah"),
            UpdateError::Db(e) => write!(f, "database: {e}"),
        }
    }
}

impl std::error::Error for UpdateError {}

/// Default backfill untuk kolom `NOT NULL` yang ditambahkan ke tabel ber-baris.
fn default_sql(ty: PgType) -> &'static str {
    match ty {
        PgType::Integer
        | PgType::BigInt
        | PgType::Numeric
        | PgType::Real
        | PgType::DoublePrecision => "0",
        PgType::Boolean => "false",
        PgType::Text => "''",
        PgType::Jsonb => "'null'::jsonb",
    }
}

/// Cast eksplisit placeholder INSERT untuk tipe yang di-bind sebagai teks
/// (Postgres tak meng-cast text→jsonb/numeric implisit).
fn insert_cast(ty: PgType) -> &'static str {
    match ty {
        PgType::Jsonb => "::jsonb",
        PgType::Numeric => "::numeric",
        _ => "",
    }
}

/// Apakah kolom `ty` dibaca sebagai teks (`col::text`) — JSONB & NUMERIC, agar
/// tak butuh dependensi serde/bigdecimal.
fn read_as_text(ty: PgType) -> bool {
    matches!(ty, PgType::Jsonb | PgType::Numeric)
}

/// `INSERT INTO cmp_x (pid, c1, c2::jsonb, …) VALUES ($1, $2, $3::jsonb, …)`.
fn insert_sql(r: &Registered) -> String {
    let mut cols = String::from("pid");
    let mut placeholders = String::from("$1");
    for (i, col) in r.columns.iter().enumerate() {
        cols.push_str(", ");
        cols.push_str(col.name);
        placeholders.push_str(&format!(", ${}{}", i + 2, insert_cast(col.ty)));
    }
    format!(
        "INSERT INTO {} ({}) VALUES ({})",
        r.table, cols, placeholders
    )
}

/// `SELECT pid, c1, c2::text AS c2, … FROM cmp_x [WHERE <filter>] ORDER BY pid`.
///
/// Kolom `JSONB`/`NUMERIC` dibaca sebagai teks (`::text`).
fn select_sql(r: &Registered, filter: Option<&str>) -> String {
    let mut cols = String::from("pid");
    for col in r.columns {
        cols.push_str(", ");
        if read_as_text(col.ty) {
            cols.push_str(&format!("{name}::text AS {name}", name = col.name));
        } else {
            cols.push_str(col.name);
        }
    }
    let where_clause = match filter {
        Some(f) => format!(" WHERE {f}"),
        None => String::new(),
    };
    format!(
        "SELECT {} FROM {}{} ORDER BY pid",
        cols, r.table, where_clause
    )
}

/// Bind satu [`PgValue`] ke query. `col_ty` menentukan tipe `NULL` yang benar.
fn bind_value<'q>(
    q: Query<'q, Postgres, PgArguments>,
    col_ty: PgType,
    value: &PgValue,
) -> Query<'q, Postgres, PgArguments> {
    match value {
        PgValue::Int(i) => q.bind(*i),
        PgValue::Float(f) => q.bind(*f),
        PgValue::Bool(b) => q.bind(*b),
        PgValue::Text(s) => q.bind(s.clone()),
        // JSON/NUMERIC di-bind sebagai teks; placeholder `$n::jsonb`/`::numeric` meng-cast.
        PgValue::Json(s) => q.bind(s.clone()),
        PgValue::Numeric(s) => q.bind(s.clone()),
        // `NULL` di-bind dengan tipe kolom yang benar (protokol Postgres).
        PgValue::Null => match col_ty {
            PgType::Integer => q.bind(Option::<i32>::None),
            PgType::BigInt => q.bind(Option::<i64>::None),
            PgType::Real => q.bind(Option::<f32>::None),
            PgType::DoublePrecision => q.bind(Option::<f64>::None),
            PgType::Boolean => q.bind(Option::<bool>::None),
            // JSONB/NUMERIC NULL di-bind sebagai teks NULL; `::jsonb`/`::numeric` meng-cast.
            PgType::Text | PgType::Jsonb | PgType::Numeric => q.bind(Option::<String>::None),
        },
    }
}

/// Baca satu kolom baris menjadi [`PgValue`] sesuai tipenya (`NULL` → `Null`).
fn read_value(row: &sqlx::postgres::PgRow, col: &ColumnDef) -> Result<PgValue, sqlx::Error> {
    Ok(match col.ty {
        PgType::Integer => match row.try_get::<Option<i32>, _>(col.name)? {
            Some(v) => PgValue::Int(i64::from(v)),
            None => PgValue::Null,
        },
        PgType::BigInt => match row.try_get::<Option<i64>, _>(col.name)? {
            Some(v) => PgValue::Int(v),
            None => PgValue::Null,
        },
        PgType::Real => match row.try_get::<Option<f32>, _>(col.name)? {
            Some(v) => PgValue::Float(f64::from(v)),
            None => PgValue::Null,
        },
        PgType::DoublePrecision => match row.try_get::<Option<f64>, _>(col.name)? {
            Some(v) => PgValue::Float(v),
            None => PgValue::Null,
        },
        PgType::Boolean => match row.try_get::<Option<bool>, _>(col.name)? {
            Some(v) => PgValue::Bool(v),
            None => PgValue::Null,
        },
        PgType::Text => match row.try_get::<Option<String>, _>(col.name)? {
            Some(v) => PgValue::Text(v),
            None => PgValue::Null,
        },
        // Dibaca lewat `col::text` (lihat `select_sql`).
        PgType::Jsonb => match row.try_get::<Option<String>, _>(col.name)? {
            Some(v) => PgValue::Json(v),
            None => PgValue::Null,
        },
        PgType::Numeric => match row.try_get::<Option<String>, _>(col.name)? {
            Some(v) => PgValue::Numeric(v),
            None => PgValue::Null,
        },
    })
}
