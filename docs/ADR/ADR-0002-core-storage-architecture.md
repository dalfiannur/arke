# ADR-0002: Arsitektur penyimpanan inti — archetype + generational entity

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-27
- **RFC terkait:** [RFC-0002](../RFC/RFC-0002-core-storage-architecture.md)

## Konteks

`Rust ECS` membutuhkan fondasi penyimpanan (MILESTONE_1) yang secara bersamaan menegakkan invarian *ergonomis = cepat*, *determinisme by construction*, dan *struktural aman* ([ARCHITECTURE_BIBLE](../ARCHITECTURE_BIBLE.md) §2), sambil tetap standalone. Beberapa model bersaing: penyimpanan archetype vs sparse-set vs hybrid; kolom bertipe vs blob byte; registrasi komponen eksplisit vs otomatis; layout entity id. Pilihan ini mengikat lapisan-lapisan di atasnya (query, scheduler, snapshot), jadi perlu direkam permanen.

## Keputusan

Kami memilih:

1. **Penyimpanan archetype-only** — entity dengan himpunan komponen sama berbagi satu tabel; komponen disusun sebagai kolom kontigu.
2. **Kolom bertipe yang di-*type-erase*** — tiap kolom adalah `Vec<T>` di balik trait object `Column`; query men-*downcast* **sekali per archetype** menjadi `&[T]`/`&mut [T]`, bukan per elemen.
3. **`Entity` = `u32` indeks + `u32` generasi** dengan free-list LIFO; akses divalidasi terhadap generasi slot.
4. **Registrasi komponen otomatis** saat insert pertama; `ComponentId` bersifat internal-proses, sedangkan serialisasi memakai nama tipe yang stabil.

Kode pengguna tidak pernah menuntut `unsafe`; `unsafe` internal dikurung di lapisan storage, disertai argumen keamanan dan tes.

## Konsekuensi

**Positif:**

- Iterasi query cache-friendly atas slice bertipe → jalur ergonomis = jalur cepat (STD-0004).
- Semua urutan (alokasi entity, id komponen, id archetype, iterasi row) deterministik (STD-0005).
- Referensi entity basi selalu terdeteksi via generasi (STD-0007).
- Tanpa dependensi eksternal; core tetap standalone (STD-0003).
- Serialisasi by-name menyiapkan snapshot portabel & deterministik (STD-0001/0002).

**Negatif / biaya:**

- Perubahan struktural memindahkan seluruh row antar-archetype (biaya salin) — harga khas desain archetype.
- `swap_remove` membuat urutan row pasca-despawn menyimpang dari urutan insert (tetap deterministik).
- Downcast per-archetype menambah sedikit overhead per archetype (teramortisasi, bukan per elemen).
- Ada `unsafe` internal untuk disjoint borrow kolom pada query tuple — harus dijaga kesahihannya.

**Netral / catatan:**

- Model paralelisme query dan caching edge graf archetype **ditunda** (M-2 dan seterusnya).
- Menambah storage sparse-set/hybrid di kemudian hari harus lewat RFC baru; abstraksi `Column`/storage sengaja menyisakan ruang untuk itu.
- Komponen wajib `'static + Send` untuk menyiapkan paralelisme M-2 tanpa mengubah API.

## Alternatif yang ditolak

- **Sparse-set-only** — iterasi multi-komponen kurang cache-friendly; bertabrakan dengan invarian performa.
- **Hybrid archetype + sparse-set** — terlalu kompleks untuk fondasi pertama; ditunda.
- **Kolom blob byte mentah** — menuntut banyak `unsafe` internal tanpa manfaat cukup untuk M-1.
- **Registrasi komponen eksplisit** — boilerplate yang menggerus ergonomi.
- **Entity id `u32` dikemas** — batas jumlah entity & pemakaian-ulang terlalu ketat.

Rincian pertimbangan ada di [RFC-0002](../RFC/RFC-0002-core-storage-architecture.md).
