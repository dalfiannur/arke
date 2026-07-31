//! Lapis 1 (RFC-0035 §7): tes pemetaan tanpa database.

use arke::Value;
use arke_mongo::bson::{Bson, Document};
use arke_mongo::{bson_to_value, value_to_bson};

#[test]
fn skalar_dipetakan_ke_bson_yang_setara() {
    assert_eq!(value_to_bson(&Value::Null), Bson::Null);
    assert_eq!(value_to_bson(&Value::Bool(true)), Bson::Boolean(true));
    assert_eq!(value_to_bson(&Value::Int(42)), Bson::Int64(42));
    assert_eq!(value_to_bson(&Value::Float(1.5)), Bson::Double(1.5));
    assert_eq!(
        value_to_bson(&Value::Text("halo".into())),
        Bson::String("halo".into())
    );
}

#[test]
fn map_menjadi_document_dengan_urutan_field_terjaga() {
    let v = Value::Map(vec![
        ("z".into(), Value::Int(1)),
        ("a".into(), Value::Int(2)),
    ]);
    let Bson::Document(d) = value_to_bson(&v) else {
        panic!("Map harus jadi Document");
    };
    let keys: Vec<&str> = d.keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["z", "a"], "urutan sisip harus terjaga");
}

#[test]
fn list_bersarang_dipetakan_rekursif() {
    let v = Value::List(vec![
        Value::Int(1),
        Value::Map(vec![("x".into(), Value::Text("y".into()))]),
    ]);
    let mut inner = Document::new();
    inner.insert("x", "y");
    assert_eq!(
        value_to_bson(&v),
        Bson::Array(vec![Bson::Int64(1), Bson::Document(inner)])
    );
}

#[test]
fn round_trip_value_bson_value_setia() {
    let cases = vec![
        Value::Null,
        Value::Bool(false),
        Value::Int(-7),
        Value::Float(0.25),
        Value::Text("teks".into()),
        Value::List(vec![Value::Int(1), Value::Null]),
        Value::Map(vec![
            ("a".into(), Value::Int(1)),
            (
                "b".into(),
                Value::Map(vec![("c".into(), Value::Bool(true))]),
            ),
        ]),
    ];
    for v in cases {
        let back = bson_to_value(&value_to_bson(&v));
        assert_eq!(back, Some(v.clone()), "round-trip gagal untuk {v:?}");
    }
}

#[test]
fn int32_dari_penulis_lain_diterima_sebagai_int() {
    assert_eq!(bson_to_value(&Bson::Int32(5)), Some(Value::Int(5)));
}

#[test]
fn tipe_bson_tak_dikenal_ditolak() {
    assert_eq!(bson_to_value(&Bson::Undefined), None);
}
