//! Derive macro untuk trait `Serialize` milik [`arke`](https://docs.rs/arke),
//! ditulis tangan memakai **hanya `proc_macro` bawaan** — nol dependensi
//! crates.io (RFC-0009).
//!
//! Mendukung struct field-bernama (→ `Value::Map`), tuple struct
//! (→ `Value::List`), dan unit struct (→ `Value::Null`). Enum, generic, dan
//! union memancarkan `compile_error!` yang jelas.

use proc_macro::{Delimiter, TokenStream, TokenTree};

/// Turunkan implementasi `arke::Serialize` untuk sebuah struct.
#[proc_macro_derive(Serialize)]
pub fn derive_serialize(input: TokenStream) -> TokenStream {
    let code = match expand(input) {
        Ok(code) => code,
        Err(message) => format!("::core::compile_error!({message:?});"),
    };
    code.parse().expect("kode hasil-derive tidak valid")
}

fn expand(input: TokenStream) -> Result<String, String> {
    let tokens: Vec<TokenTree> = input.into_iter().collect();
    let mut i = 0;

    // Temukan kata kunci `struct` (tolak enum/union), lewati atribut & visibilitas.
    let mut found_struct = false;
    while i < tokens.len() {
        if let TokenTree::Ident(id) = &tokens[i] {
            match id.to_string().as_str() {
                "struct" => {
                    found_struct = true;
                    i += 1;
                    break;
                }
                "enum" | "union" => {
                    return Err(format!(
                        "derive(Serialize) belum mendukung `{id}` — hanya struct"
                    ));
                }
                _ => {}
            }
        }
        i += 1;
    }
    if !found_struct {
        return Err("derive(Serialize): definisi struct tak ditemukan".to_string());
    }

    // Nama struct.
    let name = match tokens.get(i) {
        Some(TokenTree::Ident(id)) => id.to_string(),
        _ => return Err("derive(Serialize): nama struct tak ditemukan".to_string()),
    };
    i += 1;

    // Tipe generic belum didukung.
    if let Some(TokenTree::Punct(p)) = tokens.get(i)
        && p.as_char() == '<'
    {
        return Err("derive(Serialize): tipe generic belum didukung".to_string());
    }

    // Bentuk badan struct.
    match tokens.get(i) {
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
            let fields = parse_named_fields(g.stream())?;
            Ok(gen_named(&name, &fields))
        }
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis => {
            Ok(gen_tuple(&name, count_tuple_fields(g.stream())))
        }
        _ => Ok(gen_unit(&name)),
    }
}

/// Ekstrak nama field dari badan struct field-bernama.
fn parse_named_fields(stream: TokenStream) -> Result<Vec<String>, String> {
    let toks: Vec<TokenTree> = stream.into_iter().collect();
    let mut fields = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        // Lewati atribut field: `#` lalu grup bracket.
        while matches!(toks.get(i), Some(TokenTree::Punct(p)) if p.as_char() == '#') {
            i += 1;
            if matches!(toks.get(i), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Bracket)
            {
                i += 1;
            }
        }
        // Lewati visibilitas: `pub` [`(..)`].
        if matches!(toks.get(i), Some(TokenTree::Ident(id)) if id.to_string() == "pub") {
            i += 1;
            if matches!(toks.get(i), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis)
            {
                i += 1;
            }
        }
        // Nama field.
        match toks.get(i) {
            Some(TokenTree::Ident(id)) => fields.push(id.to_string()),
            None => break,
            _ => return Err("derive(Serialize): gagal mem-parse field bernama".to_string()),
        }
        i += 1;
        // Lewati `:` dan tipe hingga koma tingkat-atas (lacak kedalaman `<>`).
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
            i += 1;
        }
    }
    Ok(fields)
}

/// Hitung jumlah field pada tuple struct.
fn count_tuple_fields(stream: TokenStream) -> usize {
    let toks: Vec<TokenTree> = stream.into_iter().collect();
    if toks.is_empty() {
        return 0;
    }
    let mut count = 1;
    let mut depth = 0i32;
    for t in &toks {
        if let TokenTree::Punct(p) = t {
            match p.as_char() {
                '<' => depth += 1,
                '>' => depth -= 1,
                ',' if depth <= 0 => count += 1,
                _ => {}
            }
        }
    }
    // Koreksi koma trailing.
    if matches!(toks.last(), Some(TokenTree::Punct(p)) if p.as_char() == ',') {
        count -= 1;
    }
    count
}

fn gen_named(name: &str, fields: &[String]) -> String {
    let to_entries: String = fields
        .iter()
        .map(|f| {
            format!("(::std::string::String::from({f:?}), ::arke::Serialize::to_value(&self.{f})),")
        })
        .collect();
    let from_fields: String = fields
        .iter()
        .map(|f| format!("{f}: ::arke::Serialize::from_value(::arke::Value::get(value, {f:?})?)?,"))
        .collect();
    format!(
        "impl ::arke::Serialize for {name} {{
    fn to_value(&self) -> ::arke::Value {{
        ::arke::Value::Map(::std::vec![{to_entries}])
    }}
    fn from_value(value: &::arke::Value) -> ::core::option::Option<Self> {{
        ::core::option::Option::Some(Self {{ {from_fields} }})
    }}
}}"
    )
}

fn gen_tuple(name: &str, count: usize) -> String {
    let to_items: String = (0..count)
        .map(|i| format!("::arke::Serialize::to_value(&self.{i}),"))
        .collect();
    let from_items: String = (0..count)
        .map(|i| format!("::arke::Serialize::from_value(__list.get({i})?)?,"))
        .collect();
    format!(
        "impl ::arke::Serialize for {name} {{
    fn to_value(&self) -> ::arke::Value {{
        ::arke::Value::List(::std::vec![{to_items}])
    }}
    fn from_value(value: &::arke::Value) -> ::core::option::Option<Self> {{
        let __list = ::arke::Value::as_list(value)?;
        ::core::option::Option::Some(Self({from_items}))
    }}
}}"
    )
}

fn gen_unit(name: &str) -> String {
    format!(
        "impl ::arke::Serialize for {name} {{
    fn to_value(&self) -> ::arke::Value {{ ::arke::Value::Null }}
    fn from_value(value: &::arke::Value) -> ::core::option::Option<Self> {{
        match value {{
            ::arke::Value::Null => ::core::option::Option::Some(Self),
            _ => ::core::option::Option::None,
        }}
    }}
}}"
    )
}
