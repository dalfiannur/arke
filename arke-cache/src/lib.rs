//! Cache read-through berbasis **Redis / DragonflyDB / KeyDB** (protokol RESP)
//! untuk [`arke-postgres`](https://docs.rs/arke-postgres) (RFC-0033).
//!
//! Mengimplementasi [`arke_postgres::ComponentCache`]: baca komponen dilayani
//! cache (hit) atau Postgres (miss, lalu diisi); tulis meng-*invalidate*.
//! **Postgres tetap sumber kebenaran** — cache adalah lapisan transparan-performa.
//!
//! ```no_run
//! # async fn f() -> Result<(), Box<dyn std::error::Error>> {
//! use std::sync::Arc;
//! use arke_postgres::PgStore;
//! use arke_cache::RedisCache;
//!
//! let cache = RedisCache::connect("redis://localhost:6379", 300).await?; // TTL 300s
//! let mut store = PgStore::connect("postgres://…").await?.with_cache(Arc::new(cache));
//! # Ok(()) }
//! ```
//!
//! **Resilien**: kegagalan cache tak memutus aplikasi — degradasi ke Postgres
//! (error di-*swallow*, di-perlakukan miss / lewati tulis-cache).
//!
//! **Konsistensi**: `clear()` (dipakai `save` penuh) memanggil `FLUSHDB` →
//! **pakai DB/instance Redis yang didedikasikan untuk cache arke** (jangan berbagi
//! keyspace dengan data lain). TTL membatasi basi dari tulis luar-proses.

#![forbid(unsafe_code)]

use arke_postgres::ComponentCache;
use async_trait::async_trait;
use redis::aio::ConnectionManager;

/// Cache [`ComponentCache`] berbasis Redis-compatible (RFC-0033).
#[derive(Clone)]
pub struct RedisCache {
    conn: ConnectionManager,
    ttl_secs: u64,
}

impl RedisCache {
    /// Menyambung ke backend Redis-compatible (`redis://host:port[/db]`) dengan
    /// **TTL** (detik) untuk tiap kunci sebagai jaring pengaman staleness.
    pub async fn connect(url: &str, ttl_secs: u64) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(url)?;
        let conn = ConnectionManager::new(client).await?;
        Ok(Self { conn, ttl_secs })
    }

    /// Dari [`ConnectionManager`] yang sudah ada.
    pub fn from_manager(conn: ConnectionManager, ttl_secs: u64) -> Self {
        Self { conn, ttl_secs }
    }

    fn key(table: &str, id: i64) -> String {
        format!("arke:{table}:{id}")
    }
}

#[async_trait]
impl ComponentCache for RedisCache {
    async fn get_many(&self, table: &str, ids: &[i64]) -> Vec<Option<Vec<u8>>> {
        if ids.is_empty() {
            return Vec::new();
        }
        let keys: Vec<String> = ids.iter().map(|&id| Self::key(table, id)).collect();
        let mut conn = self.conn.clone();
        // Gagal → semua miss (degradasi ke Postgres).
        redis::cmd("MGET")
            .arg(&keys)
            .query_async(&mut conn)
            .await
            .unwrap_or_else(|_| vec![None; ids.len()])
    }

    async fn put_many(&self, table: &str, entries: &[(i64, Vec<u8>)]) {
        if entries.is_empty() {
            return;
        }
        let mut conn = self.conn.clone();
        let mut pipe = redis::pipe();
        for (id, bytes) in entries {
            pipe.cmd("SET")
                .arg(Self::key(table, *id))
                .arg(bytes.as_slice())
                .arg("EX")
                .arg(self.ttl_secs)
                .ignore();
        }
        let _: Result<(), _> = pipe.query_async(&mut conn).await; // best-effort
    }

    async fn invalidate(&self, table: &str, ids: &[i64]) {
        if ids.is_empty() {
            return;
        }
        let keys: Vec<String> = ids.iter().map(|&id| Self::key(table, id)).collect();
        let mut conn = self.conn.clone();
        let _: Result<(), _> = redis::cmd("DEL").arg(&keys).query_async(&mut conn).await;
    }

    async fn clear(&self) {
        let mut conn = self.conn.clone();
        // FLUSHDB: mengosongkan DB terpilih — asumsi DB didedikasikan untuk cache.
        let _: Result<(), _> = redis::cmd("FLUSHDB").query_async(&mut conn).await;
    }
}
