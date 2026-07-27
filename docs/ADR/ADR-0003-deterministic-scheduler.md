# ADR-0003: Scheduler deterministik dengan analisis konflik

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-27
- **RFC terkait:** [RFC-0003](../RFC/RFC-0003-deterministic-scheduler.md)

## Konteks

[ARCHITECTURE_BIBLE](../ARCHITECTURE_BIBLE.md) §3 menempatkan lapisan **System** dan **Scheduler** di atas `World`/query. Bagian tersulit secara korektness adalah determinisme urutan dan analisis konflik baca/tulis, bukan threading. Milestone M-2 harus menegakkan invarian *determinisme by construction* dan menyiapkan *paralelisme yang aman* tanpa menanggung risiko soundness dari eksekusi multi-thread sekaligus.

## Keputusan

Kami memilih:

1. **System = closure `FnMut(&mut World)` + deklarasi akses eksplisit** (`reads::<T>()`/`writes::<T>()`), direkam sebagai himpunan `TypeId`.
2. **Aturan konflik**: dua sistem berkonflik bila berbagi komponen yang ditulis salah satunya (write-write atau read-write); baca-baca tidak berkonflik.
3. **Penetapan stage deterministik**: `stage[i] = 0` bila tak ada pendahulu berkonflik, selain itu `1 + max stage pendahulu yang berkonflik`. Sistem satu stage dijamin pairwise tak-konflik dan aman diparalelkan.
4. **Eksekusi serial (stage demi stage) untuk M-2**; eksekusi paralel via `std::thread::scope` ditunda ke M-3 (mengaktifkan STD-0006), tanpa dependensi eksternal.

## Konsekuensi

**Positif:**

- Urutan & pengelompokan stage sepenuhnya deterministik (STD-0005).
- Analisis konflik menghasilkan rencana paralel yang benar; M-3 tinggal mengeksekusinya di thread.
- Tetap standalone: mekanisme paralel yang dituju `std::thread`, bukan crate eksternal (STD-0003).
- Aditif terhadap M-1; tak ada perubahan breaking.

**Negatif / biaya:**

- Deklarasi akses eksplisit bersifat manual dan tak diverifikasi terhadap akses aktual — deklarasi salah adalah bug pengguna sampai sistem berbasis-tipe M-3.
- `System` memakai `Box<dyn FnMut>` (dispatch dinamis tingkat-sistem — kasar, bukan per-entity, jadi tidak melanggar invarian jalur panas).
- M-2 belum memberi percepatan (serial); nilainya adalah korektness + rencana paralel.

**Netral / catatan:**

- STD-0006 (paralel setara serial) belum aktif hingga M-3.
- Sistem berbasis-tipe (`fn(Query<&A>, Query<&mut B>)`) akan **menggantikan** permukaan deklarasi eksplisit di M-3 tanpa mengubah model konflik/stage ini.

## Alternatif yang ditolak

- **SystemParam bertipe sekarang** — butuh mesin generik variadic besar; ditunda M-3.
- **Eksekusi paralel langsung di M-2** — risiko soundness/threading terlalu besar untuk digabung dengan analisis konflik.
- **rayon** — dependensi eksternal; melanggar standalone.

Rincian pertimbangan ada di [RFC-0003](../RFC/RFC-0003-deterministic-scheduler.md).
