# RFC-0021: `arke-postgres` — adapter Postgres sebagai sumber kebenaran

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-28
- **Milestone:** M-20 (Postgres Adapter)
- **ADR terkait:** [ADR-0021](../ADR/ADR-0021-arke-postgres-adapter.md)

## Ringkasan

Crate **adapter terpisah `arke-postgres`** yang menjadikan **PostgreSQL sumber kebenaran (source of truth) yang durable** bagi keadaan ECS, dengan **pemetaan relasional berkolom-tipe** (satu tabel per tipe komponen, tiap field → **kolom SQL nyata** ber-tipe) yang dapat di-query SQL biasa (join lintas-komponen, index, analitik, akses dari service lain), lewat API **async** (sqlx/tokio). Skema kolom diturunkan dari sebuah **derive `#[derive(PgComponent)]`**. `arke` core **tetap 0-dependensi** (STD-0003); seluruh dependensi DB terkurung di crate adapter.

Model relasi: **`World` = *working set* in-memory** yang di-*materialize* dari Postgres, dijalankan sistem-sistemnya secara deterministik, lalu **ditulis-balik**. Postgres memegang data otoritatif; sinkronisasi terjadi di **titik-titik tertentu** (muat saat mulai, tulis-balik saat checkpoint / lewat pelacakan-perubahan) — **bukan** per-tick.

## Motivasi

Snapshot (RFC-0007/M-6) sudah memberi persistensi **portabel** (`World` ↔ JSON, round-trip setia STD-0002), tapi sebagai **blob buram**: tak bisa di-query SQL, tak bisa dijoin, tak bisa dibaca/ditulis service lain. Kebutuhan: Postgres sebagai **sumber kebenaran** yang:

- dapat di-query lewat SQL (dashboard, analitik, join lintas-komponen);
- dibaca/ditulis service lain (bukan hanya proses Arke);
- durable & transaksional.

Ini di luar sifat inti Arke (ARCHITECTURE_BIBLE §5: Arke **bukan** basis data/ORM), maka hidup sebagai **adapter** — persis mekanisme yang sudah diantisipasi (`Cargo.toml`: "integrasi eksternal hidup di crate/feature adapter terpisah").

### Tegangan yang diakui

ECS in-memory berkinerja tinggi ("ergonomis = cepat") **tak cocok** disinkronkan ke DB **per-tick** (impedance mismatch). Karena itu modelnya **working-set**: `World` adalah salinan-kerja materialized; Postgres otoritatif & durable; sinkronisasi di titik yang dikendalikan aplikasi. RFC ini tak menjanjikan write-through real-time.

## Usulan rinci

### 1. Batas crate

```
arke            (core, 0-dep, STD-0003)  ← tak berubah
arke-postgres   (adapter)  → depends: arke + sqlx (async, Postgres, JSONB, pool, migrasi)
```

`arke-postgres` **tidak** tunduk STD-0003 (ia justru gerbang dependensi). Ia hanya memakai API publik `arke` (`Serialize`/`Value`, `register_serializable`, `snapshot`/`load_snapshot`, `Entity`, query/command buffer).

### 2. Skema relasional berkolom-tipe

**Registry entity** (cermin generational index, STD-0007):

```sql
CREATE TABLE arke_entities (
    entity_id  BIGINT PRIMARY KEY,   -- index slot (u32) sebagai BIGINT
    generation INT NOT NULL          -- versi; juga optimistic-lock
);
```

**Satu tabel per tipe komponen terdaftar**, tiap field komponen menjadi **kolom SQL ber-tipe** (bukan blob), dikunci **nama tipe** (portabel):

```rust
#[derive(PgComponent)]
struct Position { x: f32, y: f32, z: f32 }
```
→
```sql
CREATE TABLE cmp_position (           -- nama diturunkan dari type_name (disanitasi)
    entity_id BIGINT PRIMARY KEY REFERENCES arke_entities(entity_id) ON DELETE CASCADE,
    x REAL NOT NULL,
    y REAL NOT NULL,
    z REAL NOT NULL
);
```

- Field → **kolom nyata** → dapat di-index, dijoin, difilter, diagregasi langsung:
  ```sql
  SELECT p.entity_id, p.x, h.hp
  FROM cmp_position p JOIN cmp_health h USING (entity_id)
  WHERE p.x > 100 AND h.hp < 20;         -- index-able, tanpa ekstraksi JSONB
  ```
- **Field non-skalar** (nested struct, enum, `Vec<T>`, dsb.) → kolom **`JSONB`** *fallback* (via `Serialize`), sehingga komponen tetap boleh punya field kompleks tanpa memblokir field skalar dari jadi kolom nyata.

### 3. Derive `PgComponent` (skema dari tipe)

