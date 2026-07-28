//! Derive `#[derive(PgComponent)]` untuk `arke-postgres` (RFC-0021).
//!
//! Ditulis tangan memakai **hanya `proc_macro` bawaan** — nol dependensi
//! crates.io (pola `arke-derive`). Menurunkan skema kolom-tipe dari sebuah
//! struct field-bernama skalar: `TABLE`, `COLUMNS`, `to_params`, `from_params`.

use proc_macro::{Delimiter, TokenStream, TokenTree};

/// Turunkan `arke_postgres::PgComponent` untuk struct field-bernama skalar.
///
/// Tipe field skalar dipetakan ke kolom SQL ber-tipe (RFC-0021 §3). Tipe yang
/// belum didukung, generic, atau non-struct → `compile_error!`.
#[proc_macro_derive(PgComponent)]
pub fn derive_pg_component(input: TokenStream) -> TokenStream {
    let code = match expand(input) {
        Ok(code) => code,
        Err(message) => format!("::core::compile_error!({message:?});"),
    };
    code.parse().expect("kode hasil-derive tidak valid")
}

/// Satu field bernama + tipe (dinormalisasi ke string tanpa spasi).
struct Field {
    name: String,
    ty: String,
}

fn expand(input: TokenStream) -> Result<String, String> {
    let tokens: Vec<TokenTree> = input.into_iter().collect();
    let mut i = 0;

    // Lewati atribut & visibilitas; temukan `struct` (tolak enum/union).
    while i < tokens.len() {
        match &tokens[i] {
            TokenTree::Punct(p) if p.as_char() == '#' => {
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

    gen_impl(&name, &fields)
}

fn parse_named_fields(stream: TokenStream) -> Result<Vec<Field>, String> {
    let toks: Vec<TokenTree> = stream.into_iter().collect();
    let mut fields = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        // Lewati atribut field.
        while matches!(toks.get(i), Some(TokenTree::Punct(p)) if p.as_char() == '#') {
            i += 1;
            if matches!(toks.get(i), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Bracket)
            {
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
        fields.push(Field { name, ty });
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

/// Hasilkan fragmen untuk satu field (skalar atau `Option<skalar>`).
fn field_sql(f: &Field, idx: usize) -> Result<FieldSql, String> {
    let name = &f.name;
    // `Option<INNER>` → kolom nullable; None ↔ NULL.
    if let Some(inner_ty) =
        f.ty.strip_prefix("Option<")
            .and_then(|r| r.strip_suffix('>'))
    {
        let Some(s) = inner_scalar(inner_ty) else {
            return Err(format!(
                "derive(PgComponent): `Option<{inner_ty}>` (field `{name}`) belum didukung"
            ));
        };
        return Ok(FieldSql {
            column: format!(
                "::arke_postgres::ColumnDef {{ name: {name:?}, ty: ::arke_postgres::PgType::{}, nullable: true }}, ",
                s.pg_type
            ),
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
        });
    }

    // Skalar biasa (non-null).
    let Some(s) = inner_scalar(&f.ty) else {
        return Err(format!(
            "derive(PgComponent): tipe field `{name}: {}` belum didukung (skalar/`Option<skalar>`)",
            f.ty
        ));
    };
    Ok(FieldSql {
        column: format!(
            "::arke_postgres::ColumnDef {{ name: {name:?}, ty: ::arke_postgres::PgType::{}, nullable: false }}, ",
            s.pg_type
        ),
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
    })
}

fn gen_impl(name: &str, fields: &[Field]) -> Result<String, String> {
    let table = format!("cmp_{}", name.to_lowercase());

    let mut columns = String::new();
    let mut to_params = String::new();
    let mut from_fields = String::new();

    for (idx, f) in fields.iter().enumerate() {
        let frag = field_sql(f, idx)?;
        columns.push_str(&frag.column);
        to_params.push_str(&frag.to_param);
        from_fields.push_str(&frag.from_field);
    }

    Ok(format!(
        "impl ::arke_postgres::PgComponent for {name} {{\n\
            const TABLE: &'static str = {table:?};\n\
            const COLUMNS: &'static [::arke_postgres::ColumnDef] = &[{columns}];\n\
            fn to_params(&self) -> ::std::vec::Vec<::arke_postgres::PgValue> {{\n\
                ::std::vec![{to_params}]\n\
            }}\n\
            fn from_params(values: &[::arke_postgres::PgValue]) -> ::core::option::Option<Self> {{\n\
                ::core::option::Option::Some(Self {{ {from_fields} }})\n\
            }}\n\
        }}"
    ))
}
