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

use arke::{Component, Entity, QueryData, World, WorldId};
use sqlx::{PgConnection, PgPool, Postgres, Row, postgres::PgArguments, query::Query};

use crate::tx::PgTx;

use crate::cache::{ComponentCache, decode_row, encode_row};
use crate::query::tsvector_expr;
use crate::{
    ColumnDef, CompositeIndexDef, FtsDef, IndexDef, OnDelete, OnDeleteDef, PgComponent, PgType,
    PgValue, create_table_sql_from, pack_entity, quote_ident, unpack_entity,
};

/// Satu baris komponen yang di-dump: `(entity, nilai-kolom)`.
type ComponentRow = (Entity, Vec<PgValue>);

/// Indeks sentinel untuk relasi **menggantung** (target tak ikut ter-muat, RFC-0034
/// Am.3): handle `Entity::from_raw(DANGLING_INDEX, 0)` tak akan me-resolve di World
/// mana pun (butuh ~4 miliar entity untuk bertabrakan). Membedakan "relasi ada tapi
/// target tak dimuat" dari "field None" (NULL).
const DANGLING_INDEX: i64 = u32::MAX as i64;

/// Keadaan tersimpan satu entity untuk diff inkremental: nilai tiap komponen
/// terdaftar (`None` bila entity tak punya komponen itu). Kunci peta = `Entity`
/// utuh (indeks + generation), jadi slot terdaur-ulang = entity baru.
type EntityState = Vec<Option<Vec<PgValue>>>;

/// Operasi type-erased untuk satu tipe komponen terdaftar.
#[derive(Clone)]
struct Registered {
    table: &'static str,
    /// Namespace cache (RFC-0033): `table@fingerprint(kolom)`. Skema yang berubah
    /// (field ditambah/diganti tipe) otomatis memakai namespace baru, sehingga
    /// baris cache dari deploy lama — yang bentuknya tak lagi cocok dengan
    /// `from_params` — tak pernah disajikan (dulu: komponen hilang diam-diam,
    /// lalu `save_incremental` menghapus barisnya).
    cache_ns: String,
    columns: &'static [ColumnDef],
    indexes: &'static [IndexDef],
    /// Indeks komposit (RFC-0037).
    composites: &'static [CompositeIndexDef],
    /// Aksi hapus relasi (RFC-0039).
    on_delete: &'static [OnDeleteDef],
    /// Kolom full-text (`#[pg(fts)]`) → indeks GIN ekspresi.
    fts: &'static [FtsDef],
    checks: &'static [&'static str],
    /// Kumpulkan baris komponen dari `World`.
    dump: fn(&World) -> Vec<ComponentRow>,
    /// Nilai-kolom komponen `T` milik `entity`, bila ada.
    dump_one: fn(&World, Entity) -> Option<Vec<PgValue>>,
    /// Rekonstruksi komponen dari nilai-kolom lalu sisipkan ke `entity`.
    apply: fn(&mut World, Entity, &[PgValue]),
    /// Lepas komponen dari `entity` (refresh entity yang sudah termuat dan
    /// barisnya kini tiada di DB).
    remove: fn(&mut World, Entity),
}

