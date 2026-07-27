# Architecture Bible

> Sumber kebenaran konseptual untuk arsitektur `Rust ECS`. Ia menetapkan apa yang harus tetap benar ketika produk, teknologi, dan antarmuka berkembang. Dokumen ini berubah lambat dan dilindungi oleh proses RFC/ADR.

## 1. Purpose

`Rust ECS` adalah pustaka Entity-Component-System standalone: sebuah penyimpanan data berorientasi-data yang menyimpan entitas beserta komponennya dalam layout cache-friendly, dan mengeksekusi sistem terhadapnya secara deterministik.

Arsitekturnya harus memungkinkan **developer Rust** untuk:

1. memodelkan objek heterogen sebagai komposisi komponen, tanpa hierarki pewarisan;
2. mengueri dan memutasi komponen dengan iterasi cepat, tanpa menulis `unsafe`;
3. mereproduksi keadaan dunia secara bit-identik lintas mesin dan lintas strategi paralelisme.

## 2. Architectural invariants

Prinsip berikut **tidak boleh dikorbankan** demi kemudahan implementasi jangka pendek. Setiap baris memasangkan invarian dengan konsekuensi arsitekturalnya yang dapat diperiksa.

| Invarian | Konsekuensi arsitektural |
| --- | --- |
| **Ergonomis = cepat** | API aman ter-*compile* ke jalur panas optimal via monomorfisasi; tidak ada boxing, alokasi tersembunyi, atau `dyn` di jalur iterasi query. Jalur cepat tidak pernah menuntut `unsafe` dari sisi pengguna. |
| **Determinisme by construction** | Alokasi entity id, urutan iterasi query, dan resolusi konflik sistem sepenuhnya ditentukan oleh keadaan + jadwal, bukan oleh timing thread atau alamat memori. Run yang sama → keadaan akhir yang sama. |
| **Paralelisme yang aman** | Dua sistem berjalan konkuren **hanya** bila akses datanya terbukti tidak konflik (borrow-set disjoint). Penjadwal menserialisasi yang konflik; hasilnya setara dengan eksekusi serial. |
| **Kepemilikan & portabilitas data** | Seluruh keadaan `World` dapat di-snapshot dan diserialisasi ke format terbuka, lalu direkonstruksi tanpa kehilangan makna. Data pengguna tidak pernah tersandera format internal. |
| **Standalone core** | Inti pustaka tidak mengimpor game engine, renderer, runtime async, atau I/O. Integrasi eksternal berada di balik adapter/crate terpisah. |
| **Struktural aman** | Perubahan struktural (tambah/hapus komponen, spawn/despawn) tidak pernah membiarkan referensi entity basi dianggap valid — dijaga oleh generational index. |

> Isi tabel ini adalah keputusan terpenting dalam repo. Perubahan pada salah satu barisnya memerlukan RFC + ADR.

## 3. Canonical system model

Urutan ketergantungan **makna** (bukan diagram panggilan runtime): setiap lapisan di bawah melayani maksud lapisan di atasnya.

```text
Sistem pengguna (System)          — logika: fungsi atas Query & Resource
        ↓
Penjadwal (Scheduler)             — urutan deterministik + paralelisme aman
        ↓
Kueri (Query)                     — akses berpola & terverifikasi atas komponen
        ↓
Dunia (World)                     — otoritas atas entity, komponen, resource
        ↓
Penyimpanan arketipe (Archetype)  — kolom komponen kontigu (SoA)
        ↓
Snapshot / serialisasi            — artefak portabel keadaan dunia
```

### 3.1 System

Unit logika pengguna. Sistem menyatakan kebutuhan datanya lewat tipe `Query`/`Resource` yang diterimanya, dan tidak mengakses penyimpanan secara langsung. Karena kebutuhan datanya deklaratif, penjadwal dapat menalar konflik antar-sistem tanpa menjalankannya.

### 3.2 Scheduler

Menetapkan urutan eksekusi sistem yang deterministik, dan menjalankan sistem yang saling tidak-konflik secara paralel. Penjadwal adalah penjaga invarian *paralelisme yang aman* dan *determinisme*: ia tidak pernah membiarkan dua akses konflik berjalan bersamaan.

