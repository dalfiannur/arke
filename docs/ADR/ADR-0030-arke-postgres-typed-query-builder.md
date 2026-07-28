# ADR-0030: Query builder typed untuk arke-postgres

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-29
- **RFC terkait:** [RFC-0030](../RFC/RFC-0030-arke-postgres-typed-query-builder.md)

## Konteks

`load_where::<T>(w, "hp < 20")` memakai string SQL mentah: stringly-typed, rawan
injeksi (interpolasi), tak komposabel. Terinspirasi fluent builder Eloquent —
tanpa Active Record-nya (bertabrakan dgn ECS + determinisme).

## Keputusan

1. **Builder typed** `store.query::<T>().filter(..).order_by(..).limit().offset().load(w)`.
2. **Field token via derive**: `#[derive(PgComponent)]` generate `T::field() ->
   Field<T, V>` per field → operator dicek compiler.
3. **Operator**: `eq/ne/lt/lte/gt/gte/between/is_null/in_` untuk semua tipe;
   `like` **hanya** untuk `Field<C, String>` (type-safety). Kombinator `and/or/not`.
4. **SQL ter-parameterisasi** (placeholder → `$n`, nilai di-`bind`) — anti-injeksi.
5. **`load_where` string tetap** sebagai escape-hatch.
6. **`IntoPgValue`** untuk konversi nilai→bind (skalar didukung).
7. **Bukan Active Record / lazy / relasi** — di luar lingkup by design.

## Konsekuensi

**Positif:**

- Query baca typed, komposabel, anti-injeksi; `order_by/limit/offset`.
- Selaras Manifesto ("jalur ergonomis = aman & cepat").
- Aditif; bukan core (beku 1.0) → arke-postgres minor-bump.

**Negatif / biaya:**

- Derive menambah metode field per-field (risiko tabrakan nama field↔metode —
  didokumentasikan).
- Permukaan API baru untuk dijaga di arke-postgres.

**Netral:**

- `load_where` tetap; migrasi opsional.

## Alternatif yang ditolak

- **String SQL saja** — tak aman/komposabel.
- **Const-struct token** (`HealthCols::hp`) — metode `T::hp()` lebih ringkas.
- **Macro `sql!{}`** — kompleksitas proc-macro berlebih untuk scope ini.

Rincian di [RFC-0030](../RFC/RFC-0030-arke-postgres-typed-query-builder.md).
