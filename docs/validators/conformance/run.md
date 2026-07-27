# Menjalankan conformance suite

Suite ini membuktikan sebuah implementasi validator memenuhi
[Validator Contract v1](../../spec/VALIDATOR_CONTRACT.md) — apa pun bahasanya.

## Membuktikan sebuah implementasi

Panggil runner dengan `--` diikuti perintah untuk memanggil implementasimu:

```bash
cd docs/validators/conformance

# Implementasi referensi (Python)
python3 run_conformance.py -- python3 ../python/validate.py

# Contoh implementasi lain (belum disertakan — tinggal tambah folder)
python3 run_conformance.py -- node ../node/validate.js
python3 run_conformance.py -- ../rust/target/release/validate
```

Keluaran yang diharapkan untuk implementasi conformant:

```text
PASS  katalog valid lolos tanpa error
PASS  cacat properti tiap-rule terdeteksi
PASS  type dan minItems ditegakkan (type short-circuit)
PASS  pattern versi katalog ditegakkan di root

4/4 kasus cocok.
```

Exit code `0` = conformant, `1` = ada selisih (dicetak sebagai `HILANG`/`BERLEBIH`).

## Cara kerja runner (algoritma)

Runner tidak terikat Python; siapa pun boleh menulis ulang dengan langkah berikut:

1. Baca [`cases.json`](cases.json). Resolusi `schema` dan tiap `instance` relatif terhadap lokasi `cases.json`.
2. Untuk tiap kasus, panggil implementasi: `<impl> --json <schema> <instance>`.
3. Ambil elemen pertama array JSON di stdout → `{valid, errors}`.
4. Bandingkan:
   - `valid` sama dengan `expect.valid`;
   - himpunan `{(path, keyword)}` sama persis (urutan/duplikat diabaikan);
   - exit code konsisten (`0` ⟺ valid, `1` ⟺ tidak valid).
5. Conformant bila semua kasus cocok.

## Menambah kasus

Tambahkan objek ke `cases.json` yang menunjuk schema + instance (buat fixture di
`../../schema/examples/`) dan `expect` yang benar. Kasus baru **MUST** tetap
berada dalam subset keyword v1 ([kontrak §4](../../spec/VALIDATOR_CONTRACT.md));
jika butuh keyword baru, itu perubahan kontrak (RFC + ADR + versi baru).

## Menambah implementasi bahasa baru

1. Buat folder `../<bahasa>/` berisi validatormu.
2. Penuhi antarmuka CLI dan bentuk output kanonik di [kontrak](../../spec/VALIDATOR_CONTRACT.md) §2–§3.
3. Jalankan runner terhadapnya sampai `N/N kasus cocok`.
4. Catat implementasi baru di [`../README.md`](../README.md).

Tidak ada "bahasa resmi" — Python hanya kebetulan menjadi referensi pertama.
