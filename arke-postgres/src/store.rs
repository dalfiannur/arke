//! [`PgStore`]: persistensi async `World` ↔ Postgres (RFC-0021 §4).
//!
//! Fase v1: `connect`/`register`/`migrate`/`save`/`load` penuh, transaksional.
//! `World` adalah *working set*; Postgres sumber kebenaran. Determinisme muat
//! dijaga dengan `ORDER BY entity_id` (STD-0005). Tipe kolom yang didukung v1:
//! INTEGER/BIGINT/REAL/DOUBLE PRECISION/BOOLEAN/TEXT (NUMERIC/JSONB menyusul).

use std::collections::HashMap;

use arke::{Component, Entity, QueryData, World};
use sqlx::{PgPool, Postgres, Row, postgres::PgArguments, postgres::PgPoolOptions, query::Query};

use crate::{ColumnDef, PgComponent, PgType, PgValue, create_table_sql_from};

/// Satu baris komponen yang di-dump: `(entity_id, nilai-kolom)`.
type ComponentRow = (i64, Vec<PgValue>);

/// Operasi type-erased untuk satu tipe komponen terdaftar.
struct Registered {
    table: &'static str,
    columns: &'static [ColumnDef],
    /// Kumpulkan baris komponen dari `World`.
    dump: fn(&World) -> Vec<ComponentRow>,
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
        }
    }

    /// Mendaftarkan tipe komponen `T` untuk dipersist.
    pub fn register<T: PgComponent + Component>(&mut self) -> &mut Self {
        self.registered.push(Registered {
            table: T::TABLE,
            columns: T::COLUMNS,
            dump: dump_of::<T>,
            apply: apply_of::<T>,
        });
        self
    }

    /// Membuat tabel `arke_entities` + satu tabel per komponen terdaftar
    /// (idempoten via `IF NOT EXISTS`).
    pub async fn migrate(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS arke_entities \
             (entity_id BIGINT PRIMARY KEY, generation BIGINT NOT NULL)",
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
    pub async fn save(&self, world: &World) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        // Overwrite penuh: DELETE meng-cascade ke tabel komponen.
        sqlx::query("DELETE FROM arke_entities")
            .execute(&mut *tx)
            .await?;

        // Entity (yang punya ≥1 komponen).
        let mut entities: Vec<Entity> = Vec::new();
        <Entity>::each_filtered_shared::<()>(world, |e| entities.push(e));
        for e in &entities {
            sqlx::query("INSERT INTO arke_entities (entity_id, generation) VALUES ($1, $2)")
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
                for value in &params {
                    q = bind_value(q, value)?;
                }
                q.execute(&mut *tx).await?;
            }
        }

        tx.commit().await?;
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
}

/// `INSERT INTO cmp_x (entity_id, c1, c2, …) VALUES ($1, $2, $3, …)`.
fn insert_sql(r: &Registered) -> String {
    let mut cols = String::from("entity_id");
    let mut placeholders = String::from("$1");
    for (i, col) in r.columns.iter().enumerate() {
        cols.push_str(", ");
        cols.push_str(col.name);
        placeholders.push_str(&format!(", ${}", i + 2));
    }
    format!(
        "INSERT INTO {} ({}) VALUES ({})",
        r.table, cols, placeholders
    )
}

/// `SELECT entity_id, c1, c2, … FROM cmp_x ORDER BY entity_id`.
fn select_sql(r: &Registered) -> String {
    let mut cols = String::from("entity_id");
    for col in r.columns {
        cols.push_str(", ");
        cols.push_str(col.name);
    }
    format!("SELECT {} FROM {} ORDER BY entity_id", cols, r.table)
}

/// Bind satu [`PgValue`] ke query.
fn bind_value<'q>(
    q: Query<'q, Postgres, PgArguments>,
    value: &PgValue,
) -> Result<Query<'q, Postgres, PgArguments>, sqlx::Error> {
    Ok(match value {
        PgValue::Int(i) => q.bind(*i),
        PgValue::Float(f) => q.bind(*f),
        PgValue::Bool(b) => q.bind(*b),
        PgValue::Text(s) => q.bind(s.clone()),
        PgValue::Numeric(_) => {
            return Err(unsupported("NUMERIC (u64/usize)"));
        }
        PgValue::Null => {
            return Err(unsupported("NULL (Option<T>)"));
        }
    })
}

/// Baca satu kolom baris menjadi [`PgValue`] sesuai tipenya.
fn read_value(row: &sqlx::postgres::PgRow, col: &ColumnDef) -> Result<PgValue, sqlx::Error> {
    Ok(match col.ty {
        PgType::Integer => PgValue::Int(i64::from(row.try_get::<i32, _>(col.name)?)),
        PgType::BigInt => PgValue::Int(row.try_get::<i64, _>(col.name)?),
        PgType::Real => PgValue::Float(f64::from(row.try_get::<f32, _>(col.name)?)),
        PgType::DoublePrecision => PgValue::Float(row.try_get::<f64, _>(col.name)?),
        PgType::Boolean => PgValue::Bool(row.try_get::<bool, _>(col.name)?),
        PgType::Text => PgValue::Text(row.try_get::<String, _>(col.name)?),
        PgType::Numeric | PgType::Jsonb => {
            return Err(unsupported("NUMERIC/JSONB (load)"));
        }
    })
}

fn unsupported(what: &str) -> sqlx::Error {
    sqlx::Error::Protocol(format!("arke-postgres v1: tipe {what} belum didukung"))
}
