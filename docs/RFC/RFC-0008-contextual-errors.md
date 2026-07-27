# RFC-0008: Error berkonteks

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-27
- **Milestone:** M-7 (Contextual Errors)
- **ADR terkait:** [ADR-0008](../ADR/ADR-0008-contextual-errors.md)

## Ringkasan

Menambahkan tipe error **`EcsError`** dan membuat kegagalan runtime **menyebut komponen/entity yang terlibat**, mengaktifkan **STD-0008** dan mewujudkan prinsip Philosophy §3 ("pesan error mengajari, bukan menyalahkan"). Dua permukaan yang ditargetkan STD-0008 diperbaiki: **konflik borrow query** dan **komponen tak terdaftar**.

## Motivasi

Saat ini beberapa kegagalan bersifat diam (`insert` ke entity mati diabaikan), berbasis `Option` tanpa konteks, atau `panic` dengan pesan generik (mis. alias `query_pair` menyebut "A dan B", bukan tipe konkret). STD-0008 menuntut pesan yang menyebut entity/komponen/sistem, dapat diperiksa lewat tes.

## Usulan rinci

### 1. Tipe `EcsError`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EcsError {
    /// Sebuah komponen diminta sebagai `&mut` sekaligus akses lain dalam satu query.
    QueryConflict { component: &'static str },
    /// Komponen tak terdaftar untuk operasi yang membutuhkannya (mis. snapshot).
    ComponentNotRegistered { component: &'static str },
}
```

Mengimplementasikan `Display` (pesan yang menyebut nama tipe komponen) dan `std::error::Error` — tanpa dependensi eksternal (STD-0003).

### 2. Konflik borrow query

Alias `&mut` ke komponen yang sama dalam satu query adalah **bug program** → tetap `panic` (fail-fast, seperti index-out-of-bounds), tetapi pesannya kini **menyebut tipe komponen** via `type_name::<T>()`:

```text
konflik query: komponen `game::Position` diminta &mut bersama akses lain
```

### 3. Komponen tak terdaftar (snapshot)

`World::snapshot()` yang ada tetap **lunak** (melewati komponen tak-terdaftar-serializable). Ditambah varian ketat:

```rust
impl World {
    pub fn try_snapshot(&self) -> Result<Snapshot, EcsError>;
}
```

`try_snapshot` mengembalikan `Err(ComponentNotRegistered { component })` — **menyebut nama tipe** — bila ada entity hidup dengan komponen yang belum di-`register_serializable`, mencegah kehilangan data diam.

Untuk itu, `ComponentRegistry` menyimpan **nama tipe** setiap komponen (bukan hanya tipe serializable), sehingga komponen apa pun dapat dinamai dalam error.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| Ubah semua operasi jadi `Result` | Konsisten, tak ada panic | Perubahan breaking besar; `insert`/`get` jadi bertele-tele | Berlebihan; kegagalan berbeda pantas penanganan berbeda |
| Konflik query jadi `Result` bukan panic | Tak panik | Konflik adalah bug program; `Result` menyebar noise ke jalur benar | Panic fail-fast lebih tepat, asalkan berkonteks |
| Snapshot lunak saja (tanpa `try_snapshot`) | Sederhana | Kehilangan data diam saat komponen tak terdaftar | `try_snapshot` menutup celah tanpa memaksa semua pemanggil |

## Dampak

- **Kompatibilitas / migrasi:** aditif. `EcsError` & `try_snapshot` baru; `snapshot()`/`query_pair` mempertahankan perilaku (pesan panic diperkaya).
- **Keamanan / provenance:** pesan berkonteks memudahkan audit & debug; tetap tanpa `unsafe`.
- **Konsekuensi pada invarian:** mengaktifkan STD-0008; memperkuat prinsip "pesan error mengajari".

## Pertanyaan terbuka

- Perlukah `insert`/`remove` mengembalikan `Result` untuk entity mati (kini diam)? → dipertimbangkan bila jadi sumber bug; RN bila perlu.
- Menyertakan `Entity` (index+generation) dalam varian error tertentu → ditambah saat operasi ber-entity yang gagal diperkenalkan.

## Keputusan

Diterima. Lihat [ADR-0008](../ADR/ADR-0008-contextual-errors.md).
