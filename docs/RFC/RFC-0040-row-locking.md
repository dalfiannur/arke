# RFC-0040: `arke-postgres` — penguncian baris (`FOR UPDATE`)

- **Status:** Accepted
- **Tanggal:** 2026-09-25
- **Milestone:** —
- **ADR terkait:** —

## Ringkasan

`Query::for_update()` mengubah query menjadi `Locked<'_, T>` yang mengunci
baris komponen `T` hasilnya (`FOR UPDATE OF cmp_t`), dengan opsi
`.skip_locked()` atau `.nowait()`. Satu-satunya terminal adalah
`pids_in(&mut tx)`: kunci hanya bermakna di dalam transaksi, jadi pemakaian
tanpa transaksi gagal dikompilasi.

## Motivasi

Worker antrean berbasis tabel (impor, pengiriman ulang) harus mengklaim satu
job tanpa dua worker mengambil yang sama dan tanpa saling menunggu. Pola baku
Postgres adalah `SELECT … FOR UPDATE SKIP LOCKED LIMIT 1` di dalam transaksi,
lalu memperbarui job itu di transaksi yang sama. Query builder belum dapat
mengungkapkannya.

## Usulan rinci

```rust
let mut tx = store.begin().await?;
let claimed = store.query::<Job>()
    .filter(Job::status().eq(JobStatus::Pending))
    .order_by(Job::created_at(), Dir::Asc)
    .limit(1)
    .for_update().skip_locked()
    .pids_in(&mut tx).await?;
if let Some(&pid) = claimed.first() {
    store.update_where::<Job>() /* … */ .execute_in(&mut tx).await?;
}
tx.commit().await?;
```

- SQL = SQL `load_pids` (filter, join, `with`/`without`, `order_by`, kursor,
  `limit`/`offset`) + `FOR UPDATE OF <tabel T> [SKIP LOCKED | NOWAIT]`.
  `OF` membatasi kunci pada baris `T`, bukan tabel yang disentuh subquery.
- Mengembalikan `pid` saja; materialisasi entity dilakukan terpisah (`fetch`)
  bila perlu. Kunci bertahan hingga `tx` commit/rollback.
- `nowait` pada baris terkunci → galat SQLSTATE `55P03`.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| Flag `lock` di `Query` + `load_in` | Satu tipe | Kunci di luar tx dapat ditulis dan tak bermakna | Typestate mencegahnya saat kompilasi |
| Advisory lock (`PgTx::advisory_lock`) | Sudah ada | Tak memilih baris; butuh kunci numerik buatan | Tak menjawab "klaim job berikutnya" |
| `load_in` yang memuat entity di tx | Satu langkah | Materialisasi & cache belum ber-executor tx | Di luar cakupan; `pid` cukup |

## Dampak

- **Kompatibilitas / migrasi:** aditif; tipe baru `Locked`.
- **Keamanan / izin / provenance:** tak ada.
- **Konsekuensi pada invarian:** tak ada perubahan skema.

## Pertanyaan terbuka

- `FOR NO KEY UPDATE` / `FOR SHARE` bila dibutuhkan.
- `load_in(tx, world)` yang memuat entity terkunci dalam transaksi yang sama.

## Keputusan

Diterima dan diimplementasi bersama `tests/locking.rs`.
