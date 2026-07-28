//! Derive `#[derive(PgComponent)]` untuk `arke-postgres` (RFC-0021).
//!
//! Ditulis tangan memakai **hanya `proc_macro` bawaan** — nol dependensi
//! crates.io (pola `arke-derive`). Menurunkan skema kolom-tipe dari sebuah
//! struct field-bernama: `TABLE`, `COLUMNS`, `to_params`, `from_params`. Field
//! skalar → kolom ber-tipe; `Option<T>` → nullable; non-skalar → `JSONB`
//! (via `arke::Serialize`).

use proc_macro::{Delimiter, TokenStream, TokenTree};

/// Turunkan `arke_postgres::PgComponent` untuk struct field-bernama.
///
/// Tipe field dipetakan ke kolom SQL ber-tipe (RFC-0021 §3). Atribut opsional:
/// field `#[pg(index)]`/`#[pg(unique)]` → indeks; tipe `#[pg(check = "…")]` →
/// constraint `CHECK`. Tipe tak didukung, generic, atau non-struct → `compile_error!`.
#[proc_macro_derive(PgComponent, attributes(pg))]
pub fn derive_pg_component(input: TokenStream) -> TokenStream {
    let code = match expand(input) {
        Ok(code) => code,
        Err(message) => format!("::core::compile_error!({message:?});"),
    };
    code.parse().expect("kode hasil-derive tidak valid")
}

/// Satu field bernama + tipe (dinormalisasi ke string tanpa spasi) + atribut.
struct Field {
    name: String,
    ty: String,
    index: bool,
    unique: bool,
}

/// Satu item atribut `#[pg(...)]`.
enum PgItem {
    Index,
    Unique,
    Check(String),
}

/// Parse isi bracket `#[pg(...)]` → daftar item. Bracket non-`pg` → kosong.
fn parse_pg_items(bracket_stream: TokenStream) -> Result<Vec<PgItem>, String> {
    let toks: Vec<TokenTree> = bracket_stream.into_iter().collect();
    let Some(TokenTree::Ident(id)) = toks.first() else {
        return Ok(Vec::new());
    };
    if id.to_string() != "pg" {
        return Ok(Vec::new());
    }
    let Some(TokenTree::Group(g)) = toks.get(1) else {
        return Ok(Vec::new());
    };
    let inner: Vec<TokenTree> = g.stream().into_iter().collect();
    let mut items = Vec::new();
    let mut j = 0;
    while j < inner.len() {
        if let TokenTree::Ident(key) = &inner[j] {
            match key.to_string().as_str() {
                "index" => items.push(PgItem::Index),
                "unique" => items.push(PgItem::Unique),
                "check" => match (inner.get(j + 1), inner.get(j + 2)) {
                    (Some(TokenTree::Punct(p)), Some(TokenTree::Literal(lit)))
                        if p.as_char() == '=' =>
                    {
                        items.push(PgItem::Check(unquote(&lit.to_string())));
                        j += 2;
                    }
                    _ => return Err("pg(check = \"...\") tak valid".to_string()),
                },
                other => return Err(format!("atribut pg tak dikenal: `{other}`")),
            }
        }
        j += 1;
    }
    Ok(items)
}

/// Hilangkan kutip luar dari literal string.
fn unquote(s: &str) -> String {
    s.trim_matches('"').to_string()
}

