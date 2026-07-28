# ADR-0026: Seal trait ekstensi via supertrait penanda privat

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-28
- **RFC terkait:** [RFC-0026](../RFC/RFC-0026-seal-extension-traits.md)

## Konteks

Menuju 1.0 (RN-0004), trait ekstensi `Bundle`/`QueryData`/`QueryTerm`/`QueryFilter`
terbuka. `QueryData` & `QueryFilter` bersignature publik → benar-benar dapat
diimpl downstream; membekukannya di 1.0 mengunci detail implementasi selamanya.

## Keputusan

1. **Seal keempat trait** dengan supertrait penanda di `pub(crate) mod sealed`
   (`BundleSealed`, `QueryDataSealed`, `QueryTermSealed`, `QueryFilterSealed`).
   Tiap impl trait di crate mendapat impl penanda paralel.
2. **`Component` & `Serialize` tidak disegel** — `Component` sudah tertutup via
   blanket impl; `Serialize` sengaja impl-able pengguna.
3. **Uji via doc-test `compile_fail`** (0 dependensi) memuat impl downstream
   lengkap — bukan `trybuild` (dependensi eksternal, langgar STD-0003).
4. Bila `private_bounds` menyala, `#[allow(...)]` secara sadar.

## Konsekuensi

**Positif:**

- Detail implementasi (`fn each_cached`/`push`/`fetch`/`resolve`) keluar dari
  kontrak publik → bebas berevolusi pasca-1.0.
- Jaminan tertutup jadi **eksplisit**, bukan insidental via tipe privat.
- Menopang penambahan aditif pasca-1.0 (events, change-detection, dst.).

**Negatif / biaya:**

- **BREAKING** bagi impl trait di luar crate (praktik: nihil).
- Sedikit boilerplate (impl penanda paralel) — diserap makro tuple yang ada.

**Netral:**

- Murni visibilitas trait; tak menyentuh perilaku, keamanan, atau determinisme.
- Ditujukan ke rilis breaking **0.6.0**.

## Alternatif yang ditolak

- **Biarkan terbuka** — membekukan signature internal di 1.0.
- **`trybuild`** — dependensi dev eksternal; `compile_fail` doc-test cukup & 0-dep.

Rincian di [RFC-0026](../RFC/RFC-0026-seal-extension-traits.md).