### 3.3 Query

Permukaan akses yang aman dan terverifikasi atas komponen (`&T`, `&mut T`, dan komposisinya). Query menegakkan aturan borrow — tidak boleh ada dua `&mut` yang beralias ke komponen yang sama — dan menjadi jalur panas utama yang wajib mematuhi invarian *ergonomis = cepat*.

### 3.4 World

Pemilik tunggal semua entity, komponen, dan resource; satu-satunya sumber kebenaran keadaan. Segala mutasi bermuara ke `World`. Karena kepemilikan tunggal, snapshot atas `World` cukup untuk merekam seluruh keadaan.

### 3.5 Archetype

Entitas yang memiliki set komponen sama disimpan bersama; setiap komponen disusun sebagai kolom yang kontigu (struktur-of-array) untuk iterasi cache-friendly. Ini adalah mekanisme utama yang membuat jalur ergonomis juga menjadi jalur cepat.

### 3.6 Snapshot / serialisasi

Proyeksi keadaan `World` ke format terbuka dan self-describing. Mendukung save, replay, rollback, dan pengujian, serta menegakkan invarian *kepemilikan & portabilitas data*.

## 4. Data and provenance

Setiap objek inti memiliki, sejauh relevan:

| Elemen | Wujud di `Rust ECS` |
| --- | --- |
| Stable ID | `Entity` sebagai generational index (indeks + generasi) — referensi tetap valid, atau terdeteksi basi, meski slot indeksnya dipakai ulang. |
| Ownership & scope | Setiap entity dan komponen dimiliki tepat satu `World`. |
| Timestamps & version | Tick dunia / nomor generasi menandai kapan sebuah mutasi terjadi. |
| Relations | Relasi antar-entity bersifat eksplisit (mis. komponen relasi), bukan pointer tersembunyi. |
| Provenance | Perubahan struktural dapat ditelusuri ke sistem/command yang menghasilkannya — menopang determinisme dan kemudahan debug. |
| Portability | Snapshot terbuka dan self-describing; dapat di-*parse* pustaka standar tanpa runtime `Rust ECS`. |

## 5. Product boundaries

`Rust ECS` **bukan**:

- game engine, renderer, atau loop jendela;
- runtime async atau penjadwal task tujuan-umum;
- basis data persisten atau ORM.

Sistem boleh terhubung dengan hal-hal tersebut lewat adapter bila berguna, tetapi tidak mengadopsinya sebagai sifat inti.

## 6. Evolution rules

1. Format snapshot dan API publik diberi versi serta dimigrasikan secara eksplisit.
2. Integrasi engine/renderer/async berada di crate atau adapter pinggir, bukan di core.
3. Fitur baru memulai sebagai modul opsional (feature flag) sebelum menjadi inti.
4. Setiap sumber non-determinisme (paralelisme, hashing, alokasi) wajib memiliki mode atau pengujian yang membuktikan kesetaraan hasil.
5. Keputusan yang memengaruhi determinisme, portabilitas data, atau invarian "ergonomis = cepat" memerlukan tinjauan lebih tinggi (RFC + ADR).
6. Kompleksitas baru harus membuktikan peningkatan nilai yang terukur (benchmark atau ergonomi), bukan sekadar kemampuan teknis.

## 7. Architecture decision test

Sebelum sebuah keputusan arsitektural diterima, jawab:

1. Apakah jalur pengguna yang paling natural tetap ter-*compile* ke jalur cepat tanpa `unsafe`?
2. Apakah hasilnya tetap deterministik lintas jumlah thread dan urutan penjadwalan?
3. Apakah paralelisme apa pun terbukti setara dengan eksekusi serial?
4. Apakah keadaan yang terpengaruh masih dapat di-snapshot dan dipulihkan secara penuh?
5. Apakah desain ini masih berguna tanpa engine atau runtime eksternal apa pun?

Jika jawabannya tidak jelas, keputusan belum siap.

---

Dokumen ini akan melahirkan keputusan teknis, skema data, dan kontrak API. Perubahan tersebut harus **memperkuat** invarian di atas — bukan mengubahnya secara diam-diam. Perubahan invarian memerlukan RFC + ADR.
