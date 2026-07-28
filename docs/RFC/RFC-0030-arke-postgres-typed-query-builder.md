# RFC-0030: Query builder typed untuk arke-postgres (terinspirasi Eloquent)

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-29
- **Crate:** `arke-postgres` (+ `arke-postgres-derive`) — bukan core (yang beku untuk 1.0)
- **ADR terkait:** [ADR-0030](../ADR/ADR-0030-arke-postgres-typed-query-builder.md)

## Ringkasan

Menambah **query builder fluent & typed** ke `arke-postgres` untuk baca ber-filter,
menggantikan predikat **string SQL mentah** `load_where::<T>(w, "hp < 20")`:

```rust
store.query::<Health>()
    .filter(Health::hp().lt(20).and(Health::hp().gte(5)))
    .order_by(Health::hp(), Desc)
    .limit(100)
    .offset(200)
    .load(&mut world).await?;
```

Terinspirasi *fluent query builder* Laravel Eloquent — **tetapi bukan Active
Record**-nya (lihat "Yang sengaja tidak diambil"). Field token (`Health::hp()`)
di-generate `#[derive(PgComponent)]` → **dicek compiler** & SQL **ter-parameterisasi**
(anti-injeksi). `load_where(sql)` tetap ada sebagai escape-hatch.

## Motivasi

`load_where` kini menerima **string SQL mentah** yang di-interpolasi langsung:

```rust
let sql = format!("SELECT entity_id FROM {} WHERE {} ...", T::TABLE, predicate);
```

Masalah:

1. **Stringly-typed** — typo nama kolom/tipe baru ketahuan saat runtime.
2. **Rawan injeksi** — bila `predicate` berasal dari input, itu jalur injeksi SQL.
3. **Tak komposabel** — sulit menggabung filter secara programatik.
4. **Tak selaras Manifesto** — "jalur ergonomis adalah jalur *aman* & cepat"; string
   mentah adalah jalur ergonomis yang *tidak* aman.

Builder typed menyelesaikan keempatnya, sekaligus menambah `order_by`/`limit`/
`offset` (scope disepakati).

## Usulan rinci

### 1. Field token (di-generate derive)

Untuk tiap field, `#[derive(PgComponent)]` meng-generate metode pengembali token:

```rust
// #[derive(PgComponent)] struct Health { hp: i32 }  →  generate:
impl Health {
    pub fn hp() -> ::arke_postgres::Field<Self, i32> {
        ::arke_postgres::Field::new("hp")
    }
}
```

`Field<C, V>` membawa **nama kolom** (`&'static str`) + tipe fantom `(C, V)` →
operator memeriksa tipe nilai terhadap tipe field saat *compile*.

### 2. Operator predikat → `Filter<C>`

```rust
impl<C: PgComponent, V: IntoPgValue> Field<C, V> {
    pub fn eq(self, v: V)  -> Filter<C>;   // col = $n
    pub fn ne(self, v: V)  -> Filter<C>;
    pub fn lt(self, v: V)  -> Filter<C>;
    pub fn lte(self, v: V) -> Filter<C>;
    pub fn gt(self, v: V)  -> Filter<C>;
    pub fn gte(self, v: V) -> Filter<C>;
    pub fn between(self, lo: V, hi: V) -> Filter<C>;
    pub fn is_null(self)   -> Filter<C>;   // untuk Option<T>
    pub fn in_<I: IntoIterator<Item = V>>(self, vals: I) -> Filter<C>;  // col IN ($a,$b,..)
}

// `LIKE` hanya untuk field teks (type-safety): tersedia saat V = String.
impl<C: PgComponent> Field<C, String> {
    pub fn like(self, pattern: impl Into<String>) -> Filter<C>;   // col LIKE $n
}
```

`in_` dengan iterator kosong → `1 = 0` (tak cocok apa pun) agar aman. `like`
hanya muncul pada field bertipe teks — `Health::hp().like(..)` (integer) **gagal
kompilasi**.

`Filter<C>` menyimpan **fragmen SQL ber-placeholder** + `Vec<PgValue>` (nilai
ter-bind). Komposisi:

```rust
impl<C> Filter<C> {
    pub fn and(self, other: Filter<C>) -> Filter<C>;   // (a) AND (b)
    pub fn or(self, other: Filter<C>) -> Filter<C>;    // (a) OR  (b)
    pub fn not(self) -> Filter<C>;                     // NOT (a)
}
```

