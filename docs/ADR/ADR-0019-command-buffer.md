# ADR-0019: Command buffer (mutasi struktural tertunda)

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-28
- **RFC terkait:** [RFC-0019](../RFC/RFC-0019-command-buffer.md)

## Konteks

Mutasi struktural butuh `&mut World`, jadi sistem paralel (`&World`) tak bisa spawn/despawn/insert/remove — memaksa sistem `Exclusive` serial (segmentasi RFC-0018). Efek command juga tak tercermin di `Access`, sehingga analisis konflik tak menyadarinya.

## Keputusan

Kami memilih:

1. **`CommandBuffer`** merekam command sebagai penutup (`Spawn(Vec<konfigurator FnOnce(&mut World, Entity)>)` / `Op(FnOnce(&mut World))`). `spawn()` → builder `EntityCommands` (`.insert()`). `apply(&mut World)` menguras urutan-rekam. `Send` (perekaman paralel).
2. **`System::each_cmd`** (varian `Runner::SharedCmd`, paralel-mampu): tiap sistem punya buffer sendiri, direkam oleh thread pemiliknya (`&mut System`) — **tanpa sinkronisasi**.
3. **Apply di akhir run**, **urutan registrasi**, di `run` maupun `run_parallel`. Command **tak terlihat selama run** → tanpa celah `Access`, sound, dan `run` ≡ `run_parallel` (STD-0006).

## Konsekuensi

**Positif:**

- Sistem paralel dapat memicu perubahan struktural → lebih sedikit sistem `Exclusive` → lebih banyak paralelisme.
- **Tanpa `unsafe` baru**; determinisme dijaga (urutan-registrasi).
- Primitif `CommandBuffer` berguna mandiri (di luar scheduler).

**Negatif / biaya:**

- Command hanya tampak **setelah run** (bukan intra-run) — semantik "apply di akhir".
- Alokasi penutup per-command (Box) — dapat dioptimasi kemudian bila perlu.

**Netral / catatan:**

- Efek command di luar `Access` → *dengan sengaja* ditunda agar tak balapan.
- Handle spawn adalah `Entity` **nyata** (via konfigurator saat apply), bukan reservasi sinkron.

## Alternatif yang ditolak

- **Apply per-segmen** — celah `Access` tetap; lebih rumit menjaga `run` ≡ `run_parallel`.
- **Reservasi entity atomik** — kompleks; konfigurator `FnOnce(&mut World, Entity)` cukup.
- **Buffer global bersama** — butuh kunci; per-sistem bebas-kunci lebih ringan.
- **Command enum bertipe** — matriks varian × tipe; penutup lebih sederhana.

Rincian pertimbangan ada di [RFC-0019](../RFC/RFC-0019-command-buffer.md).
