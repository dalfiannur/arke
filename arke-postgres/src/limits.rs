//! Batas operasional koneksi ([`ConnectOptions`]), klasifikasi kegagalan
//! ([`FailureKind`]), dan statistik pool ([`PoolStats`]).
//!
//! Tiga batas yang saling melengkapi (masing-masing menjaga hal berbeda):
//!
//! | Batas | Menjaga | Saat terlampaui |
//! |---|---|---|
//! | `max_connections` + `acquire_timeout` | antrean **pemanggil** (admission): pool = semaphore | `PoolTimedOut` → [`FailureKind::PoolTimeout`], *shed at the door* |
//! | `statement_timeout` | **slot** server dari query kabur | SQLSTATE `57014` → [`FailureKind::StatementTimeout`] |
//! | `lock_timeout` | tunggu kunci baris/advisory | SQLSTATE `55P03` → [`FailureKind::LockTimeout`] |
//!
//! *Deadline total per-request* (antre + eksekusi) **tidak** disediakan di sini:
//! bungkus future-nya dengan `tokio::time::timeout(d, fut)` di pemanggil —
//! itu membatasi pemanggil; slot server tetap dijaga `statement_timeout`.

use std::time::Duration;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Executor, PgPool};

/// Opsi koneksi & batas operasional untuk [`crate::PgStore::connect_with`].
/// `Default` = perilaku `connect()`: 5 koneksi, `acquire_timeout` 30 s, tanpa
/// batas sisi server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectOptions {
    /// Ukuran pool = batas **admission** (jumlah query konkuren ke server).
    pub max_connections: u32,
    /// Koneksi yang dijaga tetap hidup saat idle.
    pub min_connections: u32,
    /// Maksimum menunggu koneksi dari pool; lewat → `PoolTimedOut` tanpa
    /// menyentuh server (*shed at the door*).
    pub acquire_timeout: Duration,
    /// `SET statement_timeout` per koneksi: query yang lebih lama dibatalkan
    /// server (SQLSTATE `57014`). `None` = default server.
    pub statement_timeout: Option<Duration>,
    /// `SET lock_timeout` per koneksi (SQLSTATE `55P03`).
    pub lock_timeout: Option<Duration>,
    /// `SET idle_in_transaction_session_timeout` per koneksi: transaksi yang
    /// dibiarkan menggantung diputus server (SQLSTATE `25P03`).
    pub idle_in_transaction_session_timeout: Option<Duration>,
    /// `application_name` (terlihat di `pg_stat_activity`).
    pub application_name: Option<String>,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            max_connections: 5,
            min_connections: 0,
            acquire_timeout: Duration::from_secs(30),
            statement_timeout: None,
            lock_timeout: None,
            idle_in_transaction_session_timeout: None,
            application_name: None,
        }
    }
}

impl ConnectOptions {
    /// `SET …` yang dijalankan tiap koneksi baru (`after_connect`) — lewat
    /// `SET`, bukan parameter startup, agar kompatibel PgBouncer session mode.
    /// Nilai = integer milidetik (tak ada teks pemanggil → bebas injeksi).
    pub(crate) fn session_sql(&self) -> Option<String> {
        let mut parts = Vec::new();
        let ms = |d: Duration| d.as_millis().max(1);
        if let Some(d) = self.statement_timeout {
            parts.push(format!("SET statement_timeout = {}", ms(d)));
        }
        if let Some(d) = self.lock_timeout {
            parts.push(format!("SET lock_timeout = {}", ms(d)));
        }
        if let Some(d) = self.idle_in_transaction_session_timeout {
            parts.push(format!(
                "SET idle_in_transaction_session_timeout = {}",
                ms(d)
            ));
        }
        (!parts.is_empty()).then(|| parts.join("; "))
    }

    /// Bangun pool dari `url` dengan opsi ini.
    pub(crate) async fn build_pool(&self, url: &str) -> Result<PgPool, sqlx::Error> {
        let mut conn: PgConnectOptions = url.parse()?;
        if let Some(name) = &self.application_name {
            conn = conn.application_name(name);
        }
        let mut pool = PgPoolOptions::new()
            .max_connections(self.max_connections)
            .min_connections(self.min_connections)
            .acquire_timeout(self.acquire_timeout);
        if let Some(sql) = self.session_sql() {
            pool = pool.after_connect(move |c, _meta| {
                let sql = sql.clone();
                Box::pin(async move {
                    c.execute(sql.as_str()).await?;
                    Ok(())
                })
            });
        }
        pool.connect_with(conn).await
    }
}

/// Klasifikasi kegagalan `sqlx::Error` menurut **batas mana yang menyala** —
/// agar handler bisa memilih respons (mis. `PoolTimeout` → 503 *retry later*,
/// `StatementTimeout` → 504) tanpa mengorek SQLSTATE sendiri.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// Antrean pool melampaui `acquire_timeout` — server **tidak** disentuh.
    PoolTimeout,
    /// Server membatalkan query (`statement_timeout`, SQLSTATE `57014`).
    StatementTimeout,
    /// Server membatalkan tunggu kunci (`lock_timeout`, SQLSTATE `55P03`).
    LockTimeout,
    /// Koneksi/sesi putus — termasuk `idle_in_transaction_session_timeout`
    /// (`25P03`), admin shutdown (`57P01`), atau galat I/O.
    ConnectionLost,
    /// Selainnya (constraint, sintaks, decode, …).
    Other,
}

/// Petakan `err` ke [`FailureKind`].
pub fn failure_kind(err: &sqlx::Error) -> FailureKind {
    match err {
        sqlx::Error::PoolTimedOut => FailureKind::PoolTimeout,
        sqlx::Error::PoolClosed | sqlx::Error::Io(_) | sqlx::Error::Protocol(_) => {
            FailureKind::ConnectionLost
        }
        sqlx::Error::Database(db) => match db.code().as_deref() {
            Some("57014") => FailureKind::StatementTimeout,
            Some("55P03") => FailureKind::LockTimeout,
            Some("25P03" | "57P01" | "57P02" | "57P03" | "08000" | "08003" | "08006") => {
                FailureKind::ConnectionLost
            }
            _ => FailureKind::Other,
        },
        _ => FailureKind::Other,
    }
}

/// Potret pool saat ini (untuk health endpoint / metrik).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolStats {
    /// Koneksi yang sedang terbuka (dipakai + idle).
    pub size: u32,
    /// Koneksi idle (siap dipakai).
    pub idle: usize,
    /// Batas `max_connections`.
    pub max: u32,
}

impl PoolStats {
    pub(crate) fn of(pool: &PgPool) -> Self {
        Self {
            size: pool.size(),
            idle: pool.num_idle(),
            max: pool.options().get_max_connections(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_sql_hanya_yang_diset() {
        assert_eq!(ConnectOptions::default().session_sql(), None);
        let o = ConnectOptions {
            statement_timeout: Some(Duration::from_millis(1500)),
            lock_timeout: Some(Duration::from_micros(10)), // < 1 ms → 1
            ..ConnectOptions::default()
        };
        assert_eq!(
            o.session_sql().as_deref(),
            Some("SET statement_timeout = 1500; SET lock_timeout = 1")
        );
    }

    #[test]
    fn failure_kind_dipetakan() {
        assert_eq!(
            failure_kind(&sqlx::Error::PoolTimedOut),
            FailureKind::PoolTimeout
        );
        assert_eq!(
            failure_kind(&sqlx::Error::PoolClosed),
            FailureKind::ConnectionLost
        );
        assert_eq!(failure_kind(&sqlx::Error::RowNotFound), FailureKind::Other);
    }
}
