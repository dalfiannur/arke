# RFC-0028: CHANGELOG + kebijakan MSRV, semver & stabilitas snapshot

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-28
- **Graduasi dari:** [RN-0004](../RN/RN-0004-jalan-menuju-1.0.md)
- **ADR terkait:** [ADR-0028](../ADR/ADR-0028-changelog-msrv-semver-policy.md)

## Ringkasan

Menetapkan **kontrak yang harus dikomit di 1.0** (RN-0004 §3): sebuah
**CHANGELOG** (Keep a Changelog), **kebijakan MSRV** (STD-0009), **kebijakan
semver & deprecation** (STD-0010), dan **penegakan stabilitas snapshot** (uji
pin `SCHEMA_VERSION`, mengoperasikan STD-0001). Sekaligus **mengoreksi MSRV yang
salah**: kode memakai *let-chain* (stabil di 1.88) tetapi `rust-version`
menyatakan 1.86.

## Motivasi

1.0 = janji stabilitas. Tanpa kebijakan tertulis + tercek, "stabil" tak
terdefinisi: pengguna tak tahu toolchain apa yang didukung, kapan API boleh
patah, atau apakah snapshot lama tetap terbaca.

**Temuan** (dari kerja kebijakan ini): `src/world.rs` memakai let-chain
(`if let (Some(a), Some(b)) = … && a == b`) — stabil **Rust 1.88** — sementara
`Cargo.toml` menyatakan `rust-version = "1.86"`. Pengguna 1.86/1.87 mendapat error
kompilasi kriptik. MSRV yang dideklarasi **salah** dan harus dikoreksi ke 1.88.
Inilah tepatnya yang dicegah kebijakan MSRV + job CI.

## Usulan rinci

### 1. Koreksi MSRV → 1.88

- `Cargo.toml`: `rust-version = "1.88"`.
- `README.md`: "Rust 1.88+".

### 2. STD-0009 — Kebijakan MSRV

- **Rule:** MSRV = **Rust 1.88**. **MAY** naik di rilis **minor/major**; **MUST
  NOT** naik di rilis **patch**. Kenaikan **MUST** dicatat di CHANGELOG.
- **Verify:** Job CI membangun `arke` pada toolchain MSRV persis.

### 3. STD-0010 — Kebijakan semver & deprecation

- **Rule:** Ikuti **semver**. Perubahan breaking hanya di **major** (pra-1.0:
  boleh di minor). Item usang **MUST** memakai `#[deprecated]` + `note`, dengan
  **jendela ≥ 1 rilis minor** sebelum dihapus. Penghapusan pasca-1.0 hanya di
  **major** berikutnya.
- **Verify:** CHANGELOG mencatat bagian `Deprecated`/`Removed`/breaking; item
  usang membawa atribut (mis. RFC-0027).

### 4. CHANGELOG.md

Format [Keep a Changelog](https://keepachangelog.com). Bagian `[Unreleased]`
menampung 0.6.0 (seal RFC-0026, deprecate RFC-0027, kebijakan RFC-0028, koreksi
MSRV). Rilis 0.5.x diringkas; riwayat penuh di GitHub Releases.

### 5. Penegakan stabilitas snapshot

Uji **pin** `SCHEMA_VERSION == 1` (mengoperasikan STD-0001/0002): menaikkannya
tanpa sengaja **menggagalkan** uji, memaksa keputusan sadar + jalur migrasi +
entri CHANGELOG.

## Verifikasi (TDD)

- **Uji pin snapshot:** karakterisasi/guard — memastikan format snapshot tak
  bergeser diam-diam. Dijalankan `cargo test`.
- **Job MSRV di CI:** membangun `arke` pada 1.88 — **secara empiris** menetapkan
  floor (menangkap koreksi 1.86→1.88 ini dan regresi MSRV mendatang). Rustup lokal
  tak tersedia di mesin dev; CI adalah pembukti empiris.

## Dampak

- **Kompatibilitas:** koreksi MSRV bukan breaking *perilaku* — hanya menjujurkan
  floor yang sudah ada de-facto. Masuk 0.6.0.
- **Menuju 1.0:** kontrak stabilitas menjadi eksplisit & sebagian tercek-mesin —
  prasyarat freeze yang jujur.

## Alternatif yang dipertimbangkan

| Alternatif | Mengapa tidak |
| --- | --- |
| Tulis ulang let-chain, pertahankan 1.86 | Mengontorsi kode demi MSRV lebih rendah; 1.88 (>1 thn) wajar untuk edition 2024 |
| Kebijakan tanpa job CI | Kebijakan tak-tercek meluruh; job MSRV membuatnya empiris |
| CHANGELOG retroaktif penuh 0.1–0.5 | Usaha besar, detail per-rilis tak pasti; GitHub Releases sudah jadi sumber |

## Keputusan

Diterima. Lihat [ADR-0028](../ADR/ADR-0028-changelog-msrv-semver-policy.md).
