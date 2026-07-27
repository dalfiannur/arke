# RFC-0014: Filter query `With` / `Without`

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-28
- **Milestone:** M-13 (Query Filters)
- **ADR terkait:** [ADR-0014](../ADR/ADR-0014-query-filters.md)

## Ringkasan

Menambahkan **filter** query `With<T>` dan `Without<T>` — batasan pencocokan yang **tidak** mengambil data komponen — beserta konstruktor sistem `System::each_filtered::<Q, F>(f)`. Melengkapi sistem query M-12. Tetap tanpa `unsafe`, 0 dependensi eksternal.

## Motivasi

Sistem sering perlu menyaring entity berdasarkan *kehadiran* komponen tanpa membacanya: "gerakkan semua yang punya `Velocity` **tetapi tidak** `Frozen`". Menyertakan komponen di query data hanya untuk menyaring memaksa mengambil datanya (dan menambah akses palsu). Filter memisahkan *pencocokan* dari *pengambilan*.

## Usulan rinci

### 1. Penanda filter

```rust
pub struct With<T>(PhantomData<T>);
pub struct Without<T>(PhantomData<T>);
```

Tipe zero-sized; hanya dipakai sebagai parameter tipe.

### 2. Trait `QueryFilter`

```rust
pub trait QueryFilter {
    /// Kumpulkan komponen yang harus HADIR (`with`) & harus ABSEN (`without`).
    /// Kembalikan `false` bila sebuah `With` komponennya tak terdaftar
    /// (→ query tak mencocokkan apa pun).
    fn resolve(world: &World, with: &mut Vec<ComponentId>, without: &mut Vec<ComponentId>) -> bool;
}
```

- `With<T>`: dorong `ComponentId` T ke `with`; `false` bila T tak terdaftar.
- `Without<T>`: dorong ke `without` bila terdaftar; selalu `true` (tak-terdaftar → pasti absen).
- `()`: tak menambah apa pun.
- Tuple `(F0, F1, …)`: AND — semua harus `resolve` sukses.

Filter **tidak** menyumbang ke `Access` (tak membaca/menulis data) → tak memengaruhi konflik scheduler.

### 3. Integrasi ke `QueryData`

`QueryData::each` digeneralisasi menjadi:

```rust
fn each_filtered<F: QueryFilter>(world: &mut World, f: impl FnMut(Self::Item<'_>));
fn each(world, f) { Self::each_filtered::<()>(world, f) }  // default: tanpa filter
```

`each_filtered` me-resolve filter sekali, lalu untuk tiap archetype hanya memproses bila memuat **semua** `with` dan **tak satupun** `without`.

### 4. API sistem

```rust
System::each_filtered::<&mut Position, Without<Frozen>>(|pos| pos.0 += 1);
System::each_filtered::<(&Velocity, &mut Position), (With<Player>, Without<Frozen>)>(...);
```

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| Filter sebagai term data ber-`Item=()` | Satu param tipe | `()` mengotori closure & akses | Pemisahan data/filter lebih bersih |
| Query komponen lalu `skip` manual | Tanpa fitur baru | Mengambil data tak perlu; boros | Filter lebih ekspresif & efisien |
| Predikat closure runtime | Fleksibel | Tak terdeklarasi (scheduler tak tahu) | Tipe filter lebih deklaratif |

## Dampak

- **Kompatibilitas / migrasi:** aditif. `each` lama tetap (default `each_filtered::<()>`).
- **Keamanan:** tetap tanpa `unsafe`.
- **Konsekuensi pada invarian:** memperkuat *ergonomis = cepat* (menyaring tanpa mengambil data). Filter tak memengaruhi determinisme/konflik.

## Pertanyaan terbuka

- Filter `Or<...>`, `Changed<T>`/`Added<T>` (deteksi perubahan) → milestone lanjutan.
- `Entity` sebagai term data → lanjutan.

## Keputusan

Diterima. Lihat [ADR-0014](../ADR/ADR-0014-query-filters.md).
