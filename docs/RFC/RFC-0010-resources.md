# RFC-0010: Resources sebagai parameter sistem

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-28
- **Milestone:** M-9 (Resources)
- **ADR terkait:** [ADR-0010](../ADR/ADR-0010-resources.md)

## Ringkasan

Menambahkan **resource** — state global *singleton-per-tipe* (mis. `Time`, `Config`, `Score`) yang tidak terikat entity — beserta cara mengaksesnya sebagai **parameter sistem bertipe**. `World` menyimpan satu instance per tipe; scheduler melacak konflik akses resource seperti halnya komponen. Ini melengkapi lapisan `World`/`System` di [ARCHITECTURE_BIBLE](../ARCHITECTURE_BIBLE.md) §3 yang sudah menyebut "entity, komponen, **resource**".

## Motivasi

Banyak logika butuh state global, bukan per-entity: waktu-delta, konfigurasi, input, skor. Menyimpannya sebagai komponen pada entity-palsu adalah anti-pola. Sistem juga perlu mengakses state ini secara terdeklarasi agar scheduler dapat menalar konfliknya.

## Usulan rinci

### 1. Penyimpanan resource di `World`

Satu instance per tipe (`T: 'static + Send`):

```rust
impl World {
    pub fn insert_resource<T: 'static + Send>(&mut self, resource: T);
    pub fn resource<T: 'static + Send>(&self) -> Option<&T>;
    pub fn resource_mut<T: 'static + Send>(&mut self) -> Option<&mut T>;
    pub fn remove_resource<T: 'static + Send>(&mut self) -> Option<T>;
    pub fn contains_resource<T: 'static + Send>(&self) -> bool;
}
```

Disimpan sebagai `HashMap<TypeId, Box<dyn Any + Send>>`.

### 2. Akses resource dalam scheduler

`Access` (query.rs) diperluas dengan **namespace terpisah** untuk resource (`resource_reads`/`resource_writes`), agar komponen `T` dan resource `T` (yang ber-`TypeId` sama) **tidak** salah-konflik. Aturan konflik berlaku per-namespace: dua sistem berkonflik bila berbagi komponen *atau* resource yang ditulis salah satunya.

### 3. Parameter sistem bertipe

```rust
impl System {
    // Sistem resource-saja: memutasi resource R sekali per run.
    pub fn resource<R: 'static + Send>(f: impl FnMut(&mut R) + 'static) -> Self;

    // Membaca resource R sambil mengiterasi query Q per entity.
    pub fn each_res<R: 'static + Send, Q: QueryData>(
        f: impl FnMut(&R, Q::Item<'_>) + 'static,
    ) -> Self;
}
```

- Akses tersimpul: `resource` → tulis R; `each_res` → baca R **+** akses Q.
- Implementasi `each_res` yang aman: **ambil resource keluar sementara** (`remove_resource`), iterasi query, lalu **kembalikan** (`insert_resource`) — menghindari peminjaman `&R` dan `&mut World` sekaligus **tanpa `unsafe`**.
- Bila resource tak ada, sistem menjadi no-op (didokumentasikan).

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| `Res<T>`/`ResMut<T>` variadik penuh (gaya bevy) | Paling ergonomis & umum | Mesin SystemParam variadik besar (lifetime GAT/HRTB) | Milestone besar berisiko; ditunda |
| Resource sebagai komponen entity-tunggal | Tak ada konsep baru | Anti-pola; membingungkan query | Resource memang bukan per-entity |
| Hanya storage (opaque akses) | Paling kecil | Bukan "parameter sistem"; scheduler tak tahu konflik | Tak memenuhi tujuan |
| `each_res` via `unsafe` split borrow | Tanpa take/put-back | `unsafe` tak terverifikasi (miri absen) | Take/put-back aman & cukup |

## Dampak

- **Kompatibilitas / migrasi:** aditif. API sistem yang ada tak berubah.
- **Keamanan / provenance:** tetap tanpa `unsafe`. Konflik resource ditegakkan scheduler.
- **Konsekuensi pada invarian:** memperkuat *determinisme* (akses resource terdeklarasi masuk analisis stage). Menyiapkan paralelisme tingkat-sistem yang menyertakan resource (kelak).

## Pertanyaan terbuka

- `each_res` yang **memutasi** resource (`&mut R`) sambil iterasi → varian lanjutan bila dibutuhkan.
- `Res<T>`/`ResMut<T>` variadik penuh → milestone SystemParam tersendiri.
- Serialisasi resource dalam snapshot → milestone snapshot lanjutan.
- Panik saat iterasi `each_res` membuat resource tak dikembalikan → guard bila jadi soal (RN).

## Keputusan

Diterima. Lihat [ADR-0010](../ADR/ADR-0010-resources.md).
