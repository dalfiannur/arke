# RFC-0018: Eksekutor graf-ketergantungan

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-28
- **Milestone:** M-17 (Dependency-Graph Executor)
- **ADR terkait:** [ADR-0018](../ADR/ADR-0018-dependency-graph-executor.md)

## Ringkasan

Mengganti eksekutor paralel berbasis **stage** ([RFC-0016](RFC-0016-parallel-executor.md)) dengan eksekutor berbasis **graf-ketergantungan** (DAG). Alih-alih *barrier* penuh antar-stage, tiap sistem mulai **segera** setelah **pendahulu yang berkonflik dengannya** selesai — bukan menunggu seluruh stage sebelumnya. `Schedule::run_parallel` memakai DAG ini secara transparan: **API sama, hasil identik** (STD-0006, deterministik), **paralelisme lebih tinggi**. Tanpa `unsafe` baru (memakai kembali `SyncWorld` terkurung).

## Motivasi

Model stage (RFC-0003 §3) menempatkan sistem `i` di `stage = 1 + max(stage pendahulu berkonflik)`, lalu menjalankan stage **satu per satu dengan barrier penuh** di antaranya. Barrier itu terlalu kasar:

```
A: tulis X     B: tulis Y        (stage 0)
C: baca X                        (stage 1, hanya konflik dgn A)
```

Pada model stage, `C` menunggu **A dan B** (barrier stage 0→1) walau `C` hanya butuh `A`. Bila `B` lambat, `C` menganggur sia-sia. Graf-ketergantungan membiarkan `C` mulai **tepat setelah `A`** selesai, lepas dari `B`. *Critical path*-nya sama, tapi utilisasi thread naik — memperkuat **ergonomis = cepat**.

## Usulan rinci

### 1. Graf-ketergantungan

Untuk tiap pasangan `(j, i)` dengan `j < i`, tambahkan sisi `j → i` bila `access[i]` **berkonflik** dengan `access[j]` (analisis M-2/M-4 yang sama dengan stage). Karena sisi selalu mengikuti **urutan registrasi** (`j < i`), pasangan berkonflik tak pernah berjalan bersamaan dan yang lebih dulu-registrasi selalu selesai lebih dulu → **hasil identik dengan eksekusi serial** (STD-0006). Pasangan tak-berkonflik boleh berjalan bersamaan (akses disjoint → hasil sama).

Sisi transitif yang **redundan** (mis. `i` bergantung pada `j` dan `k`, padahal `j` juga bergantung `k`) dibiarkan — tak merugikan: pendahulu redundan (`k`) selalu selesai tak-lebih-lambat dari mediator (`j`), jadi tak menunda di luar *critical path*. Reduksi transitif adalah optimasi lanjutan, bukan syarat kebenaran.

`Schedule::dependencies() -> Vec<Vec<usize>>` mengekspos daftar **pendahulu** tiap sistem (introspeksi/uji), sejajar dengan `stages()` yang tetap ada.

### 2. Eksekutor (pustaka-std saja)

Worker-pool via `std::thread::scope`, koordinasi lewat `Mutex<GraphState>` + `Condvar`:

```rust
struct GraphState {
    ready: VecDeque<usize>, // sistem siap (pending == 0)
    pending: Vec<usize>,    // # pendahulu belum selesai
    remaining: usize,       // # sistem belum selesai
}
```

- Seed `ready` dengan sistem `pending == 0`.
- Tiap worker: ambil `idx` dari `ready` (tunggu di `Condvar` bila kosong & `remaining > 0`; keluar bila `remaining == 0`), jalankan, lalu kurangi `pending` tiap suksesor — yang mencapai `0` masuk `ready` — dan `notify_all`.
- Jumlah worker = `min(available_parallelism, jumlah sistem)`.

Akses eksklusif ke tiap sistem dari worker dinamis: bungkus `Vec<Mutex<&mut System>>` (tiap sistem di-*lock* **tepat sekali**, tanpa kontensi karena graf menjamin dispatch tunggal). **100% aman** — tanpa `unsafe` baru.

### 3. Sistem `Exclusive` (segmentasi)

Sistem `Exclusive` (resource/opaque) butuh `&mut World` → tak bisa berbagi `&World`. Schedule **disegmen** pada batas `Exclusive`: tiap *run* maksimal sistem `Shared` berturut dijalankan lewat DAG (berbagi `&World`), tiap `Exclusive` dijalankan **serial** sebagai barrier `&mut World`. Ini mempertahankan urutan registrasi menyeberang barrier (deterministik) **dan** memberi paralelisme-DAG penuh di dalam tiap segmen. Segmen satu-sistem jatuh ke `run` serial (tanpa spawn).

### 4. Konfinemen `unsafe`

Tak ada `unsafe` baru. Eksekutor DAG memakai kembali `unsafe impl Sync for SyncWorld` yang sudah ada (RFC-0016): sistem yang berjalan bersamaan dijamin **tak-konflik** oleh graf → akses kolom disjoint. Diverifikasi miri di CI (uji **DAG = serial**).

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| Metode `run_graph` baru (biarkan `run_parallel` stage) | Non-breaking eksplisit | Dua API paralel; pengguna harus memilih | DAG identik-hasil & lebih baik → jadikan jalur default (ergonomis = cepat) |
| Reduksi transitif graf | Sisi minimal | Kompleksitas; tak ubah *critical path*/kebenaran | Sisi redundan tak merugikan; optimasi lanjutan |
| `Exclusive` sebagai simpul DAG | Paralelisme antar-segmen | Butuh `&mut World` di tengah `scope` `&World` | Segmentasi lebih sederhana & sudah setara stage |
| Thread-pool persisten | Hindari spawn berulang | State pool; lintas-`run` | `scope` cukup; pool optimasi lanjutan |

## Dampak

- **Kompatibilitas / migrasi:** `run_parallel` tetap (API sama). Hasil & urutan efektif **identik** (STD-0006); hanya lebih paralel. `stages()` tetap; `dependencies()` ditambah. `run` serial tak berubah.
- **Keamanan:** tanpa `unsafe` baru; memakai kembali `SyncWorld` terkurung, miri-verified.
- **Konsekuensi pada invarian:** memperkuat **paralelisme aman** (STD-0006) — utilisasi lebih tinggi tanpa mengorbankan determinisme.

## Pertanyaan terbuka

- Reduksi transitif untuk graf minimal.
- `Exclusive` sebagai simpul (butuh interior-mutability World / command buffer).
- Thread-pool persisten lintas-`run`.

## Keputusan

Diterima. Lihat [ADR-0018](../ADR/ADR-0018-dependency-graph-executor.md).
