//! Pemetaan murni [`arke::Value`] ↔ BSON (RFC-0035 §4). Tanpa I/O, tanpa World —
//! seluruh modul ini dapat diuji tanpa database.

use crate::MongoError;
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

/// Memetakan BSON kembali ke [`Value`]; `None` bila ada tipe BSON yang tak
/// punya padanan (mis. `ObjectId` di dalam badan komponen, `Undefined`).
///
/// `Int32` diterima meski [`value_to_bson`] tak pernah menghasilkannya —
/// dokumen bisa ditulis service lain atau `mongosh`, yang mengirim bilangan
/// bulat kecil sebagai `Int32`.
pub fn bson_to_value(bson: &Bson) -> Option<Value> {
    Some(match bson {
        Bson::Null => Value::Null,
        Bson::Boolean(b) => Value::Bool(*b),
        Bson::Int64(i) => Value::Int(*i),
        Bson::Int32(i) => Value::Int(i64::from(*i)),
        Bson::Double(f) => Value::Float(*f),
        Bson::String(s) => Value::Text(s.clone()),
        Bson::Array(items) => Value::List(
            items
                .iter()
                .map(bson_to_value)
                .collect::<Option<Vec<_>>>()?,
        ),
        Bson::Document(doc) => Value::Map(
            doc.iter()
                .map(|(k, v)| bson_to_value(v).map(|v| (k.clone(), v)))
                .collect::<Option<Vec<_>>>()?,
        ),
        _ => return None,
    })
}

/// Memastikan seluruh nama field di dalam `value` sah sebagai nama field BSON:
/// tak mengandung `.` dan tak berawalan `$` (RFC-0035 §4). Rekursif menembus
/// `Map` dan `List`.
pub fn validate_names(component: &'static str, value: &Value) -> Result<(), MongoError> {
    match value {
        Value::Map(entries) => {
            for (key, val) in entries {
                if key.contains('.') || key.starts_with('$') {
                    return Err(MongoError::InvalidName {
                        component,
                        field: key.clone(),
                    });
                }
                validate_names(component, val)?;
            }
            Ok(())
        }
        Value::List(items) => items.iter().try_for_each(|v| validate_names(component, v)),
        _ => Ok(()),
    }
}
