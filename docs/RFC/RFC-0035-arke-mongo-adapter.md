# RFC-0035: `arke-mongo` — adapter MongoDB dokumen-per-entity

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-31
- **Milestone:** M-32 (Adapter MongoDB — fondasi)
- **ADR terkait:** [ADR-0035](../ADR/ADR-0035-arke-mongo-adapter.md)
- **RFC terkait:** [RFC-0021](./RFC-0021-arke-postgres-adapter.md) (adapter Postgres), [RFC-0034](./RFC-0034-decoupled-persistent-id.md) (identitas persisten `pid`), [RFC-0007](./RFC-0007-world-snapshot.md) / [RFC-0009](./RFC-0009-derive-serialize.md) (`Serialize`/`Value`)

> **Amandemen 1 (2026-07-31) — mekanisme `save`.** Naskah asli §5 menyebut
> `bulkWrite ordered`. Perintah `bulkWrite` baru tersedia pada **MongoDB server
> 8.0**, sedangkan §7 sengaja menargetkan `mongod` 7 standalone; driver Rust juga
> tak menyediakan `Collection::bulk_write`. `save` karena itu diimplementasikan
> sebagai **operasi per-dokumen berurutan** (`update_one` per entity + satu
> `delete_many`). **Jaminan yang dijanjikan tidak berubah** — atomik per-entity,
> bukan per-World; yang berubah hanya mekanismenya, demi portabilitas server.

> **Amandemen 2 (2026-07-31) — validasi `NAME` saat `register`, dan
> `MongoError::DuplicateField`.** Review kode Task 3–10 menemukan dua celah.
>
> Pertama: `cmp_doc` menulis `MongoComponent::NAME` sebagai **kunci literal**
> di bawah `cmp`, sedangkan `update_ops` memakainya untuk membangun
> `format!("cmp.{NAME}")` — sebuah **path bertitik**. Untuk `NAME` yang
> mengandung `.`, kedua jalur menyasar tempat berbeda di dokumen: `create`
> menulis kunci literal, `update`/`save` menulis ke sub-dokumen bersarang.
> Akibatnya `apply` (yang membaca kunci literal) tak pernah melihat apa yang
> ditulis `update`, dan setiap update hilang diam-diam tanpa error di mana
> pun. `NAME` berawalan `$` punya masalah sejenis: menghasilkan operator
> Mongo tak sengaja (mis. `$unset: {"cmp.$evil": ""}`), ditolak server dengan
> pesan driver yang opak, bukan kegagalan jelas seperti dijanjikan §4. Karena
> itu `Registry::push` kini memvalidasi `NAME` — panic bila kosong,
> mengandung `.`, atau berawalan `$` — sejajar pemeriksaan tabrakan nama yang
> sudah ada: bug programmer di sebuah `const`, ditangkap sedini mungkin, saat
> registrasi.
>
> Kedua: `Value::Map` (`Vec<(String, Value)>`) mengizinkan kunci duplikat,
> tetapi `Document::insert` mendeduplikasi diam-diam — yang disisip terakhir
> menang. `arke-derive` tak punya penjaga tabrakan `rename`, jadi dua field
> yang memetakan ke nama BSON sama akan saling menimpa tanpa peringatan,
> melanggar klaim round-trip setia (STD-0002) di §4. Varian error baru
> `MongoError::DuplicateField { component, field }` menolak kondisi ini
> alih-alih mendiamkannya; `validate_names` di `bson_map.rs` mendeteksinya di
> tiap level `Map`, termasuk yang bersarang.

## Ringkasan

Crate adapter baru **`arke-mongo`** yang menjadikan **MongoDB sumber kebenaran durable** bagi keadaan ECS, dengan pemetaan **satu dokumen per entity**: seluruh komponen sebuah entity hidup sebagai sub-dokumen di bawah field `cmp`. Identitas persisten memakai **`ObjectId`** sebagai `pid` (RFC-0034: indeks World tetap ephemeral). Jembatan komponen → dokumen adalah **`arke::Serialize`/`Value` yang sudah ada** — **tanpa** crate proc-macro baru.

Rilis pertama (0.1) mencakup **fondasi saja**: pemetaan `Value` ↔ BSON, `MongoStore` (connect/register/ensure_indexes), CRUD per-operasi dengan optimistic-lock, dan `load`/`save` seluruh World. Query builder, relasi, cache, dan tulis-balik inkremental ditunda ke RFC lanjutan (§8).

