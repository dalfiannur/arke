# RFC-0042: `arke-postgres` — baca di dalam transaksi

- **Status:** Accepted
- **Tanggal:** 2026-09-26
- **Milestone:** —
- **ADR terkait:** —

## Ringkasan

`Query::load_pids_in(&mut tx, world)` dan `PgStore::fetch_in(&mut tx, world,
pid)` memuat entity lewat koneksi transaksi milik pemanggil, sehingga
melihat tulisan transaksi itu yang belum di-commit. Entity yang dimuat
terpetakan di store seperti muat biasa, jadi relasi `Ref` ke entity itu
resolve untuk tulisan berikutnya di transaksi yang sama.

## Motivasi

Operasi tulis per-request yang utuh (mis. menyerap satu pesan masuk: upsert
kontak → upsert percakapan → sisip pesan → perbarui ringkasan percakapan)
harus atomik dan baru mengumumkan hasilnya setelah commit. Sampai sekarang
semua baca memakai pool: baris yang baru di-upsert di transaksi tidak
terlihat, sehingga pemanggil tak dapat membaca kembali entity itu, dan tak
dapat merujuknya lewat `Ref` (relasi hanya resolve untuk entity yang
terpetakan di store). Satu-satunya jalan keluar adalah SQL mentah.

## Usulan rinci

- `Query::load_pids_in(self, tx, world)`: SQL sama dengan `load_pids`
  (filter, urutan, `limit`, kursor, `include`/`join_load`), dijalankan di
  `tx`.
- `PgStore::fetch_in(&mut self, tx, world, pid)`: padanan `fetch`.
- Di dalam transaksi **cache read-through dilewati sepenuhnya** — baik baca
  maupun isi. Baris yang belum di-commit tak boleh disajikan dari cache bersama
  (bisa di-rollback), dan cache tak boleh menyajikan versi lama dari baris
  yang baru saja diubah transaksi ini.
- Implementasi: jalur materialisasi menerima koneksi opsional; jalur tanpa
  transaksi tidak berubah perilakunya.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| `adopt(pid)` — petakan pid ke entity kosong tanpa memuat | Murah | Tak bisa membaca isi (mis. status untuk memutuskan event) | Tak cukup untuk kasus nyata |
| Membuka `PgStore::pool()` untuk SQL mentah | Serbaguna | Melewati pemetaan pid↔Entity & cache | Kembali ke hibrida yang ditolak |
| Executor generik di seluruh API baca | Satu jalur | Mengubah tanda tangan publik yang ada | Varian `_in` mengikuti pola `*_in` yang sudah ada |

## Dampak

- **Kompatibilitas / migrasi:** aditif.
- **Keamanan / izin / provenance:** tak ada.
- **Konsekuensi pada invarian:** cache tetap hanya berisi data ter-commit.

## Pertanyaan terbuka

- `load_page_in` dan `count_in` untuk keyset di dalam transaksi.

## Keputusan

Diterima dan diimplementasi bersama `tests/read_in_tx.rs`.
