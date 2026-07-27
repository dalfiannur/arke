#!/usr/bin/env python3
"""Implementasi REFERENSI validator kontrak (subset JSON Schema draft-07).

Ini adalah SATU implementasi dari kontrak di ../../spec/VALIDATOR_CONTRACT.md.
Bahasa lain bebas mengganti file ini selama lolos conformance suite yang sama
(../conformance/). Tanpa dependensi eksternal: Python 3 standar saja.

Keyword yang didukung:
  type, required, properties, additionalProperties (bool),
  enum, const, pattern, minLength, maxLength,
  minItems, maxItems, uniqueItems, items,
  minimum, maximum, $ref (lokal: "#/...").

Output:
  - Mode manusia (default): baris VALID/INVALID + daftar error terbaca.
  - Mode kanonik (--json): array JSON, satu objek per instance:
      {"instance": <path>, "valid": <bool>,
       "errors": [{"path": <JSON Pointer>, "keyword": <str>}, ...]}
    Error dibandingkan sebagai HIMPUNAN (urutan tak penting).

Exit code: 0 semua valid, 1 ada yang tidak valid, 2 argumen/berkas bermasalah.
"""
from __future__ import annotations

import json
import re
import sys

_JSON_TYPES = {"object": dict, "array": list, "string": str, "null": type(None)}


def _esc(token: str) -> str:
    """Escape satu segmen JSON Pointer (RFC 6901)."""
    return token.replace("~", "~0").replace("/", "~1")


def _type_matches(value, expected: str) -> bool:
    if expected == "integer":
        return isinstance(value, int) and not isinstance(value, bool)
    if expected == "number":
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    if expected == "boolean":
        return isinstance(value, bool)
    py = _JSON_TYPES.get(expected)
    if py is None:
        return True  # tipe tak dikenal: jangan gagalkan diam-diam
    if py is str:
        return isinstance(value, str) and not isinstance(value, bool)
    return isinstance(value, py)


def _resolve_ref(ref: str, root: dict):
    if not ref.startswith("#/"):
        raise ValueError(f"hanya $ref lokal yang didukung, dapat: {ref!r}")
    node = root
    for token in ref[2:].split("/"):
        node = node[token.replace("~1", "/").replace("~0", "~")]
    return node


def validate(instance, schema: dict, root: dict | None = None, path: str = "") -> list[dict]:
    """Kembalikan daftar error kanonik: [{"path": <pointer>, "keyword": <str>}].

    Kosong berarti valid. `path` adalah JSON Pointer (root = "").
    """
    if root is None:
        root = schema
    errors: list[dict] = []

    if "$ref" in schema:
        return validate(instance, _resolve_ref(schema["$ref"], root), root, path)

    if "type" in schema:
        expected = schema["type"]
        expected_list = expected if isinstance(expected, list) else [expected]
        if not any(_type_matches(instance, t) for t in expected_list):
            return [{"path": path, "keyword": "type"}]  # cek lanjutan tak bermakna

    if "const" in schema and instance != schema["const"]:
        errors.append({"path": path, "keyword": "const"})

    if "enum" in schema and instance not in schema["enum"]:
        errors.append({"path": path, "keyword": "enum"})

    if isinstance(instance, str) and not isinstance(instance, bool):
        if "minLength" in schema and len(instance) < schema["minLength"]:
            errors.append({"path": path, "keyword": "minLength"})
        if "maxLength" in schema and len(instance) > schema["maxLength"]:
            errors.append({"path": path, "keyword": "maxLength"})
        if "pattern" in schema and re.search(schema["pattern"], instance) is None:
            errors.append({"path": path, "keyword": "pattern"})

    if isinstance(instance, (int, float)) and not isinstance(instance, bool):
        if "minimum" in schema and instance < schema["minimum"]:
            errors.append({"path": path, "keyword": "minimum"})
        if "maximum" in schema and instance > schema["maximum"]:
            errors.append({"path": path, "keyword": "maximum"})

    if isinstance(instance, list):
        if "minItems" in schema and len(instance) < schema["minItems"]:
            errors.append({"path": path, "keyword": "minItems"})
        if "maxItems" in schema and len(instance) > schema["maxItems"]:
            errors.append({"path": path, "keyword": "maxItems"})
        if schema.get("uniqueItems") and _has_duplicates(instance):
            errors.append({"path": path, "keyword": "uniqueItems"})
        if "items" in schema:
            for i, item in enumerate(instance):
                errors += validate(item, schema["items"], root, f"{path}/{i}")

    if isinstance(instance, dict):
        for key in schema.get("required", []):
            if key not in instance:
                errors.append({"path": f"{path}/{_esc(key)}", "keyword": "required"})
        props = schema.get("properties", {})
        for key, value in instance.items():
            child = f"{path}/{_esc(key)}"
            if key in props:
                errors += validate(value, props[key], root, child)
            elif schema.get("additionalProperties") is False:
                errors.append({"path": child, "keyword": "additionalProperties"})

    return errors


def _has_duplicates(items: list) -> bool:
    seen: set[str] = set()
    for item in items:
        key = json.dumps(item, sort_keys=True)
        if key in seen:
            return True
        seen.add(key)
    return False


def _format_human(path: str, keyword: str) -> str:
    where = path if path else "(root)"
    return f"{where}: gagal '{keyword}'"


def _load(path: str):
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)


def main(argv: list[str]) -> int:
    args = argv[1:]
    as_json = False
    if args and args[0] == "--json":
        as_json = True
        args = args[1:]
    if len(args) < 2:
        print("Pemakaian: validate.py [--json] <schema.json> <instance.json> ...")
        return 2
    try:
        schema = _load(args[0])
    except (OSError, json.JSONDecodeError) as exc:
        print(f"gagal memuat schema {args[0]}: {exc}", file=sys.stderr)
        return 2

    all_ok = True
    results = []
    for instance_path in args[1:]:
        try:
            instance = _load(instance_path)
        except (OSError, json.JSONDecodeError) as exc:
            print(f"gagal memuat instance {instance_path}: {exc}", file=sys.stderr)
            return 2
        errors = validate(instance, schema)
        ok = not errors
        all_ok = all_ok and ok
        results.append({"instance": instance_path, "valid": ok, "errors": errors})

    if as_json:
        print(json.dumps(results, ensure_ascii=False, indent=2))
    else:
        for res in results:
            if res["valid"]:
                print(f"VALID    {res['instance']}")
            else:
                print(f"INVALID  {res['instance']}")
                for err in res["errors"]:
                    print(f"    - {_format_human(err['path'], err['keyword'])}")
    return 0 if all_ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
