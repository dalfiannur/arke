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

/// Info pemetaan satu tipe skalar Rust → SQL.
struct Scalar {
    /// Varian `PgType` (untuk `COLUMNS`).
    pg_type: &'static str,
    /// Varian `PgValue` (untuk `to_params`/`from_params`).
    value: &'static str,
    /// Ekspresi bind (dari `self.<field>`) di dalam `PgValue::<value>(…)`.
    to_expr: String,
    /// Ekspresi rekonstruksi (dari `v` yang di-bind pola) untuk field.
    from_expr: String,
}

/// Petakan tipe skalar (string tanpa spasi) → [`Scalar`]. `None` = tak didukung.
fn scalar(ty: &str, field: &str) -> Option<Scalar> {
    let acc = format!("self.{field}");
    let int = |pg: &'static str, rust: &str| Scalar {
        pg_type: pg,
        value: "Int",
        to_expr: format!("{acc} as i64"),
        from_expr: format!("*v as {rust}"),
    };
    Some(match ty {
        "i8" => int("Integer", "i8"),
        "i16" => int("Integer", "i16"),
        "i32" => int("Integer", "i32"),
        "u8" => int("Integer", "u8"),
        "u16" => int("Integer", "u16"),
        "i64" => int("BigInt", "i64"),
        "isize" => int("BigInt", "isize"),
        "u32" => int("BigInt", "u32"),
        "u64" | "usize" => Scalar {
            pg_type: "Numeric",
            value: "Numeric",
            to_expr: format!("{acc}.to_string()"),
            from_expr: "v.parse().ok()?".to_string(),
        },
        "f32" => Scalar {
            pg_type: "Real",
            value: "Float",
            to_expr: format!("{acc} as f64"),
            from_expr: "*v as f32".to_string(),
        },
        "f64" => Scalar {
            pg_type: "DoublePrecision",
            value: "Float",
            to_expr: format!("{acc} as f64"),
            from_expr: "*v".to_string(),
        },
        "bool" => Scalar {
            pg_type: "Boolean",
            value: "Bool",
            to_expr: acc.clone(),
            from_expr: "*v".to_string(),
        },
        "String" => Scalar {
            pg_type: "Text",
            value: "Text",
            to_expr: format!("{acc}.clone()"),
            from_expr: "v.clone()".to_string(),
        },
        _ => return None,
    })
}

fn gen_impl(name: &str, fields: &[Field]) -> Result<String, String> {
    let table = format!("cmp_{}", name.to_lowercase());

    let mut columns = String::new();
    let mut to_params = String::new();
    let mut from_fields = String::new();

    for (idx, f) in fields.iter().enumerate() {
        let Some(s) = scalar(&f.ty, &f.name) else {
            return Err(format!(
                "derive(PgComponent): tipe field `{}: {}` belum didukung (skalar saja untuk saat ini)",
                f.name, f.ty
            ));
        };
        columns.push_str(&format!(
            "::arke_postgres::ColumnDef {{ name: {:?}, ty: ::arke_postgres::PgType::{}, nullable: false }}, ",
            f.name, s.pg_type
        ));
        to_params.push_str(&format!(
            "::arke_postgres::PgValue::{}({}), ",
            s.value, s.to_expr
        ));
        from_fields.push_str(&format!(
            "{name}: match values.get({idx}) {{ \
                ::core::option::Option::Some(::arke_postgres::PgValue::{val}(v)) => {from}, \
                _ => return ::core::option::Option::None, \
             }}, ",
            name = f.name,
            idx = idx,
            val = s.value,
            from = s.from_expr,
        ));
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
