//! Implementasi Rust dari Validator Contract v1.
//!
//! Kesetaraan dengan implementasi Python/Node dibuktikan oleh conformance suite
//! yang sama (../conformance/), bukan oleh kemiripan kode. Lihat
//! ../../spec/VALIDATOR_CONTRACT.md untuk perilaku yang wajib dipenuhi.
//!
//! TANPA dependensi (tanpa serde/regex): cukup `rustc validate.rs -o validate`.
//! Berisi parser JSON minimal dan pencocok regex subset untuk keyword `pattern`.
//!
//! Keyword: type, required, properties, additionalProperties (bool), enum,
//! const, pattern, minLength, maxLength, minItems, maxItems, uniqueItems,
//! items, minimum, maximum, $ref lokal ("#/...").
//!
//! Output --json = array kanonik per instance (kontrak §3).
//! Exit code: 0 semua valid, 1 ada yang tidak, 2 argumen/berkas bermasalah.

use std::env;
use std::fs;
use std::process;

// ---------------------------------------------------------------------------
// Model nilai JSON
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq)]
enum Value {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
}

impl Value {
    fn get<'a>(&'a self, key: &str) -> Option<&'a Value> {
        if let Value::Obj(pairs) = self {
            pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v)
        } else {
            None
        }
    }
    fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
    fn as_str(&self) -> Option<&str> {
        if let Value::Str(s) = self { Some(s) } else { None }
    }
    fn as_f64(&self) -> Option<f64> {
        if let Value::Num(n) = self { Some(*n) } else { None }
    }
}

// ---------------------------------------------------------------------------
// Parser JSON minimal
// ---------------------------------------------------------------------------

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn new(input: &str) -> Self {
        Parser { chars: input.chars().collect(), pos: 0 }
    }
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }
    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\n' | '\r')) {
            self.pos += 1;
        }
    }
    fn parse(&mut self) -> Result<Value, String> {
        self.skip_ws();
        let v = self.parse_value()?;
        self.skip_ws();
        if self.pos != self.chars.len() {
            return Err(format!("karakter berlebih di posisi {}", self.pos));
        }
        Ok(v)
    }
    fn parse_value(&mut self) -> Result<Value, String> {
        self.skip_ws();
        match self.peek() {
            Some('{') => self.parse_object(),
            Some('[') => self.parse_array(),
            Some('"') => Ok(Value::Str(self.parse_string()?)),
            Some('t') | Some('f') => self.parse_bool(),
            Some('n') => self.parse_null(),
            Some(c) if c == '-' || c.is_ascii_digit() => self.parse_number(),
            other => Err(format!("token tak terduga: {:?}", other)),
        }
    }
    fn expect(&mut self, c: char) -> Result<(), String> {
        if self.bump() == Some(c) {
            Ok(())
        } else {
            Err(format!("diharapkan '{}'", c))
        }
    }
    fn parse_object(&mut self) -> Result<Value, String> {
        self.expect('{')?;
        let mut pairs = Vec::new();
        self.skip_ws();
        if self.peek() == Some('}') {
            self.bump();
            return Ok(Value::Obj(pairs));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(':')?;
            let val = self.parse_value()?;
            pairs.push((key, val));
            self.skip_ws();
            match self.bump() {
                Some(',') => continue,
                Some('}') => break,
                other => return Err(format!("diharapkan ',' atau '}}', dapat {:?}", other)),
            }
        }
        Ok(Value::Obj(pairs))
    }
    fn parse_array(&mut self) -> Result<Value, String> {
        self.expect('[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(']') {
            self.bump();
            return Ok(Value::Arr(items));
        }
        loop {
            let val = self.parse_value()?;
            items.push(val);
            self.skip_ws();
            match self.bump() {
                Some(',') => continue,
                Some(']') => break,
                other => return Err(format!("diharapkan ',' atau ']', dapat {:?}", other)),
            }
        }
        Ok(Value::Arr(items))
    }
    fn parse_string(&mut self) -> Result<String, String> {
        self.expect('"')?;
        let mut s = String::new();
        loop {
            match self.bump() {
                Some('"') => break,
                Some('\\') => match self.bump() {
                    Some('"') => s.push('"'),
                    Some('\\') => s.push('\\'),
                    Some('/') => s.push('/'),
                    Some('b') => s.push('\u{0008}'),
                    Some('f') => s.push('\u{000C}'),
                    Some('n') => s.push('\n'),
                    Some('r') => s.push('\r'),
                    Some('t') => s.push('\t'),
                    Some('u') => {
                        let mut code = 0u32;
                        for _ in 0..4 {
                            let c = self.bump().ok_or("escape \\u terpotong")?;
                            code = code * 16 + c.to_digit(16).ok_or("digit hex tak sah")?;
                        }
                        s.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                    }
                    other => return Err(format!("escape tak sah: {:?}", other)),
                },
                Some(c) => s.push(c),
                None => return Err("string tak tertutup".into()),
            }
        }
        Ok(s)
    }
    fn parse_bool(&mut self) -> Result<Value, String> {
        if self.chars[self.pos..].starts_with(&['t', 'r', 'u', 'e']) {
            self.pos += 4;
            Ok(Value::Bool(true))
        } else if self.chars[self.pos..].starts_with(&['f', 'a', 'l', 's', 'e']) {
            self.pos += 5;
            Ok(Value::Bool(false))
        } else {
            Err("literal boolean tak sah".into())
        }
    }
    fn parse_null(&mut self) -> Result<Value, String> {
        if self.chars[self.pos..].starts_with(&['n', 'u', 'l', 'l']) {
            self.pos += 4;
            Ok(Value::Null)
        } else {
            Err("literal null tak sah".into())
        }
    }
    fn parse_number(&mut self) -> Result<Value, String> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E') {
                self.pos += 1;
            } else {
                break;
            }
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        text.parse::<f64>()
            .map(Value::Num)
            .map_err(|_| format!("angka tak sah: {}", text))
    }
}

