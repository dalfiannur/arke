# arke-mongo v1 (fondasi) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Membangun crate adapter `arke-mongo` 0.1 yang menjadikan MongoDB sumber kebenaran bagi keadaan ECS `arke`, dengan pemetaan satu dokumen per entity.

**Architecture:** Komponen diserialisasi lewat `arke::Serialize` → `arke::Value` → BSON, disimpan sebagai sub-dokumen di bawah field `cmp` pada koleksi `arke_entities`. `ObjectId` adalah `pid` (identitas persisten, RFC-0034); indeks World tetap ephemeral dan dijembatani peta `pid ↔ Entity` di dalam store. Seluruh logika pemetaan ditulis sebagai **fungsi murni** di modul terpisah agar bisa diuji tanpa database; modul `store` hanya membungkusnya dengan I/O async.

**Tech Stack:** Rust 2024 (MSRV 1.88), `mongodb` 3.8 (driver resmi, API fluent-builder, bson 3.x lewat re-export `mongodb::bson`), `futures-util` 0.3 (`TryStreamExt` untuk cursor), `tokio` 1 (dev-dependency).

**Spec:** [`docs/RFC/RFC-0035-arke-mongo-adapter.md`](../../RFC/RFC-0035-arke-mongo-adapter.md)

**Catatan verifikasi API:** tanda tangan driver di rencana ini mengacu ke `mongodb` 3.8 (builder berantai: `update_one(filter, update).upsert(true)`, `find(filter).sort(doc)`, `create_indexes(models)`). Bila ada ketidakcocokan tanda tangan, langkah `cargo build`/`cargo test` di akhir tiap task menangkapnya langsung — jangan menebak, baca error kompilernya.

---

## Konteks yang perlu dibaca lebih dulu

Baca tiga berkas ini sebelum menulis kode. Rencana ini meniru polanya, dan menyimpang darinya hanya di tempat yang disebut eksplisit.

| Berkas | Yang perlu diambil |
| --- | --- |
| `docs/RFC/RFC-0035-arke-mongo-adapter.md` | Spesifikasi lengkap. Sumber kebenaran bila rencana ini ambigu. |
| `arke-postgres/src/store.rs:32-62` | Pola registry type-erased (`Registered`, `dump_one_of`, `apply_of`). `arke-mongo` memakai bentuk yang sama, hanya dengan `Value` alih-alih `Vec<PgValue>`. |
| `arke-postgres/tests/store.rs:1-10` | Pola tes integrasi yang di-*skip* bila env var tak diset. `arke-mongo` memakai pola identik dengan `MONGODB_URI`. |

**API `arke` yang dipakai** (semuanya publik, sudah diverifikasi ada):

- `arke::World::spawn() -> Entity`, `despawn(Entity)`, `insert::<T>(Entity, T)`, `get::<T>(Entity) -> Option<&T>`
- `arke::Serialize` — trait (`to_value`/`from_value`) **dan** derive macro dengan nama sama
- `arke::Value` — enum `Null | Bool | Int(i64) | Float(f64) | Text(String) | List(Vec<Value>) | Map(Vec<(String, Value)>)`
- `arke::QueryData::each_filtered_shared::<()>(world, f)` — enumerasi entity hidup; dipakai lewat `<Entity>::each_filtered_shared::<()>(...)`
- `arke::Component` di-blanket-impl untuk semua `'static + Send`, dan `Serialize: Component`. Jadi bound `T: MongoComponent` **sudah cukup** — tak perlu `+ Component`.

---

## Struktur berkas

| Berkas | Tanggung jawab |
| --- | --- |
| `arke-mongo/Cargo.toml` | Manifest crate adapter. |
| `arke-mongo/src/lib.rs` | Permukaan publik: `MongoComponent`, `IndexDef`, `Dir`, `Pid`, `MongoError`, makro `mongo_component!`, deklarasi modul, dokumentasi crate. |
| `arke-mongo/src/bson_map.rs` | **Fungsi murni**: `Value` ↔ BSON, validasi nama field. Tak tahu apa-apa soal World atau driver. |
| `arke-mongo/src/registry.rs` | **Fungsi murni**: registry type-erased + pembangunan dokumen (`cmp_doc`, `update_ops`, `apply`, `index_models`). Tahu World, tak tahu driver. |
| `arke-mongo/src/store.rs` | `MongoStore` — satu-satunya modul yang menyentuh I/O async. |
| `arke-mongo/tests/mapping.rs` | Lapis 1: tes tanpa database. |
| `arke-mongo/tests/store.rs` | Lapis 2: tes integrasi, di-skip bila `MONGODB_URI` tak diset. |
| `arke-mongo/README.md` | Dokumen crate untuk crates.io. |
| `Cargo.toml` (root) | Tambah `arke-mongo` ke `workspace.members`. |
| `.github/workflows/ci.yml` | Job `mongo` dengan service container. |
| `docs/MILESTONE_32.md` | Ringkasan milestone + Definition of Done. |

Pemisahan `bson_map` / `registry` / `store` adalah inti strategi tes: dua modul pertama bisa diuji seluruhnya tanpa database, dan hanya `store.rs` yang butuh MongoDB nyata.

---

## Task 1: Amandemen RFC — ganti `bulkWrite` dengan operasi per-dokumen berurutan

RFC-0035 §5 menyebut `bulkWrite ordered`. Itu **tak bisa diimplementasikan** sesuai batasan RFC-nya sendiri: `Client::bulk_write` di driver Rust memakai perintah `bulkWrite` yang baru ada di **MongoDB server 8.0**, sedangkan §7 menetapkan CI memakai `mongo:7` standalone justru agar adapter ini jalan di deployment sederhana. Driver Rust juga tak punya `Collection::bulk_write` seperti driver bahasa lain.

Jalan keluarnya bukan menaikkan syarat server, melainkan memakai **operasi per-dokumen berurutan** (`update_one` per entity + satu `delete_many`). Semantik yang dijanjikan RFC **tidak berubah** — atomik per-entity, tidak per-World — hanya mekanismenya yang portabel. Repo ini sudah punya preseden amandemen RFC pasca-Accepted (RFC-0034 Am.1 & Am.3).

Sekalian perbaiki nomor milestone: header RFC menulis `M-35`, tetapi berkas milestone berjalan sekuensial dan yang terakhir adalah `MILESTONE_31.md` (RFC-0033). Milestone berikutnya adalah **M-32**.

**Files:**
- Modify: `docs/RFC/RFC-0035-arke-mongo-adapter.md`

- [ ] **Step 1: Ubah header milestone**

Ganti baris:

```markdown
- **Milestone:** M-35 (Adapter MongoDB — fondasi)
```

menjadi:

```markdown
- **Milestone:** M-32 (Adapter MongoDB — fondasi)
```

- [ ] **Step 2: Tambahkan bagian amandemen tepat di bawah baris `- **RFC terkait:** …`**

```markdown
> **Amandemen 1 (2026-07-31) — mekanisme `save`.** Naskah asli §5 menyebut
> `bulkWrite ordered`. Perintah `bulkWrite` baru tersedia pada **MongoDB server
> 8.0**, sedangkan §7 sengaja menargetkan `mongod` 7 standalone; driver Rust juga
> tak menyediakan `Collection::bulk_write`. `save` karena itu diimplementasikan
> sebagai **operasi per-dokumen berurutan** (`update_one` per entity + satu
> `delete_many`). **Jaminan yang dijanjikan tidak berubah** — atomik per-entity,
> bukan per-World; yang berubah hanya mekanismenya, demi portabilitas server.
```

- [ ] **Step 3: Perbarui paragraf Atomisitas di §5**

Ganti kalimat pembuka paragraf **Atomisitas**:

```markdown
**Atomisitas.** `bulkWrite` **ordered** tanpa transaksi sesi: atomik **per-entity**, bukan per-World.
```

menjadi:

```markdown
**Atomisitas.** Operasi per-dokumen berurutan tanpa transaksi sesi (lihat Amandemen 1): atomik **per-entity**, bukan per-World.
```

- [ ] **Step 4: Perbarui dua baris lain yang menyebut `bulkWrite`**

Di §5, ganti komentar pada blok kode:

```rust
store.save(&world).await?;          // bulkWrite ordered
```

menjadi:

```rust
store.save(&world).await?;          // update_one per entity + delete_many
```

Di §5, ganti kalimat tentang despawn:

```markdown
`save` juga menghapus dokumen yang `pid`-nya ada di `entity_of` tetapi entity-nya sudah tak ada di World (despawn) — `delete_many` dalam `bulkWrite` yang sama.
```

menjadi:

```markdown
`save` juga menghapus dokumen yang `pid`-nya ada di `entity_of` tetapi entity-nya sudah tak ada di World (despawn) — satu `delete_many` setelah seluruh upsert.
```

Di tabel Alternatif, ganti sel pertama baris yang dipilih:

```markdown
| **`bulkWrite` tanpa transaksi (dipilih)** |
```

menjadi:

```markdown
| **Operasi per-dokumen tanpa transaksi (dipilih)** |
```

- [ ] **Step 5: Putuskan pertanyaan terbuka soal `NAME` bertabrakan**

Ganti butir pertanyaan terbuka:

```markdown
- **`NAME` yang bertabrakan.** Karena `NAME` wajib eksplisit (§2), dua komponen masih bisa mendeklarasikan nama yang sama. `register` mendeteksinya — tetapi bereaksi bagaimana: panic (bug programmer, gagal sedini mungkin) atau `Result`?
```

menjadi:

```markdown
- ~~**`NAME` yang bertabrakan.**~~ **Diputuskan (Am. 1): `panic`.** `register` bukan jalur yang bisa gagal karena data — dua komponen dengan `NAME` sama adalah bug programmer, dan `register` mengembalikan `&mut Self` untuk chaining (pola `PgStore::register`). Gagal sedini mungkin, dengan pesan yang menyebut nama yang bertabrakan.
```

- [ ] **Step 6: Ganti mitigasi `IndexDef::field` menjadi `debug_assert!`**

Naskah asli §2 menjanjikan "mencatat peringatan". `arke-mongo` tak punya (dan tak ingin menyeret) crate logging, dan `eprintln!` dari dalam pustaka adalah perilaku buruk. `debug_assert!` memberi hal yang sama-sama berguna tanpa dependensi: gagal keras saat pengembangan dan uji, nol biaya di rilis.

Ganti paragraf:

```markdown
Mitigasi v1 memang lemah dan diakui begitu: saat sebuah nilai `T` pertama kali diserialisasi, store membandingkan kunci hasil `to_value()` terhadap `INDEXES` dan mencatat peringatan bila ada `field` yang tak dikenali. Pemeriksaan saat `register` tak mungkin — tanpa nilai contoh, `Value` tak bisa dihasilkan. Lihat §Pertanyaan terbuka.
```

menjadi:

```markdown
Mitigasi v1 (Am. 1): saat komponen diserialisasi, `Registry` membandingkan kunci hasil `to_value()` terhadap `INDEXES` lewat `debug_assert!` — gagal keras di build debug dan di seluruh uji, nol biaya di rilis. Pemeriksaan saat `register` tak mungkin: tanpa nilai contoh, `Value` tak bisa dihasilkan. Pilihan `debug_assert!` menghindari menyeret crate logging ke dalam adapter hanya untuk satu peringatan.
```

- [ ] **Step 7: Verifikasi tak ada sisa `bulkWrite`**

Run: `grep -n "bulkWrite" docs/RFC/RFC-0035-arke-mongo-adapter.md`
Expected: hanya kemunculan di dalam blok Amandemen 1 (yang memang menjelaskan alasannya).

- [ ] **Step 8: Commit**

```bash
git add docs/RFC/RFC-0035-arke-mongo-adapter.md
git commit -m "docs(rfc): RFC-0035 amandemen 1 — save via operasi per-dokumen, milestone M-32"
```

---

## Task 2: Scaffold crate `arke-mongo`

**Files:**
- Create: `arke-mongo/Cargo.toml`
- Create: `arke-mongo/src/lib.rs`
- Modify: `Cargo.toml` (root, baris `members`)

