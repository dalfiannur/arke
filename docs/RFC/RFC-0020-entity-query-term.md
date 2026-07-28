# RFC-0020: `Entity` sebagai term query

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-28
- **Milestone:** M-19 (Entity Query Term)
- **ADR terkait:** [ADR-0020](../ADR/ADR-0020-entity-query-term.md)

## Ringkasan

Menjadikan **`Entity`** (nilai) sebuah **term query** yang menghasilkan **handle entity** baris yang sedang diiterasi — mis. `<(Entity, &Health)>::each(...)` atau `System::each_cmd::<(Entity, &Health)>(|(e, h), cmd| …)`. Ini melengkapi command buffer (RFC-0019): sistem paralel kini dapat `despawn`/`insert`/`remove` **entity yang sedang diiterasi** (pola *despawn-self*). Tanpa `unsafe` baru; `Entity` tak menyumbang akses → tanpa konflik penjadwalan; determinisme terjaga.

## Motivasi

`each_cmd` (RFC-0019) menghasilkan referensi komponen tetapi **bukan** `Entity`, sehingga pola paling umum — "despawn entity ini bila mati", "tambah komponen ke entity ini" — tak dapat diungkapkan (tak ada handle untuk diberikan ke command buffer). Query harus dapat menghasilkan `Entity` bersama komponen.

Kendala desain: `Entity` **tidak** disimpan di kolom komponen; ia ada di daftar baris archetype (`entities[row]` memetakan baris → entity). Abstraksi `QueryTerm` kini **berpusat-kolom** (`component_id` + `iter_shared(col)`), tak cocok untuk `Entity`.

## Usulan rinci

### 1. Generalisasi `QueryTerm`

`QueryTerm` diubah dari "punya `component_id`, iterasi sebuah kolom" menjadi "punya **syarat pencocokan**, iterasi diberi **archetype** (+ kolom teresolusi opsional)":

```rust
enum Requirement {
    Column(ComponentId), // butuh kolom hadir (untuk &T / &mut T)
    Any,                 // tak butuh kolom apa pun (untuk Entity)
    Never,               // tipe komponen tak terdaftar → query kosong
}

trait QueryTerm {
    type Item<'w>;
    fn access(access: &mut Access);
    fn requirement(world: &World) -> Requirement;
    fn iter_shared(archetype: &Archetype, col: Option<usize>) -> impl Iterator<Item = Self::Item<'_>>;
}
```

- `&T` / `&mut T`: `requirement` = `Column(cid)` (atau `Never` bila `T` tak terdaftar); `iter_shared` memakai `archetype.column(col.unwrap())` — perilaku lama, jalur `&mut` tetap `unsafe` terkurung (RFC-0016).
- **`Entity`**: `requirement` = `Any`; `iter_shared` mengabaikan `col`, mengembalikan `archetype.entities().iter().copied()` — **100% aman**.

### 2. Pencocokan & iterasi

`each_cached` (per RFC-0017) memakai `Requirement`:

1. Bila term mana pun `Never` → **return** (query tak mungkin cocok).
2. `required_cids` = cid dari term `Column` — dipakai `assert_no_alias` (Entity tak ikut) dan pencocokan archetype (harus memuat semua). Term `Any` tak mensyaratkan apa pun.
3. Scan inkremental + filter seperti biasa; iterasi lockstep tiap term via `iter_shared(archetype, col_teresolusi)`.

### 3. `Entity` sebagai `QueryData` tunggal

`<Entity>::each(world, |e| …)` mengiterasi **semua entity yang punya komponen** (ada di suatu archetype). Entity tanpa komponen tak ada di archetype → tak dihasilkan.

### 4. Akses & penjadwalan

`Entity::access()` **kosong** (bukan baca/tulis komponen). Dalam tuple, `Entity` tak menambah konflik — `(Entity, &mut Pos)` berkonflik sama seperti `&mut Pos` saja. Handle bersifat baca-saja & `Copy`.

### 5. Interaksi dengan command buffer

```rust
s.add(System::each_cmd::<(Entity, &Health)>(|(e, h), cmd| {
    if h.0 <= 0 { cmd.despawn(e); }
}));
```

`despawn`/`insert`/`remove` tertunda ke akhir run (RFC-0019) → aman: tak ada mutasi struktural selama iterasi. `Entity` yang di-*capture* tetap valid saat apply (mutasi diterapkan urutan-rekam; handle basi ditangani `contains`, STD-0007).

### 6. Catatan: `Entity` vs komponen `Entity`

`Entity` juga tipe `'static + Send` → secara teknis bisa jadi komponen (blanket impl). `Entity` **sebagai term** berarti *handle baris*, berbeda dari `&Entity` (query komponen `Entity`, degenerate). Overload disengaja & lazim (mengikuti konvensi ECS); `&Entity`/`&mut Entity` tetap query komponen biasa.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| Generalisasi `QueryTerm` (dipilih) | Satu jalur; komposabel di tuple | Menyentuh trait inti | Paling bersih & seragam |
| `Entity` marker + jalur iterasi paralel | Tak ubah `QueryTerm` | Duplikasi logika iterasi; tuple campuran sulit | Menghindari duplikasi |
| Yield `(Entity, Item)` implisit di `each_cmd` | Tak perlu term | Kaku; tak berlaku untuk `each`/query lain | Term eksplisit lebih fleksibel |
| Metode `each_with_entity` terpisah | Sederhana | Ledakan API per-arity | Term generik menyatu dengan tuple |

## Dampak

- **Kompatibilitas / migrasi:** aditif. `Entity` kini `QueryData`/`QueryTerm`. Refactor `QueryTerm` **internal** (`#[doc(hidden)]`); API publik query lama tak berubah, hasil identik.
- **Keamanan:** **tanpa `unsafe` baru** (iterasi `Entity` = salinan slice aman). Jalur `&mut` terkurung tetap.
- **Konsekuensi pada invarian:** membuka mutasi-struktural-per-entity dari sistem paralel (memperkuat nilai RFC-0019); determinisme & urutan iterasi identik (STD-0005/0006).

## Pertanyaan terbuka

- Reservasi entity atomik untuk handle spawn sinkron (masih terbuka dari RFC-0019).
- Term query lain yang non-kolom (mis. `Has<T>` bertipe bool) → bila dibutuhkan.

## Keputusan

Diterima. Lihat [ADR-0020](../ADR/ADR-0020-entity-query-term.md).
