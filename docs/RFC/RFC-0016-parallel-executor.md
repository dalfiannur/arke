# RFC-0016: Eksekutor paralel tingkat-sistem

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-28
- **Milestone:** M-15 (Parallel Executor)
- **ADR terkait:** [ADR-0016](../ADR/ADR-0016-parallel-executor.md)

## Ringkasan

Menambahkan **`Schedule::run_parallel`** yang menjalankan sistem-sistem satu *stage* (yang aksesnya dijamin disjoint oleh analisis konflik M-2/M-4) di beberapa thread via `std::thread::scope` — mengaktifkan **STD-0006 tingkat-sistem** (paralel = serial). Ini `unsafe` terkurung terbesar proyek, dibangun di atas storage `UnsafeCell` (RFC-0015) dan **diverifikasi miri** di CI.

## Motivasi

Tujuan akhir M-5. M-14 menyediakan storage `UnsafeCell` sehingga `&mut` kolom dapat dibentuk dari `&World` bersama. Kini eksekutor menjalankan sistem disjoint secara paralel.

## Usulan rinci

### 1. Jalur query berbagi (`&World`)

`QueryData` diseragamkan ke jalur **berbagi**:

```rust
fn each_filtered_shared<F: QueryFilter>(world: &World, f: ...);  // implementasi
fn each_filtered<F>(world: &mut World, f) { Self::each_filtered_shared::<F>(&*world, f) }  // serial: eksklusif
```

Tiap term mengakses kolomnya lewat sel: `&T` → `data()` (baca), `&mut T` → `unsafe data_mut_shared()` (tulis). Kolom-kolom **distinct** (dijamin cek-alias) → tak beralias. Tanpa `get_disjoint_mut` (akses per-sel independen).

### 2. Sistem paralel-mampu

`System` membedakan runner:

- **Exclusive** (`FnMut(&mut World)`): sistem opaque (`System::new`) & resource — **serial-saja**.
- **Shared** (`FnMut(&World) + Send`): sistem bertipe (`System::each`/`each_filtered`) — **paralel-mampu**.

### 3. Pembungkus `Sync` & eksekutor

```rust
struct SyncWorld<'a>(&'a World);
// SAFETY: sistem satu stage mengakses komponen disjoint (analisis stage).
unsafe impl Sync for SyncWorld<'_> {}
```

`run_parallel`: untuk tiap stage yang **seluruh** sistemnya `Shared`, jalankan di `std::thread::scope` (tiap thread memegang `&mut System` disjoint + berbagi `&World` lewat `SyncWorld`). Stage yang memuat sistem `Exclusive` jatuh ke **serial**.

### 4. Konfinemen `unsafe`

`unsafe` kini di tiga modul, masing-masing `#[allow(unsafe_code)]` + `// SAFETY` + diverifikasi miri: `storage` (`data_mut_shared`), `query` (`iter_shared` untuk `&mut`), `schedule` (`unsafe impl Sync`). Jalur pengguna **tetap** tanpa `unsafe` (STD-0004).

### 5. Verifikasi

Job miri CI menjalankan uji **paralel = serial** tingkat-sistem; wajib hijau.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| `run` otomatis paralel | Ergonomis | Perilaku implisit; `unsafe` tak eksplisit | `run_parallel` eksplisit (ADR-0003) |
| Paralelkan sistem resource juga | Lebih lengkap | Resource butuh interior-mut tambahan | Sistem resource serial dulu |
| Thread pool | Hindari spawn berulang | Kompleksitas/state | `scope` cukup; pool optimasi kemudian |

## Dampak

- **Kompatibilitas / migrasi:** aditif. `run` (serial) tak berubah perilaku; jalur query kini berbagi (hasil identik).
- **Keamanan:** `unsafe` terkurung di 3 modul, miri-verified. STD-0004 tetap.
- **Konsekuensi pada invarian:** mengaktifkan STD-0006 tingkat-sistem; *paralelisme yang aman* terwujud.

## Pertanyaan terbuka

- Sistem resource paralel; thread pool; command buffer (spawn/despawn saat paralel) → lanjutan.

## Keputusan

Diterima. Lihat [ADR-0016](../ADR/ADR-0016-parallel-executor.md).
