//! Uji integrasi query builder typed (RFC-0030) terhadap Postgres nyata.
//! Dilewati bila `DATABASE_URL` tak diset. Komponen `Mob` unik ke berkas ini
//! (tabel `cmp_mob`) agar tak balapan dengan berkas uji lain.

use arke::World;
use arke_postgres::{Dir, PgComponent, PgStore};

#[derive(PgComponent, PartialEq, Debug)]
struct Mob {
    power: i32,
    name: String,
}

#[tokio::test]
async fn query_builder_typed_end_to_end() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let mut store = PgStore::connect(&url).await.expect("connect");
    store.register::<Mob>();
    store.migrate().await.unwrap();
    store.save(&World::new()).await.unwrap(); // slate bersih

    // power = 0,10,20,30,40 dengan nama campur.
    let names = ["alpha", "beta", "apex", "gamma", "aqua"];
    let mut world = World::new();
    for (i, nm) in names.iter().enumerate() {
        let e = world.spawn();
        world.insert(
            e,
            Mob {
                power: (i as i32) * 10,
                name: (*nm).to_string(),
            },
        );
    }
    store.save(&world).await.unwrap();

    // 1) Builder filter ≡ load_where string setara.
    let mut a = World::new();
    let na = store
        .query::<Mob>()
        .filter(Mob::power().lt(25))
        .load(&mut a)
        .await
        .unwrap();
    let mut b = World::new();
    let nb = store.load_where::<Mob>(&mut b, "power < 25").await.unwrap();
    assert_eq!(na, nb, "builder harus setara load_where");
    assert_eq!(na, 3); // 0,10,20

    // 2) order_by DESC + limit + offset.
    let mut c = World::new();
    let nc = store
        .query::<Mob>()
        .order_by(Mob::power(), Dir::Desc)
        .limit(2)
        .offset(1)
        .load(&mut c)
        .await
        .unwrap();
    assert_eq!(nc, 2); // lewati 40, ambil 30 & 20

    // 3) in_ + like + and (power ∈ {0,10,40} DAN name LIKE 'a%').
    //    {0,10,40}=alpha,beta,aqua ; a%=alpha,apex,aqua ; irisan=alpha(0),aqua(40).
    let mut d = World::new();
    let nd = store
        .query::<Mob>()
        .filter(Mob::power().in_([0, 10, 40]).and(Mob::name().like("a%")))
        .load(&mut d)
        .await
        .unwrap();
    assert_eq!(nd, 2);

    // 4) between (inklusif): power 10,20,30.
    let mut e = World::new();
    let ne = store
        .query::<Mob>()
        .filter(Mob::power().between(10, 30))
        .load(&mut e)
        .await
        .unwrap();
    assert_eq!(ne, 3);
}
