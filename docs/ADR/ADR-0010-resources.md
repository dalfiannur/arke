# ADR-0010: Resources sebagai parameter sistem

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-28
- **RFC terkait:** [RFC-0010](../RFC/RFC-0010-resources.md)

## Konteks

Logika sering butuh state global (waktu, konfigurasi, skor) yang bukan per-entity. ARCHITECTURE_BIBLE §3 sudah menyebut "resource" sebagai bagian model. Sistem perlu mengaksesnya secara terdeklarasi agar scheduler menalar konfliknya. Model `Res<T>`/`ResMut<T>` variadik penuh menuntut mesin SystemParam besar (lifetime GAT/HRTB).

## Keputusan

Kami memilih:

1. **Penyimpanan resource singleton-per-tipe** di `World` (`insert/resource/resource_mut/remove/contains_resource`), disimpan sebagai `HashMap<TypeId, Box<dyn Any + Send>>`.
2. **`Access` diperluas dengan namespace resource terpisah** dari komponen (agar `TypeId` sama tak salah-konflik); konflik dinilai per-namespace.
3. **Konstruktor sistem bertipe** `System::resource::<R>` (tulis R) dan `System::each_res::<R, Q>` (baca R + iterasi Q), dengan akses tersimpul.
4. **`each_res` aman via take/put-back**: `remove_resource` sementara, iterasi, `insert_resource` kembali — tanpa `unsafe`, tanpa peminjaman ganda `World`.

## Konsekuensi

**Positif:**

- Resources menjadi parameter sistem bertipe untuk pola umum (resource-saja & baca-resource-saat-iterasi).
- Scheduler menalar konflik resource → determinisme & kesiapan paralel terjaga.
- Tetap tanpa `unsafe` & tanpa dependensi eksternal.

**Negatif / biaya:**

- Konstruktor bertambah (`resource`, `each_res`) — ad-hoc dibanding SystemParam terpadu.
- `each_res` mengambil resource keluar sementara; panik saat iterasi membuatnya tak dikembalikan (didokumentasikan).
- Belum ada `Res`/`ResMut` variadik.

**Netral / catatan:**

- `each_res` versi mutasi-resource, SystemParam variadik penuh, dan serialisasi resource adalah pekerjaan berikutnya.

## Alternatif yang ditolak

- **`Res`/`ResMut` variadik penuh** — mesin generik besar; ditunda.
- **Resource sebagai komponen entity-tunggal** — anti-pola.
- **`each_res` via `unsafe` split** — `unsafe` tak terverifikasi (miri absen); take/put-back cukup.

Rincian pertimbangan ada di [RFC-0010](../RFC/RFC-0010-resources.md).