- [ ] **Step 1: Buat manifest crate**

Buat `arke-mongo/Cargo.toml`:

```toml
[package]
name = "arke-mongo"
version = "0.1.0"
edition = "2024"
rust-version = "1.88"
description = "MongoDB adapter for the arke ECS — document-per-entity persistence (source of truth)."
license = "MIT"
repository = "https://github.com/dalfiannur/arke"
readme = "README.md"
keywords = ["ecs", "mongodb", "persistence", "document", "nosql"]
categories = ["database", "game-development"]
# Paket ramping: sumber + README + lisensi.
include = ["/src", "/Cargo.toml", "/README.md", "/LICENSE-MIT"]

# Adapter: BOLEH punya dependensi (di luar STD-0003 yang mengikat core `arke`).
[dependencies]
arke = { version = "0.6.1", path = ".." }
# `bson` dipakai lewat re-export `mongodb::bson` agar versi tak pernah skew.
mongodb = "3.8"
# `TryStreamExt` untuk iterasi `Cursor` pada `load`.
futures-util = { version = "0.3", default-features = false, features = ["std"] }

[dev-dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }

[lints.rust]
unsafe_code = "forbid"
missing_docs = "warn"
```

- [ ] **Step 2: Buat lib.rs minimal**

Buat `arke-mongo/src/lib.rs`:

```rust
//! Adapter MongoDB untuk ECS [`arke`](https://docs.rs/arke): persistensi
//! **satu dokumen per entity** yang menjadikan MongoDB **sumber kebenaran**
//! (RFC-0035).
//!
//! Seluruh komponen sebuah entity hidup sebagai sub-dokumen di bawah field
//! `cmp` pada koleksi `arke_entities`; identitas persisten (`pid`) adalah
//! `ObjectId` (RFC-0034 — indeks World tetap ephemeral).
//!
//! Core `arke` tetap **0-dependensi** (STD-0003); crate adapter inilah gerbang
//! dependensi driver.

/// Re-ekspor `bson` milik driver, agar pengguna tak perlu menambah dependensi
/// `bson` sendiri (dan tak bisa salah-versi).
pub use mongodb::bson;
```

- [ ] **Step 3: Daftarkan ke workspace**

Di `Cargo.toml` root, ubah baris `members`:

```toml
members = ["arke-derive", "arke-postgres", "arke-postgres-derive", "arke-cache"]
```

menjadi:

```toml
members = ["arke-derive", "arke-postgres", "arke-postgres-derive", "arke-cache", "arke-mongo"]
```

- [ ] **Step 4: Verifikasi build**

Run: `cargo build -p arke-mongo`
Expected: sukses (mengunduh `mongodb` 3.8 dan dependensinya pada run pertama — ini butuh jaringan dan bisa memakan satu-dua menit).

- [ ] **Step 5: Verifikasi gerbang STD-0003 tetap hijau**

Run: `cargo tree -p arke -e normal --prefix none | sort -u | grep -vE '^arke(-derive)? ' | grep -v '^$'`
Expected: **tak ada keluaran**. Core `arke` tetap tanpa dependensi pihak-ketiga; crate adapter baru tak boleh mengubah ini.

- [ ] **Step 6: Commit**

```bash
git add arke-mongo/Cargo.toml arke-mongo/src/lib.rs Cargo.toml Cargo.lock
git commit -m "feat(arke-mongo): scaffold crate adapter (RFC-0035)"
```

---

## Task 3: Pemetaan `Value` → BSON

**Files:**
- Create: `arke-mongo/src/bson_map.rs`
- Modify: `arke-mongo/src/lib.rs`
- Test: `arke-mongo/tests/mapping.rs`

Fungsi ini harus `pub` (bukan `pub(crate)`) supaya bisa diuji dari `tests/`, yang merupakan crate terpisah.

- [ ] **Step 1: Tulis tes yang gagal**

Buat `arke-mongo/tests/mapping.rs`:

```rust
//! Lapis 1 (RFC-0035 §7): tes pemetaan tanpa database.

use arke::Value;
use arke_mongo::bson::{Bson, Document};
use arke_mongo::value_to_bson;

#[test]
fn skalar_dipetakan_ke_bson_yang_setara() {
    assert_eq!(value_to_bson(&Value::Null), Bson::Null);
    assert_eq!(value_to_bson(&Value::Bool(true)), Bson::Boolean(true));
    assert_eq!(value_to_bson(&Value::Int(42)), Bson::Int64(42));
    assert_eq!(value_to_bson(&Value::Float(1.5)), Bson::Double(1.5));
    assert_eq!(
        value_to_bson(&Value::Text("halo".into())),
        Bson::String("halo".into())
    );
}

#[test]
fn map_menjadi_document_dengan_urutan_field_terjaga() {
    let v = Value::Map(vec![
        ("z".into(), Value::Int(1)),
        ("a".into(), Value::Int(2)),
    ]);
    let Bson::Document(d) = value_to_bson(&v) else {
        panic!("Map harus jadi Document");
    };
    let keys: Vec<&str> = d.keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["z", "a"], "urutan sisip harus terjaga");
}

#[test]
fn list_bersarang_dipetakan_rekursif() {
    let v = Value::List(vec![
        Value::Int(1),
        Value::Map(vec![("x".into(), Value::Text("y".into()))]),
    ]);
    let mut inner = Document::new();
    inner.insert("x", "y");
    assert_eq!(
        value_to_bson(&v),
        Bson::Array(vec![Bson::Int64(1), Bson::Document(inner)])
    );
}
```

- [ ] **Step 2: Jalankan tes untuk memastikan gagal**

Run: `cargo test -p arke-mongo --test mapping`
Expected: FAIL saat kompilasi — `unresolved import arke_mongo::value_to_bson`.

- [ ] **Step 3: Tulis implementasi minimal**

Buat `arke-mongo/src/bson_map.rs`:

```rust
//! Pemetaan murni [`arke::Value`] ↔ BSON (RFC-0035 §4). Tanpa I/O, tanpa World —
//! seluruh modul ini dapat diuji tanpa database.

use arke::Value;
use mongodb::bson::{Bson, Document};

/// Memetakan [`Value`] ke BSON, rekursif. Total: tiap varian punya padanan.
///
/// `Int` selalu menjadi `Int64` (bukan `Int32`) supaya round-trip tak
/// bergantung pada besar nilai.
pub fn value_to_bson(value: &Value) -> Bson {
    match value {
        Value::Null => Bson::Null,
        Value::Bool(b) => Bson::Boolean(*b),
        Value::Int(i) => Bson::Int64(*i),
        Value::Float(f) => Bson::Double(*f),
        Value::Text(s) => Bson::String(s.clone()),
        Value::List(items) => Bson::Array(items.iter().map(value_to_bson).collect()),
        Value::Map(entries) => {
            let mut doc = Document::new();
            for (key, val) in entries {
                doc.insert(key.clone(), value_to_bson(val));
            }
            Bson::Document(doc)
        }
    }
}
```

Di `arke-mongo/src/lib.rs`, tambahkan setelah baris `pub use mongodb::bson;`:

```rust
mod bson_map;
pub use bson_map::value_to_bson;
```

- [ ] **Step 4: Jalankan tes untuk memastikan lulus**

Run: `cargo test -p arke-mongo --test mapping`
Expected: PASS, 3 tes.

- [ ] **Step 5: Commit**

```bash
git add arke-mongo/src/bson_map.rs arke-mongo/src/lib.rs arke-mongo/tests/mapping.rs
git commit -m "feat(arke-mongo): pemetaan Value -> BSON (RFC-0035 §4)"
```

---

## Task 4: Pemetaan BSON → `Value` + round-trip

`Int32` sengaja diterima meski `value_to_bson` tak pernah menghasilkannya: dokumen bisa ditulis service lain atau `mongosh`, yang mengirim bilangan bulat kecil sebagai `Int32`. Menolaknya akan membuat adapter gagal membaca datanya sendiri dari sisi lain.

**Files:**
- Modify: `arke-mongo/src/bson_map.rs`
- Modify: `arke-mongo/src/lib.rs`
- Test: `arke-mongo/tests/mapping.rs`

- [ ] **Step 1: Tulis tes yang gagal**

Tambahkan ke `arke-mongo/tests/mapping.rs`:

```rust
use arke_mongo::bson_to_value;

#[test]
fn round_trip_value_bson_value_setia() {
    let cases = vec![
        Value::Null,
        Value::Bool(false),
        Value::Int(-7),
        Value::Float(0.25),
        Value::Text("teks".into()),
        Value::List(vec![Value::Int(1), Value::Null]),
        Value::Map(vec![
            ("a".into(), Value::Int(1)),
            (
                "b".into(),
                Value::Map(vec![("c".into(), Value::Bool(true))]),
            ),
        ]),
    ];
    for v in cases {
        let back = bson_to_value(&value_to_bson(&v));
        assert_eq!(back, Some(v.clone()), "round-trip gagal untuk {v:?}");
    }
}

#[test]
fn int32_dari_penulis_lain_diterima_sebagai_int() {
    assert_eq!(bson_to_value(&Bson::Int32(5)), Some(Value::Int(5)));
}

#[test]
fn tipe_bson_tak_dikenal_ditolak() {
    assert_eq!(bson_to_value(&Bson::Undefined), None);
}
```

- [ ] **Step 2: Jalankan tes untuk memastikan gagal**

Run: `cargo test -p arke-mongo --test mapping`
Expected: FAIL saat kompilasi — `unresolved import arke_mongo::bson_to_value`.

- [ ] **Step 3: Tulis implementasi minimal**

Tambahkan ke `arke-mongo/src/bson_map.rs`:

```rust
/// Memetakan BSON kembali ke [`Value`]; `None` bila ada tipe BSON yang tak
/// punya padanan (mis. `ObjectId` di dalam badan komponen, `Undefined`).
///
/// `Int32` diterima meski [`value_to_bson`] tak pernah menghasilkannya —
/// dokumen bisa ditulis service lain atau `mongosh`, yang mengirim bilangan
/// bulat kecil sebagai `Int32`.
pub fn bson_to_value(bson: &Bson) -> Option<Value> {
    Some(match bson {
        Bson::Null => Value::Null,
        Bson::Boolean(b) => Value::Bool(*b),
        Bson::Int64(i) => Value::Int(*i),
        Bson::Int32(i) => Value::Int(i64::from(*i)),
        Bson::Double(f) => Value::Float(*f),
        Bson::String(s) => Value::Text(s.clone()),
        Bson::Array(items) => {
            Value::List(items.iter().map(bson_to_value).collect::<Option<Vec<_>>>()?)
        }
        Bson::Document(doc) => Value::Map(
            doc.iter()
                .map(|(k, v)| bson_to_value(v).map(|v| (k.clone(), v)))
                .collect::<Option<Vec<_>>>()?,
        ),
        _ => return None,
    })
}
```

Di `arke-mongo/src/lib.rs`, ubah baris re-ekspor menjadi:

```rust
pub use bson_map::{bson_to_value, value_to_bson};
```

- [ ] **Step 4: Jalankan tes untuk memastikan lulus**

Run: `cargo test -p arke-mongo --test mapping`
Expected: PASS, 6 tes.

- [ ] **Step 5: Commit**

```bash
git add arke-mongo/src/bson_map.rs arke-mongo/src/lib.rs arke-mongo/tests/mapping.rs
git commit -m "feat(arke-mongo): pemetaan BSON -> Value + round-trip setia (STD-0002)"
```

---

## Task 5: `MongoError` + validasi nama field

Nama field BSON tak boleh mengandung `.` atau berawalan `$` (RFC-0035 §4). Karena nama field datang dari `Serialize` (yang mendukung `rename`/`rename_all`), pelanggaran mungkin terjadi dan harus gagal keras, bukan menghasilkan dokumen yang rusak diam-diam.

**Files:**
- Create: `arke-mongo/src/error.rs`
- Modify: `arke-mongo/src/bson_map.rs`
- Modify: `arke-mongo/src/lib.rs`
- Test: `arke-mongo/tests/mapping.rs`

