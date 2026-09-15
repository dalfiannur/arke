# RFC-0036: Audit 0.7.0 — soundness, stabilitas, keamanan, performa

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-09-15
- **Milestone:** M-33 (Audit menyeluruh → gelombang breaking kedua sebelum 1.0)
- **ADR terkait:** [ADR-0036](../ADR/ADR-0036-audit-0.7-soundness-stability-security.md)
- **RFC terkait:** [RFC-0016](./RFC-0016-parallel-executor.md) (eksekutor paralel), [RFC-0026](./RFC-0026-seal-extension-traits.md) (seal), [RFC-0028](./RFC-0028-changelog-msrv-semver-policy.md) (semver/snapshot), [RFC-0029](./RFC-0029-archetype-resolution-index-edges.md) (resolusi archetype — ditolak), [RFC-0034](./RFC-0034-decoupled-persistent-id.md) (`pid`), [RFC-0035](./RFC-0035-arke-mongo-adapter.md) (`arke-mongo`, pertanyaan terbuka `WorldId`), [RN-0004](../RN/RN-0004-jalan-menuju-1.0.md)

## Ringkasan

Audit menyeluruh seluruh workspace (core `arke`, `arke-derive`, `arke-postgres`,
`arke-postgres-derive`, `arke-cache`, `arke-mongo`) dengan empat target: **future
proof**, **stabilitas**, **performa**, **keamanan**. Temuan yang diverifikasi
dengan bukti-konsep yang berjalan menunjukkan tiga lubang soundness/keamanan di
core dan beberapa jalur kehilangan-data di adapter. RFC ini merekam temuan,
keputusan perbaikannya, dan mengapa sebagian di antaranya **breaking** —
membentuk gelombang breaking kedua (`arke` 0.7.0, `arke-postgres` 0.16.0,
`arke-postgres-derive` 0.8.0, `arke-cache` 0.4.0) di jalur RN-0004 sebelum soak
menuju 1.0.

## Motivasi

RN-0004 menilai kualitas "siap 1.0" dan menyisakan komitmen bentuk-API. Audit ini
menguji klaim itu dari sisi yang belum pernah ditinjau bersama: kontrak
`unsafe` terhadap **kode aman pengguna**, perilaku saat **panic**, input **tak
tepercaya** (snapshot JSON), dan kebenaran adapter pada **evolusi skema** dan
**daur-ulang slot**. Setiap temuan di bawah dibuktikan dengan tes yang gagal
sebelum perbaikan.

### Temuan kritis (soundness / keamanan)

1. **`&mut T` dari `&World` lewat API aman.** `QueryData::each_cached` /
   `each_filtered_shared` menerima `&World` dan untuk term `&mut T` membentuk
   `&mut T` via `data_mut_shared`. Kode aman dapat memegang `&T` dari
   `World::get` lalu memutasinya lewat query — UB tanpa `unsafe` (melanggar
   STD-0004). Cek-alias hanya intra-query; query bersarang pun lolos.
2. **`Component: Send` tapi bukan `Sync`, sementara dua sistem pembaca berjalan
   bersamaan.** `Access::conflicts` menilai baca–baca tak konflik → eksekutor
   graf menjalankan keduanya di thread berbeda lewat `&T`. Untuk `T: Send +
   !Sync` (mis. `Cell<u64>`) ini data race; `unsafe impl Sync for SyncWorld`
   berasumsi `T: Sync` tanpa pernah menuntutnya.
3. **Parser JSON rekursif tanpa batas kedalaman** — `"[".repeat(200_000)`
   meledakkan stack (abort). `Snapshot::from_json` adalah gerbang data eksternal.

### Temuan penting (stabilitas / kehilangan data)

4. Panic di satu sistem `run_parallel` → **deadlock**, bukan propagasi (suksesor
   menunggu `Condvar` yang tak pernah dinotifikasi).
5. `spawn_at` ke slot yang ada di free-list → `spawn` berikutnya menerbitkan
   **handle duplikat**; baris komponen entity yang direstorasi jadi yatim.
6. `arke-postgres`: cache read-through + evolusi skema → baris cache lama lebih
   pendek → `from_params` gagal diam-diam → komponen hilang → `save_incremental`
   **menghapus barisnya**.
7. `arke-postgres`: `stage_incremental` beriterasi `HashMap` (urutan acak, STD-0005)
   dan jembatan `pid_of` dimutasi **sebelum** `commit` (tx gagal → pid hantu).
