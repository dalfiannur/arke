# Milestone 3 — Data-Parallel Iteration

> Disalin dari [MILESTONE_TEMPLATE.md](MILESTONE_TEMPLATE.md). Lihat [RFC-0004](RFC/RFC-0004-data-parallel-iteration.md) / [ADR-0004](ADR/ADR-0004-data-parallel-iteration.md).

## Tujuan

Mengaktifkan paralelisme nyata secara **aman**: iterasi komponen yang membagi baris entity antar-thread, dengan jaminan hasil identik dengan eksekusi serial (STD-0006). Ini adiktif di atas M-1/M-2 dan tidak memperkenalkan `unsafe` maupun dependensi eksternal.

## Ruang lingkup

**Termasuk:**

- `World::par_for_each::<T>(f)` — menerapkan `f: Fn(&mut T) + Sync` pada tiap pemilik `T`, paralel via `std::thread::scope` + `chunks_mut`.
- Jumlah thread dari `std::thread::available_parallelism`.
- Bukti STD-0006: tes yang membandingkan hasil paralel vs serial pada beban per-entity.

**Tidak termasuk (sengaja ditunda):**

- Paralelisme tingkat-sistem (eksekusi stage scheduler di thread) — butuh sistem berbasis-tipe.
- Sistem berbasis-tipe `fn(Query<&A>, Query<&mut B>)`.
- Varian read-only & pasangan `(&A, &mut B)` paralel; thread pool; operasi reduksi/agregasi.

## Artefak yang dihasilkan

| Artefak | Bentuk |
| --- | --- |
| RFC-0004 | Proposal iterasi data-parallel yang aman |
| ADR-0004 | Keputusan yang diterima dari RFC-0004 |
| Kode + tes | `World::par_for_each` + unit test |
| Bukti STD-0006 | Tes integrasi paralel = serial |

## Kriteria selesai (Definition of Done)

- [ ] `par_for_each::<T>` menerapkan `f` pada setiap pemilik `T` (lintas semua archetype).
- [ ] Implementasi memakai `std::thread::scope` + `chunks_mut` — **tanpa `unsafe`** (di bawah `deny(unsafe_code)`).
- [ ] Hasil paralel identik dengan hasil serial untuk closure per-elemen independen (bukti STD-0006) — disertai tes.
- [ ] Deterministik antar-run (tes lintas-run).
- [ ] Core tetap `--no-default-features` & bebas dependensi eksternal (STD-0003).
- [ ] RFC-0004 & ADR-0004 ditulis serta konsisten dengan kode.
- [ ] Semua tes hijau.

## Ketergantungan

- **Butuh selesai lebih dulu:** M-1 (World + query).
- **Membuka jalan bagi:** sistem berbasis-tipe + paralelisme tingkat-sistem; snapshot/serialisasi.

## Pertanyaan terbuka

- Ambang ukuran di mana paralel mengalahkan serial (overhead spawn) → profil kemudian.
- Varian read-only / pasangan paralel & thread pool → milestone berikutnya.