- [ ] **Step 1: Tulis tes yang gagal**

Tambahkan ke `arke-mongo/tests/mapping.rs`:

```rust
use arke_mongo::{MongoError, validate_names};

#[test]
fn nama_field_valid_diterima() {
    let v = Value::Map(vec![("hp".into(), Value::Int(1))]);
    assert!(validate_names("health", &v).is_ok());
}

#[test]
fn titik_dalam_nama_field_ditolak() {
    let v = Value::Map(vec![("a.b".into(), Value::Int(1))]);
    match validate_names("health", &v) {
        Err(MongoError::InvalidName { component, field }) => {
            assert_eq!(component, "health");
            assert_eq!(field, "a.b");
        }
        other => panic!("harus InvalidName, dapat {other:?}"),
    }
}

#[test]
fn dollar_di_awal_nama_field_ditolak() {
    let v = Value::Map(vec![("$set".into(), Value::Int(1))]);
    assert!(matches!(
        validate_names("health", &v),
        Err(MongoError::InvalidName { .. })
    ));
}

#[test]
fn validasi_menembus_map_bersarang_dan_list() {
    let v = Value::Map(vec![(
        "items".into(),
        Value::List(vec![Value::Map(vec![("bad.name".into(), Value::Null)])]),
    )]);
    assert!(matches!(
        validate_names("inventory", &v),
        Err(MongoError::InvalidName { .. })
    ));
}
```

- [ ] **Step 2: Jalankan tes untuk memastikan gagal**

Run: `cargo test -p arke-mongo --test mapping`
Expected: FAIL saat kompilasi — `unresolved imports arke_mongo::MongoError, arke_mongo::validate_names`.

- [ ] **Step 3: Tulis tipe error**

Buat `arke-mongo/src/error.rs`:

```rust
//! Error adapter, berkonteks (STD-0008): tiap varian menyebut entity dan/atau
//! komponen yang terlibat.

use crate::Pid;

/// Kegagalan operasi `arke-mongo`.
#[derive(Debug)]
pub enum MongoError {
    /// Kegagalan dari driver MongoDB.
    Driver(mongodb::error::Error),
    /// Versi dokumen bergeser sejak dibaca (optimistic-lock, RFC-0035 §5).
    Conflict {
        /// Entity yang gagal ditulis.
        pid: Pid,
        /// Versi yang diharapkan pemanggil.
        expected: i64,
        /// Versi yang sebenarnya ada; `None` bila dokumen sudah terhapus.
        actual: Option<i64>,
    },
    /// Sub-dokumen komponen tak bisa direkonstruksi menjadi tipe Rust-nya.
    Decode {
        /// Entity yang dokumennya gagal dibaca.
        pid: Pid,
        /// Nama komponen (`MongoComponent::NAME`) yang gagal.
        component: &'static str,
    },
    /// Nama field melanggar batas BSON (mengandung `.` atau berawalan `$`).
    InvalidName {
        /// Nama komponen pemilik field.
        component: &'static str,
        /// Nama field yang melanggar.
        field: String,
    },
}

impl From<mongodb::error::Error> for MongoError {
    fn from(e: mongodb::error::Error) -> Self {
        MongoError::Driver(e)
    }
}

impl std::fmt::Display for MongoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MongoError::Driver(e) => write!(f, "kegagalan driver MongoDB: {e}"),
            MongoError::Conflict {
                pid,
                expected,
                actual,
            } => write!(
                f,
                "konflik versi pada entity {pid:?}: diharapkan {expected}, \
                 sebenarnya {actual:?}"
            ),
            MongoError::Decode { pid, component } => write!(
                f,
                "komponen `{component}` pada entity {pid:?} tak bisa \
                 direkonstruksi dari dokumen"
            ),
            MongoError::InvalidName { component, field } => write!(
                f,
                "field `{field}` pada komponen `{component}` bukan nama BSON \
                 yang sah (tak boleh mengandung `.` atau berawalan `$`)"
            ),
        }
    }
}

impl std::error::Error for MongoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MongoError::Driver(e) => Some(e),
            _ => None,
        }
    }
}
```

- [ ] **Step 4: Tambahkan `Pid` ke lib.rs**

`MongoError` merujuk `Pid`, jadi tipe itu harus ada sekarang. Tambahkan ke `arke-mongo/src/lib.rs`:

```rust
use mongodb::bson::oid::ObjectId;

/// Identitas persisten sebuah entity di MongoDB (RFC-0034/RFC-0035 §3).
///
/// `ObjectId` dialokasikan **klien**, jadi `create` cukup satu round-trip dan
/// aman untuk multi-replica tanpa titik kontensi. Berbeda dari `pid` `i64`
/// milik `arke-postgres` — keduanya sumber kebenaran alternatif, bukan dua muka
/// dari satu penyimpanan.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Pid(pub ObjectId);

impl Pid {
    /// Mengalokasikan `pid` baru (klien-side).
    pub fn new() -> Self {
        Pid(ObjectId::new())
    }
}

impl Default for Pid {
    fn default() -> Self {
        Self::new()
    }
}

mod error;
pub use error::MongoError;
```

- [ ] **Step 5: Tulis validasi nama**

Tambahkan ke `arke-mongo/src/bson_map.rs`:

```rust
use crate::MongoError;

/// Memastikan seluruh nama field di dalam `value` sah sebagai nama field BSON:
/// tak mengandung `.` dan tak berawalan `$` (RFC-0035 §4). Rekursif menembus
/// `Map` dan `List`.
pub fn validate_names(component: &'static str, value: &Value) -> Result<(), MongoError> {
    match value {
        Value::Map(entries) => {
            for (key, val) in entries {
                if key.contains('.') || key.starts_with('$') {
                    return Err(MongoError::InvalidName {
                        component,
                        field: key.clone(),
                    });
                }
                validate_names(component, val)?;
            }
            Ok(())
        }
        Value::List(items) => items.iter().try_for_each(|v| validate_names(component, v)),
        _ => Ok(()),
    }
}
```

Di `arke-mongo/src/lib.rs`, ubah re-ekspor `bson_map` menjadi:

```rust
pub use bson_map::{bson_to_value, validate_names, value_to_bson};
```

- [ ] **Step 6: Jalankan tes untuk memastikan lulus**

Run: `cargo test -p arke-mongo --test mapping`
Expected: PASS, 10 tes.

- [ ] **Step 7: Commit**

```bash
git add arke-mongo/src/error.rs arke-mongo/src/bson_map.rs arke-mongo/src/lib.rs arke-mongo/tests/mapping.rs
git commit -m "feat(arke-mongo): MongoError berkonteks + validasi nama field BSON (STD-0008)"
```

---

## Task 6: Trait `MongoComponent`, `IndexDef`, dan makro `mongo_component!`

**Files:**
- Modify: `arke-mongo/src/lib.rs`
- Test: `arke-mongo/tests/mapping.rs`

- [ ] **Step 1: Tulis tes yang gagal**

Tambahkan ke `arke-mongo/tests/mapping.rs`:

```rust
use arke_mongo::{Dir, IndexDef, MongoComponent, mongo_component};

#[derive(arke::Serialize, PartialEq, Debug)]
struct Position {
    x: f32,
    y: f32,
}
mongo_component!(Position => "position");

#[derive(arke::Serialize, PartialEq, Debug)]
struct Health {
    hp: i64,
}
mongo_component!(Health => "health", indexes: [IndexDef::asc("hp")]);

#[test]
fn makro_mengisi_nama_dan_indeks_kosong() {
    assert_eq!(Position::NAME, "position");
    assert!(Position::INDEXES.is_empty());
}

#[test]
fn makro_meneruskan_deklarasi_indeks() {
    assert_eq!(Health::NAME, "health");
    assert_eq!(Health::INDEXES.len(), 1);
    assert_eq!(Health::INDEXES[0].field, "hp");
    assert_eq!(Health::INDEXES[0].dir, Dir::Asc);
    assert!(!Health::INDEXES[0].unique);
}

#[test]
fn index_def_unique_ditandai() {
    const IDX: IndexDef = IndexDef::asc("slug").unique();
    assert!(IDX.unique);
}
```

- [ ] **Step 2: Jalankan tes untuk memastikan gagal**

Run: `cargo test -p arke-mongo --test mapping`
Expected: FAIL saat kompilasi — `unresolved imports arke_mongo::Dir, arke_mongo::IndexDef, arke_mongo::MongoComponent, arke_mongo::mongo_component`.

- [ ] **Step 3: Tulis implementasi minimal**

Tambahkan ke `arke-mongo/src/lib.rs`:

```rust
/// Arah urutan sebuah indeks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    /// Menaik (`1`).
    Asc,
    /// Menurun (`-1`).
    Desc,
}

impl Dir {
    /// Nilai arah sebagaimana dipakai spesifikasi indeks MongoDB.
    pub fn as_i32(self) -> i32 {
        match self {
            Dir::Asc => 1,
            Dir::Desc => -1,
        }
    }
}

/// Deklarasi satu indeks atas sebuah field komponen.
///
/// Indeks dibuat pada path bersarang `cmp.<NAME>.<field>`, sehingga bersifat
/// sparse secara alami: entity tanpa komponen itu tak masuk indeks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexDef {
    /// Nama field di dalam komponen.
    pub field: &'static str,
    /// Arah urutan.
    pub dir: Dir,
    /// Apakah indeks `unique`.
    pub unique: bool,
}

impl IndexDef {
    /// Indeks menaik atas `field`.
    pub const fn asc(field: &'static str) -> Self {
        Self {
            field,
            dir: Dir::Asc,
            unique: false,
        }
    }
    /// Indeks menurun atas `field`.
    pub const fn desc(field: &'static str) -> Self {
        Self {
            field,
            dir: Dir::Desc,
            unique: false,
        }
    }
    /// Menandai indeks ini `unique`.
    pub const fn unique(mut self) -> Self {
        self.unique = true;
        self
    }
}

/// Komponen yang dipersist ke MongoDB (RFC-0035 §2).
///
/// Tak ada derive: `#[derive(arke::Serialize)]` sudah menghasilkan pohon
/// [`arke::Value`] yang dibutuhkan BSON. Trait ini hanya membawa metadata yang
/// tak diketahui `Serialize`. Gunakan [`mongo_component!`] untuk mengisinya.
pub trait MongoComponent: arke::Serialize {
    /// Kunci komponen di bawah `cmp` (mis. `"position"`). Wajib eksplisit —
    /// sanitasi `type_name` tak bisa dilakukan di konteks `const`.
    const NAME: &'static str;
    /// Indeks atas path `cmp.<NAME>.<field>`; kosong bila tak ada.
    const INDEXES: &'static [IndexDef] = &[];
}

/// Mengimplementasikan [`MongoComponent`] untuk sebuah tipe.
///
/// ```
/// # use arke_mongo::{IndexDef, mongo_component};
/// #[derive(arke::Serialize)]
/// struct Position { x: f32, y: f32 }
/// mongo_component!(Position => "position");
///
/// #[derive(arke::Serialize)]
/// struct Health { hp: i64 }
/// mongo_component!(Health => "health", indexes: [IndexDef::asc("hp")]);
/// ```
#[macro_export]
macro_rules! mongo_component {
    ($ty:ty => $name:literal) => {
        impl $crate::MongoComponent for $ty {
            const NAME: &'static str = $name;
        }
    };
    ($ty:ty => $name:literal, indexes: [$($idx:expr),* $(,)?]) => {
        impl $crate::MongoComponent for $ty {
            const NAME: &'static str = $name;
            const INDEXES: &'static [$crate::IndexDef] = &[$($idx),*];
        }
    };
}
```

- [ ] **Step 4: Jalankan tes untuk memastikan lulus**

Run: `cargo test -p arke-mongo --test mapping`
Expected: PASS, 13 tes.

- [ ] **Step 5: Jalankan doctest**

Run: `cargo test -p arke-mongo --doc`
Expected: PASS, 1 doctest (contoh pada `mongo_component!`).

- [ ] **Step 6: Commit**

```bash
git add arke-mongo/src/lib.rs arke-mongo/tests/mapping.rs
git commit -m "feat(arke-mongo): trait MongoComponent + IndexDef + makro mongo_component!"
```

---

## Task 7: Registry type-erased + `cmp_doc`

Meniru `arke-postgres/src/store.rs:32-62`, dengan `Value` menggantikan `Vec<PgValue>`.

**Files:**
- Create: `arke-mongo/src/registry.rs`
- Modify: `arke-mongo/src/lib.rs`
- Test: `arke-mongo/tests/mapping.rs`

- [ ] **Step 1: Tulis tes yang gagal**

Tambahkan ke `arke-mongo/tests/mapping.rs`:

```rust
use arke::World;
use arke_mongo::Registry;

