# ADR-0028: CHANGELOG + kebijakan MSRV, semver & stabilitas snapshot

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-28
- **RFC terkait:** [RFC-0028](../RFC/RFC-0028-changelog-msrv-semver-policy.md)

## Konteks

1.0 (RN-0004 §3) menuntut kontrak stabilitas eksplisit. Kerja ini menyingkap MSRV
salah: let-chain (stabil 1.88) dipakai sementara `rust-version` = 1.86.

## Keputusan

1. **Koreksi MSRV → 1.88** (`Cargo.toml` + `README`).
2. **STD-0009 (MSRV)**: floor 1.88; naik hanya di minor/major, tak di patch;
   dicatat CHANGELOG; diverifikasi job CI MSRV.
3. **STD-0010 (semver & deprecation)**: semver; breaking hanya di major (pra-1.0:
   minor); usang via `#[deprecated]` + jendela ≥1 minor sebelum hapus.
4. **CHANGELOG.md** (Keep a Changelog); `[Unreleased]` = 0.6.0.
5. **Uji pin `SCHEMA_VERSION == 1`** mengoperasikan STD-0001/0002.
6. **Job MSRV CI** membangun `arke` pada 1.88 (pembukti empiris; rustup lokal tak
   tersedia).

## Konsekuensi

**Positif:**

- Kontrak stabilitas eksplisit & sebagian tercek-mesin (job MSRV, uji pin).
- MSRV dijujurkan; pengguna 1.86/1.87 tak lagi kena error kriptik.

**Negatif / biaya:**

- Job CI tambahan (toolchain MSRV).
- MSRV naik dari klaim 1.86 → 1.88 (tapi 1.86 memang tak pernah benar).

**Netral:**

- Murni kebijakan + koreksi; tak menyentuh perilaku runtime. Masuk 0.6.0.

## Alternatif yang ditolak

- **Tulis ulang let-chain untuk MSRV 1.86** — mengontorsi kode; 1.88 wajar.
- **Kebijakan tanpa CI** — tak-tercek, meluruh.

Rincian di [RFC-0028](../RFC/RFC-0028-changelog-msrv-semver-policy.md).
