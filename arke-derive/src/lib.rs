//! Derive macro untuk trait `Serialize` milik [`arke`](https://docs.rs/arke),
//! ditulis tangan memakai **hanya `proc_macro` bawaan** — nol dependensi
//! crates.io (RFC-0009).
//!
//! Mendukung struct field-bernama (→ `Value::Map`), tuple struct
//! (→ `Value::List`), dan unit struct (→ `Value::Null`). Enum, generic, dan
//! union memancarkan `compile_error!` yang jelas.

use proc_macro::{Delimiter, TokenStream, TokenTree};

/// Turunkan implementasi `arke::Serialize` untuk sebuah struct atau enum.
///
/// Mendukung atribut field `#[serialize(skip)]` dan `#[serialize(rename = "...")]`.
#[proc_macro_derive(Serialize, attributes(serialize))]
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

    // Temukan kata kunci `struct`/`enum` (tolak union), lewati atribut & visibilitas.
    let mut kind = None;
    while i < tokens.len() {
        if let TokenTree::Ident(id) = &tokens[i] {
            match id.to_string().as_str() {
                "struct" => {
                    kind = Some(Kind::Struct);
                    i += 1;
                    break;
                }
                "enum" => {
                    kind = Some(Kind::Enum);
                    i += 1;
                    break;
                }
                "union" => {
                    return Err("derive(Serialize) belum mendukung `union`".to_string());
                }
                _ => {}
            }
        }
        i += 1;
    }
    let Some(kind) = kind else {
        return Err("derive(Serialize): definisi struct/enum tak ditemukan".to_string());
    };

    // Nama tipe.
    let name = match tokens.get(i) {
        Some(TokenTree::Ident(id)) => id.to_string(),
        _ => return Err("derive(Serialize): nama tipe tak ditemukan".to_string()),
    };
    i += 1;

    // Tipe generic belum didukung.
    if let Some(TokenTree::Punct(p)) = tokens.get(i)
        && p.as_char() == '<'
    {
        return Err("derive(Serialize): tipe generic belum didukung".to_string());
    }

    match kind {
        Kind::Struct => match tokens.get(i) {
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
                Ok(gen_named(&name, &parse_named_fields(g.stream())?))
            }
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis => {
                Ok(gen_tuple(&name, count_tuple_fields(g.stream())))
            }
            _ => Ok(gen_unit(&name)),
        },
        Kind::Enum => match tokens.get(i) {
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
                Ok(gen_enum(&name, &parse_enum_variants(g.stream())?))
            }
            _ => Err("derive(Serialize): badan enum tak ditemukan".to_string()),
        },
    }
}

enum Kind {
    Struct,
    Enum,
}

/// Satu field bernama: nama Rust + kunci serialisasi (rename) + apakah di-skip.
struct NamedField {
    name: String,
    key: String,
    skip: bool,
}

/// Varian enum.
struct Variant {
    name: String,
    kind: VariantKind,
}

enum VariantKind {
    Unit,
    Tuple(usize),
    Struct(Vec<NamedField>),
}

