# Node.js implementation

Implementasi [Validator Contract v1](../../spec/VALIDATOR_CONTRACT.md) dengan Node.js standar — tanpa dependensi (`package.json` tak diperlukan).

## Jalankan

```bash
cd docs

# mode terbaca-manusia
node validators/node/validate.js \
  schema/v1/conformance-rule-catalog.schema.json \
  schema/examples/conformance-rule-catalog.valid.json

# mode kanonik (dipakai conformance runner)
node validators/node/validate.js --json \
  schema/v1/conformance-rule-catalog.schema.json \
  schema/examples/conformance-rule-catalog.invalid.json
```

## Buktikan conformance

```bash
cd docs/validators/conformance
node ../node/validate.js --version >/dev/null 2>&1 || true   # node harus terpasang
python3 run_conformance.py -- node ../node/validate.js
```

Harus melaporkan `4/4 kasus cocok`. Ini implementasi kedua (setelah [`../python/`](../python/)) — bukti bahwa kontrak, bukan bahasa, yang mengikat.
