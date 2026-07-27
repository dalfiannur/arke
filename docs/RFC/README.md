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
