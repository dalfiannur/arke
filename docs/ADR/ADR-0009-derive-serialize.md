# ADR-0009: `derive(Serialize)` tanpa dependensi eksternal

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-28
- **RFC terkait:** [RFC-0009](../RFC/RFC-0009-derive-serialize.md)

## Konteks

Implementasi `Serialize` manual (RFC-0007) melelahkan untuk struct berfield banyak. `derive` menghilangkannya, tetapi proc-macro Rust wajib di crate terpisah dan konvensionalnya memakai `syn`+`quote` — dependensi eksternal yang bertabrakan dengan invarian *standalone core* (STD-0003) dan identitas "0 dependensi eksternal".

## Keputusan

Kami memilih:

1. **Crate `arke-derive` ditulis tangan memakai hanya `proc_macro` bawaan** (bukan `syn`/`quote`) → nol dependensi crates.io.
2. **Impl `Serialize` untuk primitif + `Vec`/`Option`** di core (prasyarat derive rekursif); aksesor `Value` (`get`/`as_map`/`as_list`/`as_int`) dijadikan publik.
3. **`derive` mendukung struct field-bernama (→ `Map`), tuple (→ `List`), dan unit (→ `Null`)**; bentuk lain memancarkan `compile_error!`.
4. Repo menjadi **workspace** dua-crate; `arke` me-*re-export* `arke_derive::Serialize`.
5. **STD-0003 diklarifikasi**: `arke-derive` first-party & 0-dep-crates.io tidak melanggar "tanpa dependensi pihak-ketiga"; pemeriksaan CI diperbarui.

## Konsekuensi

**Positif:**

- Ergonomi snapshot meningkat drastis tanpa boilerplate.
- Janji "0 dependensi eksternal" tetap utuh — bahkan derive bebas syn/quote.
- Impl `Serialize` primitif berguna juga untuk impl manual.

**Negatif / biaya:**

- Parser `TokenStream` tulis-tangan lebih banyak kode & rapuh untuk tipe eksotis (didokumentasikan).
- Rilis jadi terkoordinasi: `arke-derive` dipublikasikan lebih dulu, `arke` bergantung padanya.
- Enum/generic/atribut field belum didukung.

**Netral / catatan:**

- Repo kini workspace; struktur build berubah, API publik tidak.
- Pemeliharaan proc-macro tulis-tangan adalah komitmen jangka panjang.

## Alternatif yang ditolak

- **`syn`+`quote` di balik feature** — membawa dependensi eksternal saat derive aktif.
- **Tetap manual (tanpa derive)** — ergonomi buruk.
- **Bound `Serialize` pada `Component`** — memaksa semua komponen serializable (ditolak sejak RFC-0007).

Rincian pertimbangan ada di [RFC-0009](../RFC/RFC-0009-derive-serialize.md).
