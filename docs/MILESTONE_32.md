# Milestone 32 — Adapter MongoDB, fondasi (RFC-0035)

> Crate `arke-mongo`: MongoDB sebagai sumber kebenaran dengan pemetaan satu
> dokumen per entity. Fondasi saja — query builder, relasi, cache, dan
> inkremental ditunda ke RFC lanjutan.

## Tujuan

Memberi pengguna yang sudah menjalankan MongoDB sebagai basis data utama
jalur persist ECS yang query-able dan bisa di-*update* parsial, tanpa
menambah Postgres hanya untuk itu — membuktikan bahwa model working-set
RFC-0021 tak terikat pada basis data relasional.

## Ruang lingkup

**Termasuk:**
- Crate baru `arke-mongo`: pemetaan `Value` ↔ BSON, validasi nama field BSON,
  trait `MongoComponent` + `IndexDef` + makro `mongo_component!` (tanpa
  proc-macro baru — memakai `#[derive(arke::Serialize)]` yang sudah ada).
- `MongoStore`: `connect`/`register`/`ensure_indexes`; CRUD per-operasi
  (`create`/`fetch`/`update`/`update_checked`/`version_of`/`remove`) dengan
  optimistic-lock `version`; `save`/`load` seluruh World.
- `pid` = `ObjectId`, dialokasikan klien (RFC-0034: indeks World ephemeral).
- CI: job `mongo`, service container `mongo:7` standalone + uji integrasi,
  dengan `ARKE_REQUIRE_MONGO=1` agar job yang salah konfigurasi tak bisa
  hijau sambil diam-diam tak menjalankan satu asersi pun.

**Tidak termasuk (sengaja ditunda):** query builder bertipe, relasi entity,
nested/recursive, cache read-through, `save_incremental`, transaksi
multi-dokumen, strategi evolusi skema. `drop_database`/metode destruktif lain
juga sengaja **tidak** masuk API publik — tak ada tempatnya di crate yang
dipublikasikan; tes memakai klien driver mentah untuk setup/teardown.

## Artefak yang dihasilkan

| Artefak | Bentuk |
| --- | --- |
| RFC-0035 | Proposal adapter MongoDB dokumen-per-entity, Accepted (+ Amandemen 1 & 2) |
| ADR-0035 | Keputusan: dokumen-per-entity, `ObjectId`, tanpa transaksi, tanpa derive |
| `arke-mongo` 0.1.0 | Implementasi (`bson_map`, `error`, `registry`, `store`) + tes dua lapis + README |
| Perbaikan core `arke` | `World::insert` jadi upsert atas komponen yang sudah dimiliki (bug ditemukan lewat pemakaian nyata adapter ini) |

## Kriteria selesai (Definition of Done)

- [x] Lapis 1 (tanpa DB) hijau: round-trip `Value` ↔ BSON, validasi nama field,
      `cmp_doc`/`apply`/`update_ops`/`index_models`, deteksi `NAME` bertabrakan
      dan field duplikat. 35 tes di `arke-mongo/tests/mapping.rs`.
- [x] Lapis 2 (MongoDB nyata) hijau: `create`/`fetch` setia, konflik versi
      terdeteksi, `remove`, `save` menghapus despawn dan **tidak** menghapus
      komponen tak terdaftar, `load` deterministik, `ensure_indexes` idempoten,
      plus dua tes regresi yang mematok bahaya lintas-`World`. 22 tes di
      `arke-mongo/tests/store.rs`, dan 2 doctest (`lib.rs` + `README.md`).
- [x] Tes Lapis 2 di-skip bersih tanpa `MONGODB_URI`; dengan `ARKE_REQUIRE_MONGO=1`
      set tanpa `MONGODB_URI`, tes panic alih-alih diam-diam skip — mencegah
      job CI yang salah konfigurasi melaporkan hijau tanpa menjalankan asersi.
- [x] `cargo fmt -p arke-mongo --check` dan
      `cargo clippy -p arke-mongo --all-targets --all-features -- -D warnings`
      hijau. Job `mongo` (service container `mongo:7` standalone) ada di CI.
      **Catatan:** `cargo fmt --all -- --check` di seluruh workspace saat ini
      menunjukkan drift pra-eksisting di `arke-postgres` (bukan hasil kerja
      milestone ini, tak tersentuh oleh commit-commit di atas) — di luar
      ruang lingkup untuk diperbaiki di sini.
- [x] Core `arke` tetap 0-dependensi pihak-ketiga (STD-0003) — `arke-mongo`
      adalah gerbang dependensi (`mongodb`, `bson`, `futures-util`), sejajar
      `arke-postgres`.
