# ADR-0035: Adapter MongoDB dokumen-per-entity

- **Status:** Accepted
- **Tanggal:** 2026-08-01
- **RFC terkait:** [RFC-0035](../RFC/RFC-0035-arke-mongo-adapter.md)

## Konteks

Model working-set RFC-0021 tak terikat pada Postgres — yang terikat hanyalah
pemetaannya. Sebagian pengguna sudah menjalankan MongoDB dan tak ingin menambah
Postgres hanya untuk mempersist ECS. Model dokumen juga cocok secara alami
dengan bentuk data ECS: entity = kumpulan komponen heterogen opsional.

## Keputusan

1. **Crate `arke-mongo`** terpisah (gerbang dependensi; core `arke` tetap 0-dep,
   STD-0003).
2. **Satu dokumen per entity**, komponen sebagai sub-dokumen di bawah `cmp` —
   bukan satu koleksi per komponen. Fetch/update entity jadi satu round-trip dan
   atomik tanpa transaksi; query lintas-komponen tanpa `$lookup`.
3. **`pid` = `ObjectId`**, dialokasikan klien → `create` satu round-trip, aman
   multi-replica tanpa titik kontensi. Berbeda dari `pid` `i64` `arke-postgres`;
   keduanya sumber kebenaran alternatif, bukan dua muka dari satu penyimpanan.
4. **Tanpa crate derive.** `#[derive(arke::Serialize)]` sudah menghasilkan pohon
   `Value` yang dibutuhkan BSON; hanya trait tipis `MongoComponent` (nama +
   indeks) yang ditambahkan, diisi lewat `macro_rules!` (`mongo_component!`).
5. **`save` tanpa transaksi, sebagai operasi per-dokumen berurutan** (Amandemen
   1) — bukan `bulkWrite` seperti naskah RFC asli: `Collection::bulk_write`
   driver Rust menuntut MongoDB server 8.0, kontradiktif dengan target `mongod`
   7 standalone RFC ini sendiri. Jaminan yang dijanjikan tak berubah — atomik
   per-entity, bukan per-World; hanya mekanismenya yang berubah, demi
   portabilitas server.
6. **`NAME` yang bertabrakan antar-komponen → `panic!` saat `register`**
   (Amandemen 1). `register` bukan jalur yang bisa gagal karena data; dua
   komponen dengan `NAME` sama adalah bug programmer, ditangkap sedini
   mungkin, sejajar pola `PgStore::register`.
7. **`NAME` divalidasi sebagai nama field BSON saat `register`** (Amandemen
   2) — kosong, mengandung `.`, atau berawalan `$` → `panic!`. Review kode
   menemukan bahwa `NAME` beririsan dua peran berbeda: kunci literal di
   `cmp_doc` dan segmen path bertitik (`cmp.{NAME}`) di `update_ops`; untuk
   `NAME` mengandung `.` kedua jalur menyasar tempat berbeda di dokumen, dan
   setiap `update` hilang diam-diam tanpa error di mana pun.
8. **`MongoError::DuplicateField`** (Amandemen 2) — `Value::Map` mengizinkan
   kunci duplikat tapi `Document::insert` mendeduplikasi diam-diam (yang
   disisip terakhir menang). Dua field yang memetakan ke nama BSON sama (mis.
   dua `#[arke(rename)]` bertabrakan) sekarang menghasilkan error eksplisit,
   bukan kehilangan data diam-diam yang melanggar STD-0002.
9. **`connect` melakukan `ping` eagerly** sebelum mengembalikan `Ok`, menyamai
   `PgStore::connect` — `uri` ke host tak terjangkau gagal fail-fast saat
   `connect`, bukan diam-diam `Ok` lalu timeout buram di operasi pertama.
10. **Tanpa `drop_database` atau metode destruktif lain di API publik.**
    Metode semacam itu tak punya tempat di permukaan crate yang
    dipublikasikan; tes integrasi memakai klien driver `mongodb` mentah untuk
    setup/teardown.