8. `arke-postgres`: jembatan ber-kunci indeks `u32` → slot terdaur-ulang mewarisi
   `pid` lama; `Ref` direkonstruksi dengan `generation 0`.
9. `arke-postgres`: `Query::load*` me-reset jembatan tiap panggilan → dua muat ke
   satu World lalu `save_incremental` menggandakan entity; `fetch` tak mengisi
   jembatan (`update_entity` sesudahnya selalu `Conflict`).

### Temuan future-proof & performa

- Kunci snapshot = `std::any::type_name` (Rust: tak stabil); tak ada
  `#[serialize(default)]`; `load_snapshot` melewati kegagalan diam-diam.
- Thread OS di-spawn tiap `run_parallel`/`par_for_each` (thread-per-sistem,
  thread-per-chunk-per-archetype); `find_or_create_archetype` scan linear +
  alokasi `Vec` per operasi struktural.
- `PgStore::commit*` satu round-trip per entity dan per baris komponen.
- Tabel `cmp_<lowercase>` tanpa `#[pg(table)]` (tabrakan nama), identifier tak
  di-quote (field `order`/`user` gagal; camelCase tak pernah cocok saat baca).
- Tak ada audit advisori dependensi di CI. `u64 > i64::MAX` wrap; kontrol char
  JSON tak di-escape; surrogate pair ditolak; NaN → JSON tak valid; `each_res`
  kehilangan resource saat panic; `generation` wrap diam; `decode_row`
  kapasitas tak dibatasi; `renumber` mengganti `?` di dalam kutip.
- `arke-mongo`: pertanyaan terbuka RFC-0035 (`WorldId`) — tiga mode kerusakan
  data lintas-World tanpa penjaga.

## Usulan rinci

### 1. Core `arke` 0.7.0 (breaking)

- **`Component: 'static + Send + Sync`.** Menutup temuan 2 secara struktural:
  setiap jalur paralel masa depan otomatis sound untuk `&T`. Doctest
  `compile_fail` untuk `Cell`.
- **Jalur `&World` berbagi dibatasi query baca-saja.** Trait penanda tertutup
  baru `ReadOnlyQuery` (diimpl untuk `&T`, `Entity`, tuple yang seluruh
  termnya baca-saja via `sealed::ReadOnlyTerm`). Implementasi inti menjadi
  `#[doc(hidden)] unsafe fn each_cached_unchecked(&World, …)` dengan kontrak
  Safety eksplisit; `each_cached` menerima `&mut World`; `each_cached_shared`
  / `each_filtered_shared` mensyaratkan `Self: ReadOnlyQuery`. `System::each*`
  memanggil jalur unchecked dengan argumen graf-konflik (RFC-0016/0018). Doctest
  `compile_fail` untuk PoC aliasing.
- **Panic-safe executor.** Guard `Drop` melepas suksesor (dan menandai selesai)
  saat unwind; mutex terracuni ditoleransi; `scope`/kolam me-re-panic.
- **`spawn_at`** mengeluarkan slot dari free-list dan membersihkan baris
  komponen slot hidup yang ditimpa.
- **`MAX_JSON_DEPTH = 128`**; `Snapshot::from_json` menolak `index` duplikat;
  `Snapshot::len/is_empty/max_index` (pemanggil membatasi alokasi slot untuk
  input tak tepercaya).
- **`World::id()` / `WorldId`** — identitas unik per-proses (counter atomik),
  bukan bagian keadaan/snapshot. Menjawab pertanyaan terbuka RFC-0035.
- **Format snapshot tahan-evolusi:** `Serialize::name()` (default `type_name`,
  didokumentasikan tak stabil) + `#[serialize(name = "…")]`;
  `World::register_serializable_alias::<T>(nama_lama)`; `#[serialize(default)]`;
  `World::try_load_snapshot` (validasi versi/kunci/decode dulu, all-or-nothing);
  `EcsError` `#[non_exhaustive]` + varian `SchemaVersionUnsupported`,
  `UnknownComponent`, `ComponentDecodeFailed`.