#[test]
fn cmp_doc_memuat_hanya_komponen_yang_dimiliki_entity() {
    let mut reg = Registry::new();
    reg.push::<Position>();
    reg.push::<Health>();

    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Position { x: 1.0, y: 2.0 });

    let doc = reg.cmp_doc(&world, e).expect("cmp_doc harus sukses");
    assert!(doc.contains_key("position"));
    assert!(
        !doc.contains_key("health"),
        "komponen yang tak dimiliki entity tak boleh muncul"
    );

    let pos = doc.get_document("position").unwrap();
    assert_eq!(pos.get_f64("x").unwrap(), 1.0);
}

#[test]
#[should_panic(expected = "position")]
fn nama_komponen_yang_bertabrakan_panic_saat_register() {
    #[derive(arke::Serialize)]
    struct Lain {
        v: i64,
    }
    mongo_component!(Lain => "position");

    let mut reg = Registry::new();
    reg.push::<Position>();
    reg.push::<Lain>();
}

#[derive(arke::Serialize)]
struct IndeksSalah {
    hp: i64,
}
mongo_component!(IndeksSalah => "indeks_salah", indexes: [IndexDef::asc("tidak_ada")]);

/// RFC-0035 §2 (Am. 1): salah-ketik nama field pada `IndexDef` tak tertangkap
/// kompilasi, jadi ditangkap `debug_assert!` saat komponen diserialisasi.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "tidak_ada")]
fn index_def_menyebut_field_yang_tak_ada_gagal_di_build_debug() {
    let mut reg = Registry::new();
    reg.push::<IndeksSalah>();

    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, IndeksSalah { hp: 1 });
    let _ = reg.cmp_doc(&world, e);
}
```

- [ ] **Step 2: Jalankan tes untuk memastikan gagal**

Run: `cargo test -p arke-mongo --test mapping`
Expected: FAIL saat kompilasi — `unresolved import arke_mongo::Registry`.

- [ ] **Step 3: Tulis implementasi minimal**

Buat `arke-mongo/src/registry.rs`:

```rust
//! Registry type-erased komponen terdaftar + pembangunan dokumen (RFC-0035 §5).
//!
//! Seluruh modul ini **murni**: ia tahu `World`, tetapi tak menyentuh driver
//! maupun I/O — sehingga dapat diuji tanpa database.

use arke::{Entity, Value, World};
use mongodb::bson::Document;

use crate::bson_map::{validate_names, value_to_bson};
use crate::{IndexDef, MongoComponent, MongoError};

/// Operasi type-erased untuk satu tipe komponen terdaftar.
struct Registered {
    name: &'static str,
    indexes: &'static [IndexDef],
    /// Nilai komponen `T` milik `entity`, bila ada.
    dump_one: fn(&World, Entity) -> Option<Value>,
}

fn dump_one_of<T: MongoComponent>(world: &World, entity: Entity) -> Option<Value> {
    world.get::<T>(entity).map(arke::Serialize::to_value)
}

/// Memastikan tiap `IndexDef::field` benar-benar ada sebagai field komponen
/// (RFC-0035 §2, Amandemen 1).
///
/// Salah-ketik nama field tak tertangkap kompilasi, dan MongoDB dengan senang
/// hati mengindeks path yang tak pernah terisi — indeks yang diam-diam kosong.
/// `debug_assert!` menangkapnya di build debug dan di seluruh uji, tanpa biaya
/// di rilis dan tanpa menyeret crate logging.
fn debug_check_indexes(name: &'static str, indexes: &[IndexDef], value: &Value) {
    if cfg!(debug_assertions) {
        let Value::Map(entries) = value else {
            return; // komponen non-map tak punya field untuk di-index
        };
        for idx in indexes {
            debug_assert!(
                entries.iter().any(|(k, _)| k == idx.field),
                "IndexDef pada komponen `{name}` menyebut field `{}` yang tak \
                 ada — indeks akan dibuat atas path yang tak pernah terisi",
                idx.field
            );
        }
    }
}

/// Kumpulan tipe komponen yang dipersist.
#[derive(Default)]
pub struct Registry {
    registered: Vec<Registered>,
}

impl Registry {
    /// Registry kosong.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mendaftarkan tipe komponen `T`.
    ///
    /// # Panics
    ///
    /// Panic bila `T::NAME` sudah dipakai komponen lain. Dua komponen dengan
    /// nama sama adalah bug programmer, bukan kegagalan data — gagal sedini
    /// mungkin (RFC-0035 Am. 1).
    pub fn push<T: MongoComponent>(&mut self) {
        assert!(
            !self.registered.iter().any(|r| r.name == T::NAME),
            "nama komponen `{}` sudah terdaftar — tiap MongoComponent::NAME \
             harus unik",
            T::NAME
        );
        self.registered.push(Registered {
            name: T::NAME,
            indexes: T::INDEXES,
            dump_one: dump_one_of::<T>,
        });
    }

    /// Membangun sub-dokumen `cmp` untuk `entity`: satu kunci per komponen
    /// terdaftar yang dimiliki entity itu.
    pub fn cmp_doc(&self, world: &World, entity: Entity) -> Result<Document, MongoError> {
        let mut doc = Document::new();
        for r in &self.registered {
            if let Some(value) = (r.dump_one)(world, entity) {
                debug_check_indexes(r.name, r.indexes, &value);
                validate_names(r.name, &value)?;
                doc.insert(r.name, value_to_bson(&value));
            }
        }
        Ok(doc)
    }
}
```

Di `arke-mongo/src/lib.rs`, tambahkan:

```rust
mod registry;
pub use registry::Registry;
```

`mod bson_map;` tetap privat — `registry.rs` mengaksesnya lewat `crate::bson_map::…`, yang sah karena keduanya di crate yang sama. Yang penting item di dalam `bson_map.rs` sudah `pub` (sudah dilakukan di Task 3–5) agar juga bisa di-re-ekspor dari `lib.rs`.

- [ ] **Step 4: Jalankan tes untuk memastikan lulus**

Run: `cargo test -p arke-mongo --test mapping`
Expected: PASS, 16 tes.

- [ ] **Step 5: Commit**

```bash
git add arke-mongo/src/registry.rs arke-mongo/src/lib.rs arke-mongo/tests/mapping.rs
git commit -m "feat(arke-mongo): registry type-erased + cmp_doc + debug_assert indeks"
```

---

## Task 8: `Registry::apply` — dokumen → World

**Files:**
- Modify: `arke-mongo/src/registry.rs`
- Test: `arke-mongo/tests/mapping.rs`

- [ ] **Step 1: Tulis tes yang gagal**

Tambahkan ke `arke-mongo/tests/mapping.rs`:

```rust
use arke_mongo::Pid;

#[test]
fn apply_menyisipkan_komponen_terdaftar_ke_world() {
    let mut reg = Registry::new();
    reg.push::<Position>();

    let mut src = World::new();
    let a = src.spawn();
    src.insert(a, Position { x: 3.0, y: 4.0 });
    let cmp = reg.cmp_doc(&src, a).unwrap();

    let mut dst = World::new();
    let b = dst.spawn();
    reg.apply(&mut dst, b, Pid::new(), &cmp).unwrap();

    assert_eq!(dst.get::<Position>(b), Some(&Position { x: 3.0, y: 4.0 }));
}

#[test]
fn apply_mengabaikan_komponen_yang_tak_terdaftar() {
    let mut reg = Registry::new();
    reg.push::<Position>();

    let mut cmp = Document::new();
    cmp.insert("tak_dikenal", Document::new());

    let mut world = World::new();
    let e = world.spawn();
    assert!(
        reg.apply(&mut world, e, Pid::new(), &cmp).is_ok(),
        "komponen milik service lain tak boleh menggagalkan pembacaan"
    );
}

#[test]
fn apply_gagal_keras_saat_bentuk_komponen_tak_cocok() {
    let mut reg = Registry::new();
    reg.push::<Position>();

    let mut bad = Document::new();
    bad.insert("x", "bukan angka");
    let mut cmp = Document::new();
    cmp.insert("position", bad);

    let mut world = World::new();
    let e = world.spawn();
    let pid = Pid::new();
    match reg.apply(&mut world, e, pid, &cmp) {
        Err(MongoError::Decode { component, .. }) => assert_eq!(component, "position"),
        other => panic!("harus Decode, dapat {other:?}"),
    }
}
```

- [ ] **Step 2: Jalankan tes untuk memastikan gagal**

Run: `cargo test -p arke-mongo --test mapping`
Expected: FAIL saat kompilasi — `no method named apply found for struct Registry`.

- [ ] **Step 3: Tulis implementasi minimal**

Di `arke-mongo/src/registry.rs`, tambahkan field `apply` ke `Registered`:

```rust
struct Registered {
    name: &'static str,
    /// Nilai komponen `T` milik `entity`, bila ada.
    dump_one: fn(&World, Entity) -> Option<Value>,
    /// Rekonstruksi komponen dari `Value` lalu sisipkan; `false` bila bentuknya
    /// tak cocok.
    apply: fn(&mut World, Entity, &Value) -> bool,
}

fn apply_of<T: MongoComponent>(world: &mut World, entity: Entity, value: &Value) -> bool {
    match T::from_value(value) {
        Some(component) => {
            world.insert(entity, component);
            true
        }
        None => false,
    }
}
```

Isi field itu di `push`:

```rust
        self.registered.push(Registered {
            name: T::NAME,
            dump_one: dump_one_of::<T>,
            apply: apply_of::<T>,
        });
```

Tambahkan `use crate::bson_map::bson_to_value;` ke daftar import, lalu tambahkan method:

```rust
    /// Menyisipkan komponen dari sub-dokumen `cmp` ke `entity`.
    ///
    /// Kunci yang **tak terdaftar diabaikan** — dokumen bisa ditulis service
    /// lain atau versi aplikasi lain, dan kehadirannya bukan kesalahan.
    /// Komponen terdaftar yang bentuknya tak cocok menghasilkan
    /// [`MongoError::Decode`], **bukan** dilewati diam-diam: kehilangan
    /// komponen tanpa suara akan lolos ke `save` berikutnya dan menjadi
    /// kehilangan data permanen (RFC-0035 §6).
    pub fn apply(
        &self,
        world: &mut World,
        entity: Entity,
        pid: crate::Pid,
        cmp: &Document,
    ) -> Result<(), MongoError> {
        for r in &self.registered {
            let Some(bson) = cmp.get(r.name) else {
                continue;
            };
            let value = bson_to_value(bson).ok_or(MongoError::Decode {
                pid,
                component: r.name,
            })?;
            if !(r.apply)(world, entity, &value) {
                return Err(MongoError::Decode {
                    pid,
                    component: r.name,
                });
            }
        }
        Ok(())
    }
