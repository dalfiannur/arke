# RFC-0039: `arke-postgres` — aksi hapus pada relasi (`on_delete`)

- **Status:** Accepted
- **Tanggal:** 2026-09-25
- **Milestone:** —
- **ADR terkait:** — (mengamandemen keputusan "relasi tanpa FK" RFC-0034 Am.3 menjadi opt-in)

## Ringkasan

Field relasi (`Entity`/`Ref<T>`, termasuk `Option<…>`) dapat diberi
`#[pg(on_delete = "cascade" | "set_null" | "restrict")]`. Derive menurunkan
`PgComponent::ON_DELETE: &[OnDeleteDef]`; `migrate` memasang FK kolom ke
`arke_entities(pid)`, indeks kolom, dan — untuk `cascade` — trigger yang
menghapus **entity** perujuk secara utuh. Relasi tanpa atribut tetap seperti
sebelumnya: tanpa FK.

## Motivasi

RFC-0034 Am.3 sengaja menghapus FK relasi: integritas dijaga oleh konstruksi,
dan rujukan menggantung terbaca sebagai handle basi. Untuk aplikasi yang
menjadikan Postgres sumber kebenaran bersama (beberapa proses, SQL lain), itu
tidak cukup: menghapus workspace harus menghapus seluruh isinya, menghapus
anggota harus mengosongkan penugasan, dan rujukan ke entity yang tak ada harus
ditolak di database, bukan terbaca basi.

## Usulan rinci

| Aksi | FK kolom | Efek saat entity dirujuk dihapus |
| --- | --- | --- |
| `cascade` | `NO ACTION DEFERRABLE INITIALLY DEFERRED` | Trigger menghapus entity perujuk (semua komponennya), rekursif |
| `set_null` | `ON DELETE SET NULL` | Kolom perujuk menjadi `NULL`; hanya `Option<…>` |
| `restrict` | `NO ACTION DEFERRABLE INITIALLY DEFERRED` | Ditolak saat commit selama perujuk masih ada |

- **Level entity, bukan baris.** `ON DELETE CASCADE` biasa hanya menghapus
  baris komponen perujuk dan meninggalkan entity-nya yatim di
  `arke_entities`. Cascade karena itu diwujudkan sebagai trigger `AFTER DELETE
  FOR EACH ROW` di `arke_entities` yang menjalankan
  `DELETE FROM arke_entities WHERE pid IN (SELECT pid FROM cmp_t WHERE col = OLD.pid)`.
- **AFTER, bukan BEFORE.** Overwrite penuh `save` menghapus semua entity dalam
  satu pernyataan; trigger BEFORE yang ikut menghapus baris yang juga sasaran
  pernyataan itu memicu galat 27000. Trigger AFTER berjalan setelah semua
  sasaran terhapus, dan siklus (a → b → a) berhenti sendiri karena baris yang
  sudah terhapus tak terlihat lagi.
- **FK deferred** untuk `cascade`/`restrict`: cek integritas di akhir
  transaksi, setelah trigger dan overwrite penuh selesai. Konsekuensinya galat
  `restrict` muncul saat commit (`remove` tanpa tx: saat pernyataan selesai).
- **SQL dinamis berpenjaga.** Fungsi trigger memeriksa `to_regclass(tabel)`
  dan memakai `EXECUTE … USING`, sehingga tabel perujuk yang di-DROP tak
  membuat setiap DELETE entity gagal.
- **Penamaan content-addressed:** FK `afk_<tabel>_<hash(kolom|aksi)>`, indeks
  `aidx_<tabel>_<hash(kolom)>`, trigger/fungsi
  `arke_ondel_<hash(tabel)>_<hash(kolom)>`. Trigger memakai hash tabel karena
  `arke_entities` dipakai bersama semua tabel — awalan nama mentah dapat
  bertumpang (`cmp_a_` ⊂ `cmp_a_b_`). Objek usang milik tabel terdaftar
  di-DROP oleh `migrate`.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| FK `ON DELETE CASCADE` pada kolom | Deklaratif murni | Entity perujuk menjadi yatim | Melanggar model entity |
| Trigger pada tabel komponen (hapus entity saat komponen hilang) | — | `commit_update` menghapus lalu menulis ulang baris komponen → entity terhapus | Salah |
| Cascade di sisi Rust (`remove` rekursif) | Tanpa trigger | Tak berlaku untuk `delete_where`, SQL lain, atau proses lain | Postgres adalah sumber kebenaran |
| FK `RESTRICT` segera | Galat lebih awal | Overwrite penuh `save` gagal | Deferred menjaga `save` |

## Dampak

- **Kompatibilitas / migrasi:** aditif (const baru berdefault kosong). Tabel
  berisi rujukan menggantung membuat `ADD CONSTRAINT` gagal keras saat
  `migrate` — pembersihan data adalah keputusan operator (pola `CHECK`).
- **Keamanan / izin / provenance:** identifier di-quote; nilai di-bind
  (`USING`).
- **Konsekuensi pada invarian:** memperkuat Postgres sebagai sumber
  kebenaran. Setiap DELETE di `arke_entities` kini menjalankan trigger cascade
  semua tabel yang mendeklarasikannya — ditopang indeks kolom otomatis.

## Pertanyaan terbuka

- Cache read-through (RFC-0033) tidak di-invalidate untuk entity yang terhapus
  lewat cascade.
- Trigger milik tabel yang tak lagi di-register oleh store mana pun tidak
  dibersihkan otomatis (tetap aman karena penjaga `to_regclass`).
- FK komposit lintas-tenant (mis. conversation.channel harus satu workspace)
  belum diungkapkan; dijaga oleh konstruksi di aplikasi.

## Keputusan

Diterima dan diimplementasi bersama `tests/on_delete.rs`.
