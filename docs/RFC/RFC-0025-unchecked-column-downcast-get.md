# RFC-0025: Downcast kolom tak-tercek pada `World::get` (unsafe terkurung)

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-28
- **Graduasi dari:** [RN-0003](../RN/RN-0003-competitive-benchmark.md)
- **ADR terkait:** [ADR-0025](../ADR/ADR-0025-unchecked-column-downcast-get.md)

## Ringkasan

Mengganti `downcast_ref::<TypedColumn<T>>()` (bercek `TypeId` + panggilan virtual `as_any`) pada `World::get` dengan **cast tak-tercek terkurung** dari `&dyn Column` ke `&TypedColumn<T>`. Aman karena kolom pada posisi `column_index(cid)` **selalu** `TypedColumn<T>` (invarian M-1). Menghapus overhead per-akses pada jalur akses-acak `get`. Diverifikasi miri.

## Motivasi

RN-0003: `get` ~1.5× lebih lambat dari hecs (setelah RFC-0024). Profil: `downcast_ref` per-`get` melakukan (1) panggilan **virtual** `as_any()` (dispatch dinamis) + (2) **perbandingan `TypeId`**. Pada akses-acak (didominasi cache-miss), overhead ini signifikan — padahal hasil downcast **dijamin** sukses.

## Usulan rinci

`col = column_index(cid)` dengan `cid` = `ComponentId` tipe `T`. Kolom pada posisi itu dibuat oleh konstruktor registry sebagai `TypedColumn<T>` (per-`ComponentId`). Maka:

```rust
#[allow(unsafe_code)]                       // terkurung ke fungsi ini
pub fn get<T: Component>(&self, entity: Entity) -> Option<&T> {
    // … resolve meta, generasi, cid, archetype, col …
    let column: &dyn Column = archetype.column(col);
    // SAFETY: kolom pada `col` (dari column_index untuk ComponentId T) SELALU
    // TypedColumn<T> (invarian M-1). Cast erased→konkret melewati cek TypeId
    // yang pasti lolos. Diverifikasi miri.
    let typed = unsafe { &*(column as *const dyn Column as *const TypedColumn<T>) };
    typed.data().get(location.row)
}
```

Cast `*const dyn Column → *const TypedColumn<T>` menjatuhkan vtable (fat→thin), mempertahankan pointer data yang menunjuk `TypedColumn<T>`.

### Konfinemen

`unsafe` dibatasi lewat `#[allow(unsafe_code)]` **per-fungsi** (`get`), bukan modul — sisa `world.rs` tetap `unsafe`-denied. Menambah lokasi confined-unsafe keempat (setelah `storage`/`query`/`schedule`), semuanya dengan `// SAFETY` + miri.

## Hasil (N=100k, Ryzen 5 8645HS, median)

| get (akses acak) | Sebelum | **Sesudah** | hecs | bevy_ecs |
| --- | ---: | ---: | ---: | ---: |
| **arke** | ~20 ns/op | **~11 ns/op** (~1.8× lebih cepat) | ~14 | ~13 |

**get ~1.8× lebih cepat** — dari **kelemahan** arke (~1.5× hecs) menjadi **mengalahkan** hecs & bevy_ecs. arke kini **kompetitif/menang di ketiga beban inti** (iter2 ≈ hecs; spawn & get **menang** vs bevy_ecs, get **menang** vs hecs).

## Dampak

- **Kompatibilitas:** internal; API & hasil `get` identik.
- **Keamanan:** menambah **satu** `unsafe` terkurung (per-fungsi), soundness dari invarian M-1, **diverifikasi miri** (uji model-based melakukan `get` acak lintas tipe). Jalur pengguna tetap bebas `unsafe` (STD-0004).
- **Konsekuensi pada invarian:** memperkuat *ergonomis = cepat*; determinisme tak terpengaruh.

## Alternatif yang dipertimbangkan

| Alternatif | Mengapa tidak |
| --- | --- |
| `Any::downcast_ref_unchecked` | Nightly-only |
| Tetap `downcast_ref` bercek | Sumber lambatnya `get` (virtual + TypeId cmp) |
| Layout storage lebih datar | Perubahan desain besar |

## Keputusan

Diterima. Lihat [ADR-0025](../ADR/ADR-0025-unchecked-column-downcast-get.md).
