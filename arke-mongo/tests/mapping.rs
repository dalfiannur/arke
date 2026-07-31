//! Lapis 1 (RFC-0035 §7): tes pemetaan tanpa database.

use arke::{Value, World};
use arke_mongo::bson::{Bson, Document};
use arke_mongo::{
    Dir, IndexDef, MongoComponent, MongoError, Pid, Registry, bson_to_value, mongo_component,
    validate_names, value_to_bson,
};

#[derive(arke::Serialize, PartialEq, Debug)]
struct Position {
    x: f32,
    y: f32,
}
mongo_component!(Position => "position");

#[derive(arke::Serialize, PartialEq, Debug)]
struct Health {
    hp: i64,
}
mongo_component!(Health => "health", indexes: [IndexDef::asc("hp")]);

#[test]
fn makro_mengisi_nama_dan_indeks_kosong() {
    assert_eq!(Position::NAME, "position");
    assert!(Position::INDEXES.is_empty());
}

#[test]
fn makro_meneruskan_deklarasi_indeks() {
    assert_eq!(Health::NAME, "health");
    assert_eq!(Health::INDEXES.len(), 1);
    assert_eq!(Health::INDEXES[0].field, "hp");
    assert_eq!(Health::INDEXES[0].dir, Dir::Asc);
    assert!(!Health::INDEXES[0].unique);
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn index_def_unique_ditandai() {
    const IDX: IndexDef = IndexDef::asc("slug").unique();
    assert!(IDX.unique);
}

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

#[test]
fn nama_field_valid_diterima() {
    let v = Value::Map(vec![("hp".into(), Value::Int(1))]);
    assert!(validate_names("health", &v).is_ok());
}

#[test]
fn titik_dalam_nama_field_ditolak() {
    let v = Value::Map(vec![("a.b".into(), Value::Int(1))]);
    match validate_names("health", &v) {
        Err(MongoError::InvalidName { component, field }) => {
            assert_eq!(component, "health");
            assert_eq!(field, "a.b");
        }
        other => panic!("harus InvalidName, dapat {other:?}"),
    }
}

#[test]
fn dollar_di_awal_nama_field_ditolak() {
    let v = Value::Map(vec![("$set".into(), Value::Int(1))]);
    assert!(matches!(
        validate_names("health", &v),
        Err(MongoError::InvalidName { .. })
    ));
}

#[test]
fn validasi_menembus_map_bersarang_dan_list() {
    let v = Value::Map(vec![(
        "items".into(),
        Value::List(vec![Value::Map(vec![("bad.name".into(), Value::Null)])]),
    )]);
    assert!(matches!(
        validate_names("inventory", &v),
        Err(MongoError::InvalidName { .. })
    ));
}

#[test]
fn cmp_doc_memuat_hanya_komponen_yang_dimiliki_entity() {
    let mut reg = Registry::new();
    reg.push::<Position>();
    reg.push::<Health>();

    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Position { x: 1.0, y: 2.0 });

    let doc = reg.cmp_doc(&world, e).expect("cmp_doc harus sukses");
    assert!(doc.contains_key("position"));
    assert!(
        !doc.contains_key("health"),
        "komponen yang tak dimiliki entity tak boleh muncul"
    );

    let pos = doc.get_document("position").unwrap();
    assert_eq!(pos.get_f64("x").unwrap(), 1.0);
}

#[test]
#[should_panic(expected = "position")]
fn nama_komponen_yang_bertabrakan_panic_saat_register() {
    #[derive(arke::Serialize)]
    struct Lain {
        v: i64,
    }
    mongo_component!(Lain => "position");

    let mut reg = Registry::new();
    reg.push::<Position>();
    reg.push::<Lain>();
}

#[derive(arke::Serialize)]
struct IndeksSalah {
    hp: i64,
}
mongo_component!(IndeksSalah => "indeks_salah", indexes: [IndexDef::asc("tidak_ada")]);

