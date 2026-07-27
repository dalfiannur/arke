//! Query berbasis-tipe dan deklarasi akses tersimpul (RFC-0005).
//!
//! [`Access`] menyatakan komponen apa yang dibaca/ditulis sebuah query atau
//! sistem — dipakai [`crate::schedule`] untuk analisis konflik. [`QueryData`]
//! menyimpulkan `Access` dari **tipe** query dan menyediakan iterasi internal.

use std::any::TypeId;

use crate::Component;

/// Himpunan komponen yang dibaca/ditulis, sebagai `TypeId`.
///
/// Bersifat opaque bagi konsumen; dibangun lewat [`Access::new`] +
/// [`Access::with_read`]/[`Access::with_write`], atau disimpulkan oleh
/// [`QueryData::access`].
#[derive(Default)]
pub struct Access {
    pub(crate) reads: Vec<TypeId>,
    pub(crate) writes: Vec<TypeId>,
}

impl Access {
    /// Akses kosong.
    pub fn new() -> Self {
        Self::default()
    }

    /// Menandai baca komponen `T`.
    pub fn with_read<T: Component>(mut self) -> Self {
        self.reads.push(TypeId::of::<T>());
        self
    }

    /// Menandai tulis komponen `T`.
    pub fn with_write<T: Component>(mut self) -> Self {
        self.writes.push(TypeId::of::<T>());
        self
    }

    /// Apakah dua akses berkonflik: berbagi komponen yang ditulis salah satu.
    pub(crate) fn conflicts(&self, other: &Access) -> bool {
        shares(&self.writes, &other.writes)
            || shares(&self.writes, &other.reads)
            || shares(&self.reads, &other.writes)
    }
}

/// Apakah dua daftar `TypeId` beririsan.
fn shares(a: &[TypeId], b: &[TypeId]) -> bool {
    a.iter().any(|id| b.contains(id))
}

use crate::World;

/// Bentuk data yang dapat diquery, dengan akses tersimpul dari tipe (RFC-0005).
///
/// Diterapkan untuk `&T`, `&mut T`, dan tuple 2-elemen. Iterasi bersifat
/// **internal**: `each` memanggil `f` untuk setiap entity yang cocok.
pub trait QueryData {
    /// Item yang dihasilkan per entity yang cocok, meminjam dari `World`.
    type Item<'w>;

    /// Akses (baca/tulis) yang disimpulkan dari tipe `Self`.
    fn access() -> Access;

    /// Menerapkan `f` pada setiap entity yang cocok.
    fn each(world: &mut World, f: impl FnMut(Self::Item<'_>));
}

impl<T: Component> QueryData for &T {
    type Item<'w> = &'w T;

    fn access() -> Access {
        Access::new().with_read::<T>()
    }

    fn each(world: &mut World, mut f: impl FnMut(&T)) {
        for item in world.query::<T>() {
            f(item);
        }
    }
}

impl<T: Component> QueryData for &mut T {
    type Item<'w> = &'w mut T;

    fn access() -> Access {
        Access::new().with_write::<T>()
    }

    fn each(world: &mut World, mut f: impl FnMut(&mut T)) {
        for item in world.query_mut::<T>() {
            f(item);
        }
    }
}

impl<A: Component, B: Component> QueryData for (&A, &mut B) {
    type Item<'w> = (&'w A, &'w mut B);

    fn access() -> Access {
        Access::new().with_read::<A>().with_write::<B>()
    }

    fn each(world: &mut World, mut f: impl FnMut(Self::Item<'_>)) {
        for pair in world.query_pair::<A, B>() {
            f(pair);
        }
    }
}

impl<A: Component, B: Component> QueryData for (&A, &B) {
    type Item<'w> = (&'w A, &'w B);

    fn access() -> Access {
        Access::new().with_read::<A>().with_read::<B>()
    }

    fn each(world: &mut World, mut f: impl FnMut(Self::Item<'_>)) {
        for pair in world.query_pair_ref::<A, B>() {
            f(pair);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::World;

    #[derive(PartialEq, Debug)]
    struct Pos(i32);
    #[derive(PartialEq, Debug)]
    struct Vel(i32);

    #[test]
    fn tuple_ref_mut_menyimpulkan_akses_dan_mengiterasi() {
        let acc = <(&Pos, &mut Vel)>::access();
        assert!(acc.reads.contains(&TypeId::of::<Pos>()));
        assert!(acc.writes.contains(&TypeId::of::<Vel>()));

        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Pos(1));
        world.insert(e, Vel(10));
        let f = world.spawn();
        world.insert(f, Vel(99)); // tanpa Pos → tak cocok

        <(&Pos, &mut Vel)>::each(&mut world, |(p, v)| v.0 += p.0);

        assert_eq!(world.get::<Vel>(e), Some(&Vel(11)));
        assert_eq!(world.get::<Vel>(f), Some(&Vel(99)));
    }

    #[test]
    fn tuple_ref_ref_menyimpulkan_baca_baca_dan_mengiterasi() {
        let acc = <(&Pos, &Vel)>::access();
        assert!(acc.reads.contains(&TypeId::of::<Pos>()));
        assert!(acc.reads.contains(&TypeId::of::<Vel>()));
        assert!(acc.writes.is_empty());

        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Pos(3));
        world.insert(e, Vel(4));

        let mut sum = 0;
        <(&Pos, &Vel)>::each(&mut world, |(p, v)| sum += p.0 + v.0);
        assert_eq!(sum, 7);
    }

    #[test]
    fn ref_menyimpulkan_baca_ref_mut_menyimpulkan_tulis() {
        let a = <&Pos>::access();
        assert!(a.reads.contains(&TypeId::of::<Pos>()));
        assert!(a.writes.is_empty());

        let b = <&mut Pos>::access();
        assert!(b.writes.contains(&TypeId::of::<Pos>()));
        assert!(b.reads.is_empty());
    }

    #[test]
    fn each_ref_mut_mengiterasi_dan_memutasi() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Pos(1));

        <&mut Pos>::each(&mut world, |p| p.0 += 10);

        assert_eq!(world.get::<Pos>(e), Some(&Pos(11)));
    }
}
