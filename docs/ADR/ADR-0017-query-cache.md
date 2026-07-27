# ADR-0017: Query Cache sebagai first-class citizen

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-28
- **RFC terkait:** [RFC-0017](../RFC/RFC-0017-query-cache.md)

## Konteks

`each_filtered_shared` memindai semua archetype + cek filter tiap run — sia-sia untuk sistem berulang di world ber-banyak-archetype. Archetype append-only & set-komponen immutable → daftar archetype-cocok stabil, hanya perlu diperluas.

## Keputusan

Kami memilih:

1. **`QueryState`** publik: `matched: Vec<usize>` (archetype cocok) + `scanned: usize`.
2. **`QueryData::each_cached<F>(world, state, f)`** sebagai implementasi inti: resolve id → scan inkremental `archetype[scanned..]` → iterasi `matched`. `each_filtered_shared` = wrapper `QueryState` sekali-pakai.
3. **`System::each`/`each_filtered` menyimpan `QueryState`** di closure → cache persist lintas-run, per-sistem (aman di jalur paralel).
4. Cache **tak perlu invalidasi** — hanya diperluas (append-only). Registrasi terlambat ditangani dengan tak memajukan `scanned` saat id belum lengkap.

## Konsekuensi

**Positif:**

- Query berulang jadi O(archetype cocok), bukan O(semua) → memperkuat *ergonomis = cepat*.
- 100% aman; hasil & urutan iterasi identik (STD-0005).
- Transparan bagi pengguna; power-user dapat memegang `QueryState`.

**Negatif / biaya:**

- Sedikit memori per sistem (`Vec<usize>` archetype cocok).
- Lookup indeks kolom masih dihitung ulang per archetype-cocok (murah; optimasi lanjutan mungkin).

**Netral / catatan:**

- Cache per-`QueryState` (bukan global) — tiap sistem punya sendiri.
- Kebenaran bersandar pada archetype append-only & set-komponen immutable (sifat desain M-1).

## Alternatif yang ditolak

- **Cache global per-signature** — perlu kunci + lookup; per-state inkremental lebih ringan.
- **Rebuild penuh saat berubah** — buang kerja; append-only tak perlu rebuild.

Rincian pertimbangan ada di [RFC-0017](../RFC/RFC-0017-query-cache.md).
