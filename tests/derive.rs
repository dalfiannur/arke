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
