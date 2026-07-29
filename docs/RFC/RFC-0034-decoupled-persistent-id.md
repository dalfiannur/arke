# RFC-0034: `arke-postgres` — identitas persisten (`pid`) yang decoupled dari indeks World

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-29
- **Milestone:** M-34 (Identitas persisten decoupled — per-op stateless store untuk web/async)
- **RFC terkait:** [RFC-0021](./RFC-0021-arke-postgres-adapter.md) (adapter Postgres), [RFC-0002](./RFC-0002-core-storage-architecture.md) (storage inti), [RFC-0033](./RFC-0033-component-cache.md) (cache)

## Ringkasan

Memisahkan **identitas persisten** sebuah entity (`pid`, dialokasikan DB) dari **indeks World arke** (posisi array dense yang *ephemeral*, milik free-list in-memory). Saat ini `arke-postgres` (RFC-0021) menyamakan `entity_id = World index`. RFC ini menjadikan **indeks World selalu ephemeral** (tak pernah dipersist) dan **`pid` sebagai kunci persisten tunggal**, dengan `arke-postgres` memelihara pemetaan `pid ↔ Entity` untuk working-set aktif.

Tujuannya: memungkinkan **store per-operasi yang stateless** (World kecil per-request, dibuang setelahnya) yang aman untuk **multi-replica** (Postgres satu-satunya sumber kebenaran) — tanpa meninggalkan model working-set RFC-0021, dan tanpa `unsafe`.

## Motivasi

RFC-0021 memodelkan `World` sebagai working-set yang di-*materialize* via `spawn_at(entity_id, generation)`, dengan `entity_id` = indeks World. Ini bekerja untuk model **World global panjang-umur** (muat semua → indeks dense 0..N → checkpoint).

Untuk **web server async** dengan World **per-operasi** (buat World kecil, muat subset, mutasi, persist, buang), model itu **pecah**:

1. **Indeks World bersifat lokal & mulai dari 0.** `World::new()` lalu `spawn()` selalu memberi indeks 0. Dua `create` per-op → dua entity ber-`entity_id = 0` → yang kedua **menimpa** yang pertama. Id tak unik lintas-operasi.
2. **Id unik-global sebagai indeks World meledakkan memori.** Bila id dialokasikan monoton (mis. sequence: 1, 2, … 1_000_000), `spawn_at(1_000_000, 0)` menumbuhkan `entities: Vec` hingga sejuta slot **untuk satu entity** (RFC-0002 §storage: indeks = posisi Vec). Tak layak.
3. **Free-list dense butuh World global.** Mempertahankan id **dense & reusable** (cara arke) mensyaratkan free-list in-memory → berarti seluruh World di memori. Bertentangan dengan per-op stateless.

Akar masalah: **indeks World arke bukan id persisten** — ia posisi array dense yang dikelola free-list. Persistensi yang benar harus **memisahkan** id persisten dari indeks ephemeral.

### Tegangan yang diakui

- **Tidak membuang model working-set** RFC-0021. RFC ini **menyempurnakan**-nya: indeks World tetap ephemeral (kini *eksplisit*), `pid` jadi kunci persisten. Model World global tetap didukung (muat semua ke World, arke-postgres memetakan `pid ↔ Entity`), sekaligus membuka model per-op.
- **Perubahan skema & kontrak** RFC-0021 (kunci `entity_id` → `pid`). Butuh jalur migrasi (lihat §5).
- **`arke` core tetap 0-dependensi** (STD-0003) — perubahan hanya di adapter.

## Usulan rinci

### 1. Skema: `pid` sebagai kunci persisten

```sql
-- SEBELUM (RFC-0021): entity_id = indeks World
arke_entities(entity_id BIGINT PK, generation BIGINT, version BIGINT)
cmp_<T>(entity_id BIGINT PK REFERENCES arke_entities, <kolom>...)

-- SESUDAH (RFC-0034): pid persisten, indeks World tak disimpan
arke_entities(pid BIGSERIAL PRIMARY KEY, version BIGINT NOT NULL DEFAULT 0)
cmp_<T>(pid BIGINT PRIMARY KEY REFERENCES arke_entities(pid) ON DELETE CASCADE, <kolom>...)
```

- `pid` dialokasikan DB (`BIGSERIAL`) — **unik global**, tak pernah jadi indeks World.
- `generation` dihapus dari peran identitas persisten (identitas = `pid`; `version` untuk optimistic-lock).

### 2. Pemetaan `pid ↔ Entity` (per working-set)

`PgStore` (atau handle operasi) memelihara, untuk World aktif:

```rust
pid_of: HashMap<Entity, i64>,   // Entity (indeks ephemeral) → pid persisten
entity_of: HashMap<i64, Entity> // pid → Entity
```

Diisi saat `create`/`fetch`/`load`. Indeks World kini **selalu dense & lokal** (via `spawn()`), pemetaan menautkannya ke `pid`. `save`/`update`/`query` bekerja lewat `pid` (baca dari peta), **bukan** `entity.index()`.

