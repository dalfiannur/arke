# ADR-0036: Gelombang breaking kedua (0.7.0) — soundness by construction, satu store satu World, kolam thread persisten

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-09-15
- **RFC terkait:** [RFC-0036](../RFC/RFC-0036-audit-0.7-soundness-stability-security.md)
- **Men-supersede sebagian:** [ADR-0029](ADR-0029-archetype-resolution-index-edges.md) (butir *index lookup* diadopsi; *edge transisi* tetap ditolak)

## Konteks

Setelah 0.6.x (RN-0004: seal, deprecate `query_pair`, kebijakan semver), audit
menyeluruh (RFC-0036) membuktikan — dengan tes yang gagal sebelum perbaikan —
bahwa klaim "jalur pengguna bebas `unsafe`" belum benar: kode aman dapat
membentuk `&mut T` dari `&World`, dan komponen `Send + !Sync` dibaca dua sistem
paralel bersamaan. Ditambah deadlock saat panic, DoS parser JSON, dan beberapa
jalur kehilangan-data di adapter yang lahir dari jembatan ber-kunci indeks dan
cache tanpa versi skema. Semua ini harus ditutup **sebelum** 1.0 membekukan
API; sebagian tak bisa ditutup tanpa breaking.

## Keputusan

1. **`Component: 'static + Send + Sync`.** Soundness eksekutor paralel
   dijamin oleh tipe, bukan oleh disiplin pemanggil.
2. **Jalur `&World` berbagi hanya untuk query baca-saja** (`ReadOnlyQuery`,
   sealed). Implementasi inti adalah `unsafe fn each_cached_unchecked` ber-
   kontrak, `#[doc(hidden)]`; hanya `System`/`Schedule` yang memanggilnya
   dengan argumen graf-konflik. Query ber-`&mut T` hanya lewat `&mut World`.
3. **Panic tidak boleh menggantung**: pelepasan suksesor via guard `Drop`;
   kolam thread me-re-panic setelah pekerjaan lain selesai; `each_res`
   mengembalikan resource sebelum propagasi.
4. **Input eksternal dibatasi**: `MAX_JSON_DEPTH`, tolak `index` duplikat,
   `try_load_snapshot` all-or-nothing dengan `EcsError` berkonteks
   (`#[non_exhaustive]`). Kunci snapshot dapat ditetapkan (`Serialize::name`,
   alias untuk migrasi); field baru boleh `#[serialize(default)]`.
5. **`WorldId` di core** (`World::id()`), unik per-proses, bukan bagian
   keadaan. Adapter memakainya: `PgStore` me-reset jembatan saat World berganti
   (`commit` = overwrite penuh, aman); `MongoStore` **menolak**
   (`WorldMismatch`) karena `save`-nya bukan overwrite penuh. Keduanya
   menyediakan `fork()`; satu store ↔ satu World.
6. **Jembatan adapter ber-kunci `Entity` utuh**, `Ref` mengemas generation,
   jembatan dipromosikan hanya setelah commit sukses, urutan tulis terurut
   (STD-0005), pid dialokasikan dua-pass, batch `UNNEST`.
7. **Cache ber-namespace fingerprint skema** — baris cache dari skema lama tak
   pernah disajikan.
8. **Kolam thread persisten** milik `Schedule`/`World`, di-join saat drop.
   Satu `unsafe` baru (transmute masa-hidup) terkurung di `pool`, sound karena
   `scope` memblokir sampai selesai (juga saat unwind), di bawah miri.
9. **Indeks archetype (lookup saja) + buffer id dipakai ulang** — mengadopsi
   butir 1 ADR-0029 dengan bukti W6, tetap menolak edge transisi.
10. **Identifier SQL di-quote bila perlu** (`quote_ident`), `#[pg(table)]`;
    bind tipe Rust tetap per tipe kolom.
11. **Audit advisori dependensi di CI** (mingguan + tiap perubahan manifest).
12. Rilis: `arke` 0.7.0, `arke-postgres` 0.16.0, `arke-postgres-derive` 0.8.0,
    `arke-cache` 0.4.0; `arke-mongo` 0.1.0 (rilis pertama sudah memuat penjaga).

## Konsekuensi

**Positif:**

- Klaim STD-0004 menjadi benar secara struktural: tak ada jalur aman yang
  membentuk `&mut` beralias; `unsafe` terkurung di lima modul, semuanya miri.
- Panic, input tak tepercaya, dan evolusi skema tak lagi merusak data diam-diam.
- Performa terukur: `run_parallel` 13,4 → 1,1–1,3 µs/sistem; `par_for_each`
  terfragmentasi 24,5 → 0,2 ns/elemen; churn 64 archetype 84 → 61 ns/op;
  `PgStore::save` 10k×2 komponen 3,0 → 0,26 s.
- Pertanyaan terbuka RFC-0035 (`WorldId`) terjawab.

**Negatif / biaya:**

- **BREAKING** untuk: komponen `!Sync`; pemanggil `each_cached(&World)` /
  `each_filtered_shared` dengan `&mut T`; `match` ekshaustif atas `EcsError`;
  `PgStore::fetch(&self)`; satu store untuk banyak World (dulu "berhasil"
  kebetulan lewat jembatan ber-indeks).
- Dua kolam (per `Schedule` dan per `World`) bisa oversubscribe bila keduanya
  dipakai bergantian; thread parkir, biaya memori kecil.
- `Value` tidak mendapat varian `UInt`; `u64 > i64::MAX` disimpan sebagai
  string desimal — benar dan portabel, tapi bukan angka JSON.

**Netral / catatan:**

- Kini **wajib**: tiap jalur `unsafe` baru punya kontrak Safety tertulis dan
  tes/doctest `compile_fail` yang membuktikan kode aman tak bisa menyalahgunakannya.
- Kini **terlarang**: API aman yang menerima `&World` dan menghasilkan `&mut`
  ke isinya; iterasi `HashMap` yang menentukan urutan tulis ke DB.
- `Cargo.lock` tetap tidak di-commit; job Audit membangkitkannya.

## Alternatif yang ditolak

- **Bound `Sync` hanya pada term `&T`** — menyisakan lubang untuk jalur paralel
  masa depan.
- **Cek-alias runtime global** — biaya jalur panas, tak menangkap `&T` dari `get`.
- **Kolam global `OnceLock`** — thread tak ter-join, miri menolak.
- **Reset jembatan Mongo saat World berganti** — dokumen yatim.
- **`Value::UInt`** — memutus `match` pengguna.
- **Quote semua identifier** — churn SQL/tes tanpa manfaat untuk kasus umum.

Rincian, bukti, dan tabel alternatif lengkap di
[RFC-0036](../RFC/RFC-0036-audit-0.7-soundness-stability-security.md).
