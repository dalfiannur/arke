# ADR-0007: Snapshot & serialisasi World

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-27
- **RFC terkait:** [RFC-0007](../RFC/RFC-0007-world-snapshot.md)

## Konteks

ARCHITECTURE_BIBLE §2/§4 menuntut keadaan `World` dapat di-snapshot ke format terbuka berversi dan dipulihkan setia (STD-0001/0002), sementara penyimpanan komponen type-erased dan serde adalah dependensi eksternal yang melanggar standalone (STD-0003).

## Keputusan

Kami memilih:

1. **Trait `Serialize` milik rust-ecs** (tanpa dep) + enum perantara **`Value`**, dengan JSON tulis-tangan.
2. **Opt-in per tipe** lewat `register_serializable::<T>()` yang menyimpan nama tipe stabil + vtable `to_value`/`from_value`. Hanya tipe terdaftar yang di-snapshot.
3. **Snapshot entity-centric** dengan `schema_version` wajib (STD-0001) dan komponen dikunci **nama tipe** (portabel).
4. **Round-trip setia** (STD-0002): entity direkam dengan `index`+`generation`; `load_snapshot(&world.snapshot())` menghasilkan `World` setara observasional.
5. **JSON schema** di `schema/v1/` mengikuti pola validator repo.

## Konsekuensi

**Positif:**

- Mengaktifkan STD-0001 & STD-0002; data pengguna portabel & terbaca di luar aplikasi.
- Tetap standalone (tanpa serde) dan tanpa `unsafe`.
- Nama-tipe sebagai kunci → snapshot stabil lintas-proses.

**Negatif / biaya:**

- Kurang ergonomis dari serde; komponen harus mengimplementasikan `Serialize` dan didaftarkan.
- JSON tulis-tangan (emit + parser) adalah kode yang harus dipelihara.
- Hanya tipe terdaftar yang ikut snapshot (didokumentasikan).

**Netral / catatan:**

- Hanya entity hidup di-snapshot; rekonstruksi free-list persis ditunda.
- Migrasi antar `schema_version` menjadi pekerjaan saat versi kedua muncul.

## Alternatif yang ditolak

- **serde (feature opsional)** — dependensi eksternal; melawan semangat standalone.
- **Bound `Serialize` pada `Component`** — memaksa semua komponen serializable.
- **Kunci `ComponentId` numerik** — tak stabil lintas-proses.

Rincian pertimbangan ada di [RFC-0007](../RFC/RFC-0007-world-snapshot.md).
