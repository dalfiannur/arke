# ADR-0024: Resolusi komponen cepat (hasher `TypeId` + threading cid)

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-28
- **RFC terkait:** [RFC-0024](../RFC/RFC-0024-fast-component-resolution.md)

## Konteks

RN-0003: `spawn` ~3.5× lebih lambat dari hecs. Biaya per-spawn didominasi lookup `HashMap<TypeId, ComponentId>` — SipHash pada `TypeId` (128-bit) boros; `spawn_bundle` ~4 lookup/spawn.

## Keputusan

Kami memilih:

1. **Hasher `TypeId` FxHash** (`BuildHasherDefault<TypeIdHasher>`, ~5 baris, 0-dep) untuk `ComponentRegistry.ids` — `TypeId` sudah hash berkualitas; SipHash tak perlu.
2. **Threading `ComponentId` bundle** dari `ids()` ke `push(…, cids)` — hapus `registry.get` ulang di `push`.

## Konsekuensi

**Positif:**

- spawn ~2.4× lebih cepat (~76 → ~32 ns/op), **mengalahkan bevy_ecs**. iter2 turun ke ~0.95 (**setara hecs**). get membaik (~25 → ~18–20). arke kini **kompetitif** di ketiga beban inti (RN-0003).
- Internal — API & hasil identik; determinisme terjaga (lookup titik, STD-0005).
- Tanpa `unsafe` baru.

**Negatif / biaya:**

- Hasher FxHash tanpa DoS-resistance — **tak relevan** (kunci `TypeId` internal, bukan input pengguna).

**Netral / catatan:**

- `get` masih ~1.4× hecs; cache archetype HashMap → target lanjutan.

## Alternatif yang ditolak

- **Tetap SipHash** — sumber lambatnya spawn.
- **Dependensi `rustc-hash`/`fxhash`** — melanggar 0-dependensi; hand-roll ~5 baris cukup.

Rincian ada di [RFC-0024](../RFC/RFC-0024-fast-component-resolution.md).
