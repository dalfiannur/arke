# RFC-0012: `derive(Serialize)` — `rename_all` & atribut level-tipe

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-28
- **Milestone:** M-11 (Derive: rename_all)
- **ADR terkait:** [ADR-0012](../ADR/ADR-0012-derive-rename-all.md)

## Ringkasan

Menambahkan atribut **level-tipe** `#[serialize(rename_all = "...")]` yang menerapkan konvensi penamaan pada **semua** kunci field (struct) dan **nama varian** (enum), plus `#[serialize(rename = "...")]` pada varian enum. Tetap ditulis tangan dengan `proc_macro` bawaan — **0 dependensi crates.io**. Melengkapi atribut field M-10.

## Motivasi

Snapshot sering harus cocok dengan konvensi eksternal (mis. `camelCase` untuk JSON JavaScript, `SCREAMING_SNAKE_CASE` untuk konstanta). Menandai tiap field dengan `rename` melelahkan; `rename_all` di level tipe melakukannya sekaligus.

## Usulan rinci

### 1. Atribut level-tipe

```rust
#[derive(Serialize)]
#[serialize(rename_all = "camelCase")]
struct User { user_name: String, is_active: bool }
// → {"userName": ..., "isActive": ...}
```

### 2. Konvensi yang didukung

| Nilai | `user_name` → | Varian `PlayerJoined` → |
| --- | --- | --- |
| `lowercase` | `username` | `playerjoined` |
| `UPPERCASE` | `USERNAME` | `PLAYERJOINED` |
| `snake_case` | `user_name` | `player_joined` |
| `SCREAMING_SNAKE_CASE` | `USER_NAME` | `PLAYER_JOINED` |
| `kebab-case` | `user-name` | `player-joined` |
| `SCREAMING-KEBAB-CASE` | `USER-NAME` | `PLAYER-JOINED` |
| `camelCase` | `userName` | `playerJoined` |
| `PascalCase` | `UserName` | `PlayerJoined` |

Nama dipecah jadi kata (batas `_`, `-`, dan transisi huruf-kecil→huruf-besar) lalu dirakit ulang sesuai target. Nilai tak dikenal → `compile_error!`.

### 3. Presedensi

`rename` per-field/per-varian **menang** atas `rename_all`. Field ber-`skip` tetap dilewati.

### 4. Cakupan

- Struct field-bernama & field varian-enum bertipe-struct: kunci di-`rename_all`.
- Nama varian enum: di-`rename_all`.
- Tuple struct: tak punya kunci → `rename_all` tak berpengaruh (tak error).

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| Hanya `rename` per-field | Sudah ada (M-10) | Melelahkan untuk banyak field | `rename_all` jauh lebih ringkas |
| Subset konvensi lebih kecil | Kode lebih sedikit | Kurang fleksibel | Set standar (serde) murah diimplementasikan |
| Pustaka case (heck/convert_case) | Robust | Dependensi eksternal | Melanggar 0-dep |

## Dampak

- **Kompatibilitas / migrasi:** aditif. Tanpa `rename_all`, perilaku M-10 tak berubah.
- **Keamanan:** tetap tanpa `unsafe`, 0 dependensi crates.io.
- **Konsekuensi pada invarian:** memperkuat *portabilitas data* (snapshot cocok konvensi eksternal).

## Pertanyaan terbuka

- `rename_all` terpisah untuk field vs varian pada tipe yang sama → lanjutan bila perlu.
- Atribut `default` eksplisit di level field/tipe → lanjutan.

## Keputusan

Diterima. Lihat [ADR-0012](../ADR/ADR-0012-derive-rename-all.md).