```

- [ ] **Step 4: Jalankan tes untuk memastikan lulus**

Run: `cargo test -p arke-mongo --test mapping`
Expected: PASS, 19 tes.

- [ ] **Step 5: Commit**

```bash
git add arke-mongo/src/registry.rs arke-mongo/tests/mapping.rs
git commit -m "feat(arke-mongo): Registry::apply — dokumen -> World, decode gagal keras"
```

---

## Task 9: `Registry::update_ops` — `$set` / `$unset` / `$inc`

Keputusan paling penting di task ini: `$set` menyasar sub-field (`cmp.position`), **bukan** mengganti `cmp` utuh. Mengganti `cmp` utuh akan menghapus diam-diam komponen yang ditulis service lain (RFC-0035 §5).

**Files:**
- Modify: `arke-mongo/src/registry.rs`
- Test: `arke-mongo/tests/mapping.rs`

- [ ] **Step 1: Tulis tes yang gagal**

Tambahkan ke `arke-mongo/tests/mapping.rs`:

```rust
#[test]
fn update_ops_set_per_sub_field_bukan_mengganti_cmp() {
    let mut reg = Registry::new();
    reg.push::<Position>();
    reg.push::<Health>();

    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Position { x: 1.0, y: 2.0 });

    let ops = reg.update_ops(&world, e).unwrap();
    let set = ops.get_document("$set").unwrap();
    assert!(
        set.contains_key("cmp.position"),
        "harus menyasar sub-field, bukan `cmp`"
    );
    assert!(
        !set.contains_key("cmp"),
        "mengganti `cmp` utuh akan menghapus komponen milik service lain"
    );
}

#[test]
fn update_ops_unset_komponen_terdaftar_yang_hilang() {
    let mut reg = Registry::new();
    reg.push::<Position>();
    reg.push::<Health>();

    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Position { x: 1.0, y: 2.0 });

    let ops = reg.update_ops(&world, e).unwrap();
    let unset = ops.get_document("$unset").unwrap();
    assert!(
        unset.contains_key("cmp.health"),
        "komponen terdaftar yang tak dimiliki entity harus di-unset"
    );
}

#[test]
fn update_ops_menaikkan_version_dengan_inc() {
    let mut reg = Registry::new();
    reg.push::<Position>();

    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Position { x: 0.0, y: 0.0 });

    let ops = reg.update_ops(&world, e).unwrap();
    assert_eq!(ops.get_document("$inc").unwrap().get_i64("version"), Ok(1));
}
```

- [ ] **Step 2: Jalankan tes untuk memastikan gagal**

Run: `cargo test -p arke-mongo --test mapping`
Expected: FAIL saat kompilasi — `no method named update_ops found for struct Registry`.

- [ ] **Step 3: Tulis implementasi minimal**

Tambahkan ke `arke-mongo/src/registry.rs` (butuh `use mongodb::bson::doc;` di import):

```rust
    /// Membangun dokumen update untuk `entity`: `$set` per sub-field komponen
    /// yang dimiliki, `$unset` untuk komponen **terdaftar** yang hilang, dan
    /// `$inc` pada `version`.
    ///
    /// `$set` sengaja menyasar `cmp.<nama>` alih-alih mengganti `cmp` utuh —
    /// mengganti utuh akan menghapus diam-diam komponen yang ditulis service
    /// lain atau versi aplikasi lain (RFC-0035 §5). Karena itu `$unset` pun
    /// hanya menyasar komponen terdaftar.
    pub fn update_ops(&self, world: &World, entity: Entity) -> Result<Document, MongoError> {
        let mut set = Document::new();
        let mut unset = Document::new();
        for r in &self.registered {
            let path = format!("cmp.{}", r.name);
            match (r.dump_one)(world, entity) {
                Some(value) => {
                    debug_check_indexes(r.name, r.indexes, &value);
                    validate_names(r.name, &value)?;
                    set.insert(path, value_to_bson(&value));
                }
                None => {
                    unset.insert(path, "");
                }
            }
        }
        let mut ops = doc! { "$inc": { "version": 1i64 } };
        if !set.is_empty() {
            ops.insert("$set", set);
        }
        if !unset.is_empty() {
            ops.insert("$unset", unset);
        }
        Ok(ops)
    }
```

- [ ] **Step 4: Jalankan tes untuk memastikan lulus**

Run: `cargo test -p arke-mongo --test mapping`
Expected: PASS, 22 tes.

- [ ] **Step 5: Commit**

```bash
git add arke-mongo/src/registry.rs arke-mongo/tests/mapping.rs
git commit -m "feat(arke-mongo): update_ops per sub-field ($set/$unset/$inc)"
```

---

## Task 10: `Registry::index_models`

**Files:**
- Modify: `arke-mongo/src/registry.rs`
- Test: `arke-mongo/tests/mapping.rs`

- [ ] **Step 1: Tulis tes yang gagal**

Tambahkan ke `arke-mongo/tests/mapping.rs`:

```rust
#[test]
fn index_models_memakai_path_bersarang() {
    let mut reg = Registry::new();
    reg.push::<Health>(); // IndexDef::asc("hp")

    let models = reg.index_models();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].keys.get_i32("cmp.health.hp"), Ok(1));
}

#[test]
fn komponen_tanpa_indeks_tak_menghasilkan_model() {
    let mut reg = Registry::new();
    reg.push::<Position>();
    assert!(reg.index_models().is_empty());
}
```

- [ ] **Step 2: Jalankan tes untuk memastikan gagal**

Run: `cargo test -p arke-mongo --test mapping`
Expected: FAIL saat kompilasi — `no method named index_models found for struct Registry`.

- [ ] **Step 3: Tulis implementasi minimal**

Field `indexes` sudah ada pada `Registered` sejak Task 7 (dipakai `debug_check_indexes`), jadi task ini hanya menambah satu method.

Tambahkan import `use mongodb::{IndexModel, options::IndexOptions};` ke `arke-mongo/src/registry.rs`, lalu method:

```rust
    /// Spesifikasi indeks untuk seluruh komponen terdaftar, atas path bersarang
    /// `cmp.<nama>.<field>`.
    pub fn index_models(&self) -> Vec<IndexModel> {
        let mut models = Vec::new();
        for r in &self.registered {
            for idx in r.indexes {
                let key = format!("cmp.{}.{}", r.name, idx.field);
                let mut opts = IndexOptions::default();
                if idx.unique {
                    opts.unique = Some(true);
                }
                models.push(
                    IndexModel::builder()
                        .keys(doc! { key: idx.dir.as_i32() })
                        .options(opts)
                        .build(),
                );
            }
        }
        models
    }
```

- [ ] **Step 4: Jalankan tes untuk memastikan lulus**

Run: `cargo test -p arke-mongo --test mapping`
Expected: PASS, 24 tes.

- [ ] **Step 5: Jalankan clippy dan fmt**

Run: `cargo fmt -p arke-mongo && cargo clippy -p arke-mongo --all-targets -- -D warnings`
Expected: bersih. Perbaiki apa pun yang dikeluhkan sebelum lanjut — CI memakai `RUSTFLAGS: -D warnings`.

- [ ] **Step 6: Commit**

```bash
git add arke-mongo/src/registry.rs arke-mongo/tests/mapping.rs
git commit -m "feat(arke-mongo): index_models atas path cmp.<nama>.<field>"
```

---

## Task 11: `MongoStore::connect` + `register` + `ensure_indexes`

Mulai di sini semua tes butuh MongoDB nyata dan di-skip bila `MONGODB_URI` tak diset.

**Jalankan MongoDB lokal untuk pengembangan:**

```bash
docker run -d --name arke-mongo-dev -p 27017:27017 mongo:7
export MONGODB_URI=mongodb://localhost:27017
```

**Files:**
- Create: `arke-mongo/src/store.rs`
- Modify: `arke-mongo/src/lib.rs`
- Test: `arke-mongo/tests/store.rs`

- [ ] **Step 1: Tulis tes yang gagal**

Buat `arke-mongo/tests/store.rs`:

```rust
//! Lapis 2 (RFC-0035 §7): uji `MongoStore` terhadap MongoDB nyata.
//!
//! Dilewati (skip) bila `MONGODB_URI` tak diset — sehingga CI tanpa MongoDB
//! tetap hijau; job `mongo` di CI menyetel env ini. Tiap tes memakai database
//! sendiri agar tak saling mengganggu.

use arke::World;
use arke_mongo::{IndexDef, MongoStore, mongo_component};

#[derive(arke::Serialize, PartialEq, Debug)]
struct Position {
    x: f32,
    y: f32,
}
mongo_component!(Position => "position");

#[derive(arke::Serialize, PartialEq, Debug)]
struct Health {
    hp: i64,
}
mongo_component!(Health => "health", indexes: [IndexDef::asc("hp")]);

/// URI uji, atau `None` bila env tak diset (tes di-skip).
fn uri() -> Option<String> {
    std::env::var("MONGODB_URI").ok()
}

/// Store bersih pada database bernama `db_name` (di-drop lebih dulu).
async fn store(db_name: &str) -> Option<MongoStore> {
    let uri = uri()?;
    let mut s = MongoStore::connect(&uri, db_name).await.unwrap();
    s.drop_database().await.unwrap();
    s.register::<Position>();
    s.register::<Health>();
    Some(s)
}

#[tokio::test]
async fn ensure_indexes_idempoten() {
    let Some(s) = store("arke_test_indexes").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };
    s.ensure_indexes().await.unwrap();
    s.ensure_indexes().await.unwrap();
}
```

- [ ] **Step 2: Jalankan tes untuk memastikan gagal**

Run: `cargo test -p arke-mongo --test store`
Expected: FAIL saat kompilasi — `unresolved import arke_mongo::MongoStore`.

- [ ] **Step 3: Tulis implementasi minimal**

Buat `arke-mongo/src/store.rs`:

```rust
//! [`MongoStore`]: persistensi async `World` ↔ MongoDB (RFC-0035 §5).
//!
//! Satu-satunya modul yang menyentuh I/O; seluruh logika pemetaan hidup di
//! `bson_map` dan `registry` sebagai fungsi murni.

use std::collections::HashMap;

use arke::{Entity, World};
use mongodb::bson::{Document, doc};
use mongodb::{Client, Collection, Database};

use crate::registry::Registry;
use crate::{MongoComponent, MongoError, Pid};

/// Versi format dokumen (STD-0001), disimpan di koleksi `arke_meta`.
const SCHEMA_VERSION: i64 = 1;

/// Nama koleksi utama.
const ENTITIES: &str = "arke_entities";

/// Penyimpan MongoDB untuk keadaan ECS (RFC-0035).
///
/// Daftarkan tiap tipe komponen via [`Self::register`], panggil
/// [`Self::ensure_indexes`], lalu pakai jalur per-operasi
/// (`create`/`fetch`/`update`/`remove`) atau jalur seluruh World
/// (`save`/`load`).
pub struct MongoStore {
    db: Database,
    entities: Collection<Document>,
    reg: Registry,
    /// Jembatan Entity (indeks ephemeral) → `pid` persisten (RFC-0034 §2).
    pid_of: HashMap<Entity, Pid>,
    /// Jembatan `pid` → Entity (handle lokal working-set).
    entity_of: HashMap<Pid, Entity>,
}

impl MongoStore {
    /// Menyambung ke MongoDB pada `uri`, memakai database `db_name`.
    pub async fn connect(uri: &str, db_name: &str) -> Result<Self, MongoError> {
        let client = Client::with_uri_str(uri).await?;
        let db = client.database(db_name);
        let entities = db.collection::<Document>(ENTITIES);
        Ok(Self {
            db,
            entities,
            reg: Registry::new(),
            pid_of: HashMap::new(),
            entity_of: HashMap::new(),
        })
    }

    /// Mendaftarkan tipe komponen `T` untuk dipersist.
    ///
    /// # Panics
    ///
    /// Panic bila `T::NAME` sudah dipakai komponen lain (RFC-0035 Am. 1).
    pub fn register<T: MongoComponent>(&mut self) -> &mut Self {
        self.reg.push::<T>();
        self
    }

    /// Membuat indeks untuk seluruh komponen terdaftar dan mencatat
    /// `schema_version` (STD-0001). Idempoten — aman dipanggil tiap start-up.
    pub async fn ensure_indexes(&self) -> Result<(), MongoError> {
        let models = self.reg.index_models();
        if !models.is_empty() {
            self.entities.create_indexes(models).await?;
        }
        self.db
            .collection::<Document>("arke_meta")
            .update_one(
                doc! { "_id": "schema" },
                doc! { "$set": { "schema_version": SCHEMA_VERSION } },
            )
            .upsert(true)
            .await?;
        Ok(())
    }

