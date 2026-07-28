# RFC-0026: Seal trait ekstensi (`Bundle`, `QueryData`, `QueryTerm`, `QueryFilter`)

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-28
- **Graduasi dari:** [RN-0004](../RN/RN-0004-jalan-menuju-1.0.md)
- **ADR terkait:** [ADR-0026](../ADR/ADR-0026-seal-extension-traits.md)

## Ringkasan

Menutup (*seal*) empat trait ekstensi — [`Bundle`], [`QueryData`], [`QueryTerm`],
[`QueryFilter`] — sehingga **hanya crate `arke`** yang boleh mengimplementasikannya.
Dicapai lewat *supertrait* penanda di modul privat (`mod sealed`). Ini menghapus
signature internal (`fn push`, `fn fetch`, `fn each_cached`, …) dari kontrak
publik **sebelum 1.0**, membuka ruang evolusi aditif pasca-1.0.

## Motivasi

1.0 membekukan API publik (semver). Sebuah **trait terbuka** membekukan pula
*seluruh signature method*-nya — termasuk yang murni detail implementasi.

Analisis permukaan (RN-0004):

| Trait | Signature memakai tipe | Bisa di-impl downstream hari ini? |
| --- | --- | --- |
| [`QueryData`] | `World`, `QueryState`, `Access`, `QueryFilter` — **semua publik** | **YA** |
| [`QueryFilter`] | `World`, `ComponentId` — **semua publik** | **YA** |
| [`Bundle`] | `Archetype`, `ComponentRegistry` — **privat** | Tidak (insidental) |
| [`QueryTerm`] | `Archetype`, `Requirement` — **privat** | Tidak (insidental) |

`QueryData` & `QueryFilter` **benar-benar dapat diimplementasi pengguna sekarang**
— membekukannya di 1.0 mengunci `fn each_cached`/`fn resolve` selamanya. `Bundle`
& `QueryTerm` kebetulan tak-dapat-diimpl karena tipe privat bocor di signature —
tapi itu jaminan **insidental**, bukan **disengaja**. Seal membuat keempatnya
tertutup secara **eksplisit & legibel**.

`Component` (blanket `impl<T: 'static + Send>`) dan `Serialize` (sengaja
`derive`-able / impl-able pengguna) **tidak** disegel.

## Usulan rinci

Modul penanda privat-crate:

```rust
// lib.rs
pub(crate) mod sealed {
    pub trait BundleSealed {}
    pub trait QueryDataSealed {}
    pub trait QueryTermSealed {}
    pub trait QueryFilterSealed {}
}
```

Tiap trait publik menambah supertrait penanda:

```rust
pub trait QueryData: crate::sealed::QueryDataSealed { /* … */ }
pub trait QueryFilter: crate::sealed::QueryFilterSealed { /* … */ }
pub trait Bundle: crate::sealed::BundleSealed { /* … */ }
pub trait QueryTerm: crate::sealed::QueryTermSealed { /* … */ }
```

Tiap **impl** trait di crate mendapat impl penanda paralel (via makro yang sudah
ada untuk tuple):

| Penanda | Di-impl untuk |
| --- | --- |
| `BundleSealed` | tuple arity 1–8 |
| `QueryDataSealed` | `&T`, `&mut T`, `Entity`, tuple arity 2–6 |
| `QueryTermSealed` | `&T`, `&mut T`, `Entity` |
| `QueryFilterSealed` | `With<T>`, `Without<T>`, `()`, tuple arity 1–4 |

Downstream tak dapat menamai `crate::sealed::*` (modul privat) → tak dapat
memenuhi *bound* supertrait → **tak dapat mengimpl** trait publik. Method internal
boleh berubah pasca-1.0 tanpa mematahkan semver.

Karena supertrait privat pada trait publik, lint `private_bounds` mungkin menyala;
bila ya, di-`allow` secara sadar (memang disengaja) — konsisten dengan
`#[allow(private_interfaces)]` yang sudah dipakai.

## Verifikasi (TDD)

Sealing adalah properti **waktu-kompilasi** → diuji dengan **doc-test
`compile_fail`** (bawaan rustdoc, **0 dependensi** — selaras STD-0003; tak perlu
`trybuild`). Tiap uji memuat impl downstream yang **lengkap** (semua method) untuk
`QueryFilter` & `QueryData` (dua trait yang benar-benar terbuka):

- **RED:** sebelum seal, impl lengkap **kompilasi** → blok `compile_fail` *gagal*
  (rustdoc: "seharusnya gagal, tapi kompilasi sukses"). Disaksikan.
- **GREEN:** setelah seal, impl gagal pada *bound* `…Sealed` yang tak terpenuhi →
  `compile_fail` lolos. Karena impl-nya lengkap, satu-satunya sebab gagal adalah
  seal (bukan method hilang → bukan false-green).

## Dampak

- **Kompatibilitas:** **BREAKING** untuk siapa pun yang mengimpl trait ini di luar
  `arke` — secara praktik tak ada (`QueryTerm`/`Bundle` sudah tak-dapat-diimpl;
  `QueryData`/`QueryFilter` tak diintensikan untuk impl eksternal). Ditujukan ke
  **0.6.0** (rilis breaking bentuk-API menuju 1.0).
- **Keamanan/determinisme:** tak terpengaruh (murni visibilitas trait).
- **Manfaat 1.0:** memungkinkan penambahan aditif pasca-1.0 (events,
  change-detection, dsb.) tanpa membekukan detail implementasi.

## Alternatif yang dipertimbangkan

| Alternatif | Mengapa tidak |
| --- | --- |
| Biarkan terbuka | Membekukan signature internal di 1.0; menutup ruang evolusi |
| Andalkan tipe privat yang bocor | Jaminan insidental, tak legibel; `QueryData`/`QueryFilter` tetap terbuka |
| `trybuild` untuk uji compile-fail | Dependensi dev eksternal — melanggar etos 0-dependensi (STD-0003) |

## Keputusan

Diterima. Lihat [ADR-0026](../ADR/ADR-0026-seal-extension-traits.md).

[`Bundle`]: ../../src/bundle.rs
[`QueryData`]: ../../src/query.rs
[`QueryTerm`]: ../../src/query.rs
[`QueryFilter`]: ../../src/query.rs
