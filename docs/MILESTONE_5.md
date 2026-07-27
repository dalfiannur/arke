# Milestone 5 — System-Level Parallelism (DEFERRED)

> **Status: DITUNDA.** Lihat [RFC-0006](RFC/RFC-0006-system-level-parallelism.md) / [ADR-0006](ADR/ADR-0006-defer-system-parallelism.md).

## Tujuan

Menjalankan sistem-sistem satu *stage* (yang aksesnya dijamin disjoint oleh M-2/M-4) di beberapa thread untuk percepatan tingkat-sistem.

## Mengapa ditunda

Implementasi yang **sound** menuntut membentuk `&mut [T]` ke data kolom dari `&World` bersama, yang hanya sah lewat penyimpanan berbasis `UnsafeCell`. Menulis `unsafe` aliasing tanpa verifikasi `miri` (tak tersedia di lingkungan ini) berisiko men-*ship* *undefined behavior*. Analisis lengkap di RFC-0006.

## Prasyarat (blocker)

- [ ] **RN-0001** graduate: rancangan penyimpanan kolom `UnsafeCell` (lihat [RN-0001](RN/RN-0001-unsafecell-column-storage.md)).
- [ ] **`miri` di CI**: job `cargo +nightly miri test` untuk memverifikasi soundness `unsafe`.
- [ ] RFC tersendiri untuk redesign storage disetujui.

## Sementara ini

Paralelisme yang tersedia adalah **data-parallel** via [`World::par_for_each`](../src/world.rs) (M-3) — sudah aman dan mengaktifkan STD-0006. Crate tetap **0 `unsafe`**.

## Ketergantungan

- **Butuh selesai lebih dulu:** M-4 (akses tersimpul) ✓ + prasyarat di atas.
- **Membuka jalan bagi:** command buffer (mutasi struktural tertunda saat paralel).
