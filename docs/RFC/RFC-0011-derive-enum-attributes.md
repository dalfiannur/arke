# RFC-0011: `derive(Serialize)` untuk enum + atribut field

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-28
- **Milestone:** M-10 (Derive: enum & attributes)
- **ADR terkait:** [ADR-0011](../ADR/ADR-0011-derive-enum-attributes.md)

## Ringkasan

Memperluas `#[derive(Serialize)]` (RFC-0009) untuk mendukung **enum** (representasi *externally-tagged*) dan **atribut field** `#[serialize(skip)]` / `#[serialize(rename = "...")]` pada struct field-bernama. Tetap ditulis tangan dengan `proc_macro` bawaan — **0 dependensi crates.io**.

## Motivasi

M-8 hanya menurunkan `Serialize` untuk struct. Enum umum dipakai untuk state komponen (mis. `enum State { Idle, Running(u32) }`). Atribut field memberi kontrol: melewatkan field non-serializable/derivable, atau memakai kunci JSON berbeda dari nama field Rust.

## Usulan rinci

### 1. Representasi enum (externally-tagged)

| Bentuk varian | `Value` |
| --- | --- |
| Unit `A` | `Text("A")` |
| Tuple `A(x, y)` | `Map([("A", List([x, y]))])` |
| Struct `A { x, y }` | `Map([("A", Map([("x", x), ("y", y)]))])` |

`from_value` menerima `Text(nama)` untuk varian unit, dan `Map` satu-entri `{nama: payload}` untuk varian berdata; nama tak dikenal → `None`.

> *Externally-tagged* dipilih karena eksplisit, mudah dibaca, dan tak ambigu (nama varian selalu ada). Ini format permanen v1; perubahannya butuh versi baru.

### 2. Atribut field (struct field-bernama)

```rust
#[derive(Serialize)]
struct Config {
    #[serialize(rename = "n")]
    name: String,
    #[serialize(skip)]
    cache: Vec<u8>,
}
```

- **`rename = "kunci"`** — memakai `"kunci"` sebagai kunci `Map`, bukan nama field Rust.
- **`skip`** — field tak diserialisasi; saat `from_value` diisi `Default::default()` (field ber-`skip` **wajib** `Default`).

Berlaku untuk **struct field-bernama**. Tuple struct & field varian enum tidak menerima atribut (ditunda).

### 3. Bentuk tak didukung

Generic, union, dan atribut selain `skip`/`rename` memancarkan `compile_error!` yang jelas.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| Enum *internally-tagged* (`{"type":"A", ...}`) | Ringkas untuk struct-variant | Ambigu untuk tuple/unit; butuh field tag | Externally-tagged lebih seragam |
| Enum *adjacently-tagged* (`{"tag":.., "content":..}`) | Eksplisit | Lebih verbose | Externally-tagged cukup & umum |
| `skip` tanpa `Default` (butuh nilai eksplisit) | Tak ada bound | Tak bisa merekonstruksi | `Default` konvensi serde, ergonomis |
| Pustaka atribut (syn) | Robust | Dependensi eksternal | Melanggar 0-dep |

## Dampak

- **Kompatibilitas / migrasi:** aditif ke derive; snapshot format enum baru (berversi via `schema_version` snapshot).
- **Keamanan:** tetap tanpa `unsafe`, 0 dependensi crates.io.
- **Konsekuensi pada invarian:** memperkuat *ergonomis = cepat* & *portabilitas data*.

## Pertanyaan terbuka

- Atribut pada field tuple struct & varian enum → milestone lanjutan.
- Atribut `default` eksplisit, `rename_all` di level tipe → lanjutan.
- Varian enum dengan discriminant eksplisit (`= 1`) berdata → tak relevan (Rust melarang campuran); unit-with-discriminant didukung.

## Keputusan

Diterima. Lihat [ADR-0011](../ADR/ADR-0011-derive-enum-attributes.md).
