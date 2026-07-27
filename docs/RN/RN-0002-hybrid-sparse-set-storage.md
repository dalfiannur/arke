# RN-0002: Apakah penyimpanan hybrid sparse-set layak menyertai archetype?

- **Status:** Investigating <!-- Open | Investigating | Graduated to RFC-XXXX | Closed -->
- **Tanggal:** 2026-07-28
- **Dipicu oleh:** Diskusi arsitektur (usulan pivot ke sparse-set) — lihat juga [RFC-0017](../RFC/RFC-0017-query-cache.md), [RFC-0018](../RFC/RFC-0018-dependency-graph-executor.md)

## Pertanyaan

Beban-kerja nyata mana (jika ada) yang cukup didominasi **akses acak per-entity** dan/atau **churn struktural** sehingga penyimpanan **sparse-set** (opt-in, berdampingan dengan archetype) membenarkan kompleksitas + biayanya — mengingat archetype sudah dominan pada jalur panas ECS (iterasi query multi-komponen)?

## Konteks

Pivot penuh ke sparse-set **ditolak** (membalik tesis Manifesto: archetype unggul justru untuk iterasi query multi-komponen, jalur panas per-frame). Pertanyaan tersisa: apakah **hybrid opt-in** (mis. tandai komponen tertentu sebagai sparse) sepadan. Keputusan itu **harus berbasis angka**, bukan intuisi — maka RN ini membangun **harness benchmark 0-dependensi** ([`benches/storage_workloads.rs`](../../benches/storage_workloads.rs)) sebagai instrumen dan mencatat **baseline archetype** saat ini.

## Yang sudah diketahui

Trade-off teoretis archetype (SoA per-kombinasi-komponen) vs sparse-set (array padat + peta entity→indeks per-komponen):

| Aspek | Archetype (kini) | Sparse-set |
| --- | --- | --- |
| Iterasi 1 komponen | Cepat (kolom kontigu) | Cepat (padat) |
| Iterasi **N komponen** | **Sangat cepat** (kolom kolokasi, satu archetype) | Lebih lambat (cek keanggotaan / gather antar-array) |
| Akses acak `get(entity)` | Tak-langsung (entity→lokasi→archetype→kolom) | **O(1)** (indeks langsung) |
| Insert/remove komponen | Pindah baris antar-archetype (salin) | Set/clear slot (murah) |
| Fragmentasi (banyak archetype) | Iterasi bisa terpecah; diringankan **query-cache** (RFC-0017) | Tak terpengaruh |

- **Baseline harus diukur**, bukan diasumsikan — arke kini punya query-cache (RFC-0017) & eksekutor graf (RFC-0018) yang mengubah kalkulus.
- Kendala: **0 dependensi** (STD-0003) → harness tak boleh pakai `criterion`; memakai `std::time::Instant` + `black_box`, urutan acak lewat LCG deterministik.

## Instrumen: harness benchmark

`benches/storage_workloads.rs` (`harness = false`), lima beban pembeda archetype vs sparse-set:

- **W1** iterasi satu-komponen (`&mut Position`).
- **W2** query dua-komponen (`&Position, &mut Velocity`) — jalur panas per-frame.
- **W3** akses acak per-entity (`get::<Position>`, urutan LCG).
- **W4** churn struktural (insert+remove `Tag` → round-trip pindah archetype).
- **W5** iterasi terfragmentasi (`Position` tersebar di 16 archetype).

Menjalankan: `cargo bench --bench storage_workloads` (smoke: `ARKE_BENCH_QUICK=1 …`).

## Baseline archetype (temuan awal)

Lingkungan: **AMD Ryzen 5 8645HS**, rustc 1.97.1, profil `bench` (`-O`), 100k entity (W4: 10k), median dari beberapa run. **Angka relatif, bukan absolut** (mesin-spesifik).

| Beban | ns/op | Mop/s | Catatan |
| --- | --- | --- | --- |
| W1 iter single | ~1.4 | ~700 | dasar iterasi |
| **W2 query two** | **~1.2** | **~845** | jalur panas — sangat cepat |
| W3 random get | ~30 | ~33 | **~20–25× lebih lambat** dari iterasi |
| W4 churn move | ~40 | ~25 | biaya pindah archetype per operasi |
| W5 fragmented (16 arch) | ~1.3 | ~775 | **iterasi ~tak terpengaruh fragmentasi** |

**Interpretasi:**

1. Jalur panas ECS (W2 iterasi multi-komponen) sudah **~1 ns/op**; fragmentasi (W5) hampir tak memengaruhinya (query-cache bekerja). Ini justru **keunggulan** yang akan **dikorbankan** sparse-set.
2. Dua beban yang secara teori diuntungkan sparse-set (W3 akses acak ~30 ns, W4 churn ~40 ns) memang **20–30× lebih mahal** dari iterasi — tapi **hanya relevan** bila sebuah aplikasi nyata **didominasi** pola itu.
3. Sebagian besar biaya W3 kemungkinan dari indireksi entity→lokasi + lookup kolom; ada ruang optimasi **di dalam archetype** (mis. cache lokasi) sebelum menempuh sparse-set.

## Arah yang dieksplorasi

| Arah | Catatan awal |
| --- | --- |
| **Tetap archetype** (default) | Baseline iterasi unggul & stabil; optimalkan `get`/churn di dalam archetype dulu (murah, tanpa `unsafe`/kompleksitas baru). **Condong ke sini.** |
| Hybrid opt-in (komponen sparse) | Hanya bila ada beban nyata yang W3/W4-dominan **dan** tak bisa direstrukturisasi. Menambah jalur kode, matriks uji, & risiko `unsafe`. |
| Pivot penuh sparse-set | **Ditolak** — membalik keunggulan iterasi multi-komponen (tesis Manifesto). |

## Kriteria graduation

RN ini **graduate menjadi RFC (hybrid sparse-set)** hanya bila **semua** benar:

1. Ada **beban-kerja nyata** (bukan mikro-benchmark) yang profilnya menunjukkan `get`/churn (kelas W3/W4) mendominasi waktu-frame; **dan**
2. Optimasi **di dalam archetype** (mis. cache lokasi entity untuk `get`, atau strategi churn) terbukti **tak cukup** menutup selisih; **dan**
3. Desain hybrid dapat mempertahankan **STD-0004** (jalur pengguna tanpa `unsafe`), **STD-0005/0006** (determinisme/paralel), dan tak meregresi baseline W2/W5.

Bila (1) tak pernah muncul dalam beban target, RN ini **ditutup (won't pursue)** — archetype + query-cache + eksekutor graf sudah memadai.

## Catatan / temuan

- 2026-07-28: RN dibuka; harness 0-dependensi dibangun; baseline archetype dicatat (di atas). Kesimpulan sementara: **belum ada justifikasi** untuk hybrid — jalur panas sudah optimal; langkah berikut yang lebih murah adalah mengoptimasi `get` (W3) di dalam archetype bila sebuah beban nyata menuntutnya.
