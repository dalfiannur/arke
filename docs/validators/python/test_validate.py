#!/usr/bin/env python3
"""Tes unit untuk implementasi referensi validate.py — tanpa dependensi.

Ini menguji impl Python secara langsung (error kanonik terstruktur). Bukti
lintas-bahasa yang sesungguhnya ada di ../conformance/ (runner + cases.json).

Jalankan:
  python3 -m unittest discover -s docs/validators/python
atau:
  python3 docs/validators/python/test_validate.py
"""
from __future__ import annotations

import json
import os
import unittest

from validate import validate

HERE = os.path.dirname(os.path.abspath(__file__))
SCHEMA_DIR = os.path.normpath(os.path.join(HERE, "..", "..", "schema"))


def _load(*parts):
    with open(os.path.join(SCHEMA_DIR, *parts), encoding="utf-8") as fh:
        return json.load(fh)


def _pairs(errors):
    return {(e["path"], e["keyword"]) for e in errors}


class CatalogFixtures(unittest.TestCase):
    def setUp(self):
        self.schema = _load("v1", "conformance-rule-catalog.schema.json")

    def test_valid_example_passes(self):
        instance = _load("examples", "conformance-rule-catalog.valid.json")
        self.assertEqual(validate(instance, self.schema), [])

    def test_invalid_example_reports_exact_error_set(self):
        instance = _load("examples", "conformance-rule-catalog.invalid.json")
        self.assertEqual(
            _pairs(validate(instance, self.schema)),
            {
                ("/rules/0/id", "pattern"),
                ("/rules/0/level", "enum"),
                ("/rules/0/statement", "minLength"),
                ("/rules/1/verify", "required"),
                ("/rules/1/note", "additionalProperties"),
            },
        )

    def test_type_errors_short_circuit(self):
        instance = _load("examples", "conformance-rule-catalog.type-errors.json")
        self.assertEqual(
            _pairs(validate(instance, self.schema)),
            {("/schema_version", "type"), ("/rules", "minItems")},
        )

    def test_bad_version_pattern_at_root(self):
        instance = _load("examples", "conformance-rule-catalog.bad-version.json")
        self.assertEqual(
            _pairs(validate(instance, self.schema)),
            {("/schema_version", "pattern")},
        )


class SubsetKeywords(unittest.TestCase):
    def test_required_points_at_missing_member(self):
        errs = validate({}, {"type": "object", "required": ["a"]})
        self.assertEqual(_pairs(errs), {("/a", "required")})

    def test_enum(self):
        self.assertEqual(_pairs(validate("X", {"enum": ["A", "B"]})), {("", "enum")})
        self.assertEqual(validate("A", {"enum": ["A", "B"]}), [])

    def test_pattern(self):
        s = {"type": "string", "pattern": "^STD-[0-9]{4}$"}
        self.assertEqual(validate("STD-0001", s), [])
        self.assertEqual(_pairs(validate("STD-1", s)), {("", "pattern")})

    def test_integer_not_bool(self):
        self.assertEqual(_pairs(validate(True, {"type": "integer"})), {("", "type")})
        self.assertEqual(validate(3, {"type": "integer"}), [])

    def test_string_not_bool(self):
        self.assertEqual(_pairs(validate(True, {"type": "string"})), {("", "type")})

    def test_min_items(self):
        self.assertEqual(_pairs(validate([], {"type": "array", "minItems": 1})), {("", "minItems")})

    def test_unique_items(self):
        s = {"type": "array", "uniqueItems": True}
        self.assertEqual(_pairs(validate([1, 1], s)), {("", "uniqueItems")})
        self.assertEqual(validate([1, 2], s), [])

    def test_local_ref_and_pointer_nesting(self):
        root = {
            "type": "object",
            "properties": {"x": {"$ref": "#/definitions/pos"}},
            "definitions": {"pos": {"type": "integer", "minimum": 0}},
        }
        self.assertEqual(validate({"x": 5}, root, root), [])
        self.assertEqual(_pairs(validate({"x": -1}, root, root)), {("/x", "minimum")})

    def test_json_pointer_escaping(self):
        s = {"type": "object", "required": ["a/b"]}
        self.assertEqual(_pairs(validate({}, s)), {("/a~1b", "required")})


if __name__ == "__main__":
    unittest.main()
