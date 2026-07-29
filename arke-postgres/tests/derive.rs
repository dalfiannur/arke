//! Uji `#[derive(PgComponent)]` — **tanpa database** (skema + round-trip).

use arke_postgres::{ColumnDef, IndexDef, PgComponent, PgType, PgValue, create_table_sql};

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

// RFC-0030: derive meng-generate token field typed per field skalar. Uji
// compile-level: token ada dgn tipe benar + operator/kombinator tersedia.
#[test]
fn token_field_typed_di_generate() {
    use arke_postgres::{Dir, Field};
    let _: Field<Stats, i32> = Stats::level();
    let _: Field<Stats, i64> = Stats::hp();
    let _: Field<Stats, u64> = Stats::xp();
    let _: Field<Stats, bool> = Stats::alive();
    let _: Field<Stats, String> = Stats::name();
    let _: Field<Position, f32> = Position::x();
    // Operator + kombinator + like (hanya pada String) kompilasi.
    let _ = Stats::level()
        .gte(5)
        .and(Stats::name().like("a%"))
        .or(Stats::hp().in_([1i64, 2]))
        .not();
    let _ = Dir::Desc;
}

#[test]
fn table_dan_kolom_untuk_struct_skalar() {
    assert_eq!(Position::TABLE, "cmp_position");
    assert_eq!(
        Position::COLUMNS.to_vec(),
        vec![
            ColumnDef::scalar("x", PgType::Real, false),
            ColumnDef::scalar("y", PgType::Real, false),
            ColumnDef::scalar("z", PgType::Real, false),
        ]
    );

    assert_eq!(Stats::TABLE, "cmp_stats");
    assert_eq!(
        Stats::COLUMNS.to_vec(),
        vec![
            ColumnDef::scalar("level", PgType::Integer, false),
            ColumnDef::scalar("hp", PgType::BigInt, false),
            ColumnDef::scalar("xp", PgType::Numeric, false),
            ColumnDef::scalar("speed", PgType::BigInt, false),
            ColumnDef::scalar("alive", PgType::Boolean, false),
            ColumnDef::scalar("name", PgType::Text, false),
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

#[derive(PgComponent, PartialEq, Debug)]
struct Optional {
    hp: Option<i32>,
    tag: Option<String>,
    ratio: Option<f32>,
}

#[test]
fn option_jadi_kolom_nullable() {
    assert_eq!(
        Optional::COLUMNS.to_vec(),
        vec![
            ColumnDef::scalar("hp", PgType::Integer, true),
            ColumnDef::scalar("tag", PgType::Text, true),
            ColumnDef::scalar("ratio", PgType::Real, true),
        ]
    );
}

#[test]
fn option_round_trip_some_dan_none() {
    let full = Optional {
        hp: Some(42),
        tag: Some("x".to_string()),
        ratio: Some(1.5),
    };
    assert_eq!(
        full.to_params(),
        vec![
            PgValue::Int(42),
            PgValue::Text("x".to_string()),
            PgValue::Float(1.5),
        ]
    );
    let back = Optional::from_params(&full.to_params()).unwrap();
    assert_eq!(back, full);

    let empty = Optional {
        hp: None,
        tag: None,
        ratio: None,
    };
    assert_eq!(
        empty.to_params(),
        vec![PgValue::Null, PgValue::Null, PgValue::Null]
    );
    assert_eq!(Optional::from_params(&empty.to_params()).unwrap(), empty);
}

#[derive(arke::Serialize, PartialEq, Debug, Clone)]
struct Inner {
    tags: i32,
    label: String,
}

#[derive(PgComponent, PartialEq, Debug)]
struct Blob {
    flat: i32,
    meta: Inner,          // non-skalar → JSONB
    maybe: Option<Inner>, // Option non-skalar → JSONB nullable
}

#[test]
fn jsonb_fallback_untuk_field_non_skalar() {
    assert_eq!(
        Blob::COLUMNS.to_vec(),
        vec![
            ColumnDef::scalar("flat", PgType::Integer, false),
            ColumnDef::scalar("meta", PgType::Jsonb, false),
            ColumnDef::scalar("maybe", PgType::Jsonb, true),
        ]
    );

    let b = Blob {
        flat: 5,
        meta: Inner {
            tags: 3,
            label: "hero".to_string(),
        },
        maybe: Some(Inner {
            tags: 9,
            label: "z".to_string(),
        }),
    };
    let params = b.to_params();
    // meta & maybe adalah JSON.
    match &params[1] {
        PgValue::Json(s) => assert!(s.contains("label") && s.contains("hero")),
        other => panic!("bukan Json: {other:?}"),
    }
    assert_eq!(Blob::from_params(&params).unwrap(), b);

    // maybe = None → Null.
    let n = Blob {
        flat: 1,
        meta: Inner {
            tags: 0,
            label: String::new(),
        },
        maybe: None,
    };
    assert_eq!(n.to_params()[2], PgValue::Null);
    assert_eq!(Blob::from_params(&n.to_params()).unwrap(), n);
}

#[derive(PgComponent, PartialEq, Debug)]
#[pg(check = "hp >= 0")]
struct Unit {
    #[pg(index)]
    kind: i32,
    #[pg(unique)]
    tag: i32,
    hp: i32,
}

#[test]
fn index_dan_check_dari_atribut_pg() {
    assert_eq!(
        Unit::INDEXES.to_vec(),
        vec![
            IndexDef {
                column: "kind",
                unique: false
            },
            IndexDef {
                column: "tag",
                unique: true
            },
        ]
    );
    assert_eq!(Unit::CHECKS.to_vec(), vec!["hp >= 0"]);
    // Komponen tanpa atribut → kosong.
    assert!(Position::INDEXES.is_empty());
    assert!(Position::CHECKS.is_empty());
}

#[test]
fn create_table_sql_benar() {
    assert_eq!(
        create_table_sql::<Position>(),
        "CREATE TABLE IF NOT EXISTS cmp_position \
         (pid BIGINT PRIMARY KEY REFERENCES arke_entities(pid) ON DELETE CASCADE, \
         x REAL NOT NULL, y REAL NOT NULL, z REAL NOT NULL)"
    );
}

// RFC-0034 Am.3: field Ref<T> → SATU kolom `<name>_id` (pid, `entity_ref`) + token
// relasi BERTIPE RelRef<T>. `_gen` dihapus; generasi tak lagi disimpan.
#[derive(PgComponent, PartialEq, Debug)]
struct RefTarget {
    hp: i32,
}
#[derive(PgComponent, PartialEq, Debug)]
struct RefHolder {
    a: arke_postgres::Ref<RefTarget>,
    b: Option<arke_postgres::Ref<RefTarget>>,
}

#[test]
fn ref_bertipe_kolom_token_dan_round_trip() {
    use arke_postgres::{Field, RelRef};
    // Ref<T> → 1 kolom BIGINT `<name>_id` ber-`entity_ref`; Option ikut nullable.
    let ref_col = |name: &'static str, nullable: bool| ColumnDef {
        name,
        ty: PgType::BigInt,
        nullable,
        references: None,
        entity_ref: true,
    };
    assert_eq!(
        RefHolder::COLUMNS.to_vec(),
        vec![ref_col("a_id", false), ref_col("b_id", true)]
    );
    // Token relasi bertipe (compile-level).
    let _: Field<RefHolder, RelRef<RefTarget>> = RefHolder::a();
    let _: Field<RefHolder, RelRef<RefTarget>> = RefHolder::b();

    // Round-trip tanpa DB: `Ref` menyimpan **indeks** (5); generasi hilang → gen 0.
    let e = arke::Entity::from_raw(5, 2);
    let h = RefHolder {
        a: arke_postgres::Ref::new(e),
        b: None,
    };
    assert_eq!(h.to_params(), vec![PgValue::Ref(5), PgValue::Null]);
    let back = RefHolder::from_params(&h.to_params()).unwrap();
    assert_eq!(back.a.entity(), arke::Entity::from_raw(5, 0));
    assert_eq!(back.b, None);
}