- **Kolam thread persisten** (`src/pool.rs`): `available_parallelism` pekerja
  parkir di `Condvar`, dimiliki `Schedule`/`World` (malas), di-join saat drop.
  `Pool::scope` memblokir sampai semua pekerjaan selesai — juga saat unwind —
  sehingga satu `transmute` masa-hidup terkurung sound; panic dipropagasi
  ulang setelah pekerjaan lain selesai. Eksekutor graf menjadi pekerja terbatas
  + antrean sistem siap; `par_for_each` membagi chunk seimbang ke pekerja.
- **Indeks archetype** (`HashMap<Box<[ComponentId]>, usize>` ber-hasher Fx) +
  buffer id yang dipakai ulang (`scratch_ids`) untuk `insert`/`remove`/
  `insert_bundle`. Lihat "Kaitan dengan RFC-0029" di bawah.
- Kasus tepi: `u64`/`usize` > `i64::MAX` → `Value::Text` desimal; escape
  kontrol < 0x20; surrogate pair; NaN/Inf → `null`; `generation == u32::MAX` →
  slot dipensiunkan; `spawn` > 2³² slot panic eksplisit; `each_res`
  `catch_unwind` + reinsert.

### 2. `arke-postgres` 0.16.0 / `arke-postgres-derive` 0.8.0 / `arke-cache` 0.4.0

- Jembatan `pid_of`/`entity_of`/`last` ber-kunci **`Entity`** utuh + **`WorldId`**
  (World lain → reset otomatis; satu store ↔ satu World, `fork()` per World).
- `PgValue::Ref` mengemas `generation << 32 | index` (`pack_entity`/`unpack_entity`).
- Namespace cache `table@fingerprint(kolom)` (FNV-1a atas nama/tipe/nullable);
  `PgStore::cache_namespace(table)`.
- `commit*` deterministik (terurut), jembatan dipromosikan pasca-commit, alokasi
  pid dua-pass, dan **batch `UNNEST`** (≤ 2.000 baris/pernyataan).
- `Query::load*` **aditif** (refresh di tempat, lepas komponen yang barisnya
  hilang); `fetch` `&mut self` lewat `materialize`; `register` idempoten;
  `migrate` merekonsiliasi FK cascade yang hilang (+ purge baris yatim).
- `#[pg(table = "…")]`, kolom raw-ident, dan `quote_ident` (publik) di seluruh
  SQL yang dibangun — identifier polos huruf-kecil tak berubah.
- Bind tipe Rust **tetap per tipe kolom** (`Integer` → `Option<i32>`, …) —
  memperbaiki `22P03` pada `Option<i32>`/`Option<f32>` (cache prepared
  statement sqlx).
- `decode_row` kapasitas dibatasi + `checked_add`; `renumber` mengabaikan `?`
  di dalam kutip.

### 3. `arke-mongo` 0.1.0 (belum dirilis)

- Penjaga `WorldId`: store menautkan diri ke `World` pertama; `World` lain →
  `MongoError::WorldMismatch` (menolak, bukan reset — `save` Mongo bukan
  overwrite penuh sehingga reset akan meninggalkan dokumen yatim).
  `MongoStore::fork()`; `bound_world()`.

### 4. CI

- Workflow `Audit` (`rustsec/audit-check`) pada perubahan manifest/lockfile +
  mingguan; `Cargo.lock` dibangkitkan dulu (tidak di-commit). Pemasangan
  pertama menemukan RUSTSEC-2026-0285 (rustls 0.23.43) di pohon `mongodb`.
- Bench regresi-guard baru W6/W7/W8.

### Kaitan dengan RFC-0029 (ditolak)