/// Ekstrak field bernama (nama + atribut `serialize`) dari badan struct/varian.
fn parse_named_fields(stream: TokenStream) -> Result<Vec<NamedField>, String> {
    let toks: Vec<TokenTree> = stream.into_iter().collect();
    let mut fields = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let mut rename = None;
        let mut skip = false;
        // Atribut field: `#` lalu grup bracket. Baca `serialize(...)`, lewati lainnya.
        while matches!(toks.get(i), Some(TokenTree::Punct(p)) if p.as_char() == '#') {
            i += 1;
            if let Some(TokenTree::Group(g)) = toks.get(i)
                && g.delimiter() == Delimiter::Bracket
            {
                parse_serialize_attr(g.stream(), &mut rename, &mut skip)?;
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
        let name = match toks.get(i) {
            Some(TokenTree::Ident(id)) => id.to_string(),
            None => break,
            _ => return Err("derive(Serialize): gagal mem-parse field bernama".to_string()),
        };
        i += 1;
        let key = rename.unwrap_or_else(|| name.clone());
        fields.push(NamedField { name, key, skip });
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

/// Parse isi atribut `#[...]`; hanya `serialize(skip | rename = "...")` yang
/// diproses (atribut lain diabaikan).
fn parse_serialize_attr(
    stream: TokenStream,
    rename: &mut Option<String>,
    skip: &mut bool,
) -> Result<(), String> {
    let toks: Vec<TokenTree> = stream.into_iter().collect();
    // Bentuk: `serialize ( ... )`.
    let Some(TokenTree::Ident(id)) = toks.first() else {
        return Ok(());
    };
    if id.to_string() != "serialize" {
        return Ok(());
    }
    let Some(TokenTree::Group(g)) = toks.get(1) else {
        return Ok(());
    };
    let inner: Vec<TokenTree> = g.stream().into_iter().collect();
    let mut j = 0;
    while j < inner.len() {
        if let TokenTree::Ident(key) = &inner[j] {
            match key.to_string().as_str() {
                "skip" => *skip = true,
                "rename" => {
                    // rename = "nama"
                    match (inner.get(j + 1), inner.get(j + 2)) {
                        (Some(TokenTree::Punct(p)), Some(TokenTree::Literal(lit)))
                            if p.as_char() == '=' =>
                        {
                            *rename = Some(unquote(&lit.to_string()));
                            j += 2;
                        }
                        _ => return Err("serialize(rename = \"...\") tak valid".to_string()),
                    }
                }
                other => {
                    return Err(format!("atribut serialize tak dikenal: `{other}`"));
                }
            }
        }
        j += 1;
    }
    Ok(())
}

/// Buang tanda kutip pembungkus literal string (mis. `"halo"` → `halo`).
fn unquote(lit: &str) -> String {
    lit.trim_matches('"').to_string()
}

/// Ekstrak varian dari badan enum.
fn parse_enum_variants(stream: TokenStream) -> Result<Vec<Variant>, String> {
    let toks: Vec<TokenTree> = stream.into_iter().collect();
    let mut variants = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        // Lewati atribut varian.
        while matches!(toks.get(i), Some(TokenTree::Punct(p)) if p.as_char() == '#') {
            i += 1;
            if matches!(toks.get(i), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Bracket)
            {
                i += 1;
            }
        }
        // Nama varian.
        let name = match toks.get(i) {
            Some(TokenTree::Ident(id)) => id.to_string(),
            None => break,
            _ => return Err("derive(Serialize): gagal mem-parse varian enum".to_string()),
        };
        i += 1;
        // Bentuk varian.
        let kind = match toks.get(i) {
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis => {
                let c = count_tuple_fields(g.stream());
                i += 1;
                VariantKind::Tuple(c)
            }
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
                let f = parse_named_fields(g.stream())?;
                i += 1;
                VariantKind::Struct(f)
            }
            _ => VariantKind::Unit,
        };
        variants.push(Variant { name, kind });
        // Lewati hingga koma tingkat-atas (termasuk `= discriminant` varian unit).
        while i < toks.len() {
            if matches!(&toks[i], TokenTree::Punct(p) if p.as_char() == ',') {
                i += 1;
                break;
            }
            i += 1;
        }
    }
    Ok(variants)
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

fn gen_named(name: &str, fields: &[NamedField]) -> String {
    let to_entries: String = fields
        .iter()
        .filter(|f| !f.skip)
        .map(|f| {
            format!(
                "(::std::string::String::from({:?}), ::arke::Serialize::to_value(&self.{})),",
                f.key, f.name
            )
        })
        .collect();
    let from_fields: String = fields
        .iter()
        .map(|f| {
            if f.skip {
                format!("{}: ::core::default::Default::default(),", f.name)
            } else {
                format!(
                    "{}: ::arke::Serialize::from_value(::arke::Value::get(value, {:?})?)?,",
                    f.name, f.key
                )
            }
        })
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

fn gen_enum(name: &str, variants: &[Variant]) -> String {
    let mut to_arms = String::new();
    let mut unit_from = String::new();
    let mut data_from = String::new();

    for v in variants {
        let vname = &v.name;
        match &v.kind {
            VariantKind::Unit => {
                to_arms.push_str(&format!(
                    "Self::{vname} => ::arke::Value::Text(::std::string::String::from({vname:?})),"
                ));
                unit_from.push_str(&format!(
                    "{vname:?} => ::core::option::Option::Some(Self::{vname}),"
                ));
            }
            VariantKind::Tuple(count) => {
                let binds: Vec<String> = (0..*count).map(|k| format!("__{k}")).collect();
                let pat = binds.join(", ");
                let to_items: String = binds
                    .iter()
                    .map(|b| format!("::arke::Serialize::to_value({b}),"))
                    .collect();
                to_arms.push_str(&format!(
                    "Self::{vname}({pat}) => ::arke::Value::Map(::std::vec![(::std::string::String::from({vname:?}), ::arke::Value::List(::std::vec![{to_items}]))]),"
                ));
                let from_items: String = (0..*count)
                    .map(|k| format!("::arke::Serialize::from_value(__list.get({k})?)?,"))
                    .collect();
                data_from.push_str(&format!(
                    "{vname:?} => {{ let __list = ::arke::Value::as_list(__payload)?; ::core::option::Option::Some(Self::{vname}({from_items})) }},"
                ));
            }
            VariantKind::Struct(fields) => {
                let pat: String = fields
                    .iter()
                    .map(|f| format!("{}, ", f.name))
                    .collect::<String>();
                let to_items: String = fields
                    .iter()
                    .filter(|f| !f.skip)
                    .map(|f| {
                        format!(
                            "(::std::string::String::from({:?}), ::arke::Serialize::to_value({})),",
                            f.key, f.name
                        )
                    })
                    .collect();
                to_arms.push_str(&format!(
                    "Self::{vname} {{ {pat} }} => ::arke::Value::Map(::std::vec![(::std::string::String::from({vname:?}), ::arke::Value::Map(::std::vec![{to_items}]))]),"
                ));
                let from_fields: String = fields
                    .iter()
                    .map(|f| {
                        if f.skip {
                            format!("{}: ::core::default::Default::default(),", f.name)
                        } else {
                            format!(
                                "{}: ::arke::Serialize::from_value(::arke::Value::get(__payload, {:?})?)?,",
                                f.name, f.key
                            )
                        }
                    })
                    .collect();
                data_from.push_str(&format!(
                    "{vname:?} => ::core::option::Option::Some(Self::{vname} {{ {from_fields} }}),"
                ));
            }
        }
    }

    format!(
        "impl ::arke::Serialize for {name} {{
    fn to_value(&self) -> ::arke::Value {{
        match self {{ {to_arms} }}
    }}
    fn from_value(value: &::arke::Value) -> ::core::option::Option<Self> {{
        match value {{
            ::arke::Value::Text(__name) => match __name.as_str() {{
                {unit_from}
                _ => ::core::option::Option::None,
            }},
            ::arke::Value::Map(__m) if __m.len() == 1 => {{
                let (__name, __payload) = &__m[0];
                match __name.as_str() {{
                    {data_from}
                    _ => ::core::option::Option::None,
                }}
            }},
            _ => ::core::option::Option::None,
        }}
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
