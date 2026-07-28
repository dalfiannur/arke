# Changelog

Semua perubahan penting pada `arke` didokumentasikan di sini. Format mengikuti
[Keep a Changelog](https://keepachangelog.com/id/1.1.0/); proyek menganut
[Semantic Versioning](https://semver.org) (lihat STD-0010). Riwayat lengkap tiap
rilis juga ada di [GitHub Releases](https://github.com/dalfiannur/arke/releases).

## [Unreleased]

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
