//! Transaksi yang dipegang **pemanggil** ([`PgStore::begin`] → [`PgTx`]).
//!
//! Semua op `PgStore` yang ada (`commit_insert`, `commit_update`, `remove`, …)
//! membuka dan menutup transaksinya sendiri — cukup untuk satu operasi, tetapi
//! pola *cek-lalu-tulis* (mis. "tolak bila slot booking bentrok, kalau tidak
//! insert") harus atomik terhadap pemesan lain. `PgTx` membuat pemanggil yang
//! memegang batas transaksi:
//!
//! ```ignore
//! let mut tx = store.begin().await?;
//! tx.advisory_lock(room_id).await?;                       // serialkan per-ruang
//! let taken = store.query::<Slot>().filter(overlap).exists_in(&mut tx).await?;
//! if taken { tx.rollback().await?; return Ok(None); }
//! let pid = store.commit_insert_in(&mut tx, staged).await?;
//! tx.commit().await?;
//! ```
//!
//! Varian `*_in(&mut tx, …)` menjalankan op di transaksi itu tanpa commit;
//! `Query::count_in`/`exists_in` membaca lewat koneksi yang sama (melihat baris
//! yang belum di-commit oleh tx ini). Drop `PgTx` tanpa `commit` = **rollback**.
//! Muat entity ke `World` (`load`/`fetch`) sengaja tidak ada versi tx-nya —
//! jalur itu menyentuh jembatan pid↔entity dan cache; di dalam tx cukup
//! `count_in`/`exists_in`.

use sqlx::{PgConnection, Postgres, Transaction};

use crate::PgStore;

/// Transaksi Postgres yang dipegang pemanggil. Lihat dokumentasi modul.
pub struct PgTx<'a> {
    tx: Transaction<'a, Postgres>,
}

impl PgStore {
    /// Mulai transaksi baru dari pool store ini. Op di dalamnya lewat varian
    /// `*_in(&mut tx, …)`; akhiri dengan [`PgTx::commit`] (atau drop = rollback).
    pub async fn begin(&self) -> Result<PgTx<'static>, sqlx::Error> {
        Ok(PgTx {
            tx: self.pool().begin().await?,
        })
    }
}

impl PgTx<'_> {
    /// `pg_advisory_xact_lock(key)`: kunci eksklusif ber-`key` (i64, global per
    /// database) yang **dilepas otomatis** saat commit/rollback. Pemanggil lain
    /// dengan `key` sama menunggu — pakai untuk menserialkan cek-lalu-tulis per
    /// sumber daya (mis. `room_id`). Bagikan ruang key dengan sadar bila
    /// aplikasi lain di DB yang sama juga memakai advisory lock.
    pub async fn advisory_lock(&mut self, key: i64) -> Result<(), sqlx::Error> {
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(key)
            .execute(&mut *self.tx)
            .await?;
        Ok(())
    }

    /// Commit. Setelah ini `PgTx` habis dipakai.
    pub async fn commit(self) -> Result<(), sqlx::Error> {
        self.tx.commit().await
    }

    /// Rollback eksplisit (drop tanpa commit melakukan hal yang sama).
    pub async fn rollback(self) -> Result<(), sqlx::Error> {
        self.tx.rollback().await
    }

    /// Koneksi mentah transaksi ini (untuk op internal `*_in`).
    pub(crate) fn conn(&mut self) -> &mut PgConnection {
        &mut self.tx
    }
}
