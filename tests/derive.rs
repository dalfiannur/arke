//! `#[derive(Serialize)]` end-to-end (RFC-0009), API publik, `forbid(unsafe_code)`.

#![forbid(unsafe_code)]

use arke::{Serialize, Snapshot, World};

#[derive(Serialize, PartialEq, Debug)]
struct Position {
    x: i64,
    y: i64,
}

#[derive(Serialize, PartialEq, Debug)]
struct Rgb(u8, u8, u8);

#[derive(Serialize, PartialEq, Debug)]
struct Marker;

#[derive(Serialize, PartialEq, Debug)]
enum Shape {
    Point,
    Circle(f64),
    Rect { w: f64, h: f64 },
}

// Field dengan tipe generik ber-koma menguji pelacakan kedalaman `<>` parser.
#[derive(Serialize, PartialEq, Debug)]
struct Bag {
    items: Vec<i64>,
    tag: Option<String>,
}

#[test]
fn derive_named_struct_round_trip() {
    let p = Position { x: 3, y: -5 };
    assert_eq!(Position::from_value(&p.to_value()), Some(p));
}

#[test]
fn derive_tuple_struct_round_trip() {
    let c = Rgb(10, 20, 30);
    assert_eq!(Rgb::from_value(&c.to_value()), Some(c));
}

#[test]
fn derive_unit_struct_round_trip() {
    assert_eq!(Marker::from_value(&Marker.to_value()), Some(Marker));
}

#[test]
fn derive_nested_generics_round_trip() {
    let b = Bag {
        items: vec![1, 2, 3],
        tag: Some("halo".to_string()),
    };
    let back = Bag::from_value(&b.to_value()).unwrap();
    assert_eq!(back.items, vec![1, 2, 3]);
    assert_eq!(back.tag.as_deref(), Some("halo"));
}

#[derive(Serialize, PartialEq, Debug, Default)]
struct Config {
    #[serialize(rename = "n")]
    name: String,
    #[serialize(skip)]
    cache: i64,
    active: bool,
}

#[test]
fn derive_field_attributes_rename_dan_skip() {
    let c = Config {
        name: "arke".to_string(),
        cache: 999,
        active: true,
    };
    let v = c.to_value();

    // `name` di-rename jadi kunci "n"; `cache` di-skip (tak muncul).
    assert!(matches!(v.get("n"), Some(arke::Value::Text(_))));
    assert!(v.get("name").is_none());
    assert!(v.get("cache").is_none());

    // Round-trip: field skip kembali ke Default; sisanya utuh.
    let back = Config::from_value(&v).unwrap();
    assert_eq!(back.name, "arke");
    assert!(back.active);
    assert_eq!(back.cache, 0);
}

#[test]
fn derive_enum_round_trip() {
    for v in [
        Shape::Point,
        Shape::Circle(2.5),
        Shape::Rect { w: 3.0, h: 4.0 },
    ] {
        assert_eq!(Shape::from_value(&v.to_value()), Some(v));
    }
}

#[test]
fn derive_enum_varian_tak_dikenal_none() {
    assert_eq!(
        Shape::from_value(&arke::Value::Text("Nope".to_string())),
        None
    );
    assert_eq!(Shape::from_value(&arke::Value::Int(1)), None);
}

#[test]
fn derive_bekerja_dengan_snapshot_world() {
    let mut world = World::new();
    world.register_serializable::<Position>();
    let e = world.spawn();
    world.insert(e, Position { x: 1, y: 2 });

    let json = world.snapshot().to_json();
    let snap = Snapshot::from_json(&json).unwrap();

    let mut restored = World::new();
    restored.register_serializable::<Position>();
    restored.load_snapshot(&snap);

    assert_eq!(restored.get::<Position>(e), Some(&Position { x: 1, y: 2 }));
}
