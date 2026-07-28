# RFC-0024: Resolusi komponen cepat (hasher `TypeId` + threading cid bundle)

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-28
- **Graduasi dari:** [RN-0003](../RN/RN-0003-competitive-benchmark.md)
- **ADR terkait:** [ADR-0024](../ADR/ADR-0024-fast-component-resolution.md)

## Ringkasan

Mempercepat **resolusi tipe komponen → `ComponentId`** (jalur panas spawn/insert/query) dengan (1) **hasher `TypeId` cepat** (FxHash) menggantikan SipHash default, dan (2) **threading `ComponentId` bundle** dari `Bundle::ids` ke `Bundle::push` (menghapus lookup registry ulang). Tanpa perubahan API; determinisme terjaga (lookup titik saja).

## Motivasi

RN-0003: `spawn` ~3.5× & `get` ~1.9× lebih lambat dari hecs. Profil menunjukkan biaya per-spawn didominasi **lookup `HashMap<TypeId, ComponentId>`** — SipHash pada kunci `TypeId` (128-bit) **boros**, padahal `TypeId` **sudah** hash berkualitas. `spawn_bundle` melakukan ~4 lookup (register×2 di `ids`, get×2 di `push`).

## Usulan rinci

### 1. Hasher `TypeId` cepat (FxHash)

`ComponentRegistry.ids` memakai `HashMap<TypeId, ComponentId, BuildHasherDefault<TypeIdHasher>>` dengan hasher FxHash (dipakai rustc), ~5 baris, 0-dependensi:

```rust
fn write_u64(&mut self, i: u64) { self.0 = (self.0.rotate_left(5) ^ i).wrapping_mul(FX_K); }
```

`TypeId` hash lewat `write_u64`/`write_u128` → mix murah, bukan SipHash penuh. **Deterministik** (tanpa seed acak); hanya untuk *lookup titik* (register/get) — urutan `ComponentId` tetap dari urutan insert (STD-0005), tak dari iterasi `HashMap`.

### 2. Threading cid bundle

`Bundle::push(self, archetype, cids: &[ComponentId])` menerima id dari `ids()` (urut tuple) alih-alih `registry.get::<T>()` ulang → menghapus 2 lookup registry per `spawn_bundle`.

## Hasil (N=100k, Ryzen 5 8645HS, median)

| Beban | Baseline | RFC-0023 | **RFC-0024** | hecs | bevy_ecs |
| --- | ---: | ---: | ---: | ---: | ---: |
| iter2 | ~2.0 | ~1.3 | **~0.95** | ~0.85 | ~1.1 |
| spawn | ~76 | ~76 | **~32** | ~18 | ~54 |
| get | ~25 | ~25 | **~18–20** | ~14 | ~12 |

- **spawn ~2.4× lebih cepat**; kini **mengalahkan bevy_ecs**, ~1.8× hecs.
- **iter2 turun lagi** ke ~0.95 (hasher mempercepat resolusi cid query) → **setara hecs, mengalahkan bevy_ecs**.
- **get membaik** (~25 → ~18–20) — `world.get` juga me-resolve cid.

arke kini **kompetitif** di ketiga beban inti (mengalahkan bevy_ecs pada iter2 & spawn; dalam ~1.4–1.8× hecs). **Klaim "ergonomis = cepat" jauh lebih tervalidasi.**

## Dampak

- **Kompatibilitas:** internal; API & hasil identik.
- **Keamanan:** tanpa `unsafe` baru; hasher aman (FxHash, tanpa DoS-resistance — tak relevan untuk kunci `TypeId` internal).
- **Konsekuensi pada invarian:** determinisme terjaga (STD-0005); memperkuat *ergonomis = cepat*.

## Pertanyaan terbuka

- `get` masih ~1.4× hecs (indireksi entity→lokasi→kolom + downcast) — optimasi lanjutan.
- Cache HashMap untuk `find_or_create_archetype` (relevan world ber-banyak-archetype).
- Regresi-guard performa di CI.

## Keputusan

Diterima. Lihat [ADR-0024](../ADR/ADR-0024-fast-component-resolution.md).
