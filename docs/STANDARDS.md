# Standards

> Aturan lintas-potong yang **dapat diuji**. Berbeda dari Philosophy (penilaian) — Standards adalah aturan yang idealnya bisa diperiksa mesin: schema, linter, atau tes.

Setiap standar punya **ID**, pernyataan **normatif** (gunakan MUST / SHOULD / MAY), dan cara **verifikasi**.

## Format

```text
STD-<NNNN>: <judul singkat>
  Rule:   <pernyataan normatif — MUST/SHOULD/MAY>
  Why:    <alasan; kaitkan ke invarian ARCHITECTURE_BIBLE bila ada>
  Verify: <bagaimana memeriksanya — schema/tes/lint/review>
```

## Katalog

### STD-0001: Versi pada format snapshot

- **Rule:** Setiap snapshot yang diserialisasi **MUST** menyertakan field versi format (`schema_version` atau setara).
- **Why:** Mendukung migrasi eksplisit format ekspor (ARCHITECTURE_BIBLE §6.1) dan invarian portabilitas data (§2).
- **Verify:** Schema JSON menandai field versi sebagai `required`; validator menolak snapshot tanpanya.

### STD-0002: Snapshot round-trip setia

- **Rule:** `serialize(world)` diikuti `deserialize` **MUST** menghasilkan `World` yang setara secara observasional (entity, komponen, dan nilainya identik).
- **Why:** Invarian kepemilikan & portabilitas data (§2).
- **Verify:** Tes round-trip berbasis properti: dunia acak → serialize → deserialize → snapshot kedua sama dengan snapshot pertama.

### STD-0003: Core standalone

- **Rule:** Crate inti **MUST NOT** bergantung pada game engine, renderer, runtime async, atau crate I/O. Ketergantungan eksternal **MUST** berada di crate/feature adapter terpisah.
- **Why:** Invarian standalone core (§2); independensi vendor.
- **Verify:** Aturan dependensi (`cargo deny`/allowlist) di CI; uji build core dengan `--no-default-features`.

### STD-0004: Jalur pengguna tanpa `unsafe`

- **Rule:** Seluruh operasi publik pada jalur panas (spawn, insert, query, iterasi) **MUST** dapat dipakai dari kode pengguna yang mengaktifkan `#![forbid(unsafe_code)]`.
- **Why:** Invarian ergonomis = cepat (§2).
- **Verify:** Crate contoh dan benchmark yang mengaktifkan `forbid(unsafe_code)` harus tetap ter-*compile* dan lolos CI.

### STD-0005: Iterasi & alokasi deterministik

- **Rule:** Urutan iterasi query dan alokasi `Entity` id **MUST** hanya bergantung pada urutan operasi — bukan pada timing thread atau alamat memori.
- **Why:** Invarian determinisme by construction (§2).
- **Verify:** Tes: urutan operasi yang identik pada dua run/proses berbeda menghasilkan snapshot yang bit-identik.

### STD-0006: Paralel setara serial

- **Rule:** Menjalankan sebuah jadwal secara multi-thread **MUST** menghasilkan keadaan akhir yang identik dengan eksekusi single-thread untuk jadwal yang sama. *(Aktif mulai milestone scheduler.)*
- **Why:** Invarian paralelisme yang aman (§2).
- **Verify:** Tes membandingkan snapshot hasil eksekusi 1 thread vs N thread untuk jadwal yang sama.

### STD-0007: Keamanan referensi basi (generational)

- **Rule:** Mengakses `Entity` yang sudah di-despawn — meski slot indeksnya telah dipakai ulang — **MUST** gagal secara terdeteksi (mis. `None`/error), dan tidak pernah mengembalikan data entity lain.
- **Why:** Invarian struktural aman (§2).
- **Verify:** Tes: spawn → despawn → spawn (memakai ulang slot) → akses handle lama mengembalikan `None`.

### STD-0008: Error berkonteks

- **Rule:** Error runtime **SHOULD** menyebut entity/komponen/sistem yang terlibat.
- **Why:** Prinsip "pesan error mengajari" (Philosophy §3).
- **Verify:** Tes snapshot atas pesan error untuk konflik borrow query dan komponen tak terdaftar.

> Prioritaskan aturan yang benar-benar bisa diverifikasi otomatis — aturan yang hanya bisa dinilai manusia sebaiknya tinggal di Philosophy.

## Menuju conformance yang machine-checkable

Saat proyek matang, promosikan standar menjadi artefak yang dapat dieksekusi:

1. **Schema** ([`schema/v1/*.json`](schema/v1/)) untuk struktur data — mis. format snapshot (STD-0001).
2. **Katalog aturan** — daftar STD dalam bentuk data, divalidasi oleh schema.
3. **Fixture + validator** yang menjalankan schema terhadap contoh valid dan tak-valid.

Dengan begitu, spesifikasi tidak bisa diam-diam menyimpang dari implementasi.

### Contoh yang sudah tersedia

Template ini menyertakan satu kontrak lengkap sebagai teladan — katalog aturan ini sendiri dalam bentuk data:

| Artefak | Berkas |
| --- | --- |
| Schema katalog aturan v1 | [`schema/v1/conformance-rule-catalog.schema.json`](schema/v1/conformance-rule-catalog.schema.json) |
| Contoh valid & tak-valid | [`schema/examples/`](schema/examples/) |
| Kontrak validator (language-neutral) | [`spec/VALIDATOR_CONTRACT.md`](spec/VALIDATOR_CONTRACT.md) |
| Implementasi + conformance suite | [`validators/`](validators/) |

Jalankan `python3 validators/python/validate.py schema/v1/conformance-rule-catalog.schema.json schema/examples/*.json` dari dalam `docs/` untuk melihatnya bekerja. Validator didefinisikan sebagai **kontrak**, jadi bahasa implementasinya bebas diganti selama lolos [conformance suite](validators/conformance/). Tiru pola ini untuk setiap kontrak baru — termasuk format snapshot ketika STD-0001/0002 dipromosikan menjadi schema.