RFC-0029 mengusulkan index lookup **+ edge transisi**, dan ditolak karena
pengukuran spawn menunjukkan resolusi archetype bukan bottleneck. Audit ini
mengukur jalur **churn** (insert+remove) pada 64 archetype (W6, baru): scan
linear + alokasi `Vec` per operasi 84 ns/op → 61 ns/op setelah index +
`scratch_ids`. Kontribusi index sendiri kecil (≈ 7 ns); sisanya dari
menghilangkan alokasi. RFC ini mengadopsi **index lookup saja** sebagai bagian
dari pembersihan jalur churn dan **tetap menolak edge transisi** (kompleksitas
tanpa bukti kebutuhan). ADR-0036 men-*supersede* ADR-0029 hanya pada butir
index.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| Bound `Sync` hanya di `QueryTerm for &T` (bukan `Component`) | Kurang breaking (`!Sync` masih bisa `insert/get`) | Lubang tetap terbuka untuk jalur paralel masa depan; dua aturan untuk satu konsep | `Component: Send + Sync` sound by construction (bevy memilih sama) |
| `each_cached(&World)` tetap aman + cek-alias runtime global | Tak ubah signature | Butuh pelacakan pinjaman runtime (biaya jalur panas), tak menangkap `&T` dari `get` | Batasi tipe (`ReadOnlyQuery`), pindahkan risiko ke `unsafe fn` berkontrak |
| Reset jembatan Mongo saat World berganti (seperti Pg) | Simetris dengan `PgStore` | `save` Mongo bukan overwrite penuh → dokumen yatim | Menolak (`WorldMismatch`) + `fork()` |
| `Value::UInt(u64)` untuk `u64 > i64::MAX` | Representasi "benar" | Varian baru memutus `match` pengguna atas `Value` | `Value::Text` desimal: valid JSON, aman BSON/JSONB, non-breaking |
| Kolam thread global (`OnceLock`) | Satu kolam per proses | Thread tak pernah di-join → miri menolak; state global | Kolam per `Schedule`/`World`, di-join saat drop |
| Pekerja terbatas via `thread::scope` (tanpa kolam) | 0 `unsafe` baru | Masih spawn `inti` thread per panggilan (W7 hanya turun 13 → 7,8 µs/sistem) | Kolam persisten: 1,1–1,3 µs/sistem |
| Batch INSERT via multi-row `VALUES` | Tanpa `UNNEST` | Jumlah placeholder = baris × kolom (batas 65535) | `UNNEST` array per kolom: placeholder = kolom |
| Quote **semua** identifier | Sederhana | Semua SQL/tes berubah; kasus umum tak butuh | Quote bila perlu (kata kunci reserved / non-`[a-z0-9_]`) |

## Dampak

- **Kompatibilitas / migrasi:** BREAKING (pra-1.0, RN-0004). `Component` butuh
  `Sync` (komponen `Cell`/`RefCell` tak lagi diterima — pakai atomics atau
  pindah ke resource); query ber-`&mut T` hanya lewat `&mut World`
  (`each`/`each_filtered`/`each_cached`); `EcsError` `#[non_exhaustive]`
  (`match` butuh `_`); `PgStore::fetch` `&mut self`; satu `PgStore`/`MongoStore`
  per World (`fork()`); `PgValue::Ref` berformat baru (nilai di DB = `pid`,
  tak berubah — hanya representasi in-memory); `arke-cache` ikut 0.4.0 karena
  namespace cache berubah (entri lama kedaluwarsa via TTL).
- **Keamanan / izin / provenance:** menutup UB dari kode aman; DoS parser JSON;
  audit advisori dependensi di CI; `quote_ident` bukan pengganti bind — nilai
  tetap terparameterisasi, `load_where`/`query_pids` tetap SQL tepercaya.
- **Konsekuensi pada invarian:** memperkuat *ergonomis = cepat* (jalur pengguna
  bebas `unsafe` — sekarang benar-benar, bukan hanya di dokumentasi) dan
  *determinisme by construction* (urutan tulis inkremental & alokasi pid
  terurut; hasil `run_parallel` tak berubah). `unsafe` kini terkurung di
  `storage`, `query`, `world`, `schedule`, `pool` — semua di bawah miri.

## Pertanyaan terbuka

- Kolam thread per `Schedule` **dan** per `World` bisa oversubscribe bila
  keduanya dipakai bergantian di satu frame (2× inti thread parkir; hanya satu
  aktif). Kolam bersama antar-owner (bukan global) ditunda sampai ada kasus.
- `CHECK` constraint masih di-key indeks posisi (`chk_<tabel>_<i>`): ekspresi
  yang berubah di indeks sama tak diterapkan `migrate`.
- Batas `max_index` snapshot diserahkan ke pemanggil; `try_load_snapshot`
  tidak memaksakan batas alokasi.

## Keputusan

Diterima (2026-09-15) dan diimplementasikan di branch `feat/arke-mongo`
(commit `bd638a4` … `3ccbcf9`), CI (termasuk miri, Postgres, Mongo, Audit)
hijau. Keputusan direkam di
[ADR-0036](../ADR/ADR-0036-audit-0.7-soundness-stability-security.md).
Rilis 0.7.0 / 0.16.0 / 0.8.0 / 0.4.0 menyusul; setelah soak tanpa breaking baru,
buka Milestone 1.0 (RN-0004).
