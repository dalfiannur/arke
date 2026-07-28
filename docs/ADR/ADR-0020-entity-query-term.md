# ADR-0020: `Entity` sebagai term query

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-28
- **RFC terkait:** [RFC-0020](../RFC/RFC-0020-entity-query-term.md)

## Konteks

`each_cmd` (RFC-0019) tak menghasilkan `Entity`, sehingga pola *despawn-self*/mutasi-per-entity tak dapat diungkapkan. `Entity` tak disimpan di kolom komponen (ada di `entities[row]` archetype), jadi abstraksi `QueryTerm` yang berpusat-kolom tak memuatnya.

## Keputusan

Kami memilih:

1. **Generalisasi `QueryTerm`**: ganti `component_id` → `requirement(world) -> Requirement { Column(cid) | Any | Never }`, dan `iter_shared(col)` → `iter_shared(archetype, col: Option<usize>)`. Memuat term non-kolom secara seragam.
2. **`Entity` sebagai `QueryTerm`/`QueryData`**: `requirement = Any`, iterasi `archetype.entities().iter().copied()` — aman. `access()` **kosong** → tanpa konflik penjadwalan.
3. **Pencocokan berbasis `Requirement`**: `Never` → query kosong; `required_cids` (dari `Column`) untuk alias-check + match; `Any` tak mensyaratkan kolom.

## Konsekuensi

**Positif:**

- Sistem paralel dapat `despawn`/`insert`/`remove` entity yang diiterasi (via command buffer) → nilai penuh RFC-0019.
- **Tanpa `unsafe` baru**; determinisme & urutan identik (STD-0005/0006).
- Term generik menyatu dengan tuple; tak ada ledakan API per-arity.

**Negatif / biaya:**

- Menyentuh trait inti `QueryTerm` (internal, `#[doc(hidden)]`).
- `requirement(world)` dihitung ulang per-term saat resolusi kolom (lookup registry murah).

**Netral / catatan:**

- `Entity` (term) = handle baris; `&Entity` tetap query komponen (degenerate) — overload disengaja, lazim di ECS.

## Alternatif yang ditolak

- **`Entity` marker + jalur paralel terpisah** — duplikasi logika iterasi; tuple campuran sulit.
- **Yield `(Entity, Item)` implisit di `each_cmd`** — kaku; tak berlaku untuk `each`/query umum.
- **`each_with_entity` per-arity** — ledakan API; term generik lebih baik.

Rincian pertimbangan ada di [RFC-0020](../RFC/RFC-0020-entity-query-term.md).
