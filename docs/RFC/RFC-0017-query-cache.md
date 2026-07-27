# RFC-0017: Query Cache sebagai first-class citizen

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-28
- **Milestone:** M-16 (Query Cache)
- **ADR terkait:** [ADR-0017](../ADR/ADR-0017-query-cache.md)

## Ringkasan

Menjadikan **`QueryState`** sebuah objek first-class yang **meng-cache archetype yang cocok** untuk sebuah query, di-update **inkremental** saat archetype baru muncul. Menghilangkan pemindaian **seluruh** archetype + cek filter pada **setiap** pemanggilan query. `System::each`/`each_filtered` memakainya **transparan** (cache persist lintas-run). 100% aman, determinisme & hasil tak berubah.

## Motivasi

Kini `each_filtered_shared` memindai **semua** archetype dan mengecek `column_index` + filter **tiap run**. Untuk sistem yang jalan tiap frame di world ber-banyak-archetype, ini O(archetype) sia-sia berulang. Archetype bersifat **append-only** dan **set-komponennya immutable** → daftar "archetype yang cocok" untuk sebuah query **stabil** dan hanya perlu diperluas saat archetype baru dibuat.

## Usulan rinci

### 1. `QueryState`

```rust
#[derive(Default)]
pub struct QueryState {
    matched: Vec<usize>, // indeks archetype yang cocok
    scanned: usize,      // jumlah archetype yang sudah diperiksa
}
```

### 2. Alur `each_cached`

```rust
fn each_cached<F: QueryFilter>(world: &World, state: &mut QueryState, f);
```

1. Resolve `ComponentId` query + filter. Bila ada komponen **fetch/`With`** yang belum terdaftar → tak ada yang cocok, **return** (tanpa memajukan `scanned`).
2. **Scan inkremental**: untuk `archetype[state.scanned..]`, tambahkan yang cocok (memuat semua komponen fetch + filter) ke `matched`; set `scanned = archetypes.len()`.
3. Iterasi **hanya** `matched` (fetch item, panggil `f`).

`each_filtered_shared` menjadi wrapper: `QueryState::default()` sekali-pakai (memindai semua) → jalur tanpa-cache untuk pemanggilan satu-kali.

### 3. Kebenaran cache (kenapa aman)

- Archetype **tak pernah dihapus** & **set-komponennya tetap** → sebuah archetype yang cocok tetap cocok selamanya; cache tak perlu invalidasi, hanya **diperluas**.
- Perubahan struktural (tambah/hapus komponen) **memindah entity antar-archetype yang sudah ada** atau **membuat archetype baru** — keanggotaan berubah, tapi identitas archetype tetap. Scan inkremental menangkap archetype baru; archetype lama tetap valid.
- Registrasi komponen terlambat: bila komponen query belum terdaftar, `each_cached` return tanpa memajukan `scanned` → run berikut (saat terdaftar) memindai dari awal. Konsisten.

### 4. Integrasi sistem

`System::each::<Q>`/`each_filtered::<Q, F>` menyimpan `QueryState` di environment closure-nya → cache **persist** lintas-run. Tiap sistem punya `QueryState` sendiri (dimutasi thread-nya sendiri di jalur paralel — tak ada berbagi state). Transparan bagi pengguna.

### 5. First-class

`QueryState` publik: pengguna dapat memegangnya dan memanggil `Q::each_cached(world, &mut state, f)` untuk query berulang di luar sistem.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| Cache global di `World` (per-signature) | Berbagi antar-sistem | Perlu kunci signature + lookup HashMap tiap run | Per-`QueryState` inkremental lebih sederhana & cepat |
| Rebuild penuh saat archetype berubah | Sederhana | Buang kerja; butuh sinyal invalidasi | Inkremental (append-only) tak perlu rebuild |
| Cache (archetype, kolom) | Skip lookup kolom | State per-arity rumit | Simpan indeks archetype; lookup kolom archetype-cocok murah |

## Dampak

- **Kompatibilitas / migrasi:** aditif & transparan. Hasil & urutan iterasi **identik** (STD-0005). API lama tak berubah.
- **Keamanan:** 100% aman (tanpa `unsafe` baru).
- **Konsekuensi pada invarian:** **memperkuat *ergonomis = cepat*** (query berulang jadi O(cocok), bukan O(semua)).

## Pertanyaan terbuka

- Cache indeks kolom per archetype-cocok (optimasi lanjutan).
- `QueryState` yang di-share lintas-sistem dengan signature sama → milestone lanjutan bila perlu.

## Keputusan

Diterima. Lihat [ADR-0017](../ADR/ADR-0017-query-cache.md).
