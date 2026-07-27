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
