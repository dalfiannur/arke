# ADR-0023: Iterasi query berbasis-indeks (kolom)

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-28
- **RFC terkait:** [RFC-0023](../RFC/RFC-0023-columnar-query-iteration.md)

## Konteks

RN-0003 (benchmark kompetitif) menemukan iterasi query arke ~2.2× lebih lambat dari hecs. Loop lockstep `match (a.next(), b.next())` membangun & mencocokkan tuple `Option` per-elemen — percabangan yang mencegah vektorisasi.

## Keputusan

Kami memilih:

1. **`QueryTerm` berbasis fetch+get per-indeks** (bukan iterator): `fetch(archetype, col) -> Fetch<'_>` (slice kolom/entity) + `get(&mut fetch, i) -> Item`.
2. **Loop iterasi `for i in 0..len`** mengindeks tiap term — menghapus percabangan `Option` per-elemen, memungkinkan LLVM meng-elide bounds-check & memvektorisasi.
3. Term meminjam **variabel fetch berbeda** per-indeks → borrow disjoint, `&mut` tetap sound.

## Konsekuensi

**Positif:**

- iter2 ~1.5× lebih cepat (~2.0 → ~1.3 ns/op) → **setara bevy_ecs**, mempersempit gap ke hecs. Memperbaiki *ergonomis = cepat* (RN-0003).
- Internal saja — API publik & hasil identik; determinisme terjaga.
- `unsafe` `&mut` tetap terkurung, soundness sama (miri-verified).

**Negatif / biaya:**

- `QueryTerm` kini punya GAT `Fetch<'w>` (lifetime lebih rumit di trait internal).

**Netral / catatan:**

- `spawn`/`get` belum dioptimasi (target RN-0003 lanjutan).

## Alternatif yang ditolak

- **`.zip()` bersarang std** — lebih baik dari lockstep manual, tapi bersarang untuk arity>2 rumit di makro; index-based seragam semua arity.
- **Tetap lockstep** — sumber lambatnya jalur panas.

Rincian ada di [RFC-0023](../RFC/RFC-0023-columnar-query-iteration.md).