`arke` core **tetap 0-dependensi** (STD-0003); seluruh dependensi driver terkurung di crate adapter.

## Motivasi

`arke-postgres` (RFC-0021) membuktikan model **working-set**: `World` adalah salinan-kerja in-memory yang di-*materialize* dari basis data otoritatif, dijalankan deterministik, lalu ditulis-balik. Model itu tak terikat pada Postgres — yang terikat Postgres hanyalah *pemetaannya* (tabel berkolom-tipe, SQL, `BIGSERIAL`).

Sebagian pengguna sudah menjalankan MongoDB sebagai basis data utama dan tidak ingin menambah Postgres hanya untuk mempersist ECS. Bagi mereka, pilihan hari ini adalah snapshot JSON blob (RFC-0007) yang tak bisa di-query atau di-*update* parsial.

Ada pula alasan struktural: model dokumen **cocok secara alami** dengan bentuk data ECS. Sebuah entity adalah kumpulan komponen heterogen — persis "dokumen dengan sub-dokumen opsional". Di Postgres, mengambil satu entity utuh menuntut join lintas-N tabel komponen; di Mongo ia satu `find_one`. Dan mutasi satu entity menjadi **atomik tanpa transaksi**, karena Mongo menjamin atomisitas per-dokumen.

### Tegangan yang diakui

- **Bukan paritas fitur dengan `arke-postgres`.** Adapter ini sengaja mulai dari fondasi. Meniru API relasional (query builder typed, join relasi, `WITH RECURSIVE`) ke dalam model dokumen tanpa lebih dulu punya pengalaman pemakaian berisiko mengunci desain yang salah.
- **`save` seluruh World tidak atomik lintas-entity** (§5). Ini janji yang lebih lemah dari `arke-postgres` dan harus tertulis eksplisit, bukan tersirat.
- **Evolusi skema tidak dijawab v1** (§6). Mongo tak punya `ALTER TABLE`; strategi migrasinya berbeda secara fundamental dan pantas dapat RFC sendiri.
- **Tipe `pid` berbeda antar-adapter** (`ObjectId` vs `i64`). Data tidak berpindah begitu saja antara `arke-postgres` dan `arke-mongo`; keduanya adalah sumber kebenaran alternatif, bukan dua muka dari satu penyimpanan.

## Usulan rinci

### 1. Batas crate

```
arke            (core, 0-dep, STD-0003)  ← tak berubah
arke-mongo      (adapter)  → depends: arke + mongodb (driver resmi, async/tokio) + bson
```

`arke-mongo` **tidak** tunduk STD-0003 — ia gerbang dependensi, sejajar `arke-postgres` (RFC-0021 §1). Ia hanya memakai API publik `arke`: `Serialize`/`Value`, `Entity`, `World::spawn`/`get`/`insert`, query.

`unsafe_code = "forbid"`. Gerbang CI standalone-core tetap di-scope `-p arke`, jadi tak terpengaruh.

### 2. Tanpa crate derive — `arke::Serialize` sebagai jembatan

`arke-postgres` butuh `#[derive(PgComponent)]` karena kolom-tipe SQL menuntut **tipe Rust konkret tiap field** — informasi yang `Value` (dynamically-typed) buang (RFC-0021 §3). Model dokumen tidak menuntut itu: BSON sendiri dynamically-typed, dan bentuk yang dibutuhkan persis pohon `Value`.

`#[derive(arke::Serialize)]` sudah tersedia di core (RFC-0009), lengkap dengan atribut field (RFC-0011) dan `rename_all` (RFC-0012). Maka yang tersisa hanyalah trait tipis untuk metadata yang tak diketahui `Serialize`:

```rust
/// Komponen yang dipersist ke MongoDB (RFC-0035).
pub trait MongoComponent: arke::Serialize {
    /// Kunci komponen di bawah `cmp` (mis. `"position"`). Wajib eksplisit —
    /// tak ada default: sanitasi `type_name` tak bisa dilakukan di konteks const.
    const NAME: &'static str;
    /// Indeks atas path `cmp.<NAME>.<field>`; kosong bila tak ada.
    const INDEXES: &'static [IndexDef] = &[];
}

/// Spesifikasi satu indeks.
pub struct IndexDef {
    /// Nama field di dalam komponen.
    pub field: &'static str,
    /// Arah urutan.
    pub dir: Dir,        // Asc | Desc
    /// Apakah indeks `unique`.
    pub unique: bool,
}
```

