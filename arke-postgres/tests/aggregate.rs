//! Uji agregasi typed (`sum`/`min`/`max`/`avg`, `group_by().count()/sum()`).
//! Dilewati bila `DATABASE_URL` tak diset. Komponen `Sale` unik ke berkas ini.

use arke::World;
use arke_postgres::{PgComponent, PgStore};

#[derive(PgComponent, PartialEq, Debug, Clone)]
struct Sale {
    region: String,
    qty: i32,
    amount: i64,
    price: f64,
}

#[tokio::test]
async fn agregasi_typed() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL tak diset");
        return;
    };
    let mut store = PgStore::connect(&url).await.expect("connect");
    store.register::<Sale>();
    store.migrate().await.unwrap();
    store.save(&World::new()).await.unwrap();

    // Kosong → None / Vec kosong.
    assert_eq!(
        store.query::<Sale>().sum::<i64>(Sale::qty()).await.unwrap(),
        None
    );
    assert!(
        store
            .query::<Sale>()
            .group_by(Sale::region())
            .count()
            .await
            .unwrap()
            .is_empty()
    );

    let mut w = World::new();
    for (region, qty, amount, price) in [
        ("east", 1, 100, 1.5),
        ("east", 2, 200, 2.5),
        ("west", 3, 300, 3.5),
        ("west", 4, 4_000_000_000, 4.5), // amount > i32::MAX
        ("north", 5, 500, 5.5),
    ] {
        let e = w.spawn();
        w.insert(
            e,
            Sale {
                region: region.into(),
                qty,
                amount,
                price,
            },
        );
    }
    store.save(&w).await.unwrap();

    // Skalar, dengan filter.
    let f = || Sale::region().in_(["east".to_string(), "west".to_string()]);
    assert_eq!(
        store
            .query::<Sale>()
            .filter(f())
            .sum::<i64>(Sale::qty())
            .await
            .unwrap(),
        Some(10)
    );
    assert_eq!(
        store
            .query::<Sale>()
            .filter(f())
            .min::<i32>(Sale::qty())
            .await
            .unwrap(),
        Some(1)
    );
    assert_eq!(
        store
            .query::<Sale>()
            .filter(f())
            .max::<i32>(Sale::qty())
            .await
            .unwrap(),
        Some(4)
    );
    assert_eq!(
        store
            .query::<Sale>()
            .filter(f())
            .avg::<f64>(Sale::qty())
            .await
            .unwrap(),
        Some(2.5)
    );
    // SUM(bigint) → numeric di Postgres; cast eksplisit ke bigint tetap eksak.
    assert_eq!(
        store
            .query::<Sale>()
            .sum::<i64>(Sale::amount())
            .await
            .unwrap(),
        Some(4_000_001_100)
    );
    // Nilai eksak sebagai teks.
    assert_eq!(
        store
            .query::<Sale>()
            .sum::<String>(Sale::amount())
            .await
            .unwrap()
            .as_deref(),
        Some("4000001100")
    );
    assert_eq!(
        store
            .query::<Sale>()
            .max::<f64>(Sale::price())
            .await
            .unwrap(),
        Some(5.5)
    );
    // Cast sempit yang meluap → error decode, bukan nilai salah.
    assert!(
        store
            .query::<Sale>()
            .sum::<i32>(Sale::amount())
            .await
            .is_err()
    );

    // GROUP BY: urut kunci.
    let counts = store
        .query::<Sale>()
        .group_by(Sale::region())
        .count()
        .await
        .unwrap();
    assert_eq!(
        counts,
        vec![
            ("east".to_string(), 2),
            ("north".to_string(), 1),
            ("west".to_string(), 2)
        ]
    );
    let sums = store
        .query::<Sale>()
        .filter(Sale::qty().gte(2))
        .group_by(Sale::region())
        .sum::<i64>(Sale::qty())
        .await
        .unwrap();
    assert_eq!(
        sums,
        vec![
            ("east".to_string(), Some(2)),
            ("north".to_string(), Some(5)),
            ("west".to_string(), Some(7))
        ]
    );
    // Kunci numerik.
    let by_qty = store
        .query::<Sale>()
        .group_by(Sale::qty())
        .avg::<f64>(Sale::price())
        .await
        .unwrap();
    assert_eq!(by_qty.len(), 5);
    assert_eq!(by_qty[0], (1, Some(1.5)));
}