// ---------------------------------------------------------------------------
// Serialisasi kanonik (untuk const / enum / uniqueItems)
// ---------------------------------------------------------------------------

fn canon(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Num(n) => {
            if n.fract() == 0.0 && n.is_finite() {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        Value::Str(s) => json_string(s),
        Value::Arr(items) => {
            let inner: Vec<String> = items.iter().map(canon).collect();
            format!("[{}]", inner.join(","))
        }
        Value::Obj(pairs) => {
            let mut sorted: Vec<&(String, Value)> = pairs.iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            let inner: Vec<String> = sorted
                .iter()
                .map(|(k, val)| format!("{}:{}", json_string(k), canon(val)))
                .collect();
            format!("{{{}}}", inner.join(","))
        }
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// Pencocok regex subset (untuk keyword `pattern`)
//   Didukung: ^ $ . literal, escape \x, kelas [..] dgn rentang & negasi,
//   kuantifier * + ? {n} {n,} {n,m}. Cukup untuk pola schema umum.
// ---------------------------------------------------------------------------

enum Class {
    Any,
    Literal(char),
    Set { negated: bool, ranges: Vec<(char, char)>, singles: Vec<char> },
}

impl Class {
    fn matches(&self, c: char) -> bool {
        match self {
            Class::Any => true,
            Class::Literal(l) => *l == c,
            Class::Set { negated, ranges, singles } => {
                let hit = singles.contains(&c) || ranges.iter().any(|(lo, hi)| *lo <= c && c <= *hi);
                hit != *negated
            }
        }
    }
}

enum Tok {
    Start,
    End,
    Elem { class: Class, min: usize, max: Option<usize> },
}

fn compile(pattern: &str) -> Vec<Tok> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let class = match c {
            '^' => {
                toks.push(Tok::Start);
                i += 1;
                continue;
            }
            '$' => {
                toks.push(Tok::End);
                i += 1;
                continue;
            }
            '.' => {
                i += 1;
                Class::Any
            }
            '\\' => {
                i += 1;
                let e = chars.get(i).copied().unwrap_or('\\');
                i += 1;
                match e {
                    'd' => Class::Set { negated: false, ranges: vec![('0', '9')], singles: vec![] },
                    'w' => Class::Set {
                        negated: false,
                        ranges: vec![('a', 'z'), ('A', 'Z'), ('0', '9')],
                        singles: vec!['_'],
                    },
                    's' => Class::Set { negated: false, ranges: vec![], singles: vec![' ', '\t', '\n', '\r'] },
                    other => Class::Literal(other),
                }
            }
            '[' => {
                i += 1;
                let mut negated = false;
                if chars.get(i) == Some(&'^') {
                    negated = true;
                    i += 1;
                }
                let mut ranges = Vec::new();
                let mut singles = Vec::new();
                while i < chars.len() && chars[i] != ']' {
                    let lo = chars[i];
                    if chars.get(i + 1) == Some(&'-') && chars.get(i + 2).is_some_and(|&x| x != ']') {
                        ranges.push((lo, chars[i + 2]));
                        i += 3;
                    } else {
                        singles.push(lo);
                        i += 1;
                    }
                }
                i += 1; // lewati ']'
                Class::Set { negated, ranges, singles }
            }
            other => {
                i += 1;
                Class::Literal(other)
            }
        };
        // kuantifier
        let (min, max) = match chars.get(i) {
            Some('*') => {
                i += 1;
                (0, None)
            }
            Some('+') => {
                i += 1;
                (1, None)
            }
            Some('?') => {
                i += 1;
                (0, Some(1))
            }
            Some('{') => {
                i += 1;
                let mut lo = String::new();
                while chars.get(i).is_some_and(|c| c.is_ascii_digit()) {
                    lo.push(chars[i]);
                    i += 1;
                }
                let mut hi = String::new();
                let mut has_comma = false;
                if chars.get(i) == Some(&',') {
                    has_comma = true;
                    i += 1;
                    while chars.get(i).is_some_and(|c| c.is_ascii_digit()) {
                        hi.push(chars[i]);
                        i += 1;
                    }
                }
                if chars.get(i) == Some(&'}') {
                    i += 1;
                }
                let lo_n = lo.parse::<usize>().unwrap_or(0);
                let hi_n = if !has_comma {
                    Some(lo_n)
                } else if hi.is_empty() {
                    None
                } else {
                    Some(hi.parse::<usize>().unwrap_or(lo_n))
                };
                (lo_n, hi_n)
            }
            _ => (1, Some(1)),
        };
        toks.push(Tok::Elem { class, min, max });
    }
    toks
}

fn match_seq(toks: &[Tok], ti: usize, text: &[char], pos: usize) -> bool {
    if ti == toks.len() {
        return true;
    }
    match &toks[ti] {
        Tok::Start => pos == 0 && match_seq(toks, ti + 1, text, pos),
        Tok::End => pos == text.len() && match_seq(toks, ti + 1, text, pos),
        Tok::Elem { class, min, max } => {
            let limit = max.unwrap_or(usize::MAX);
            let mut end = pos;
            while (end - pos) < limit && end < text.len() && class.matches(text[end]) {
                end += 1;
            }
            let maxcount = end - pos;
            if maxcount < *min {
                return false;
            }
            let mut count = maxcount;
            loop {
                if match_seq(toks, ti + 1, text, pos + count) {
                    return true;
                }
                if count == *min {
                    return false;
                }
                count -= 1;
            }
        }
    }
}

fn regex_search(pattern: &str, text: &str) -> bool {
    let toks = compile(pattern);
    let chars: Vec<char> = text.chars().collect();
    for start in 0..=chars.len() {
        if match_seq(&toks, 0, &chars, start) {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Validator
// ---------------------------------------------------------------------------

fn esc(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

fn type_matches(value: &Value, expected: &str) -> bool {
    match expected {
        "integer" => matches!(value, Value::Num(n) if n.fract() == 0.0),
        "number" => matches!(value, Value::Num(_)),
        "boolean" => matches!(value, Value::Bool(_)),
        "string" => matches!(value, Value::Str(_)),
        "object" => matches!(value, Value::Obj(_)),
        "array" => matches!(value, Value::Arr(_)),
        "null" => matches!(value, Value::Null),
        _ => true,
    }
}

fn resolve_ref<'a>(reference: &str, root: &'a Value) -> Option<&'a Value> {
    let rest = reference.strip_prefix("#/")?;
    let mut node = root;
    for raw in rest.split('/') {
        let token = raw.replace("~1", "/").replace("~0", "~");
        node = node.get(&token)?;
    }
    Some(node)
}

fn has_duplicates(items: &[Value]) -> bool {
    let mut seen = std::collections::HashSet::new();
    for item in items {
        if !seen.insert(canon(item)) {
            return true;
        }
    }
    false
}

fn validate(instance: &Value, schema: &Value, root: &Value, path: &str) -> Vec<(String, String)> {
    let mut errors = Vec::new();

    if let Some(Value::Str(reference)) = schema.get("$ref") {
        return match resolve_ref(reference, root) {
            Some(target) => validate(instance, target, root, path),
            None => errors,
        };
    }

    if let Some(type_val) = schema.get("type") {
        let expected: Vec<&str> = match type_val {
            Value::Str(s) => vec![s.as_str()],
            Value::Arr(items) => items.iter().filter_map(|v| v.as_str()).collect(),
            _ => vec![],
        };
        if !expected.iter().any(|t| type_matches(instance, t)) {
            return vec![(path.to_string(), "type".to_string())];
        }
    }

    if let Some(c) = schema.get("const") {
        if canon(instance) != canon(c) {
            errors.push((path.to_string(), "const".to_string()));
        }
    }

    if let Some(Value::Arr(options)) = schema.get("enum") {
        let ic = canon(instance);
        if !options.iter().any(|o| canon(o) == ic) {
            errors.push((path.to_string(), "enum".to_string()));
        }
    }

    if let Value::Str(s) = instance {
        let len = s.chars().count() as f64;
        if let Some(m) = schema.get("minLength").and_then(Value::as_f64) {
            if len < m {
                errors.push((path.to_string(), "minLength".to_string()));
            }
        }
        if let Some(m) = schema.get("maxLength").and_then(Value::as_f64) {
            if len > m {
                errors.push((path.to_string(), "maxLength".to_string()));
            }
        }
        if let Some(Value::Str(p)) = schema.get("pattern") {
            if !regex_search(p, s) {
                errors.push((path.to_string(), "pattern".to_string()));
            }
        }
    }

    if let Value::Num(n) = instance {
        if let Some(m) = schema.get("minimum").and_then(Value::as_f64) {
            if *n < m {
                errors.push((path.to_string(), "minimum".to_string()));
            }
        }
        if let Some(m) = schema.get("maximum").and_then(Value::as_f64) {
            if *n > m {
                errors.push((path.to_string(), "maximum".to_string()));
            }
        }
    }

    if let Value::Arr(items) = instance {
        if let Some(m) = schema.get("minItems").and_then(Value::as_f64) {
            if (items.len() as f64) < m {
                errors.push((path.to_string(), "minItems".to_string()));
            }
        }
        if let Some(m) = schema.get("maxItems").and_then(Value::as_f64) {
            if (items.len() as f64) > m {
                errors.push((path.to_string(), "maxItems".to_string()));
            }
        }
        if matches!(schema.get("uniqueItems"), Some(Value::Bool(true))) && has_duplicates(items) {
            errors.push((path.to_string(), "uniqueItems".to_string()));
        }
        if let Some(item_schema) = schema.get("items") {
            for (i, item) in items.iter().enumerate() {
                errors.extend(validate(item, item_schema, root, &format!("{}/{}", path, i)));
            }
        }
    }

    if let Value::Obj(pairs) = instance {
        if let Some(Value::Arr(required)) = schema.get("required") {
            for req in required {
                if let Value::Str(key) = req {
                    if !instance.has(key) {
                        errors.push((format!("{}/{}", path, esc(key)), "required".to_string()));
                    }
                }
            }
        }
        let props = schema.get("properties");
        let additional_false = matches!(schema.get("additionalProperties"), Some(Value::Bool(false)));
        for (key, value) in pairs {
            let child = format!("{}/{}", path, esc(key));
            match props.and_then(|p| p.get(key)) {
                Some(sub) => errors.extend(validate(value, sub, root, &child)),
                None => {
                    if additional_false {
                        errors.push((child, "additionalProperties".to_string()));
                    }
                }
            }
        }
    }

    errors
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

fn load(path: &str) -> Result<Value, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    Parser::new(&text).parse()
}

fn main() {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let as_json = args.first().map(|s| s == "--json").unwrap_or(false);
    if as_json {
        args.remove(0);
    }
    if args.len() < 2 {
        println!("Pemakaian: validate [--json] <schema.json> <instance.json> ...");
        process::exit(2);
    }

    let schema = match load(&args[0]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("gagal memuat schema {}: {}", args[0], e);
            process::exit(2);
        }
    };

    let mut all_ok = true;
    let mut json_results: Vec<String> = Vec::new();
    for instance_path in &args[1..] {
        let instance = match load(instance_path) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("gagal memuat instance {}: {}", instance_path, e);
                process::exit(2);
            }
        };
        let errors = validate(&instance, &schema, &schema, "");
        let ok = errors.is_empty();
        all_ok = all_ok && ok;

        if as_json {
            let errs: Vec<String> = errors
                .iter()
                .map(|(p, k)| {
                    format!(
                        "      {{\n        \"path\": {},\n        \"keyword\": {}\n      }}",
                        json_string(p),
                        json_string(k)
                    )
                })
                .collect();
            let errs_block = if errs.is_empty() {
                "[]".to_string()
            } else {
                format!("[\n{}\n    ]", errs.join(",\n"))
            };
            json_results.push(format!(
                "  {{\n    \"instance\": {},\n    \"valid\": {},\n    \"errors\": {}\n  }}",
                json_string(instance_path),
                ok,
                errs_block
            ));
        } else if ok {
            println!("VALID    {}", instance_path);
        } else {
            println!("INVALID  {}", instance_path);
            for (p, k) in &errors {
                let where_ = if p.is_empty() { "(root)" } else { p };
                println!("    - {}: gagal '{}'", where_, k);
            }
        }
    }

    if as_json {
        println!("[\n{}\n]", json_results.join(",\n"));
    }
    process::exit(if all_ok { 0 } else { 1 });
}
