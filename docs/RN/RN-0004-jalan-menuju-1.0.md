# RN-0004: Jalan menuju 1.0 — apa yang harus dibekukan, apa yang boleh menyusul?

- **Status:** Investigating <!-- Open | Investigating | Graduated to RFC-XXXX | Closed -->
- **Tanggal:** 2026-07-28
- **Dipicu oleh:** Pertanyaan kematangan — setelah pengerasan 0.5.x (M-22, RN-0003, audit doc), apakah arke siap membekukan API-nya?

## Pertanyaan

Rilis **1.0** adalah janji semver: *"API publik ini tak akan kami patahkan."* Bukan janji fitur-lengkap. Dua sub-pertanyaan:

1. Apakah **bentuk** API arke saat ini layak dibekukan?
2. Apa jalur konkret dari 0.5.2 ke 1.0.0 — dan apa yang boleh menyusul secara **aditif** pasca-1.0?

## Konteks

Per 0.5.2, fondasi kualitas 1.0 sudah di tempat:

| Fondasi | Status | Rujukan |
| --- | --- | --- |
| Kebenaran (property/oracle + stress paralel) | ✅ | M-22 |
| Determinisme (`run` == `run_parallel`, miri) | ✅ | STD-0006 |
| Keamanan (4 `unsafe` terkurung, miri; jalur pengguna bebas-`unsafe`) | ✅ | STD-0004, RFC-0015/0023/0025 |
| Performa kompetitif + regresi-guard CI | ✅ | RN-0003 |
| Dokumentasi (0 `missing_docs`, doc-test, link ter-`deny`) | ✅ | — |
| Kebersihan kode (0 TODO/FIXME/unimplemented) | ✅ | — |

Kualitas **bukan** penghalang. Penghalangnya adalah **komitmen bentuk-API** — hal yang, bila ditunda ke pasca-1.0, menjadi *breaking*.

## Temuan: tiga penghalang bentuk-API

### 1. Trait ekstensi belum di-*seal* (paling kritis)

`Bundle`, `QueryData`, `QueryTerm`, `QueryFilter` adalah **trait publik terbuka** dengan method `#[doc(hidden)]` yang tetap bocor ke permukaan API. Bila dibiarkan, 1.0 **membekukan signature internal mereka** (`fn push`, `fn fetch`, `fn get`, …) — padahal itu murni detail implementasi yang di-generate makro untuk tuple. Konsekuensi: mustahil mengevolusi internal iterasi/bundle pasca-1.0 tanpa mematahkan semver.

`Component` sudah aman (blanket `impl<T: 'static + Send>`); `Serialize` sengaja dapat di-`derive`/impl pengguna (bagian kontrak).

**Keputusan:** *seal* keempat trait ekstensi via pola `mod sealed { pub trait Sealed {} }` sebelum 1.0. Ini justru yang **memungkinkan** pertumbuhan aditif pasca-1.0 (lihat bagian roadmap).

### 2. API query tumpang-tindih

Dua keluarga melakukan hal yang sama:

- `World::query_pair` / `query_pair_ref` — **khusus arity-2**.
- Jalur `QueryData` tuple generik (`<(&A, &mut B)>::each`, arity & mutabilitas campuran).

Membekukan keduanya = komit selamanya pada API redundan & tak-ortogonal.

**Keputusan:** `#[deprecated]` method khusus-arity di 0.6.0, konvergen ke jalur `QueryData` generik. `query`/`query_mut` (arity-1, kasus paling umum) **dipertahankan** sebagai shortcut ergonomis; hanya varian `_pair`/`_pair_ref` yang di-deprecate karena persis ditiru jalur generik.

### 3. Kebijakan yang harus dikomit di 1.0

- **MSRV** — kini Rust 1.86; butuh *policy* eksplisit (mis. "MSRV boleh naik di rilis minor, tak pernah di patch").
- **Format snapshot** — sudah berversi (`schema_version`); bekukan sebagai kontrak round-trip.
- **CHANGELOG** — belum ada; 1.0 lazim mewajibkannya (Keep a Changelog).
- **Versioning ekosistem** — `arke-postgres` ber-versi terpisah; tak wajib 1.0 bersamaan.

