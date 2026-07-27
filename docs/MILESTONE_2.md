# Milestone 2 — Deterministic Scheduler

> Disalin dari [MILESTONE_TEMPLATE.md](MILESTONE_TEMPLATE.md). Lihat [RFC-0003](RFC/RFC-0003-deterministic-scheduler.md) / [ADR-0003](ADR/ADR-0003-deterministic-scheduler.md).

## Tujuan

Menambahkan lapisan **System** dan **Schedule** di atas `World`: cara mengorganisasi logika menjadi sistem terjadwal yang berjalan dalam urutan deterministik, dengan konflik baca/tulis dianalisis untuk menghasilkan rencana eksekusi paralel. M-2 mengeksekusi rencana itu secara **serial**; eksekusi multi-thread nyata adalah M-3.

## Ruang lingkup

**Termasuk:**

- `System`: pembungkus `FnMut(&mut World)` + deklarasi akses eksplisit (`reads::<T>`/`writes::<T>`).
- Aturan konflik baca/tulis antar-sistem.
- `Schedule`: registrasi sistem, penetapan **stage** deterministik dari konflik, dan `run` stage-demi-stage (serial).
- `Schedule::stages()`: rencana paralel deterministik (kelompok indeks sistem yang aman berjalan bersamaan).

**Tidak termasuk (sengaja ditunda):**

- Eksekusi multi-thread nyata via `std::thread::scope` (M-3) — karena itu STD-0006 belum aktif.
- Sistem berbasis-tipe (`fn(Query<&A>, Query<&mut B>)` dengan akses tersimpul) — M-3.
- Resources (state global) sebagai parameter sistem, dan penjadwalan mutasi struktural.

## Artefak yang dihasilkan

| Artefak | Bentuk |
| --- | --- |
| RFC-0003 | Proposal scheduler deterministik + analisis konflik |
| ADR-0003 | Keputusan yang diterima dari RFC-0003 |
| Kode + tes | `System`, `Schedule` (stage + run) + unit test |

## Kriteria selesai (Definition of Done)

- [x] `System` dapat dibuat dari closure dan mendeklarasikan `reads`/`writes`.
- [x] Aturan konflik benar: write-write & read-write berkonflik; read-read tidak.
- [x] `Schedule::stages()` menempatkan sistem tak-konflik dalam stage yang sama dan yang berkonflik dalam stage berbeda, secara deterministik (bukti STD-0005).
- [x] `Schedule::run` mengeksekusi sistem stage-demi-stage; hasilnya setara eksekusi serial urutan registrasi.
- [x] Menjalankan schedule yang sama dua kali menghasilkan keadaan identik (tes lintas-run).
- [x] RFC-0003 & ADR-0003 ditulis serta konsisten dengan kode.
- [x] Semua tes hijau (21 tes) secara lokal; core tetap `--no-default-features` & bebas dependensi (STD-0003).

## Ketergantungan

- **Butuh selesai lebih dulu:** M-1 (World + query).
- **Membuka jalan bagi:** M-3 (eksekusi paralel via `std::thread::scope`, sistem berbasis-tipe), lalu snapshot/serialisasi.

## Pertanyaan terbuka

- Deklarasi akses eksplisit tak diverifikasi terhadap akses aktual → ditutup oleh sistem berbasis-tipe M-3.
- Mutasi struktural (spawn/despawn) di dalam sistem terhadap paralelisme M-3 → RN saat M-3.