11. **`update` yang menyasar dokumen yang sudah hilang → `MongoError::Missing`**,
    dibedakan dari `Conflict` (versi bergeser) — bukan `Ok(())` yang diam-diam
    membuang tulisan saat penulis lain sudah menghapus dokumennya.
12. **`save` menulis per sub-field** (`$set: {"cmp.x": …}`), tak pernah mengganti
    `cmp` utuh — komponen milik penulis lain tak boleh lenyap.
13. **Decode gagal → error**, tak pernah dilewati diam-diam.
14. **Satu `MongoStore` per satu `World`, tak dijaga di runtime.**
    `pid_of`/`entity_of` mengunci `Entity` — handle yang hanya bermakna di
    dalam satu `World` — ke `pid` persisten. Melanggar asumsi ini (mis.
    `fetch` ke `World` sekali-pakai, atau dua `World` independen yang
    men-spawn `Entity` identik) merusak data secara diam-diam. Tiga mode
    kegagalan didokumentasikan eksplisit di rustdoc `MongoStore` dan README,
    dipatok oleh tes regresi. Penjaga sungguhan (mis. `bind_world`) menuntut
    `WorldId` di **core `arke`** — perubahan di luar scope adapter ini;
    dicatat sebagai pertanyaan terbuka RFC-0035, bukan diselesaikan di sini.

## Konsekuensi

- **Positif**: MongoDB jadi sumber kebenaran ECS yang query-able; permukaan v1
  kecil sehingga API query belum terkunci; nol proc-macro baru untuk dipelihara;
  `mongod` standalone cukup untuk dev, tes, dan produksi sederhana.
- **Biaya**: `save` memberi jaminan lebih lemah daripada `arke-postgres`
  (didokumentasikan di rustdoc dan README, bukan hanya RFC/ADR); dua permukaan
  adapter untuk dijaga koheren; deklarasi indeks manual tanpa cek kompilasi;
  bahaya satu-store-satu-`World` tetap tanpa penjaga runtime sampai `WorldId`
  ada di core.
- **Netral**: evolusi skema belum dijawab — Mongo schemaless, strateginya beda
  secara fundamental dan pantas dapat RFC sendiri. Implementasi menyingkap bug
  pra-eksisting di core `arke` (`World::insert` atas komponen yang sudah
  dimiliki merusak archetype secara diam-diam di build rilis) — diperbaiki di
  core (`3c588ea`), dicatat di `CHANGELOG.md`, bukan di ADR ini karena bukan
  keputusan arsitektural adapter ini.

## Alternatif ditolak

- **Satu koleksi per tipe komponen** — memakai Mongo seperti "Postgres yang
  lebih lemah"; `$lookup` untuk tiap query lintas-komponen.
- **`i64` via koleksi counter** — menukar skalabilitas tulis demi keseragaman
  tipe `pid` yang kosmetik.
- **Transaksi multi-dokumen** — menuntut replica set untuk dev, tes, dan
  produksi; beban setup tak sepadan untuk v1 fondasi.
- **`bulkWrite` ordered untuk `save`** — naskah RFC asli sebelum Amandemen 1;
  ditolak karena `Collection::bulk_write` driver Rust menuntut MongoDB server
  8.0, kontradiktif dengan target `mongod` 7 standalone.
- **`#[derive(MongoComponent)]`** — parser atribut kedua tanpa imbalan di v1.
- **Peringatan runtime (bukan panic) untuk `NAME` bertabrakan/tak sah** —
  ditolak: keduanya bug programmer di sebuah `const`, pantas gagal keras
  sedini mungkin saat `register`, bukan diam-diam terus berjalan dengan data
  yang berpotensi rusak.

Rincian di [RFC-0035](../RFC/RFC-0035-arke-mongo-adapter.md).