fn expand(input: TokenStream) -> Result<String, String> {
    let tokens: Vec<TokenTree> = input.into_iter().collect();
    let mut i = 0;
    let mut checks: Vec<String> = Vec::new();

    // Lewati atribut & visibilitas; temukan `struct` (tolak enum/union).
    // Atribut tipe `#[pg(check = "...")]` dikumpulkan.
    while i < tokens.len() {
        match &tokens[i] {
            TokenTree::Punct(p) if p.as_char() == '#' => {
                if let Some(TokenTree::Group(g)) = tokens.get(i + 1)
                    && g.delimiter() == Delimiter::Bracket
                {
                    for item in parse_pg_items(g.stream())? {
                        if let PgItem::Check(expr) = item {
                            checks.push(expr);
                        }
                    }
                }
                i += 2; // '#' + grup bracket
                continue;
            }
            TokenTree::Ident(id) => match id.to_string().as_str() {
                "struct" => {
                    i += 1;
                    break;
                }
                "enum" | "union" => {
                    return Err("derive(PgComponent) hanya mendukung `struct`".to_string());
                }
                _ => {}
            },
            _ => {}
        }
        i += 1;
    }

    // Nama tipe.
    let name = match tokens.get(i) {
        Some(TokenTree::Ident(id)) => id.to_string(),
        _ => return Err("derive(PgComponent): nama struct tak ditemukan".to_string()),
    };
    i += 1;

    // Generic belum didukung.
    if let Some(TokenTree::Punct(p)) = tokens.get(i)
        && p.as_char() == '<'
    {
        return Err("derive(PgComponent): tipe generic belum didukung".to_string());
    }

    // Badan struct: harus field-bernama (grup brace).
    let fields = match tokens.get(i) {
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
            parse_named_fields(g.stream())?
        }
        _ => {
            return Err(
                "derive(PgComponent): hanya struct field-bernama yang didukung".to_string(),
            );
        }
    };
    if fields.is_empty() {
        return Err("derive(PgComponent): struct tanpa field tak didukung".to_string());
    }

    gen_impl(&name, &fields, &checks)
}

fn parse_named_fields(stream: TokenStream) -> Result<Vec<Field>, String> {
    let toks: Vec<TokenTree> = stream.into_iter().collect();
    let mut fields = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        // Atribut field: baca `#[pg(index|unique)]`, lewati lainnya.
        let mut index = false;
        let mut unique = false;
        while matches!(toks.get(i), Some(TokenTree::Punct(p)) if p.as_char() == '#') {
            i += 1;
            if let Some(TokenTree::Group(g)) = toks.get(i)
                && g.delimiter() == Delimiter::Bracket
            {
                for item in parse_pg_items(g.stream())? {
                    match item {
                        PgItem::Index => index = true,
                        PgItem::Unique => unique = true,
                        PgItem::Check(_) => {
                            return Err("pg(check) hanya valid di level-tipe".to_string());
                        }
                    }
                }
                i += 1;
            }
        }
        // Lewati visibilitas `pub` [`(..)`].
        if matches!(toks.get(i), Some(TokenTree::Ident(id)) if id.to_string() == "pub") {
            i += 1;
            if matches!(toks.get(i), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis)
            {
                i += 1;
            }
        }
        // Nama field.
        let name = match toks.get(i) {
            Some(TokenTree::Ident(id)) => id.to_string(),
            None => break,
            _ => return Err("derive(PgComponent): gagal mem-parse field".to_string()),
        };
        i += 1;
        // ':' pemisah.
        if !matches!(toks.get(i), Some(TokenTree::Punct(p)) if p.as_char() == ':') {
            return Err(format!("derive(PgComponent): field `{name}` tanpa tipe"));
        }
        i += 1;
        // Kumpulkan token tipe hingga koma tingkat-atas (lacak kedalaman `<>`).
        let mut ty = String::new();
        let mut depth = 0i32;
        while i < toks.len() {
            if let TokenTree::Punct(p) = &toks[i] {
                match p.as_char() {
                    '<' => depth += 1,
                    '>' => depth -= 1,
                    ',' if depth <= 0 => {
                        i += 1;
                        break;
                    }
                    _ => {}
                }
            }
            ty.push_str(&toks[i].to_string());
            i += 1;
        }
        fields.push(Field {
            name,
            ty,
            index,
            unique,
        });
    }
    Ok(fields)
}

/// Pemetaan satu tipe skalar Rust → SQL, berbasis **binding referensi** `v`.
struct Scalar {
    /// Varian `PgType` (untuk `COLUMNS`).
    pg_type: &'static str,
    /// Varian `PgValue` (untuk `to_params`/`from_params`).
    value: &'static str,
    /// Ekspresi bind dari `v: &INNER` → nilai di dalam `PgValue::<value>(…)`.
    to_ref: &'static str,
    /// Ekspresi rekonstruksi dari `v` (di-bind pola `PgValue::<value>(v)`).
    from_expr: &'static str,
}

