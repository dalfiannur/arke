//! Representasi terbuka `Value` + trait `Serialize` + JSON tulis-tangan
//! (RFC-0007). Tanpa dependensi eksternal (STD-0003).

use crate::Component;

/// Nilai perantara terbuka antara komponen dan JSON.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Ketiadaan nilai.
    Null,
    /// Boolean.
    Bool(bool),
    /// Bilangan bulat 64-bit.
    Int(i64),
    /// Bilangan pecahan 64-bit.
    Float(f64),
    /// Teks UTF-8.
    Text(String),
    /// Deret nilai terurut.
    List(Vec<Value>),
    /// Pasangan kunci-nilai terurut (seperti objek JSON).
    Map(Vec<(String, Value)>),
}

/// Komponen yang dapat di-*snapshot* (opt-in, RFC-0007 §2).
pub trait Serialize: Component + Sized {
    /// Mengubah `self` menjadi [`Value`].
    fn to_value(&self) -> Value;
    /// Merekonstruksi dari [`Value`]; `None` bila bentuknya tak cocok.
    fn from_value(value: &Value) -> Option<Self>;
}

macro_rules! impl_serialize_int {
    ($($t:ty),*) => {$(
        impl Serialize for $t {
            fn to_value(&self) -> Value {
                Value::Int(*self as i64)
            }
            fn from_value(value: &Value) -> Option<Self> {
                match value {
                    Value::Int(i) => (*i).try_into().ok(),
                    _ => None,
                }
            }
        }
    )*};
}
impl_serialize_int!(i8, i16, i32, i64, u8, u16, u32, u64, usize, isize);

macro_rules! impl_serialize_float {
    ($($t:ty),*) => {$(
        impl Serialize for $t {
            fn to_value(&self) -> Value {
                Value::Float(f64::from(*self))
            }
            fn from_value(value: &Value) -> Option<Self> {
                match value {
                    Value::Float(f) => Some(*f as $t),
                    _ => None,
                }
            }
        }
    )*};
}
impl_serialize_float!(f32, f64);

impl Serialize for bool {
    fn to_value(&self) -> Value {
        Value::Bool(*self)
    }
    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

impl Serialize for String {
    fn to_value(&self) -> Value {
        Value::Text(self.clone())
    }
    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Text(s) => Some(s.clone()),
            _ => None,
        }
    }
}

impl Serialize for char {
    fn to_value(&self) -> Value {
        Value::Text(self.to_string())
    }
    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Text(s) if s.chars().count() == 1 => s.chars().next(),
            _ => None,
        }
    }
}

impl<T: Serialize> Serialize for Vec<T> {
    fn to_value(&self) -> Value {
        Value::List(self.iter().map(Serialize::to_value).collect())
    }
    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::List(items) => items.iter().map(T::from_value).collect(),
            _ => None,
        }
    }
}

impl<T: Serialize> Serialize for Option<T> {
    fn to_value(&self) -> Value {
        match self {
            Some(t) => t.to_value(),
            None => Value::Null,
        }
    }
    fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Null => Some(None),
            other => T::from_value(other).map(Some),
        }
    }
}

impl Value {
    /// Meng-*encode* nilai ini menjadi teks JSON.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        self.write_json(&mut out);
        out
    }

    fn write_json(&self, out: &mut String) {
        match self {
            Value::Null => out.push_str("null"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Int(i) => out.push_str(&i.to_string()),
            Value::Float(f) => {
                let s = f.to_string();
                out.push_str(&s);
                // Pastikan float selalu punya penanda pecahan agar round-trip
                // tak salah dibaca sebagai Int.
                if !s.contains(['.', 'e', 'E', 'n', 'i']) {
                    out.push_str(".0");
                }
            }
            Value::Text(t) => write_json_string(t, out),
            Value::List(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write_json(out);
                }
                out.push(']');
            }
            Value::Map(entries) => {
                out.push('{');
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_json_string(k, out);
                    out.push(':');
                    v.write_json(out);
                }
                out.push('}');
            }
        }
    }

    /// Isi `Map` bila nilai ini sebuah map (untuk impl `Serialize` manual/derive).
    pub fn as_map(&self) -> Option<&[(String, Value)]> {
        match self {
            Value::Map(m) => Some(m),
            _ => None,
        }
    }

    /// Isi `List` bila nilai ini sebuah list.
    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(l) => Some(l),
            _ => None,
        }
    }

    /// Isi `Int` bila nilai ini bilangan bulat.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Nilai untuk `key` bila ini sebuah map yang memuatnya.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_map()?
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// Mem-*parse* teks JSON menjadi [`Value`]; `None` bila bukan JSON valid
    /// (subset yang didukung).
    pub fn from_json(text: &str) -> Option<Value> {
        let mut parser = Parser {
            chars: text.chars().collect(),
            pos: 0,
        };
        let value = parser.parse_value()?;
        parser.skip_ws();
        if parser.pos == parser.chars.len() {
            Some(value)
        } else {
            None
        }
    }
}

