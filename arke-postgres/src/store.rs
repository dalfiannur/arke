//! [`PgStore`]: persistensi async `World` ↔ Postgres (RFC-0021 §4).
//!
//! `connect`/`register`/`migrate`/`save`/`load` penuh + `update_entity`
//! (optimistic-lock) + `save_incremental` (diff), semua transaksional.
//! `World` adalah *working set*; Postgres sumber kebenaran. Determinisme muat
//! dijaga dengan `ORDER BY entity_id` (STD-0005). Tipe kolom yang didukung:
//! INTEGER/BIGINT/NUMERIC (u64/usize)/REAL/DOUBLE PRECISION/BOOLEAN/TEXT +
//! `Option<T>` nullable + `JSONB` (field non-skalar via `arke::Serialize`).

use std::collections::HashMap;

use arke::{Component, Entity, QueryData, World};
use sqlx::{PgPool, Postgres, Row, postgres::PgArguments, postgres::PgPoolOptions, query::Query};

use crate::{ColumnDef, PgComponent, PgType, PgValue, create_table_sql_from};

/// Satu baris komponen yang di-dump: `(entity_id, nilai-kolom)`.
type ComponentRow = (i64, Vec<PgValue>);

/// Keadaan tersimpan satu entity untuk diff inkremental: `generation` + nilai
/// tiap komponen terdaftar (`None` bila entity tak punya komponen itu).
type EntityState = (i64, Vec<Option<Vec<PgValue>>>);