/// Petakan tipe skalar (string tanpa spasi) → [`Scalar`]. `None` = tak didukung.
///
/// Semua ekspresi berasumsi `v` bertipe `&INNER` (untuk `to_ref`) atau
/// `&INNER` yang di-bind pola `PgValue` (untuk `from_expr`) — seragam antara
/// field skalar biasa dan pembungkus `Option<T>`.
fn inner_scalar(ty: &str) -> Option<Scalar> {
    let s = |pg, value, to_ref, from_expr| Scalar {
        pg_type: pg,
        value,
        to_ref,
        from_expr,
    };
    Some(match ty {
        "i8" => s("Integer", "Int", "*v as i64", "*v as i8"),
        "i16" => s("Integer", "Int", "*v as i64", "*v as i16"),
        "i32" => s("Integer", "Int", "*v as i64", "*v as i32"),
        "u8" => s("Integer", "Int", "*v as i64", "*v as u8"),
        "u16" => s("Integer", "Int", "*v as i64", "*v as u16"),
        "i64" => s("BigInt", "Int", "*v", "*v"),
        "isize" => s("BigInt", "Int", "*v as i64", "*v as isize"),
        "u32" => s("BigInt", "Int", "*v as i64", "*v as u32"),
        "u64" => s("Numeric", "Numeric", "v.to_string()", "v.parse().ok()?"),
        "usize" => s("Numeric", "Numeric", "v.to_string()", "v.parse().ok()?"),
        "f32" => s("Real", "Float", "*v as f64", "*v as f32"),
        "f64" => s("DoublePrecision", "Float", "*v", "*v"),
        "bool" => s("Boolean", "Bool", "*v", "*v"),
        "String" => s("Text", "Text", "v.clone()", "v.clone()"),
        _ => return None,
    })
}

/// Fragmen kode SQL/serde untuk satu field.
struct FieldSql {
    column: String,
    to_param: String,
    from_field: String,
}

/// `ColumnDef` sebagai teks kode.
fn column_def(name: &str, pg_type: &str, nullable: bool) -> String {
    format!(
        "::arke_postgres::ColumnDef {{ name: {name:?}, ty: ::arke_postgres::PgType::{pg_type}, nullable: {nullable} }}, "
    )
}

/// Hasilkan fragmen untuk satu field: skalar, `Option<skalar>`, atau — untuk
/// tipe non-skalar — kolom `JSONB` via `arke::Serialize` (fallback, RFC-0021 §2).
fn field_sql(f: &Field, idx: usize) -> Result<FieldSql, String> {
    let name = &f.name;

    // `Option<INNER>` → kolom nullable; None ↔ NULL.
    if let Some(inner) =
        f.ty.strip_prefix("Option<")
            .and_then(|r| r.strip_suffix('>'))
    {
        return Ok(match inner_scalar(inner) {
            Some(s) => FieldSql {
                column: column_def(name, s.pg_type, true),
                to_param: format!(
                    "match &self.{name} {{ \
                        ::core::option::Option::Some(v) => ::arke_postgres::PgValue::{}({}), \
                        ::core::option::Option::None => ::arke_postgres::PgValue::Null, \
                     }}, ",
                    s.value, s.to_ref
                ),
                from_field: format!(
                    "{name}: match values.get({idx}) {{ \
                        ::core::option::Option::Some(::arke_postgres::PgValue::Null) => ::core::option::Option::None, \
                        ::core::option::Option::Some(::arke_postgres::PgValue::{val}(v)) => ::core::option::Option::Some({from}), \
                        _ => return ::core::option::Option::None, \
                     }}, ",
                    val = s.value,
                    from = s.from_expr,
                ),
            },
            // Non-skalar → JSONB nullable via Serialize.
            None => FieldSql {
                column: column_def(name, "Jsonb", true),
                to_param: format!(
                    "match &self.{name} {{ \
                        ::core::option::Option::Some(v) => ::arke_postgres::PgValue::Json(::arke::Serialize::to_value(v).to_json()), \
                        ::core::option::Option::None => ::arke_postgres::PgValue::Null, \
                     }}, "
                ),
                from_field: from_jsonb(name, idx, inner, true),
            },
        });
    }

    // Skalar biasa (non-null).
    if let Some(s) = inner_scalar(&f.ty) {
        return Ok(FieldSql {
            column: column_def(name, s.pg_type, false),
            to_param: format!(
                "::arke_postgres::PgValue::{}({{ let v = &self.{name}; {} }}), ",
                s.value, s.to_ref
            ),
            from_field: format!(
                "{name}: match values.get({idx}) {{ \
                    ::core::option::Option::Some(::arke_postgres::PgValue::{val}(v)) => {from}, \
                    _ => return ::core::option::Option::None, \
                 }}, ",
                val = s.value,
                from = s.from_expr,
            ),
        });
    }

    // Fallback non-skalar → JSONB via Serialize.
    Ok(FieldSql {
        column: column_def(name, "Jsonb", false),
        to_param: format!(
            "::arke_postgres::PgValue::Json(::arke::Serialize::to_value(&self.{name}).to_json()), "
        ),
        from_field: from_jsonb(name, idx, &f.ty, false),
    })
}

