# Validator Contract v1

> Kontrak **language-neutral** untuk validator schema template ini. Ia mendefinisikan *perilaku* yang harus dipenuhi setiap implementasi — bukan bahasanya. Implementasi apa pun (Python, TypeScript, Rust, Go, …) dianggap **conformant** bila lolos [conformance suite](../validators/conformance/) tanpa perkecualian.
>
> Dengan begitu bahasa pemrograman validator menjadi **pilihan di pinggir sistem**, persis seperti invarian *independensi vendor* di [ARCHITECTURE_BIBLE](../ARCHITECTURE_BIBLE.md) §2.

## 1. Tugas

Diberi sebuah **schema** (subset JSON Schema draft-07, §4) dan sebuah **instance** (dokumen JSON), validator menentukan apakah instance **valid** terhadap schema, dan bila tidak, mengeluarkan daftar **error kanonik** (§3).

## 2. Antarmuka baris perintah

Untuk dapat dijalankan oleh conformance runner, implementasi **MUST** menyediakan invokasi:

```text
<impl> --json <schema-path> <instance-path> [<instance-path> ...]
```

- `--json` mengaktifkan **output kanonik** (§3). Tanpa `--json`, implementasi bebas mencetak format terbaca-manusia apa pun.
- **stdout** (mode `--json`) **MUST** berupa satu **array JSON**, satu elemen per instance, terurut sesuai argumen:

  ```json
  [
    { "instance": "<path apa adanya>", "valid": true,  "errors": [] },
    { "instance": "<path apa adanya>", "valid": false, "errors": [ /* §3 */ ] }
  ]
  ```

- **Exit code** **MUST**: `0` bila semua instance valid; `1` bila ada yang tidak valid; `2` bila argumen/berkas/parse bermasalah (pesan ke stderr).

> Detail lain (nama flag tambahan, warna, logging) bebas selama kontrak di atas dipenuhi.

## 3. Bentuk error kanonik

Setiap error **MUST** berupa objek dengan tepat dua field:

| Field | Isi |
| --- | --- |
| `path` | Lokasi di dalam instance sebagai **JSON Pointer** (RFC 6901). Root = `""`. |
| `keyword` | Nama keyword schema yang gagal (§4), mis. `pattern`, `required`. |

Aturan penempatan `path`:

- **`required`** → pointer ke anggota yang **seharusnya ada** tetapi hilang: `<objek>/<key>`.
- **`additionalProperties`** → pointer ke anggota tak dikenal yang muncul: `<objek>/<key>`.
- Keyword lain → pointer ke nilai yang gagal.

Aturan perbandingan:

- Daftar `errors` dibandingkan sebagai **HIMPUNAN** — urutan tak penting, duplikat diabaikan.
- Bila `type` gagal pada suatu nilai, implementasi **MAY** berhenti memeriksa keyword lain pada nilai itu (short-circuit) dan hanya melaporkan `type`. Conformance suite menghormati aturan ini.

Contoh (dari fixture tak-valid):

```json
[
  { "path": "/rules/0/id",        "keyword": "pattern" },
  { "path": "/rules/0/level",     "keyword": "enum" },
  { "path": "/rules/0/statement", "keyword": "minLength" },
  { "path": "/rules/1/verify",    "keyword": "required" },
  { "path": "/rules/1/note",      "keyword": "additionalProperties" }
]
```

## 4. Subset yang wajib didukung

Implementasi conformant **MUST** menegakkan keyword berikut:

| Kategori | Keyword |
| --- | --- |
| Umum | `type` (string atau array-of-string; `object`/`array`/`string`/`boolean`/`number`/`integer`/`null`), `enum`, `const` |
| String | `minLength`, `maxLength`, `pattern` (regex) |
| Angka | `minimum`, `maximum` |
| Array | `minItems`, `maxItems`, `uniqueItems`, `items` (schema tunggal) |
| Objek | `required`, `properties`, `additionalProperties` (boolean) |
| Referensi | `$ref` lokal berbentuk `#/...` (mis. `#/definitions/rule`) |

Aturan tipe yang perlu diperhatikan:

- `integer`/`number` **MUST NOT** menganggap boolean sebagai angka.
- `string` **MUST NOT** menganggap boolean sebagai string.

Keyword di luar daftar ini **MAY** diabaikan (tidak menghasilkan error) — tetapi implementasi yang mengabaikannya **SHOULD** mendokumentasikan batas itu. Menambah dukungan keyword baru **MUST** disertai kasus baru di conformance suite.

## 5. Versi & evolusi

- Kontrak ini adalah **v1**. Perubahan yang menambah keyword wajib atau mengubah bentuk output adalah perubahan yang **breaking** dan memerlukan RFC + ADR serta versi baru (`VALIDATOR_CONTRACT` v2), sesuai [ADR-0001](../ADR/ADR-0001-documentation-first.md).
- Menambah *kasus* ke conformance suite yang masih dalam subset v1 bukan perubahan breaking.

## 6. Cara membuktikan sebuah implementasi

Lihat [../validators/conformance/run.md](../validators/conformance/run.md). Ringkas: arahkan conformance runner ke perintah implementasimu; runner menjalankan setiap kasus dan membandingkan `valid` + himpunan `errors` terhadap ekspektasi. Implementasi conformant menghasilkan **0 selisih**.
