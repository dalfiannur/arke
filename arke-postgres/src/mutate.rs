//! Mutasi **massal ber-filter** tanpa memuat entity ke `World`:
//! [`UpdateWhere`] (`UPDATE cmp_T SET … WHERE …`) dan [`DeleteWhere`]
//! (`DELETE FROM arke_entities WHERE pid IN (SELECT pid FROM cmp_T WHERE …)`).
//! Dibuat lewat [`PgStore::update_where`]/[`PgStore::delete_where`].
//!
//! ```ignore
//! let n = store.update_where::<Booking>()
//!     .filter(Booking::end_ts().lt(now))
//!     .set(Booking::status(), "expired".to_string())
//!     .execute().await?;
//! ```
//!
//! Keduanya transaksional dan **menaikkan `version`** entity terdampak (writer
//! `update_entity` lain akan mendapat `Conflict` — datanya memang berubah) serta
//! meng-invalidate cache read-through. Rekam `save_incremental` di store lain
//! tak tahu perubahan ini; diff nilai berikutnya menulis ulang, tidak korup.
//! `set` hanya untuk kolom skalar/JSONB (token relasi tak punya `set`).
//! Varian `execute_in(&mut tx)` menjalankan di transaksi milik pemanggil.

use std::marker::PhantomData;

use sqlx::Row;

use crate::query::{Field, Filter, IntoPgValue, renumber};
use crate::store::bind_value;
use crate::tx::PgTx;
use crate::{PgComponent, PgStore, PgType, PgValue, quote_ident};

/// Builder `UPDATE … WHERE` atas komponen `T`. Lihat dokumentasi modul.
pub struct UpdateWhere<'a, T: PgComponent> {
    store: &'a PgStore,
    filter: Option<Filter<T>>,
    sets: Vec<SetClause>,
    _pd: PhantomData<fn() -> T>,
}

impl<'a, T: PgComponent> UpdateWhere<'a, T> {
    pub(crate) fn new(store: &'a PgStore) -> Self {
        Self {
            store,
            filter: None,
            sets: Vec::new(),
            _pd: PhantomData,
        }
    }

    /// Predikat `WHERE` (dipanggil >1× → `AND`). Tanpa filter → **semua** baris `T`.
    pub fn filter(mut self, f: Filter<T>) -> Self {
        self.filter = Some(match self.filter.take() {
            Some(existing) => existing.and(f),
            None => f,
        });
        self
    }

    /// `SET col = v` (nilai bertipe field, dicek compiler). Boleh berkali-kali.
    pub fn set<V: IntoPgValue>(mut self, field: Field<T, V>, v: V) -> Self {
        self.sets
            .push((field.column, field.cast(), field.ty, v.into_pg_value()));
        self
    }

    /// `(sql ter-renumber, params)`; `None` bila tak ada `set`.
    fn build(&self) -> Option<(String, Vec<(PgType, PgValue)>)> {
        update_sql(T::TABLE, &self.sets, self.filter.as_ref())
    }

    /// Jalankan dalam transaksi sendiri. Mengembalikan jumlah baris `T` terubah.
    pub async fn execute(self) -> Result<u64, sqlx::Error> {
        let mut tx = self.store.begin().await?;
        let n = self.execute_in(&mut tx).await?;
        tx.commit().await?;
        Ok(n)
    }

    /// Jalankan di transaksi `tx` milik pemanggil (tanpa commit).
    pub async fn execute_in(self, tx: &mut PgTx<'_>) -> Result<u64, sqlx::Error> {
        let Some((sql, params)) = self.build() else {
            return Ok(0);
        };
        let pids = run_returning_pids(tx, &sql, &params).await?;
        bump_versions(tx, &pids).await?;
        self.store.invalidate_cache(T::TABLE, &pids).await;
        Ok(pids.len() as u64)
    }
}

/// Builder `DELETE` entity yang komponen `T`-nya cocok. Lihat dokumentasi modul.
pub struct DeleteWhere<'a, T: PgComponent> {
    store: &'a PgStore,
    filter: Option<Filter<T>>,
    _pd: PhantomData<fn() -> T>,
}

impl<'a, T: PgComponent> DeleteWhere<'a, T> {
    pub(crate) fn new(store: &'a PgStore) -> Self {
        Self {
            store,
            filter: None,
            _pd: PhantomData,
        }
    }

    /// Predikat `WHERE` (dipanggil >1× → `AND`). Tanpa filter → **semua** entity
    /// yang punya komponen `T`.
    pub fn filter(mut self, f: Filter<T>) -> Self {
        self.filter = Some(match self.filter.take() {
            Some(existing) => existing.and(f),
            None => f,
        });
        self
    }

    fn build(&self) -> (String, Vec<(PgType, PgValue)>) {
        delete_sql(T::TABLE, self.filter.as_ref())
    }

    /// Jalankan dalam transaksi sendiri. Mengembalikan jumlah **entity** terhapus.
    pub async fn execute(self) -> Result<u64, sqlx::Error> {
        let mut tx = self.store.begin().await?;
        let n = self.execute_in(&mut tx).await?;
        tx.commit().await?;
        Ok(n)
    }

    /// Jalankan di transaksi `tx` milik pemanggil (tanpa commit).
    pub async fn execute_in(self, tx: &mut PgTx<'_>) -> Result<u64, sqlx::Error> {
        let (sql, params) = self.build();
        let pids = run_returning_pids(tx, &sql, &params).await?;
        // Entity hilang seluruhnya → invalidate di tiap tabel komponen.
        self.store.invalidate_all_tables(&pids).await;
        Ok(pids.len() as u64)
    }
}

