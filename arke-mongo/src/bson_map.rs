//! Pemetaan murni [`arke::Value`] ↔ BSON (RFC-0035 §4). Tanpa I/O, tanpa World —
//! seluruh modul ini dapat diuji tanpa database.

use arke::Value;
use mongodb::bson::{Bson, Document};

/// Memetakan [`Value`] ke BSON, rekursif. Total: tiap varian punya padanan.
///
/// `Int` selalu menjadi `Int64` (bukan `Int32`) supaya round-trip tak
/// bergantung pada besar nilai.
pub fn value_to_bson(value: &Value) -> Bson {
    match value {
        Value::Null => Bson::Null,
        Value::Bool(b) => Bson::Boolean(*b),
        Value::Int(i) => Bson::Int64(*i),
        Value::Float(f) => Bson::Double(*f),
        Value::Text(s) => Bson::String(s.clone()),
        Value::List(items) => Bson::Array(items.iter().map(value_to_bson).collect()),
        Value::Map(entries) => {
            let mut doc = Document::new();
            for (key, val) in entries {
                doc.insert(key.clone(), value_to_bson(val));
            }
            Bson::Document(doc)
        }
    }
}