`IntoPgValue` (trait baru) mengubah `V` → `PgValue` untuk bind — di-impl untuk
skalar yang didukung (`i32`/`i64`/`f32`/`f64`/`bool`/`String`/`&str`), selaras
`PgType`.

### 3. Builder `Query<T>`

```rust
impl PgStore {
    pub fn query<T: PgComponent>(&mut self) -> Query<'_, T>;
}

pub struct Query<'a, T> { /* store, filter, order, limit, offset */ }

impl<'a, T: PgComponent> Query<'a, T> {
    pub fn filter(self, f: Filter<T>) -> Self;             // gabung AND bila dipanggil >1×
    pub fn order_by<V>(self, field: Field<T, V>, dir: Dir) -> Self;  // Dir::{Asc,Desc}
    pub fn limit(self, n: u64) -> Self;
    pub fn offset(self, n: u64) -> Self;
    pub async fn load(self, world: &mut World) -> Result<usize, sqlx::Error>;
}
```

`load` menyusun SQL **ter-parameterisasi**:

```sql
SELECT entity_id FROM cmp_health
WHERE (hp < $1 AND hp >= $2)
ORDER BY hp DESC
LIMIT $3 OFFSET $4
```

lalu `materialize` seperti `load_where` (memuat entity + seluruh komponennya).
Penomoran `$n` dikelola saat assembly; nilai di-`bind` (bukan diinterpolasi).

### 4. Escape-hatch tetap

`load_where::<T>(w, "predikat SQL")` **dipertahankan** untuk predikat yang belum
didukung builder (fungsi window, ekspresi JSONB kompleks, dll) — sejalan pola
"builder ergonomis + escape-hatch mentah".

## Yang sengaja **tidak** diambil dari Eloquent

Eloquent = **Active Record**; sebagian besar bertabrakan dengan ECS + determinisme:

| Fitur Eloquent | Kenapa tidak |
| --- | --- |
| Active Record (`$model->save()`) | Komponen = data polos; persistensi eksplisit lewat `PgStore` (sumber kebenaran) |
| Relasi lazy-load (`$user->posts`) | N+1 tersembunyi; melawan load eksplisit & determinisme |
| Scope global/magic | Perilaku implisit; arke pilih eksplisit |
| Query dinamis via string field | Diganti token typed (dicek compiler) |

Yang **diambil**: hanya *fluent builder*-nya — komposisi `where/order/limit` yang
ergonomis, dibuat typed & parameterized ala-Rust.

## Dampak

- **Kompatibilitas:** **aditif** — `load_where` string tetap; `query()` baru.
  Bukan core (beku 1.0) → aman. arke-postgres minor-bump (0.6.0).
- **Derive:** menambah generasi metode field per-field. Risiko tabrakan nama
  (field bernama sama dgn metode builder) — didokumentasikan; jarang.
- **Keamanan:** menghapus jalur injeksi `load_where` untuk kasus umum (bind, bukan
  interpolasi).

## Alternatif yang dipertimbangkan

| Alternatif | Kenapa tidak |
| --- | --- |
| Tetap string SQL saja | Stringly-typed, rawan injeksi, tak komposabel |
| Field token via const struct (`HealthCols::hp`) | Metode `Health::hp()` lebih ringkas & cocok contoh; setara fungsional |
| Builder mengembalikan SQL (bukan eksekusi) | Kurang ergonomis; `load` end-to-end lebih sesuai "ergonomis = jalur" |
| Macro query `sql!{}` | Kompleksitas proc-macro besar; builder cukup utk scope ini |

## Rencana verifikasi (TDD, saat Accepted)

- Uji **generasi SQL** (unit, tanpa DB): `Query` → string SQL + urutan `PgValue`
  yang diharapkan (parameterized) untuk tiap operator & kombinator.
- Uji **integrasi** (job `postgres` CI): `query().filter().order_by().limit()` vs
  `load_where` string yang setara → hasil identik.
- Uji **typed-negatif** (doc-test `compile_fail`): `Health::hp().lt("teks")` gagal
  kompilasi (tipe nilai ≠ tipe field).

## Keputusan

**Draft — menunggu review.** Bila disetujui: ADR-0030 + Milestone + TDD.
