//! **Upsert** entity berkunci indeks unik (RFC-0038): sisipkan entity baru,
//! atau — bila komponen `T`-nya bentrok dengan baris yang ada pada target
//! konflik — pakai entity lama (DO NOTHING) atau perbarui kolom tertentu
//! (DO UPDATE, opsional `coalesce`). Dibuat lewat [`PgStore::upsert`].
//!
//! ```ignore
//! let staged = store.stage_insert(&world, e);
//! let out = store.upsert::<Contact>(staged)
//!     .on(Contact::workspace()).on(Contact::wa_chat_id())
//!     .update_coalesce(Contact::push_name())
//!     .execute().await?;
//! // out.pid: entity baru atau lama; out.inserted: apakah baru.
//! ```
//!
//! - Target konflik (`on`) harus persis kolom sebuah indeks/constraint unik
//!   (mis. `#[pg(unique(a, b))]`, RFC-0037) — aturan inferensi Postgres.
//! - `pid` dialokasikan lebih dulu; bila ternyata konflik, pid itu dihapus lagi
//!   dalam transaksi yang sama, jadi tak ada entity yatim.
//! - Komponen lain di `staged` hanya ditulis bila entity **baru**.
//! - DO UPDATE menaikkan `version` entity lama (writer `update_entity` lain
//!   mendapat `Conflict`) dan meng-invalidate cache read-through pada
//!   [`Upsert::execute`]. Varian [`Upsert::execute_in`] tidak meng-invalidate
//!   (pola `*_in` lain): pemanggil yang memegang transaksi.
//! - Aman terhadap upsert serentak berkunci sama: tepat satu yang `inserted`.

use std::marker::PhantomData;

use sqlx::Row;
use sqlx::postgres::PgConnection;

use crate::query::Field;
use crate::store::{StagedInsert, bind_value};
use crate::tx::PgTx;
use crate::{PgComponent, PgStore, PgType, PgValue, quote_ident};

/// Hasil [`Upsert`]: `pid` entity (baru atau yang sudah ada) dan apakah baru.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Upserted {
    /// `pid` entity hasil.
    pub pid: i64,
    /// `true` bila entity baru disisipkan; `false` bila bentrok dengan yang ada.
    pub inserted: bool,
}

/// Aksi satu kolom saat konflik.
#[derive(Clone, Copy)]
enum SetMode {
    /// `col = EXCLUDED.col`.
    Replace,
    /// `col = coalesce(EXCLUDED.col, <tabel>.col)` — NULL baru tak menimpa.
    Coalesce,
}

/// Builder upsert atas komponen kunci `T`. Lihat dokumentasi modul.
pub struct Upsert<'a, T: PgComponent> {
    store: &'a PgStore,
    staged: StagedInsert,
    target: Vec<&'static str>,
    sets: Vec<(&'static str, SetMode)>,
    _pd: PhantomData<fn() -> T>,
}

/// Batas percobaan ulang bila baris yang bentrok hilang sebelum sempat dibaca
/// (dihapus transaksi lain di antara INSERT dan SELECT).
const MAX_ATTEMPTS: usize = 3;

impl<'a, T: PgComponent> Upsert<'a, T> {
    pub(crate) fn new(store: &'a PgStore, staged: StagedInsert) -> Self {
        Self {
            store,
            staged,
            target: Vec::new(),
            sets: Vec::new(),
            _pd: PhantomData,
        }
    }

    /// Tambah kolom ke target konflik `ON CONFLICT (…)`, berurutan.
    pub fn on<V>(mut self, field: Field<T, V>) -> Self {
        self.target.push(field.column);
        self
    }

    /// Saat konflik: `col = EXCLUDED.col` (nilai baru menimpa).
    pub fn update<V>(mut self, field: Field<T, V>) -> Self {
        self.sets.push((field.column, SetMode::Replace));
        self
    }

    /// Saat konflik: `col = coalesce(EXCLUDED.col, col)` — nilai baru menimpa
    /// kecuali ia `NULL`.
    pub fn update_coalesce<V>(mut self, field: Field<T, V>) -> Self {
        self.sets.push((field.column, SetMode::Coalesce));
        self
    }

    /// Jalankan dalam transaksi sendiri.
    pub async fn execute(self) -> Result<Upserted, sqlx::Error> {
        let store = self.store;
        let updates = !self.sets.is_empty();
        let mut tx = store.begin().await?;
        let out = self.run(tx.conn()).await?;
        tx.commit().await?;
        if updates && !out.inserted {
            store.invalidate_all_tables(&[out.pid]).await;
        }
        Ok(out)
    }

    /// Jalankan di transaksi `tx` milik pemanggil (tanpa commit, tanpa
    /// invalidasi cache).
    pub async fn execute_in(self, tx: &mut PgTx<'_>) -> Result<Upserted, sqlx::Error> {
        self.run(tx.conn()).await
    }

