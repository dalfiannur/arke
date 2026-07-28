# RFC-0021: `arke-postgres` — adapter Postgres sebagai sumber kebenaran

- **Status:** Draft <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-28
- **Milestone:** M-20 (Postgres Adapter) — *diusulkan*
- **ADR terkait:** ADR-0021 (*ditulis saat RFC ini diterima*)

## Ringkasan

Crate **adapter terpisah `arke-postgres`** yang menjadikan **PostgreSQL sumber kebenaran (source of truth) yang durable** bagi keadaan ECS, dengan **pemetaan relasional** (satu tabel per tipe komponen) yang dapat di-query SQL biasa (join lintas-komponen, analitik, akses dari service lain), lewat API **async** (sqlx/tokio). `arke` core **tetap 0-dependensi** (STD-0003); seluruh dependensi DB terkurung di crate adapter.

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

### 2. Skema relasional

**Registry entity** (cermin generational index, STD-0007):

```sql
CREATE TABLE arke_entities (
    entity_id  BIGINT PRIMARY KEY,   -- index slot (u32) sebagai BIGINT
    generation INT NOT NULL          -- versi; juga optimistic-lock
);
```

**Satu tabel per tipe komponen terdaftar**, dikunci **nama tipe** (portabel, seperti Snapshot):

```sql
CREATE TABLE cmp_position (           -- nama diturunkan dari type_name komponen
    entity_id BIGINT PRIMARY KEY REFERENCES arke_entities(entity_id) ON DELETE CASCADE,
    data      JSONB NOT NULL          -- komponen via Serialize → Value → JSON
);
```

- **Komponen sebagai `JSONB`** (v1): tipe Rust arbitrer memetakan tanpa derive-skema; tetap **query-able** (`data->>'x'`) dan **join-able** (JOIN pada `entity_id`). Memakai ulang `Serialize`/`Value` yang sudah ada.
- Query lintas-komponen = JOIN biasa:
  ```sql
  SELECT p.entity_id, p.data->>'x', h.data->>'hp'
  FROM cmp_position p JOIN cmp_health h USING (entity_id);
  ```
- **Kolom bertipe** (memproyeksikan field → kolom SQL nyata untuk index/join lebih kaya) = **opt-in lanjutan** (butuh trait/derive skema per komponen).

### 3. API async (sqlx)

```rust
let store = PgStore::connect(&url).await?;        // connection pool
store.migrate(&world).await?;                      // buat/pastikan tabel utk komponen terdaftar

// Materialize working-set dari Postgres (deterministik: ORDER BY entity_id → STD-0005).
store.load(&mut world).await?;

// ... jalankan Schedule secara in-memory (deterministik) ...

// Tulis-balik (transaksional).
store.save(&world).await?;
```

- `migrate` membuat `arke_entities` + `cmp_<name>` untuk tiap komponen yang di-`register_serializable`.
- `load` men-SELECT semua entity + komponen, merekonstruksi `World` (memakai `inserter`/`load_snapshot`-path). Urutan `ORDER BY entity_id` menjaga determinisme iterasi (STD-0005) pasca-materialize.
- `save` menulis seluruh working-set dalam **satu transaksi** (upsert entity + komponen, DELETE yang hilang).

### 4. Konsistensi & konkurensi

- **Transaksi**: tiap `save` atomik.
- **Generation = optimistic lock**: tulis-balik `UPDATE … WHERE entity_id=$1 AND generation=$2`; mismatch → error konflik (writer lain mengubah entity itu). Ini **memakai ulang invarian generational** (STD-0007) sebagai kolom versi DB — handle basi = baris usang.
- **Despawn → DELETE** (cascade ke tabel komponen).
- **Multi-writer**: Postgres otoritatif; konflik terdeteksi lewat generation. Kebijakan resolusi (retry / last-writer-wins / merge) = **open question**.

### 5. Fidelity & versi

- Round-trip komponen bersandar pada `Serialize`/`Value` (STD-0002).
- `schema_version` (STD-0001) disimpan (mis. tabel `arke_meta`) untuk migrasi format lintas-versi.

### 6. Determinisme

Eksekusi ECS tetap **deterministik** atas working-set yang dimuat; Postgres adalah **batas I/O**, bukan bagian jalur panas. `load` **wajib** deterministik (`ORDER BY entity_id`) agar urutan iterasi identik antar-materialize (STD-0005).

### 7. Rencana bertahap

| Fase | Isi |
| --- | --- |
| **v1** (M-20) | Skema (entity + JSONB per komponen), `migrate`, `load`/`save` penuh, transaksi, generation optimistic-lock. |
| v2 | Tulis-balik **inkremental** via change-log (kandidat: memanfaatkan `CommandBuffer`/pelacakan-perubahan sebagai sumber diff). |
| v3 | **Kolom bertipe** (proyeksi field → kolom SQL) via trait/derive skema; materialisasi **query-scoped** (muat subset dunia); partial worlds. |

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih (v1) |
| --- | --- | --- | --- |
| Snapshot blob JSONB tunggal | Sepele; memakai ulang M-6 utuh | **Tak** query-able/join-able → gagal syarat "sumber kebenaran SQL" | Tak memenuhi tujuan; tetap tersedia untuk pure save/restore |
| Kolom bertipe penuh sejak v1 | Query/index SQL terkaya | Butuh derive-skema per komponen; migrasi rumit | Ditunda ke v3; JSONB cukup untuk join/query v1 |
| Driver sync (`postgres`/diesel) | Tanpa runtime | Default pengguna **async**; blocking di service async buruk | Async dipilih; sync bisa jadi feature lanjutan |
| `tokio-postgres` langsung | Ringan | Tanpa pool/migrasi bawaan | sqlx: pool + migrasi + query cek-kompilasi |
| Sinkronisasi per-tick (write-through) | Selalu konsisten | Impedance mismatch; membunuh performa ECS | Model working-set + sync di titik terkendali |

## Dampak

- **Kompatibilitas / migrasi:** aditif & **terisolasi**. `arke` core tak berubah, tetap 0-dep (STD-0003) — gerbang CI standalone tetap hijau (adapter di luar cek, atau di workspace terpisah).
- **Keamanan:** adapter murni safe Rust + sqlx; tak menyentuh `unsafe` core.
- **Konsekuensi pada invarian:** memperluas **portabilitas/kepemilikan data** ke Postgres tanpa mengorbankan determinisme (batas I/O). Snapshot (STD-0001/0002) jadi fondasi fidelity.

## Pertanyaan terbuka

- **Pelacakan-perubahan** untuk tulis-balik inkremental — perlukah hook di core (mis. change-log), atau cukup diff snapshot? Bisakah `CommandBuffer` jadi sumber diff?
- **Kebijakan konflik** multi-writer (retry / LWW / merge) saat generation mismatch.
- **Evolusi skema komponen** (field ditambah/dihapus) & migrasi JSONB.
- **Materialisasi parsial** (query-scoped world) — muat hanya subset entity/komponen.
- **Pemetaan nama tabel** dari `type_name` (namespace, tabrakan, sanitasi identifier SQL).
- **Feature `sync`** (driver blocking) untuk pengguna non-async — perlukah?

## Keputusan

*Belum diputuskan.* RFC ini **Draft** untuk ditinjau. Bila diterima: tulis ADR-0021, buka M-20 (fase v1), lalu TDD (skema + `migrate`/`load`/`save` + optimistic-lock) dengan Postgres uji (via `sqlx::test` atau kontainer) di CI.
