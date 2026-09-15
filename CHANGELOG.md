# Changelog

Semua perubahan penting pada `arke` didokumentasikan di sini. Format mengikuti
[Keep a Changelog](https://keepachangelog.com/id/1.1.0/); proyek menganut
[Semantic Versioning](https://semver.org) (lihat STD-0010). Riwayat lengkap tiap
rilis juga ada di [GitHub Releases](https://github.com/dalfiannur/arke/releases).

## [Unreleased]

### Added

- **Crate baru `arke-mongo` 0.1.0** — adapter MongoDB dengan pemetaan **satu
  dokumen per entity** (komponen sebagai sub-dokumen di bawah `cmp`);
  identitas persisten `pid` = `ObjectId` yang dialokasikan klien
  ([RFC-0035](docs/RFC/RFC-0035-arke-mongo-adapter.md), [ADR-0035](docs/ADR/ADR-0035-arke-mongo-adapter.md)).
  Tanpa derive baru: `#[derive(arke::Serialize)]` yang sudah ada menjadi
  jembatan ke BSON — hanya trait tipis `MongoComponent` + `IndexDef` lewat
  makro `mongo_component!` yang ditambahkan. `MongoStore` menyediakan
  `connect`/`register`/`ensure_indexes`, CRUD per-operasi
  (`create`/`fetch`/`update`/`update_checked`/`version_of`/`remove`) dengan
  optimistic-lock `version`, dan `save`/`load` seluruh World. Core `arke` tak
  berubah dan tetap 0-dependensi (STD-0003).

  **Batasan yang diketahui:** `save` seluruh World memakai operasi per-dokumen
  berurutan tanpa transaksi — atomik **per-entity**, bukan per-World, sehingga
  `mongod` standalone sudah cukup; kegagalan di tengah meninggalkan sebagian
  entity tertulis. Untuk `save` yang all-or-nothing, pakai `arke-postgres`.
  Satu `MongoStore` harus melayani satu `World` — `pid_of`/`entity_of`
  mengunci `Entity`, handle yang hanya bermakna di dalam satu `World`; tiga
  mode kegagalan konkret didokumentasikan di rustdoc dan README serta dipatok
  tes regresi, tapi belum ada penjaga runtime (butuh `WorldId` di core,
  dicatat sebagai pertanyaan terbuka RFC-0035). `drop_database` sengaja tidak
  ada di API publik. Query builder, relasi, cache, `save_incremental`, dan
  strategi evolusi skema ditunda ke RFC lanjutan.

- **`arke-postgres`: `Query::count()` dan `contains`/`contains_all` untuk field
  array JSONB.** `count()` menjalankan `SELECT COUNT(*)` dengan `WHERE` yang sama
  dengan `load()` (tanpa `ORDER BY`/`LIMIT`/`OFFSET`) — total halaman tanpa
  `load_where` + `sqlx` mentah; `Filter` kini `Clone` agar satu filter dipakai
  untuk `count()` dan `load()`. `#[derive(PgComponent)]` kini juga menghasilkan
  token untuk field non-skalar (`Field<Self, T>` atas kolom JSONB); untuk
  `Vec<V>`/`Option<Vec<V>>` tersedia `contains(v)`/`contains_all(iter)` →
  `col @> $n::jsonb`. Additif; `load_where` tetap.
- **`arke-postgres`: `#[pg(index)]` pada kolom JSONB kini membuat index GIN**
  (bukan btree) agar `contains`/`@>` ter-index; `migrate` mengganti index
  btree lama bernama sama di kolom JSONB dengan GIN (DROP + CREATE, idempoten
  sesudahnya). `#[pg(unique)]` tetap btree karena GIN tak mendukung UNIQUE.

- **`arke-postgres`: transaksi milik pemanggil (`PgStore::begin` → `PgTx`).**
  Pola *cek-lalu-tulis* atomik (mis. tolak booking yang bentrok) kini tanpa SQL
  mentah: `tx.advisory_lock(key)` (`pg_advisory_xact_lock`, lepas otomatis),
  `Query::exists_in`/`count_in(&mut tx)` membaca lewat koneksi tx, dan
  `commit_insert_in`/`commit_update_in`/`remove_in(&mut tx, …)` menulis tanpa
  commit; `tx.commit()`/`rollback()`, drop = rollback. `Query::exists()` (pool)
  ikut ditambahkan. Op lama (`commit_insert` dkk.) kini wrapper begin→`_in`→commit
  — perilaku sama.

- **`arke-postgres`: pola World per-request.** `PgStore::fork()` — store baru
  berbagi pool/registry/cache dengan jembatan pid↔entity kosong (satu template
  di state, `fork()` per handler; tanpa `Mutex`). `PgStore::pid_of(entity)` /
  `entity_of(pid)` kini publik, dan `Query::load_pids(world)` mengembalikan
  `(pid, Entity)` entity yang dimuat — id persisten untuk `remove`/respons API
  tanpa `load_where`. README: resep POST/GET/PATCH/DELETE dengan id publik
  kolom `#[pg(unique)]` dan `save_incremental` relatif ke working set.

- **`arke-postgres`: semi-join by value, mutasi massal, agregasi.**
  `Field::in_where(Field<R, V>, Filter<R>)` → `col IN (SELECT other FROM cmp_R
  WHERE …)` untuk hubungan lewat nilai kolom tanpa `Ref`. `store.update_where::<T>()
  .filter(..).set(T::col(), v).execute()` dan `delete_where::<T>()` (menghapus
  entity, cascade) — transaksional, menaikkan `version` entity terdampak,
  meng-invalidate cache, ada `execute_in(&mut tx)`. Terminal agregat
  `sum/min/max/avg::<A>(T::col())` dan `group_by(T::key()).count()/sum()/…`
  dengan `A: FromPgScalar` = cast SQL eksplisit (`::bigint`/`::float8`/`::text`);
  nol baris → `None`. `IntoPgValue` kini juga untuk `Vec<T: Serialize>` (JSONB)
  dan `Option<T>` — sehingga `set` bisa mengosongkan kolom dan field `Option<V>`
  punya operator perbandingan (`col = NULL` tetap tak pernah benar; pakai `is_null`).

### Fixed

- **`World::insert` atas komponen yang sudah dimiliki merusak archetype**
  (`src/world.rs`). Cabang "entity sudah punya komponen" menyusun himpunan
  komponen tujuan dengan `push` + `sort_unstable` **tanpa dedup**, sehingga
  menyisipkan ulang tipe `T` yang sudah dimiliki menghasilkan daftar ber-id
  kembar. Di build debug hal itu memicu panik `debug_assert` urutan-ketat di
  `Archetype::new`; di **build rilis** `debug_assert` hilang dan archetype rusak
  **terbentuk diam-diam** — dua kolom logis untuk satu komponen, sehingga
  `get`/query membaca kolom pertama dan mengembalikan **nilai basi**.
  `insert` kini bersifat **upsert**: bila `T` sudah dimiliki, nilainya ditimpa
  di tempat tanpa perpindahan archetype (baris tidak bergerak, urutan iterasi
  tetap deterministik — STD-0005). `insert_bundle` tidak terpengaruh (sudah
  menjaga dengan `panic!` sejati). Sebagai pertahanan berlapis, guard urutan di
  `Archetype::new` dinaikkan dari `debug_assert!` menjadi `assert!` agar
  himpunan komponen cacat gagal keras juga di rilis (jalur dingin: sekali per
  archetype baru).

## [0.6.1] — 2026-07-29

### Added

- **`Entity::from_raw(index, generation)`** — merekonstruksi handle dari nilai
  mentah (deserialisasi), mis. relasi persisten `arke-postgres`
  ([RFC-0031](docs/RFC/RFC-0031-persistent-entity-relations-join.md)). Additif;
  handle basi tetap ditolak saat dipakai (`World::get`, STD-0007).

## [0.6.0] — 2026-07-29

Rilis **bentuk-API menuju 1.0** ([RN-0004](docs/RN/RN-0004-jalan-menuju-1.0.md)).

### Changed (BREAKING)

- **Trait ekstensi ditutup (*sealed*)**: `Bundle`, `QueryData`, `QueryTerm`,
  `QueryFilter` kini hanya dapat diimplementasi oleh `arke`
  ([RFC-0026](docs/RFC/RFC-0026-seal-extension-traits.md)). Mengeluarkan signature
  internal dari kontrak publik sebelum 1.0. Praktik: tak ada pemakai eksternal.

### Deprecated

- **`World::query_pair` & `World::query_pair_ref`** (khusus arity-2) — pakai jalur
  `QueryData` generik `<(&A, &mut B)>::each(&mut world, |(a, b)| { .. })`
  ([RFC-0027](docs/RFC/RFC-0027-deprecate-query-pair.md)). Tetap berfungsi
  sepanjang 0.6.x; **dihapus di 1.0**. `query`/`query_mut` (arity-1) dipertahankan.

### Fixed

- **MSRV dikoreksi** `1.86` → **`1.88`** ([RFC-0028](docs/RFC/RFC-0028-changelog-msrv-semver-policy.md)):
  kode memakai *let-chain* (stabil di 1.88), jadi klaim 1.86 salah dan memutus
  build pengguna 1.86/1.87.

### Added

- **`CHANGELOG.md`** (berkas ini) + **kebijakan MSRV (STD-0009)** & **semver/
  deprecation (STD-0010)** + **job CI `msrv`** + **uji pin `SCHEMA_VERSION`**
  (menegakkan stabilitas format snapshot, STD-0001/0002).

## [0.5.2] — 2026-07-28

### Changed

- `World::get` ~1.8× lebih cepat (~20→~11 ns/op) via downcast kolom tak-tercek
  terkurung, miri-verified ([RFC-0025](docs/RFC/RFC-0025-unchecked-column-downcast-get.md)).
  arke kini kompetitif/menang di ketiga beban inti (iter2/spawn/get) vs hecs & bevy_ecs.

## [0.5.1] — 2026-07-28

### Changed

- Iterasi query berkolom ([RFC-0023](docs/RFC/RFC-0023-columnar-query-iteration.md))
  & resolusi komponen cepat ([RFC-0024](docs/RFC/RFC-0024-fast-component-resolution.md)):
  iter2 ~2.0→~0.95 ns/op, spawn ~76→~32 ns/op. Regresi-guard performa di CI (RN-0003).

## [0.5.0] — 2026-07

### Added

- **Bundle komponen** ([RFC-0022](docs/RFC/RFC-0022-component-bundles.md)):
  `spawn_bundle`/`insert_bundle` menyisipkan tuple komponen dalam satu pindah archetype.

## [0.4.x] dan sebelumnya

Fondasi inti (M-1…M-19): entity/komponen archetype, query tuple generik + filter
`With`/`Without`, scheduler deterministik, iterasi data-parallel, resources,
snapshot berversi + `#[derive(Serialize)]`, error berkonteks, query cache,
eksekutor graf-ketergantungan, command buffer, `Entity` sebagai term query. Adapter
[`arke-postgres`](arke-postgres/) diperkenalkan pada era 0.4.x. Detail per rilis:
[GitHub Releases](https://github.com/dalfiannur/arke/releases).

[Unreleased]: https://github.com/dalfiannur/arke/compare/v0.6.1...HEAD
[0.6.1]: https://github.com/dalfiannur/arke/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/dalfiannur/arke/compare/v0.5.2...v0.6.0
[0.5.2]: https://github.com/dalfiannur/arke/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/dalfiannur/arke/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/dalfiannur/arke/compare/v0.4.2...v0.5.0