### 3. API per-operasi (facet baru, pid-addressed)

Semua dua-fase (sync-baca-World / async-DB) agar future `Send` (lih. `stage`/`commit`, RFC terkait):

```rust
impl PgStore {
    /// Alokasi pid (INSERT arke_entities RETURNING pid) + tulis komponen entity di `world`.
    pub async fn insert(&mut self, world: &World, entity: Entity) -> Result<i64>;   // → pid
    /// Muat komponen `pid` ke `world` sebagai entity lokal baru; kembalikan handle.
    pub async fn fetch(&mut self, world: &mut World, pid: i64) -> Result<Option<Entity>>;
    /// Tulis-ulang komponen `pid` dari `entity` (versi naik, optimistic opsional).
    pub async fn update(&mut self, world: &World, pid: i64, entity: Entity) -> Result<()>;
    /// Hapus `pid` (cascade ke tabel komponen).
    pub async fn remove(&mut self, pid: i64) -> Result<()>;
    /// Query typed (RFC-0030): muat entity yang cocok + kembalikan (pid, Entity).
    pub async fn query_pids<T: PgComponent>(&mut self, /* builder */) -> Result<Vec<(i64, Entity)>>;
}
```

Model global World (RFC-0021) **tetap** didukung: `load` memuat semua entity ke World dengan indeks lokal dense (via `spawn`, bukan `spawn_at(pid)`), mengisi `pid_of`/`entity_of`; `save_incremental` mem-*diff* by `pid`.

### 4. Dua-fase & Send

`insert`/`update` mengikuti pola `stage`/`commit` (RFC pendahulu): fase sync membaca komponen entity dari `world` → data owned; fase async menulis DB tanpa menahan `&World`. Menjaga future handler async `Send` tanpa `World: Sync`.

### 5. Migrasi & kompatibilitas

- **Data lama** ber-`entity_id`: skrip migrasi menyalin `entity_id` → `pid` (nilai sama), set `BIGSERIAL` di atas `MAX(entity_id)`.
- **API lama** (`save`/`load` berbasis indeks): dipertahankan sementara di atas pemetaan (indeks lama diperlakukan sebagai `pid`) atau ditandai *deprecated* → dihapus di rilis mayor. Keputusan di §Pertanyaan terbuka.
- Cache (RFC-0033) di-key ulang oleh `pid`.

### 6. Determinisme & konkurensi

- Query tetap deterministik (`ORDER BY pid`).
- Optimistic-lock via `version` per `pid` (`update` dengan `expected_version` opsional).
- Multi-replica: stateless per-op → Postgres otoritatif; tak ada World global bersama.

## Alternatif yang dipertimbangkan

1. **Sequence + `spawn_at(pid)`** — id unik tapi indeks World = pid → `Vec` meledak (motivasi §2). **Ditolak.**
2. **World global panjang-umur saja** (indeks dense via free-list) — sesuai desain arke, tapi memori-terikat & **tak multi-replica** (tiap replica World sendiri). Baik untuk single-instance; tak memenuhi tujuan web/scale. **Ditolak sebagai satu-satunya model.**
3. **`pid` sebagai komponen pengguna** (mis. `Pid(i64)`) — menautkan pid via komponen, bukan peta internal. Sederhana tapi membocorkan identitas persisten ke ruang komponen pengguna & menyulitkan query. **Dipertimbangkan; peta internal lebih bersih.**

## Dampak

- **RFC-0021 diamandemen:** kunci persisten `entity_id` → `pid`; indeks World eksplisit-ephemeral. Model working-set tetap.
- **Breaking (skema + API):** perlu migrasi + versi mayor `arke-postgres`.
- **Konsumen (mis. backend-rs):** `Store` jadi thin wrapper per-op (`create`/`get`/`query`/`update`/`delete` by `pid`), stateless & multi-replica-safe. Menghapus kebutuhan World global.
- **`stage`/`commit` & `stage_incremental`/`commit_incremental`** yang sudah ada di-*rework* ke keying `pid`.

## Pertanyaan terbuka (terjawab saat penerimaan)

1. **API berbasis-indeks lama.** → **Rekey bersih.** `save`/`load`/`save_incremental` di-*rekey* internal ke `pid` (indeks World ephemeral di mana-mana); **tanpa** API-indeks lama paralel. Pre-1.0 + konsumen tunggal (co-dev) → boleh breaking. Bump **`0.12.0`**.
2. **Bentuk `query_pids`.** → **Kembalikan `(pid, Entity)`** (eksplisit, tak mengotori ruang komponen). `PgStore` juga mengekspos `pid` untuk Entity yang dimuat (via peta).
3. **`generation`.** → **Dihapus dari skema persisten.** Identitas = `pid` (`BIGSERIAL`, tak pernah dipakai-ulang); `version` untuk optimistic-lock. Generation in-memory `arke` core **tak berubah**.
4. **Migrasi data lama.** → **Greenfield sekarang** (belum ada data produksi → drop/recreate). Jalur migrasi in-place didokumentasikan: `entity_id` → `pid` (nilai identik), `setval` sequence di atas `MAX(entity_id)` — untuk data nyata kelak.
5. **Keying cache.** → **Cache di-key oleh `pid`** (mengamandemen RFC-0033).

