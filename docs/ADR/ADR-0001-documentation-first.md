# ADR-0001: Tata-kelola documentation-first

- **Status:** Accepted
- **Tanggal:** 2026-07-27
- **RFC terkait:** [RFC-0001](../RFC/RFC-0001-documentation-first-governance.md)

## Konteks

Keputusan arsitektural yang tidak terekam akan hilang: alasannya menguap, implementasi menyimpang dari maksud, dan perdebatan yang sama berulang. Proyek ini menginginkan arah yang stabil dan keputusan yang dapat diaudit meski orang, teknologi, dan model berubah.

## Keputusan

Kami mengadopsi tata-kelola **documentation-first**. Setiap perubahan konsekuensial melewati rantai artefak:

```text
Manifesto → Architecture → RFC → ADR → Specification → Implementation → Tests
```

Proposal ditangkap sebagai **RFC**; keputusan yang diterima direkam sebagai **ADR**; pertanyaan terbuka ditampung sebagai **RN** hingga *graduate*. Implementasi mengikuti artefak ini dan tidak mendefinisikannya ulang secara diam-diam.

## Konsekuensi

**Positif:**

- Keputusan dapat ditelusuri; alternatif yang ditolak terdokumentasi.
- Arah arsitektur stabil terhadap pergantian orang dan teknologi.
- Implementasi tidak bisa menyimpang dari spesifikasi tanpa jejak.

**Negatif / biaya:**

- Ada overhead menulis untuk perubahan konsekuensial.
- Butuh disiplin menomori dan men-*supersede*, bukan menghapus.

**Netral / catatan:**

- RFC/ADR/RN diberi nomor berurutan dan bersifat append-only setelah diterima.
- Perubahan yang menyentuh invarian arsitektur wajib melewati RFC.

## Alternatif yang ditolak

- **Komentar PR saja** — tersebar dan tak dapat ditelusuri.
- **Wiki bebas** — tanpa lifecycle/versi, mudah usang tanpa jejak supersede.

Rincian pertimbangan ada di [RFC-0001](../RFC/RFC-0001-documentation-first-governance.md).
