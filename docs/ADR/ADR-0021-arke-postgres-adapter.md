# ADR-0021: `arke-postgres` — adapter Postgres sebagai sumber kebenaran

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-28
- **RFC terkait:** [RFC-0021](../RFC/RFC-0021-arke-postgres-adapter.md)

## Konteks

Snapshot (M-6) memberi persistensi portabel tapi **blob buram** — tak bisa di-query/join SQL atau dibaca service lain. Kebutuhan: Postgres sebagai **sumber kebenaran durable** yang dapat di-query SQL. Ini di luar sifat inti Arke (bukan DB/ORM), maka lewat **adapter**. `arke` core wajib tetap 0-dependensi (STD-0003).

## Keputusan

Kami memilih:

1. **Crate adapter terpisah `arke-postgres`** (+ `arke-postgres-derive`), depend `arke` + `sqlx` (async). Core `arke` tak berubah; dep DB terkurung di adapter. Cek STD-0003 CI di-scope ke `-p arke`.
2. **Pemetaan relasional berkolom-tipe sejak v1**: tiap field komponen → **kolom SQL nyata ber-tipe** (bukan JSONB blob) → query/index/join/agregasi langsung. Field non-skalar → kolom `JSONB` *fallback*.
3. **`#[derive(PgComponent)]`** (tulis-tangan, pola `arke-derive`) menurunkan skema: `TABLE`, `COLUMNS` (nama+tipe SQL), `to_params`, `from_params`. Kolom-tipe butuh tipe Rust konkret yang `Value` buang → derive, bukan `Serialize`/`Value` saja.
4. **API async (sqlx)**: `connect`/`migrate`/`load`/`save`; pool + migrasi + query cek-kompilasi.
5. **Model working-set**: `World` materialized dari Postgres, dijalankan deterministik, ditulis-balik di titik terkendali — **bukan** per-tick (impedance mismatch).
6. **Konsistensi**: `save` transaksional; **generation = optimistic lock** (memakai ulang invarian generational STD-0007 sebagai kolom versi DB); despawn → DELETE cascade.
7. **Bertahap**: v1 derive+skema+load/save+optimistic-lock; v2 tulis-balik inkremental + `ALTER` migrasi; v3 materialisasi query-scoped.

## Konsekuensi

**Positif:**

- Keadaan ECS jadi **sumber kebenaran SQL-native** (query/join/analitik, lintas-service) tanpa mengubah core.
- Kolom-tipe = skema self-documenting, index/constraint kaya.
- Determinisme ECS terjaga (Postgres = batas I/O; `load` `ORDER BY entity_id`).
- Fidelity bersandar pada `Serialize`/`Value` (STD-0002) + `schema_version` (STD-0001).

**Negatif / biaya:**

- Butuh derive baru + pemetaan tipe; hanya komponen ber-`derive(PgComponent)` yang dipersist (opt-in).
- Evolusi skema komponen → migrasi `ALTER TABLE`.
- Uji integrasi butuh Postgres nyata (di luar CI self-contained core).
- Field non-skalar turun ke `JSONB` (kurang relasional).

**Netral / catatan:**

- Model working-set, bukan write-through real-time — sinkronisasi di titik terkendali aplikasi.
- Adapter tak tunduk STD-0003 (ia gerbang dependensi); core tetap standalone.

## Alternatif yang ditolak

- **JSONB per komponen / blob tunggal** — kalah ergonomis SQL; JSONB hanya jadi *fallback* field non-skalar.
- **Driver sync / `tokio-postgres` langsung / diesel** — default pengguna async; sqlx beri pool+migrasi+cek-kompilasi.
- **Sinkronisasi per-tick (write-through)** — impedance mismatch; membunuh performa ECS.

Rincian pertimbangan ada di [RFC-0021](../RFC/RFC-0021-arke-postgres-adapter.md).
