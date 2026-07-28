# Milestone 26 — Query builder typed arke-postgres (RFC-0030)

> Fitur baca ergonomis untuk `arke-postgres`: builder fluent+typed menggantikan
> predikat string SQL mentah. Terinspirasi fluent builder Eloquent (bukan Active
> Record). Aditif; bukan core (beku 1.0).

## Tujuan

Mengimplementasikan [RFC-0030](RFC/RFC-0030-arke-postgres-typed-query-builder.md):
`store.query::<T>().filter(..).order_by(..).limit().offset().load(w)` dengan field
token via derive, SQL ter-parameterisasi (anti-injeksi).

## Ruang lingkup

**Termasuk:**

- `arke-postgres`: `Field<C,V>`, `Filter<C>` (+ and/or/not), `Query<T>`
  (filter/order_by/limit/offset/load), `Dir`, trait `IntoPgValue`.
- Operator: `eq/ne/lt/lte/gt/gte/between/is_null/in_` (semua tipe) + `like`
  (hanya `Field<C,String>`).
- `arke-postgres-derive`: generate `T::field() -> Field<Self, V>` per field.
- `load_where` string **tetap** (escape-hatch).

**Tidak termasuk:**

- Active Record, lazy-load, relasi, agregasi/join — di luar lingkup.
- Perubahan core arke.

## Definition of Done

- [ ] Unit test SQL-generation (tanpa DB): builder → SQL + params yang diharapkan,
      ter-parameterisasi, tiap operator/kombinator.
- [ ] `compile_fail` doc-test: `hp().lt("teks")` & `hp().like(..)` (integer) gagal.
- [ ] Integrasi (CI `postgres`): builder ≡ `load_where` string setara.
- [ ] fmt/clippy/miri/CI hijau; `load_where` tak berubah.