/// Satu `SET` yang sudah di-*erase* tipenya: `(kolom, cast placeholder, tipe, nilai)`.
type SetClause = (&'static str, &'static str, PgType, PgValue);

/// `UPDATE table SET c1 = ?, … [WHERE f] RETURNING pid` ter-renumber; `None`
/// bila `sets` kosong. Terpisah dari builder agar teruji tanpa DB.
fn update_sql<C>(
    table: &str,
    sets: &[SetClause],
    filter: Option<&Filter<C>>,
) -> Option<(String, Vec<(PgType, PgValue)>)> {
    if sets.is_empty() {
        return None;
    }
    let mut params: Vec<(PgType, PgValue)> = Vec::with_capacity(sets.len());
    let assigns: Vec<String> = sets
        .iter()
        .map(|(col, cast, ty, v)| {
            params.push((*ty, v.clone()));
            format!("{} = ?{cast}", quote_ident(col))
        })
        .collect();
    let mut sql = format!("UPDATE {} SET {}", quote_ident(table), assigns.join(", "));
    if let Some(f) = filter {
        sql.push_str(" WHERE ");
        sql.push_str(&f.sql);
        params.extend(f.params.iter().cloned());
    }
    sql.push_str(" RETURNING pid");
    Some((renumber(&sql), params))
}

/// `DELETE FROM arke_entities WHERE pid IN (SELECT pid FROM table [WHERE f])
/// RETURNING pid` ter-renumber.
fn delete_sql<C>(table: &str, filter: Option<&Filter<C>>) -> (String, Vec<(PgType, PgValue)>) {
    let (where_sql, params) = match filter {
        Some(f) => (format!(" WHERE {}", f.sql), f.params.clone()),
        None => (String::new(), Vec::new()),
    };
    let sql = format!(
        "DELETE FROM arke_entities WHERE pid IN (SELECT pid FROM {}{where_sql}) RETURNING pid",
        quote_ident(table)
    );
    (renumber(&sql), params)
}

/// Jalankan `sql … RETURNING pid` pada `tx`; kembalikan pid terdampak.
async fn run_returning_pids(
    tx: &mut PgTx<'_>,
    sql: &str,
    params: &[(PgType, PgValue)],
) -> Result<Vec<i64>, sqlx::Error> {
    let mut q = sqlx::query(sql);
    for (ty, val) in params {
        q = bind_value(q, *ty, val);
    }
    q.fetch_all(tx.conn())
        .await?
        .iter()
        .map(|r| r.try_get("pid"))
        .collect()
}

/// `version = version + 1` untuk `pids` (no-op bila kosong).
async fn bump_versions(tx: &mut PgTx<'_>, pids: &[i64]) -> Result<(), sqlx::Error> {
    if pids.is_empty() {
        return Ok(());
    }
    sqlx::query("UPDATE arke_entities SET version = version + 1 WHERE pid = ANY($1)")
        .bind(pids)
        .execute(tx.conn())
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ColumnDef;

    struct Job {
        _attempts: i32,
    }
    impl PgComponent for Job {
        const TABLE: &'static str = "cmp_job";
        const COLUMNS: &'static [ColumnDef] =
            &[ColumnDef::scalar("attempts", PgType::Integer, false)];
        fn to_params(&self) -> Vec<PgValue> {
            vec![PgValue::Int(i64::from(self._attempts))]
        }
        fn from_params(v: &[PgValue]) -> Option<Self> {
            match v {
                [PgValue::Int(i)] => Some(Job {
                    _attempts: *i as i32,
                }),
                _ => None,
            }
        }
    }
    impl Job {
        fn attempts() -> Field<Self, i32> {
            Field::new("attempts", PgType::Integer)
        }
        fn tags() -> Field<Self, Vec<i64>> {
            Field::new("tags", PgType::Jsonb)
        }
    }

    #[test]
    fn update_sql_set_where_returning() {
        // Placeholder SET dinomori dulu, baru WHERE; cast JSONB ikut.
        let tags = Job::tags();
        let sets: Vec<SetClause> = vec![
            ("status", "", PgType::Text, PgValue::Text("failed".into())),
            (
                "tags",
                tags.cast(),
                PgType::Jsonb,
                PgValue::Json("[9]".into()),
            ),
        ];
        let f = Job::attempts().gte(1);
        let (sql, params) = update_sql(Job::TABLE, &sets, Some(&f)).unwrap();
        assert_eq!(
            sql,
            "UPDATE cmp_job SET status = $1, tags = $2::jsonb WHERE attempts >= $3 RETURNING pid"
        );
        assert_eq!(params[2], (PgType::Integer, PgValue::Int(1)));
        // Tanpa filter → semua baris; tanpa set → None.
        let (sql, _) = update_sql::<Job>(Job::TABLE, &sets, None).unwrap();
        assert_eq!(
            sql,
            "UPDATE cmp_job SET status = $1, tags = $2::jsonb RETURNING pid"
        );
        assert!(update_sql::<Job>(Job::TABLE, &[], Some(&Job::attempts().eq(0))).is_none());
    }

    #[test]
    fn delete_sql_hapus_entity_via_subquery() {
        let f = Job::attempts().gt(3);
        let (sql, params) = delete_sql(Job::TABLE, Some(&f));
        assert_eq!(
            sql,
            "DELETE FROM arke_entities WHERE pid IN (SELECT pid FROM cmp_job WHERE attempts > $1) RETURNING pid"
        );
        assert_eq!(params, vec![(PgType::Integer, PgValue::Int(3))]);
        let (sql, params) = delete_sql::<Job>(Job::TABLE, None);
        assert_eq!(
            sql,
            "DELETE FROM arke_entities WHERE pid IN (SELECT pid FROM cmp_job) RETURNING pid"
        );
        assert!(params.is_empty());
    }
}