Ditulis tangan (tiga baris), atau lewat makro deklaratif ringan yang mengisi `NAME` dari literal yang diberikan — bukan proc-macro, cukup `macro_rules!` di crate adapter:

```rust
mongo_component!(Position => "position");
mongo_component!(Health   => "health", indexes: [IndexDef::asc("hp")]);
```

**Trade-off yang diterima:** deklarasi indeks jadi manual (`IndexDef::asc("x")`) alih-alih atribut `#[mongo(index)]`, dan tak ada jaminan kompilasi bahwa `field` benar-benar ada pada struct — Mongo dengan senang hati mengindeks path yang tak pernah terisi, jadi salah ketik tidak menimbulkan error, hanya indeks yang diam-diam kosong.

Mitigasi v1 (Am. 1): saat komponen diserialisasi, `Registry` membandingkan kunci hasil `to_value()` terhadap `INDEXES` lewat `debug_assert!` — gagal keras di build debug dan di seluruh uji, nol biaya di rilis. Pemeriksaan saat `register` tak mungkin: tanpa nilai contoh, `Value` tak bisa dihasilkan. Pilihan `debug_assert!` menghindari menyeret crate logging ke dalam adapter hanya untuk satu peringatan.

Bila kelak query builder typed masuk — yang memang butuh metadata per-field sungguhan — derive ditambahkan saat itu, dengan alasan nyata.

### 3. Identitas persisten: `ObjectId`

Mengikuti RFC-0034: **indeks World selalu ephemeral, `pid` adalah kunci persisten tunggal.** Di sini `pid` = `ObjectId`:

```rust
/// Identitas persisten sebuah entity di MongoDB.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Pid(pub bson::oid::ObjectId);
```

`ObjectId` dialokasikan **klien** (12 byte: timestamp + random + counter), unik tanpa koordinasi server. Konsekuensi yang diinginkan: `create` cukup **satu round-trip** (tak perlu meminta id lebih dulu) dan aman untuk multi-replica tanpa titik kontensi.

Store memelihara pemetaan per working-set aktif, pola RFC-0034 §2:

```rust
pid_of:    HashMap<Entity, Pid>,
entity_of: HashMap<Pid, Entity>,
```

Diisi oleh `create`/`fetch`/`load`. `World` selalu diisi lewat `spawn()` biasa → indeks dense & lokal.

### 4. Model dokumen

Satu koleksi `arke_entities`, satu dokumen per entity:

```js
{
  _id: ObjectId("66aa…"),   // pid
  version: 3,               // optimistic lock
  cmp: {
    position: { x: 1.0, y: 2.0 },
    health:   { hp: 80, max: 100 }
  }
}
```

**Penamaan komponen** (`MongoComponent::NAME`): konvensi yang dianjurkan adalah segmen terakhir nama tipe dalam `snake_case` — `game::unit::Position` → `"position"`. Aturan yang sama dengan tabel `cmp_position` di `arke-postgres`, supaya kedua adapter terbaca konsisten oleh manusia. Bedanya, di sini nama ditulis eksplisit alih-alih diturunkan otomatis — yang sekaligus menghilangkan risiko tabrakan diam-diam antara dua tipe bernama sama dari modul berbeda.

**Pemetaan `arke::Value` → BSON**, satu-satu:

| `Value` | BSON |
| --- | --- |
| `Null` | `Null` |
| `Bool(b)` | `Boolean` |
| `Int(i64)` | `Int64` |
| `Float(f64)` | `Double` |
| `Text(s)` | `String` |
| `List(v)` | `Array` |
| `Map(kv)` | `Document` |

Round-trip setia (STD-0002): `to_value` → BSON → `Value` → `from_value` menghasilkan komponen yang sama. Perhatikan `Value::Map` adalah `Vec<(String, Value)>` (**terurut**) sedangkan BSON `Document` juga menjaga urutan sisip — jadi round-trip mempertahankan urutan field.

**Batasan yang diketahui, dicatat eksplisit:**

