# RFC — Request for Comments

Proposal berumur panjang untuk perubahan konsekuensial: arsitektur, kontrak, format data, atau kemampuan inti. RFC adalah tempat **alternatif dipertimbangkan secara terbuka** sebelum sebuah arah dipilih.

## Kapan menulis RFC

Tulis RFC bila perubahan:

- menyentuh invarian atau model sistem di [ARCHITECTURE_BIBLE](../ARCHITECTURE_BIBLE.md);
- menamb/mengubah kontrak data atau API publik;
- memperkenalkan kemampuan inti baru; atau
- sulit dibatalkan setelah dirilis.

Perubahan kecil dan lokal tidak butuh RFC.

## Lifecycle

```text
Draft → Discussion → Accepted | Rejected | Superseded
```

- **Accepted** RFC melahirkan satu atau lebih **ADR** yang merekam keputusan + konsekuensinya.
- **Superseded** RFC tetap disimpan; tambahkan tautan ke penggantinya.
- Pertanyaan yang belum matang untuk RFC ditampung sebagai [RN](../RN/README.md) sampai *graduate*.

## Konvensi

- Nomor berurutan, empat digit: `RFC-0001`, `RFC-0002`, …
- Nama file: `RFC-<NNNN>-<slug-kebab-case>.md`.
- Salin [`_TEMPLATE.md`](_TEMPLATE.md) untuk memulai.

## Indeks

| RFC | Judul | Status |
| --- | --- | --- |
| [RFC-0001](RFC-0001-documentation-first-governance.md) | Tata-kelola documentation-first | Accepted |
| [RFC-0002](RFC-0002-core-storage-architecture.md) | Arsitektur penyimpanan inti — archetype + generational entity | Accepted |
| [RFC-0003](RFC-0003-deterministic-scheduler.md) | Scheduler deterministik dengan analisis konflik | Accepted |
| [RFC-0004](RFC-0004-data-parallel-iteration.md) | Iterasi data-parallel yang aman | Accepted |
| [RFC-0005](RFC-0005-type-based-systems.md) | Sistem berbasis-tipe dengan akses tersimpul | Accepted |
| [RFC-0006](RFC-0006-system-level-parallelism.md) | Paralelisme tingkat-sistem (analisis & penundaan) | Accepted (ditunda) |
| [RFC-0007](RFC-0007-world-snapshot.md) | Snapshot & serialisasi World | Accepted |
| [RFC-0008](RFC-0008-contextual-errors.md) | Error berkonteks | Accepted |
| [RFC-0009](RFC-0009-derive-serialize.md) | `derive(Serialize)` tanpa dependensi eksternal | Accepted |
| [RFC-0010](RFC-0010-resources.md) | Resources sebagai parameter sistem | Accepted |
| [RFC-0011](RFC-0011-derive-enum-attributes.md) | `derive(Serialize)` untuk enum + atribut field | Accepted |
| [RFC-0012](RFC-0012-derive-rename-all.md) | `derive(Serialize)` — rename_all & atribut level-tipe | Accepted |
| [RFC-0013](RFC-0013-generic-tuple-queries.md) | Query tuple generik (arity & mutabilitas campuran) | Accepted |
| [RFC-0014](RFC-0014-query-filters.md) | Filter query `With` / `Without` | Accepted |
| [RFC-0015](RFC-0015-unsafecell-column-storage.md) | Penyimpanan kolom berbasis `UnsafeCell` | Accepted |
| [RFC-0016](RFC-0016-parallel-executor.md) | Eksekutor paralel tingkat-sistem | Accepted |
| [RFC-0017](RFC-0017-query-cache.md) | Query Cache sebagai first-class citizen | Accepted |
| [RFC-0018](RFC-0018-dependency-graph-executor.md) | Eksekutor graf-ketergantungan | Accepted |
| [RFC-0019](RFC-0019-command-buffer.md) | Command buffer (mutasi struktural tertunda) | Accepted |
| [RFC-0020](RFC-0020-entity-query-term.md) | `Entity` sebagai term query | Accepted |
| [RFC-0021](RFC-0021-arke-postgres-adapter.md) | `arke-postgres` — adapter Postgres sebagai sumber kebenaran | Accepted |
| [RFC-0022](RFC-0022-component-bundles.md) | Bundle komponen (spawn/insert tuple) | Accepted |
| [RFC-0023](RFC-0023-columnar-query-iteration.md) | Iterasi query berbasis-indeks (kolom) — optimasi | Accepted |
| [RFC-0024](RFC-0024-fast-component-resolution.md) | Resolusi komponen cepat (hasher TypeId + threading cid) | Accepted |
| [RFC-0025](RFC-0025-unchecked-column-downcast-get.md) | Downcast kolom tak-tercek pada `World::get` (unsafe terkurung) | Accepted |
| [RFC-0026](RFC-0026-seal-extension-traits.md) | Seal trait ekstensi (`Bundle`/`QueryData`/`QueryTerm`/`QueryFilter`) | Accepted |
| [RFC-0027](RFC-0027-deprecate-query-pair.md) | Deprecate `query_pair`/`query_pair_ref` → konvergen `QueryData` | Accepted |
