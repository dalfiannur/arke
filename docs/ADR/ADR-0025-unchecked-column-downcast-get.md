# ADR-0025: Downcast kolom tak-tercek pada `World::get`

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-28
- **RFC terkait:** [RFC-0025](../RFC/RFC-0025-unchecked-column-downcast-get.md)

## Konteks

RN-0003: `get` ~1.5× lebih lambat dari hecs. `downcast_ref::<TypedColumn<T>>()` per-`get` melakukan panggilan virtual `as_any()` + perbandingan `TypeId` — padahal downcast **dijamin** sukses (kolom pada `column_index(cid)` untuk `T` selalu `TypedColumn<T>`, invarian M-1).

## Keputusan

Kami memilih:

1. **Cast tak-tercek terkurung** `&dyn Column → &TypedColumn<T>` pada `World::get`, dengan `// SAFETY` bersandar invarian M-1.
2. **`#[allow(unsafe_code)]` per-fungsi** (bukan modul) → sisa `world.rs` tetap `unsafe`-denied. Lokasi confined-unsafe keempat.
3. Diverifikasi **miri** (uji model-based melakukan `get` acak lintas tipe).

## Konsekuensi

**Positif:**

- get ~1.8× lebih cepat (~20 → ~11 ns/op) → **mengalahkan hecs & bevy_ecs**. arke kini kompetitif/menang di ketiga beban inti (RN-0003).
- Internal — API & hasil identik.

**Negatif / biaya:**

- Menambah **satu** `unsafe` (terkurung per-fungsi) — memperluas permukaan yang harus dijaga miri.
- Kebenaran bersandar invarian M-1 (kolom per-`ComponentId`) — tetap dijaga uji.

**Netral / catatan:**

- Jalur pengguna tetap bebas `unsafe` (STD-0004).

## Alternatif yang ditolak

- **`downcast_ref_unchecked`** — nightly-only.
- **Tetap `downcast_ref` bercek** — sumber lambatnya `get`.
- **Layout storage lebih datar** — perubahan desain besar.

Rincian ada di [RFC-0025](../RFC/RFC-0025-unchecked-column-downcast-get.md).