## Amandemen 1 — Resolusi kolom relasi entity (2026-07-29, ditemukan saat implementasi)

Temuan: `arke-postgres-derive` menghasilkan `to_params` yang menulis kolom **relasi
entity** (RFC-0031, field `EntityRef`) sebagai **indeks World entity yang dirujuk**
(`e.index() as i64` + generation), dan predikat relasi di `query.rs` juga memakai
`index`. Di bawah `pid`, kolom relasi harus menyimpan **`pid`** entity yang dirujuk —
tapi derive tak punya konteks `pid`.

**Keputusan:** **resolusi saat-save** memanfaatkan `ColumnDef.references` (sudah
menandai kolom relasi `_id` → `Some("arke_entities(pid)")`). Alur `save`/`save_incremental`:
1. **Pass 1:** pastikan setiap entity working-set punya `pid` (`pid_of`; alokasi untuk
   yang baru).
2. **Pass 2:** untuk tiap kolom ber-`references`, ganti `PgValue::Int(index)` → `pid`
   entity yang dirujuk (via `entity_of`/`pid_of`). Load: kolom relasi = `pid` →
   rekonstruksi handle via `entity_of` (entity yang dirujuk **harus** ikut ter-load,
   atau ref di-resolve lazily).

**Batasan:** relasi entity hanya koheren di **mode World-global** (semua entity
ter-map). Konsumen per-op yang butuh referensi antar-entity memakai **id string**
(mis. `owner_id: String`) — bukan `EntityRef` arke — sehingga tak terkena batasan ini.
`generation` pada kolom relasi (dulu untuk deteksi handle basi) dihapus; identitas ref
= `pid`.

**Dampak implementasi:** whole-world rekey (`save`/`load`/`save_incremental`/`materialize`
+ `dump→Entity` + map) plus perubahan `arke-postgres-derive` (kolom ref emit penanda,
di-resolve store) dan `query.rs` (predikat relasi by `pid`). Tes `relations.rs`/
`query_builder.rs`/`constraints.rs` harus hijau. Ini pekerjaan terfokus tersendiri;
jalur per-op (nilai inti RFC) sudah terimplementasi & terverifikasi.

## Amandemen 2 — Relasi ditunda dari 0.12.0 (2026-07-29, ditemukan saat implementasi)

Temuan lebih dalam dari Amandemen 1: resolusi kolom relasi **saat-save** (index→`pid`)
belum cukup. Model relasi RFC-0031/0032 juga bergantung pada **preservasi handle lintas
`load`** — `load` lama memakai `spawn_at(index, generation)` sehingga handle yang
disimpan sebelum `load` (mis. `b_weak`) tetap valid setelahnya, dan handle basi ditolak
lewat gerbang `generation`. Di bawah `pid`, `load` `spawn()` entity dengan indeks
**baru** (ephemeral) dan `generation` persisten dihapus → kedua andalan itu runtuh.
Kolom `_id`/`_gen` hasil derive pun masih menyimpan indeks World + generation.

**Keputusan:** **tunda relasi** dari rilis 0.12.0. Rekey `pid` **non-relasi** dituntaskan
dan dikapalkan (whole-world `save`/`load`/`save_incremental`/`materialize`/`update_entity`
+ query builder non-relasi + constraints, semua hijau vs Postgres). Tes yang bergantung
relasi/preservasi-handle (`relations`, `nested`, `recursive`, `typed_relations`, unit
`matches_bersarang_3_deep`) ditandai `#[ignore]` dengan alasan terdokumentasi; contoh
`persist.rs` ditulis-ulang agar tak mengandalkan preservasi handle (verifikasi via isi
world muat). `arke-postgres-derive` & `query.rs` (jalur relasi) **belum** diubah.

**Tindak lanjut (opsi 1, sesi berikutnya):** desain ulang relasi **berbasis `pid`** —
kolom `EntityRef` menyimpan `pid` yang dirujuk (bukan indeks/generation), join by `pid`,
`PgValue::Ref`, perubahan derive + `query.rs`, penulisan-ulang tes relasi. Akan menjadi
RFC tersendiri atau Amandemen 3.

## Keputusan

**Diterima (Accepted), 2026-07-29.** Memisahkan `pid` (persisten, dialokasikan DB via `BIGSERIAL`) dari indeks `World` (ephemeral). `arke-postgres` memelihara peta `pid ↔ Entity` per working-set; seluruh API persist di-rekey ke `pid`; `generation` persisten dihapus; cache (RFC-0033) di-key oleh `pid`. Breaking → `arke-postgres 0.12.0`. Mengamandemen kontrak RFC-0021 (kunci persisten `entity_id` → `pid`) tanpa membuang model working-set. Implementasi mengikuti pola dua-fase (`stage`/`commit`) yang sudah ada, di-rekey ke `pid`.
