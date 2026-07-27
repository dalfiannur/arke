# Rust implementation

Implementasi [Validator Contract v1](../../spec/VALIDATOR_CONTRACT.md) dengan Rust, **tanpa dependensi** — tanpa `serde`, tanpa `regex`, tanpa cargo/registry. Berisi parser JSON minimal dan pencocok regex subset untuk keyword `pattern`, sesuai etos "yang berat/vendor di pinggir" ([ARCHITECTURE_BIBLE](../../ARCHITECTURE_BIBLE.md) §2).

## Build

Cukup `rustc` — tidak perlu jaringan:

```bash
cd docs/validators/rust
rustc -O validate.rs -o validate
```

Biner hasil (`validate`) sengaja **tidak dilacak git** (lihat `.gitignore`); ia dibangun ulang dari sumber.

## Jalankan

```bash
cd docs

# mode terbaca-manusia
validators/rust/validate \
  schema/v1/conformance-rule-catalog.schema.json \
  schema/examples/conformance-rule-catalog.valid.json

# mode kanonik (dipakai conformance runner)
validators/rust/validate --json \
  schema/v1/conformance-rule-catalog.schema.json \
  schema/examples/conformance-rule-catalog.invalid.json
```

## Buktikan conformance

```bash
cd docs/validators/rust && rustc -O validate.rs -o validate && cd ../conformance
python3 run_conformance.py -- ../rust/validate
```

Harus melaporkan `4/4 kasus cocok`.

## Catatan implementasi

- **JSON parser** buatan tangan (objek/array/string/angka/bool/null, escape `\uXXXX`).
- **Regex** subset backtracking: `^ $ . \d \w \s`, kelas `[...]` dengan rentang & negasi, kuantifier `* + ? {n} {n,} {n,m}`. Cukup untuk pola schema umum; bukan mesin regex lengkap. Menambah kebutuhan regex yang lebih kaya sebaiknya dipertimbangkan sebagai keputusan (beralih ke crate `regex`) lewat RFC/ADR.
- Panjang string diukur dalam **Unicode scalar values** (`chars().count()`), selaras implementasi referensi Python. Fixture saat ini ASCII sehingga tak ada perbedaan teramati antar-bahasa.
