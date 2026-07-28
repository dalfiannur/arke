# RN-0003: Benchmark kompetitif — apakah "ergonomis = cepat" tervalidasi?

- **Status:** Investigating <!-- Open | Investigating | Graduated to RFC-XXXX | Closed -->
- **Tanggal:** 2026-07-28
- **Dipicu oleh:** Pertanyaan kematangan produksi — validasi klaim performa Manifesto.

## Pertanyaan

Invarian inti Manifesto adalah **"jalur ergonomis adalah jalur cepat"**. Apakah performa arke **kompetitif** dengan ECS Rust mapan (hecs, bevy_ecs) pada beban inti — atau klaim itu belum terpenuhi?

## Konteks

RN-0002 mengukur **baseline internal** arke (archetype). RN ini membandingkan arke **langsung** vs hecs 0.11 & bevy_ecs 0.19 memakai harness yang **sama** (`benchmarks/`, hand-rolled `Instant`+`black_box`, di luar workspace inti). Micro-benchmark, satu mesin (Ryzen 5 8645HS) — **angka relatif**.

## Temuan (N = 100k, median beberapa run)

| Beban | arke | hecs | bevy_ecs | arke vs tercepat |
| --- | ---: | ---: | ---: | --- |
| **iter2** (`pos += vel`) | ~2.0 ns/op | **~0.9** | ~1.1 | **~2.2× lebih lambat** |
| **spawn** (100k ber-2-komponen) | ~70 ns/op | **~20** | ~51 | **~3.5× lebih lambat** |
| **get** (akses acak per-entity) | ~25 ns/op | ~13 | **~13** | **~1.9× lebih lambat** |

arke iter2 memakai **query cache persisten** (`each_cached`, jalur `System::each`) — tetap ~2×. Cache tak menutup selisih karena world 1-archetype (scan sudah murah): **overhead ada di loop iterasi**, bukan pemindaian.

## Kesimpulan jujur

**Klaim "ergonomis = cepat" BELUM tervalidasi secara kompetitif.** arke saat ini **1.8–3.5× lebih lambat** dari hecs/bevy_ecs pada ketiga beban inti — termasuk **iterasi query** (jalur panas yang justru diklaim jadi keunggulan). arke ergonomis, aman (0-`unsafe` pengguna), deterministik, dan benar — tetapi **belum** kompetitif secara performa dengan ECS yang sudah bertahun-tahun dioptimasi.

Ini konsisten dengan urutan prioritas proyek: **kebenaran → determinisme → keamanan → governance dulu; penyetelan performa adalah fase tersendiri yang belum dikerjakan.**

## Hipotesis sumber overhead (arah optimasi)

| Beban | Dugaan penyebab | Arah |
| --- | --- | --- |
| **iter2** | Loop lockstep tuple `match (a.next(), b.next())` per-item + panggilan closure tak ter-vektorisasi seperti iterator berbasis-slice hecs/bevy | Iterasi berbasis-indeks/slice zip (`&mut [T]`↔`&[U]`) agar LLVM auto-vektorisasi |
| **spawn** | Per-insert: `find_or_create_archetype` (scan linear), `registry.register` (hashmap), `sort_unstable(ids)`, `column_index` (scan) — berulang | Cache resolusi archetype/kolom; hindari registrasi & sort berulang |
| **get** | Rantai indireksi entity→meta→lokasi→archetype→`column_index`(scan)→downcast→indeks | Cache indeks kolom; jalur entity→komponen lebih langsung (lih. RN-0002) |

## Kriteria graduation

RN ini **graduate menjadi RFC (optimasi performa)** ketika:

1. Ada target performa terukur (mis. iter2 dalam ~1.2× hecs) sebagai Definition of Done; dan
2. Optimasi tak mengorbankan invarian (0-`unsafe` pengguna, determinisme STD-0005/0006, soundness miri); dan
3. Ada regresi-guard (benchmark di CI atau ambang).

Bila performa kompetitif **bukan** tujuan (arke memilih ergonomis+aman+deterministik di atas kecepatan mentah), Manifesto perlu **direvisi** agar tak mengklaim "cepat" secara kompetitif — kejujuran klaim itu sendiri sebuah keputusan.

