//! Filter query `With`/`Without` (RFC-0014), API publik, `forbid(unsafe_code)`.

#![forbid(unsafe_code)]

use arke::{Schedule, System, With, Without, World};

#[derive(PartialEq, Debug)]
struct Health(i64);
struct Frozen; // penanda (unit struct)
struct Player;

#[test]
fn without_hanya_yang_tak_punya_komponen() {
    let mut w = World::new();
    let bebas = w.spawn();
    w.insert(bebas, Health(1));
    let beku = w.spawn();
    w.insert(beku, Health(1));
    w.insert(beku, Frozen);

    let mut s = Schedule::new();
    s.add(System::each_filtered::<&mut Health, Without<Frozen>>(|h| {
        h.0 += 10;
    }));
    s.run(&mut w);

    assert_eq!(w.get::<Health>(bebas), Some(&Health(11))); // tanpa Frozen → naik
    assert_eq!(w.get::<Health>(beku), Some(&Health(1))); // Frozen → dilewati
}

#[test]
fn with_hanya_yang_punya_komponen() {
    let mut w = World::new();
    let biasa = w.spawn();
    w.insert(biasa, Health(1));
    let pemain = w.spawn();
    w.insert(pemain, Health(1));
    w.insert(pemain, Player);

    let mut s = Schedule::new();
    s.add(System::each_filtered::<&mut Health, With<Player>>(|h| {
        h.0 += 10;
    }));
    s.run(&mut w);

    assert_eq!(w.get::<Health>(biasa), Some(&Health(1))); // tanpa Player → dilewati
    assert_eq!(w.get::<Health>(pemain), Some(&Health(11))); // Player → naik
}

#[test]
fn tuple_filter_with_dan_without() {
    let mut w = World::new();
    let target = w.spawn();
    w.insert(target, Health(1));
    w.insert(target, Player);
    let beku = w.spawn();
    w.insert(beku, Health(1));
    w.insert(beku, Player);
    w.insert(beku, Frozen);

    let mut s = Schedule::new();
    s.add(System::each_filtered::<
        &mut Health,
        (With<Player>, Without<Frozen>),
    >(|h| h.0 += 10));
    s.run(&mut w);

    assert_eq!(w.get::<Health>(target), Some(&Health(11))); // Player & bukan Frozen
    assert_eq!(w.get::<Health>(beku), Some(&Health(1))); // Frozen → dilewati
}

#[test]
fn filter_tak_memengaruhi_konflik_scheduler() {
    // Sistem 0 menulis Health dengan filter Without<Frozen>; sistem 1 menulis
    // KOMPONEN Frozen. Karena filter tak menyumbang akses, keduanya tak konflik.
    let mut s = Schedule::new();
    s.add(System::each_filtered::<&mut Health, Without<Frozen>>(
        |_| {},
    ));
    s.add(System::each::<&mut Frozen>(|_| {}));
    assert_eq!(s.stages(), vec![vec![0, 1]]);
}
