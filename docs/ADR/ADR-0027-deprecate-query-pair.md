# ADR-0027: Deprecate `query_pair`/`query_pair_ref`, konvergen ke `QueryData`

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-28
- **RFC terkait:** [RFC-0027](../RFC/RFC-0027-deprecate-query-pair.md)

## Konteks

Menuju 1.0 (RN-0004), `World::query_pair`/`query_pair_ref` (khusus arity-2)
tumpang-tindih dengan jalur `QueryData` generik yang menangani arity sembarang.
Membekukan keduanya di 1.0 mengunci API redundan; `query_pair_ref` bahkan tanpa
pemakai.

## Keputusan

1. **`#[deprecated(since = "0.6.0")]`** pada `query_pair` & `query_pair_ref`,
   dengan `note` mengarahkan ke `<(&A, &mut B)>::each(...)`. Implementasi tetap.
2. **Pertahankan** `query`/`query_mut` (arity-1, kasus umum & ergonomis).
3. **Hapus di 1.0** (jendela migrasi sepanjang 0.6.x).
4. **Migrasi pemakai internal** (contoh, doc) ke jalur generik; **uji perilaku**
   deprecated ditandai `#[allow(deprecated)]` (CI `-D warnings`).
5. **Verifikasi** doc-test `compile_fail` + `#![deny(deprecated)]` (0 dependensi).

## Konsekuensi

**Positif:**

- API query 1.0 ortogonal: satu jalur tuple generik + shortcut arity-1.
- Non-breaking di 0.6.0 (peringatan); pengguna punya jendela migrasi.

**Negatif / biaya:**

- Pemakai `query_pair` harus migrasi sebelum 1.0.
- Uji internal perlu `#[allow(deprecated)]` sampai penghapusan.

**Netral:**

- Murni atribut; tak menyentuh perilaku/performa. Ditujukan ke 0.6.0.

## Alternatif yang ditolak

- **Pertahankan di 1.0** — membekukan redundansi.
- **Hapus langsung** — breaking tanpa jendela migrasi.

Rincian di [RFC-0027](../RFC/RFC-0027-deprecate-query-pair.md).
