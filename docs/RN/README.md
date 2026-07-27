# RN — Research Notes

Tempat menampung **pertanyaan terbuka** dan eksplorasi yang belum matang untuk menjadi RFC. RN menjaga agar ketidakpastian tercatat tanpa memaksa keputusan prematur.

## Kapan menulis RN

- Ada pertanyaan penting yang belum bisa dijawab tegas.
- Sebuah RFC memunculkan sub-pertanyaan yang layak diselidiki terpisah.
- Kamu ingin merekam eksperimen/temuan sebelum mengusulkan arah.

## Lifecycle

```text
Open → Investigating → Graduated (→ RFC-XXXX) | Closed (won't pursue)
```

Ketika sebuah RN cukup matang, ia **graduate** menjadi RFC. RN tetap disimpan dengan tautan ke RFC penggantinya.

## Konvensi

- Nomor berurutan: `RN-0001`, `RN-0002`, …
- Nama file: `RN-<NNNN>-<slug-kebab-case>.md`.
- Salin [`_TEMPLATE.md`](_TEMPLATE.md) untuk memulai.

## Indeks

| RN | Judul | Status |
| --- | --- | --- |
| [RN-0001](RN-0001-unsafecell-column-storage.md) | Penyimpanan kolom berbasis `UnsafeCell` untuk akses interior yang sound | Open |