## Keputusan lingkup: freeze minimal sekarang

**0.6.0** memuat *hanya* perubahan breaking bentuk-API — seal trait (#1), deprecate query khusus-arity (#2), tambah CHANGELOG & policy (#3). Lalu **soak** beberapa rilis pemakaian nyata, lalu **1.0.0**.

Jangan lompat langsung ke 1.0: 0.6.0 adalah tempat semua breaking terakhir mendarat, dan soak memberi kesempatan pengguna nyata menemukan kejanggalan API sebelum dibekukan.

## Roadmap konkret

```text
0.5.2  (kini)  ── pengerasan selesai
   │
0.6.0  BREAKING ── seal Bundle/QueryData/QueryTerm/QueryFilter (RFC-0026)
   │              ── #[deprecated] query_pair/query_pair_ref (RFC-0027)
   │              ── CHANGELOG.md + MSRV/snapshot policy (RFC-0028)
   │
 soak  ── beberapa rilis; kumpulkan umpan-balik API dari pemakaian nyata
   │
1.0.0  ── hapus item ter-deprecate; bekukan API; jaminan semver
   │
1.x    ── fitur ADITIF (non-breaking, dimungkinkan oleh sealing):
          1.1 events, 1.2 change-detection (Added/Changed),
          1.3 run-conditions, 1.4 relationships/hierarki …
```

## Yang **tidak** menghalangi 1.0

Fitur berikut dapat ditambah **aditif** (metode/tipe baru, tanpa mematahkan signature lama) — jadi tak perlu sebelum freeze; justru *sealing* yang menjaga ruang ini tetap terbuka:

- Change detection (`Added<T>` / `Changed<T>` sebagai `QueryFilter` baru).
- Events / message passing.
- Relationships / hierarki entity.
- Run-conditions & ordering constraints eksplisit.

## Kriteria graduation

RN ini **graduate menjadi tiga RFC** ketika keputusan di atas siap diimplementasi TDD:

1. **RFC-0026** — sealing trait ekstensi.
2. **RFC-0027** — deprecate & konvergensi API query.
3. **RFC-0028** — CHANGELOG + kebijakan MSRV & stabilitas snapshot.

Setelah ketiganya mendarat di **0.6.0** dan periode soak lewat tanpa perubahan breaking baru, buka **Milestone 1.0** (bekukan API, hapus item ter-deprecate).

## Catatan / temuan

- 2026-07-28: RN dibuka. Penilaian: kualitas siap 1.0; penghalangnya bentuk-API. Diputuskan (bersama pemilik proyek): **seal trait ekstensi**, **deprecate query khusus-arity → konvergen ke `QueryData`**, **freeze minimal** (0.6.0 breaking → soak → 1.0). Fitur besar ditunda aditif pasca-1.0.
- 2026-07-28: **Penghalang #1 selesai** → [RFC-0026](../RFC/RFC-0026-seal-extension-traits.md) / [M-23](../MILESTONE_23.md). Keempat trait di-seal via `pub(crate) mod sealed`; diverifikasi doc-test `compile_fail` (RED disaksikan: impl downstream `QueryData`/`QueryFilter` kompilasi sebelum seal). 0 dependensi, tanpa perubahan perilaku. Sisa untuk 0.6.0: RFC-0027 (deprecate query), RFC-0028 (CHANGELOG+policy).
- 2026-07-28: **Penghalang #2 selesai** → [RFC-0027](../RFC/RFC-0027-deprecate-query-pair.md) / [M-24](../MILESTONE_24.md). `query_pair`/`query_pair_ref` `#[deprecated(since=0.6.0)]` → konvergen ke `<(..)>::each`; `query`/`query_mut` dipertahankan. Pemakai internal (contoh, doc) dimigrasi; uji perilaku `#[allow(deprecated)]`. Diverifikasi doc-test `compile_fail` + `#![deny(deprecated)]` (RED disaksikan). Non-breaking (peringatan); dihapus di 1.0. Sisa untuk 0.6.0: RFC-0028 (CHANGELOG+policy).