    /// Menghapus seluruh database. **Hanya untuk uji** — merusak data.
    #[doc(hidden)]
    pub async fn drop_database(&mut self) -> Result<(), MongoError> {
        self.db.drop().await?;
        self.pid_of.clear();
        self.entity_of.clear();
        Ok(())
    }
}
```

Di `arke-mongo/src/lib.rs`, tambahkan:

```rust
mod store;
pub use store::MongoStore;
```

- [ ] **Step 4: Jalankan tes untuk memastikan lulus**

Run: `MONGODB_URI=mongodb://localhost:27017 cargo test -p arke-mongo --test store`
Expected: PASS, 1 tes.

- [ ] **Step 5: Verifikasi tes di-skip tanpa env**

Run: `env -u MONGODB_URI cargo test -p arke-mongo --test store`
Expected: PASS, 1 tes (dengan pesan "MONGODB_URI tak diset — tes dilewati"). Ini gerbang penting: CI default tak punya MongoDB.

- [ ] **Step 6: Commit**

```bash
git add arke-mongo/src/store.rs arke-mongo/src/lib.rs arke-mongo/tests/store.rs
git commit -m "feat(arke-mongo): MongoStore connect/register/ensure_indexes"
```

---

## Task 12: `create` dan `fetch`

**Files:**
- Modify: `arke-mongo/src/store.rs`
- Test: `arke-mongo/tests/store.rs`

- [ ] **Step 1: Tulis tes yang gagal**

Tambahkan ke `arke-mongo/tests/store.rs`:

```rust
#[tokio::test]
async fn create_lalu_fetch_round_trip_setia() {
    let Some(mut s) = store("arke_test_create_fetch").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };

    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, Position { x: 1.5, y: -2.5 });
    w.insert(e, Health { hp: 77 });
    let pid = s.create(&w, e).await.unwrap();

    // World baru: materialisasi dari MongoDB.
    let mut w2 = World::new();
    let e2 = s.fetch(&mut w2, pid).await.unwrap().expect("entity harus ada");

    assert_eq!(w2.get::<Position>(e2), Some(&Position { x: 1.5, y: -2.5 }));
    assert_eq!(w2.get::<Health>(e2), Some(&Health { hp: 77 }));
}

#[tokio::test]
async fn fetch_pid_tak_dikenal_mengembalikan_none() {
    let Some(mut s) = store("arke_test_fetch_none").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };
    let mut w = World::new();
    assert!(s.fetch(&mut w, arke_mongo::Pid::new()).await.unwrap().is_none());
}
```

- [ ] **Step 2: Jalankan tes untuk memastikan gagal**

Run: `MONGODB_URI=mongodb://localhost:27017 cargo test -p arke-mongo --test store`
Expected: FAIL saat kompilasi — `no method named create found for struct MongoStore`.

- [ ] **Step 3: Tulis implementasi minimal**

Tambahkan ke `impl MongoStore` di `arke-mongo/src/store.rs`:

```rust
    /// Menyimpan `entity` sebagai dokumen baru dan mengembalikan `pid`-nya.
    ///
    /// `pid` dialokasikan di sisi klien, jadi operasi ini cukup satu
    /// round-trip dan aman untuk multi-replica (RFC-0035 §3).
    pub async fn create(&mut self, world: &World, entity: Entity) -> Result<Pid, MongoError> {
        let cmp = self.reg.cmp_doc(world, entity)?;
        let pid = Pid::new();
        self.entities
            .insert_one(doc! { "_id": pid.0, "version": 0i64, "cmp": cmp })
            .await?;
        self.pid_of.insert(entity, pid);
        self.entity_of.insert(pid, entity);
        Ok(pid)
    }

    /// Memuat dokumen `pid` ke `world` sebagai entity baru; `None` bila dokumen
    /// tak ada.
    ///
    /// Bila sebuah komponen gagal di-decode, entity yang telanjur di-spawn
    /// dibuang lagi supaya `world` tak meninggalkan entity separuh terisi.
    pub async fn fetch(
        &mut self,
        world: &mut World,
        pid: Pid,
    ) -> Result<Option<Entity>, MongoError> {
        let Some(document) = self.entities.find_one(doc! { "_id": pid.0 }).await? else {
            return Ok(None);
        };
        let entity = world.spawn();
        if let Ok(cmp) = document.get_document("cmp")
            && let Err(e) = self.reg.apply(world, entity, pid, cmp)
        {
            world.despawn(entity);
            return Err(e);
        }
        self.pid_of.insert(entity, pid);
        self.entity_of.insert(pid, entity);
        Ok(Some(entity))
    }
```

Catatan: `let … && let …` adalah *let-chain*, stabil sejak Rust 1.88 (MSRV repo ini). Bila toolchain menolaknya, pecah menjadi `if let Ok(cmp) = … { if let Err(e) = … { … } }`.

- [ ] **Step 4: Jalankan tes untuk memastikan lulus**

Run: `MONGODB_URI=mongodb://localhost:27017 cargo test -p arke-mongo --test store`
Expected: PASS, 3 tes.

- [ ] **Step 5: Commit**

```bash
git add arke-mongo/src/store.rs arke-mongo/tests/store.rs
git commit -m "feat(arke-mongo): create/fetch per-operasi (RFC-0034 pola pid)"
```

---

## Task 13: `update`, `update_checked`, `version_of`, `remove`

**Files:**
- Modify: `arke-mongo/src/store.rs`
- Test: `arke-mongo/tests/store.rs`

- [ ] **Step 1: Tulis tes yang gagal**

Tambahkan ke `arke-mongo/tests/store.rs`:

```rust
use arke_mongo::MongoError;

#[tokio::test]
async fn update_menulis_nilai_baru_dan_menaikkan_version() {
    let Some(mut s) = store("arke_test_update").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };

    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, Position { x: 0.0, y: 0.0 });
    let pid = s.create(&w, e).await.unwrap();
    assert_eq!(s.version_of(pid).await.unwrap(), Some(0));

    w.insert(e, Position { x: 9.0, y: 9.0 });
    s.update(&w, e, pid).await.unwrap();
    assert_eq!(s.version_of(pid).await.unwrap(), Some(1));

    let mut w2 = World::new();
    let e2 = s.fetch(&mut w2, pid).await.unwrap().unwrap();
    assert_eq!(w2.get::<Position>(e2), Some(&Position { x: 9.0, y: 9.0 }));
}

#[tokio::test]
async fn update_checked_mendeteksi_konflik_versi() {
    let Some(mut s) = store("arke_test_conflict").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };

    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, Health { hp: 10 });
    let pid = s.create(&w, e).await.unwrap();

    // Penulis lain menaikkan versi lebih dulu.
    s.update(&w, e, pid).await.unwrap();

    // Kita masih memegang harapan versi 0 → konflik.
    match s.update_checked(&w, e, pid, 0).await {
        Err(MongoError::Conflict { expected, actual, .. }) => {
            assert_eq!(expected, 0);
            assert_eq!(actual, Some(1));
        }
        other => panic!("harus Conflict, dapat {other:?}"),
    }
}

#[tokio::test]
async fn update_checked_sukses_mengembalikan_versi_baru() {
    let Some(mut s) = store("arke_test_checked_ok").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };

    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, Health { hp: 1 });
    let pid = s.create(&w, e).await.unwrap();

    assert_eq!(s.update_checked(&w, e, pid, 0).await.unwrap(), 1);
}

#[tokio::test]
async fn remove_menghapus_dokumen() {
    let Some(mut s) = store("arke_test_remove").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };

    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, Health { hp: 3 });
    let pid = s.create(&w, e).await.unwrap();

    s.remove(pid).await.unwrap();
    assert_eq!(s.version_of(pid).await.unwrap(), None);
}
```

- [ ] **Step 2: Jalankan tes untuk memastikan gagal**

Run: `MONGODB_URI=mongodb://localhost:27017 cargo test -p arke-mongo --test store`
Expected: FAIL saat kompilasi — `no method named version_of found for struct MongoStore`.

- [ ] **Step 3: Tulis implementasi minimal**

Tambahkan `use mongodb::options::ReturnDocument;` ke import `store.rs`, lalu tambahkan ke `impl MongoStore`:

```rust
    /// Menulis keadaan `entity` ke dokumen `pid` (last-write-wins) dan
    /// menaikkan `version`.
    pub async fn update(
        &mut self,
        world: &World,
        entity: Entity,
        pid: Pid,
    ) -> Result<(), MongoError> {
        let ops = self.reg.update_ops(world, entity)?;
        self.entities.update_one(doc! { "_id": pid.0 }, ops).await?;
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
    pub async fn version_of(&self, pid: Pid) -> Result<Option<i64>, MongoError> {
        Ok(self
            .entities
            .find_one(doc! { "_id": pid.0 })
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
```

- [ ] **Step 4: Jalankan tes untuk memastikan lulus**

Run: `MONGODB_URI=mongodb://localhost:27017 cargo test -p arke-mongo --test store`
Expected: PASS, 7 tes.

- [ ] **Step 5: Commit**

```bash
git add arke-mongo/src/store.rs arke-mongo/tests/store.rs
git commit -m "feat(arke-mongo): update/update_checked/version_of/remove + optimistic-lock"
```

---

## Task 14: `save` seluruh World

**Files:**
- Modify: `arke-mongo/src/store.rs`
- Test: `arke-mongo/tests/store.rs`

- [ ] **Step 1: Tulis tes yang gagal**

Tambahkan ke `arke-mongo/tests/store.rs`:

```rust
use arke_mongo::bson::doc;

#[tokio::test]
async fn save_menulis_entity_baru_dan_menghapus_yang_despawn() {
    let Some(mut s) = store("arke_test_save").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };

    let mut w = World::new();
    let a = w.spawn();
    w.insert(a, Health { hp: 1 });
    let b = w.spawn();
    w.insert(b, Health { hp: 2 });
    s.save(&w).await.unwrap();

    let pid_b = s.pid_of(b).expect("b harus punya pid setelah save");
    w.despawn(b);
    s.save(&w).await.unwrap();

    assert_eq!(s.version_of(pid_b).await.unwrap(), None, "b harus terhapus");
    let pid_a = s.pid_of(a).unwrap();
    assert!(s.version_of(pid_a).await.unwrap().is_some(), "a harus tetap");
}

#[tokio::test]
async fn save_tidak_menghapus_komponen_yang_tak_terdaftar() {
    let Some(mut s) = store("arke_test_foreign").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };

    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, Health { hp: 5 });
    let pid = s.create(&w, e).await.unwrap();

    // Service lain menulis komponennya sendiri ke dokumen yang sama.
    s.collection()
        .update_one(
            doc! { "_id": pid.0 },
            doc! { "$set": { "cmp.dari_service_lain": { "v": 1i64 } } },
        )
        .await
        .unwrap();

    w.insert(e, Health { hp: 6 });
    s.save(&w).await.unwrap();

    let d = s.collection().find_one(doc! { "_id": pid.0 }).await.unwrap().unwrap();
    assert!(
        d.get_document("cmp").unwrap().contains_key("dari_service_lain"),
        "save tak boleh menghapus komponen milik penulis lain"
    );
}
```

- [ ] **Step 2: Jalankan tes untuk memastikan gagal**

Run: `MONGODB_URI=mongodb://localhost:27017 cargo test -p arke-mongo --test store`
Expected: FAIL saat kompilasi — `no method named save found for struct MongoStore`.

- [ ] **Step 3: Tulis implementasi minimal**

Tambahkan `use arke::QueryData;` ke import `store.rs`, lalu tambahkan ke `impl MongoStore`:

