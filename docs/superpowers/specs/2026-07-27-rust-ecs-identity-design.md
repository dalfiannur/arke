# Design — Mengisi Placeholder Identitas & Arsitektur `Rust ECS`

- **Tanggal:** 2026-07-27
- **Status:** Disetujui (brainstorming)
- **Keluaran:** MANIFESTO, VISION, PHILOSOPHY, ARCHITECTURE_BIBLE, STANDARDS, MILESTONE_1

## Konteks

Repo `rust-ecs` adalah template tata-kelola *documentation-first* dengan seluruh dokumen identitas/arsitektur masih berupa placeholder. Tujuan sesi ini: mengisi placeholder itu untuk proyek nyata.

## Keputusan yang diambil (dari brainstorming)

1. **Jenis proyek:** Library produksi (dipublikasikan, dipakai orang), bukan sekadar proyek belajar.
2. **ECS = Entity-Component-System.**
3. **Diferensiasi:** standalone & minim dependensi, ergonomi + pesan error terbaik, performa archetype kelas atas, dan determinisme/serialisasi — dijalin menjadi satu positioning koheren.
4. **Trade-off tajam (jadi invarian):**
   - *Ergonomis = cepat* — jalur API paling natural HARUS jadi jalur tercepat; kalau bertabrakan, itu cacat desain, bukan pilihan pengguna.
   - *Determinisme by construction* — hasil harus deterministik apa pun penjadwalannya; paralelisme hanya bila terbukti setara dengan eksekusi serial.
5. **Persona inti:** developer Rust umum (bukan khusus game) — framing melebar ke game, simulasi, tooling, backend.
6. **Milestone kode pertama (M-1):** core storage + query minimal.

## Ringkasan isi tiap dokumen

- **MANIFESTO** — identitas standalone ECS; 3 keyakinan (ergonomis=cepat, determinisme, ECS sebagai struktur data bukan framework); penolakan (kunci-engine, fast path via `unsafe`, paralelisme yang mengorbankan determinisme diam-diam).
- **VISION** — ECS berperforma tinggi & deterministik sebagai "default membosankan"; 4 kemampuan pengguna; horizon pendek/menengah/panjang.
- **PHILOSOPHY** — 3 prinsip (jalur aman = jalur cepat; deterministik dulu; error mengajari) + tabel trade-off sadar + tanda keputusan buruk.
- **ARCHITECTURE_BIBLE** — 6 invarian (ergonomis=cepat, determinisme, paralelisme aman, portabilitas data, standalone core, struktural aman); model sistem berlapis System→Scheduler→Query→World→Archetype→Snapshot; tabel data & provenance (generational `Entity`, kepemilikan World, tick/generasi, provenance perubahan, snapshot portabel); batas produk; aturan evolusi; decision test 5 pertanyaan.
- **STANDARDS** — STD-0001..0008 yang verifiable (versi snapshot, round-trip, core standalone via cargo-deny, `forbid(unsafe_code)` di jalur pengguna, iterasi/alokasi deterministik, paralel=serial, keamanan generational, error berkonteks).
- **MILESTONE_1** — core storage & query minimal; ruang lingkup + DoD yang mengikat ke STD-0003/0004/0005/0007; artefak termasuk RFC-0002/ADR-0002.

## Batas & langkah berikutnya

- Sesi ini hanya mengisi placeholder dokumen yang sudah ada (pendekatan A). Tidak membuat RFC-0002/ADR-0002 — itu langkah berikutnya sebelum M-1 mulai koding.
- RN-0001 (registrasi komponen eksplisit vs otomatis) adalah kandidat catatan riset pertama.
