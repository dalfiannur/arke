//! Derive macro untuk trait `Serialize` milik [`arke`](https://docs.rs/arke),
//! ditulis tangan memakai **hanya `proc_macro` bawaan** — nol dependensi
//! crates.io (RFC-0009).
//!
//! Mendukung struct field-bernama (→ `Value::Map`), tuple struct
//! (→ `Value::List`), unit struct (→ `Value::Null`), dan enum *externally-tagged*.
//! Atribut: `#[serialize(rename_all = "...")]` (level-tipe), `#[serialize(skip)]`,
//! `#[serialize(rename = "...")]`. Generic & union memancarkan `compile_error!`.

use proc_macro::{Delimiter, TokenStream, TokenTree};

/// Turunkan implementasi `arke::Serialize` untuk sebuah struct atau enum.
///
/// Atribut: `#[serialize(rename_all = "...")]` (level-tipe),
/// `#[serialize(skip)]`, dan `#[serialize(rename = "...")]` (per-field/varian).
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

    // Temukan kata kunci `struct`/`enum` (tolak union). Parse atribut level-tipe
    // (`rename_all`), lewati visibilitas.
    let mut kind = None;
    let mut type_attrs = Attrs::default();
    while i < tokens.len() {
        match &tokens[i] {
            TokenTree::Punct(p) if p.as_char() == '#' => {
                i += 1; // lewati '#'
                if let Some(TokenTree::Group(g)) = tokens.get(i)
                    && g.delimiter() == Delimiter::Bracket
                {
                    parse_serialize_attr(g.stream(), &mut type_attrs)?;
                }
                i += 1; // lewati grup bracket
                continue;
            }
            TokenTree::Ident(id) => match id.to_string().as_str() {
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
            },
            _ => {}
        }
        i += 1;
    }
    let Some(kind) = kind else {
        return Err("derive(Serialize): definisi struct/enum tak ditemukan".to_string());
    };
    let rename_all = type_attrs.rename_all;

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
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => Ok(gen_named(
                &name,
                &parse_named_fields(g.stream())?,
                rename_all,
            )),
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis => {
                Ok(gen_tuple(&name, count_tuple_fields(g.stream())))
            }
            _ => Ok(gen_unit(&name)),
        },
        Kind::Enum => match tokens.get(i) {
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => Ok(gen_enum(
                &name,
                &parse_enum_variants(g.stream())?,
                rename_all,
            )),
            _ => Err("derive(Serialize): badan enum tak ditemukan".to_string()),
        },
    }
}

enum Kind {
    Struct,
    Enum,
}

/// Satu field bernama: nama Rust + rename eksplisit (bila ada) + apakah di-skip.
struct NamedField {
    name: String,
    rename: Option<String>,
    skip: bool,
}

impl NamedField {
    /// Kunci serialisasi akhir: `rename` menang, lalu `rename_all`, lalu nama.
    fn key(&self, rename_all: Option<Case>) -> String {
        self.rename
            .clone()
            .unwrap_or_else(|| apply_case(&self.name, rename_all))
    }
}

/// Varian enum: nama + rename eksplisit (bila ada) + bentuk.
struct Variant {
    name: String,
    rename: Option<String>,
    kind: VariantKind,
}

impl Variant {
    fn key(&self, rename_all: Option<Case>) -> String {
        self.rename
            .clone()
            .unwrap_or_else(|| apply_case(&self.name, rename_all))
    }
}

/// Konvensi penamaan untuk `rename_all`.
#[derive(Clone, Copy)]
enum Case {
    Lower,
    Upper,
    Snake,
    ScreamingSnake,
    Kebab,
    ScreamingKebab,
    Camel,
    Pascal,
}

fn parse_case(s: &str) -> Result<Case, String> {
    Ok(match s {
        "lowercase" => Case::Lower,
        "UPPERCASE" => Case::Upper,
        "snake_case" => Case::Snake,
        "SCREAMING_SNAKE_CASE" => Case::ScreamingSnake,
        "kebab-case" => Case::Kebab,
        "SCREAMING-KEBAB-CASE" => Case::ScreamingKebab,
        "camelCase" => Case::Camel,
        "PascalCase" => Case::Pascal,
        other => return Err(format!("rename_all tak dikenal: `{other}`")),
    })
}

