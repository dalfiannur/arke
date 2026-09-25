# RFC-0037: `arke-postgres` — indeks komposit level-tipe

- **Status:** Accepted
- **Tanggal:** 2026-09-25
- **Milestone:** —
- **ADR terkait:** —

## Ringkasan

Atribut level-tipe `#[pg(index(a, b, …))]` dan `#[pg(unique(a, b, …))]` pada
`#[derive(PgComponent)]` menurunkan konstanta baru
`PgComponent::COMPOSITE_INDEXES: &[CompositeIndexDef]`. `migrate` membuat
indeksnya secara idempoten dengan nama *content-addressed*
`cidx_<tabel>_<fnv64(unik|kolom)>`, dan membuang indeks berawalan sama yang tak
lagi dideklarasikan.

## Motivasi

Aplikasi web yang menjadikan arke-postgres sumber data penuh butuh keunikan
atas lebih dari satu kolom: idempotensi webhook (`(channel, wa_message_id)`),
satu baris per pasangan (`(workspace, user)`), dan indeks akses urut
(`(conversation, wa_timestamp)`). `#[pg(unique)]` per-field tak dapat
mengungkapkannya, dan menambahkannya lewat SQL manual di luar `migrate`
memecah model "skema diturunkan dari tipe".

## Usulan rinci

```rust
#[derive(PgComponent)]
#[pg(unique(channel, wa_id), index(conversation, at))]
struct Message { channel: Ref<Channel>, conversation: Ref<Conversation>, wa_id: Option<String>, at: DateTime<Utc> }
```

- Isi kurung adalah **nama field**. Field relasi (`Entity`/`Ref<T>`) dipetakan
  ke kolom `<name>_id`. Nama yang tak dikenal → `compile_error!`.
- Di level-field, `index`/`unique` tanpa kurung tetap berarti indeks satu
  kolom (perilaku lama); bentuk berkurung di level-field ditolak.
- `CompositeIndexDef { columns, unique }` adalah konstanta **baru** dengan
  default `&[]` pada trait, jadi `IndexDef` dan impl manual `PgComponent` tak
  rusak.
- `migrate`: daftar indeks `pg_indexes` milik tabel dengan awalan
  `cidx_<tabel>_` dibandingkan dengan nama yang diinginkan. Yang usang di-DROP,
  yang belum ada dibuat (`CREATE [UNIQUE] INDEX`). Definisi sama → tanpa DDL.
  Indeks di luar awalan itu tak pernah disentuh.
- Semantik NULL mengikuti Postgres bawaan (`NULLS DISTINCT`): dua baris dengan
  `wa_id` NULL tak bentrok.
- Indeks UNIQUE (bukan constraint) sengaja dipakai: ia tetap menjadi sasaran
  inferensi `ON CONFLICT (a, b)` untuk upsert (RFC-0038).

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| Memperluas `IndexDef` jadi multi-kolom | Satu tipe | Mengubah field publik (breaking) | Konstanta terpisah non-breaking |
| Nama `idx_<tabel>_<kolom…>` | Mudah dibaca | Melampaui 63 byte lalu terpotong diam-diam | Hash menjamin nama stabil & muat |
| Tidak membuang yang usang | Lebih konservatif | UNIQUE usang terus menolak tulis | Awalan khusus membuat DROP aman |

## Dampak

- **Kompatibilitas / migrasi:** aditif. Tabel lama mendapat indeks baru saat
  `migrate`; baris yang melanggar UNIQUE baru membuat `migrate` gagal keras
  (keputusan operator, sama seperti `CHECK`).
- **Keamanan / izin / provenance:** tak ada.
- **Konsekuensi pada invarian:** memperkuat "skema diturunkan dari tipe".

## Pertanyaan terbuka

- Indeks parsial / ekspresi (`WHERE …`, `lower(col)`) — belum dibutuhkan.

## Keputusan

Diterima dan diimplementasi bersama `tests/composite.rs`.