```rust
    /// `pid` milik `entity` pada working-set aktif, bila sudah dipetakan.
    pub fn pid_of(&self, entity: Entity) -> Option<Pid> {
        self.pid_of.get(&entity).copied()
    }

    /// Koleksi `arke_entities` mentah — jalan keluar untuk query yang belum
    /// dilayani API ini (query builder ditunda ke RFC lanjutan).
    pub fn collection(&self) -> &Collection<Document> {
        &self.entities
    }

    /// Menulis seluruh working-set: upsert tiap entity hidup, lalu hapus
    /// dokumen milik entity yang sudah tak ada di `world`.
    ///
    /// # Atomisitas
    ///
    /// Operasi per-dokumen berurutan **tanpa transaksi**: atomik **per-entity**,
    /// **bukan** per-World. Kegagalan di tengah meninggalkan sebagian entity
    /// tertulis. Ini konsekuensi sadar agar `mongod` standalone cukup — transaksi
    /// multi-dokumen MongoDB menuntut replica set. Janji ini **lebih lemah**
    /// daripada `arke-postgres::PgStore::save`, yang transaksional penuh
    /// (RFC-0035 §5, Amandemen 1).
    pub async fn save(&mut self, world: &World) -> Result<(), MongoError> {
        // Kumpulkan entity hidup lebih dulu (sinkron) agar `&World` tak ditahan
        // melewati `.await`.
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

        let mut still_live: Vec<Pid> = Vec::with_capacity(ops.len());
        for (entity, existing, update) in ops {
            let pid = existing.unwrap_or_else(Pid::new);
            self.entities
                .update_one(doc! { "_id": pid.0 }, update)
                .upsert(true)
                .await?;
            self.pid_of.insert(entity, pid);
            self.entity_of.insert(pid, entity);
            still_live.push(pid);
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
```

Catatan: `upsert(true)` pada dokumen baru menghasilkan `version: 1` (karena `$inc` berjalan dari ketiadaan), bukan `0` seperti `create`. Itu tak masalah — `version` hanya dipakai sebagai penanda perubahan monotonik, bukan hitungan tulis.

- [ ] **Step 4: Jalankan tes untuk memastikan lulus**

Run: `MONGODB_URI=mongodb://localhost:27017 cargo test -p arke-mongo --test store`
Expected: PASS, 9 tes.

- [ ] **Step 5: Commit**

```bash
git add arke-mongo/src/store.rs arke-mongo/tests/store.rs
git commit -m "feat(arke-mongo): save seluruh World (upsert per-entity + hapus despawn)"
```

---

## Task 15: `load` deterministik

**Files:**
- Modify: `arke-mongo/src/store.rs`
- Test: `arke-mongo/tests/store.rs`

- [ ] **Step 1: Tulis tes yang gagal**

Tambahkan ke `arke-mongo/tests/store.rs`:

```rust
#[tokio::test]
async fn load_memuat_seluruh_koleksi() {
    let Some(mut s) = store("arke_test_load").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };

    let mut w = World::new();
    for hp in 1..=3i64 {
        let e = w.spawn();
        w.insert(e, Health { hp });
    }
    s.save(&w).await.unwrap();

    let mut w2 = World::new();
    s.load(&mut w2).await.unwrap();

    let mut hps: Vec<i64> = Vec::new();
    <(arke::Entity, &Health)>::each_filtered_shared::<()>(&w2, |(_, h)| hps.push(h.hp));
    hps.sort_unstable();
    assert_eq!(hps, vec![1, 2, 3]);
}

#[tokio::test]
async fn load_deterministik_antar_materialisasi() {
    let Some(mut s) = store("arke_test_load_order").await else {
        eprintln!("MONGODB_URI tak diset — tes dilewati");
        return;
    };

    let mut w = World::new();
    for hp in 1..=5i64 {
        let e = w.spawn();
        w.insert(e, Health { hp });
    }
    s.save(&w).await.unwrap();

    let order = |world: &World| {
        let mut v = Vec::new();
        <(arke::Entity, &Health)>::each_filtered_shared::<()>(world, |(_, h)| v.push(h.hp));
        v
    };

    let mut a = World::new();
    s.load(&mut a).await.unwrap();
    let mut b = World::new();
    s.load(&mut b).await.unwrap();

    assert_eq!(order(&a), order(&b), "urutan materialisasi harus identik (STD-0005)");
}
```

Tambahkan `use arke::QueryData;` di bagian import berkas tes.

- [ ] **Step 2: Jalankan tes untuk memastikan gagal**

Run: `MONGODB_URI=mongodb://localhost:27017 cargo test -p arke-mongo --test store`
Expected: FAIL saat kompilasi — `no method named load found for struct MongoStore`.

- [ ] **Step 3: Tulis implementasi minimal**

Tambahkan `use futures_util::TryStreamExt;` ke import `store.rs`, lalu tambahkan ke `impl MongoStore`:

```rust
    /// Memuat seluruh koleksi ke `world` sebagai working-set.
    ///
    /// Diurutkan `_id` menaik supaya urutan materialisasi identik antar-run
    /// (STD-0005) — sejajar `ORDER BY pid` di `arke-postgres`.
    ///
    /// Memuat **seluruh** koleksi tanpa paging; materialisasi parsial
    /// (`load_where`) ditunda ke RFC lanjutan.
    pub async fn load(&mut self, world: &mut World) -> Result<(), MongoError> {
        let mut cursor = self
            .entities
            .find(doc! {})
            .sort(doc! { "_id": 1 })
            .await?;
        while let Some(document) = cursor.try_next().await? {
            let Ok(oid) = document.get_object_id("_id") else {
                continue; // dokumen dengan `_id` non-ObjectId bukan milik arke
            };
            let pid = Pid(oid);
            let entity = world.spawn();
            if let Ok(cmp) = document.get_document("cmp")
                && let Err(e) = self.reg.apply(world, entity, pid, cmp)
            {
                world.despawn(entity);
                return Err(e);
            }
            self.pid_of.insert(entity, pid);
            self.entity_of.insert(pid, entity);
        }
        Ok(())
    }
```

- [ ] **Step 4: Jalankan tes untuk memastikan lulus**

Run: `MONGODB_URI=mongodb://localhost:27017 cargo test -p arke-mongo --test store`
Expected: PASS, 11 tes.

- [ ] **Step 5: Jalankan seluruh tes + lint**

Run: `cargo fmt --all && cargo clippy -p arke-mongo --all-targets -- -D warnings && MONGODB_URI=mongodb://localhost:27017 cargo test -p arke-mongo`
Expected: semua hijau (24 tes mapping + 11 tes store + doctest).

- [ ] **Step 6: Commit**

```bash
git add arke-mongo/src/store.rs arke-mongo/tests/store.rs
git commit -m "feat(arke-mongo): load deterministik (sort _id, STD-0005)"
```

---

## Task 16: README crate

**Files:**
- Create: `arke-mongo/README.md`

- [ ] **Step 1: Tulis README**

Buat `arke-mongo/README.md`:

````markdown
# arke-mongo