Kolom-tipe menuntut tahu **tipe Rust konkret** tiap field — informasi yang `Value` (dynamically-typed) buang. Maka skema diturunkan oleh derive tulis-tangan (pola `arke-derive`), di crate adapter (`arke-postgres-derive`), meng-emit:

```rust
trait PgComponent {
    const TABLE: &'static str;                    // dari type_name, disanitasi
    const COLUMNS: &'static [Column];             // (nama, PgType, nullable)
    fn to_params(&self) -> Vec<PgValue>;          // bind per kolom
    fn from_row(row: &PgRow) -> Self;             // baca per kolom
}
```

**Pemetaan tipe Rust → SQL:**

| Rust | SQL |
| --- | --- |
| `i8`/`i16`/`i32` | `INTEGER` |
| `i64`/`isize` | `BIGINT` |
| `u8`/`u16`/`u32` | `BIGINT` (Postgres tanpa unsigned; lebar aman) |
| `u64`/`usize` | `NUMERIC(20)` (melampaui `BIGINT`) |
| `f32` | `REAL` |
| `f64` | `DOUBLE PRECISION` |
| `bool` | `BOOLEAN` |
| `String` | `TEXT` |
| `Option<T>` | kolom `T` **nullable** |
| lainnya (nested/enum/`Vec`) | `JSONB` (fallback via `Serialize`) |

Hanya komponen yang men-`derive(PgComponent)` yang dipersist (opt-in — sejajar `register_serializable`). Ini **lebih ketat** dari JSONB-untuk-semua, tapi itulah harga kolom-tipe.

### 4. API async (sqlx)

```rust
let store = PgStore::connect(&url).await?;        // connection pool
store.migrate(&world).await?;                      // buat/pastikan tabel utk komponen terdaftar

// Materialize working-set dari Postgres (deterministik: ORDER BY entity_id → STD-0005).
store.load(&mut world).await?;

// ... jalankan Schedule secara in-memory (deterministik) ...

// Tulis-balik (transaksional).
store.save(&world).await?;
```

- `migrate` membuat `arke_entities` + `cmp_<name>` (kolom dari `PgComponent::COLUMNS`) untuk tiap komponen terdaftar.
- `load` men-SELECT semua entity + komponen, membaca tiap kolom via `PgComponent::from_row`, merekonstruksi `World` (jalur `insert`). Urutan `ORDER BY entity_id` menjaga determinisme iterasi (STD-0005) pasca-materialize.
- `save` menulis seluruh working-set dalam **satu transaksi** (upsert entity + komponen via `to_params`, DELETE yang hilang).

### 5. Konsistensi & konkurensi

