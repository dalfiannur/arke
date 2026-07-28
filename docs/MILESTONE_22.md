# Milestone 22 — Test Hardening (model-based + stress)

> Pengerasan menuju kematangan produksi. Bukan fitur/API baru — memperkuat keyakinan pada bagian paling berisiko (penyimpanan archetype + eksekutor paralel) lewat uji **acak-deterministik**.

## Tujuan

Menaikkan kepercayaan pada kebenaran & soundness dengan uji **model-based (property)** dan **stress**, tanpa dependensi eksternal (LCG tulis-tangan, reproducible). Titik fokus: storage (spawn/despawn/insert/remove/bundle + query) dan `run_parallel` (STD-0006).

## Ruang lingkup

**Termasuk:**

- **Uji model-based storage**: barisan operasi acak vs *oracle* referensi; verifikasi `contains`/`get`/`query`/handle-basi konsisten. Mencakup recycling generational & pindah archetype.
- **Uji stress paralel**: `run_parallel` diulang banyak kali harus **selalu** setara `run` serial (mengguncang interleaving thread).
- **Skala via `cfg!(miri)`**: kecil di bawah miri (soundness `unsafe`), besar di `cargo test` (skala/kebenaran).

**Tidak termasuk:**

- Fuzzing eksternal (`cargo-fuzz`), benchmark kompetitif — terpisah.

## Kriteria selesai (Definition of Done)

- [x] Uji model-based: 8 seed × 4000 op acak konsisten dengan oracle — hijau (`model_based_storage_konsisten_dengan_oracle`).
- [x] Handle basi (despawn+respawn slot) terdeteksi `!contains` — teruji (dead-handle sweep).
- [x] Ekuivalensi bundle↔insert & get/query benar di sekuens acak — teruji (oracle memverifikasi nilai + jumlah query).
- [x] Uji stress paralel: 300 run `run_parallel` == serial (graf konflik non-trivial) — hijau (`stress_run_parallel_selalu_setara_serial`).
- [x] Kedua uji jalan di bawah miri (kecil, `cfg!(miri)`) & `cargo test` (besar).
- [x] Semua tes + miri hijau di CI.

## Catatan / temuan

- 2026-07-28: harness ditambahkan. (Temuan bug, bila ada, dicatat di sini.)
