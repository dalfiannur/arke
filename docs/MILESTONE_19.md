# Milestone 19 — Entity Query Term

> Disalin dari [MILESTONE_TEMPLATE.md](MILESTONE_TEMPLATE.md). Lihat [RFC-0020](RFC/RFC-0020-entity-query-term.md) / [ADR-0020](ADR/ADR-0020-entity-query-term.md).

## Tujuan

Menjadikan `Entity` sebuah term query yang menghasilkan handle entity baris — melengkapi command buffer (RFC-0019) agar sistem paralel dapat memutasi struktural **entity yang sedang diiterasi** (despawn-self / insert / remove). Tanpa `unsafe` baru; tanpa konflik penjadwalan; determinisme terjaga.

## Ruang lingkup

**Termasuk:**

- `Entity` sebagai `QueryData` tunggal (`<Entity>::each`) & `QueryTerm` (dalam tuple `(Entity, &T, …)`).
- Generalisasi `QueryTerm`: `Requirement { Column | Any | Never }` + `iter_shared(archetype, col)`.
- `Archetype::entities()` accessor (pub(crate)).
- `Entity::access()` kosong (tanpa konflik).

**Tidak termasuk (sengaja ditunda):**

- Reservasi entity atomik (handle spawn sinkron); term non-kolom lain (`Has<T>`).

## Artefak yang dihasilkan

| Artefak | Bentuk |
| --- | --- |
| RFC-0020 | Proposal `Entity` sebagai term query |
| ADR-0020 | Keputusan generalisasi `QueryTerm` |
| kode + tes | `QueryData`/`QueryTerm` untuk `Entity`, refactor term, `each_cmd` despawn-self |

## Kriteria selesai (Definition of Done)

- [x] `<(Entity, &T)>::each` menghasilkan handle + komponen yang benar — teruji (`tuple_entity_dan_komponen_menghasilkan_handle`).
- [x] `<Entity>::each` mengiterasi semua entity ber-komponen — teruji (`entity_tunggal_mengiterasi_semua_ber_komponen`).
- [x] `Entity` dalam tuple tak mengubah pencocokan komponen lain — teruji (idem: `f` tanpa Pos tak cocok).
- [x] `each_cmd::<(Entity, &Health)>` men-despawn entity yang diiterasi (despawn-self) — teruji (`each_cmd_despawn_self_via_entity_term`).
- [x] `run` ≡ `run_parallel` untuk sistem entity+cmd (STD-0006) — teruji (`each_cmd_despawn_self_run_parallel_setara_serial`, + 40× stress).
- [x] `Entity` tak menyumbang akses (tak menambah konflik/stage) — teruji (`entity_term_tak_menyumbang_akses`).
- [x] Determinisme & urutan iterasi identik dengan sebelumnya — 82 tes hijau.
- [x] Tetap **tanpa `unsafe` baru**; jalur pengguna aman (contoh `no_unsafe` memakai term `Entity` di bawah `forbid(unsafe_code)`).
- [x] RFC-0020 & ADR-0020 ditulis serta konsisten dengan kode.
- [x] Semua tes + miri hijau (miri di CI).

## Ketergantungan

- **Butuh selesai lebih dulu:** M-13 (query tuple generik), M-18 (command buffer).
- **Membuka jalan bagi:** pola despawn-self; term query non-kolom lanjutan.

## Pertanyaan terbuka

- Reservasi entity atomik; term non-kolom (`Has<T>`) → lanjutan.
