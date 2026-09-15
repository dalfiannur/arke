//! `#[derive(Serialize)]` end-to-end (RFC-0009), API publik, `forbid(unsafe_code)`.

#![forbid(unsafe_code)]

use arke::{Serialize, Snapshot, Value, World};

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

#[derive(Serialize, PartialEq, Debug)]
#[serialize(rename_all = "camelCase")]
struct User {
    user_name: String,
    is_active: bool,
    #[serialize(rename = "id")]
    user_id: u32,
}

#[test]
fn rename_all_camelcase_dengan_override_per_field() {
    let u = User {
        user_name: "x".to_string(),
        is_active: true,
        user_id: 7,
    };
    let v = u.to_value();
    assert!(v.get("userName").is_some());
    assert!(v.get("isActive").is_some());
    assert!(v.get("id").is_some()); // rename per-field menang atas rename_all
    assert!(v.get("userId").is_none());
    assert!(v.get("user_name").is_none());
    assert_eq!(User::from_value(&v), Some(u));
}

#[derive(Serialize, PartialEq, Debug)]
#[serialize(rename_all = "snake_case")]
enum Event {
    PlayerJoined,
    ScoreChanged(u32),
}

#[test]
fn rename_all_enum_snake_case() {
    assert_eq!(
        Event::PlayerJoined.to_value(),
        arke::Value::Text("player_joined".to_string())
    );
    for e in [Event::PlayerJoined, Event::ScoreChanged(9)] {
        assert_eq!(Event::from_value(&e.to_value()), Some(e));
    }
}

#[derive(Serialize, PartialEq, Debug)]
#[serialize(rename_all = "SCREAMING_SNAKE_CASE")]
struct Cfg {
    max_size: u32,
}

#[derive(Serialize, PartialEq, Debug)]
#[serialize(rename_all = "kebab-case")]
struct Kb {
    first_name: String,
}

#[test]
fn rename_all_variasi_konvensi() {
    let c = Cfg { max_size: 5 };
    assert!(c.to_value().get("MAX_SIZE").is_some());
    assert_eq!(Cfg::from_value(&c.to_value()), Some(c));

    let k = Kb {
        first_name: "a".to_string(),
    };
    assert!(k.to_value().get("first-name").is_some());
    assert_eq!(Kb::from_value(&k.to_value()), Some(k));
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

// ── Evolusi format (audit 0.7.0): nama stabil & field default ──────────────

/// Kunci snapshot eksplisit — tak bergantung `std::any::type_name` (yang Rust
/// nyatakan tak stabil antar versi/rename modul).
#[derive(Serialize, PartialEq, Debug)]
#[serialize(name = "game.hp")]
struct Hp(i64);

/// Field baru `#[serialize(default)]`: snapshot lama tanpa kunci itu tetap
/// termuat (nilai `Default`), bukan gagal seluruh komponen.
#[derive(Serialize, PartialEq, Debug)]
struct Stats {
    level: i64,
    #[serialize(default)]
    xp: i64,
    #[serialize(default)]
    title: Option<String>,
}

#[test]
fn serialize_name_eksplisit_dipakai_sebagai_kunci_snapshot() {
    assert_eq!(<Hp as Serialize>::name(), "game.hp");
    assert!(<Position as Serialize>::name().ends_with("Position")); // default: type_name

    let mut w = World::new();
    w.register_serializable::<Hp>();
    let e = w.spawn();
    w.insert(e, Hp(9));
    let json = w.snapshot().to_json();
    assert!(json.contains("\"game.hp\""), "{json}");
    assert!(!json.contains("derive::Hp"), "{json}");

    let mut w2 = World::new();
    w2.register_serializable::<Hp>();
    w2.try_load_snapshot(&Snapshot::from_json(&json).unwrap())
        .unwrap();
    assert_eq!(w2.get::<Hp>(e), Some(&Hp(9)));
}

#[test]
fn alias_nama_lama_memuat_snapshot_lama() {
    // Snapshot ditulis di bawah nama lama (mis. `type_name` versi sebelumnya).
    let json = r#"{"schema_version":1,"entities":[
        {"index":0,"generation":0,"components":{"legacy::Hp":[7]}}
    ]}"#;
    let snap = Snapshot::from_json(json).unwrap();

    let mut w = World::new();
    w.register_serializable::<Hp>();
    assert!(matches!(
        w.try_load_snapshot(&snap),
        Err(arke::EcsError::UnknownComponent { .. })
    ));

    let mut w = World::new();
    w.register_serializable::<Hp>();
    w.register_serializable_alias::<Hp>("legacy::Hp");
    w.try_load_snapshot(&snap).unwrap();
    assert_eq!(w.get::<Hp>(arke::Entity::from_raw(0, 0)), Some(&Hp(7)));
}

#[test]
fn field_default_mengisi_kunci_yang_hilang() {
    let v = Value::Map(vec![("level".to_string(), Value::Int(3))]);
    assert_eq!(
        Stats::from_value(&v),
        Some(Stats {
            level: 3,
            xp: 0,
            title: None
        })
    );
    // Kunci wajib yang hilang tetap gagal.
    assert_eq!(Stats::from_value(&Value::Map(vec![])), None);
    // Round-trip penuh tetap setia.
    let s = Stats {
        level: 1,
        xp: 50,
        title: Some("x".into()),
    };
    assert_eq!(Stats::from_value(&s.to_value()), Some(s));
}