- **Transaksi**: tiap `save`/`update_entity` atomik.
- **Optimistic-lock = kolom `version` + gerbang `generation`**: `arke_entities` punya `version BIGINT` yang **naik tiap tulis-balik**. `update_entity(world, entity, expected_version)` menjalankan `UPDATE … SET version=version+1 WHERE entity_id=$1 AND generation=$2 AND version=$3 RETURNING version`; 0 baris → **konflik** (writer lain mengubahnya). Catatan penting: `generation` (STD-0007) hanya naik saat despawn/**re**spawn, jadi ia mendeteksi konflik **identitas**, bukan **nilai** — maka kolom `version` terpisah diperlukan untuk konflik mutasi-komponen. `entity_version(entity)` membaca versi terkini untuk retry.
- **`save` (overwrite penuh)** mereset `version` ke 0 (baseline single-writer/inisialisasi); multi-writer memakai `update_entity`.
- **Despawn → DELETE** (cascade ke tabel komponen).
- **Multi-writer**: Postgres otoritatif; konflik terdeteksi lewat `version`. Kebijakan resolusi (retry / LWW / merge) diserahkan ke pemanggil (`update_entity` mengembalikan `Conflict`; contoh retry di uji).

### 6. Fidelity & versi

- Round-trip field skalar via `PgComponent` (kolom-tipe); field non-skalar via `Serialize`/`Value` (STD-0002) di kolom `JSONB`.
- `schema_version` (STD-0001) disimpan (tabel `arke_meta`) untuk migrasi format lintas-versi.

### 7. Determinisme

Eksekusi ECS tetap **deterministik** atas working-set yang dimuat; Postgres adalah **batas I/O**, bukan bagian jalur panas. `load` **wajib** deterministik (`ORDER BY entity_id`) agar urutan iterasi identik antar-materialize (STD-0005).

### 8. Rencana bertahap

| Fase | Isi |
| --- | --- |
| **v1** (M-20) | `#[derive(PgComponent)]` + pemetaan tipe (skalar/`Option`/`JSONB`/`NUMERIC`); skema kolom-tipe; `migrate`, `load`/`save` penuh; optimistic-lock (`version` + `generation`). |
| **v2** (M-20) | Tulis-balik **inkremental** `save_incremental` — **diff berbasis-nilai** terhadap rekam sinkron-terakhir (arke tak melacak perubahan otomatis; menyimpan salinan keadaan). Hanya entity baru/berubah ditulis (UPSERT+versi), yang hilang di-DELETE. *(Pendekatan `CommandBuffer`-sebagai-diff ditolak: hanya menangkap perubahan struktural, bukan mutasi-nilai `&mut`.)* **Migrasi evolusi-skema**: `migrate` merekonsiliasi tabel — field ditambah → `ALTER ADD COLUMN` (backfill), field dihapus → kolom usang jadi nullable (`DROP NOT NULL`, non-destruktif). |
| **v3** (M-20) | Materialisasi **query-scoped**: `load_where::<T>(world, predicate)` memuat subset entity yang cocok predikat SQL atas kolom `T` beserta seluruh komponennya (working-set parsial; aman dengan `save_incremental`). Index/constraint kustom per komponen → menyusul. |

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| **Kolom-tipe (dipilih)** | Query/index/join/agregasi SQL terkaya; skema self-documenting | Butuh derive per komponen; field non-skalar perlu fallback; migrasi `ALTER` | — (dipilih untuk v1 sesuai tujuan "sumber kebenaran SQL") |
| JSONB per komponen | Tanpa derive; tipe arbitrer | Query lewat `data->>'x'` (kurang ergonomis, index terbatas) | Kalah ergonomis SQL; dipakai hanya sebagai **fallback** field non-skalar |
| Snapshot blob JSONB tunggal | Sepele; memakai ulang M-6 utuh | **Tak** query-able/join-able → gagal syarat "sumber kebenaran SQL" | Tak memenuhi tujuan; tetap tersedia untuk pure save/restore |
| Driver sync (`postgres`/diesel) | Tanpa runtime | Default pengguna **async**; blocking di service async buruk | Async dipilih; sync bisa jadi feature lanjutan |
| `tokio-postgres` langsung | Ringan | Tanpa pool/migrasi bawaan | sqlx: pool + migrasi + query cek-kompilasi |
| Sinkronisasi per-tick (write-through) | Selalu konsisten | Impedance mismatch; membunuh performa ECS | Model working-set + sync di titik terkendali |

## Dampak

- **Kompatibilitas / migrasi:** aditif & **terisolasi**. `arke` core tak berubah, tetap 0-dep (STD-0003) — gerbang CI standalone tetap hijau (adapter di luar cek, atau di workspace terpisah).
- **Keamanan:** adapter murni safe Rust + sqlx; tak menyentuh `unsafe` core.
- **Konsekuensi pada invarian:** memperluas **portabilitas/kepemilikan data** ke Postgres tanpa mengorbankan determinisme (batas I/O). Snapshot (STD-0001/0002) jadi fondasi fidelity.

## Pertanyaan terbuka

- **Derive `PgComponent`**: crate `arke-postgres-derive` sendiri (0-dep, pola `arke-derive`) vs perluasan `arke-derive` (menyeret perhatian SQL ke core). Condong ke crate adapter-side terpisah.
- **Field non-skalar**: `JSONB` fallback (dipilih) vs tabel anak (relasi 1-N) untuk `Vec<T>` — yang terakhir lebih relasional tapi jauh lebih kompleks.
- **Evolusi skema komponen** (field ditambah/dihapus) → migrasi `ALTER TABLE`; bagaimana mendeteksi & menerapkan aman.
- **Tipe tak-terpetakan** (mis. `u128`, custom): kompilasi gagal (ketat) vs `JSONB` fallback diam-diam?
- **Pemetaan nama tabel/kolom** dari `type_name`/field (namespace, tabrakan, sanitasi & pembatasan identifier SQL, panjang 63-char Postgres).
- **Pelacakan-perubahan** untuk tulis-balik inkremental (v2) — hook core (change-log) vs diff snapshot; bisakah `CommandBuffer` jadi sumber diff?
- **Kebijakan konflik** multi-writer (retry / LWW / merge) saat generation mismatch.
- **Materialisasi parsial** (query-scoped world) — muat hanya subset entity/komponen (v3).
- **Feature `sync`** (driver blocking) untuk pengguna non-async — perlukah?

## Keputusan

**Diterima.** Lihat [ADR-0021](../ADR/ADR-0021-arke-postgres-adapter.md). M-20 (fase v1) dibuka; TDD mulai dari derive `PgComponent` + pemetaan tipe (dapat diuji **tanpa** DB), lalu `migrate`/`load`/`save` + optimistic-lock dengan Postgres uji (via `sqlx::test` atau kontainer) di CI.