    async fn run(self, conn: &mut PgConnection) -> Result<Upserted, sqlx::Error> {
        if self.target.is_empty() {
            return Err(protocol("upsert: target konflik kosong (panggil `on`)"));
        }
        let Some(ci) = self.store.registered_index(T::TABLE) else {
            return Err(protocol(&format!(
                "upsert: komponen `{}` belum di-register",
                T::TABLE
            )));
        };
        let Some(key_params) = self.staged.params_of(ci) else {
            return Err(protocol(&format!(
                "upsert: entity yang di-stage tak punya komponen `{}`",
                T::TABLE
            )));
        };
        let key_params = self.store.resolve_refs_pub(key_params);
        let insert = self.insert_sql();

        for _ in 0..MAX_ATTEMPTS {
            let pid: i64 =
                sqlx::query_scalar("INSERT INTO arke_entities (version) VALUES (0) RETURNING pid")
                    .fetch_one(&mut *conn)
                    .await?;
            let mut q = sqlx::query(&insert).bind(pid);
            for (v, col) in key_params.iter().zip(T::COLUMNS) {
                q = bind_value(q, col.ty, v);
            }
            let got = match q.fetch_optional(&mut *conn).await? {
                Some(row) => Some(row.try_get::<i64, _>("pid")?),
                None => None,
            };

            if got == Some(pid) {
                // Baru: tulis komponen lain entity ini.
                for (other, params) in self.staged.rows() {
                    if *other != ci {
                        self.store.insert_row(conn, *other, pid, params).await?;
                    }
                }
                return Ok(Upserted {
                    pid,
                    inserted: true,
                });
            }

            // Konflik: lepas pid yang tadi dialokasikan.
            sqlx::query("DELETE FROM arke_entities WHERE pid = $1")
                .bind(pid)
                .execute(&mut *conn)
                .await?;
            let existing = match got {
                // DO UPDATE mengembalikan pid baris lama.
                Some(existing) => Some(existing),
                // DO NOTHING tak mengembalikan apa pun: baca baris yang bentrok.
                None => self.select_existing(conn, &key_params).await?,
            };
            if let Some(existing) = existing {
                if !self.sets.is_empty() {
                    sqlx::query("UPDATE arke_entities SET version = version + 1 WHERE pid = $1")
                        .bind(existing)
                        .execute(&mut *conn)
                        .await?;
                }
                return Ok(Upserted {
                    pid: existing,
                    inserted: false,
                });
            }
            // Baris yang bentrok terhapus sebelum terbaca → coba lagi.
        }
        Err(protocol(
            "upsert: baris yang bentrok terus menghilang (percobaan habis)",
        ))
    }

    /// `INSERT INTO t (pid, …) VALUES ($1, …) ON CONFLICT (…) DO NOTHING|DO
    /// UPDATE SET … RETURNING pid`.
    fn insert_sql(&self) -> String {
        let table = quote_ident(T::TABLE);
        let mut cols = String::from("pid");
        let mut placeholders = String::from("$1");
        for (i, col) in T::COLUMNS.iter().enumerate() {
            cols.push_str(", ");
            cols.push_str(&quote_ident(col.name));
            placeholders.push_str(&format!(", ${}{}", i + 2, col.ty.bind_cast()));
        }
        let target: Vec<_> = self.target.iter().map(|c| quote_ident(c)).collect();
        let action = if self.sets.is_empty() {
            "DO NOTHING".to_string()
        } else {
            let sets: Vec<String> = self
                .sets
                .iter()
                .map(|(c, mode)| {
                    let c = quote_ident(c);
                    match mode {
                        SetMode::Replace => format!("{c} = EXCLUDED.{c}"),
                        SetMode::Coalesce => format!("{c} = coalesce(EXCLUDED.{c}, {table}.{c})"),
                    }
                })
                .collect();
            format!("DO UPDATE SET {}", sets.join(", "))
        };
        format!(
            "INSERT INTO {table} ({cols}) VALUES ({placeholders}) \
             ON CONFLICT ({}) {action} RETURNING pid",
            target.join(", ")
        )
    }

    /// `pid` baris `T` yang kolom targetnya sama dengan nilai `key_params`.
    async fn select_existing(
        &self,
        conn: &mut PgConnection,
        key_params: &[PgValue],
    ) -> Result<Option<i64>, sqlx::Error> {
        let mut conds = Vec::new();
        let mut binds: Vec<(PgType, &PgValue)> = Vec::new();
        for name in &self.target {
            let Some(i) = T::COLUMNS.iter().position(|c| c.name == *name) else {
                return Err(protocol(&format!(
                    "upsert: kolom target `{name}` tak dikenal"
                )));
            };
            let col = &T::COLUMNS[i];
            binds.push((col.ty, &key_params[i]));
            conds.push(format!(
                "{} = ${}{}",
                quote_ident(col.name),
                binds.len(),
                col.ty.bind_cast()
            ));
        }
        let sql = format!(
            "SELECT pid FROM {} WHERE {}",
            quote_ident(T::TABLE),
            conds.join(" AND ")
        );
        let mut q = sqlx::query(&sql);
        for (ty, v) in binds {
            q = bind_value(q, ty, v);
        }
        match q.fetch_optional(&mut *conn).await? {
            Some(row) => Ok(Some(row.try_get::<i64, _>("pid")?)),
            None => Ok(None),
        }
    }
}

fn protocol(msg: &str) -> sqlx::Error {
    sqlx::Error::Protocol(msg.to_string())
}