/// FNV-1a 64-bit — deterministik lintas rilis Rust (bukan `DefaultHasher`).
fn fnv1a(parts: &[&[u8]]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for &b in *part {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    h
}

/// Fingerprint deterministik definisi kolom (atas `nama:tipe:null`).
fn schema_fingerprint(columns: &[ColumnDef]) -> u64 {
    let mut parts: Vec<&[u8]> = Vec::with_capacity(columns.len() * 4);
    for c in columns {
        parts.push(c.name.as_bytes());
        parts.push(b":");
        parts.push(c.ty.sql().as_bytes());
        parts.push(if c.nullable { b":n;" } else { b":;" });
    }
    fnv1a(&parts)
}

/// Nama constraint CHECK **content-addressed**: `chk_<tabel>_<fnv64(expr)>`.
/// Ekspresi berbeda → nama berbeda, sehingga `migrate` dapat merekonsiliasi
/// perubahan ekspresi tanpa registry nama. Bagian tabel dipangkas agar total
/// ≤ 63 byte (batas identifier Postgres; bila lebih, Postgres memotong diam-diam
/// dan nama tak lagi cocok saat dibandingkan).
fn check_constraint_name(table: &str, expr: &str) -> String {
    let hash = format!("{:016x}", fnv1a(&[expr.trim().as_bytes()]));
    // "chk_" (4) + tabel + "_" (1) + 16 hex ≤ 63 → tabel ≤ 42 byte.
    format!("chk_{}_{hash}", truncate_ident(table, 42))
}

/// Potong `s` ke paling banyak `max` byte di batas karakter — nama identifier
/// Postgres dibatasi 63 byte dan dipotong diam-diam bila lebih.
fn truncate_ident(s: &str, max: usize) -> &str {
    let mut t = s;
    while t.len() > max {
        let mut cut = t.len() - 1;
        while !t.is_char_boundary(cut) {
            cut -= 1;
        }
        t = &t[..cut];
    }
    t
}

/// Nama indeks komposit **content-addressed** (RFC-0037):
/// `cidx_<tabel>_<fnv64(unik|kolom)>`. Definisi yang berubah = nama baru, jadi
/// `migrate` cukup membandingkan nama — pola yang sama dengan
/// [`check_constraint_name`].
fn composite_index_name(table: &str, def: &CompositeIndexDef) -> String {
    let kind: &[u8] = if def.unique { b"u" } else { b"i" };
    let cols = def.columns.join(",");
    let hash = format!("{:016x}", fnv1a(&[kind, b"|", cols.as_bytes()]));
    // "cidx_" (5) + tabel + "_" (1) + 16 hex ≤ 63 → tabel ≤ 41 byte.
    format!("cidx_{}_{hash}", truncate_ident(table, 41))
}

/// Nama objek aksi hapus (RFC-0039) untuk kolom relasi `def` di `table`:
/// `(fk, indeks, trigger)`. FK & indeks memuat nama tabel (disaring per tabel
/// saat rekonsiliasi); trigger hidup di `arke_entities` yang dipakai bersama
/// semua tabel, jadi awalannya memakai hash tabel agar tak pernah bertumpang
/// dengan awalan tabel lain.
fn on_delete_names(table: &str, def: &OnDeleteDef) -> (String, String, String) {
    let action: &[u8] = match def.action {
        OnDelete::Cascade => b"c",
        OnDelete::SetNull => b"n",
        OnDelete::Restrict => b"r",
    };
    let t = truncate_ident(table, 41);
    let col = def.column.as_bytes();
    (
        format!("afk_{t}_{:016x}", fnv1a(&[col, b"|", action])),
        format!("aidx_{}_{:016x}", truncate_ident(table, 40), fnv1a(&[col])),
        format!("{}{:016x}", on_delete_trigger_prefix(table), fnv1a(&[col])),
    )
}

/// Awalan trigger cascade milik `table` di `arke_entities`.
fn on_delete_trigger_prefix(table: &str) -> String {
    format!("arke_ondel_{:016x}_", fnv1a(&[table.as_bytes()]))
}

fn dump_of<T: PgComponent + Component>(world: &World) -> Vec<ComponentRow> {
    let mut out = Vec::new();
    <(Entity, &T)>::each_filtered_shared::<()>(world, |(e, c)| {
        out.push((e, c.to_params()));
    });
    out
}

fn remove_of<T: PgComponent + Component>(world: &mut World, entity: Entity) {
    world.remove::<T>(entity);
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
    /// World asal snapshot — `commit` menautkan store ke World ini.
    world_id: WorldId,
    /// Entity yang punya ≥1 komponen (pid dialokasikan saat commit), urut World.
    entities: Vec<Entity>,
    /// Baris komponen, sejajar urutan `registered` saat `stage`.
    components: Vec<Vec<ComponentRow>>,
    /// Rekam sinkron baru (untuk `save_incremental` berikutnya).
    next_state: HashMap<Entity, EntityState>,
}

/// Diff **owned** (sync) dari sebuah `World` vs rekam sinkron internal, siap
/// di-[`commit_incremental`](PgStore::commit_incremental) secara async — hasil
/// [`PgStore::stage_incremental`]. Tak memegang `&World`.
pub struct StagedIncremental {
    /// World asal diff — `commit_incremental` menautkan store ke World ini.
    world_id: WorldId,
    /// entity yang hilang (ada di rekam, tak ada di world) → DELETE; terurut.
    deletes: Vec<Entity>,
    /// entity baru/berubah → UPSERT: `(entity, params per-komponen)`; terurut
    /// (indeks, generation) agar urutan tulis & alokasi pid deterministik
    /// (STD-0005) dan dua writer konkuren mengunci baris dalam urutan sama.
    upserts: Vec<(Entity, EntityState)>,
    /// Rekam sinkron baru setelah commit.
    next_state: HashMap<Entity, EntityState>,
}

/// Komponen owned satu entity, siap di-`commit_insert` (RFC-0034). Tak memegang
/// `&World` → future commit `Send` tanpa `World: Sync`.
pub struct StagedInsert {
    rows: Vec<(usize, Vec<PgValue>)>, // (index registered, params)
}

impl StagedInsert {
    /// Baris per komponen: `(index registered, params)`.
    pub(crate) fn rows(&self) -> &[(usize, Vec<PgValue>)] {
        &self.rows
    }

    /// Params komponen `registered[ci]`, bila entity punya komponen itu.
    pub(crate) fn params_of(&self, ci: usize) -> Option<&[PgValue]> {
        self.rows
            .iter()
            .find(|(c, _)| *c == ci)
            .map(|(_, p)| p.as_slice())
    }
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
    /// Rekam keadaan sinkron terakhir (per `Entity`) untuk `save_incremental`.
    /// Handle stabil dalam satu sesi working-set (RFC-0034: handle ephemeral, `pid`
    /// persisten — jembatan `pid_of`/`entity_of` di bawah).
    last: HashMap<Entity, EntityState>,
    /// Jembatan **`Entity` → `pid`** (RFC-0034). Diisi saat load/materialize &
    /// save; dipakai oleh jalur tulis untuk menulis di bawah `pid` persisten.
    /// Kunci membawa generation: slot yang didaur-ulang `despawn`+`spawn` adalah
    /// entity **baru** (pid baru), bukan pewaris pid lama.
    pid_of: HashMap<Entity, i64>,
    /// Jembatan **`pid` → Entity** (handle lokal working-set).
    entity_of: HashMap<i64, Entity>,
    /// Entity yang dimuat **parsial** (`Query::only`) → tabel komponen yang
    /// dimuat untuknya. Jalur tulis per-entity (`update_entity`,
    /// `save_incremental`) melewati tabel di luar himpunan ini agar komponen
    /// yang tak dimuat tak terhapus. Entry dilepas saat entity dimuat penuh.
    partial: HashMap<Entity, HashSet<&'static str>>,
    /// World yang jembatan & rekam sinkron di atas merujuk (`None` sebelum
    /// operasi pertama). World lain yang datang → jembatan di-reset otomatis
    /// ([`Self::bind_world`]): handle `Entity` tak bermakna lintas-World.
    world_id: Option<WorldId>,
    /// Cache read-through opsional (RFC-0033); `None` → langsung Postgres.
    cache: Option<Arc<dyn ComponentCache>>,
}

impl PgStore {
    /// Menyambung ke Postgres pada `url` (pool koneksi).
    pub async fn connect(url: &str) -> Result<Self, sqlx::Error> {
        Self::connect_with(url, &crate::ConnectOptions::default()).await
    }

    /// Seperti [`connect`](Self::connect) dengan batas operasional eksplisit
    /// (ukuran pool/admission, `acquire_timeout`, `statement_timeout`,
    /// `lock_timeout`, …) — lihat [`crate::limits`]. Untuk service publik,
    /// setel minimal `acquire_timeout` pendek + `statement_timeout`.
    pub async fn connect_with(
        url: &str,
        opts: &crate::ConnectOptions,
    ) -> Result<Self, sqlx::Error> {
        Ok(Self::from_pool(opts.build_pool(url).await?))
    }

    /// Potret pool (koneksi terbuka/idle/maks) untuk health endpoint & metrik.
    pub fn pool_stats(&self) -> crate::PoolStats {
        crate::PoolStats::of(&self.pool)
    }

    /// Membangun dari `PgPool` yang sudah ada.
    pub fn from_pool(pool: PgPool) -> Self {
        Self {
            pool,
            registered: Vec::new(),
            last: HashMap::new(),
            pid_of: HashMap::new(),
            entity_of: HashMap::new(),
            partial: HashMap::new(),
            world_id: None,
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

    /// Store baru dengan **pool, registry, dan cache yang sama** tetapi jembatan
    /// pid↔entity dan rekam `save_incremental` **kosong** — untuk pola *World
    /// per-request*: simpan satu store template (sudah `register` + `migrate`)
    /// di state aplikasi, `fork()` di tiap handler, pakai bersama satu `World`
    /// sekali-pakai. Murah: `PgPool` adalah `Arc` di dalam; registry hanya
    /// fn-pointer.
    pub fn fork(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            registered: self.registered.clone(),
            last: HashMap::new(),
            pid_of: HashMap::new(),
            entity_of: HashMap::new(),
            partial: HashMap::new(),
            world_id: None,
            cache: self.cache.clone(),
        }
    }

    /// Menautkan store ke `world`: bila berbeda dari World yang terakhir dilayani,
    /// jembatan `Entity↔pid` dan rekam `save_incremental` di-reset — keduanya
    /// hanya bermakna untuk satu World. Dipanggil di awal tiap jalur muat/tulis
    /// yang menerima `&World`.
    fn bind_world(&mut self, world: &World) {
        self.bind_world_id(world.id());
    }

    /// Seperti [`Self::bind_world`] dari id (jalur `commit*`, yang tak memegang
    /// `&World`).
    fn bind_world_id(&mut self, id: WorldId) {
        if self.world_id != Some(id) {
            self.world_id = Some(id);
            self.pid_of.clear();
            self.entity_of.clear();
            self.partial.clear();
            self.last.clear();
        }
    }

    /// Apakah `world` adalah World yang jembatan store ini merujuk.
    fn is_bound_to(&self, world: &World) -> bool {
        self.world_id == Some(world.id())
    }

    /// `pid` persisten untuk `entity` di working-set ini — terisi setelah
    /// `load`/`load_pids`/`fetch`/`save*` memetakan entity tersebut. `None` bila
    /// entity belum pernah disinkronkan lewat store ini.
    pub fn pid_of(&self, entity: Entity) -> Option<i64> {
        self.pid_of.get(&entity).copied()
    }

    /// Kebalikan [`Self::pid_of`]: handle lokal untuk `pid`, bila termuat di
    /// working-set ini.
    pub fn entity_of(&self, pid: i64) -> Option<Entity> {
        self.entity_of.get(&pid).copied()
    }

    /// Mendaftarkan tipe komponen `T` untuk dipersist. Idempoten: tabel yang
    /// sudah terdaftar dilewati (registrasi ganda dulu menulis tiap baris dua
    /// kali → `duplicate key`).
    pub fn register<T: PgComponent + Component>(&mut self) -> &mut Self {
        if self.registered.iter().any(|r| r.table == T::TABLE) {
            return self;
        }
        self.registered.push(Registered {
            table: T::TABLE,
            cache_ns: format!("{}@{:016x}", T::TABLE, schema_fingerprint(T::COLUMNS)),
            columns: T::COLUMNS,
            indexes: T::INDEXES,
            composites: T::COMPOSITE_INDEXES,
            on_delete: T::ON_DELETE,
            fts: T::FTS,
            checks: T::CHECKS,
            dump: dump_of::<T>,
            dump_one: dump_one_of::<T>,
            apply: apply_of::<T>,
            remove: remove_of::<T>,
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
        let mut tx = self.begin().await?;
        let pid = self.commit_insert_in(&mut tx, staged).await?;
        tx.commit().await?;
        Ok(pid)
    }

    /// Seperti [`Self::commit_insert`] tetapi di dalam transaksi `tx` milik
    /// pemanggil (tanpa commit) — lihat [`crate::tx`].
    pub async fn commit_insert_in(
        &self,
        tx: &mut PgTx<'_>,
        staged: StagedInsert,
    ) -> Result<i64, sqlx::Error> {
        let conn = tx.conn();
        let pid: i64 =
            sqlx::query_scalar("INSERT INTO arke_entities (version) VALUES (0) RETURNING pid")
                .fetch_one(&mut *conn)
                .await?;
        for (ci, params) in &staged.rows {
            self.insert_row(conn, *ci, pid, params).await?;
        }
        Ok(pid)
    }

    /// Sisipkan satu baris komponen `registered[ci]` untuk `pid`. Per-op:
    /// `pid_of` kosong → relasi lintas-op menggantung → NULL (RFC-0034 Am.3);
    /// konsumen per-op memakai id-string, bukan `EntityRef`.
    pub(crate) async fn insert_row(
        &self,
        conn: &mut PgConnection,
        ci: usize,
        pid: i64,
        params: &[PgValue],
    ) -> Result<(), sqlx::Error> {
        let r = &self.registered[ci];
        let insert = insert_sql(r);
        let params = self.resolve_refs(params);
        let mut q = sqlx::query(&insert).bind(pid);
        for (v, col) in params.iter().zip(r.columns) {
            q = bind_value(q, col.ty, v);
        }
        q.execute(&mut *conn).await?;
        Ok(())
    }

    /// Muat komponen `pid` ke `world` sebagai entity lokal; kembalikan handle
    /// (atau `None` bila `pid` tak ada). Jalur batch yang sama dengan `load_ids`: jembatan
    /// `pid_of`/`entity_of` ikut terisi (sehingga `entity_version`/
    /// `update_entity`/`commit_update` bekerja sesudahnya), cache read-through
    /// dilayani, dan `pid` yang sudah termuat di World ini di-*refresh* di
    /// tempat, bukan digandakan. Rekam `save_incremental` tak disentuh.
    pub async fn fetch(
        &mut self,
        world: &mut World,
        pid: i64,
    ) -> Result<Option<Entity>, sqlx::Error> {
        let loaded = self.materialize(world, &[pid]).await?;
        Ok(loaded.first().map(|&(_, e)| e))
    }

    /// Seperti [`Self::fetch`] tetapi dibaca di transaksi `tx` (RFC-0042):
    /// melihat tulisan `tx` yang belum di-commit; cache read-through dilewati.
    pub async fn fetch_in(
        &mut self,
        tx: &mut PgTx<'_>,
        world: &mut World,
        pid: i64,
    ) -> Result<Option<Entity>, sqlx::Error> {
        let loaded = self
            .load_ids_only_on(world, &[pid], None, Some(tx.conn()))
            .await?;
        Ok(loaded.first().map(|&(_, e)| e))
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
        let mut tx = self.begin().await?;
        self.commit_update_in(&mut tx, pid, staged).await?;
        tx.commit().await?;
        self.invalidate_all_tables(&[pid]).await;
        Ok(())
    }

    /// Seperti [`Self::commit_update`] tetapi di dalam transaksi `tx` milik
    /// pemanggil (tanpa commit) — lihat [`crate::tx`].
    pub async fn commit_update_in(
        &self,
        tx: &mut PgTx<'_>,
        pid: i64,
        staged: StagedUpdate,
    ) -> Result<(), sqlx::Error> {
        let conn = tx.conn();
        sqlx::query("UPDATE arke_entities SET version = version + 1 WHERE pid = $1")
            .bind(pid)
            .execute(&mut *conn)
            .await?;
        for (ci, params) in &staged.rows {
            let r = &self.registered[*ci];
            sqlx::query(&format!(
                "DELETE FROM {} WHERE pid = $1",
                quote_ident(r.table)
            ))
            .bind(pid)
            .execute(&mut *conn)
            .await?;
            if let Some(params) = params {
                self.insert_row(conn, *ci, pid, params).await?;
            }
        }
        Ok(())
    }

    /// Hapus `pid` (cascade ke tabel komponen).
    pub async fn remove(&self, pid: i64) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM arke_entities WHERE pid = $1")
            .bind(pid)
            .execute(&self.pool)
            .await?;
        self.invalidate_all_tables(&[pid]).await;
        Ok(())
    }

    /// Seperti [`Self::remove`] tetapi di dalam transaksi `tx` milik pemanggil
    /// (tanpa commit) — lihat [`crate::tx`].
    pub async fn remove_in(&self, tx: &mut PgTx<'_>, pid: i64) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM arke_entities WHERE pid = $1")
            .bind(pid)
            .execute(tx.conn())
            .await?;
        Ok(())
    }

    /// Pool koneksi store ini (dipakai [`Self::begin`]).
    pub(crate) fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// **Upsert** entity `staged` berkunci komponen `T` (RFC-0038) — lihat
    /// [`crate::upsert`].
    pub fn upsert<T: PgComponent>(&self, staged: StagedInsert) -> crate::Upsert<'_, T> {
        crate::Upsert::new(self, staged)
    }

    /// Index `registered` untuk tabel komponen `table`.
    pub(crate) fn registered_index(&self, table: &str) -> Option<usize> {
        self.registered.iter().position(|r| r.table == table)
    }

    /// **UPDATE massal ber-filter** atas kolom komponen `T` (lihat
    /// [`crate::mutate`]): `.filter(..).set(T::col(), v).execute()`.
    pub fn update_where<T: PgComponent>(&self) -> crate::UpdateWhere<'_, T> {
        crate::UpdateWhere::new(self)
    }

    /// **DELETE massal ber-filter**: hapus **entity** (cascade seluruh
    /// komponennya) yang komponen `T`-nya memenuhi filter (lihat [`crate::mutate`]).
    pub fn delete_where<T: PgComponent>(&self) -> crate::DeleteWhere<'_, T> {
        crate::DeleteWhere::new(self)
    }

    /// Namespace cache (RFC-0033) untuk tabel komponen `table`:
    /// `table@fingerprint(kolom)` — kunci yang dipakai [`ComponentCache`] untuk
    /// tabel itu pada skema saat ini. `None` bila tabel tak terdaftar. Berguna
    /// untuk inspeksi/purge kunci di backend (mis. `arke:<namespace>:<pid>` di
    /// `arke-cache`).
    pub fn cache_namespace(&self, table: &str) -> Option<&str> {
        self.registered
            .iter()
            .find(|r| r.table == table)
            .map(|r| r.cache_ns.as_str())
    }

    /// Invalidate cache komponen bertabel `table` untuk `pids` (no-op tanpa
    /// cache / pids kosong / tabel tak terdaftar). Namespace cache = `cache_ns`
    /// tabel itu (ber-fingerprint skema).
    pub(crate) async fn invalidate_cache(&self, table: &str, pids: &[i64]) {
        if let Some(c) = &self.cache
            && !pids.is_empty()
            && let Some(r) = self.registered.iter().find(|r| r.table == table)
        {
            c.invalidate(&r.cache_ns, pids).await;
        }
    }

    /// Invalidate cache `pids` di **semua** tabel komponen terdaftar (entity
    /// hilang/berubah seluruhnya).
    pub(crate) async fn invalidate_all_tables(&self, pids: &[i64]) {
        if let Some(c) = &self.cache
            && !pids.is_empty()
        {
            for r in &self.registered {
                c.invalidate(&r.cache_ns, pids).await;
            }
        }
    }

    /// Muat entity yang cocok `predicate` (fragmen `WHERE` atas tabel `T`) ke
    /// `world`; kembalikan pasangan `(pid, Entity)`. `predicate` = SQL tepercaya.
    ///
    /// Memuat lewat [`Self::materialize`] — satu query per tabel komponen dengan
    /// `pid = ANY($1)` — bukan satu `fetch` per pid. Versi per-pid membuat biaya
    /// tumbuh sebagai `baris × (1 + jumlah komponen terdaftar)`: pada 32 komponen
    /// terdaftar, memuat 398 entity berarti 13.135 round-trip, dan itu terukur
    /// sebagai ~900 ms untuk satu request yang isinya cuma satu tabel. Jalur batch
    /// membuatnya tetap `1 + 1 + jumlah komponen` berapa pun jumlah barisnya.
    ///
    /// Dua konsekuensi yang disengaja, keduanya menyelaraskan jalur ini dengan
    /// `load_ids` yang sudah memakai `materialize`:
    /// - Jembatan `pid_of`/`entity_of` kini ikut terisi, sehingga kolom
    ///   `entity_ref` yang targetnya ikut ter-muat menerjemah ke `Ref` yang benar
    ///   alih-alih selalu menggantung — persis kontrak di `translate_refs`.
    /// - Cache read-through (RFC-0033) kini melayani jalur ini juga; `fetch`
    ///   melewatinya.
    ///
    /// `self.last` sengaja TIDAK disentuh: ini jalur baca, dan `fetch` dulu juga
    /// tidak menyentuhnya. Menyelaraskan rekam sinkron di sini akan mengubah arti
    /// `save_incremental` bagi pemanggil yang cuma membaca.
    pub async fn query_pids<T: PgComponent>(
        &mut self,
        world: &mut World,
        predicate: Option<&str>,
    ) -> Result<Vec<(i64, Entity)>, sqlx::Error> {
        let where_c = predicate.map(|p| format!(" WHERE {p}")).unwrap_or_default();
        let sql = format!(
            "SELECT pid FROM {}{} ORDER BY pid",
            quote_ident(T::TABLE),
            where_c
        );
        let pids: Vec<i64> = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(|r| r.try_get("pid"))
            .collect::<Result<_, _>>()?;
        self.materialize(world, &pids).await
    }

    /// Membuat tabel komponen bila belum ada, lalu menyelaraskan kolomnya dengan
    /// [`PgComponent::COLUMNS`] terkini (tambah yang hilang; usang → nullable).
    async fn reconcile_table(&self, r: &Registered) -> Result<(), sqlx::Error> {
        sqlx::query(&create_table_sql_from(r.table, r.columns))
            .execute(&self.pool)
            .await?;

        // FK `pid → arke_entities ON DELETE CASCADE` wajib ada: tanpanya `save`
        // (DELETE FROM arke_entities) meninggalkan baris komponen yatim. Hilang
        // bila `arke_entities` pernah di-`DROP … CASCADE` lalu dibuat ulang —
        // `CREATE TABLE IF NOT EXISTS` tak memasangnya kembali di tabel lama.
        // Baris yatim yang sudah telanjur ada (tak punya entity → tak pernah bisa
        // dimuat) dibuang dulu agar constraint bisa dipasang.
        sqlx::query(&format!(
            "DO $$ BEGIN \
               IF NOT EXISTS (SELECT 1 FROM pg_constraint \
                              WHERE conname = '{fk}' \
                                AND conrelid = '{qtable}'::regclass) THEN \
                 DELETE FROM {qtable} WHERE pid NOT IN (SELECT pid FROM arke_entities); \
                 ALTER TABLE {qtable} ADD CONSTRAINT {qfk} \
                   FOREIGN KEY (pid) REFERENCES arke_entities(pid) ON DELETE CASCADE; \
               END IF; \
             END $$;",
            fk = format!("{}_pid_fkey", r.table).replace('\'', "''"),
            qfk = quote_ident(&format!("{}_pid_fkey", r.table)),
            qtable = quote_ident(r.table).replace('\'', "''"),
        ))
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
                quote_ident(r.table),
                quote_ident(c.name),
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
                    quote_ident(r.table),
                    quote_ident(&name)
                ))
                .execute(&self.pool)
                .await?;
            }
        }

        // Indeks kustom (`#[pg(index)]`/`#[pg(unique)]`), idempoten. Kolom JSONB
        // non-unik → GIN (melayani `@>`/`contains`); GIN tak mendukung UNIQUE,
        // jadi `#[pg(unique)]` tetap btree. Indeks bernama sama yang metodenya
        // beda (btree sisa migrate lama di kolom JSONB) di-DROP lalu dibuat ulang.
        for idx in r.indexes {
            let name = format!("idx_{}_{}", r.table, idx.column);
            let is_jsonb = r
                .columns
                .iter()
                .any(|c| c.name == idx.column && c.ty == PgType::Jsonb);
            let method = if is_jsonb && !idx.unique {
                "gin"
            } else {
                "btree"
            };
            let existing: Option<String> = sqlx::query_scalar(
                "SELECT indexdef FROM pg_indexes \
                 WHERE schemaname = current_schema() AND tablename = $1 AND indexname = $2",
            )
            .bind(r.table)
            .bind(&name)
            .fetch_optional(&self.pool)
            .await?;
            if let Some(def) = existing {
                if def.contains(&format!("USING {method} ")) {
                    continue;
                }
                sqlx::query(&format!("DROP INDEX {}", quote_ident(&name)))
                    .execute(&self.pool)
                    .await?;
            }
            let unique = if idx.unique { "UNIQUE " } else { "" };
            sqlx::query(&format!(
                "CREATE {unique}INDEX {} ON {} USING {method} ({})",
                quote_ident(&name),
                quote_ident(r.table),
                quote_ident(idx.column)
            ))
            .execute(&self.pool)
            .await?;
        }

        // Indeks komposit (RFC-0037), nama content-addressed: yang tak lagi
        // diinginkan (awalan `cidx_<tabel>_` pada tabel ini) di-DROP, yang belum
        // ada dibuat. Baris yang melanggar UNIQUE baru membuat CREATE gagal keras.
        let prefix = composite_index_name(
            r.table,
            &CompositeIndexDef {
                columns: &[],
                unique: false,
            },
        );
        let prefix = &prefix[..prefix.len() - 16];
        let desired: Vec<(String, &CompositeIndexDef)> = r
            .composites
            .iter()
            .map(|d| (composite_index_name(r.table, d), d))
            .collect();
        let existing: Vec<String> = sqlx::query_scalar(
            "SELECT indexname::text FROM pg_indexes \
             WHERE schemaname = current_schema() AND tablename = $1 AND starts_with(indexname, $2)",
        )
        .bind(r.table)
        .bind(prefix)
        .fetch_all(&self.pool)
        .await?;
        for name in &existing {
            if !desired.iter().any(|(n, _)| n == name) {
                sqlx::query(&format!("DROP INDEX {}", quote_ident(name)))
                    .execute(&self.pool)
                    .await?;
            }
        }
        for (name, def) in &desired {
            if existing.contains(name) {
                continue;
            }
            let cols: Vec<_> = def.columns.iter().map(|c| quote_ident(c)).collect();
            let unique = if def.unique { "UNIQUE " } else { "" };
            sqlx::query(&format!(
                "CREATE {unique}INDEX {} ON {} ({})",
                quote_ident(name),
                quote_ident(r.table),
                cols.join(", ")
            ))
            .execute(&self.pool)
            .await?;
        }

        self.reconcile_on_delete(r).await?;

        // Indeks full-text (`#[pg(fts)]`): GIN atas ekspresi
        // `to_tsvector('<cfg>', col)` — persis ekspresi yang dipakai
        // `Field::search`/`order_by_rank`, sehingga planner mencocokkannya.
        // Idempoten; config berubah (indexdef tak memuat `'<cfg>'::regconfig`)
        // → DROP + buat ulang. Tanpa kolom tambahan → rekonsiliasi kolom tak
        // tersentuh.
        for f in r.fts {
            let name = format!("idx_{}_{}_fts", r.table, f.column);
            let existing: Option<String> = sqlx::query_scalar(
                "SELECT indexdef FROM pg_indexes \
                 WHERE schemaname = current_schema() AND tablename = $1 AND indexname = $2",
            )
            .bind(r.table)
            .bind(&name)
            .fetch_optional(&self.pool)
            .await?;
            if let Some(def) = existing {
                if def.contains("USING gin ") && def.contains(&format!("'{}'::regconfig", f.config))
                {
                    continue;
                }
                sqlx::query(&format!("DROP INDEX {}", quote_ident(&name)))
                    .execute(&self.pool)
                    .await?;
            }
            sqlx::query(&format!(
                "CREATE INDEX {} ON {} USING gin ({})",
                quote_ident(&name),
                quote_ident(r.table),
                tsvector_expr(f.config, &quote_ident(f.column)),
            ))
            .execute(&self.pool)
            .await?;
        }

        // Constraint `CHECK` (`#[pg(check = "…")]`) ber-nama **content-addressed**:
        // `chk_<tabel>_<fnv64(ekspresi)>`. Ekspresi yang berubah = nama baru →
        // dipasang; nama `chk_<tabel>_*` yang tak lagi diinginkan (ekspresi
        // dihapus/berubah) di-DROP. Definisi yang sama → tak ada DDL (stabil).
        // Baris yang melanggar CHECK baru membuat `ADD CONSTRAINT` gagal keras —
        // migrasi data adalah keputusan operator, bukan sesuatu yang dilewati diam.
        let desired: Vec<(String, &str)> = r
            .checks
            .iter()
            .map(|expr| (check_constraint_name(r.table, expr), *expr))
            .collect();
        let existing: Vec<String> = sqlx::query_scalar(
            "SELECT conname FROM pg_constraint \
             WHERE conrelid = $1::regclass AND contype = 'c' AND conname LIKE 'chk\\_%'",
        )
        .bind(quote_ident(r.table).as_ref())
        .fetch_all(&self.pool)
        .await?;
        for name in &existing {
            if !desired.iter().any(|(n, _)| n == name) {
                sqlx::query(&format!(
                    "ALTER TABLE {} DROP CONSTRAINT {}",
                    quote_ident(r.table),
                    quote_ident(name)
                ))
                .execute(&self.pool)
                .await?;
            }
        }
        for (name, expr) in &desired {
            if existing.contains(name) {
                continue;
            }
            sqlx::query(&format!(
                "ALTER TABLE {} ADD CONSTRAINT {} CHECK ({expr})",
                quote_ident(r.table),
                quote_ident(name)
            ))
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    /// Aksi hapus relasi (RFC-0039), nama content-addressed per jenis objek:
    ///
    /// - FK kolom → `arke_entities(pid)`: `set_null` = `ON DELETE SET NULL`;
    ///   `cascade`/`restrict` = `NO ACTION DEFERRABLE INITIALLY DEFERRED` (cek
    ///   saat commit, jadi overwrite penuh `save` yang menghapus lalu menulis
    ///   ulang semua entity tetap sah).
    /// - Indeks btree pada kolom (trigger & aksi FK mencari perujuk lewat kolom ini).
    /// - `cascade`: trigger `AFTER DELETE` baris pada `arke_entities` yang
    ///   menghapus **entity** perujuk. AFTER (bukan BEFORE) sehingga sasaran
    ///   pernyataan luar sudah terhapus saat trigger jalan — tak ada bentrok
    ///   "baris sudah diubah perintah ini", dan siklus berhenti sendiri.
    ///
    /// Objek usang (awalan milik tabel ini, tak lagi dideklarasikan) di-DROP.
    /// Baris lama yang melanggar FK baru membuat `migrate` gagal keras.
    async fn reconcile_on_delete(&self, r: &Registered) -> Result<(), sqlx::Error> {
        let qtable = quote_ident(r.table);
        let desired: Vec<(String, String, String, &OnDeleteDef)> = r
            .on_delete
            .iter()
            .map(|d| {
                let (fk, idx, trg) = on_delete_names(r.table, d);
                (fk, idx, trg, d)
            })
            .collect();

        // FK.
        let fk_prefix = format!("afk_{}_", truncate_ident(r.table, 41));
        let existing: Vec<String> = sqlx::query_scalar(
            "SELECT conname::text FROM pg_constraint \
             WHERE conrelid = $1::regclass AND contype = 'f' AND starts_with(conname, $2)",
        )
        .bind(qtable.as_ref())
        .bind(&fk_prefix)
        .fetch_all(&self.pool)
        .await?;
        for name in &existing {
            if !desired.iter().any(|(fk, ..)| fk == name) {
                sqlx::query(&format!(
                    "ALTER TABLE {qtable} DROP CONSTRAINT {}",
                    quote_ident(name)
                ))
                .execute(&self.pool)
                .await?;
            }
        }
        for (fk, _, _, d) in &desired {
            if existing.contains(fk) {
                continue;
            }
            let action = match d.action {
                OnDelete::SetNull => "ON DELETE SET NULL",
                OnDelete::Cascade | OnDelete::Restrict => "DEFERRABLE INITIALLY DEFERRED",
            };
            sqlx::query(&format!(
                "ALTER TABLE {qtable} ADD CONSTRAINT {} FOREIGN KEY ({}) \
                 REFERENCES arke_entities(pid) {action}",
                quote_ident(fk),
                quote_ident(d.column)
            ))
            .execute(&self.pool)
            .await?;
        }

        // Indeks kolom relasi.
        let idx_prefix = format!("aidx_{}_", truncate_ident(r.table, 40));
        let existing: Vec<String> = sqlx::query_scalar(
            "SELECT indexname::text FROM pg_indexes \
             WHERE schemaname = current_schema() AND tablename = $1 AND starts_with(indexname, $2)",
        )
        .bind(r.table)
        .bind(&idx_prefix)
        .fetch_all(&self.pool)
        .await?;
        for name in &existing {
            if !desired.iter().any(|(_, idx, ..)| idx == name) {
                sqlx::query(&format!("DROP INDEX {}", quote_ident(name)))
                    .execute(&self.pool)
                    .await?;
            }
        }
        for (_, idx, _, d) in &desired {
            if existing.contains(idx) {
                continue;
            }
            sqlx::query(&format!(
                "CREATE INDEX {} ON {qtable} ({})",
                quote_ident(idx),
                quote_ident(d.column)
            ))
            .execute(&self.pool)
            .await?;
        }

        // Trigger cascade di `arke_entities`.
        let trg_prefix = on_delete_trigger_prefix(r.table);
        let existing: Vec<String> = sqlx::query_scalar(
            "SELECT tgname::text FROM pg_trigger \
             WHERE tgrelid = 'arke_entities'::regclass AND starts_with(tgname, $1)",
        )
        .bind(&trg_prefix)
        .fetch_all(&self.pool)
        .await?;
        let cascades: Vec<_> = desired
            .iter()
            .filter(|(.., d)| d.action == OnDelete::Cascade)
            .collect();
        for name in &existing {
            if !cascades.iter().any(|(_, _, trg, _)| trg == name) {
                let q = quote_ident(name);
                sqlx::query(&format!("DROP TRIGGER {q} ON arke_entities"))
                    .execute(&self.pool)
                    .await?;
                sqlx::query(&format!("DROP FUNCTION IF EXISTS {q}()"))
                    .execute(&self.pool)
                    .await?;
            }
        }
        for (_, _, trg, d) in cascades {
            let q = quote_ident(trg);
            // SQL dinamis dijaga `to_regclass`: tabel perujuk yang di-DROP (komponen
            // dibuang) meninggalkan trigger ini, dan SQL statis akan membuat setiap
            // DELETE entity gagal "relation does not exist". Isi fungsi selalu
            // diganti agar fungsi dari versi lama ikut diperbarui.
            let lit = |x: &str| format!("'{}'", x.replace('\'', "''"));
            let delete = format!(
                "DELETE FROM arke_entities WHERE pid IN (SELECT pid FROM {qtable} WHERE {} = $1)",
                quote_ident(d.column)
            );
            sqlx::query(&format!(
                "CREATE OR REPLACE FUNCTION {q}() RETURNS trigger LANGUAGE plpgsql AS $arke$ \
                 BEGIN \
                   IF to_regclass({}) IS NOT NULL THEN \
                     EXECUTE {} USING OLD.pid; \
                   END IF; \
                   RETURN NULL; \
                 END $arke$",
                lit(&qtable),
                lit(&delete)
            ))
            .execute(&self.pool)
            .await?;
            if existing.contains(trg) {
                continue;
            }
            sqlx::query(&format!(
                "CREATE TRIGGER {q} AFTER DELETE ON arke_entities \
                 FOR EACH ROW EXECUTE FUNCTION {q}()"
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
        let mut entities: Vec<Entity> = Vec::new();
        <Entity>::each_filtered_shared::<()>(world, |e| entities.push(e));
        let components: Vec<Vec<ComponentRow>> =
            self.registered.iter().map(|r| (r.dump)(world)).collect();
        let next_state = self.dump_state(world);
        StagedSave {
            world_id: world.id(),
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
        self.bind_world_id(staged.world_id);
        let mut tx = self.pool.begin().await?;

        // Overwrite penuh: DELETE meng-cascade ke tabel komponen.
        sqlx::query("DELETE FROM arke_entities")
            .execute(&mut *tx)
            .await?;

        // Alokasi `pid` (BIGSERIAL) tiap entity — satu round-trip batch — ke
        // jembatan **lokal**; jembatan store baru diganti setelah commit sukses
        // (bila tx gagal, jembatan lama tetap konsisten dengan DB yang tak berubah).
        // Pid terurut menaik dipasangkan dengan entity urut World → deterministik.
        let pids = allocate_pids(&mut tx, staged.entities.len()).await?;
        let bridge: HashMap<Entity, i64> = staged
            .entities
            .iter()
            .copied()
            .zip(pids.iter().copied())
            .collect();

        // Komponen ditulis di bawah `pid`, batch per tabel (`UNNEST`). Semua pid
        // working-set kini teralokasi → resolusi `Ref(entity)`→`pid` valid
        // (RFC-0034 Am.3).
        for (r, rows) in self.registered.iter().zip(&staged.components) {
            let rows: Vec<(i64, Vec<PgValue>)> = rows
                .iter()
                .map(|(entity, params)| (bridge[entity], resolve_refs_with(&bridge, params)))
                .collect();
            batch_insert_rows(&mut tx, r, &rows).await?;
        }

        tx.commit().await?;
        // Overwrite penuh → kosongkan cache (RFC-0033).
        if let Some(c) = &self.cache {
            c.clear().await;
        }
        self.entity_of = bridge.iter().map(|(&e, &pid)| (pid, e)).collect();
        self.pid_of = bridge;
        // Overwrite penuh: DB kini = World, tak ada lagi entity parsial.
        self.partial.clear();
        // Selaraskan rekam sinkron dengan keadaan yang baru ditulis.
        self.last = staged.next_state;
        Ok(())
    }

    /// Simpan **seluruh** `world` (overwrite penuh) dalam satu transaksi —
    /// = [`stage`](Self::stage) (sinkron) + [`commit`](Self::commit) (async).
    ///
    /// Sengaja **bukan** `async fn`: parameter sebuah `async fn` hidup di state
    /// future sampai selesai, sehingga `&World` ikut tertangkap melewati `.await`
    /// dan future pemanggil menjadi `!Send` (`World` bukan `Sync`). Di sini
    /// `world` selesai dibaca sebelum kembali; future yang dikembalikan hanya
    /// memegang `&mut self` + data owned, dan dijamin `Send` oleh tanda tangan.
    pub fn save<'s>(
        &'s mut self,
        world: &World,
    ) -> impl std::future::Future<Output = Result<(), sqlx::Error>> + Send + 's {
        let staged = self.stage(world);
        self.commit(staged)
    }

    /// Memuat (materialize) **seluruh** keadaan dari Postgres ke `world`,
    /// merekonstruksi entity dengan **handle identik** (via [`World::spawn_at`]).
    /// Ditujukan untuk `World` kosong/segar. Deterministik (`ORDER BY entity_id`).
    ///
    /// Menyelaraskan rekam sinkron internal → `save_incremental` berikutnya hanya
    /// menulis perubahan setelah muat ini.
    pub async fn load(&mut self, world: &mut World) -> Result<(), sqlx::Error> {
        self.bind_world(world);
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
        self.bind_world(world);
        let sql = format!(
            "SELECT pid FROM {} WHERE {} ORDER BY pid",
            quote_ident(T::TABLE),
            predicate
        );
        let rows = sqlx::query(&sql).fetch_all(&self.pool).await?;
        let ids: Vec<i64> = rows
            .iter()
            .map(|r| r.try_get("pid"))
            .collect::<Result<_, _>>()?;
        let loaded = self.materialize(world, &ids).await?;
        self.last = self.dump_state(world);
        Ok(loaded.len())
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
        only: Option<&[&'static str]>,
    ) -> Result<Vec<(i64, Entity)>, sqlx::Error> {
        self.load_by_query_on(sql, params, world, only, None).await
    }

    /// [`Self::load_by_query`] pada koneksi transaksi `conn` bila ada.
    pub(crate) async fn load_by_query_on(
        &mut self,
        sql: String,
        params: Vec<(PgType, PgValue)>,
        world: &mut World,
        only: Option<&[&'static str]>,
        mut conn: Option<&mut PgConnection>,
    ) -> Result<Vec<(i64, Entity)>, sqlx::Error> {
        let ids: Vec<i64> = self
            .fetch_rows_on(&sql, &params, conn.as_deref_mut())
            .await?
            .iter()
            .map(|r| r.try_get("pid"))
            .collect::<Result<_, _>>()?;
        self.load_ids_only_on(world, &ids, only, conn).await
    }

    /// Jalankan `sql` ter-parameterisasi, kembalikan baris mentah.
    pub(crate) async fn fetch_rows(
        &self,
        sql: &str,
        params: &[(PgType, PgValue)],
    ) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error> {
        self.fetch_rows_on(sql, params, None).await
    }

    /// [`Self::fetch_rows`] pada koneksi transaksi `conn` bila ada (RFC-0042).
    pub(crate) async fn fetch_rows_on(
        &self,
        sql: &str,
        params: &[(PgType, PgValue)],
        conn: Option<&mut PgConnection>,
    ) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error> {
        let mut q = sqlx::query(sql);
        for (ty, val) in params {
            q = bind_value(q, *ty, val);
        }
        match conn {
            Some(c) => q.fetch_all(c).await,
            None => q.fetch_all(&self.pool).await,
        }
    }

    /// Materialisasi `ids` (hidrasi selektif `only`) + selaraskan rekam sinkron.
    /// Hasil **urut mengikuti `ids`** (urutan query), bukan `ORDER BY pid`.
    async fn load_ids_only_on(
        &mut self,
        world: &mut World,
        ids: &[i64],
        only: Option<&[&'static str]>,
        conn: Option<&mut PgConnection>,
    ) -> Result<Vec<(i64, Entity)>, sqlx::Error> {
        let loaded = self.materialize_on(world, ids, only, conn).await?;
        self.last = self.dump_state(world);
        let by_pid: HashMap<i64, Entity> = loaded.into_iter().collect();
        Ok(ids
            .iter()
            .filter_map(|pid| by_pid.get(pid).map(|&e| (*pid, e)))
            .collect())
    }

    /// Rekonstruksi entity `ids` + seluruh komponennya ke `world`.
    ///
    /// Mengembalikan pasangan `(pid, Entity)` — `pid` ikut dikembalikan supaya
    /// pemanggil tak perlu menebak ulang lewat jembatan `entity_of`, yang hidup
    /// lebih lama dari satu muat dan bisa memuat sisa World sebelumnya.
    /// Urutannya mengikuti `ORDER BY pid` pada tabel `arke_entities`, dan `pid`
    /// yang tak punya baris di sana memang tidak muncul.
    async fn materialize(
        &mut self,
        world: &mut World,
        ids: &[i64],
    ) -> Result<Vec<(i64, Entity)>, sqlx::Error> {
        self.materialize_only(world, ids, None).await
    }

    /// [`Self::materialize`] dengan **hidrasi selektif** (`Query::only`): bila
    /// `only = Some(tables)`, hanya tabel komponen tersebut yang dibaca; komponen
    /// lain tak disentuh di `world`. Entity yang **baru** di-spawn lewat jalur
    /// parsial dicatat di `partial`; muat penuh (`None`) melepas catatan itu.
    async fn materialize_only(
        &mut self,
        world: &mut World,
        ids: &[i64],
        only: Option<&[&'static str]>,
    ) -> Result<Vec<(i64, Entity)>, sqlx::Error> {
        self.materialize_on(world, ids, only, None).await
    }

    /// Inti materialisasi. Dengan `conn` (transaksi, RFC-0042) semua baca lewat
    /// koneksi itu dan **cache dilewati sepenuhnya**: baris yang belum di-commit
    /// tak boleh disajikan dari, maupun ditulis ke, cache bersama.
    async fn materialize_on(
        &mut self,
        world: &mut World,
        ids: &[i64],
        only: Option<&[&'static str]>,
        mut conn: Option<&mut PgConnection>,
    ) -> Result<Vec<(i64, Entity)>, sqlx::Error> {
        let use_cache = conn.is_none();
        self.bind_world(world);
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        // `ids` = `pid`s. `pid` yang sudah tertaut ke entity hidup di `world` ini
        // di-refresh di tempat (muat aditif, mis. beberapa `Query::load` ke satu
        // World per-request); selebihnya spawn entity lokal baru (indeks
        // ephemeral, RFC-0034) + catat jembatan Entity↔pid.
        let q =
            sqlx::query("SELECT pid FROM arke_entities WHERE pid = ANY($1) ORDER BY pid").bind(ids);
        let rows = match conn.as_deref_mut() {
            Some(c) => q.fetch_all(c).await?,
            None => q.fetch_all(&self.pool).await?,
        };
        let mut by_id: HashMap<i64, Entity> = HashMap::with_capacity(rows.len());
        let mut entities: Vec<(i64, Entity)> = Vec::with_capacity(rows.len());
        let mut refreshed: HashSet<Entity> = HashSet::new();
        for row in rows {
            let pid: i64 = row.try_get("pid")?;
            let entity = match self.entity_of.get(&pid) {
                Some(&e) if world.contains(e) => {
                    refreshed.insert(e);
                    e
                }
                _ => {
                    let e = world.spawn();
                    // Jaga bijeksi: tautan lama di salah satu sisi dilepas dari
                    // sisi lainnya hanya bila masih menunjuk balik ke sini.
                    if let Some(old_pid) = self.pid_of.insert(e, pid)
                        && old_pid != pid
                        && self.entity_of.get(&old_pid) == Some(&e)
                    {
                        self.entity_of.remove(&old_pid);
                    }
                    if let Some(old_e) = self.entity_of.insert(pid, e)
                        && old_e != e
                        && self.pid_of.get(&old_e) == Some(&pid)
                    {
                        self.pid_of.remove(&old_e);
                    }
                    e
                }
            };
            by_id.insert(pid, entity);
            entities.push((pid, entity));
        }

        // Catatan parsial: entity baru lewat `only` → parsial dengan himpunan
        // ini; yang sudah parsial → himpunan digabung; muat penuh → dilepas.
        // Entity yang sudah lengkap (refreshed, tanpa catatan) tetap lengkap —
        // komponen di luar `only` masih ada di World dari muat sebelumnya.
        match only {
            Some(tables) => {
                for &(_, entity) in &entities {
                    if refreshed.contains(&entity) {
                        if let Some(set) = self.partial.get_mut(&entity) {
                            set.extend(tables.iter().copied());
                        }
                    } else {
                        self.partial
                            .insert(entity, tables.iter().copied().collect());
                    }
                }
            }
            None => {
                for &(_, entity) in &entities {
                    self.partial.remove(&entity);
                }
            }
        }

        for r in &self.registered {
            if let Some(tables) = only
                && !tables.contains(&r.table)
            {
                continue;
            }
            // Read-through cache (RFC-0033): layani hit dari cache, ambil miss dari
            // Postgres lalu isi cache. Tanpa cache → jalur langsung.
            let cached = match &self.cache {
                Some(c) if use_cache => c.get_many(&r.cache_ns, ids).await,
                _ => vec![None; ids.len()],
            };
            // Entity refresh yang barisnya ditemukan; sisanya → komponen dilepas.
            let mut found: HashSet<i64> = HashSet::new();
            let mut miss_ids: Vec<i64> = Vec::new();
            for (i, &id) in ids.iter().enumerate() {
                match cached
                    .get(i)
                    .and_then(|b| b.as_deref())
                    .and_then(decode_row)
                {
                    Some(mut values) => {
                        // Cache menyimpan pid mentah → terjemahkan pid→Ref di sini
                        // (RFC-0034 Am.3), setelah decode, sebelum apply.
                        self.translate_refs(r, &mut values);
                        if let Some(&entity) = by_id.get(&id) {
                            (r.apply)(world, entity, &values);
                            found.insert(id);
                        }
                    }
                    None => miss_ids.push(id),
                }
            }
            if !miss_ids.is_empty() {
                let sql = select_sql(r, Some("pid = ANY($1)"));
                let q = sqlx::query(&sql).bind(&miss_ids);
                let rows = match conn.as_deref_mut() {
                    Some(c) => q.fetch_all(c).await?,
                    None => q.fetch_all(&self.pool).await?,
                };
                let mut to_cache: Vec<(i64, Vec<u8>)> = Vec::new();
                for row in rows {
                    let id: i64 = row.try_get("pid")?;
                    let mut values = Vec::with_capacity(r.columns.len());
                    for col in r.columns {
                        values.push(read_value(&row, col)?);
                    }
                    // Cache disimpan dengan pid mentah (sebelum terjemahan) agar
                    // valid lintas-World; terjemahkan pid→Ref hanya untuk apply
                    // (RFC-0034 Am.3).
                    if self.cache.is_some() && use_cache {
                        to_cache.push((id, encode_row(&values)));
                    }
                    self.translate_refs(r, &mut values);
                    if let Some(&entity) = by_id.get(&id) {
                        (r.apply)(world, entity, &values);
                        found.insert(id);
                    }
                }
                if let Some(c) = &self.cache
                    && !to_cache.is_empty()
                {
                    c.put_many(&r.cache_ns, &to_cache).await;
                }
            }
            // Refresh: komponen yang barisnya sudah tiada di DB dilepas dari
            // entity, agar World tak menyimpan keadaan basi.
            for &(pid, entity) in &entities {
                if refreshed.contains(&entity) && !found.contains(&pid) {
                    (r.remove)(world, entity);
                }
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
        Ok(entities.into_iter().map(|(_, e)| e).collect())
    }

    /// Versi optimistic-lock `entity` di DB, atau `None` bila entity tak ada.
    /// Identitas ke-`pid` diselesaikan via jembatan `pid_of` (RFC-0034); indeks
    /// World bersifat ephemeral sehingga identitas persisten = `pid`.
    pub async fn entity_version(&self, entity: Entity) -> Result<Option<i64>, sqlx::Error> {
        let Some(&pid) = self.pid_of.get(&entity) else {
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
    ///
    /// Bukan `async fn` (lihat [`save`](Self::save)): komponen dibaca dari
    /// `world` secara sinkron di sini, future yang dikembalikan tak memegang
    /// `&World` dan `Send`.
    pub fn update_entity<'s>(
        &'s self,
        world: &World,
        entity: Entity,
        expected_version: i64,
    ) -> impl std::future::Future<Output = Result<i64, UpdateError>> + Send + 's {
        // Identitas dari jembatan (`entity` = handle World yang ditautkan);
        // `world` hanya sumber nilai komponen — boleh World lain yang mereplay
        // handle yang sama lewat `spawn_at` (pola writer RFC-0021 §5).
        let pid = self.pid_of.get(&entity).copied();
        // Komponen entity ini dari keadaan `world` saat ini. Entity parsial
        // (`Query::only`): tabel yang tak dimuat **dan** tak diisi pemanggil di
        // World dilewati — nilainya di DB bukan milik working-set ini. Yang diisi
        // pemanggil tetap ditulis.
        let partial = self.partial.get(&entity);
        let rows: Vec<(usize, Option<Vec<PgValue>>)> = self
            .registered
            .iter()
            .enumerate()
            .filter_map(|(ci, r)| {
                let params = (r.dump_one)(world, entity);
                if params.is_none()
                    && let Some(set) = partial
                    && !set.contains(r.table)
                {
                    return None;
                }
                Some((ci, params))
            })
            .collect();
        self.update_entity_staged(pid, rows, expected_version)
    }

    /// Fase async [`update_entity`](Self::update_entity): gerbang versi + tulis
    /// ulang komponen yang sudah di-stage. `pid` `None` = entity tak ditautkan
    /// ke jembatan → konflik.
    async fn update_entity_staged(
        &self,
        pid: Option<i64>,
        rows: Vec<(usize, Option<Vec<PgValue>>)>,
        expected_version: i64,
    ) -> Result<i64, UpdateError> {
        let Some(pid) = pid else {
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

        for (ci, params) in rows {
            let r = &self.registered[ci];
            sqlx::query(&format!(
                "DELETE FROM {} WHERE pid = $1",
                quote_ident(r.table)
            ))
            .bind(pid)
            .execute(&mut *tx)
            .await
            .map_err(UpdateError::Db)?;
            if let Some(params) = params {
                let insert = insert_sql(r);
                let params = self.resolve_refs(&params);
                let mut q = sqlx::query(&insert).bind(pid);
                for (value, col) in params.iter().zip(r.columns) {
                    q = bind_value(q, col.ty, value);
                }
                q.execute(&mut *tx).await.map_err(UpdateError::Db)?;
            }
        }

        tx.commit().await.map_err(UpdateError::Db)?;
        // Invalidate cache untuk entity ini di tiap tabel (RFC-0033).
        self.invalidate_all_tables(&[pid]).await;
        Ok(new_version)
    }

    /// **Tulis (RFC-0034 Am.3):** ganti `PgValue::Ref(entity)` → `Int(pid)` via
    /// jembatan store. Lihat [`resolve_refs_with`].
    fn resolve_refs(&self, params: &[PgValue]) -> Vec<PgValue> {
        resolve_refs_with(&self.pid_of, params)
    }

    /// [`Self::resolve_refs`] untuk modul lain di crate ini.
    pub(crate) fn resolve_refs_pub(&self, params: &[PgValue]) -> Vec<PgValue> {
        self.resolve_refs(params)
    }

    /// **Baca (RFC-0034 Am.3):** untuk kolom `entity_ref`, `Int(pid)` → `Ref(indeks
    /// lokal)` via `entity_of`. Pid ada di DB tapi target **tak ikut ter-muat**
    /// (mis. `join` filter-saja) → `Ref(DANGLING_INDEX)`: relasi **tetap ada** (bukan
    /// NULL) tetapi handle tak me-resolve ke entity mana pun (bukan target yang salah).
    /// `NULL` DB (field `None`) tetap `Null`. Dipanggil sebelum `apply`, setelah
    /// `entity_of` terisi penuh untuk World ini.
    fn translate_refs(&self, r: &Registered, values: &mut [PgValue]) {
        for (v, col) in values.iter_mut().zip(r.columns) {
            if col.entity_ref {
                *v = match v {
                    PgValue::Int(pid) => match self.entity_of.get(pid) {
                        Some(&e) => PgValue::Ref(pack_entity(e)),
                        None => PgValue::Ref(DANGLING_INDEX),
                    },
                    _ => PgValue::Null,
                };
            }
        }
    }

    /// Kumpulkan keadaan seluruh entity + komponen `world` (untuk diff).
    fn dump_state(&self, world: &World) -> HashMap<Entity, EntityState> {
        let n = self.registered.len();
        let mut current: HashMap<Entity, EntityState> = HashMap::new();
        <Entity>::each_filtered_shared::<()>(world, |e| {
            current.insert(e, vec![None; n]);
        });
        for (ci, r) in self.registered.iter().enumerate() {
            for (e, params) in (r.dump)(world) {
                if let Some(state) = current.get_mut(&e) {
                    state[ci] = Some(params);
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
    ///
    /// Bukan `async fn` dengan alasan yang sama seperti [`save`](Self::save):
    /// future yang dikembalikan tidak memegang `&World`, dan `Send`.
    pub fn save_incremental<'s>(
        &'s mut self,
        world: &World,
    ) -> impl std::future::Future<Output = Result<SyncStats, sqlx::Error>> + Send + 's {
        let staged = self.stage_incremental(world);
        self.commit_incremental(staged)
    }

    /// **Fase 1 (sync)** incremental dua-fase: diff `world` vs rekam sinkron internal
    /// → [`StagedIncremental`] owned (tanpa await, tak menahan `&World`). Membuat
    /// `commit_incremental` `Send` tanpa `World: Sync` (ramah handler async).
    pub fn stage_incremental(&self, world: &World) -> StagedIncremental {
        let current = self.dump_state(world);
        // World lain dari yang terakhir dilayani → rekam sinkron tak berlaku:
        // diff terhadap rekam kosong (semua entity = baru), dan `commit_incremental`
        // me-reset jembatan lewat `world_id`.
        let empty = HashMap::new();
        let last = if self.is_bound_to(world) {
            &self.last
        } else {
            &empty
        };
        let mut deletes: Vec<Entity> = last
            .keys()
            .copied()
            .filter(|e| !current.contains_key(e))
            .collect();
        let mut upserts: Vec<(Entity, EntityState)> = current
            .iter()
            .filter(|&(e, state)| last.get(e) != Some(state))
            .map(|(e, state)| (*e, state.clone()))
            .collect();
        // Urutan deterministik (STD-0005): `HashMap` beriterasi acak.
        deletes.sort_unstable_by_key(|e| (e.index(), e.generation()));
        upserts.sort_unstable_by_key(|(e, _)| (e.index(), e.generation()));
        StagedIncremental {
            world_id: world.id(),
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
        self.bind_world_id(staged.world_id);
        let mut tx = self.pool.begin().await?;
        let mut stats = SyncStats {
            written: 0,
            deleted: 0,
        };
        // Entity yang tersentuh (dihapus/berubah) → invalidate cache (RFC-0033).
        // Kunci diff internal = `Entity` (ephemeral, stabil dalam-sesi); identitas
        // DB = `pid` diselesaikan via jembatan (RFC-0034). Jembatan dimutasi pada
        // salinan **lokal** dan baru dipromosikan setelah commit sukses — tx yang
        // gagal tak meninggalkan pid hantu (hasil INSERT yang di-rollback).
        let mut affected: Vec<i64> = Vec::new();
        let mut bridge = self.pid_of.clone();

        // Hilang → DELETE batch (cascade ke tabel komponen).
        let deleted: Vec<i64> = staged
            .deletes
            .iter()
            .filter_map(|e| bridge.remove(e))
            .collect();
        if !deleted.is_empty() {
            sqlx::query("DELETE FROM arke_entities WHERE pid = ANY($1)")
                .bind(&deleted)
                .execute(&mut *tx)
                .await?;
            stats.deleted = deleted.len();
            affected.extend_from_slice(&deleted);
        }

        // Pass 1: alokasi `pid` (satu batch) untuk semua entity baru **sebelum**
        // baris komponen ditulis, sehingga relasi ke entity baru se-batch
        // me-resolve (bukan menggantung NULL). Entity terurut → pid terurut.
        let fresh: Vec<Entity> = staged
            .upserts
            .iter()
            .map(|(e, _)| *e)
            .filter(|e| !bridge.contains_key(e))
            .collect();
        let existing: Vec<i64> = staged
            .upserts
            .iter()
            .filter_map(|(e, _)| bridge.get(e).copied())
            .collect();
        let new_pids = allocate_pids(&mut tx, fresh.len()).await?;
        bridge.extend(fresh.iter().copied().zip(new_pids.iter().copied()));

        // Pass 2: versi naik (batch) untuk yang sudah ada; ganti baris komponen
        // per tabel: DELETE semua pid terdampak (batch) lalu INSERT batch.
        if !existing.is_empty() {
            sqlx::query("UPDATE arke_entities SET version = version + 1 WHERE pid = ANY($1)")
                .bind(&existing)
                .execute(&mut *tx)
                .await?;
        }
        let upsert_pids: Vec<i64> = staged.upserts.iter().map(|(e, _)| bridge[e]).collect();
        for (ci, r) in self.registered.iter().enumerate() {
            // Entity parsial (`Query::only`) yang tak memuat tabel ini **dan**
            // tak punya nilainya di World dilewati di tabel ini: barisnya di DB
            // bukan milik working-set → tak di-DELETE, tak ditulis ulang. Nilai
            // yang diisi pemanggil tetap ditulis.
            let owns = |e: &Entity, state: &EntityState| {
                state[ci].is_some() || self.partial.get(e).is_none_or(|set| set.contains(r.table))
            };
            let table_pids: Vec<i64> = staged
                .upserts
                .iter()
                .filter(|(e, state)| owns(e, state))
                .map(|(e, _)| bridge[e])
                .collect();
            if table_pids.is_empty() {
                continue;
            }
            sqlx::query(&format!(
                "DELETE FROM {} WHERE pid = ANY($1)",
                quote_ident(r.table)
            ))
            .bind(&table_pids)
            .execute(&mut *tx)
            .await?;
            let rows: Vec<(i64, Vec<PgValue>)> = staged
                .upserts
                .iter()
                .filter(|(e, state)| owns(e, state))
                .filter_map(|(e, state)| {
                    state[ci]
                        .as_ref()
                        .map(|params| (bridge[e], resolve_refs_with(&bridge, params)))
                })
                .collect();
            batch_insert_rows(&mut tx, r, &rows).await?;
        }
        affected.extend_from_slice(&upsert_pids);
        stats.written = staged.upserts.len();

        tx.commit().await?;
        self.invalidate_all_tables(&affected).await;
        self.entity_of = bridge.iter().map(|(&e, &pid)| (pid, e)).collect();
        self.pid_of = bridge;
        for e in &staged.deletes {
            self.partial.remove(e);
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

/// **Tulis (RFC-0034 Am.3):** ganti `PgValue::Ref(entity)` → `Int(pid)` via
/// `bridge`. Ref menggantung (entity tak ter-map, mis. relasi lintas-op atau ke
/// entity yang sudah di-despawn) → `Null`. Nilai lain apa adanya. Panggil
/// **setelah** semua pid working-set teralokasi.
fn resolve_refs_with(bridge: &HashMap<Entity, i64>, params: &[PgValue]) -> Vec<PgValue> {
    params
        .iter()
        .map(|v| match v {
            PgValue::Ref(packed) => match bridge.get(&unpack_entity(*packed)) {
                Some(pid) => PgValue::Int(*pid),
                None => PgValue::Null,
            },
            other => other.clone(),
        })
        .collect()
}

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
        PgType::Uuid => "'00000000-0000-0000-0000-000000000000'::uuid",
        PgType::TimestampTz => "'epoch'::timestamptz",
    }
}

/// Cast eksplisit placeholder INSERT untuk tipe yang di-bind sebagai teks.
fn insert_cast(ty: PgType) -> &'static str {
    ty.bind_cast()
}

/// `INSERT INTO cmp_x (pid, c1, c2::jsonb, …) VALUES ($1, $2, $3::jsonb, …)`.
fn insert_sql(r: &Registered) -> String {
    let mut cols = String::from("pid");
    let mut placeholders = String::from("$1");
    for (i, col) in r.columns.iter().enumerate() {
        cols.push_str(", ");
        cols.push_str(&quote_ident(col.name));
        placeholders.push_str(&format!(", ${}{}", i + 2, insert_cast(col.ty)));
    }
    format!(
        "INSERT INTO {} ({}) VALUES ({})",
        quote_ident(r.table),
        cols,
        placeholders
    )
}

/// `SELECT pid, c1, c2::text AS c2, … FROM cmp_x [WHERE <filter>] ORDER BY pid`.
///
/// Kolom `JSONB`/`NUMERIC`/`UUID`/`TIMESTAMPTZ` dibaca sebagai teks
/// ([`PgType::read_expr`]) — tanpa dependensi serde/bigdecimal/uuid/chrono.
fn select_sql(r: &Registered, filter: Option<&str>) -> String {
    let mut cols = String::from("pid");
    for col in r.columns {
        cols.push_str(", ");
        let name = quote_ident(col.name);
        match col.ty.read_expr(&name) {
            Some(expr) => cols.push_str(&format!("{expr} AS {name}")),
            None => cols.push_str(&name),
        }
    }
    let where_clause = match filter {
        Some(f) => format!(" WHERE {f}"),
        None => String::new(),
    };
    format!(
        "SELECT {} FROM {}{} ORDER BY pid",
        cols,
        quote_ident(r.table),
        where_clause
    )
}

/// Nilai satu kolom dalam tipe Rust **tetap per tipe kolom** — `Integer` selalu
/// `Option<i32>`, `Real` selalu `Option<f32>`, dst., baik `NULL` maupun tidak.
/// Penting: sqlx meng-cache prepared statement per teks SQL dengan tipe
/// parameter dari eksekusi **pertama**; bila `NULL` di-bind sebagai `Option<i32>`
/// lalu nilai berikutnya sebagai `i64`, Postgres menolak (`22P03 incorrect
/// binary data format`). Dipakai `bind_value` (per-baris) dan jalur batch
/// (`UNNEST` per-kolom).
enum Typed {
    I32(Option<i32>),
    I64(Option<i64>),
    F32(Option<f32>),
    F64(Option<f64>),
    Bool(Option<bool>),
    /// TEXT / JSONB / NUMERIC / UUID / TIMESTAMPTZ (selain TEXT di-cast di SQL).
    Text(Option<String>),
}

fn typed(col_ty: PgType, value: &PgValue) -> Typed {
    match col_ty {
        PgType::Integer => Typed::I32(match value {
            PgValue::Int(i) | PgValue::Ref(i) => i32::try_from(*i).ok(),
            _ => None,
        }),
        PgType::BigInt => Typed::I64(match value {
            PgValue::Int(i) | PgValue::Ref(i) => Some(*i),
            _ => None,
        }),
        PgType::Real => Typed::F32(match value {
            PgValue::Float(f) => Some(*f as f32),
            _ => None,
        }),
        PgType::DoublePrecision => Typed::F64(match value {
            PgValue::Float(f) => Some(*f),
            _ => None,
        }),
        PgType::Boolean => Typed::Bool(match value {
            PgValue::Bool(b) => Some(*b),
            _ => None,
        }),
        PgType::Text | PgType::Jsonb | PgType::Numeric | PgType::Uuid | PgType::TimestampTz => {
            Typed::Text(match value {
                PgValue::Text(s) | PgValue::Json(s) | PgValue::Numeric(s) => Some(s.clone()),
                _ => None,
            })
        }
    }
}

/// Bind satu [`PgValue`] ke query dengan tipe Rust tetap per tipe kolom
/// (lihat [`Typed`]).
pub(crate) fn bind_value<'q>(
    q: Query<'q, Postgres, PgArguments>,
    col_ty: PgType,
    value: &PgValue,
) -> Query<'q, Postgres, PgArguments> {
    match typed(col_ty, value) {
        Typed::I32(v) => q.bind(v),
        Typed::I64(v) => q.bind(v),
        Typed::F32(v) => q.bind(v),
        Typed::F64(v) => q.bind(v),
        Typed::Bool(v) => q.bind(v),
        Typed::Text(v) => q.bind(v),
    }
}

/// Kolom-kolom sebuah batch baris sebagai array per-kolom (untuk `UNNEST`).
enum TypedVec {
    I32(Vec<Option<i32>>),
    I64(Vec<Option<i64>>),
    F32(Vec<Option<f32>>),
    F64(Vec<Option<f64>>),
    Bool(Vec<Option<bool>>),
    Text(Vec<Option<String>>),
}

impl TypedVec {
    fn new(col_ty: PgType) -> Self {
        match col_ty {
            PgType::Integer => TypedVec::I32(Vec::new()),
            PgType::BigInt => TypedVec::I64(Vec::new()),
            PgType::Real => TypedVec::F32(Vec::new()),
            PgType::DoublePrecision => TypedVec::F64(Vec::new()),
            PgType::Boolean => TypedVec::Bool(Vec::new()),
            PgType::Text | PgType::Jsonb | PgType::Numeric | PgType::Uuid | PgType::TimestampTz => {
                TypedVec::Text(Vec::new())
            }
        }
    }

    fn push(&mut self, col_ty: PgType, value: &PgValue) {
        match (self, typed(col_ty, value)) {
            (TypedVec::I32(v), Typed::I32(x)) => v.push(x),
            (TypedVec::I64(v), Typed::I64(x)) => v.push(x),
            (TypedVec::F32(v), Typed::F32(x)) => v.push(x),
            (TypedVec::F64(v), Typed::F64(x)) => v.push(x),
            (TypedVec::Bool(v), Typed::Bool(x)) => v.push(x),
            (TypedVec::Text(v), Typed::Text(x)) => v.push(x),
            _ => unreachable!("TypedVec dibuat dari tipe kolom yang sama"),
        }
    }

    /// Tipe array SQL untuk placeholder `UNNEST($n::<tipe>[])`.
    fn array_sql(col_ty: PgType) -> &'static str {
        match col_ty {
            PgType::Integer => "int4[]",
            PgType::BigInt => "int8[]",
            PgType::Real => "float4[]",
            PgType::DoublePrecision => "float8[]",
            PgType::Boolean => "bool[]",
            PgType::Text | PgType::Jsonb | PgType::Numeric | PgType::Uuid | PgType::TimestampTz => {
                "text[]"
            }
        }
    }

    fn bind<'q>(self, q: Query<'q, Postgres, PgArguments>) -> Query<'q, Postgres, PgArguments> {
        match self {
            TypedVec::I32(v) => q.bind(v),
            TypedVec::I64(v) => q.bind(v),
            TypedVec::F32(v) => q.bind(v),
            TypedVec::F64(v) => q.bind(v),
            TypedVec::Bool(v) => q.bind(v),
            TypedVec::Text(v) => q.bind(v),
        }
    }
}

/// Batas baris per pernyataan batch (`UNNEST`) — membatasi ukuran satu pesan
/// bind & memori array di server.
const BATCH_ROWS: usize = 2_000;

/// `INSERT INTO cmp_x (pid, c1, c2, …) SELECT u.pid, u.c1, u.c2::jsonb, … FROM
/// UNNEST($1::int8[], $2::…[], …) AS u(pid, c1, c2, …)` — satu round-trip per
/// ≤ `BATCH_ROWS` baris, bukan per baris.
fn batch_insert_sql(r: &Registered) -> String {
    let mut cols = String::from("pid");
    let mut selects = String::from("u.pid");
    let mut arrays = String::from("$1::int8[]");
    let mut aliases = String::from("pid");
    for (i, col) in r.columns.iter().enumerate() {
        let name = quote_ident(col.name);
        cols.push_str(", ");
        cols.push_str(&name);
        // Alias kolom UNNEST memakai nama posisi (`c1`, …) — bebas kata kunci.
        selects.push_str(&format!(", u.c{i}{}", insert_cast(col.ty)));
        arrays.push_str(&format!(", ${}::{}", i + 2, TypedVec::array_sql(col.ty)));
        aliases.push_str(&format!(", c{i}"));
    }
    format!(
        "INSERT INTO {} ({cols}) SELECT {selects} FROM UNNEST({arrays}) AS u({aliases})",
        quote_ident(r.table)
    )
}

/// Sisipkan `rows` (`(pid, params)`) ke tabel `r` secara batch di `conn`.
async fn batch_insert_rows(
    conn: &mut PgConnection,
    r: &Registered,
    rows: &[(i64, Vec<PgValue>)],
) -> Result<(), sqlx::Error> {
    if rows.is_empty() {
        return Ok(());
    }
    let sql = batch_insert_sql(r);
    for chunk in rows.chunks(BATCH_ROWS) {
        let pids: Vec<Option<i64>> = chunk.iter().map(|(pid, _)| Some(*pid)).collect();
        let mut cols: Vec<TypedVec> = r.columns.iter().map(|c| TypedVec::new(c.ty)).collect();
        for (_, params) in chunk {
            for ((col, def), value) in cols.iter_mut().zip(r.columns).zip(params) {
                col.push(def.ty, value);
            }
        }
        let mut q = sqlx::query(&sql).bind(pids);
        for col in cols {
            q = col.bind(q);
        }
        q.execute(&mut *conn).await?;
    }
    Ok(())
}

/// Alokasi `n` pid baru di `arke_entities` (satu round-trip), terurut menaik.
async fn allocate_pids(conn: &mut PgConnection, n: usize) -> Result<Vec<i64>, sqlx::Error> {
    if n == 0 {
        return Ok(Vec::new());
    }
    let mut pids: Vec<i64> = sqlx::query_scalar(
        "INSERT INTO arke_entities (version) SELECT 0 FROM generate_series(1, $1) RETURNING pid",
    )
    .bind(n as i64)
    .fetch_all(&mut *conn)
    .await?;
    // Urutan RETURNING tak dijamin spesifikasi → urutkan agar penetapan pid ke
    // entity deterministik (STD-0005).
    pids.sort_unstable();
    debug_assert_eq!(pids.len(), n);
    Ok(pids)
}

/// Baca satu kolom baris menjadi [`PgValue`] sesuai tipenya (`NULL` → `Null`).
fn read_value(row: &sqlx::postgres::PgRow, col: &ColumnDef) -> Result<PgValue, sqlx::Error> {
    read_typed(row, col.name, col.ty)
}

/// Baca kolom `name` bertipe `ty` dari `row` sebagai [`PgValue`] (`NULL` → `Null`).
/// JSONB/NUMERIC/UUID/TIMESTAMPTZ diharapkan sudah dibaca sebagai teks oleh SQL
/// pemanggil ([`PgType::read_expr`]).
pub(crate) fn read_typed(
    row: &sqlx::postgres::PgRow,
    name: &str,
    ty: PgType,
) -> Result<PgValue, sqlx::Error> {
    Ok(match ty {
        PgType::Integer => match row.try_get::<Option<i32>, _>(name)? {
            Some(v) => PgValue::Int(i64::from(v)),
            None => PgValue::Null,
        },
        PgType::BigInt => match row.try_get::<Option<i64>, _>(name)? {
            Some(v) => PgValue::Int(v),
            None => PgValue::Null,
        },
        PgType::Real => match row.try_get::<Option<f32>, _>(name)? {
            Some(v) => PgValue::Float(f64::from(v)),
            None => PgValue::Null,
        },
        PgType::DoublePrecision => match row.try_get::<Option<f64>, _>(name)? {
            Some(v) => PgValue::Float(v),
            None => PgValue::Null,
        },
        PgType::Boolean => match row.try_get::<Option<bool>, _>(name)? {
            Some(v) => PgValue::Bool(v),
            None => PgValue::Null,
        },
        PgType::Text => match row.try_get::<Option<String>, _>(name)? {
            Some(v) => PgValue::Text(v),
            None => PgValue::Null,
        },
        // Dibaca lewat `col::text` (lihat `select_sql`).
        PgType::Jsonb => match row.try_get::<Option<String>, _>(name)? {
            Some(v) => PgValue::Json(v),
            None => PgValue::Null,
        },
        PgType::Numeric => match row.try_get::<Option<String>, _>(name)? {
            Some(v) => PgValue::Numeric(v),
            None => PgValue::Null,
        },
        PgType::Uuid | PgType::TimestampTz => match row.try_get::<Option<String>, _>(name)? {
            Some(v) => PgValue::Text(v),
            None => PgValue::Null,
        },
    })
}
