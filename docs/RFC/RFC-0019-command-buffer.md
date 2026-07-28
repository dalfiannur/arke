# RFC-0019: Command buffer (mutasi struktural tertunda)

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-28
- **Milestone:** M-18 (Command Buffer)
- **ADR terkait:** [ADR-0019](../ADR/ADR-0019-command-buffer.md)

## Ringkasan

Menambahkan **`CommandBuffer`**: perekam **mutasi struktural tertunda** (spawn / despawn / insert / remove) yang di-*apply* nanti saat `&mut World` tersedia. Sistem paralel-mampu (`&World`) kini dapat **merekam** perubahan struktural via `System::each_cmd` tanpa memerlukan akses eksklusif. Buffer di-apply **di akhir run**, **urutan registrasi** → deterministik, sound, dan menjaga `run` ≡ `run_parallel` (STD-0006).

## Motivasi

Mutasi struktural (`spawn`/`despawn`/`insert`/`remove`) butuh `&mut World` — tak bisa dilakukan sistem paralel (`&World`) maupun saat iterasi query berlangsung. Akibatnya, setiap kebutuhan spawn/despawn memaksa sistem `Exclusive` (serial), memotong paralelisme (segmentasi RFC-0018). Command buffer memindahkan mutasi ke **titik-apply** tertunda: sistem paralel merekam, world memutakhirkan diri belakangan. Lebih sedikit sistem `Exclusive` → lebih banyak paralelisme.

## Usulan rinci

### 1. Primitif `CommandBuffer`

```rust
let mut cmd = CommandBuffer::new();
let _ = cmd.spawn().insert(Position(0)).insert(Velocity(1)); // spawn + konfigurasi
cmd.despawn(old);
cmd.insert(existing, Health(100));
cmd.remove::<Tag>(existing);
cmd.apply(&mut world); // eksekusi berurutan-rekam, lalu buffer kosong lagi
```

- Internal: `Vec<Command>` di mana `Command` = `Spawn(Vec<konfigurator>)` atau `Op(Box<dyn FnOnce(&mut World) + Send>)`.
- `spawn()` mengembalikan **`EntityCommands`** (builder): `.insert(c)` menambah konfigurator `FnOnce(&mut World, Entity)`. Saat apply: `let e = world.spawn(); for cfg in configs { cfg(world, e) }` — jadi konfigurator melihat **`Entity` nyata** hasil spawn.
- `despawn`/`insert`/`remove` merekam `Op` penutup (`move |w| w.despawn(e)`, dst).
- `apply(&mut World)` menguras (`drain`) buffer **urutan-rekam** → deterministik (STD-0005); buffer dapat dipakai ulang.
- `Send`: semua penutup + komponen `Send` (Component: `'static + Send`) → `CommandBuffer: Send` (dibutuhkan model thread-per-sistem RFC-0018).

### 2. Integrasi scheduler: `System::each_cmd`

```rust
s.add(System::each_cmd::<&Health>(|h, cmd| {
    if h.0 <= 0 { /* cmd.spawn()… / cmd.despawn(…) */ }
}));
```

- `Runner` bertambah varian **`SharedCmd(FnMut(&World, &mut CommandBuffer))`** — **paralel-mampu** (seperti `Shared`).
- Tiap `System` memiliki `CommandBuffer` sendiri. Saat run berbagi, sistem merekam ke buffer-nya (thread pemilik `&mut System` → `&mut buffer`; **tanpa sinkronisasi**, tanpa berbagi state).
- Akses konflik memakai `Q::access()` (sisi baca query) — sama seperti sistem `Shared`.

### 3. Titik-apply: **akhir run**, urutan registrasi

Setelah **seluruh** sistem selesai (`run` maupun `run_parallel`), scheduler meng-apply buffer tiap sistem, **urutan indeks registrasi**, dengan `&mut World`.

**Kenapa akhir-run (bukan segmen/antar-sistem):**

- **Soundness tanpa celah**: efek command (mis. `insert(Health)`) **tidak** tercermin di `Access` sistem, jadi analisis konflik tak "tahu" tentangnya. Menunda ke akhir-run membuat efek **tak terlihat** selama run → tak ada balapan (mutasi struktural terjadi belakangan, `&mut World` eksklusif).
- **`run` ≡ `run_parallel` (STD-0006)**: bila **keduanya** apply di akhir-run urutan-registrasi, hasil identik. Sistem efek-langsung (`Exclusive`) tetap berjalan di tempatnya; hanya command yang tertunda.
- **Determinisme**: urutan-registrasi → satu urutan apply yang pasti.

Semantiknya: **command tampak setelah run selesai** (mis. frame berikutnya) — pilihan yang lazim & sederhana.

### 4. Contoh soundness

Sistem paralel A merekam `spawn`; sistem paralel B (segmen sama) mengiterasi query. Selama run, **tak ada** archetype berubah (A hanya menulis buffer privat). Setelah scope join, buffer di-apply serial → aman. Tak ada mutasi struktural bersamaan dengan iterasi.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| Apply per-segmen (sebelum tiap `Exclusive`) | Command lebih cepat terlihat | Celah `Access` tetap; `run` harus meniru batas segmen | Akhir-run lebih sederhana & sama-sound |
| Reservasi entity atomik (spawn handle sinkron) | `Entity` nyata saat rekam | Alokator entity atomik dari `&World`; kompleks | Konfigurator `FnOnce(&mut World, Entity)` cukup; reservasi = lanjutan |
| Buffer global bersama (satu untuk semua sistem) | Satu titik apply | Butuh sinkronisasi lintas-thread | Per-sistem buffer bebas-kunci (thread-per-sistem) |
| Command sebagai `enum` bertipe (bukan penutup) | Introspektif; serializable | Matriks varian × tipe komponen; type-erasure rumit | Penutup `FnOnce` sederhana & cukup |

## Dampak

- **Kompatibilitas / migrasi:** aditif. `CommandBuffer` baru; `System::each_cmd` baru; `run`/`run_parallel` kini meng-apply buffer di akhir (no-op bila tak ada cmd-system). API lama tak berubah.
- **Keamanan:** **tanpa `unsafe` baru** — buffer & apply 100% aman; perekaman paralel memakai `&mut System` milik thread (model RFC-0018).
- **Konsekuensi pada invarian:** mengurangi kebutuhan sistem `Exclusive` → lebih banyak paralelisme (memperkuat STD-0006). Determinisme dijaga (urutan-registrasi).

## Pertanyaan terbuka

- **`Entity` sebagai term query** — agar `each_cmd` bisa `despawn`/`insert` entity yang sedang diiterasi (pola "despawn-self"). Follow-up (membuka nilai penuh command buffer).
- Reservasi entity atomik untuk handle spawn sinkron.
- Apply per-*sync-point* eksplisit (bila dibutuhkan visibilitas intra-run).

## Keputusan

Diterima. Lihat [ADR-0019](../ADR/ADR-0019-command-buffer.md).