fn write_json_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
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

    fn parse_value(&mut self) -> Option<Value> {
        self.skip_ws();
        match self.peek()? {
            'n' => self.parse_lit("null", Value::Null),
            't' => self.parse_lit("true", Value::Bool(true)),
            'f' => self.parse_lit("false", Value::Bool(false)),
            '"' => Some(Value::Text(self.parse_string()?)),
            '[' => self.parse_list(),
            '{' => self.parse_map(),
            c if c == '-' || c.is_ascii_digit() => self.parse_number(),
            _ => None,
        }
    }

    fn parse_lit(&mut self, lit: &str, value: Value) -> Option<Value> {
        for expected in lit.chars() {
            if self.bump()? != expected {
                return None;
            }
        }
        Some(value)
    }

    fn parse_string(&mut self) -> Option<String> {
        if self.bump()? != '"' {
            return None;
        }
        let mut s = String::new();
        loop {
            match self.bump()? {
                '"' => return Some(s),
                '\\' => match self.bump()? {
                    '"' => s.push('"'),
                    '\\' => s.push('\\'),
                    '/' => s.push('/'),
                    'n' => s.push('\n'),
                    't' => s.push('\t'),
                    'r' => s.push('\r'),
                    'b' => s.push('\u{8}'),
                    'f' => s.push('\u{c}'),
                    'u' => {
                        let mut code = 0u32;
                        for _ in 0..4 {
                            code = code * 16 + self.bump()?.to_digit(16)?;
                        }
                        s.push(char::from_u32(code)?);
                    }
                    _ => return None,
                },
                c => s.push(c),
            }
        }
    }

    fn parse_number(&mut self) -> Option<Value> {
        let start = self.pos;
        if self.peek() == Some('-') {
            self.pos += 1;
        }
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.pos += 1;
        }
        let mut is_float = false;
        if self.peek() == Some('.') {
            is_float = true;
            self.pos += 1;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.peek(), Some('+' | '-')) {
                self.pos += 1;
            }
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        if is_float {
            text.parse::<f64>().ok().map(Value::Float)
        } else {
            text.parse::<i64>().ok().map(Value::Int)
        }
    }

    fn parse_list(&mut self) -> Option<Value> {
        self.bump(); // '['
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(']') {
            self.bump();
            return Some(Value::List(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.skip_ws();
            match self.bump()? {
                ',' => {}
                ']' => return Some(Value::List(items)),
                _ => return None,
            }
        }
    }

    fn parse_map(&mut self) -> Option<Value> {
        self.bump(); // '{'
        let mut entries = Vec::new();
        self.skip_ws();
        if self.peek() == Some('}') {
            self.bump();
            return Some(Value::Map(entries));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            if self.bump()? != ':' {
                return None;
            }
            let value = self.parse_value()?;
            entries.push((key, value));
            self.skip_ws();
            match self.bump()? {
                ',' => {}
                '}' => return Some(Value::Map(entries)),
                _ => return None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitif_dan_kontainer_round_trip() {
        fn rt<T: Serialize + Clone + PartialEq + std::fmt::Debug>(v: T) {
            assert_eq!(T::from_value(&v.clone().to_value()), Some(v));
        }
        rt(42_i64);
        rt(-7_i32);
        rt(255_u8);
        rt(1_000_000_u32);
        rt(3.5_f64);
        rt(2.0_f32);
        rt(true);
        rt('x');
        rt(String::from("halo\n"));
        rt(vec![1_i64, 2, 3]);
        rt(Some(5_i64));
        rt(Option::<i64>::None);
        rt(vec![Some(1_i64), None, Some(3)]);
    }

    #[test]
    fn value_round_trip_lewat_json() {
        let v = Value::Map(vec![
            ("x".into(), Value::Int(3)),
            ("ratio".into(), Value::Float(2.5)),
            ("whole".into(), Value::Float(4.0)),
            ("name".into(), Value::Text("ha\"lo\n".into())),
            ("flag".into(), Value::Bool(true)),
            (
                "list".into(),
                Value::List(vec![Value::Int(1), Value::Null, Value::Bool(false)]),
            ),
        ]);
        let json = v.to_json();
        assert_eq!(Value::from_json(&json), Some(v));
    }

    #[test]
    fn json_tak_valid_mengembalikan_none() {
        assert_eq!(Value::from_json("{oops}"), None);
        assert_eq!(Value::from_json("[1,2"), None);
        assert_eq!(Value::from_json("tru"), None);
    }
}