## Catatan / temuan

- 2026-07-28: benchmark kompetitif dibuat (`benchmarks/`, dikecualikan dari workspace inti). arke 1.8–3.5× lebih lambat pada iter2/spawn/get. Overhead iter2 di loop, bukan scan. Belum ada optimasi dilakukan — RN ini men-*dokumentasikan* gap sebagai baseline jujur.
- 2026-07-28: **iter2 dioptimasi** → graduate ke [RFC-0023](../RFC/RFC-0023-columnar-query-iteration.md) (iterasi berbasis-indeks/kolom). Hasil: arke iter2 **~2.0 → ~1.3 ns/op** (~1.5× lebih cepat), kini **setara bevy_ecs** & ~1.5× hecs (dari ~2.2×). `spawn` (~3.5×) & `get` (~1.9×) **masih** gap — target lanjutan.
- 2026-07-28: **spawn+get dioptimasi** → [RFC-0024](../RFC/RFC-0024-fast-component-resolution.md) (hasher `TypeId` cepat + threading cid bundle). Hasil: spawn ~76→~32 ns/op (~2.4×, kalahkan bevy_ecs), iter2 turun lagi ~0.95 (setara hecs), get ~25→~18–20. **arke kini kompetitif di ketiga beban inti** — klaim "ergonomis = cepat" jauh lebih tervalidasi. Sisa: get ~1.4× hecs.
- 2026-07-28: **`get` di-tweak** — gabung lookup entity (satu akses `entities[i]`: cek hidup+generasi+lokasi) menghapus indeks redundan. get ~20–24 (noisy). **Sisa gap `get` bersifat arsitektural** — rantai indireksi entity→meta→lokasi→archetype + **downcast type-erasure** per akses; menutupnya penuh butuh `unsafe` downcast-tak-tercek (saat itu **diperkirakan** gain marginal — **keliru**, lih. entri berikutnya) atau layout storage lebih datar (perubahan desain). **Dihentikan sementara di ~1.5× hecs** — trade-off jujur: arke pilih 0-`unsafe`-pengguna + type-erasure aman di atas beberapa ns pada akses acak. iter2/spawn (jalur lebih panas) sudah kompetitif/menang.
- 2026-07-28: **`get` dioptimasi** → [RFC-0025](../RFC/RFC-0025-unchecked-column-downcast-get.md) (downcast kolom tak-tercek terkurung, miri-verified). get ~20→~11 ns/op (~1.8×) — dari kelemahan menjadi **mengalahkan hecs & bevy_ecs**. **arke kini kompetitif/menang di KETIGA beban inti** (iter2≈hecs; spawn & get menang vs bevy; get menang vs hecs). Klaim "ergonomis = cepat" **tervalidasi**.
- 2026-07-29: **spawn diselidiki lagi** → [RFC-0029](../RFC/RFC-0029-archetype-resolution-index-edges.md) **DITOLAK**. Hipotesis: sisa gap spawn = scan linear `find_or_create_archetype` O(n_archetypes). Diimplementasi penuh (index hash + edge transisi), lolos oracle + miri, lalu **diukur A/B (git-stash)**. **Premis dibantah:** scan linear OLD di **4096** archetype (~62 ns/op) ≈ di 256 (~68 ns/op) — membandingkan slice-id pendek murah & branch-predicted, tak tumbuh berarti. **Resolusi archetype bukan bottleneck spawn; alokasi (`entities`/kolom push) yang dominan.** Kemenangan terkubur noise mesin (NEW berayun 43–75). **Di-revert** (YAGNI): kompleksitas 2 HashMap/World + edge tak sepadan untuk win tak-terukur. Pelajaran (lagi): **ukur sebelum menambah kompleksitas** — hipotesis "O(n) itu mahal" salah saat n kecil & konstanta mungil. Menutup gap spawn vs hecs butuh menyerang **alokasi**, bukan resolusi — belum ada arah ber-ROI-jelas.