- [x] Non-atomisitas `save` terdokumentasi di rustdoc **dan** README
      (bagian "Atomisitas — baca ini" di `arke-mongo/README.md`).
- [x] Bahaya satu-store-satu-`World` (`pid_of`/`entity_of` terkunci ke
      `Entity`, handle yang hanya bermakna di dalam satu `World`)
      terdokumentasikan eksplisit di rustdoc `MongoStore` dan README, dan
      dipatok oleh tes regresi (`bahaya_fetch_ke_world_scratch_merebut_pid`,
      `bahaya_entity_handle_bertabrakan_antar_world`). **Tak ada penjaga
      runtime** — butuh `WorldId` di core `arke`, dicatat sebagai pertanyaan
      terbuka RFC-0035, bukan diselesaikan di milestone ini.

## Ketergantungan

- **Butuh selesai lebih dulu:** RFC-0035 (Accepted) + ADR-0035.
- **Membuka jalan bagi:** query builder dokumen (v2), relasi entity (v3),
  `save_incremental` + transaksi opsional + cache read-through (v4).

## Catatan yang tak diprediksi rencana awal

- **Dua amandemen RFC-0035 lahir selama implementasi, bukan sebelum.**
  Amandemen 1: `save` memakai operasi per-dokumen berurutan, bukan
  `bulkWrite` — `bulk_write` driver Rust butuh MongoDB server 8.0, kontradiktif
  dengan target `mongod` 7 standalone di §7 RFC itu sendiri; juga memutuskan
  `NAME` bertabrakan → `panic` saat `register`. Amandemen 2: `NAME` divalidasi
  sebagai nama field BSON saat `register` (kosong / mengandung `.` / berawalan
  `$` → panic), dan varian error baru `MongoError::DuplicateField` ditambahkan
  untuk field yang saling menimpa diam-diam di `Document`.
- **`MongoError` berakhir dengan enam varian**, bukan lima seperti draf awal
  RFC: `Driver`, `Conflict`, `Decode`, `InvalidName`, `DuplicateField`, dan
  `Missing` (ditambahkan saat `update` menyasar dokumen yang sudah dihapus
  penulis lain — sebelumnya berisiko `Ok(())` yang diam-diam membuang tulisan).
- **Bug nyata ditemukan dan diperbaiki di core `arke`** (`3c588ea`):
  `World::insert` atas komponen yang sudah dimiliki menyusun himpunan komponen
  tujuan dengan `push` + `sort_unstable` tanpa dedup, menghasilkan
  `ComponentId` kembar. Di build debug ini `debug_assert`-panic; di **build
  rilis** archetype dua-kolom rusak terbentuk **diam-diam**, dan `get`/query
  membaca kolom pertama sehingga mengembalikan **nilai basi**. `arke-mongo`
  adalah konsumen pertama yang mengeksekusi jalur ini sampai menyingkap bug.
  `insert` sekarang bersifat upsert. Entri `### Fixed` untuk ini sudah ada di
  `CHANGELOG.md` di bawah `## [Unreleased]` — tidak diduplikasi di sini.
- **`drop_database` sengaja tidak dikirim** ke API publik — metode destruktif
  tak punya tempat di permukaan crate yang dipublikasikan; tes integrasi
  memakai klien driver `mongodb` mentah untuk setup/teardown alih-alih.
- **`connect` melakukan `ping` eagerly** sebelum mengembalikan, menyamai
  `PgStore::connect` — `uri` ke host yang tak terjangkau gagal saat `connect`,
  bukan diam-diam `Ok` lalu timeout buram di operasi pertama.
- **Satu bahaya nyata, terdokumentasi, dan tak dijaga tetap ada**: satu
  `MongoStore` harus melayani satu `World`; `pid_of`/`entity_of` mengunci
  `Entity`, yang hanya bermakna di dalam satu `World`. Tiga mode kegagalan
  konkret didokumentasikan di rustdoc `MongoStore` dan README, dan dipatok
  oleh tes regresi — tapi tak ada penjaga runtime di v1. Penjaga sungguhan
  menuntut `WorldId` di core `arke`, sudah tercatat di pertanyaan terbuka
  RFC-0035.

## Pertanyaan terbuka

Lihat RFC-0035 §Pertanyaan terbuka — terutama validasi `IndexDef::field`,
ambang dokumen 16 MB, `load` tanpa paging, bentuk query builder v2, dan
penjaga `WorldId` untuk pemetaan lintas-`World`.
