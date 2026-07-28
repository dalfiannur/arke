# RFC-0022: Bundle komponen (spawn/insert tuple)

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-28
- **Milestone:** M-21 (Component Bundles)
- **ADR terkait:** [ADR-0022](../ADR/ADR-0022-component-bundles.md)

## Ringkasan

Menambahkan **`Bundle`**: menyisipkan **beberapa komponen sekaligus** lewat tuple —
`world.spawn_bundle((Pos(0,0), Vel(1,1)))` dan `world.insert_bundle(e, (A, B, C))` —
dalam **satu** perpindahan archetype (bukan `N`). Persis tesis **ergonomis =
cepat**: API lebih ringkas **dan** lebih cepat (menghindari archetype antara).

## Motivasi

Menyisipkan komponen satu per satu memindahkan entity antar-archetype **tiap**
`insert`:

```rust
world.insert(e, Pos(0,0));      // {} → {Pos}
world.insert(e, Vel(1,1));      // {Pos} → {Pos,Vel}
world.insert(e, Health(100));   // {Pos,Vel} → {Pos,Vel,Health}
```

Tiga perpindahan baris + dua archetype antara yang tak berguna. Sebuah bundle
menghitung archetype tujuan sekali dan memindahkan baris **sekali**:

```rust
world.insert_bundle(e, (Pos(0,0), Vel(1,1), Health(100)));  // 1 pindah
let e = world.spawn_bundle((Pos(0,0), Vel(1,1)));
```

## Usulan rinci

### 1. Trait `Bundle`

Detail implementasi (`#[doc(hidden)]`), diimplementasikan untuk **tuple
arity 1–8** dari tipe `Component` yang **distinct**:

```rust
pub trait Bundle {
    /// Registrasikan tipe komponen; kembalikan id-nya (urut tuple).
    fn ids(registry: &mut ComponentRegistry) -> Vec<ComponentId>;
    /// Dorong tiap komponen ke kolomnya di `archetype`.
    fn push(self, archetype: &mut Archetype, registry: &ComponentRegistry);
}
```

> **Bukan `insert(e, tuple)`.** Tuple sudah menjadi `Component` (blanket
> `impl<T: 'static + Send> Component`), jadi `insert(e, (a,b,c))` **sudah** berarti
> "simpan tuple sebagai satu komponen". Maka API bundle memakai nama berbeda:
> **`insert_bundle`/`spawn_bundle`**. `Bundle` diimplementasikan untuk *bentuk
> tuple* `(A,)`…`(A,…,H)`, bukan `T: Component` generik (menghindari tumpang-tindih).

### 2. API `World`

```rust
pub fn insert_bundle<B: Bundle>(&mut self, entity: Entity, bundle: B);
pub fn spawn_bundle<B: Bundle>(&mut self, bundle: B) -> Entity;  // spawn + insert_bundle
```

**Alur `insert_bundle`** (satu pindah archetype):

1. `ids = B::ids(registry)` (registrasi tipe).
2. Archetype tujuan `= komponen lama ∪ ids` (dedup, terurut).
3. Pindahkan baris lama ke tujuan **sekali** (`move_row_to`) — memindahkan
   komponen yang sudah ada.
4. `B::push(bundle, dst, registry)` mendorong tiap komponen bundle ke kolomnya.
5. Perbarui lokasi + `fix_swapped`.

`spawn_bundle` = `spawn()` lalu `insert_bundle` (satu penempatan, tanpa pindah).

### 3. Kontrak & keamanan

- **Distinct + baru**: komponen bundle harus **distinct** antar-elemen **dan**
  belum dimiliki entity. Pelanggaran → **panic** menyebut komponennya (STD-0008),
  sejalan dengan cek-alias query (`assert_no_alias`). (Mencegah kolom rusak /
  push-ganda; setara kontrak `insert` tunggal yang mengasumsikan komponen baru.)
- **Tanpa `unsafe` baru**: memakai operasi archetype aman yang sudah ada
  (`move_row_to`, `push_component`).
- **Determinisme**: id archetype tujuan **terurut** → penempatan deterministik
  (STD-0005), identik dengan urutan `insert` satu-per-satu.

### 4. Ekuivalensi

`insert_bundle(e, (A, B))` menghasilkan keadaan **identik** dengan
`insert(e, A); insert(e, B)` (archetype & nilai sama) — hanya lebih sedikit
perpindahan antara. Round-trip snapshot & query tak berubah.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| Overload `insert(e, bundle)` | Satu nama | Tuple = `Component` → ambigu/breaking | `insert_bundle` eksplisit & aman |
| `Bundle` utk `T: Component` + tuple | `spawn_bundle(Pos)` tunggal | Tumpang-tindih impl (tuple *adalah* Component) | Impl hanya untuk bentuk tuple |
| Sisip satu-per-satu (status quo) | Tak ada kode baru | `N` pindah archetype + archetype antara | Tujuan RFC ini justru menghapusnya |
| Overwrite komponen yang sudah ada | Lebih fleksibel | Butuh set-nilai-in-place bertipe | Ditunda; kontrak "baru" selaras `insert` |

## Dampak

- **Kompatibilitas / migrasi:** aditif. `insert`/`spawn` tak berubah; `Bundle`,
  `insert_bundle`, `spawn_bundle` baru.
- **Keamanan:** tanpa `unsafe` baru; jalur pengguna tetap bebas `unsafe` (STD-0004).
- **Konsekuensi pada invarian:** memperkuat **ergonomis = cepat** (lebih ringkas
  + lebih sedikit perpindahan). Determinisme dijaga.

## Pertanyaan terbuka

- **Bundle di `CommandBuffer`** (spawn tertunda dgn bundle) — follow-up.
- **Overwrite** komponen yang sudah ada via bundle — follow-up.
- **`remove_bundle`** (hapus banyak sekaligus) — follow-up.

## Keputusan

Diterima. Lihat [ADR-0022](../ADR/ADR-0022-component-bundles.md).
