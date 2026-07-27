#!/usr/bin/env node
"use strict";
/**
 * Implementasi Node.js dari Validator Contract v1.
 *
 * Kesetaraan dengan implementasi referensi Python dibuktikan oleh conformance
 * suite yang sama (../conformance/) — bukan oleh kemiripan kode. Lihat
 * ../../spec/VALIDATOR_CONTRACT.md untuk perilaku yang wajib dipenuhi.
 * Tanpa dependensi eksternal: Node standar saja.
 *
 * Keyword yang didukung:
 *   type, required, properties, additionalProperties (bool),
 *   enum, const, pattern, minLength, maxLength,
 *   minItems, maxItems, uniqueItems, items,
 *   minimum, maximum, $ref (lokal: "#/...").
 *
 * Output: sama seperti kontrak §3 (mode --json = array kanonik per instance).
 * Exit code: 0 semua valid, 1 ada yang tidak, 2 argumen/berkas bermasalah.
 */

const fs = require("fs");

/** Escape satu segmen JSON Pointer (RFC 6901). */
function esc(token) {
  return token.replace(/~/g, "~0").replace(/\//g, "~1");
}

/** Serialisasi kanonik (kunci terurut) untuk perbandingan nilai. */
function stable(value) {
  if (value === null || typeof value !== "object") {
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) {
    return "[" + value.map(stable).join(",") + "]";
  }
  const keys = Object.keys(value).sort();
  return "{" + keys.map((k) => JSON.stringify(k) + ":" + stable(value[k])).join(",") + "}";
}

function typeMatches(value, expected) {
  switch (expected) {
    case "integer":
      return typeof value === "number" && Number.isInteger(value);
    case "number":
      return typeof value === "number";
    case "boolean":
      return typeof value === "boolean";
    case "string":
      return typeof value === "string";
    case "object":
      return value !== null && typeof value === "object" && !Array.isArray(value);
    case "array":
      return Array.isArray(value);
    case "null":
      return value === null;
    default:
      return true; // tipe tak dikenal: jangan gagalkan diam-diam
  }
}

function resolveRef(ref, root) {
  if (!ref.startsWith("#/")) {
    throw new Error(`hanya $ref lokal yang didukung, dapat: ${JSON.stringify(ref)}`);
  }
  let node = root;
  for (const raw of ref.slice(2).split("/")) {
    node = node[raw.replace(/~1/g, "/").replace(/~0/g, "~")];
  }
  return node;
}

function hasDuplicates(items) {
  const seen = new Set();
  for (const item of items) {
    const key = stable(item);
    if (seen.has(key)) return true;
    seen.add(key);
  }
  return false;
}

/**
 * Kembalikan daftar error kanonik: [{path, keyword}].
 * Kosong berarti valid. `path` adalah JSON Pointer (root = "").
 */
function validate(instance, schema, root, path) {
  if (root === undefined) root = schema;
  if (path === undefined) path = "";
  const errors = [];

  if (Object.prototype.hasOwnProperty.call(schema, "$ref")) {
    return validate(instance, resolveRef(schema["$ref"], root), root, path);
  }

  if (Object.prototype.hasOwnProperty.call(schema, "type")) {
    const list = Array.isArray(schema.type) ? schema.type : [schema.type];
    if (!list.some((t) => typeMatches(instance, t))) {
      return [{ path, keyword: "type" }]; // cek lanjutan tak bermakna
    }
  }

  if (Object.prototype.hasOwnProperty.call(schema, "const") && stable(instance) !== stable(schema.const)) {
    errors.push({ path, keyword: "const" });
  }

  if (Object.prototype.hasOwnProperty.call(schema, "enum") &&
      !schema.enum.some((e) => stable(e) === stable(instance))) {
    errors.push({ path, keyword: "enum" });
  }

  if (typeof instance === "string") {
    if ("minLength" in schema && instance.length < schema.minLength) {
      errors.push({ path, keyword: "minLength" });
    }
    if ("maxLength" in schema && instance.length > schema.maxLength) {
      errors.push({ path, keyword: "maxLength" });
    }
    if ("pattern" in schema && !new RegExp(schema.pattern).test(instance)) {
      errors.push({ path, keyword: "pattern" });
    }
  }

  if (typeof instance === "number") {
    if ("minimum" in schema && instance < schema.minimum) {
      errors.push({ path, keyword: "minimum" });
    }
    if ("maximum" in schema && instance > schema.maximum) {
      errors.push({ path, keyword: "maximum" });
    }
  }

  if (Array.isArray(instance)) {
    if ("minItems" in schema && instance.length < schema.minItems) {
      errors.push({ path, keyword: "minItems" });
    }
    if ("maxItems" in schema && instance.length > schema.maxItems) {
      errors.push({ path, keyword: "maxItems" });
    }
    if (schema.uniqueItems && hasDuplicates(instance)) {
      errors.push({ path, keyword: "uniqueItems" });
    }
    if ("items" in schema) {
      instance.forEach((item, i) => {
        errors.push(...validate(item, schema.items, root, `${path}/${i}`));
      });
    }
  }

  if (instance !== null && typeof instance === "object" && !Array.isArray(instance)) {
    for (const key of schema.required || []) {
      if (!Object.prototype.hasOwnProperty.call(instance, key)) {
        errors.push({ path: `${path}/${esc(key)}`, keyword: "required" });
      }
    }
    const props = schema.properties || {};
    for (const [key, value] of Object.entries(instance)) {
      const child = `${path}/${esc(key)}`;
      if (Object.prototype.hasOwnProperty.call(props, key)) {
        errors.push(...validate(value, props[key], root, child));
      } else if (schema.additionalProperties === false) {
        errors.push({ path: child, keyword: "additionalProperties" });
      }
    }
  }

  return errors;
}

function load(path) {
  return JSON.parse(fs.readFileSync(path, "utf8"));
}

function main(argv) {
  let args = argv.slice(2);
  let asJson = false;
  if (args[0] === "--json") {
    asJson = true;
    args = args.slice(1);
  }
  if (args.length < 2) {
    console.log("Pemakaian: validate.js [--json] <schema.json> <instance.json> ...");
    return 2;
  }

  let schema;
  try {
    schema = load(args[0]);
  } catch (exc) {
    console.error(`gagal memuat schema ${args[0]}: ${exc.message}`);
    return 2;
  }

  let allOk = true;
  const results = [];
  for (const instancePath of args.slice(1)) {
    let instance;
    try {
      instance = load(instancePath);
    } catch (exc) {
      console.error(`gagal memuat instance ${instancePath}: ${exc.message}`);
      return 2;
    }
    const errors = validate(instance, schema);
    const ok = errors.length === 0;
    allOk = allOk && ok;
    results.push({ instance: instancePath, valid: ok, errors });
  }

  if (asJson) {
    console.log(JSON.stringify(results, null, 2));
  } else {
    for (const res of results) {
      if (res.valid) {
        console.log(`VALID    ${res.instance}`);
      } else {
        console.log(`INVALID  ${res.instance}`);
        for (const err of res.errors) {
          console.log(`    - ${err.path || "(root)"}: gagal '${err.keyword}'`);
        }
      }
    }
  }
  return allOk ? 0 : 1;
}

module.exports = { validate };

if (require.main === module) {
  process.exit(main(process.argv));
}
