# RFC-0023: Iterasi query berbasis-indeks (kolom) — optimasi jalur panas

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-28
- **Graduasi dari:** [RN-0003](../RN/RN-0003-competitive-benchmark.md) (benchmark kompetitif)
- **ADR terkait:** [ADR-0023](../ADR/ADR-0023-columnar-query-iteration.md)

## Ringkasan

Mengganti iterasi query dari **lockstep iterator** (`match (a.next(), b.next(), …)`) menjadi **loop berbasis-indeks** atas slice kolom (`for i in 0..len { f((get(a,i), get(b,i), …)) }`). Menghapus percabangan per-elemen (match tuple-`Option`) yang menghambat vektorisasi. **Tanpa perubahan API**, hasil identik; jalur `unsafe` `&mut` **tetap terkurung** & miri-verified.

## Motivasi

RN-0003 menemukan iter2 (`pos += vel`, jalur panas) arke **~2.2× lebih lambat** dari hecs. Hipotesisnya: loop lockstep tuple

```rust
loop { match (a.next(), b.next()) { (Some(a), Some(b)) => f((a, b)), _ => break } }
```

membangun & mencocokkan tuple `Option` tiap elemen — percabangan yang mencegah LLVM auto-vektorisasi, tak seperti iterator berbasis-slice hecs/bevy.

## Usulan rinci

`QueryTerm` diubah dari "kembalikan iterator" menjadi **fetch + get per-indeks**:

```rust
trait QueryTerm {
    type Item<'w>;
    type Fetch<'w>;                                        // slice kolom/entity
    fn fetch(archetype: &Archetype, col: Option<usize>) -> Self::Fetch<'_>;
    fn get<'a>(fetch: &'a mut Self::Fetch<'_>, i: usize) -> Self::Item<'a>;
}
```

- `&T`: `Fetch = &[T]`, `get = &fetch[i]`.
- `&mut T`: `Fetch = &mut [T]` (dari `data_mut_shared`), `get = &mut fetch[i]`.
- `Entity`: `Fetch = &[Entity]`, `get = fetch[i]` (Copy).

Iterasi (single & tuple) menjadi:

```rust
let (mut fa, mut fb) = (A::fetch(arch, ca), B::fetch(arch, cb));
for i in 0..arch.entities().len() {
    f((A::get(&mut fa, i), B::get(&mut fb, i)));   // borrow disjoint per term
}
```

Semua term satu archetype punya panjang sama (= jumlah baris). `get(&mut fa, i)`/`get(&mut fb, i)` meminjam variabel-variabel **berbeda** → tak beralias. Loop `0..len` terhitung → LLVM meng-elide bounds-check & memvektorisasi bila `f` inline.

## Hasil (N=100k, Ryzen 5 8645HS, median beberapa run)

| iter2 (`pos += vel`) | Sebelum | Sesudah |
| --- | ---: | ---: |
| **arke** | ~2.0 ns/op | **~1.3 ns/op** (~1.5× lebih cepat) |
| hecs | ~0.85 | ~0.85 |
| bevy_ecs | ~1.1 | ~1.1 |

arke iter2 dari **~2.2× lebih lambat** dari hecs → **~1.5×**, dan **setara bevy_ecs**. `get`/`spawn` tak tersentuh (target optimasi lain).

## Dampak

- **Kompatibilitas:** internal (`QueryTerm` `#[doc(hidden)]`); API publik query tak berubah, hasil identik.
- **Keamanan:** `unsafe` `&mut` tetap terkurung (`data_mut_shared` → `&mut [T]`, indeks aman); soundness sama (kolom distinct, akses disjoint), diverifikasi miri.
- **Konsekuensi pada invarian:** memperbaiki **ergonomis = cepat** pada jalur panas (RN-0003); determinisme & urutan iterasi identik (STD-0005/0006).

## Pertanyaan terbuka

- Optimasi `spawn` (cache resolusi archetype/kolom) & `get` (indeks kolom) — RN-0003.
- Regresi-guard performa di CI.

## Keputusan

Diterima. Lihat [ADR-0023](../ADR/ADR-0023-columnar-query-iteration.md).