Adapter MongoDB untuk ECS [`arke`](https://crates.io/crates/arke): persistensi
**satu dokumen per entity**, dengan MongoDB sebagai **sumber kebenaran**
(RFC-0035).

## Model data

Satu koleksi `arke_entities`, satu dokumen per entity:

```js
{
  _id: ObjectId("..."),   // pid — identitas persisten
  version: 3,             // optimistic lock
  cmp: {
    position: { x: 1.0, y: 2.0 },
    health:   { hp: 80 }
  }
}
```

Query lintas-komponen cukup satu `find`, tanpa join:

```js
db.arke_entities.find({ "cmp.position.x": { $gt: 100 }, "cmp.health.hp": { $lt: 20 } })
```

## Pemakaian

```rust
use arke::World;
use arke_mongo::{IndexDef, MongoStore, mongo_component};

#[derive(arke::Serialize)]
struct Position { x: f32, y: f32 }
mongo_component!(Position => "position");

#[derive(arke::Serialize)]
struct Health { hp: i64 }
mongo_component!(Health => "health", indexes: [IndexDef::asc("hp")]);

# async fn contoh() -> Result<(), arke_mongo::MongoError> {
let mut store = MongoStore::connect("mongodb://localhost:27017", "game").await?;
store.register::<Position>();
store.register::<Health>();
store.ensure_indexes().await?;

let mut world = World::new();
let e = world.spawn();
world.insert(e, Position { x: 1.0, y: 2.0 });

let pid = store.create(&world, e).await?;      // satu round-trip
store.update(&world, e, pid).await?;           // last-write-wins
store.save(&world).await?;                     // seluruh working-set
# Ok(())
# }
```

Tak ada derive khusus: `#[derive(arke::Serialize)]` sudah menghasilkan bentuk
yang dibutuhkan BSON; `mongo_component!` hanya melengkapi nama dan indeks.

## Atomisitas — baca ini

`save` memakai **operasi per-dokumen berurutan tanpa transaksi**: atomik
**per-entity**, bukan per-World. Kegagalan di tengah meninggalkan sebagian
entity tertulis.

Ini konsekuensi sadar agar `mongod` **standalone** cukup — transaksi
multi-dokumen MongoDB menuntut replica set. Bila kamu butuh `save` yang
all-or-nothing, pakai [`arke-postgres`](https://crates.io/crates/arke-postgres),
yang transaksional penuh.

Mutasi **satu** entity (`create`/`update`/`update_checked`/`remove`) tetap
atomik, karena MongoDB menjamin atomisitas per-dokumen.

## Batasan v1

Belum ada: query builder bertipe, relasi entity, nested/recursive load, cache
read-through, `save_incremental`, transaksi multi-dokumen, dan strategi evolusi
skema. Semuanya ditunda ke RFC lanjutan — lihat RFC-0035 §8. `MongoStore::collection()`
membuka koleksi mentah sebagai jalan keluar sementara.

Menambah field non-`Option` ke komponen yang sudah punya dokumen lama akan
menghasilkan `MongoError::Decode`. Untuk sekarang: pakai `Option<T>` pada field
baru, atau jalankan skrip backfill.

## Lisensi

MIT
````

- [ ] **Step 2: Verifikasi contoh README ter-kompilasi**

Tambahkan ke akhir `arke-mongo/src/lib.rs`:

```rust
#[doc = include_str!("../README.md")]
#[cfg(doctest)]
struct ReadmeDoctests;
```

Run: `cargo test -p arke-mongo --doc`
Expected: PASS. Bila contoh README gagal kompilasi, perbaiki README — bukan menghapus gerbangnya.

- [ ] **Step 3: Commit**

```bash
git add arke-mongo/README.md arke-mongo/src/lib.rs
git commit -m "docs(arke-mongo): README + gerbang doctest contoh"
```

---

## Task 17: Job CI `mongo`

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Lihat job Postgres sebagai pembanding**

Run: `grep -n "postgres" -A 20 .github/workflows/ci.yml`

Ini hanya untuk konteks — YAML lengkap job `mongo` ada di Step 2. Perhatikan indentasi job yang ada dan cocokkan.

- [ ] **Step 2: Tambahkan job `mongo`**

Tambahkan job berikut di bawah `jobs:` pada `.github/workflows/ci.yml`, sejajar job lain:

```yaml
  # Uji integrasi arke-mongo (RFC-0035 §7). MongoDB 7 **standalone** — tanpa
  # replica set, karena `save` sengaja tak memakai transaksi multi-dokumen.
  mongo:
    runs-on: ubuntu-latest
    services:
      mongo:
        image: mongo:7
        ports:
          - 27017:27017
        options: >-
          --health-cmd "mongosh --eval 'db.runCommand({ping:1})'"
          --health-interval 10s
          --health-timeout 5s
          --health-retries 10
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable

      - name: Test arke-mongo (integrasi)
        env:
          MONGODB_URI: mongodb://localhost:27017
        run: cargo test -p arke-mongo --verbose
```

- [ ] **Step 3: Verifikasi YAML sah**

Run: `python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/ci.yml')); print('YAML OK')"`
Expected: `YAML OK`

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: job mongo (service container mongo:7 standalone)"
```

---

## Task 18: Milestone doc + CHANGELOG

**Files:**
- Create: `docs/MILESTONE_32.md`
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Tulis milestone doc**

Buat `docs/MILESTONE_32.md`:

```markdown
# Milestone 32 — Adapter MongoDB, fondasi (RFC-0035)

> Crate `arke-mongo`: MongoDB sebagai sumber kebenaran dengan pemetaan satu
> dokumen per entity. Fondasi saja — query builder, relasi, cache, dan
> inkremental ditunda ke RFC lanjutan.

## Ruang lingkup

**Termasuk:**
- Crate baru `arke-mongo`: pemetaan `Value` ↔ BSON, validasi nama field BSON,
  trait `MongoComponent` + `IndexDef` + makro `mongo_component!` (tanpa
  proc-macro baru — memakai `#[derive(arke::Serialize)]` yang sudah ada).
- `MongoStore`: `connect`/`register`/`ensure_indexes`; CRUD per-operasi
  (`create`/`fetch`/`update`/`update_checked`/`version_of`/`remove`) dengan
  optimistic-lock `version`; `save`/`load` seluruh World.
- `pid` = `ObjectId`, dialokasikan klien (RFC-0034: indeks World ephemeral).
- CI: service container `mongo:7` standalone + uji integrasi.

**Tidak termasuk (sengaja ditunda):** query builder bertipe, relasi entity,
nested/recursive, cache read-through, `save_incremental`, transaksi
multi-dokumen, strategi evolusi skema.

## Artefak yang dihasilkan

| Artefak | Bentuk |
| --- | --- |
| RFC-0035 | Proposal adapter MongoDB dokumen-per-entity (+ Amandemen 1) |
| ADR-0035 | Keputusan: dokumen-per-entity, `ObjectId`, tanpa transaksi, tanpa derive |
| `arke-mongo` 0.1.0 | Implementasi + tes dua lapis |

## Kriteria selesai (Definition of Done)

- [ ] Lapis 1 (tanpa DB) hijau: round-trip `Value` ↔ BSON, validasi nama field,
      `cmp_doc`/`apply`/`update_ops`/`index_models`, deteksi `NAME` bertabrakan.
- [ ] Lapis 2 (MongoDB nyata) hijau: `create`/`fetch` setia, konflik versi
      terdeteksi, `remove`, `save` menghapus despawn dan **tidak** menghapus
      komponen tak terdaftar, `load` deterministik, `ensure_indexes` idempoten.
- [ ] Tes Lapis 2 di-skip bersih tanpa `MONGODB_URI` (CI default tetap hijau).
- [ ] `cargo fmt --check`, `clippy -D warnings`, dan job `mongo` hijau di CI.
- [ ] Core `arke` tetap 0-dependensi pihak-ketiga (STD-0003).
- [ ] Non-atomisitas `save` terdokumentasi di rustdoc **dan** README.

## Ketergantungan

- **Butuh selesai lebih dulu:** RFC-0035 (Accepted) + ADR-0035.
- **Membuka jalan bagi:** query builder dokumen (v2), relasi entity (v3).

## Pertanyaan terbuka

Lihat RFC-0035 §Pertanyaan terbuka — terutama validasi `IndexDef::field`,
ambang dokumen 16 MB, dan `load` tanpa paging.
```

- [ ] **Step 2: Tambahkan entri CHANGELOG**

Di `CHANGELOG.md`, ganti baris:

```markdown
## [Unreleased]
```

menjadi:

```markdown
## [Unreleased]

### Added

- **Crate baru `arke-mongo` 0.1.0** — adapter MongoDB dengan pemetaan **satu
  dokumen per entity** (komponen sebagai sub-dokumen di bawah `cmp`);
  identitas persisten `pid` = `ObjectId` yang dialokasikan klien
  ([RFC-0035](docs/RFC/RFC-0035-arke-mongo-adapter.md)). Tanpa derive baru:
  `#[derive(arke::Serialize)]` yang sudah ada menjadi jembatan ke BSON.
  Core `arke` tak berubah dan tetap 0-dependensi (STD-0003).

  **Batasan yang diketahui:** `save` seluruh World memakai operasi per-dokumen
  tanpa transaksi — atomik **per-entity**, bukan per-World, sehingga `mongod`
  standalone sudah cukup. Untuk `save` yang all-or-nothing, pakai
  `arke-postgres`. Query builder, relasi, cache, `save_incremental`, dan
  strategi evolusi skema ditunda ke RFC lanjutan.
```

- [ ] **Step 3: Commit**

```bash
git add docs/MILESTONE_32.md CHANGELOG.md
git commit -m "docs: MILESTONE_32 (adapter MongoDB) + CHANGELOG arke-mongo 0.1.0"
```

---

## Task 19: Terima RFC + tulis ADR-0035

Dikerjakan **terakhir**, setelah implementasi membuktikan desainnya bisa dibangun. Repo ini memisahkan RFC (proposal) dari ADR (keputusan + konsekuensi); ADR lahir saat RFC diterima.

**Files:**
- Create: `docs/ADR/ADR-0035-arke-mongo-adapter.md`
- Modify: `docs/RFC/RFC-0035-arke-mongo-adapter.md`
- Modify: `docs/RFC/README.md`
- Modify: `docs/ADR/README.md`

- [ ] **Step 1: Tulis ADR**

Buat `docs/ADR/ADR-0035-arke-mongo-adapter.md`:

```markdown
# ADR-0035: Adapter MongoDB dokumen-per-entity

- **Status:** Accepted
- **Tanggal:** 2026-07-31
- **RFC terkait:** [RFC-0035](../RFC/RFC-0035-arke-mongo-adapter.md)

## Konteks

Model working-set RFC-0021 tak terikat pada Postgres — yang terikat hanyalah
pemetaannya. Sebagian pengguna sudah menjalankan MongoDB dan tak ingin menambah
Postgres hanya untuk mempersist ECS. Model dokumen juga cocok secara alami
dengan bentuk data ECS: entity = kumpulan komponen heterogen opsional.

## Keputusan

1. **Crate `arke-mongo`** terpisah (gerbang dependensi; core `arke` tetap 0-dep,
   STD-0003).
2. **Satu dokumen per entity**, komponen sebagai sub-dokumen di bawah `cmp` —
   bukan satu koleksi per komponen. Fetch/update entity jadi satu round-trip dan
   atomik tanpa transaksi; query lintas-komponen tanpa `$lookup`.
3. **`pid` = `ObjectId`**, dialokasikan klien → `create` satu round-trip, aman
   multi-replica tanpa titik kontensi. Berbeda dari `pid` `i64` `arke-postgres`;
   keduanya sumber kebenaran alternatif, bukan dua muka dari satu penyimpanan.
4. **Tanpa crate derive.** `#[derive(arke::Serialize)]` sudah menghasilkan pohon
   `Value` yang dibutuhkan BSON; hanya trait tipis `MongoComponent` (nama +
   indeks) yang ditambahkan, diisi lewat `macro_rules!`.
5. **`save` tanpa transaksi** — operasi per-dokumen berurutan; atomik per-entity,
   bukan per-World. `mongod` standalone cukup untuk dev, tes, dan produksi
   sederhana.
6. **Decode gagal → error**, tak pernah dilewati diam-diam.
7. **`save` menulis per sub-field** (`$set: {"cmp.x": …}`), tak pernah mengganti
   `cmp` utuh — komponen milik penulis lain tak boleh lenyap.

## Konsekuensi

- **Positif**: MongoDB jadi sumber kebenaran ECS yang query-able; permukaan v1
  kecil sehingga API query belum terkunci; nol proc-macro baru untuk dipelihara.
- **Biaya**: `save` memberi jaminan lebih lemah daripada `arke-postgres`
  (didokumentasikan di rustdoc dan README, bukan hanya RFC); dua permukaan
  adapter untuk dijaga koheren; deklarasi indeks manual tanpa cek kompilasi.
- **Netral**: evolusi skema belum dijawab — Mongo schemaless, strateginya beda
  secara fundamental dan pantas dapat RFC sendiri.

## Alternatif ditolak

- **Satu koleksi per tipe komponen** — memakai Mongo seperti "Postgres yang
  lebih lemah"; `$lookup` untuk tiap query lintas-komponen.
- **`i64` via koleksi counter** — menukar skalabilitas tulis demi keseragaman
  tipe `pid` yang kosmetik.
- **Transaksi multi-dokumen** — menuntut replica set untuk dev, tes, dan
  produksi; beban setup tak sepadan untuk v1 fondasi.
- **`#[derive(MongoComponent)]`** — parser atribut kedua tanpa imbalan di v1.

Rincian di [RFC-0035](../RFC/RFC-0035-arke-mongo-adapter.md).
```

- [ ] **Step 2: Ubah status RFC menjadi Accepted**

Di `docs/RFC/RFC-0035-arke-mongo-adapter.md`, ganti baris status:

```markdown
- **Status:** Draft <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
```

menjadi:

```markdown
- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
```

Ganti baris ADR:

```markdown
- **ADR terkait:** — (menyusul bila status menjadi Accepted)
```

menjadi:

```markdown
- **ADR terkait:** [ADR-0035](../ADR/ADR-0035-arke-mongo-adapter.md)
```

Ganti bagian `## Keputusan`:

```markdown
Belum diputuskan — status **Draft**. Saat diterima, bagian ini diisi ringkasan keputusan dan ADR-0035 dibuat, lalu M-35 dibuka dengan TDD mulai dari pemetaan `Value` ↔ BSON (§7 Lapis 1, tanpa DB).
```

menjadi:

```markdown
**Diterima.** Lihat [ADR-0035](../ADR/ADR-0035-arke-mongo-adapter.md). M-32 diselesaikan lewat TDD dari pemetaan `Value` ↔ BSON (§7 Lapis 1, tanpa DB) hingga `MongoStore` penuh; `bulkWrite` diganti operasi per-dokumen berurutan (Amandemen 1) demi portabilitas server.
```

- [ ] **Step 3: Perbarui indeks RFC**

Di `docs/RFC/README.md`, ganti baris RFC-0035:

```markdown
| [RFC-0035](RFC-0035-arke-mongo-adapter.md) | `arke-mongo` — adapter MongoDB dokumen-per-entity | **Draft** |
```

menjadi:

```markdown
| [RFC-0035](RFC-0035-arke-mongo-adapter.md) | `arke-mongo` — adapter MongoDB dokumen-per-entity | Accepted |
```

- [ ] **Step 4: Perbarui indeks ADR**

Di `docs/ADR/README.md`, tambahkan baris berikut tepat di bawah baris ADR-0033 (baris terakhir tabel):

```markdown
| [ADR-0035](ADR-0035-arke-mongo-adapter.md) | Adapter MongoDB dokumen-per-entity (`arke-mongo`) | Accepted |
```

Catatan: ADR-0034 memang belum ada — RFC-0034 diterima tanpa ADR pendamping. Itu celah lama, di luar ruang lingkup milestone ini; jangan mengarang ADR-0034 untuk menutup nomornya.

- [ ] **Step 5: Verifikasi seluruh gerbang hijau**

Run:

```bash
cargo fmt --all -- --check \
  && cargo clippy --workspace --all-targets --all-features -- -D warnings \
  && cargo test --workspace \
  && MONGODB_URI=mongodb://localhost:27017 cargo test -p arke-mongo
```

Expected: semua hijau. Jangan commit bila ada yang merah.

- [ ] **Step 6: Commit**

```bash
git add docs/ADR/ADR-0035-arke-mongo-adapter.md docs/RFC/RFC-0035-arke-mongo-adapter.md docs/RFC/README.md docs/ADR/README.md
git commit -m "docs(rfc): terima RFC-0035 + ADR-0035 (adapter MongoDB)"
```

---

## Definition of Done keseluruhan

- [ ] `cargo test --workspace` hijau tanpa `MONGODB_URI` (tes integrasi di-skip bersih).
- [ ] `MONGODB_URI=… cargo test -p arke-mongo` hijau: 24 tes mapping + 11 tes store + doctest.
- [ ] `cargo fmt --all -- --check` dan `cargo clippy --workspace --all-targets --all-features -- -D warnings` bersih.
- [ ] `cargo tree -p arke -e normal` tak menampilkan dependensi pihak-ketiga (STD-0003).
- [ ] Job CI `mongo` hijau.
- [ ] RFC-0035 berstatus Accepted, ADR-0035 ada, kedua indeks diperbarui.
- [ ] Non-atomisitas `save` tertulis di rustdoc `MongoStore::save` **dan** `arke-mongo/README.md`.
