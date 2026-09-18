# Changelog

Semua perubahan penting pada `arke` didokumentasikan di sini. Format mengikuti
[Keep a Changelog](https://keepachangelog.com/id/1.1.0/); proyek menganut
[Semantic Versioning](https://semver.org) (lihat STD-0010). Riwayat lengkap tiap
rilis juga ada di [GitHub Releases](https://github.com/dalfiannur/arke/releases).

## [Unreleased]

### Added

- **`arke-postgres`: paginasi keyset `Query::after`/`before`/`load_page`.**
  `load_page` mengambil `limit + 1` baris → `Page { items, next, prev }` tanpa
  `COUNT(*)`; `Cursor` opaque aman-URL (JSON `arke::Value` + base64url, 0-dep;
  `Display`/`FromStr`). Arah seragam → row-value compare `(k…, pid) > (…)`
  (ramah index); arah campur → OR-expanded. Kursor divalidasi terhadap
  `order_by` (`CursorError::Mismatch`); `NULL`/JSONB sebagai kunci ditolak.
  `after`/`before` juga berlaku untuk `load`/`load_pids` (galat kursor →
  `sqlx::Error::Protocol`); `count()` mengabaikan kursor. Tipe baru: `Cursor`,
  `CursorError`, `Page`, `PageError`.

- **`arke-postgres`: batas operasional** — `PgStore::connect_with(url,
  &ConnectOptions { max_connections, min_connections, acquire_timeout,
  statement_timeout, lock_timeout, idle_in_transaction_session_timeout,
  application_name })`; batas sisi server dipasang lewat `SET` per koneksi
  (`after_connect`, kompatibel PgBouncer session mode). `failure_kind(&err)
  → FailureKind { PoolTimeout, StatementTimeout, LockTimeout, ConnectionLost,
  Other }` memetakan `PoolTimedOut`/SQLSTATE agar handler memilih 503/504
  tanpa mengorek `sqlx::Error`. `PgStore::pool_stats() → PoolStats { size,
  idle, max }`. `connect(url)` tak berubah (= `ConnectOptions::default()`).

- **`arke-postgres`: agregasi lanjutan** — `Query::count_distinct(field)`,
  `group_by` **multi-kunci** (tuple 2–4 `Field` → hasil `((K1, K2), …)`; trait
  `GroupKey`), dan `Grouped::having(..)` dengan builder typed di modul
  `aggregate` (`count::<T>()`, `count_distinct`, `sum`, `min`, `max`, `avg` +
  `gt/gte/lt/lte/eq/ne`, gabung `and/or/not`; tipe `Having`, `AggExpr`).
  `Grouped<'a, T, K>` menjadi `Grouped<'a, T, G: GroupKey<T>>`.

- **`arke-postgres`: filter lintas komponen `Query::with::<R>()` /
  `with_where(Filter<R>)` / `without::<R>()`** — padanan `With`/`Without` arke
  core di Postgres: semi-join `pid [NOT] IN (SELECT pid FROM cmp_r [WHERE …])`
  pada entity yang sama (bukan relasi FK), ikut `where_clause` sehingga berlaku
  untuk `load*`, `count`, `exists`, `count_estimate`, `load_page`, dan mutasi
  massal.

- **`arke-postgres`: full-text search.** `#[pg(fts)]`/`#[pg(fts = "<regconfig>")]`
  (derive 0.8 → `PgComponent::FTS`, `FtsDef`; hanya `String`/`Option<String>`)
  → `migrate` membuat indeks GIN ekspresi `to_tsvector('<cfg>', col)`
  (idempoten, config berubah → dibuat ulang). `Field<C, String>::search(q)`
  (`websearch_to_tsquery`: AND/OR/`-kata`/`"frasa"`) dan
  `Query::order_by_rank(field, q)` (`ts_rank` menurun) — kunci `ORDER BY`
  berekspresi, sehingga **keyset `load_page` berjalan di atas rank**. Default
  config `simple` (tanpa stemming); field tanpa atribut tetap bisa `search`.

- **`arke-postgres`: `Query::count_estimate()`** — estimasi jumlah baris dari
  planner (`EXPLAIN` → `rows=`) tanpa memindai tabel, `WHERE` sama dengan
  `count()`. Untuk "≈ N hasil"/total halaman kasar; akurasi bergantung
  statistik `ANALYZE`, tabel kosong bisa memberi ≥ 1. `count()` tetap eksak.

- **`arke-postgres`: hidrasi selektif `Query::only::<S>()`.** `S` = satu
  komponen atau tuple 1–8 (`only::<(Health, Position)>()`); `load`/`load_pids`
  hanya membaca tabel komponen dalam `S` (round-trip `2 + |S|`, bukan `2 + R`
  untuk R komponen terdaftar). Komponen di luar `S` tak disentuh di `World`.
  Entity yang dimuat parsial dicatat di store: `update_entity`/`save_incremental`
  melewati tabel yang tak dimuat **dan** tak diisi pemanggil, sehingga komponen
  yang tak dimuat tak terhapus; muat penuh ke World yang sama melepas status
  parsial. `save()` overwrite penuh tetap tak dijaga (seperti `load_where`).
  Trait `ComponentSet` diekspor.

### Changed

- **`arke-postgres`: `ORDER BY` query builder selalu diikat `pid`** (arah
  mengikuti kunci terakhir) → urutan total & deterministik juga untuk
  `limit`/`offset`. **`load_pids` kini mengembalikan urutan `ORDER BY` query**
  (sebelumnya `ORDER BY pid` dari `materialize`, bertentangan dengan doc-nya).

- **`arke-postgres`: `#[pg(check)]` ber-nama-stabil (content-addressed).**
  Constraint kini bernama `chk_<tabel>_<fnv64(ekspresi)>` (dulu
  `chk_<tabel>_<indeks posisi>`, sehingga ekspresi yang diubah di posisi yang
  sama tak pernah diterapkan). `migrate` merekonsiliasi: ekspresi baru/berubah
  dipasang, `chk_<tabel>_*` yang tak lagi dideklarasikan di-DROP; definisi
  sama → tanpa DDL. Constraint `chk_<tabel>_<n>` lama dari 0.16 di-drop dan
  dipasang ulang dengan nama baru pada `migrate` pertama. Baris yang melanggar
  CHECK baru membuat `migrate` gagal keras (keputusan migrasi data ada di
  operator).

### Fixed

- **`arke-postgres`: future `PgStore::save`/`save_incremental`/`update_entity`
  kini `Send` tanpa `World: Sync`.** `save*` sudah dua-fase (`stage*` sinkron +
  `commit*` async) dan `update_entity` membaca `world` di dalam transaksi;
  sebagai `async fn`, parameter `&World` hidup di state future sampai selesai —
  sehingga future *pemanggil* (handler axum, `tokio::spawn`) menjadi `!Send`
  walau `World: Send`. Kini ketiganya `fn` biasa yang membaca `world`
  secara sinkron lalu mengembalikan future fase async (`impl Future + Send +
  '_`); pemanggilan `.await` tak berubah. Regresi dijaga uji kompilasi
  `tests/send_future.rs`. Pola "World per-request" di README kini benar-benar
  bisa di-await langsung di handler multi-thread.

## [0.7.0] — 2026-09-15

Rilis gelombang breaking kedua menuju 1.0 ([RFC-0036](docs/RFC/RFC-0036-audit-0.7-soundness-stability-security.md), [ADR-0036](docs/ADR/ADR-0036-audit-0.7-soundness-stability-security.md), [Milestone 33](docs/MILESTONE_33.md)). Crate pendamping: `arke-derive` 0.4.0 (`#[serialize(name/default)]`; kode hasil derive butuh `arke` ≥ 0.7), `arke-postgres` 0.16.0, `arke-postgres-derive` 0.8.0, `arke-cache` 0.4.0, `arke-mongo` 0.1.0 (rilis pertama).

### Changed (BREAKING — audit 0.7.0: soundness, stabilitas, keamanan)

- **`Component` kini `'static + Send + Sync`** (dulu hanya `Send`). Eksekutor
  paralel (`Schedule::run_parallel`) menjalankan dua sistem yang sama-sama
  *membaca* `T` **bersamaan** di thread berbeda lewat `&T` — hanya sound bila
  `T: Sync`. Tipe `Send + !Sync` (mis. `Cell<u32>`) sebelumnya lolos sebagai
  komponen dan menjadi **data race** di jalur itu (dibuktikan: dua
  `System::each::<&Cell<u64>>` dinilai tak-konflik). Kini gagal kompilasi.
- **`QueryData`: jalur `&World` berbagi dibatasi query baca-saja.**
  `each_filtered_shared`/`each_cached_shared` kini mensyaratkan
  `Self: ReadOnlyQuery` (trait penanda baru, tertutup: `&T`, `Entity`, dan
  tuple yang seluruh termnya baca-saja). Sebelumnya kode **aman** dapat
  membentuk `&mut T` dari `&World` yang beralias dengan `&T` hasil
  `World::get` yang masih hidup (UB tanpa `unsafe` — melanggar STD-0004).
  `each_cached` kini menerima `&mut World`; implementasi inti menjadi
  `unsafe fn each_cached_unchecked` (`#[doc(hidden)]`) dengan kontrak
  eksplisit, hanya dipanggil `System`/`Schedule` yang menjamin disjoint lewat
  graf-konflik. Migrasi: query dengan term `&mut T` pakai `each`/`each_filtered`
  /`each_cached` (`&mut World`); query baca-saja tak berubah.
- **`arke-postgres` 0.16 / `arke-postgres-derive` 0.8 / `arke-cache` 0.4.**
  `PgStore::fetch` kini `&mut self` (mengisi jembatan pid↔entity, melayani
  cache, me-refresh pid yang sudah termuat). `PgValue::Ref` membawa `Entity`
  utuh (`generation << 32 | index`, `pack_entity`/`unpack_entity`) — bukan
  indeks saja — sehingga relasi ke slot terdaur-ulang basi, bukan menunjuk
  entity baru; dan `Ref` me-resolve di World yang slotnya pernah dipakai
  (dulu direkonstruksi dengan `generation 0`). Satu `PgStore` melayani
  **satu `World`**: World lain yang datang (dideteksi via `World::id()`)
  me-reset jembatan & rekam `save_incremental` — dulu jembatan ber-kunci
  indeks diam-diam mencampur handle lintas-World; pakai `fork()` per World
  (README sudah begitu). `Query::load*` kini **aditif** (tak me-reset jembatan):
  beberapa `load` ke satu World saling melengkapi, pid yang sudah termuat
  di-refresh di tempat.

### Added

- **Format snapshot tahan-evolusi.** `Serialize::name()` = kunci komponen di
  snapshot (default tetap `type_name`, yang Rust nyatakan **tak stabil** antar
  versi/rename modul); `#[serialize(name = "game.hp")]` menetapkannya
  eksplisit. `World::register_serializable_alias::<T>("nama::lama")` membaca
  snapshot yang ditulis di bawah nama lama. `#[serialize(default)]` pada field:
  kunci yang hilang (snapshot sebelum field ditambah) diisi `Default`, bukan
  menggagalkan seluruh komponen. `World::try_load_snapshot` memvalidasi dulu
  (versi skema, kunci terdaftar, decode) dan **all-or-nothing** — `EcsError`
  varian baru `SchemaVersionUnsupported`/`UnknownComponent`/
  `ComponentDecodeFailed` menyebut komponen & entity-nya; `load_snapshot` tetap
  lunak. `EcsError` kini `#[non_exhaustive]`.
- **`World::id()` / `WorldId`** — identitas unik per-proses sebuah World (bukan
  bagian keadaan/snapshot). Menjawab pertanyaan terbuka RFC-0035: adapter dapat
  mendeteksi World yang berganti alih-alih mencampur handle `Entity`
  lintas-World.
- **`Snapshot::len`/`is_empty`/`max_index`** — `max_index` untuk membatasi
  alokasi tabel slot sebelum `load_snapshot` dari sumber tak tepercaya (indeks
  `u32::MAX` = ~4 miliar slot).
- **`arke::serialize::MAX_JSON_DEPTH`** (128) — batas kedalaman parser JSON.
- **`arke-postgres`: `#[pg(table = "…")]`** — nama tabel kustom (dipakai
  verbatim, di-quote); default tetap `cmp_<nama struct huruf kecil>`. Field
  raw-ident (`r#type`) → kolom `type`.
- **`arke-postgres`: identifier SQL di-quote bila perlu** (`quote_ident`,
  publik): kata kunci reserved Postgres (`order`, `user`, `end`, `select`, …),
  huruf besar/camelCase, atau karakter lain dibungkus `"…"` di seluruh SQL yang
  dibangun (DDL `migrate`, insert/select, query builder, agregat, mutasi massal,
  path/rekursif). Identifier polos huruf-kecil tak berubah (`cmp_x` ≡ `"cmp_x"`).
  Sebelumnya field bernama `order` gagal SQL dan field camelCase tak pernah
  cocok saat baca.
- **`arke-postgres`: `PgStore::register` idempoten** (tabel yang sudah terdaftar
  dilewati); `migrate` merekonsiliasi **FK `pid → arke_entities ON DELETE
  CASCADE`** yang hilang (mis. setelah `DROP TABLE arke_entities CASCADE`) —
  tanpanya `save` meninggalkan baris komponen yatim; `commit_update`/`remove`
  kini meng-invalidate cache.

### Performance

- **`arke-postgres`: tulis batch (`UNNEST`).** `commit`/`commit_incremental`
  kini mengalokasikan pid dalam satu `INSERT … generate_series … RETURNING`,
  menghapus/menaikkan versi dengan `pid = ANY($1)`, dan menyisipkan baris
  komponen per tabel lewat `INSERT … SELECT FROM UNNEST($1::int8[], …)` (≤ 2.000
  baris per pernyataan) — round-trip `O(jumlah tabel)` alih-alih `O(entity ×
  tabel)`. 10k entity × 2 komponen (lokal): `save` **3,0 s → 0,26 s**,
  `save_incremental` **2,0 s → 0,29 s**.
- **Kolam thread persisten** (`src/pool.rs`) untuk `Schedule::run_parallel` dan
  `World::par_for_each`: dulu `thread::scope` men-spawn thread OS tiap panggilan
  (thread-per-sistem; thread-per-chunk-per-archetype), ~15–20 µs per thread.
  Kini `available_parallelism` pekerja parkir di `Condvar`, dimiliki
  `Schedule`/`World`, di-join saat drop. Eksekutor graf menjadi pekerja
  terbatas + antrean sistem siap. Bench baru W7 (16 sistem kecil): **13,4 µs →
  1,3 µs per sistem**; W8 (`par_for_each`, 16 archetype): **24,5 ns → 0,2 ns
  per elemen** (dulu 20× lebih lambat dari serial). Satu `unsafe` baru
  (transmute masa-hidup pekerjaan, terkurung di `pool`, sound karena `scope`
  memblokir sampai semua pekerjaan selesai — juga saat unwind), diverifikasi
  miri.
- **Indeks archetype** (`HashMap` ber-hasher Fx atas himpunan komponen) di
  `World` menggantikan scan linear semua archetype di tiap `insert`/`remove`
  struktural; buffer id dipakai ulang (tanpa alokasi `Vec` per operasi). Bench
  baru W6 (64 archetype): **84 → 61 ns/op**; W4: 35 → 30 ns/op.

### Security

- **`cargo audit` di CI** (`.github/workflows/audit.yml`, RustSec
  `audit-check`): tiap perubahan manifest/lockfile + terjadwal mingguan.
  `Cargo.lock` tidak di-commit, jadi job membangkitkannya dulu — yang diaudit
  resolusi terbaru yang kompatibel. Pemasangan pertama (lokal) langsung
  menemukan **RUSTSEC-2026-0285** (rustls 0.23.43, medium — pesan handshake
  TLS 1.3 diterima lintas batas level enkripsi) di pohon dependensi adapter
  (`mongodb`); resolusi terbaru sudah rustls 0.23.45 (+ `chacha20` 0.10.2
  menggantikan 0.10.1 yang di-yank).

### Fixed

- **Kasus tepi serialisasi (audit).** `u64`/`usize` di atas `i64::MAX`
  dulu di-`as i64` (wrap negatif → gagal dibaca); kini `Value::Text` desimal
  (JSON string valid, round-trip setia). JSON: semua karakter kontrol < 0x20
  di-escape (`\u00XX`, RFC 8259), surrogate pair `\uD83D\uDE00` diterima
  (surrogate tunggal ditolak), `NaN`/`Infinity` ditulis `null` alih-alih teks
  yang membuat dokumen tak valid.
- **Slot entity dengan `generation == u32::MAX` dipensiunkan** saat `despawn`
  (tak masuk free-list) — `+= 1` dulu wrap diam di rilis (ABA setelah 2³² daur
  ulang). `spawn` melewati 2³² slot kini panic eksplisit, bukan truncate.
- **`System::each_res` kehilangan resource bila closure panic** — resource
  dilepas sementara selama iterasi; kini dikembalikan lewat `catch_unwind`
  lalu panic dipropagasi ulang.
- **`arke-postgres`: `decode_row` mengalokasikan kapasitas dari byte cache
  tanpa batas** (cache korup → OOM/`capacity overflow`); kini dibatasi sisa
  byte, dan panjang string dijumlah dengan `checked_add`. `renumber` tak lagi
  mengganti `?` di dalam identifier/literal ter-quote (nama tabel kustom
  `"a?b"` aman).
- **`arke-postgres`: `Option<i32>`/`Option<f32>` (kolom INTEGER/REAL nullable)
  gagal ditulis setelah baris pertama** — `22P03 incorrect binary data format`.
  `bind_value` mem-bind `NULL` sebagai `Option<i32>` tetapi nilai sebagai `i64`;
  sqlx meng-cache prepared statement dengan tipe parameter eksekusi pertama.
  Kini tipe Rust **tetap per tipe kolom** (`Integer` → `Option<i32>`, `Real` →
  `Option<f32>`, …) untuk `NULL` maupun nilai.
- **Panic di satu sistem `run_parallel` menggantung, bukan dipropagasi**
  (`src/schedule.rs`). Thread yang unwind tak pernah melepas penghitung
  suksesornya; suksesor menunggu `Condvar` selamanya dan `thread::scope` tak
  pernah selesai. Kini guard `Drop` melepas suksesor saat unwind (mutex
  terracuni ditoleransi) dan `scope` me-re-panic ke pemanggil.
- **`World::spawn_at` di slot yang ada di free-list menerbitkan handle
  duplikat** (`src/world.rs`): `spawn` berikutnya mem-pop slot yang sama dan
  mengembalikan `Entity` **identik** dengan hasil `spawn_at`, sementara baris
  komponen entity yang direstorasi menjadi yatim di archetype (masih ter-query,
  `get` → `None`). Slot kini dikeluarkan dari free-list; slot hidup yang
  ditimpa dibersihkan baris komponennya dulu. Memengaruhi `load_snapshot` ke
  World yang pernah `despawn`.
- **Parser JSON rekursif tanpa batas kedalaman** (`src/serialize.rs`): input
  `[[[[…` ribuan tingkat meledakkan stack (abort proses) — DoS dari snapshot
  tak tepercaya. Kini `None` di atas `MAX_JSON_DEPTH`. `Snapshot::from_json`
  juga menolak `index` entity duplikat (dua entity di satu slot tak mungkin
  direkonstruksi tanpa korupsi).
- **`arke-postgres`: cache read-through + evolusi skema = kehilangan data.**
  Baris cache dari deploy lama (kolom lebih sedikit) membuat `from_params`
  gagal diam-diam → komponen tak termuat → `save_incremental` berikutnya
  **menghapus barisnya**. Namespace cache kini `table@fingerprint(kolom)`
  (FNV-1a atas nama/tipe/nullable), sehingga skema yang berubah otomatis
  memakai namespace baru.
- **`arke-postgres`: `stage_incremental` non-deterministik & jembatan
  dimutasi sebelum commit.** Upsert/delete diiterasi dari `HashMap` (urutan
  acak → alokasi pid & urutan kunci baris acak, melanggar STD-0005); `pid_of`
  di-clear/diisi **sebelum** `tx.commit()`, sehingga tx yang gagal
  meninggalkan pid hantu. Kini terurut (indeks, generation), jembatan dimutasi
  pada salinan lokal dan dipromosikan setelah commit; pid entity baru
  dialokasikan **dua-pass** sehingga relasi ke entity baru se-batch me-resolve
  (dulu: NULL menggantung).
- **`arke-postgres`: slot World terdaur-ulang mewarisi `pid` lama.** Jembatan
  ber-kunci indeks: `despawn` + `spawn` di indeks sama membuat entity baru
  menulis di atas baris DB entity lama (dan `Ref` di tabel lain kini menunjuk
  entity yang salah). Jembatan & rekam kini ber-kunci `Entity` utuh.


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
  Satu `MongoStore` melayani satu `World`: store menautkan diri ke `World`
  pertama (`World::id()`) dan menolak `World` lain dengan
  `MongoError::WorldMismatch` alih-alih mencampur handle `Entity` (tiga mode
  kerusakan data yang dulu lolos didokumentasikan di rustdoc `MongoStore`);
  `MongoStore::fork()` untuk `World` lain. `drop_database` sengaja tidak ada
  di API publik. Query builder, relasi, cache, `save_incremental`, dan
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

[Unreleased]: https://github.com/dalfiannur/arke/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/dalfiannur/arke/compare/v0.6.1...v0.7.0
[0.6.1]: https://github.com/dalfiannur/arke/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/dalfiannur/arke/compare/v0.5.2...v0.6.0
[0.5.2]: https://github.com/dalfiannur/arke/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/dalfiannur/arke/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/dalfiannur/arke/compare/v0.4.2...v0.5.0
