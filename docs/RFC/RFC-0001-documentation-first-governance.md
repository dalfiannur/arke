# RFC-0001: Tata-kelola documentation-first

- **Status:** Accepted
- **Tanggal:** 2026-07-27
- **Milestone:** M-1 (Foundation)
- **ADR terkait:** [ADR-0001](../ADR/ADR-0001-documentation-first.md)

> Ini adalah **RFC contoh** yang terisi sebagai panduan gaya. Isinya berlaku untuk repo template itu sendiri; ganti/hapus saat kamu memulai proyek nyata. Perhatikan strukturnya: motivasi → usulan → alternatif → dampak.

## Ringkasan

Setiap perubahan konsekuensial pada Arke didahului artefak tertulis yang dapat ditinjau: proposal ditangkap sebagai **RFC**, keputusan yang diterima direkam sebagai **ADR**, dan implementasi mengikuti — bukan mendefinisikan ulang — artefak tersebut.

## Motivasi

Tanpa jejak keputusan yang eksplisit:

- alasan di balik desain hilang seiring waktu dan pergantian orang;
- implementasi diam-diam menyimpang dari maksud semula;
- perdebatan yang sama terulang karena tak ada catatan alternatif yang sudah ditolak.

Kita menginginkan sistem yang keputusannya **dapat diaudit** dan arahnya **stabil** meski teknologi berubah.

## Usulan rinci

Rantai artefak kanonik:

```text
Manifesto → Architecture → RFC → ADR → Specification → Implementation → Tests
```

Aturan:

1. Perubahan yang menyentuh invarian arsitektur, kontrak publik, atau kemampuan inti **MUST** melewati RFC.
2. RFC yang **Accepted** melahirkan **ADR** yang merekam keputusan dan konsekuensinya.
3. Pertanyaan terbuka yang belum matang ditampung sebagai **RN** hingga *graduate* menjadi RFC.
4. Kode dan schema **MUST** konsisten dengan RFC/ADR yang berlaku; menyimpang tanpa artefak baru dianggap bug proses.
5. Nomor RFC/ADR/RN berurutan dan tidak dipakai ulang; dokumen yang usang di-*supersede*, bukan dihapus.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| Hanya komentar di PR | Ringan | Tersebar, tak dapat ditelusuri, hilang saat repo dipindah | Tak memenuhi tujuan auditability |
| Wiki bebas | Fleksibel | Tanpa lifecycle/versi, mudah usang tanpa jejak | Tak ada disiplin supersede |
| Documentation-first (RFC/ADR/RN) | Jejak eksplisit, alternatif terekam, dekat dengan kode | Overhead menulis | Overhead sepadan untuk keputusan konsekuensial |

## Dampak

- **Kompatibilitas / migrasi:** tidak ada; ini keputusan proses.
- **Keamanan / izin / provenance:** memperkuat provenance keputusan.
- **Konsekuensi pada invarian:** mendukung seluruh invarian dengan menjadikan perubahannya eksplisit.

## Pertanyaan terbuka

- Tingkat perubahan mana yang cukup kecil untuk melewati RFC? `<tetapkan ambang di proyekmu.>`

## Keputusan

Diterima. Lihat [ADR-0001](../ADR/ADR-0001-documentation-first.md).