/// Operasi type-erased untuk satu tipe komponen terdaftar.
struct Registered {
    table: &'static str,
    columns: &'static [ColumnDef],
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

/// Penyimpan Postgres untuk keadaan ECS (RFC-0021).
///
/// Daftarkan tiap tipe komponen via [`Self::register`], `migrate`, lalu
/// `save`/`load`. Hanya komponen `#[derive(PgComponent)]` yang dipersist.
pub struct PgStore {
    pool: PgPool,
    registered: Vec<Registered>,
    /// Rekam keadaan sinkron terakhir (per entity) untuk `save_incremental`.
    last: HashMap<i64, EntityState>,
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
        }
    }

    /// Mendaftarkan tipe komponen `T` untuk dipersist.
    pub fn register<T: PgComponent + Component>(&mut self) -> &mut Self {
        self.registered.push(Registered {
            table: T::TABLE,
            columns: T::COLUMNS,
            dump: dump_of::<T>,
            dump_one: dump_one_of::<T>,
            apply: apply_of::<T>,
        });
        self
    }

    /// Membuat tabel `arke_entities` + satu tabel per komponen terdaftar
    /// (idempoten via `IF NOT EXISTS`).
    pub async fn migrate(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS arke_entities \
             (entity_id BIGINT PRIMARY KEY, generation BIGINT NOT NULL, \
              version BIGINT NOT NULL DEFAULT 0)",
        )
        .execute(&self.pool)
        .await?;
        // Tabel lama tanpa kolom `version` → tambahkan (optimistic-lock).
        sqlx::query(
            "ALTER TABLE arke_entities ADD COLUMN IF NOT EXISTS version BIGINT NOT NULL DEFAULT 0",
        )
        .execute(&self.pool)
        .await?;
        for r in &self.registered {
            sqlx::query(&create_table_sql_from(r.table, r.columns))
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
    pub async fn save(&mut self, world: &World) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        // Overwrite penuh: DELETE meng-cascade ke tabel komponen.
        sqlx::query("DELETE FROM arke_entities")
            .execute(&mut *tx)
            .await?;

        // Entity (yang punya ≥1 komponen).
        let mut entities: Vec<Entity> = Vec::new();
        <Entity>::each_filtered_shared::<()>(world, |e| entities.push(e));
        for e in &entities {
            // Overwrite penuh mereset versi ke 0 (baseline single-writer).
            sqlx::query(
                "INSERT INTO arke_entities (entity_id, generation, version) VALUES ($1, $2, 0)",
            )
            .bind(i64::from(e.index()))
            .bind(i64::from(e.generation()))
            .execute(&mut *tx)
            .await?;
        }

        // Komponen (referensi FK ke arke_entities).
        for r in &self.registered {
            let insert = insert_sql(r);
            for (entity_id, params) in (r.dump)(world) {
                let mut q = sqlx::query(&insert).bind(entity_id);
                for (value, col) in params.iter().zip(r.columns) {
                    q = bind_value(q, col.ty, value);
                }
                q.execute(&mut *tx).await?;
            }
        }

        tx.commit().await?;
        // Selaraskan rekam sinkron dengan keadaan yang baru ditulis.
        self.last = self.dump_state(world);
        Ok(())
    }

    /// Memuat (materialize) keadaan dari Postgres ke `world`, merekonstruksi
    /// entity dengan **handle identik** (via [`World::spawn_at`]). Ditujukan
    /// untuk `World` kosong/segar. Deterministik (`ORDER BY entity_id`).
    pub async fn load(&self, world: &mut World) -> Result<(), sqlx::Error> {
        // Entity dulu (parent FK), urutan deterministik.
        let rows =
            sqlx::query("SELECT entity_id, generation FROM arke_entities ORDER BY entity_id")
                .fetch_all(&self.pool)
                .await?;
        let mut by_id: HashMap<i64, Entity> = HashMap::with_capacity(rows.len());
        for row in rows {
            let id: i64 = row.try_get("entity_id")?;
            let generation: i64 = row.try_get("generation")?;
            let entity = world.spawn_at(id as u32, generation as u32);
            by_id.insert(id, entity);
        }

        // Komponen.
        for r in &self.registered {
            let rows = sqlx::query(&select_sql(r)).fetch_all(&self.pool).await?;
            for row in rows {
                let id: i64 = row.try_get("entity_id")?;
                let Some(&entity) = by_id.get(&id) else {
                    continue;
                };
                let mut values = Vec::with_capacity(r.columns.len());
                for col in r.columns {
                    values.push(read_value(&row, col)?);
                }
                (r.apply)(world, entity, &values);
            }
        }
        Ok(())
    }

    /// Versi optimistic-lock `entity` di DB, atau `None` bila entity tak ada
    /// (identitas dicek lewat `generation`, STD-0007).
    pub async fn entity_version(&self, entity: Entity) -> Result<Option<i64>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT version FROM arke_entities WHERE entity_id = $1 AND generation = $2",
        )
        .bind(i64::from(entity.index()))
        .bind(i64::from(entity.generation()))
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(row) => Ok(Some(row.try_get("version")?)),
            None => Ok(None),
        }
    }

    /// **Tulis-balik ber-optimistic-lock**: memperbarui baris `entity` (versi
    /// naik) beserta komponennya dari `world`, **hanya bila** versi DB masih
    /// `expected_version` dan identitas (`generation`) cocok (RFC-0021 §5).
    ///
    /// Mengembalikan versi baru bila sukses, atau [`UpdateError::Conflict`] bila
    /// writer lain telah mengubah entity ini (versi/identitas tak cocok).
    /// Transaksional: pada konflik, tak ada perubahan.
    pub async fn update_entity(
        &self,
        world: &World,
        entity: Entity,
        expected_version: i64,
    ) -> Result<i64, UpdateError> {
        let id = i64::from(entity.index());
        let mut tx = self.pool.begin().await.map_err(UpdateError::Db)?;

        // Gerbang: naikkan versi hanya bila versi & identitas cocok.
        let new_version: Option<i64> = sqlx::query_scalar(
            "UPDATE arke_entities SET version = version + 1 \
             WHERE entity_id = $1 AND generation = $2 AND version = $3 \
             RETURNING version",
        )
        .bind(id)
        .bind(i64::from(entity.generation()))
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
            sqlx::query(&format!("DELETE FROM {} WHERE entity_id = $1", r.table))
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(UpdateError::Db)?;
            if let Some(params) = (r.dump_one)(world, entity) {
                let insert = insert_sql(r);
                let mut q = sqlx::query(&insert).bind(id);
                for (value, col) in params.iter().zip(r.columns) {
                    q = bind_value(q, col.ty, value);
                }
                q.execute(&mut *tx).await.map_err(UpdateError::Db)?;
            }
        }

        tx.commit().await.map_err(UpdateError::Db)?;
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
        let current = self.dump_state(world);
        let mut tx = self.pool.begin().await?;
        let mut stats = SyncStats {
            written: 0,
            deleted: 0,
        };

        // Hilang: ada di rekam, tak ada di `current`.
        for id in self.last.keys() {
            if !current.contains_key(id) {
                sqlx::query("DELETE FROM arke_entities WHERE entity_id = $1")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                stats.deleted += 1;
            }
        }

        // Baru atau berubah.
        for (id, state) in &current {
            if self.last.get(id) == Some(state) {
                continue; // tak berubah → lewati
            }
            sqlx::query(
                "INSERT INTO arke_entities (entity_id, generation, version) VALUES ($1, $2, 0) \
                 ON CONFLICT (entity_id) \
                 DO UPDATE SET generation = EXCLUDED.generation, version = arke_entities.version + 1",
            )
            .bind(*id)
            .bind(state.0)
            .execute(&mut *tx)
            .await?;
            // Ganti baris komponen entity ini.
            for (ci, r) in self.registered.iter().enumerate() {
                sqlx::query(&format!("DELETE FROM {} WHERE entity_id = $1", r.table))
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                if let Some(params) = &state.1[ci] {
                    let insert = insert_sql(r);
                    let mut q = sqlx::query(&insert).bind(*id);
                    for (value, col) in params.iter().zip(r.columns) {
                        q = bind_value(q, col.ty, value);
                    }
                    q.execute(&mut *tx).await?;
                }
            }
            stats.written += 1;
        }

        tx.commit().await?;
        self.last = current;
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

/// `INSERT INTO cmp_x (entity_id, c1, c2::jsonb, …) VALUES ($1, $2, $3::jsonb, …)`.
fn insert_sql(r: &Registered) -> String {
    let mut cols = String::from("entity_id");
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

/// `SELECT entity_id, c1, c2::text AS c2, … FROM cmp_x ORDER BY entity_id`.
///
/// Kolom `JSONB`/`NUMERIC` dibaca sebagai teks (`::text`).
fn select_sql(r: &Registered) -> String {
    let mut cols = String::from("entity_id");
    for col in r.columns {
        cols.push_str(", ");
        if read_as_text(col.ty) {
            cols.push_str(&format!("{name}::text AS {name}", name = col.name));
        } else {
            cols.push_str(col.name);
        }
    }
    format!("SELECT {} FROM {} ORDER BY entity_id", cols, r.table)
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
