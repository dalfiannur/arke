# ADR-0014: Filter query `With` / `Without`

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-28
- **RFC terkait:** [RFC-0014](../RFC/RFC-0014-query-filters.md)

## Konteks

Sistem sering menyaring entity berdasarkan kehadiran komponen tanpa membacanya. Menyertakan komponen di query data hanya untuk menyaring memaksa mengambil datanya dan menambah akses palsu ke analisis konflik scheduler. Filter memisahkan pencocokan dari pengambilan.

## Keputusan

Kami memilih:

1. Penanda zero-sized **`With<T>`** dan **`Without<T>`**.
2. Trait **`QueryFilter`** dengan `resolve(world, with, without) -> bool` yang mengumpulkan komponen wajib-hadir/wajib-absen; diimplementasikan untuk `With`, `Without`, `()`, dan tuple (AND).
3. **`QueryData::each_filtered::<F>`** yang me-resolve filter lalu memproses hanya archetype yang memuat semua `with` dan tak satupun `without`; `each` lama menjadi default `each_filtered::<()>`.
4. **`System::each_filtered::<Q, F>`**.
5. Filter **tidak** menyumbang `Access` → tak memengaruhi konflik scheduler.

## Konsekuensi

**Positif:**

- Penyaringan berdasarkan kehadiran komponen tanpa mengambil datanya.
- Tetap tanpa `unsafe`, 0 dependensi eksternal; determinisme tak berubah.
- `each_filtered::<()>` menjaga kompatibilitas `each`.

**Negatif / biaya:**

- `QueryData` kini punya method `each_filtered` (impl per-arity diperbarui).
- Impl `QueryFilter` untuk tuple terbatas arity (bisa diperluas).

**Netral / catatan:**

- Filter `Or`, `Changed`/`Added` (deteksi perubahan) ditunda.
- `With<T>` untuk komponen tak-terdaftar → query tak mencocokkan apa pun; `Without<T>` tak-terdaftar → trivial terpenuhi.

## Alternatif yang ditolak

- **Filter sebagai term data ber-`Item=()`** — mengotori closure & akses.
- **Query lalu `skip` manual** — mengambil data tak perlu.
- **Predikat closure runtime** — tak terdeklarasi.

Rincian pertimbangan ada di [RFC-0014](../RFC/RFC-0014-query-filters.md).
