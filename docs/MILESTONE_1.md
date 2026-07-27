# Milestone 1 — Core Storage & Query Minimal

> Satu milestone = satu potongan ruang lingkup dengan kriteria selesai yang tak ambigu. Disalin dari [MILESTONE_TEMPLATE.md](MILESTONE_TEMPLATE.md).

## Tujuan

Meletakkan fondasi penyimpanan berorientasi-data yang menjadi tumpuan semua hal berikutnya: sebuah `World` yang bisa membuat/menghapus entity, menempelkan komponen, menyimpannya dalam archetype kolumnar, dan mengiterasinya lewat query yang aman. Milestone ini membuktikan sejak fondasi bahwa jalur ergonomis dapat sekaligus menjadi jalur cepat.

## Ruang lingkup

**Termasuk:**

- `World`: pembuatan, serta kepemilikan entity dan komponen.
- `Entity`: generational index — spawn/despawn dengan pemakaian ulang slot yang aman.
- Komponen: registrasi tipe, serta insert/remove pada sebuah entity.
- Penyimpanan **archetype** kolumnar (struktur-of-array) — entity dengan set komponen sama disimpan bersama.
- **Query** dasar: `Query<&T>`, `Query<&mut T>`, dan tuple (mis. `Query<(&A, &mut B)>`) beserta iterasinya.
- Aturan borrow query yang menegakkan akses aman (menolak alias `&mut` ke komponen yang sama).

**Tidak termasuk (sengaja ditunda):**

- Scheduler dan paralelisme (milestone berikutnya) — karena itu STD-0006 belum aktif di sini.
- Serialisasi/snapshot dan format terbuka (STD-0001/0002 aktif pada milestone snapshot).
- Resources (singleton global), command buffer (mutasi struktural tertunda), dan relasi antar-entity.
- Filter query lanjutan (`With`/`Without`/`Changed`).

## Artefak yang dihasilkan

| Artefak | Bentuk |
| --- | --- |
| RFC-0002 | Proposal arsitektur core: layout archetype + model generational entity |
| ADR-0002 | Keputusan yang diterima dari RFC-0002 |
| Kode + tes | Crate core: `World`, `Entity`, penyimpanan archetype, query + 17 tes |
| Contoh no-unsafe | [`examples/no_unsafe.rs`](../examples/no_unsafe.rs) di bawah `forbid(unsafe_code)` (bukti STD-0004) |

## Kriteria selesai (Definition of Done)

Milestone dianggap **selesai** ketika semua benar:

- [x] `World` dapat spawn/despawn entity serta insert/remove komponen; slot dipakai ulang dengan aman.
- [x] Handle `Entity` yang basi terdeteksi (bukti STD-0007) — disertai tes.
- [x] Query `&T`/`&mut T`/tuple mengiterasi hanya entity yang cocok; alias `&mut` ditolak. *(API M-1: `World::query` / `query_mut` / `query_pair`; trait `QueryData` generik ditunda ke milestone berikutnya.)*
- [x] Iterasi dan alokasi bersifat deterministik (bukti STD-0005) — disertai tes lintas-run ([`tests/determinism.rs`](../tests/determinism.rs)).
- [x] Kode pengguna dapat memakai API di bawah `forbid(unsafe_code)` (bukti STD-0004). *Catatan: seluruh core M-1 ternyata **bebas `unsafe`** — `split_at_mut` cukup untuk peminjaman kolom disjoint, sehingga `unsafe` terkurung yang diantisipasi RFC-0002 §5 tidak diperlukan di M-1. Benchmark performa (criterion) ditunda.*
- [x] Core build dengan `--no-default-features` tanpa dependensi terlarang (bukti STD-0003).
- [x] RFC-0002 dan ADR-0002 ditulis serta konsisten dengan kode.
- [x] Semua tes hijau secara lokal (17 tes). *CI GitHub Actions terkonfigurasi; menunggu remote untuk dijalankan.*

## Ketergantungan

- **Butuh selesai lebih dulu:** RFC-0002 / ADR-0002 (keputusan arsitektur core).
- **Membuka jalan bagi:** M-2 (scheduler deterministik + paralelisme), lalu M-3 (snapshot/serialisasi).

## Pertanyaan terbuka

- ~~Strategi penyimpanan archetype: satu `Vec` per kolom vs blob mentah dengan offset?~~ → **diputuskan** di [RFC-0002](RFC/RFC-0002-core-storage-architecture.md): kolom bertipe `Vec<T>` yang di-*type-erase*, downcast per-archetype.
- ~~Registrasi komponen eksplisit vs otomatis saat insert pertama?~~ → **diputuskan** di [RFC-0002](RFC/RFC-0002-core-storage-architecture.md): otomatis saat insert pertama; serialisasi memakai nama tipe stabil.
- Caching *edge* graf archetype dan normalisasi urutan row pasca-despawn → ditunda; catat sebagai RN bila jadi soal (lihat Pertanyaan terbuka RFC-0002).