- `Value::Int` adalah `i64`. `u64`/`usize` di atas `i64::MAX` sudah lossy **di core** (RFC-0009); `arke-mongo` mewarisi batas itu dan tidak memperburuknya. (`arke-postgres` lolos karena punya `NUMERIC(20)` — pemetaan kolom-tipe punya informasi yang `Value` tak punya.)
- `f32` melewati `Value::Float(f64)`; `f32 → f64 → f32` eksak, jadi round-trip tetap setia.
- Nama field BSON **tak boleh** mengandung `.` atau berawalan `$`. Divalidasi saat penulisan pertama → `MongoError::InvalidName`, gagal keras.

**Indeks** dibuat atas path bersarang `cmp.<name>.<field>` dan bersifat **sparse secara alami**: dokumen tanpa komponen itu tak masuk indeks — selaras dengan semantik ECS "entity yang memiliki komponen X".

### 5. API `MongoStore`

Permukaan mengikuti `PgStore` supaya pengguna lintas-adapter tak perlu belajar model baru, kecuali di tempat yang memang berbeda.

```rust
let mut store = MongoStore::connect("mongodb://localhost", "game").await?;
store.register::<Position>();
store.register::<Health>();
store.ensure_indexes().await?;      // idempoten
```

`register::<T: MongoComponent + Component>()` menyimpan closure type-erased — pola registry yang sama dengan `arke-postgres`:

```rust
extract: fn(&World, Entity) -> Option<Value>,
insert:  fn(&mut World, Entity, &Value) -> Result<(), ()>,
```

**Per-operasi** (stateless, aman multi-replica — RFC-0034 §3):

```rust
let pid = store.create(&world, entity).await?;                 // insert_one, ObjectId klien
let e   = store.fetch(&mut world, pid).await?;                 // Option<Entity>
store.update(&world, entity, pid).await?;                      // last-write-wins, $inc version
store.update_checked(&world, entity, pid, expected).await?;    // -> Err(Conflict) bila versi bergeser
store.remove(pid).await?;                                      // delete_one
store.version_of(pid).await?;                                  // Option<i64>, untuk retry
```

`update_checked` menjalankan `find_one_and_update` dengan filter `{ _id, version: expected }`; 0 dokumen terpengaruh → `MongoError::Conflict`. Sejajar `PgStore::update_entity` (RFC-0021 §5): kebijakan resolusi (retry/LWW/merge) diserahkan pemanggil.

**Seluruh World:**

```rust
store.save(&world).await?;          // update_one per entity + delete_many
store.load(&mut world).await?;      // find({}).sort({ _id: 1 })
```

(`save`/`load` menerima `&mut self` karena keduanya memperbarui peta `pid ↔ Entity`.)

**`save` menulis per-sub-field, bukan mengganti `cmp` utuh:**

