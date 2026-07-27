# Milestone 6 — World Snapshot & Serialization

> Disalin dari [MILESTONE_TEMPLATE.md](MILESTONE_TEMPLATE.md). Lihat [RFC-0007](RFC/RFC-0007-world-snapshot.md) / [ADR-0007](ADR/ADR-0007-world-snapshot.md).

## Tujuan

Membuat keadaan `World` dapat di-snapshot ke format terbuka (JSON) yang berversi dan dipulihkan secara setia — mewujudkan invarian *kepemilikan & portabilitas data* dan mengaktifkan STD-0001/0002. Tanpa dependensi eksternal, tanpa `unsafe`.

## Ruang lingkup

**Termasuk:**

- Enum `Value` (bebas-dep) + trait `Serialize` (`to_value`/`from_value`).
- JSON tulis-tangan: `Value` ↔ teks JSON (emit + parse).
- `World::register_serializable::<T>()` — simpan nama tipe stabil + vtable.
- `World::snapshot()` → `Snapshot` dengan `schema_version` (STD-0001).
- `World::load_snapshot()` — round-trip setia (STD-0002).
- `Snapshot::to_json`/`from_json`.
- JSON schema `schema/v1/world-snapshot.schema.json` + contoh valid/tak-valid.

**Tidak termasuk (sengaja ditunda):**

- Rekonstruksi free-list & entity mati persis (hanya entity hidup di-snapshot).
- Migrasi antar `schema_version` (belum ada v2).
- Turunan `Serialize` otomatis (derive macro).

## Artefak yang dihasilkan

| Artefak | Bentuk |
| --- | --- |
| RFC-0007 / ADR-0007 | Proposal & keputusan snapshot |
| Kode + tes | `Value`, `Serialize`, JSON, `snapshot`/`load_snapshot` + unit/integration test |
| Schema | `schema/v1/world-snapshot.schema.json` + contoh |

## Kriteria selesai (Definition of Done)

- [x] `Value` ↔ JSON: emit dan parse round-trip untuk semua varian.
- [x] `snapshot()` menghasilkan `Snapshot` dengan `schema_version` (STD-0001).
- [x] `to_json()` menyertakan `schema_version`; ditolak oleh schema bila hilang (dibuktikan validator repo).
- [x] Round-trip setia: `load_snapshot(&world.snapshot())` menghasilkan `World` setara observasional (STD-0002) — `tests/snapshot.rs`.
- [x] Round-trip lewat teks: `from_json(&world.snapshot().to_json())` juga setia.
- [x] JSON schema `world-snapshot` + contoh valid/tak-valid; validator repo menerima valid & menolak tak-valid.
- [x] Tetap **tanpa `unsafe`** & bebas dependensi eksternal (STD-0003).
- [x] RFC-0007 & ADR-0007 ditulis serta konsisten dengan kode.
- [x] Semua tes hijau (36 tes) secara lokal.

## Ketergantungan

- **Butuh selesai lebih dulu:** M-1 (World + komponen).
- **Membuka jalan bagi:** save/replay/rollback; migrasi format berversi.

## Pertanyaan terbuka

- Rekonstruksi free-list persis → RN bila diperlukan untuk determinisme pasca-restore.
- Derive macro untuk `Serialize` → milestone ergonomi.
