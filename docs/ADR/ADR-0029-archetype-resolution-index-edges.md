# ADR-0029: Index lookup + edge transisi untuk resolusi archetype

- **Status:** **Rejected** — premis dibantah pengukuran (lihat RFC-0029)
- **Tanggal:** 2026-07-29
- **RFC terkait:** [RFC-0029](../RFC/RFC-0029-archetype-resolution-index-edges.md)

> **DITOLAK (2026-07-29):** implementasi diukur; resolusi archetype bukan
> bottleneck spawn (alokasi dominan; scan linear tak bite bahkan di 4096
> archetype). Di-revert per YAGNI. Temuan di [RN-0003](../RN/RN-0003-competitive-benchmark.md).

## Konteks

`find_or_create_archetype` scan linear O(n_archetypes) per mutasi struktural →
degradasi super-linear di world dengan banyak archetype (sisa celah spawn RN-0003).
Archetype **append-only** (tak pernah dihapus) → struktur pencari tak pernah basi.

## Keputusan

1. **Index** `HashMap<Box<[ComponentId]>, usize>` (FxHash 0-dep) → `find_or_create`
   O(1). `Box<[_]>: Borrow<[_]>` memungkinkan lookup tanpa alokasi kunci.
2. **Edge** `add_edges`/`remove_edges: HashMap<(archetype, ComponentId), usize>` di
   level `World` → insert/remove tunggal melompati konstruksi+sort id pada transisi
   berulang. Tanpa `RefCell` (map di `World`, bukan `Archetype`).
3. **Tanpa invalidasi** — append-only membuat entri permanen valid.
4. **Determinisme dijaga** — vektor `archetypes` tetap push-order; struktur baru
   hanya lookup. Diverifikasi oracle + miri.
5. **Benchmark W6 (banyak archetype)** ditambah untuk membuktikan O(1) secara
   empiris; existing tak boleh regresi.

## Konsekuensi

**Positif:**

- Spawn/insert/remove O(1) resolusi archetype → skala di world kompleks.
- Edge memangkas alokasi pada churn.
- Aditif internal — aman pasca-1.0.

**Negatif / biaya:**

- Dua HashMap kecil per `World` (memori proporsional archetype/edge).
- Sedikit overhead hash pada world ber-archetype-sedikit (dapat diabaikan).

**Netral:**

- API & hasil identik; tak menyentuh determinisme/keamanan.

## Alternatif yang ditolak

- **Scan linear** — degradasi O(n).
- **SipHash** — mahal per-mutasi; FxHash cukup & 0-dep.
- **Edge dalam `Archetype`** — butuh interior mutability; map `World` lebih bersih.

Rincian di [RFC-0029](../RFC/RFC-0029-archetype-resolution-index-edges.md).
