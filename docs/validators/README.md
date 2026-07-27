# Validators

Validator template ini didefinisikan sebagai **kontrak language-neutral**, bukan sebagai satu program. Bahasa pemrograman menjadi pilihan di pinggir — persis invarian *independensi vendor*.

- **Kontrak** — [`../spec/VALIDATOR_CONTRACT.md`](../spec/VALIDATOR_CONTRACT.md): perilaku, antarmuka CLI, bentuk error kanonik, dan subset keyword yang wajib.
- **Conformance suite** — [`conformance/`](conformance/): fixtures + runner yang membuktikan implementasi apa pun memenuhi kontrak.
- **Implementasi referensi** — [`python/`](python/): satu implementasi (tanpa dependensi). Boleh diganti/ditambah bahasa lain.

```text
validators/
├── python/                implementasi referensi
│   ├── validate.py        subset JSON Schema draft-07, mode --json kanonik
│   └── test_validate.py   tes unit impl (error terstruktur)
├── node/                  implementasi Node.js (tanpa dependensi)
│   └── validate.js        output kanonik identik dengan python/
├── rust/                  implementasi Rust (tanpa dependensi; parser+regex tangan)
│   └── validate.rs        build: rustc -O validate.rs -o validate
└── conformance/           bukti lintas-bahasa
    ├── cases.json         suite machine-readable (schema + instance + expect)
    ├── run_conformance.py runner referensi (memanggil impl apa pun)
    └── run.md             cara menjalankan + algoritma runner
```

## Jalan cepat

```bash
# Buktikan implementasi referensi lolos kontrak
cd docs/validators/conformance
python3 run_conformance.py -- python3 ../python/validate.py

# Tes unit implementasi referensi
cd docs/validators/python
python3 -m unittest discover -s .

# Pakai validator langsung (mode terbaca-manusia)
cd docs
python3 validators/python/validate.py \
  schema/v1/conformance-rule-catalog.schema.json \
  schema/examples/conformance-rule-catalog.valid.json \
  schema/examples/conformance-rule-catalog.invalid.json
```

## Menambah bahasa baru

Tulis validatormu di `<bahasa>/`, penuhi [kontrak](../spec/VALIDATOR_CONTRACT.md), lalu jalankan conformance runner terhadapnya sampai semua kasus cocok. Panduan lengkap: [`conformance/run.md`](conformance/run.md).

## Implementasi tersedia

| Bahasa | Lokasi | Status conformance |
| --- | --- | --- |
| Python 3 (referensi) | [`python/`](python/) | 4/4 kasus |
| Node.js | [`node/`](node/) | 4/4 kasus |
| Rust | [`rust/`](rust/) | 4/4 kasus |
| _(tambahkan milikmu)_ | | |

## Menghubungkan ke CI

Jalankan `run_conformance.py` untuk tiap implementasi + `unittest` untuk impl referensi. Selama langkah ini hijau, tidak ada implementasi yang boleh menyimpang dari kontrak, dan spesifikasi tak bisa diam-diam berbeda dari contoh.
