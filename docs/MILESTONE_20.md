# Milestone 20 — Postgres Adapter (fase v1)

> Disalin dari [MILESTONE_TEMPLATE.md](MILESTONE_TEMPLATE.md). Lihat [RFC-0021](RFC/RFC-0021-arke-postgres-adapter.md) / [ADR-0021](ADR/ADR-0021-arke-postgres-adapter.md).

## Tujuan

Menjadikan Postgres sumber kebenaran durable bagi keadaan ECS lewat crate adapter `arke-postgres` dengan pemetaan relasional **berkolom-tipe**. Fase v1: derive skema + `migrate`/`load`/`save` penuh + optimistic-lock. Core `arke` tetap 0-dependensi (STD-0003).

## Ruang lingkup

**Termasuk:**

- Crate `arke-postgres` (trait `PgComponent`, `PgType`, `ColumnDef`, `PgValue`) + `arke-postgres-derive` (`#[derive(PgComponent)]`, 0-dep proc-macro).
- Pemetaan tipe Rust → SQL (skalar + `Option<T>` nullable); `to_params`/`from_params` round-trip.
- Skema kolom-tipe: `arke_entities` + `cmp_<name>` per komponen; `migrate` (CREATE TABLE).
- `PgStore` async (sqlx): `connect`/`load`/`save` transaksional + generation optimistic-lock.
- Cek STD-0003 CI di-scope ke `-p arke` (adapter dikecualikan).

**Tidak termasuk (sengaja ditunda):**

- Field non-skalar → JSONB fallback (subfase); tulis-balik inkremental (v2); `ALTER` migrasi (v2); materialisasi query-scoped (v3); feature driver sync.

## Artefak yang dihasilkan

| Artefak | Bentuk |
| --- | --- |
| RFC-0021 / ADR-0021 | Proposal + keputusan adapter |
| `arke-postgres-derive` | `#[derive(PgComponent)]` (kode + tes, tanpa DB) |
| `arke-postgres` | trait/tipe + `PgStore` async (kode + tes) |

## Kriteria selesai (Definition of Done)

- [x] `#[derive(PgComponent)]` meng-emit `TABLE` + `COLUMNS` benar untuk struct skalar — teruji tanpa DB (`table_dan_kolom_untuk_struct_skalar`).
- [x] Pemetaan tipe Rust → SQL benar (i8..i64/isize, u8..u64/usize, f32/f64/bool/String) — teruji. *(`Option<T>` nullable: menyusul.)*
- [x] `to_params`/`from_params` round-trip setia — teruji (`to_params_from_params_round_trip`, `from_params_menolak_bentuk_salah`).
- [x] `create_table_sql` (pembangun `migrate`) benar — teruji (`create_table_sql_benar`).
- [ ] Tipe tak-terpetakan → `compile_error!` (jalur ada; uji trybuild menyusul).
- [x] `migrate`/`save`/`load` round-trip `World` ↔ Postgres setia; handle direkonstruksi (`spawn_at`); determinisme `ORDER BY entity_id` — teruji Postgres (`pgstore_save_load_round_trip_dan_overwrite`, job CI `postgres`).
- [ ] Generation optimistic-lock mendeteksi konflik tulis-balik — menyusul (v1 kini overwrite-penuh transaksional; optimistic-lock = subfase).
- [x] `arke` core tetap 0-dep (STD-0003 `-p arke` hijau; cek CI di-scope).
- [x] Bridge core: `Entity::index/generation` publik + `World::spawn_at` (restore) — teruji.
- [x] RFC-0021 & ADR-0021 konsisten dengan kode.

## Ketergantungan

- **Butuh selesai lebih dulu:** M-6 (snapshot/serialize), M-1 (spawn/insert), `arke-derive` (pola derive).
- **Membuka jalan bagi:** tulis-balik inkremental (v2), kolom bertipe lanjutan, query-scoped (v3).

## Pertanyaan terbuka

- Tipe tak-terpetakan: `compile_error!` (ketat) vs `JSONB` fallback — v1 mulai ketat.
- `Vec<T>`: JSONB vs tabel-anak; migrasi `ALTER`; sanitasi identifier SQL → lanjutan.
