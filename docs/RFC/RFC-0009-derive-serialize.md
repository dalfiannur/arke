# RFC-0009: `derive(Serialize)` tanpa dependensi eksternal

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-28
- **Milestone:** M-8 (Derive Serialize)
- **ADR terkait:** [ADR-0009](../ADR/ADR-0009-derive-serialize.md)

## Ringkasan

Menambahkan `#[derive(Serialize)]` untuk menghilangkan boilerplate implementasi [`Serialize`](RFC-0007-world-snapshot.md). Proc-macro Rust wajib berada di crate terpisah; agar janji **"0 dependensi eksternal"** tetap utuh, crate derive **`arke-derive` ditulis tangan memakai hanya `proc_macro` bawaan kompiler** (bukan `syn`/`quote`). Sebagai prasyarat, `Serialize` diimplementasikan untuk tipe primitif + `Vec`/`Option`. Repo menjadi **workspace** dua-crate.

## Motivasi

M-6 menuntut setiap komponen serializable mengimplementasikan `Serialize` secara manual (menyusun `Value` per field). Untuk struct dengan banyak field ini melelahkan dan rawan salah. `derive` menghasilkannya otomatis dari definisi struct.

Tantangan: `syn`+`quote` (cara konvensional menulis proc-macro) adalah **dependensi eksternal** yang bertabrakan dengan invarian *standalone core* (STD-0003) dan identitas proyek.

## Usulan rinci

### 1. Impl `Serialize` untuk tipe dasar (prasyarat)

`derive` memanggil `to_value`/`from_value` tiap field secara rekursif; maka tipe field harus `Serialize`. Ditambahkan di core (tanpa dep):

| Tipe | `Value` |
| --- | --- |
| `i8..i64`, `u8..u64`, `usize`/`isize` | `Int` (dengan pengecekan rentang saat `from_value`) |
| `f32`, `f64` | `Float` |
| `bool` | `Bool` |
| `char`, `String` | `Text` |
| `Vec<T: Serialize>` | `List` |
| `Option<T: Serialize>` | `Null` atau nilai `T` |

Aksesor `Value` (`get`, `as_map`, `as_list`, `as_int`) dijadikan **publik** agar kode hasil-derive (dan impl manual pengguna) dapat memakainya.

### 2. Crate `arke-derive` (0 dependensi)

Crate proc-macro (`[lib] proc-macro = true`) yang **hanya** memakai `proc_macro` bawaan — parsing `TokenStream` manual (lewati atribut & visibilitas, temukan `struct`, nama, dan field). Tidak ada `syn`/`quote`. `cargo tree` untuk `arke-derive` menampilkan **nol** dependensi crates.io.

### 3. Bentuk yang didukung

| Bentuk | Kode hasil |
| --- | --- |
| `struct S { a, b }` (field bernama) | `to_value` → `Value::Map` berkunci nama field; `from_value` membaca per kunci |
| `struct S(A, B)` (tuple) | `to_value` → `Value::List` terurut; `from_value` membaca per indeks |
| `struct S;` (unit) | `to_value` → `Value::Null` |

**Ditunda:** enum, generic, union — `derive` memancarkan `compile_error!` yang jelas untuk bentuk tak didukung.

### 4. Workspace & re-export

Repo menjadi workspace: paket `arke` (root) + anggota `arke-derive`. `arke` bergantung pada `arke-derive` (path + versi) dan me-*re-export* derive-nya: `pub use arke_derive::Serialize;` — berbagi nama dengan trait `Serialize` (namespace berbeda: makro vs tipe), sehingga `use arke::Serialize;` membawa keduanya (pola serde).

### 5. STD-0003: klarifikasi

`arke-derive` adalah crate **first-party** di dalam workspace dan **bebas dependensi crates.io**. Janji STD-0003 ("tanpa dependensi eksternal/pihak-ketiga") tetap utuh. Pemeriksaan CI standalone diperbarui: menolak dependensi pihak-ketiga, tetapi mengizinkan `arke`/`arke-derive`.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| `syn`+`quote` di balik feature `derive` | Robust, ergonomis, standar ekosistem | Membawa dep eksternal saat fitur aktif | Bertabrakan dengan identitas "0 dep eksternal" |
| Tanpa derive (tetap manual) | Nol kode baru | Boilerplate melelahkan | Ergonomi buruk untuk snapshot |
| Bound `Serialize` pada `Component` | Otomatis | Memaksa semua komponen serializable | Ditolak sejak RFC-0007 |

## Dampak

- **Kompatibilitas / migrasi:** aditif. `derive` opsional; impl manual tetap sah. Repo jadi workspace (perubahan struktur, bukan API).
- **Publikasi:** `arke-derive` harus dipublikasikan ke crates.io lebih dulu, lalu `arke` versi baru bergantung padanya (`arke-derive = "x"`). Rilis terkoordinasi.
- **Konsekuensi pada invarian:** menjaga *standalone core* (STD-0003) via proc-macro 0-dep; memperkuat *ergonomis = cepat*.

## Pertanyaan terbuka

- `derive` untuk enum & generic → milestone lanjutan.
- Atribut field (mis. rename, skip) → milestone ergonomi.
- Tipe field dengan koma tingkat-atas di generik (`HashMap<K, V>`): parser melacak kedalaman `<>`; tipe eksotis (fn pointer `->`) mungkin tak didukung → didokumentasikan.

## Keputusan

Diterima. Lihat [ADR-0009](../ADR/ADR-0009-derive-serialize.md).
