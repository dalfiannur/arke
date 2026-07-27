//! Round-trip snapshot World lewat JSON (RFC-0007), API publik, di bawah
//! `forbid(unsafe_code)`. Bukti STD-0001 (versi) & STD-0002 (round-trip setia).

#![forbid(unsafe_code)]

use rust_ecs::{Serialize, Snapshot, Value, World};

#[derive(PartialEq, Debug, Clone)]
struct Position {
    x: i64,
    y: i64,
}

impl Serialize for Position {
    fn to_value(&self) -> Value {
        Value::Map(vec![
            ("x".to_string(), Value::Int(self.x)),
            ("y".to_string(), Value::Int(self.y)),
        ])
    }

    fn from_value(value: &Value) -> Option<Self> {
        Some(Position {
            x: value.get_field("x")?,
            y: value.get_field("y")?,
        })
    }
}

// Helper kecil khusus tes untuk membaca field Int dari sebuah Value::Map.
trait GetField {
    fn get_field(&self, key: &str) -> Option<i64>;
}
impl GetField for Value {
    fn get_field(&self, key: &str) -> Option<i64> {
        if let Value::Map(entries) = self {
            for (k, v) in entries {
                if k == key {
                    if let Value::Int(i) = v {
                        return Some(*i);
                    }
                }
            }
        }
        None
    }
}

#[test]
fn round_trip_world_lewat_json_setia() {
    let mut world = World::new();
    world.register_serializable::<Position>();
    let a = world.spawn();
    world.insert(a, Position { x: 3, y: 5 });
    let b = world.spawn();
    world.insert(b, Position { x: -1, y: 42 });

    // Snapshot → JSON (memuat schema_version, STD-0001).
    let json = world.snapshot().to_json();
    assert!(json.contains("\"schema_version\""));

    // JSON → Snapshot → World baru.
    let snap = Snapshot::from_json(&json).expect("JSON snapshot valid");
    assert_eq!(snap.schema_version(), 1);

    let mut restored = World::new();
    restored.register_serializable::<Position>();
    restored.load_snapshot(&snap);

    // Handle yang sama tetap valid dan nilainya identik (STD-0002).
    assert_eq!(restored.get::<Position>(a), Some(&Position { x: 3, y: 5 }));
    assert_eq!(
        restored.get::<Position>(b),
        Some(&Position { x: -1, y: 42 })
    );

    // Himpunan hasil query identik.
    let mut got: Vec<(i64, i64)> = restored.query::<Position>().map(|p| (p.x, p.y)).collect();
    got.sort();
    assert_eq!(got, vec![(-1, 42), (3, 5)]);
}

#[test]
fn snapshot_tanpa_schema_version_ditolak() {
    // STD-0001: format tanpa versi harus ditolak.
    assert!(Snapshot::from_json(r#"{"entities":[]}"#).is_none());
}

#[derive(PartialEq, Debug)]
struct Untracked(i32); // tidak impl Serialize, tidak diregistrasi

#[test]
fn try_snapshot_menolak_komponen_tak_terdaftar_menyebut_namanya() {
    // STD-0008: error menyebut komponen yang terlibat.
    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Untracked(7));

    let err = world.try_snapshot().unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("Untracked"),
        "pesan tak menyebut komponen: {msg}"
    );
}

#[test]
fn try_snapshot_ok_bila_semua_terdaftar() {
    let mut world = World::new();
    world.register_serializable::<Position>();
    let e = world.spawn();
    world.insert(e, Position { x: 1, y: 2 });

    assert!(world.try_snapshot().is_ok());
}
