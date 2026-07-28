# ADR — Architecture Decision Records

Catatan **keputusan** konsekuensial yang telah **diterima**, beserta konteks dan konsekuensinya. Berbeda dari RFC (yang mengeksplorasi opsi), ADR merekam pilihan yang sudah diambil agar tidak diperdebatkan ulang tanpa alasan baru.

## Kapan menulis ADR

Tulis ADR ketika sebuah RFC diterima, atau ketika keputusan arsitektural konsekuensial diambil dan perlu direkam permanen. Satu ADR = satu keputusan.

## Lifecycle

```text
Proposed → Accepted → (Deprecated | Superseded by ADR-XXXX)
```

ADR bersifat **immutable** setelah Accepted: jangan menulis ulang isinya. Jika keputusan berubah, tulis ADR baru yang men-*supersede* yang lama dan tautkan keduanya.

## Konvensi

- Nomor berurutan, empat digit: `ADR-0001`, `ADR-0002`, …
- Nama file: `ADR-<NNNN>-<slug-kebab-case>.md`.
- Salin [`_TEMPLATE.md`](_TEMPLATE.md) untuk memulai.

## Indeks

| ADR | Judul | Status |
| --- | --- | --- |
| [ADR-0001](ADR-0001-documentation-first.md) | Tata-kelola documentation-first | Accepted |
| [ADR-0002](ADR-0002-core-storage-architecture.md) | Arsitektur penyimpanan inti — archetype + generational entity | Accepted |
| [ADR-0003](ADR-0003-deterministic-scheduler.md) | Scheduler deterministik dengan analisis konflik | Accepted |
| [ADR-0004](ADR-0004-data-parallel-iteration.md) | Iterasi data-parallel yang aman | Accepted |
| [ADR-0005](ADR-0005-type-based-systems.md) | Sistem berbasis-tipe dengan akses tersimpul | Accepted |
| [ADR-0006](ADR-0006-defer-system-parallelism.md) | Menunda paralelisme tingkat-sistem hingga `UnsafeCell` + miri | Accepted |
| [ADR-0007](ADR-0007-world-snapshot.md) | Snapshot & serialisasi World | Accepted |
| [ADR-0008](ADR-0008-contextual-errors.md) | Error berkonteks | Accepted |
| [ADR-0009](ADR-0009-derive-serialize.md) | `derive(Serialize)` tanpa dependensi eksternal | Accepted |
| [ADR-0010](ADR-0010-resources.md) | Resources sebagai parameter sistem | Accepted |
| [ADR-0011](ADR-0011-derive-enum-attributes.md) | `derive(Serialize)` untuk enum + atribut field | Accepted |
| [ADR-0012](ADR-0012-derive-rename-all.md) | `derive(Serialize)` — rename_all & atribut level-tipe | Accepted |
| [ADR-0013](ADR-0013-generic-tuple-queries.md) | Query tuple generik (arity & mutabilitas campuran) | Accepted |
| [ADR-0014](ADR-0014-query-filters.md) | Filter query `With` / `Without` | Accepted |
| [ADR-0015](ADR-0015-unsafecell-column-storage.md) | Penyimpanan kolom berbasis `UnsafeCell` | Accepted |
| [ADR-0016](ADR-0016-parallel-executor.md) | Eksekutor paralel tingkat-sistem | Accepted |
| [ADR-0017](ADR-0017-query-cache.md) | Query Cache sebagai first-class citizen | Accepted |
| [ADR-0018](ADR-0018-dependency-graph-executor.md) | Eksekutor graf-ketergantungan | Accepted |
| [ADR-0019](ADR-0019-command-buffer.md) | Command buffer (mutasi struktural tertunda) | Accepted |
| [ADR-0020](ADR-0020-entity-query-term.md) | `Entity` sebagai term query | Accepted |
| [ADR-0021](ADR-0021-arke-postgres-adapter.md) | `arke-postgres` — adapter Postgres sebagai sumber kebenaran | Accepted |
| [ADR-0022](ADR-0022-component-bundles.md) | Bundle komponen (spawn/insert tuple) | Accepted |
| [ADR-0023](ADR-0023-columnar-query-iteration.md) | Iterasi query berbasis-indeks (kolom) | Accepted |
| [ADR-0024](ADR-0024-fast-component-resolution.md) | Resolusi komponen cepat (hasher TypeId + threading cid) | Accepted |
| [ADR-0025](ADR-0025-unchecked-column-downcast-get.md) | Downcast kolom tak-tercek pada `World::get` (unsafe terkurung) | Accepted |
