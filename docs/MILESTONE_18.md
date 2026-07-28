# Milestone 18 — Command Buffer

> Disalin dari [MILESTONE_TEMPLATE.md](MILESTONE_TEMPLATE.md). Lihat [RFC-0019](RFC/RFC-0019-command-buffer.md) / [ADR-0019](ADR/ADR-0019-command-buffer.md).

## Tujuan

Memungkinkan **mutasi struktural tertunda** (spawn/despawn/insert/remove) direkam saat hanya `&World` tersedia (sistem paralel), lalu di-apply saat `&mut World` tersedia. Mengurangi kebutuhan sistem `Exclusive` → lebih banyak paralelisme, dengan determinisme & soundness terjaga.

## Ruang lingkup

**Termasuk:**

- Primitif `CommandBuffer`: `spawn()` (+ `EntityCommands::insert`), `despawn`, `insert`, `remove`, `apply(&mut World)`, `is_empty`/`len`/`clear`. Deterministik (urutan-rekam).
- Integrasi scheduler: `System::each_cmd` (`Runner::SharedCmd`, paralel-mampu); buffer per-sistem di-apply di **akhir run**, urutan registrasi (`run` & `run_parallel`).

**Tidak termasuk (sengaja ditunda):**

- `Entity` sebagai term query (pola despawn-self) — follow-up.
- Reservasi entity atomik (handle spawn sinkron); apply per-sync-point intra-run.

## Artefak yang dihasilkan

| Artefak | Bentuk |
| --- | --- |
| RFC-0019 | Proposal command buffer |
| ADR-0019 | Keputusan apply-di-akhir-run, urutan registrasi |
| kode + tes | `command` module, `System::each_cmd`, apply di scheduler |

## Kriteria selesai (Definition of Done)

- [ ] `CommandBuffer` merekam & meng-apply spawn/despawn/insert/remove **urutan-rekam** — teruji.
- [ ] `spawn().insert(..)` menghasilkan entity terkonfigurasi saat apply — teruji.
- [ ] `apply` menguras buffer (dapat dipakai ulang); `is_empty`/`clear` benar — teruji.
- [ ] `System::each_cmd` via `run` (serial) menerapkan perubahan struktural — teruji.
- [ ] `System::each_cmd` via `run_parallel` memberi hasil **identik** dengan `run` (STD-0006) — teruji.
- [ ] Buffer per-sistem terkuras antar-run (tak menumpuk) — teruji.
- [ ] Determinisme terjaga (urutan registrasi apply).
- [ ] Tetap **tanpa `unsafe` baru**; jalur pengguna aman.
- [ ] RFC-0019 & ADR-0019 ditulis serta konsisten dengan kode.
- [ ] Semua tes + miri hijau.

## Ketergantungan

- **Butuh selesai lebih dulu:** M-17 (eksekutor graf/thread-per-sistem), M-1 (spawn/despawn/insert/remove).
- **Membuka jalan bagi:** `Entity` sebagai term query (despawn-self); pengurangan sistem `Exclusive`.

## Pertanyaan terbuka

- `Entity` sebagai term query; reservasi entity atomik; apply per-sync-point → lanjutan.
