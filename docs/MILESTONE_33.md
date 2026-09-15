# Milestone 33 — Audit 0.7.0: soundness, stabilitas, keamanan, performa (RFC-0036)

> Audit menyeluruh seluruh workspace, lalu gelombang breaking kedua sebelum
> soak menuju 1.0 (RN-0004). Setiap temuan dibuktikan dengan tes yang gagal
> sebelum perbaikan; setiap perubahan `unsafe` diverifikasi miri di CI.

## Tujuan

Menjadikan klaim inti arke — *jalur pengguna bebas `unsafe`*, *determinisme by
construction*, *panic & input tak tepercaya tak merusak keadaan* — benar secara
struktural, bukan hanya terdokumentasi, dan menutup jalur kehilangan-data di
adapter sebelum API dibekukan di 1.0.

## Ruang lingkup

**Termasuk:**

- Core `arke` 0.7.0: `Component: Send + Sync`; `ReadOnlyQuery` + `unsafe fn
  each_cached_unchecked`; eksekutor panic-safe; `spawn_at` vs free-list;
  `MAX_JSON_DEPTH`, tolak index duplikat; `WorldId`; `Serialize::name` +
  `#[serialize(name/default)]` + alias + `try_load_snapshot`; kolam thread
  persisten (`pool`); indeks archetype + `scratch_ids`; kasus tepi
  serialisasi/overflow; `each_res` panic-safe.
- `arke-postgres` 0.16.0 / derive 0.8.0 / `arke-cache` 0.4.0: jembatan
  ber-`Entity` + `WorldId`; `Ref` ber-generation; cache ber-fingerprint skema;
  commit deterministik + batch `UNNEST`; muat aditif; `fetch` mengisi jembatan;
  FK reconcile; `#[pg(table)]` + `quote_ident`; bind tipe tetap per kolom;
  `decode_row`/`renumber` hardening.
- `arke-mongo` 0.1.0: penjaga `WorldId` (`WorldMismatch`), `fork()`.
- CI: workflow `Audit` (RustSec) + bench W6/W7/W8 sebagai regresi-guard.
- Dokumen: RFC-0036, ADR-0036, CHANGELOG, README tiap crate, catatan
  "Diselesaikan" pada pertanyaan terbuka RFC-0035.

**Tidak termasuk (sengaja ditunda):**

- Rilis/tag ke crates.io (langkah berikutnya, terpisah).
- Kolam thread bersama antar-owner; `CHECK` constraint ber-nama-stabil;
  batas alokasi otomatis di `try_load_snapshot`; edge transisi archetype
  (tetap ditolak, ADR-0029).

## Artefak yang dihasilkan

| Artefak | Bentuk |
| --- | --- |
| RFC-0036 | Temuan audit + usulan perbaikan + alternatif, Accepted |
| ADR-0036 | Keputusan gelombang breaking kedua; supersede sebagian ADR-0029 |
| Kode + tes | `src/pool.rs` (baru), `src/{component,query,schedule,world,serialize,snapshot,error}.rs`, `arke-derive`, `arke-postgres{,-derive}`, `arke-mongo`; tes regresi `tests/{parallel,snapshot,derive,resources}.rs`, `arke-postgres/tests/{audit_regressions,audit_identifiers_batch}.rs`, `arke-mongo/tests/store.rs` |
| CI | `.github/workflows/audit.yml`; `benches/storage_workloads.rs` W6/W7/W8 |
| CHANGELOG | Bagian *Changed (BREAKING)*, *Added*, *Performance*, *Security*, *Fixed* di `[Unreleased]` |

## Kriteria selesai (Definition of Done)

Milestone dianggap **selesai** ketika semua benar:

- [x] Tiga PoC soundness/keamanan (aliasing `&mut` dari `&World`, data race
      `Cell` dua pembaca, stack overflow JSON) gagal dikompilasi / ditolak,
      dibuktikan doctest `compile_fail` & tes.
- [x] Panic di sistem paralel dipropagasi dalam batas waktu (tes `parallel.rs`).
- [x] Suite `arke-postgres` & `arke-mongo` hijau terhadap DB nyata (lokal + CI),
      termasuk skenario lintas-World, evolusi skema + cache, slot terdaur-ulang,
      batas batch, semua tipe kolom + NULL.
- [x] `cargo clippy --workspace --all-targets` 0 warning; `cargo fmt --check`;
      miri hijau; bench regresi-guard (`ARKE_BENCH_CHECK=1`) hijau.
- [x] Workflow `Audit` hijau (advisori rustls 0.23.43 diselesaikan).
- [x] RFC-0036 Accepted + ADR-0036; CHANGELOG lengkap; README tiap crate
      diperbarui.

## Ketergantungan

- **Butuh selesai lebih dulu:** RN-0004 gelombang pertama (RFC-0026/27/28),
  RFC-0034/0035 (jembatan `pid`, `arke-mongo`).
- **Membuka jalan bagi:** rilis 0.7.0/0.16.0/0.8.0/0.4.0/arke-mongo 0.1.0 →
  soak → Milestone 1.0.

## Pertanyaan terbuka

Lihat RFC-0036 §Pertanyaan terbuka (kolam bersama, `CHECK` ber-nama-stabil,
batas alokasi snapshot).
