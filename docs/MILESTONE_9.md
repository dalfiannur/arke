# Milestone 9 — Resources

> Disalin dari [MILESTONE_TEMPLATE.md](MILESTONE_TEMPLATE.md). Lihat [RFC-0010](RFC/RFC-0010-resources.md) / [ADR-0010](ADR/ADR-0010-resources.md).

## Tujuan

Menambahkan resource (state global singleton-per-tipe) dan cara mengaksesnya sebagai parameter sistem bertipe, dengan konflik resource ditalar scheduler. Tetap tanpa `unsafe` & tanpa dependensi eksternal.

## Ruang lingkup

**Termasuk:**

- `World`: `insert_resource`, `resource`, `resource_mut`, `remove_resource`, `contains_resource`.
- `Access` diperluas: `resource_reads`/`resource_writes` (namespace terpisah dari komponen); konflik per-namespace.
- `System::resource::<R>(|r: &mut R| ...)` — sistem resource-saja (tulis R).
- `System::each_res::<R, Q>(|r: &R, item| ...)` — baca resource + iterasi query (take/put-back aman).

**Tidak termasuk (sengaja ditunda):**

- `each_res` yang memutasi resource (`&mut R`) sambil iterasi.
- `Res<T>`/`ResMut<T>` variadik penuh (SystemParam).
- Serialisasi resource dalam snapshot.

## Artefak yang dihasilkan

| Artefak | Bentuk |
| --- | --- |
| RFC-0010 / ADR-0010 | Proposal & keputusan resources |
| Kode + tes | Storage resource, konstruktor sistem, konflik scheduler + tes |

## Kriteria selesai (Definition of Done)

- [ ] `insert_resource`/`resource`/`resource_mut`/`remove_resource`/`contains_resource` bekerja; round-trip nilai teruji.
- [ ] Resource dan komponen ber-`TypeId` sama tidak salah-konflik (namespace terpisah) — teruji.
- [ ] `System::resource::<R>` memutasi resource saat dijalankan; akses tersimpul tulis R.
- [ ] `System::each_res::<R, Q>` membaca resource sambil mengiterasi query; resource dikembalikan usai iterasi.
- [ ] `Schedule` menempatkan sistem yang konflik pada resource ke stage berbeda (bukti STD-0005) — teruji.
- [ ] Tetap **tanpa `unsafe`** & tanpa dependensi eksternal (STD-0003).
- [ ] RFC-0010 & ADR-0010 ditulis serta konsisten dengan kode.
- [ ] Semua tes hijau.

## Ketergantungan

- **Butuh selesai lebih dulu:** M-2 (scheduler/Access), M-4 (QueryData).
- **Membuka jalan bagi:** SystemParam variadik; serialisasi resource.

## Pertanyaan terbuka

- `each_res` mutasi-resource; `Res`/`ResMut` variadik → milestone lanjutan.
