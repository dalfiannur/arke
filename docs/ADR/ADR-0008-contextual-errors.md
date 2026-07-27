# ADR-0008: Error berkonteks

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-27
- **RFC terkait:** [RFC-0008](../RFC/RFC-0008-contextual-errors.md)

## Konteks

STD-0008 dan Philosophy §3 menuntut pesan kegagalan yang menyebut entity/komponen yang terlibat. Beberapa kegagalan kini diam, ber-`Option` tanpa konteks, atau `panic` dengan pesan generik. Verify STD-0008 menunjuk dua skenario: konflik borrow query dan komponen tak terdaftar.

## Keputusan

Kami memilih:

1. Menambahkan tipe **`EcsError`** (`Display` + `std::error::Error`, tanpa dep) dengan varian `QueryConflict { component }` dan `ComponentNotRegistered { component }` yang **menyebut nama tipe komponen**.
2. **Konflik borrow query** tetap `panic` (bug program, fail-fast) tetapi pesannya menyebut tipe komponen via `type_name`.
3. Menambahkan **`World::try_snapshot() -> Result<Snapshot, EcsError>`** yang menolak komponen tak-terdaftar-serializable dengan menyebut namanya; `snapshot()` tetap lunak.
4. `ComponentRegistry` menyimpan **nama tipe** tiap komponen agar dapat dinamai dalam error.

## Konsekuensi

**Positif:**

- Mengaktifkan STD-0008; kegagalan menjelaskan komponen yang terlibat.
- `try_snapshot` mencegah kehilangan data diam.
- Tetap tanpa `unsafe` & tanpa dependensi eksternal.

**Negatif / biaya:**

- Menambah permukaan API (`EcsError`, `try_snapshot`).
- Registry menyimpan nama tipe tambahan (biaya memori kecil).

**Netral / catatan:**

- Konflik query tetap panic (bukan `Result`); dianggap bug program.
- Menyertakan `Entity` dalam error tertentu ditunda sampai ada operasi ber-entity yang gagal.

## Alternatif yang ditolak

- **Semua operasi jadi `Result`** — perubahan breaking berlebihan.
- **Konflik query jadi `Result`** — konflik adalah bug program; panic fail-fast lebih tepat.
- **Snapshot lunak saja** — membiarkan kehilangan data diam.

Rincian pertimbangan ada di [RFC-0008](../RFC/RFC-0008-contextual-errors.md).
