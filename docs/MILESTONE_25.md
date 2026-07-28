# Milestone 25 — CHANGELOG + kebijakan MSRV/semver (menuju 1.0)

> Langkah bentuk-API ketiga & terakhir menuju 0.6.0 (RN-0004 §3). Menetapkan
> kontrak stabilitas yang harus dikomit di 1.0: CHANGELOG, kebijakan MSRV &
> semver, penegakan stabilitas snapshot. Menyingkap & mengoreksi MSRV yang salah.

## Tujuan

Mengimplementasikan [RFC-0028](RFC/RFC-0028-changelog-msrv-semver-policy.md).

## Ruang lingkup

**Termasuk:**

- `CHANGELOG.md` (Keep a Changelog); `[Unreleased]` = 0.6.0.
- `STD-0009` (MSRV) & `STD-0010` (semver/deprecation) di `STANDARDS.md`.
- Koreksi MSRV `1.86`→`1.88` (`Cargo.toml` + `README`) — kode pakai let-chain.
- Job CI `msrv` membangun `arke` pada 1.88 (penegak empiris).
- Uji pin `SCHEMA_VERSION == 1` (menegakkan STD-0001/0002).

**Tidak termasuk:**

- Rilis 0.6.0 itu sendiri (keputusan pemilik proyek).
- Milestone 1.0 (setelah soak).

## Definition of Done

- [ ] CHANGELOG + STD-0009/0010 ditulis; MSRV konsisten 1.88 di semua rujukan.
- [ ] Uji pin snapshot hijau; seluruh suite + fmt hijau (CI `-D warnings`).
- [ ] Job CI `msrv` (1.88) hijau — membuktikan floor secara empiris.
- [ ] Ketiga penghalang RN-0004 selesai → 0.6.0 siap dirilis.