```js
{ $set:   { "cmp.position": {…}, "cmp.health": {…} },
  $unset: { "cmp.stunned": "" },               // komponen terdaftar yang hilang dari World
  $inc:   { version: 1 } }

Alasannya penting: mengganti `cmp` utuh akan **menghapus diam-diam** komponen yang ditulis service lain atau versi aplikasi lain yang mendaftarkan himpunan komponen berbeda. Karena itu `$unset` hanya menyasar komponen **terdaftar** yang hilang dari World; komponen tak terdaftar sengaja dibiarkan utuh.

`save` juga menghapus dokumen yang `pid`-nya ada di `entity_of` tetapi entity-nya sudah tak ada di World (despawn) — satu `delete_many` setelah seluruh upsert.

**Atomisitas.** Operasi per-dokumen berurutan tanpa transaksi sesi (lihat Amandemen 1): atomik **per-entity**, bukan per-World. Kegagalan di tengah meninggalkan sebagian entity tertulis. Ini konsekuensi sadar agar `mongod` **standalone** cukup untuk dev, tes, dan produksi sederhana — transaksi multi-dokumen Mongo menuntut replica set. Janji ini **lebih lemah** dari `arke-postgres` dan **wajib** tertulis di rustdoc `save` dan di README, bukan hanya di RFC ini. Transaksi opsional → RFC lanjutan (§8).

**Error berkonteks** (STD-0008 — menyebut entity/komponen yang terlibat):

```rust
pub enum MongoError {
    Driver(mongodb::error::Error),
    Conflict       { pid: Pid, expected: i64, actual: Option<i64> },
    Decode         { pid: Pid, component: &'static str },
    InvalidName    { component: &'static str, field: String },
    DuplicateField { component: &'static str, field: String },  // Am. 2
}
```

### 6. Determinisme, fidelity & skema

- **`load` deterministik**: `.sort({ _id: 1 })` → urutan materialisasi identik antar-run (STD-0005), sejajar `ORDER BY pid` di Postgres. `ObjectId` punya urutan total yang stabil.
- **Eksekusi ECS tetap deterministik** atas working-set; Mongo adalah batas I/O, bukan jalur panas (RFC-0021 §7).
- **`schema_version`** (STD-0001) disimpan di koleksi `arke_meta`.
- **Dokumen yang gagal decode → `Err(Decode { pid, component })`, bukan dilewati diam-diam.** Kehilangan komponen tanpa suara jauh lebih berbahaya daripada gagal keras: ia lolos ke `save` berikutnya dan menjadi kehilangan data permanen.
- **Evolusi skema tidak dijawab v1.** Mongo schemaless dan tak punya `ALTER TABLE`; menambah field non-`Option` ke komponen yang sudah punya dokumen lama akan memunculkan `Decode`. Jalur yang ada hari ini: pakai `Option<T>` untuk field baru, atau jalankan skrip backfill. Strategi migrasi yang benar → RFC lanjutan (§8).

### 7. Pengujian

Kunci desainnya: **memisahkan fungsi murni dari I/O**, sehingga mayoritas tes jalan tanpa database dan TDD bisa dimulai sebelum driver disentuh.

**Lapis 1 — tanpa DB** (job CI default):

- round-trip `Value` ↔ BSON tiap varian, termasuk `Map`/`List` bersarang
- `document_for(&world, entity) -> Document` dan kebalikannya — fungsi murni atas registry
- pembangunan operasi `$set`/`$unset` untuk `save`
- validasi nama field (`.` / awalan `$` ditolak)
- deteksi `NAME` yang bertabrakan saat `register`
- pembangunan spesifikasi indeks dari `INDEXES` (path `cmp.<name>.<field>`)

**Lapis 2 — MongoDB nyata**, di-skip bila `MONGODB_URI` tak diset — persis pola `DATABASE_URL` di `arke-postgres`, agar CI tanpa Mongo tetap hijau:

- `create`/`fetch` round-trip setia
- `update_checked` → konflik versi terdeteksi
- `remove` + `save` menghapus entity yang di-despawn
- `save` **tidak** menghapus komponen tak terdaftar (regresi-guard untuk keputusan §5)
- `load` deterministik: dua materialisasi menghasilkan urutan identik
- `ensure_indexes` idempoten

CI mendapat job `mongo` dengan service container `mongo:7` **standalone** — tanpa replica set, konsekuensi langsung keputusan §5.

### 8. Rencana bertahap

| Fase | Isi |
| --- | --- |
| **v1 (0.1)** — RFC ini | Pemetaan `Value` ↔ BSON; `MongoComponent` + `IndexDef`; `MongoStore` connect/register/`ensure_indexes`; CRUD per-op + optimistic-lock; `load`/`save` seluruh World. |
| **v2** (RFC lanjutan) | Query builder atas path `cmp.<name>.<field>` — bentuknya ditentukan **setelah** ada pengalaman pemakaian v1, bukan disalin dari RFC-0030. |
| **v3** (RFC lanjutan) | Relasi entity (`Pid` sebagai referensi), `$lookup`/`$graphLookup` untuk nested & rekursif. |
| **v4** (RFC lanjutan) | `save_incremental`; transaksi multi-dokumen opsional (replica set); cache read-through (RFC-0033); strategi evolusi skema. |

Penundaan ini **sadar**, bukan kelupaan: mengunci API query untuk model dokumen sebelum ada pemakaian nyata adalah cara termurah untuk salah.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| **Dokumen per entity (dipilih)** | Idiomatik Mongo; fetch/update entity 1 round-trip & atomik tanpa transaksi; query lintas-komponen tanpa `$lookup` | Menyimpang dari model `arke-postgres` → query builder harus didesain ulang | Dipilih: memakai kekuatan Mongo alih-alih meniru relasional |
| Satu koleksi per tipe komponen | Cermin `arke-postgres`; API bisa 1:1 | Query lintas-komponen butuh `$lookup` (lambat, tak transaksional secara default); update entity menyentuh N koleksi | Memakai Mongo seperti "Postgres yang lebih lemah" |
| Hibrida (per-entity + koleksi opsional per komponen) | Fleksibel untuk komponen besar/jarang | Dua jalur kode & dua semantik sejak v1 | Kompleksitas ganda tanpa kebutuhan terbukti; bisa jadi RFC lanjutan |
| **`ObjectId` sebagai `pid` (dipilih)** | Alokasi klien → `create` 1 round-trip; unik tanpa koordinasi; `_id` idiomatik | Tipe `pid` beda dari `arke-postgres` (`i64`) | Dipilih: kontensi nol; keseragaman tipe lintas-adapter bukan tujuan |
| `i64` via koleksi counter (`findAndModify $inc`) | `pid` seragam dengan `arke-postgres` | Round-trip ekstra per `create`; satu dokumen counter jadi titik kontensi tulis global | Menukar skalabilitas tulis demi keseragaman kosmetik |
| UUID v7 | Terurut waktu; portabel lintas-backend | Bukan `_id` idiomatik Mongo; indeks lebih besar dari `ObjectId` | `ObjectId` sudah memberi urut-waktu + alokasi klien, lebih ringkas |
| **Operasi per-dokumen tanpa transaksi (dipilih)** | Jalan di `mongod` standalone; dev & CI cukup satu kontainer | `save` tidak all-or-nothing lintas-entity | Dipilih + didokumentasikan eksplisit; transaksi → RFC lanjutan |
| Transaksi multi-dokumen | `save` all-or-nothing, sejajar `arke-postgres` | Menuntut replica set untuk dev, tes, dan produksi | Beban setup tak sepadan untuk v1 fondasi |
| Deteksi otomatis (transaksi bila replica set) | Portabel | Jaminan bergantung deployment; dua jalur kode & dua semantik | Semantik yang berubah diam-diam menurut deployment sulit dinalar |
| **Tanpa crate derive (dipilih)** | Memakai ulang `derive(Serialize)` yang sudah matang; nol proc-macro baru untuk dipelihara | Indeks dideklarasikan manual; salah-ketik nama field tak tertangkap kompilasi | Model dokumen tak butuh tipe per-field; derive ditambahkan bila query builder typed menuntutnya |
| `#[derive(MongoComponent)]` (crate `arke-mongo-derive`) | `#[mongo(index)]` ergonomis; nama field tervalidasi | Parser atribut kedua untuk dipelihara, tanpa imbalan di v1 | Ditunda sampai ada kebutuhan nyata (v2) |
| Snapshot blob JSON di satu dokumen | Sepele; memakai ulang RFC-0007 | Tak query-able, tak bisa update parsial | Gagal syarat "sumber kebenaran" |

## Dampak

- **Kompatibilitas / migrasi:** murni **aditif & terisolasi**. `arke` core tak berubah dan tetap 0-dep (STD-0003); `arke-postgres` tak tersentuh. Crate baru dimulai di `0.1.0`, di luar janji stabilitas 1.0 core (RN-0004).
- **Keamanan:** adapter murni safe Rust (`forbid(unsafe_code)`); tak menyentuh `unsafe` core. Menambah satu dependensi driver (`mongodb` + `bson`) — terkurung di crate adapter, sejajar `sqlx` di `arke-postgres`.
- **Konsekuensi pada invarian:** memperluas **portabilitas & kepemilikan data** ke MongoDB tanpa mengorbankan determinisme (Mongo adalah batas I/O — `load` terurut `_id`, STD-0005). Menegaskan bahwa model working-set RFC-0021 **tidak terikat** pada basis data relasional. STD-0002 (round-trip setia) berlaku lewat `Value` ↔ BSON.
- **Beban pemeliharaan:** adapter kedua berarti dua permukaan API untuk dijaga tetap koheren. Dimitigasi dengan menjaga v1 tetap kecil dan menolak paritas fitur prematur.

## Pertanyaan terbuka

- **Validasi `IndexDef::field`** (§2): peringatan runtime terasa lemah. Apakah pemeriksaan saat `ensure_indexes` terhadap satu dokumen contoh yang sudah ada lebih baik, atau justru menyesatkan saat koleksi masih kosong?
- **Ambang ukuran dokumen 16 MB.** Entity dengan komponen `Vec<T>` besar bisa menabraknya. Perlukah v1 memberi peringatan proaktif, atau cukup membiarkan error driver muncul apa adanya?
- **`load` seluruh koleksi** tanpa paging: untuk koleksi besar ini memuat semua ke memori. `load_where` (setara RFC-0021 v3) ditunda ke v2 — apakah v1 setidaknya perlu `load_ids(&[Pid])`?
- ~~**`NAME` yang bertabrakan.**~~ **Diputuskan (Am. 1): `panic`.** `register` bukan jalur yang bisa gagal karena data — dua komponen dengan `NAME` sama adalah bug programmer, dan `register` mengembalikan `&mut Self` untuk chaining (pola `PgStore::register`). Gagal sedini mungkin, dengan pesan yang menyebut nama yang bertabrakan.
- **Bentuk query builder v2** untuk model dokumen — apakah `Filter` bergaya RFC-0030 masih cocok, atau path bersarang menuntut abstraksi lain?
- **Feature `sync`** (driver blocking) untuk pengguna non-async — pertanyaan yang sama masih terbuka di RFC-0021.
- **Penjaga `WorldId` untuk pemetaan lintas-World.** `MongoStore::pid_of`/`entity_of` mengunci `Entity` — handle yang cuma bermakna di dalam satu `World` — ke `pid` persisten. Review kode blok store (Task 15b) menemukan bahwa ini bisa dilanggar tanpa pernah memanggil `save` dengan `World` kedua secara sengaja: `fetch` ke `World` sekali-pakai untuk inspeksi cukup merebut tautan `pid` dari entity aslinya (entity itu kehilangan tautannya, `save` berikutnya mencetak pid baru dan menghapus dokumen lama), dan dua `World` independen yang sama-sama men-spawn entity pertamanya menghasilkan `Entity` yang identik sehingga `save` yang kedua diam-diam menimpa dokumen milik yang pertama. Keduanya didokumentasikan sebagai bahaya pada rustdoc `MongoStore` dan dipatok oleh tes regresi (`bahaya_fetch_ke_world_scratch_merebut_pid`, `bahaya_entity_handle_bertabrakan_antar_world`) di `arke-mongo/tests/store.rs`, tapi tak ada penjaga runtime — mendeteksinya menuntut cara membedakan `World` satu dari `World` lain, yang tak ada di `arke` core hari ini. Penjaga yang sungguhan (mis. `store.bind_world(&world)` yang menolak `Entity` dari `World` lain) menuntut sebuah `WorldId` di **core `arke`** — perubahan `arke`, bukan perubahan adapter ini — sehingga keputusannya dicatat di sini, bukan hilang begitu saja.

  **Diselesaikan (arke 0.7 / arke-mongo 0.1):** core `arke` kini menyediakan
  `World::id()` (`WorldId`, unik per-proses). `MongoStore` menautkan diri ke
  `World` pertama yang dilayaninya dan menolak `World` lain dengan
  `MongoError::WorldMismatch`; `MongoStore::fork()` memberi store baru untuk
  `World` lain. Dua tes yang dulu mematok bahayanya kini memverifikasi
  penjaganya (`fetch_ke_world_scratch_ditolak_bukan_merebut_pid`,
  `entity_handle_bertabrakan_antar_world_ditolak`).
## Keputusan

**Diterima.** Lihat [ADR-0035](../ADR/ADR-0035-arke-mongo-adapter.md). M-32 diselesaikan lewat TDD dari pemetaan `Value` ↔ BSON (§7 Lapis 1, tanpa DB) hingga `MongoStore` penuh (CRUD per-operasi, `save`/`load` seluruh World). Dua amandemen lahir selama implementasi: Amandemen 1 mengganti `bulkWrite` dengan operasi per-dokumen berurutan untuk `save` (portabilitas ke `mongod` 7 standalone) dan memutuskan `NAME` bertabrakan → `panic` saat `register`; Amandemen 2 menambah validasi `NAME` sebagai nama field BSON saat `register` dan `MongoError::DuplicateField` untuk field yang saling menimpa diam-diam.
