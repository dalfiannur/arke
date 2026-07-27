# RFC-0002: Arsitektur penyimpanan inti — archetype + generational entity

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-27
- **Milestone:** M-1 (Core Storage & Query Minimal)
- **ADR terkait:** [ADR-0002](../ADR/ADR-0002-core-storage-architecture.md)

## Ringkasan

Menetapkan model penyimpanan inti Arke: entity sebagai **generational index** (`u32` indeks + `u32` generasi), komponen disimpan dalam **archetype** (tabel per-kombinasi-komponen) dengan kolom **kontigu bertipe** yang di-*type-erase* di balik trait object, dan **registrasi komponen otomatis** saat insert pertama. Query mengiterasi tiap archetype yang cocok dengan men-*downcast* kolom **sekali per archetype** menjadi `&[T]`/`&mut [T]`. Kombinasi ini menegakkan invarian *ergonomis = cepat*, *determinisme by construction*, dan *struktural aman* dari [ARCHITECTURE_BIBLE](../ARCHITECTURE_BIBLE.md) §2.

## Motivasi

MILESTONE_1 membutuhkan fondasi penyimpanan yang menjadi tumpuan semua hal berikutnya (scheduler, snapshot). Fondasi ini harus, sekaligus:

- membuat **iterasi query** cache-friendly tanpa menuntut `unsafe` dari pengguna (STD-0004);
- membuat **alokasi entity & urutan iterasi** deterministik (STD-0005);
- membuat **referensi entity basi** selalu terdeteksi (STD-0007);
- tetap **standalone** tanpa dependensi berat (STD-0003).

Pertanyaan terbuka di MILESTONE_1 (layout kolom archetype; registrasi komponen eksplisit vs otomatis) diselesaikan di sini.

## Usulan rinci

### 1. Model entity — generational index

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Entity {
    index: u32,      // slot di dalam World
    generation: u32, // dinaikkan tiap slot dipakai ulang
}
```

`World` memegang tabel slot dan sebuah *free-list* LIFO:

```rust
struct EntityMeta {
    generation: u32,
    location: Option<EntityLocation>, // None = slot bebas
}
struct EntityLocation { archetype: ArchetypeId, row: u32 }
```

- **Spawn:** bila free-list tak kosong, `pop()` indeksnya (LIFO — deterministik: murni fungsi urutan operasi); jika tidak, alokasikan indeks baru = panjang tabel. Entity yang dikembalikan memakai `generation` slot saat ini.
- **Despawn:** naikkan `generation` slot, set `location = None`, dorong indeks ke free-list. Handle lama kini memiliki `generation` yang tak lagi cocok.
- **Validasi:** akses `Entity` valid hanya bila `meta[index].generation == entity.generation` **dan** `location.is_some()`. Jika tidak → `None`/error (STD-0007).

Pilihan `u32 + u32`: ~4 miliar entity hidup dan ~4 miliar pemakaian-ulang per slot — headroom besar tanpa batas praktis untuk target pengguna.

### 2. Model komponen — registrasi otomatis

Komponen apa pun adalah tipe `'static + Send` (batas `Send` menyiapkan paralelisme M-2 tanpa mengubah API):

```rust
pub struct ComponentId(u32);

struct ComponentInfo {
    id: ComponentId,
    type_id: TypeId,
    type_name: &'static str, // kunci stabil untuk serialisasi (M-3)
    new_column: fn() -> Box<dyn Column>,
}
```

`World` menyimpan `ComponentRegistry` yang memetakan `TypeId -> ComponentInfo`. Pada **insert pertama** sebuah tipe, id diberikan berurutan secara otomatis — tanpa boilerplate pengguna. `ComponentId` bersifat internal ke satu proses; **serialisasi memakai `type_name`**, bukan id numerik, sehingga snapshot tetap portabel dan deterministik lintas run (menopang STD-0001/0002 di milestone snapshot).

### 3. Penyimpanan archetype

Satu **archetype** = satu himpunan `ComponentId` yang unik & terurut. Semua entity dengan himpunan komponen persis sama berbagi satu tabel:

```rust
struct Archetype {
    id: ArchetypeId,
    component_ids: Box<[ComponentId]>,      // terurut
    columns: Vec<Box<dyn Column>>,          // sejajar dengan component_ids
    entities: Vec<Entity>,                  // row -> Entity (untuk swap-remove & iterasi)
}

trait Column {
    fn swap_remove(&mut self, row: usize);
    fn len(&self) -> usize;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

struct TypedColumn<T>(Vec<T>); // impl Column; kolom KONTIGU & bertipe
```

