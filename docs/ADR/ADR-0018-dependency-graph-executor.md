# ADR-0018: Eksekutor graf-ketergantungan

- **Status:** Accepted <!-- Proposed | Accepted | Deprecated | Superseded by ADR-XXXX -->
- **Tanggal:** 2026-07-28
- **RFC terkait:** [RFC-0018](../RFC/RFC-0018-dependency-graph-executor.md)

## Konteks

Eksekutor stage (RFC-0016) memakai *barrier* penuh antar-stage: sistem menunggu **seluruh** stage sebelumnya, walau hanya berkonflik dengan sebagian. Utilisasi thread rugi saat beban sistem tak seimbang. Analisis konflik (M-2/M-4) sudah cukup untuk membangun graf-ketergantungan yang lebih halus.

## Keputusan

Kami memilih:

1. **Graf-ketergantungan (DAG)**: sisi `j → i` untuk tiap `j < i` yang aksesnya berkonflik. Urutan registrasi → pasangan berkonflik terurut → **hasil identik serial** (STD-0006). Sisi transitif redundan dibiarkan (tak merugikan; reduksi = optimasi lanjutan).
2. **`run_parallel` di-reimplementasi di atas DAG** (bukan API baru): worker-pool `std::thread::scope` + `Mutex<GraphState>` (`ready`/`pending`/`remaining`) + `Condvar`. Tiap sistem `Mutex<&mut System>`, di-*lock* tepat sekali → **aman, tanpa `unsafe` baru**.
3. **Segmentasi `Exclusive`**: *run* maksimal sistem `Shared` → DAG; `Exclusive` → serial (barrier `&mut World`). Mempertahankan determinisme menyeberang barrier.
4. **`dependencies()`** publik mengekspos graf (pendahulu per-sistem); **`stages()` tetap** untuk introspeksi/kompatibilitas.

## Konsekuensi

**Positif:**

- Paralelisme lebih tinggi (tanpa barrier stage penuh) dengan *critical path* sama → memperkuat *ergonomis = cepat*.
- API tak berubah; hasil & urutan efektif identik (STD-0006).
- 100% aman (memakai kembali `SyncWorld` terkurung, miri-verified).

**Negatif / biaya:**

- Koordinasi runtime (mutex+condvar) lebih rumit dari spawn-per-stage.
- Sisi redundan menaikkan penghitung `pending` (tak memengaruhi kebenaran/critical path).

**Netral / catatan:**

- `Exclusive` tetap barrier serial (segmentasi) — integrasi penuh menunggu command buffer.
- Kebenaran bersandar pada analisis konflik M-2/M-4 (sama seperti stage).

## Alternatif yang ditolak

- **Metode `run_graph` terpisah** — dua API paralel; DAG identik-hasil & lebih baik, jadi jadikan default.
- **Reduksi transitif** — tak mengubah kebenaran/critical path; ditunda.
- **`Exclusive` sebagai simpul DAG** — butuh `&mut World` di tengah `scope` berbagi; segmentasi lebih sederhana.

Rincian pertimbangan ada di [RFC-0018](../RFC/RFC-0018-dependency-graph-executor.md).
