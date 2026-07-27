# Milestone 16 — Query Cache

> Disalin dari [MILESTONE_TEMPLATE.md](MILESTONE_TEMPLATE.md). Lihat [RFC-0017](RFC/RFC-0017-query-cache.md) / [ADR-0017](ADR/ADR-0017-query-cache.md).

## Tujuan

Meng-cache archetype yang cocok untuk sebuah query (di-update inkremental) agar query berulang tak memindai seluruh archetype tiap run. Memperkuat *ergonomis = cepat*. 100% aman.

## Ruang lingkup

**Termasuk:**

- `QueryState` publik (`matched` + `scanned`).
- `QueryData::each_cached<F>(world, state, f)` (scan inkremental + iterasi cocok); `each_filtered_shared` jadi wrapper.
- `System::each`/`each_filtered` menyimpan `QueryState` (cache persist lintas-run).

**Tidak termasuk (sengaja ditunda):**

- Cache global lintas-sistem; cache indeks kolom per archetype.

## Kriteria selesai (Definition of Done)

- [ ] `each_cached` memberi hasil identik dengan `each` (tanpa cache) — teruji.
- [ ] Menjalankan query berulang dengan `QueryState` yang sama memberi hasil konsisten — teruji.
- [ ] Archetype baru (dibuat antar-run) tertangkap oleh scan inkremental — teruji.
- [ ] `System::each` yang dijalankan berkali-kali dengan entity baru memproses entity baru (cache ter-update) — teruji.
- [ ] Determinisme & urutan iterasi identik dengan sebelumnya (STD-0005).
- [ ] Tetap **tanpa `unsafe` baru**; jalur pengguna aman.
- [ ] RFC-0017 & ADR-0017 ditulis serta konsisten dengan kode.
- [ ] Semua tes + miri hijau.

## Ketergantungan

- **Butuh selesai lebih dulu:** M-13 (query + filter), M-15 (jalur berbagi).
- **Membuka jalan bagi:** dependency-graph executor.

## Pertanyaan terbuka

- Cache indeks kolom; cache global lintas-sistem → lanjutan.
