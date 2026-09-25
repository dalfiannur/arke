# RFC-0041: `arke-postgres` — `Query::include` untuk relasi bertipe

- **Status:** Accepted
- **Tanggal:** 2026-09-26
- **Milestone:** —
- **ADR terkait:** —

## Ringkasan

`Query::include(T::rel())` memuat entity target relasi bertipe (`Ref<R>` /
`Option<Ref<R>>`) untuk baris `T` yang dimuat query, sehingga relasi pada
entity hasil langsung resolve ke entity target di `World`.

## Motivasi

Pada pola World per-request (RFC-0034), relasi hanya resolve bila targetnya
ikut termuat di World yang sama; bila tidak, handle-nya menggantung. Untuk
relasi **bertipe**, belum ada cara memuat target bersama query utama:
`join_load` hanya menerima relasi `Entity` tanpa tipe dan mensyaratkan filter,
sedangkan path `through` membuang filter root. Aplikasi web butuh pola dasar
"muat membership berikut workspace-nya" atau "muat halaman percakapan berikut
kontaknya" di hampir setiap permintaan.

## Usulan rinci

```rust
store.query::<Conversation>()
    .filter(…).order_by(Conversation::last_message_at(), Dir::Desc).limit(30)
    .include(Conversation::contact())
    .include(Conversation::assignee_membership())
    .load_pids(&mut world).await?;
```

- Target diambil dari **baris hasil** SQL utama saja:
  `SELECT DISTINCT t.rel FROM cmp_t t WHERE t.pid IN (SELECT m.pid FROM (<SQL
  utama>) m) AND t.rel IS NOT NULL`. `limit`, `order_by`, `offset`, dan kursor
  ikut berlaku; SQL utama sudah bernomor `$n`, jadi param-nya dipakai ulang.
- Target dimuat **sebelum** entity utama (urutan yang sama dengan `join_load`),
  sehingga pembacaan relasi utama menemukan target di jembatan `entity_of`.
- Tidak menyaring: relasi `None` tetap `None`, baris tanpa target tetap dimuat.
- Berlaku untuk `load`, `load_pids`, dan `load_page`. Satu tingkat; beberapa
  relasi dengan memanggil `include` berulang.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| `join_load` versi bertipe | Mekanisme yang ada | Menyaring baris dan memuat target semua baris yang cocok filter, bukan halaman | Semantik berbeda dari "sertakan" |
| Memperbaiki `through` agar memakai filter root | Path multi-hop | Mengubah semantik API yang ada | Di luar cakupan |
| Muat target dengan query terpisah di aplikasi | Tanpa API baru | Pid target tak diketahui saat relasi menggantung | Tidak mungkin tanpa akses SQL mentah |

## Dampak

- **Kompatibilitas / migrasi:** aditif.
- **Keamanan / izin / provenance:** tak ada; tak ada nilai baru di SQL.
- **Konsekuensi pada invarian:** tak ada perubahan skema.

## Pertanyaan terbuka

- Include bersarang (`include(A::b()).then(B::c())`).

## Keputusan

Diterima dan diimplementasi bersama `tests/include.rs`.