/// Pecah identifier menjadi kata (huruf-kecil) pada batas `_`, `-`, dan
/// transisi huruf-kecil/digit → huruf-besar.
fn split_words(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut prev_lower_or_digit = false;
    for c in s.chars() {
        if c == '_' || c == '-' {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            prev_lower_or_digit = false;
        } else if c.is_ascii_uppercase() && prev_lower_or_digit {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            cur.push(c.to_ascii_lowercase());
            prev_lower_or_digit = false;
        } else {
            cur.push(c.to_ascii_lowercase());
            prev_lower_or_digit = c.is_ascii_lowercase() || c.is_ascii_digit();
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

fn capitalize(w: &str) -> String {
    let mut chars = w.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// Terapkan konvensi `rename_all` pada `name`. `None` → tak berubah.
fn apply_case(name: &str, case: Option<Case>) -> String {
    let Some(case) = case else {
        return name.to_string();
    };
    let words = split_words(name);
    match case {
        Case::Lower => words.concat(),
        Case::Upper => words.concat().to_uppercase(),
        Case::Snake => words.join("_"),
        Case::ScreamingSnake => words.join("_").to_uppercase(),
        Case::Kebab => words.join("-"),
        Case::ScreamingKebab => words.join("-").to_uppercase(),
        Case::Camel => words
            .iter()
            .enumerate()
            .map(|(i, w)| if i == 0 { w.clone() } else { capitalize(w) })
            .collect(),
        Case::Pascal => words.iter().map(|w| capitalize(w)).collect(),
    }
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
        let mut attrs = Attrs::default();
        // Atribut field: `#` lalu grup bracket. Baca `serialize(...)`, lewati lainnya.
        while matches!(toks.get(i), Some(TokenTree::Punct(p)) if p.as_char() == '#') {
            i += 1;
            if let Some(TokenTree::Group(g)) = toks.get(i)
                && g.delimiter() == Delimiter::Bracket
            {
                parse_serialize_attr(g.stream(), &mut attrs)?;
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
        fields.push(NamedField {
            name,
            rename: attrs.rename,
            skip: attrs.skip,
        });
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

/// Atribut `#[serialize(...)]` yang terkumpul.
#[derive(Default)]
struct Attrs {
    rename: Option<String>,
    skip: bool,
    rename_all: Option<Case>,
}

/// Parse isi atribut `#[...]` ke dalam `attrs`; hanya
/// `serialize(skip | rename = "..." | rename_all = "...")` yang diproses.
fn parse_serialize_attr(stream: TokenStream, attrs: &mut Attrs) -> Result<(), String> {
    let toks: Vec<TokenTree> = stream.into_iter().collect();
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
                "skip" => attrs.skip = true,
                "rename" => {
                    attrs.rename = Some(expect_str_value(&inner, j, "rename")?);
                    j += 2;
                }
                "rename_all" => {
                    attrs.rename_all =
                        Some(parse_case(&expect_str_value(&inner, j, "rename_all")?)?);
                    j += 2;
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

/// Membaca `= "nilai"` setelah posisi `j` (kunci), mengembalikan nilai tanpa kutip.
fn expect_str_value(inner: &[TokenTree], j: usize, name: &str) -> Result<String, String> {
    match (inner.get(j + 1), inner.get(j + 2)) {
        (Some(TokenTree::Punct(p)), Some(TokenTree::Literal(lit))) if p.as_char() == '=' => {
            Ok(unquote(&lit.to_string()))
        }
        _ => Err(format!("serialize({name} = \"...\") tak valid")),
    }
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
        let mut attrs = Attrs::default();
        // Atribut varian: baca `serialize(rename = ...)`, lewati lainnya.
        while matches!(toks.get(i), Some(TokenTree::Punct(p)) if p.as_char() == '#') {
            i += 1;
            if let Some(TokenTree::Group(g)) = toks.get(i)
                && g.delimiter() == Delimiter::Bracket
            {
                parse_serialize_attr(g.stream(), &mut attrs)?;
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
        variants.push(Variant {
            name,
            rename: attrs.rename,
            kind,
        });
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

fn gen_named(name: &str, fields: &[NamedField], rename_all: Option<Case>) -> String {
    let to_entries: String = fields
        .iter()
        .filter(|f| !f.skip)
        .map(|f| {
            format!(
                "(::std::string::String::from({:?}), ::arke::Serialize::to_value(&self.{})),",
                f.key(rename_all),
                f.name
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
                    f.name,
                    f.key(rename_all)
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

fn gen_enum(name: &str, variants: &[Variant], rename_all: Option<Case>) -> String {
    let mut to_arms = String::new();
    let mut unit_from = String::new();
    let mut data_from = String::new();

    for v in variants {
        let vname = &v.name; // identifier Rust (untuk pola & konstruksi)
        let vkey = v.key(rename_all); // string terserialisasi
        match &v.kind {
            VariantKind::Unit => {
                to_arms.push_str(&format!(
                    "Self::{vname} => ::arke::Value::Text(::std::string::String::from({vkey:?})),"
                ));
                unit_from.push_str(&format!(
                    "{vkey:?} => ::core::option::Option::Some(Self::{vname}),"
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
                    "Self::{vname}({pat}) => ::arke::Value::Map(::std::vec![(::std::string::String::from({vkey:?}), ::arke::Value::List(::std::vec![{to_items}]))]),"
                ));
                let from_items: String = (0..*count)
                    .map(|k| format!("::arke::Serialize::from_value(__list.get({k})?)?,"))
                    .collect();
                data_from.push_str(&format!(
                    "{vkey:?} => {{ let __list = ::arke::Value::as_list(__payload)?; ::core::option::Option::Some(Self::{vname}({from_items})) }},"
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
                            f.key(rename_all),
                            f.name
                        )
                    })
                    .collect();
                to_arms.push_str(&format!(
                    "Self::{vname} {{ {pat} }} => ::arke::Value::Map(::std::vec![(::std::string::String::from({vkey:?}), ::arke::Value::Map(::std::vec![{to_items}]))]),"
                ));
                let from_fields: String = fields
                    .iter()
                    .map(|f| {
                        if f.skip {
                            format!("{}: ::core::default::Default::default(),", f.name)
                        } else {
                            format!(
                                "{}: ::arke::Serialize::from_value(::arke::Value::get(__payload, {:?})?)?,",
                                f.name,
                                f.key(rename_all)
                            )
                        }
                    })
                    .collect();
                data_from.push_str(&format!(
                    "{vkey:?} => ::core::option::Option::Some(Self::{vname} {{ {from_fields} }}),"
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
