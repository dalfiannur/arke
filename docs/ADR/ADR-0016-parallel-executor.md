# ADR-0016: Eksekutor paralel tingkat-sistem

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-28
- **RFC terkait:** [RFC-0016](../RFC/RFC-0016-parallel-executor.md)

## Konteks

Tujuan akhir M-5. Storage `UnsafeCell` (M-14) memungkinkan `&mut` kolom dari `&World` bersama. Sistem satu stage aksesnya disjoint (analisis konflik M-2/M-4). Kini eksekutor menjalankannya paralel — `unsafe` terbesar proyek, di atas gerbang miri.

## Keputusan

Kami memilih:

1. **Jalur query berbagi**: `QueryData::each_filtered_shared(&World)` sebagai implementasi; `each_filtered(&mut World)` = `each_filtered_shared(&*world)`. Term `&mut T` mengakses kolom via `unsafe data_mut_shared` (kolom distinct → tak beralias).
2. **Runner sistem**: `Exclusive` (opaque/resource, serial-saja) vs `Shared` (bertipe, paralel-mampu).
3. **`SyncWorld` + `unsafe impl Sync`** (sound via disjoint stage) + **`Schedule::run_parallel`** yang menjalankan stage `Shared` di `std::thread::scope`; stage dengan sistem `Exclusive` → serial.
4. `unsafe` di tiga modul (`storage`/`query`/`schedule`), masing-masing terkurung + `// SAFETY` + **miri-verified**. Jalur pengguna tetap tanpa `unsafe` (STD-0004).

## Konsekuensi

**Positif:**

- Mengaktifkan STD-0006 tingkat-sistem (paralel = serial); *paralelisme yang aman* terwujud.
- `unsafe` terkurung & miri-verified, bukan diklaim.
- `run` serial tak berubah perilaku.

**Negatif / biaya:**

- `unsafe` kini di 3 modul (dari 1) — permukaan lebih luas, tetap terkurung.
- Sistem resource & opaque tak paralel (stage-nya serial).
- Pengembangan bergantung miri CI (lokal tanpa nightly).

**Netral / catatan:**

- Thread pool, command buffer, sistem resource paralel → lanjutan.

## Alternatif yang ditolak

- **`run` otomatis paralel** — perilaku implisit.
- **Paralelkan resource** — butuh interior-mut tambahan.
- **Thread pool** — kompleksitas; `scope` cukup.

Rincian pertimbangan ada di [RFC-0016](../RFC/RFC-0016-parallel-executor.md).