- **Kolom kontigu bertipe** (`Vec<T>` di balik trait object) — bukan blob byte mentah. Iterasi memperoleh `&[T]` asli, jadi kompilator melihat tipe konkret dan meng-*inline*/vektorisasi seperti biasa.
- **Perubahan struktural** (insert/remove komponen): hitung archetype tujuan, pindahkan nilai kolom-per-kolom dari row sumber ke row tujuan, lalu `swap_remove` di sumber. `swap_remove` memindahkan row terakhir ke posisi kosong → perbarui `EntityLocation` entity yang berpindah. Semua langkah adalah fungsi murni dari urutan operasi (deterministik).

### 4. Query & pemeriksaan borrow

```rust
for (pos, vel) in world.query::<(&Position, &mut Velocity)>() { /* ... */ }
```

Iterasi:

1. Cari semua archetype yang **superset** dari komponen yang diminta query.
2. Iterasi archetype dalam **urutan `ArchetypeId`** (dibuat berurutan → deterministik). Dalam tiap archetype, iterasi `row` `0..len`.
3. Untuk tiap archetype yang cocok, **downcast kolom sekali** (`as_any().downcast_ref::<TypedColumn<T>>()`) menjadi `&[T]`/`&mut [T]`, lalu iterasi slice bertipe. Biaya downcast teramortisasi per-archetype, bukan per-elemen → jalur ergonomis tetap jalur cepat (invarian §2).

**Borrow query:** sebuah query yang meminta `&mut A` bersama akses lain ke `A` (baik `&A` maupun `&mut A`) adalah alias terlarang dan **ditolak** — sedapat mungkin lewat sistem tipe, selebihnya lewat assert saat konstruksi query dengan pesan yang menyebut komponennya (STD-0008).

### 5. Batas `unsafe`

Kode pengguna tidak pernah butuh `unsafe` (STD-0004). `unsafe` internal diperbolehkan namun **dikurung di lapisan storage** — khususnya untuk memperoleh `&mut` ke kolom-kolom berbeda secara serentak dalam satu query tuple (disjoint borrow). Setiap blok `unsafe` internal wajib menyertakan argumen keamanan dan tercakup oleh tes.

### 6. Determinisme (bukti STD-0005)

Semua sumber urutan bersifat murni-fungsi-dari-operasi: free-list LIFO, `ComponentId` berurutan, `ArchetypeId` berurutan, iterasi row `0..len`. Efek samping yang perlu dicatat: setelah `despawn`, `swap_remove` mengubah urutan row relatif terhadap urutan insert — tetap deterministik, tetapi bukan urutan insert. Ini konsisten antar-run untuk urutan operasi yang sama.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| Sparse-set-only (gaya EnTT) | Mutasi struktural murah | Iterasi multi-komponen kurang cache-friendly | Bertabrakan dengan invarian performa iterasi |
| Hybrid archetype + sparse-set (gaya bevy) | Cepat untuk komponen yang sering berubah | Kompleksitas besar untuk M-1 | Ditunda; abstraksi `Column`/storage menyisakan ruang menambahkannya nanti |
| Kolom blob byte mentah + offset manual | Kontrol memori maksimal | Butuh banyak `unsafe` internal | Ditunda sampai profiling membuktikan perlu; kolom bertipe sudah cache-friendly |
| Registrasi komponen eksplisit | Id numerik stabil, mudah divalidasi | Boilerplate; menggerus ergonomi | Registrasi otomatis + serialisasi by-name mencapai tujuan yang sama |
| Entity id `u32` dikemas | Handle setengah ukuran | Batas ~16 juta entity & 256 reuse/slot | Batas terlalu ketat untuk library tujuan-umum |

## Dampak

- **Kompatibilitas / migrasi:** tidak ada — ini fondasi pertama, belum ada API publik sebelumnya. Menetapkan garis dasar untuk versi berikutnya.
- **Keamanan / izin / provenance:** generational index menegakkan keamanan referensi (STD-0007); `unsafe` dikurung & terdokumentasi di lapisan storage.
- **Konsekuensi pada invarian:** memperkuat *ergonomis = cepat* (kolom bertipe, downcast teramortisasi), *determinisme by construction* (semua urutan deterministik), *struktural aman* (validasi generasi), dan *standalone core* (tanpa dependensi). Menyiapkan *portabilitas data* lewat serialisasi by-name.

## Pertanyaan terbuka

- Caching *edge* graf archetype (mempercepat perpindahan archetype berulang) — optimasi; ditunda sampai profiling.
- Apakah urutan row pasca-`swap_remove` perlu dinormalisasi untuk snapshot yang stabil-terhadap-riwayat? → catat sebagai RN bila jadi soal saat milestone snapshot.
- Model akses paralel untuk query (M-2) — di luar lingkup RFC ini.

## Keputusan

Diterima. Lihat [ADR-0002](../ADR/ADR-0002-core-storage-architecture.md).
