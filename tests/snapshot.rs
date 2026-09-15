//! Round-trip snapshot World lewat JSON (RFC-0007), API publik, di bawah
//! `forbid(unsafe_code)`. Bukti STD-0001 (versi) & STD-0002 (round-trip setia).

#![forbid(unsafe_code)]

use arke::{Serialize, Snapshot, Value, World};

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
                if k == key
                    && let Value::Int(i) = v
                {
                    return Some(*i);
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

/// Parser JSON menolak sarang terlalu dalam alih-alih meledakkan stack
/// (input tak tepercaya → DoS).
#[test]
fn json_bersarang_terlalu_dalam_ditolak() {
    let deep = "[".repeat(100_000) + &"]".repeat(100_000);
    assert_eq!(Value::from_json(&deep), None);
    // Kedalaman wajar tetap diterima.
    let ok = "[".repeat(32) + &"]".repeat(32);
    assert!(Value::from_json(&ok).is_some());
}

/// Snapshot dengan `index` entity duplikat ditolak — dua entity di satu slot
/// tak mungkin direkonstruksi tanpa korupsi.
#[test]
fn snapshot_index_duplikat_ditolak() {
    let json = r#"{"schema_version":1,"entities":[
        {"index":0,"generation":0,"components":{}},
        {"index":0,"generation":1,"components":{}}
    ]}"#;
    assert!(Snapshot::from_json(json).is_none());
}

/// `spawn_at` ke slot yang sedang ada di free-list mengeluarkannya dari
/// free-list, sehingga `spawn` berikutnya tak menerbitkan handle duplikat;
/// bila slot masih hidup, komponen lamanya dibersihkan (tanpa baris yatim).
#[test]
fn spawn_at_tak_menerbitkan_handle_duplikat_dan_membersihkan_slot() {
    let mut world = World::new();
    let a = world.spawn();
    world.despawn(a); // index 0 → free-list, generation 1

    let restored = world.spawn_at(0, 1);
    world.insert(restored, Position { x: 7, y: 7 });
    let b = world.spawn();
    assert_ne!(
        b, restored,
        "spawn tak boleh mendaur-ulang slot yang baru direstorasi"
    );
    assert_eq!(
        world.get::<Position>(restored),
        Some(&Position { x: 7, y: 7 })
    );

    // Timpa slot hidup: baris lama harus hilang, bukan jadi yatim.
    let again = world.spawn_at(0, 5);
    assert_eq!(world.query::<Position>().count(), 0);
    assert_eq!(world.get::<Position>(restored), None);
    assert!(world.contains(again));
}
