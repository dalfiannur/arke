# RFC-0038: `arke-postgres` — upsert entity berkunci indeks unik

- **Status:** Accepted
- **Tanggal:** 2026-09-25
- **Milestone:** —
- **ADR terkait:** —

## Ringkasan

`PgStore::upsert::<T>(staged)` menyisipkan entity baru, atau — bila komponen
kunci `T` bentrok pada target konflik — memakai entity yang sudah ada (DO
NOTHING) atau memperbarui kolom tertentu (DO UPDATE, opsional `coalesce`).
Hasilnya `Upserted { pid, inserted }`. Varian `execute_in(&mut tx)` tersedia.

## Motivasi

Penyerapan event dari luar (webhook yang dapat terkirim ulang, impor yang
dapat diulang) harus idempoten dan aman di bawah konkurensi. Pola
cek-lalu-sisip memiliki celah balapan; satu-satunya jalan benar adalah
`INSERT … ON CONFLICT`. Sebelum RFC ini, arke-postgres hanya punya insert
murni, sehingga pengguna harus turun ke SQL mentah dan kehilangan pengelolaan
`pid`/`version`/cache.

## Usulan rinci

```rust
let staged = store.stage_insert(&world, e);            // entity baru: Contact (+ komponen lain)
let out = store.upsert::<Contact>(staged)
    .on(Contact::workspace()).on(Contact::wa_chat_id())  // = kolom sebuah indeks unik (RFC-0037)
    .update_coalesce(Contact::push_name())              // NULL baru tak menimpa
    .update(Contact::avatar_url())                      // nilai baru menimpa
    .execute().await?;
```

Alur dalam satu transaksi:

1. Alokasikan `pid` di `arke_entities`.
2. `INSERT INTO cmp_t (pid, …) VALUES (…) ON CONFLICT (target) DO NOTHING |
   DO UPDATE SET … RETURNING pid`.
3. `RETURNING` = pid baru → **baru**: tulis komponen lain di `staged`.
4. Selain itu → **konflik**: hapus pid yang dialokasikan (tak ada entity
   yatim). DO UPDATE mengembalikan pid lama langsung; DO NOTHING membaca pid
   lama dengan `SELECT … WHERE target = nilai`. Bila baris itu terhapus
   transaksi lain di antara kedua langkah, ulangi (maks. 3 kali).
5. DO UPDATE pada baris lama menaikkan `version` entity (kontrak optimistic
   lock `update_entity`) dan `execute` meng-invalidate cache read-through.

Aturan:

- Target harus persis kolom sebuah indeks/constraint unik (inferensi
  Postgres); target kosong, komponen `T` tak terdaftar, atau `staged` tanpa
  komponen `T` → `sqlx::Error::Protocol`.
- Komponen lain hanya ditulis untuk entity baru — upsert tidak menggabungkan
  komponen ke entity lama.
- Upsert serentak berkunci sama: tepat satu `inserted`, sisanya mendapat pid
  yang sama (diuji 8 tugas paralel).

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| Alokasi pid lewat CTE dalam satu pernyataan | Tanpa DELETE pid | CTE data-modifying tak dapat bergantung pada hasil ON CONFLICT dengan jelas | Hapus pid dalam tx sama sudah bebas yatim |
| `ON CONFLICT DO UPDATE SET col = col` untuk selalu dapat RETURNING | Tanpa SELECT kedua | Menulis baris & menaikkan xmax walau tak berubah | DO NOTHING harus benar-benar tak menulis |
| Upsert atas World (`save_incremental`) | Tak perlu API baru | Model overwrite tak mengenal kunci alami | Kunci alami adalah inti masalah |

## Dampak

- **Kompatibilitas / migrasi:** aditif; tipe baru `Upsert`, `Upserted`.
- **Keamanan / izin / provenance:** identifier dari derive di-quote; nilai
  selalu di-bind.
- **Konsekuensi pada invarian:** menjaga "Postgres sumber kebenaran" di
  bawah konkurensi; sequence `pid` dapat berlubang (tak dijanjikan rapat).

## Pertanyaan terbuka

- Upsert batch (`UNNEST … ON CONFLICT`) untuk impor massal — belum dibutuhkan.
- Ekspresi SET kustom (mis. `greatest(EXCLUDED.x, t.x)`).

## Keputusan

Diterima dan diimplementasi bersama `tests/upsert.rs`.
