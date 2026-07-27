# RFC-0003: Scheduler deterministik dengan analisis konflik

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-27
- **Milestone:** M-2 (Deterministic Scheduler)
- **ADR terkait:** [ADR-0003](../ADR/ADR-0003-deterministic-scheduler.md)

## Ringkasan

Memperkenalkan **System** — unit logika yang berjalan atas `World` dan menyatakan akses datanya secara **eksplisit** (`reads`/`writes` per tipe komponen) — dan **Schedule** yang menghitung urutan eksekusi **deterministik** beserta pengelompokan **stage** dari analisis konflik. Untuk M-2 eksekusi bersifat **serial** (stage demi stage, sistem dalam satu stage terbukti tak-konflik sehingga dapat diparalelkan). Eksekusi multi-thread nyata (mengaktifkan STD-0006) dan sistem berbasis-tipe ditunda ke M-3.

## Motivasi

[ARCHITECTURE_BIBLE](../ARCHITECTURE_BIBLE.md) §3.1–3.2 menetapkan lapisan **System** dan **Scheduler**: sistem menyatakan kebutuhan datanya, penjadwal menetapkan urutan deterministik dan hanya menjalankan sistem tak-konflik secara bersamaan. M-1 menyediakan `World` + query; kini dibutuhkan cara mengorganisasi logika menjadi sistem terjadwal.

Bagian tersulit secara korektness adalah **determinisme urutan** dan **analisis konflik baca/tulis** — bukan threading itu sendiri. M-2 menyelesaikan bagian ini lebih dulu; threading dibangun di atasnya (M-3) tanpa mengubah model.

## Usulan rinci

### 1. System

Sebuah sistem membungkus closure `FnMut(&mut World)` dan sebuah **deklarasi akses**:

```rust
pub struct System {
    run: Box<dyn FnMut(&mut World)>,
    access: Access,
}

impl System {
    pub fn new(run: impl FnMut(&mut World) + 'static) -> Self;
    pub fn reads<T: Component>(self) -> Self;   // menandai baca komponen T
    pub fn writes<T: Component>(self) -> Self;   // menandai tulis komponen T
}
```

Akses direkam sebagai himpunan `TypeId` (bukan `ComponentId`) agar tak bergantung pada registry saat schedule dibangun:

```rust
struct Access { reads: Vec<TypeId>, writes: Vec<TypeId> }
```

> Deklarasi eksplisit dipilih untuk M-2 karena sederhana dan cukup untuk analisis konflik. Sistem berbasis-tipe (`fn(Query<&A>, Query<&mut B>)` yang menyimpulkan akses dari tipe parameter, sesuai §3.1) adalah evolusi M-3 dan akan **menggantikan**, bukan mengubah, model konflik ini.

### 2. Aturan konflik

Dua sistem **berkonflik** bila berbagi komponen di mana setidaknya satu menulisnya:

```text
conflict(a, b) ⇔ (a.writes ∩ b.writes)
              ∪ (a.writes ∩ b.reads)
              ∪ (a.reads  ∩ b.writes)  ≠ ∅
```

Baca-baca atas komponen sama **tidak** berkonflik.

### 3. Schedule & penetapan stage deterministik

```rust
pub struct Schedule { systems: Vec<System> }

impl Schedule {
    pub fn new() -> Self;
    pub fn add(&mut self, system: System) -> &mut Self;
    pub fn run(&mut self, world: &mut World);   // eksekusi stage demi stage (serial di M-2)
    pub fn stages(&self) -> Vec<Vec<usize>>;    // rencana paralel deterministik
}
```

Sistem dipertahankan dalam **urutan registrasi**. Setiap sistem `i` diberi **stage**:

```text
stage[i] = 0                              bila tak ada j < i yang berkonflik dengan i
stage[i] = 1 + max{ stage[j] : j < i, conflict(i, j) }   selain itu
```

Sifat yang dijamin penetapan ini:

- Bila `i` dan `k` (`i < k`) berkonflik, maka `stage[k] > stage[i]` → **sistem dalam stage yang sama pasti pairwise tak-konflik** (aman diparalelkan).
- Setiap sistem berada di stage **setelah** semua pendahulu yang berkonflik dengannya → eksekusi stage-demi-stage **setara** dengan eksekusi serial urutan registrasi.

`run()` mengeksekusi stage berurutan; di M-2 sistem dalam satu stage dijalankan serial (urutannya tak penting karena tak-konflik). Di M-3, sistem satu stage dijalankan paralel via `std::thread::scope` — hasilnya identik (STD-0006).

### 4. Determinisme

Urutan registrasi, penetapan stage (fungsi murni dari akses + urutan), dan eksekusi seluruhnya deterministik → `run()` menghasilkan keadaan yang sama untuk urutan sistem + keadaan awal yang sama (STD-0005).

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| Sistem berbasis-tipe (`SystemParam`) sekarang | Ergonomis, akses tersimpul otomatis, tanpa rework | Butuh mesin generik besar (QueryData variadic) | Ditunda M-3; deklarasi eksplisit cukup untuk analisis konflik M-2 |
| Eksekusi paralel langsung di M-2 | Milestone lebih lengkap | Threading + kemungkinan `unsafe` akses World disjoint; risiko soundness besar | Determinisme+konflik adalah korektness inti; paralel dibangun di atasnya (M-3) |
| Tanpa stage, hanya urutan registrasi | Paling sederhana | Tak ada rencana paralel; M-3 mulai dari nol | Stage adalah artefak analisis konflik yang menyiapkan paralelisme |
| Paralelisme via rayon | Work-stealing kuat | Dependensi eksternal | Melanggar standalone (STD-0003); `std::thread::scope` cukup |

## Dampak

- **Kompatibilitas / migrasi:** aditif; tak mengubah API M-1. Sistem berbasis-tipe M-3 akan menjadi lapisan di atas `System`.
- **Keamanan / izin / provenance:** analisis konflik menegakkan disiplin akses yang menjadi dasar keamanan paralelisme (invarian *paralelisme yang aman*).
- **Konsekuensi pada invarian:** memperkuat *determinisme by construction* (urutan & stage deterministik) dan menyiapkan *paralelisme yang aman* (stage tak-konflik). STD-0006 belum aktif hingga eksekusi paralel M-3.

## Pertanyaan terbuka

- Deklarasi akses eksplisit tidak diverifikasi terhadap akses aktual sistem; sistem berbasis-tipe M-3 akan menutup celah ini. Sampai saat itu, deklarasi yang salah adalah bug pengguna. → catat sebagai RN bila perlu.
- Kebijakan untuk sistem yang mengakses `World` secara struktural (spawn/despawn di tengah schedule) terhadap paralelisme M-3. → RN saat M-3.

## Keputusan

Diterima. Lihat [ADR-0003](../ADR/ADR-0003-deterministic-scheduler.md).