/// Fragmen `from_params` untuk kolom JSONB (parse teks → `Value` → `from_value`).
fn from_jsonb(name: &str, idx: usize, ty: &str, optional: bool) -> String {
    // Ekspresi rekonstruksi `x: ty` dari teks JSON `s`.
    let decode = format!(
        "match ::arke::Value::from_json(s) {{ \
            ::core::option::Option::Some(val) => match <{ty} as ::arke::Serialize>::from_value(&val) {{ \
                ::core::option::Option::Some(x) => x, \
                ::core::option::Option::None => return ::core::option::Option::None, \
            }}, \
            ::core::option::Option::None => return ::core::option::Option::None, \
        }}"
    );
    if optional {
        format!(
            "{name}: match values.get({idx}) {{ \
                ::core::option::Option::Some(::arke_postgres::PgValue::Null) => ::core::option::Option::None, \
                ::core::option::Option::Some(::arke_postgres::PgValue::Json(s)) => ::core::option::Option::Some({decode}), \
                _ => return ::core::option::Option::None, \
             }}, "
        )
    } else {
        format!(
            "{name}: match values.get({idx}) {{ \
                ::core::option::Option::Some(::arke_postgres::PgValue::Json(s)) => {decode}, \
                _ => return ::core::option::Option::None, \
             }}, "
        )
    }
}

fn gen_impl(name: &str, fields: &[Field], checks: &[String]) -> Result<String, String> {
    let table = format!("cmp_{}", name.to_lowercase());

    let mut columns = String::new();
    let mut to_params = String::new();
    let mut from_fields = String::new();
    let mut indexes = String::new();

    for (idx, f) in fields.iter().enumerate() {
        let frag = field_sql(f, idx)?;
        columns.push_str(&frag.column);
        to_params.push_str(&frag.to_param);
        from_fields.push_str(&frag.from_field);
        if f.index || f.unique {
            indexes.push_str(&format!(
                "::arke_postgres::IndexDef {{ column: {:?}, unique: {} }}, ",
                f.name, f.unique
            ));
        }
    }

    let mut checks_str = String::new();
    for c in checks {
        checks_str.push_str(&format!("{c:?}, "));
    }

    Ok(format!(
        "impl ::arke_postgres::PgComponent for {name} {{\n\
            const TABLE: &'static str = {table:?};\n\
            const COLUMNS: &'static [::arke_postgres::ColumnDef] = &[{columns}];\n\
            const INDEXES: &'static [::arke_postgres::IndexDef] = &[{indexes}];\n\
            const CHECKS: &'static [&'static str] = &[{checks_str}];\n\
            fn to_params(&self) -> ::std::vec::Vec<::arke_postgres::PgValue> {{\n\
                ::std::vec![{to_params}]\n\
            }}\n\
            fn from_params(values: &[::arke_postgres::PgValue]) -> ::core::option::Option<Self> {{\n\
                ::core::option::Option::Some(Self {{ {from_fields} }})\n\
            }}\n\
        }}"
    ))
}
