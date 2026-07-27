#!/usr/bin/env python3
"""Conformance runner untuk Validator Contract v1.

Membuktikan sebuah implementasi validator (bahasa APA PUN) lolos suite di
cases.json. Runner ini language-neutral: ia memanggil implementasi lewat
antarmuka baris perintah yang didefinisikan di ../../spec/VALIDATOR_CONTRACT.md
(`<impl> --json <schema> <instance>`) dan membandingkan output kanoniknya.

Pemakaian:
  python3 run_conformance.py -- <perintah-impl> [arg-tetap...]

Contoh:
  # implementasi referensi Python
  python3 run_conformance.py -- python3 ../python/validate.py

  # implementasi Node
  python3 run_conformance.py -- node ../node/validate.js

  # biner Rust
  python3 run_conformance.py -- ../rust/target/release/validate

Exit code: 0 bila semua kasus cocok (conformant), 1 bila ada selisih,
2 bila argumen/berkas bermasalah.

Catatan: runner ini kebetulan ditulis dengan Python, tetapi itu tak mengikat —
algoritmanya dijelaskan di run.md dan boleh ditulis ulang di bahasa lain.
"""
from __future__ import annotations

import json
import os
import subprocess
import sys


def _canon(errors) -> set[tuple[str, str]]:
    """Normalkan daftar error menjadi himpunan (path, keyword)."""
    return {(e["path"], e["keyword"]) for e in errors}


def run(impl_cmd: list[str], cases_path: str) -> int:
    here = os.path.dirname(os.path.abspath(cases_path))
    with open(cases_path, encoding="utf-8") as fh:
        suite = json.load(fh)

    schema_path = os.path.normpath(os.path.join(here, suite["schema"]))
    cases = suite["cases"]
    failures = 0

    for case in cases:
        name = case["name"]
        instance_path = os.path.normpath(os.path.join(here, case["instance"]))
        expect = case["expect"]

        proc = subprocess.run(
            impl_cmd + ["--json", schema_path, instance_path],
            capture_output=True,
            text=True,
        )
        if proc.returncode not in (0, 1):
            print(f"FAIL  {name}\n      impl keluar dengan kode {proc.returncode}: {proc.stderr.strip()}")
            failures += 1
            continue
        try:
            results = json.loads(proc.stdout)
            result = results[0]
        except (json.JSONDecodeError, IndexError, KeyError) as exc:
            print(f"FAIL  {name}\n      output --json tak sesuai kontrak: {exc}\n      stdout: {proc.stdout!r}")
            failures += 1
            continue

        got_valid = result.get("valid")
        got_errors = _canon(result.get("errors", []))
        want_valid = expect["valid"]
        want_errors = _canon(expect["errors"])

        exit_ok = (proc.returncode == 0) == want_valid
        if got_valid == want_valid and got_errors == want_errors and exit_ok:
            print(f"PASS  {name}")
            continue

        failures += 1
        print(f"FAIL  {name}")
        if got_valid != want_valid:
            print(f"      valid: harap {want_valid}, dapat {got_valid}")
        if not exit_ok:
            print(f"      exit code {proc.returncode} tak sesuai valid={want_valid}")
        missing = want_errors - got_errors
        extra = got_errors - want_errors
        for path, kw in sorted(missing):
            print(f"      HILANG   {path} [{kw}]")
        for path, kw in sorted(extra):
            print(f"      BERLEBIH {path} [{kw}]")

    total = len(cases)
    passed = total - failures
    print(f"\n{passed}/{total} kasus cocok.")
    return 0 if failures == 0 else 1


def main(argv: list[str]) -> int:
    if "--" not in argv:
        print("Pemakaian: run_conformance.py -- <perintah-impl> [arg-tetap...]")
        return 2
    impl_cmd = argv[argv.index("--") + 1:]
    if not impl_cmd:
        print("Perintah implementasi kosong setelah '--'.")
        return 2
    cases_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "cases.json")
    if not os.path.exists(cases_path):
        print(f"cases.json tak ditemukan di {cases_path}")
        return 2
    return run(impl_cmd, cases_path)


if __name__ == "__main__":
    sys.exit(main(sys.argv))