/// RFC-0035 §2 (Am. 1): salah-ketik nama field pada `IndexDef` tak tertangkap
/// kompilasi, jadi ditangkap `debug_assert!` saat komponen diserialisasi.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "tidak_ada")]
fn index_def_menyebut_field_yang_tak_ada_gagal_di_build_debug() {
    let mut reg = Registry::new();
    reg.push::<IndeksSalah>();

    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, IndeksSalah { hp: 1 });
    let _ = reg.cmp_doc(&world, e);
}

#[test]
fn apply_menyisipkan_komponen_terdaftar_ke_world() {
    let mut reg = Registry::new();
    reg.push::<Position>();

    let mut src = World::new();
    let a = src.spawn();
    src.insert(a, Position { x: 3.0, y: 4.0 });
    let cmp = reg.cmp_doc(&src, a).unwrap();

    let mut dst = World::new();
    let b = dst.spawn();
    reg.apply(&mut dst, b, Pid::new(), &cmp).unwrap();

    assert_eq!(dst.get::<Position>(b), Some(&Position { x: 3.0, y: 4.0 }));
}

#[test]
fn apply_mengabaikan_komponen_yang_tak_terdaftar() {
    let mut reg = Registry::new();
    reg.push::<Position>();

    let mut cmp = Document::new();
    cmp.insert("tak_dikenal", Document::new());

    let mut world = World::new();
    let e = world.spawn();
    assert!(
        reg.apply(&mut world, e, Pid::new(), &cmp).is_ok(),
        "komponen milik service lain tak boleh menggagalkan pembacaan"
    );
}

#[test]
fn apply_gagal_keras_saat_bentuk_komponen_tak_cocok() {
    let mut reg = Registry::new();
    reg.push::<Position>();

    let mut bad = Document::new();
    bad.insert("x", "bukan angka");
    let mut cmp = Document::new();
    cmp.insert("position", bad);

    let mut world = World::new();
    let e = world.spawn();
    let pid = Pid::new();
    match reg.apply(&mut world, e, pid, &cmp) {
        Err(MongoError::Decode { component, .. }) => assert_eq!(component, "position"),
        other => panic!("harus Decode, dapat {other:?}"),
    }
}

#[test]
fn update_ops_set_per_sub_field_bukan_mengganti_cmp() {
    let mut reg = Registry::new();
    reg.push::<Position>();
    reg.push::<Health>();

    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Position { x: 1.0, y: 2.0 });

    let ops = reg.update_ops(&world, e).unwrap();
    let set = ops.get_document("$set").unwrap();
    assert!(
        set.contains_key("cmp.position"),
        "harus menyasar sub-field, bukan `cmp`"
    );
    assert!(
        !set.contains_key("cmp"),
        "mengganti `cmp` utuh akan menghapus komponen milik service lain"
    );
}

#[test]
fn update_ops_unset_komponen_terdaftar_yang_hilang() {
    let mut reg = Registry::new();
    reg.push::<Position>();
    reg.push::<Health>();

    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Position { x: 1.0, y: 2.0 });

    let ops = reg.update_ops(&world, e).unwrap();
    let unset = ops.get_document("$unset").unwrap();
    assert!(
        unset.contains_key("cmp.health"),
        "komponen terdaftar yang tak dimiliki entity harus di-unset"
    );
}

#[test]
fn update_ops_menaikkan_version_dengan_inc() {
    let mut reg = Registry::new();
    reg.push::<Position>();

    let mut world = World::new();
    let e = world.spawn();
    world.insert(e, Position { x: 0.0, y: 0.0 });

    let ops = reg.update_ops(&world, e).unwrap();
    assert_eq!(ops.get_document("$inc").unwrap().get_i64("version"), Ok(1));
}

#[test]
fn index_models_memakai_path_bersarang() {
    let mut reg = Registry::new();
    reg.push::<Health>(); // IndexDef::asc("hp")

    let models = reg.index_models();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].keys.get_i32("cmp.health.hp"), Ok(1));
}

#[test]
fn komponen_tanpa_indeks_tak_menghasilkan_model() {
    let mut reg = Registry::new();
    reg.push::<Position>();
    assert!(reg.index_models().is_empty());
}
