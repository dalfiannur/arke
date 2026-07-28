//! Uji `#[derive(PgComponent)]` — **tanpa database** (skema + round-trip).

use arke_postgres::{ColumnDef, PgComponent, PgType, PgValue, create_table_sql};

#[derive(PgComponent)]
struct Position {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(PgComponent)]
struct Stats {
    level: i32,
    hp: i64,
    xp: u64,
    speed: u32,
    alive: bool,
    name: String,
}

#[test]
fn table_dan_kolom_untuk_struct_skalar() {
    assert_eq!(Position::TABLE, "cmp_position");
    assert_eq!(
        Position::COLUMNS.to_vec(),
        vec![
            ColumnDef {
                name: "x",
                ty: PgType::Real,
                nullable: false
            },
            ColumnDef {
                name: "y",
                ty: PgType::Real,
                nullable: false
            },
            ColumnDef {
                name: "z",
                ty: PgType::Real,
                nullable: false
            },
        ]
    );

    assert_eq!(Stats::TABLE, "cmp_stats");
    assert_eq!(
        Stats::COLUMNS.to_vec(),
        vec![
            ColumnDef {
                name: "level",
                ty: PgType::Integer,
                nullable: false
            },
            ColumnDef {
                name: "hp",
                ty: PgType::BigInt,
                nullable: false
            },
            ColumnDef {
                name: "xp",
                ty: PgType::Numeric,
                nullable: false
            },
            ColumnDef {
                name: "speed",
                ty: PgType::BigInt,
                nullable: false
            },
            ColumnDef {
                name: "alive",
                ty: PgType::Boolean,
                nullable: false
            },
            ColumnDef {
                name: "name",
                ty: PgType::Text,
                nullable: false
            },
        ]
    );
}

#[test]
fn to_params_from_params_round_trip() {
    let p = Position {
        x: 1.5,
        y: -2.0,
        z: 3.25,
    };
    assert_eq!(
        p.to_params(),
        vec![
            PgValue::Float(1.5),
            PgValue::Float(-2.0),
            PgValue::Float(3.25)
        ]
    );
    let back = Position::from_params(&p.to_params()).unwrap();
    assert_eq!((back.x, back.y, back.z), (1.5, -2.0, 3.25));

    let s = Stats {
        level: 7,
        hp: -100,
        xp: 999_999_999_999,
        speed: 4_000_000_000,
        alive: true,
        name: "hero".to_string(),
    };
    let sb = Stats::from_params(&s.to_params()).unwrap();
    assert_eq!(sb.level, 7);
    assert_eq!(sb.hp, -100);
    assert_eq!(sb.xp, 999_999_999_999);
    assert_eq!(sb.speed, 4_000_000_000);
    assert!(sb.alive);
    assert_eq!(sb.name, "hero");
}

#[test]
fn from_params_menolak_bentuk_salah() {
    // Terlalu sedikit nilai / tipe salah → None.
    assert!(Position::from_params(&[PgValue::Float(1.0)]).is_none());
    assert!(Position::from_params(&[PgValue::Int(1), PgValue::Int(2), PgValue::Int(3)]).is_none());
}

#[test]
fn create_table_sql_benar() {
    assert_eq!(
        create_table_sql::<Position>(),
        "CREATE TABLE IF NOT EXISTS cmp_position \
         (entity_id BIGINT PRIMARY KEY REFERENCES arke_entities(entity_id) ON DELETE CASCADE, \
         x REAL NOT NULL, y REAL NOT NULL, z REAL NOT NULL)"
    );
}
