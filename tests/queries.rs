//! Query tuple generik (RFC-0013): arity & mutabilitas campuran, API publik,
//! `forbid(unsafe_code)`.

#![forbid(unsafe_code)]

use arke::{Schedule, System, World};

#[derive(PartialEq, Debug)]
struct A(i64);
#[derive(PartialEq, Debug)]
struct B(i64);
#[derive(PartialEq, Debug)]
struct C(i64);
#[derive(PartialEq, Debug)]
struct D(i64);

#[test]
fn query_dua_mutabel() {
    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, A(1));
    w.insert(e, B(2));

    let mut s = Schedule::new();
    s.add(System::each::<(&mut A, &mut B)>(|(a, b)| {
        a.0 += 10;
        b.0 += 20;
    }));
    s.run(&mut w);

    assert_eq!(w.get::<A>(e), Some(&A(11)));
    assert_eq!(w.get::<B>(e), Some(&B(22)));
}

#[test]
fn query_arity_tiga_mutabilitas_campuran() {
    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, A(1));
    w.insert(e, B(2));
    w.insert(e, C(3));
    let f = w.spawn();
    w.insert(f, A(1));
    w.insert(f, B(2)); // tanpa C → tak cocok

    let mut s = Schedule::new();
    s.add(System::each::<(&A, &B, &mut C)>(|(a, b, c)| {
        c.0 += a.0 + b.0
    }));
    s.run(&mut w);

    assert_eq!(w.get::<C>(e), Some(&C(6))); // 1+2+3
    assert_eq!(w.get::<C>(f), None);
}

#[test]
fn query_arity_empat() {
    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, A(1));
    w.insert(e, B(2));
    w.insert(e, C(3));
    w.insert(e, D(4));

    let mut s = Schedule::new();
    s.add(System::each::<(&A, &B, &C, &mut D)>(|(a, b, c, d)| {
        d.0 += a.0 + b.0 + c.0;
    }));
    s.run(&mut w);

    assert_eq!(w.get::<D>(e), Some(&D(10))); // 1+2+3+4
}

#[test]
#[should_panic(expected = "A")]
fn query_alias_mut_ke_komponen_sama_ditolak() {
    let mut w = World::new();
    let e = w.spawn();
    w.insert(e, A(1));
    // (&mut A, &mut A): alias &mut ke komponen yang sama → panik menyebut A.
    let mut s = Schedule::new();
    s.add(System::each::<(&mut A, &mut A)>(|(x, y)| {
        x.0 += y.0;
    }));
    s.run(&mut w);
}
