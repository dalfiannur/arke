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
    // Namespace terpisah agar komponen `T` dan resource `T` (TypeId sama) tak
    // salah-konflik (RFC-0010).
    pub(crate) resource_reads: Vec<TypeId>,
    pub(crate) resource_writes: Vec<TypeId>,
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

    /// Menandai baca resource `R`.
    pub fn with_resource_read<R: 'static + Send>(mut self) -> Self {
        self.resource_reads.push(TypeId::of::<R>());
        self
    }

    /// Menandai tulis resource `R`.
    pub fn with_resource_write<R: 'static + Send>(mut self) -> Self {
        self.resource_writes.push(TypeId::of::<R>());
        self
    }

    /// Apakah dua akses berkonflik: berbagi komponen **atau** resource yang
    /// ditulis salah satu (dinilai per-namespace).
    pub(crate) fn conflicts(&self, other: &Access) -> bool {
        shares(&self.writes, &other.writes)
            || shares(&self.writes, &other.reads)
            || shares(&self.reads, &other.writes)
            || shares(&self.resource_writes, &other.resource_writes)
            || shares(&self.resource_writes, &other.resource_reads)
            || shares(&self.resource_reads, &other.resource_writes)
    }
}

/// Apakah dua daftar `TypeId` beririsan.
fn shares(a: &[TypeId], b: &[TypeId]) -> bool {
    a.iter().any(|id| b.contains(id))
}

use crate::World;

/// Bentuk data yang dapat diquery, dengan akses tersimpul dari tipe (RFC-0005).
///
/// Diterapkan untuk `&T`, `&mut T`, dan **tuple sembarang-arity** (2–8) dengan
/// mutabilitas campuran (RFC-0013). Iterasi bersifat **internal**: `each`
/// memanggil `f` untuk setiap entity yang cocok.
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

// ---- Query tuple generik (RFC-0013) --------------------------------------

use crate::component::ComponentId;
use crate::error::EcsError;
use crate::storage::{Column, TypedColumn};

/// Satu elemen (`&T` atau `&mut T`) dari sebuah query tuple.
///
/// Detail implementasi: hanya `&T`/`&mut T` yang mengimplementasikannya;
/// pengguna tak perlu menyentuhnya langsung (dipakai sebagai *bound* oleh impl
/// `QueryData` tuple).
#[doc(hidden)]
#[allow(private_interfaces)]
pub trait QueryTerm {
    /// Item yang dihasilkan per baris.
    type Item<'w>;
    /// Menambahkan akses (baca/tulis) term ini.
    fn access(access: &mut Access);
    /// `ComponentId` komponen term ini, bila terdaftar.
    fn component_id(world: &World) -> Option<ComponentId>;
    /// Iterator atas item dari sebuah referensi kolom mutabel.
    fn iter(col: &mut Box<dyn Column>) -> impl Iterator<Item = Self::Item<'_>>;
}

#[allow(private_interfaces)]
impl<T: Component> QueryTerm for &T {
    type Item<'w> = &'w T;
    fn access(access: &mut Access) {
        access.reads.push(TypeId::of::<T>());
    }
    fn component_id(world: &World) -> Option<ComponentId> {
        world.component_id::<T>()
    }
    fn iter(col: &mut Box<dyn Column>) -> impl Iterator<Item = &T> {
        col.as_any()
            .downcast_ref::<TypedColumn<T>>()
            .expect("tipe kolom tak cocok")
            .0
            .iter()
    }
}

#[allow(private_interfaces)]
impl<T: Component> QueryTerm for &mut T {
    type Item<'w> = &'w mut T;
    fn access(access: &mut Access) {
        access.writes.push(TypeId::of::<T>());
    }
    fn component_id(world: &World) -> Option<ComponentId> {
        world.component_id::<T>()
    }
    fn iter(col: &mut Box<dyn Column>) -> impl Iterator<Item = &mut T> {
        col.as_any_mut()
            .downcast_mut::<TypedColumn<T>>()
            .expect("tipe kolom tak cocok")
            .0
            .iter_mut()
    }
}

/// Menolak query di mana dua term merujuk komponen yang sama (alias `&mut`),
/// dengan pesan yang menyebut komponen (STD-0008).
fn assert_no_alias(world: &World, cids: &[ComponentId]) {
    for (i, &c) in cids.iter().enumerate() {
        if cids[..i].contains(&c) {
            panic!(
                "{}",
                EcsError::QueryConflict {
                    component: world.component_name(c),
                }
            );
        }
    }
}

/// Menghasilkan `impl QueryData` untuk tuple dengan arity tertentu.
macro_rules! impl_query_tuple {
    ($($T:ident $cid:ident $var:ident),+) => {
        impl<$($T: QueryTerm),+> QueryData for ($($T,)+) {
            type Item<'w> = ($($T::Item<'w>,)+);

            fn access() -> Access {
                let mut access = Access::new();
                $(<$T as QueryTerm>::access(&mut access);)+
                access
            }

            fn each(world: &mut World, mut f: impl FnMut(Self::Item<'_>)) {
                let ($(::core::option::Option::Some($cid),)+) =
                    ($(<$T as QueryTerm>::component_id(world),)+)
                else {
                    return;
                };
                let cids = [$($cid),+];
                assert_no_alias(world, &cids);

                for archetype in world.archetypes_mut() {
                    let idxs = [$(archetype.column_index($cid)),+];
                    if idxs.iter().any(::core::option::Option::is_none) {
                        continue;
                    }
                    let cols = idxs.map(|o| o.unwrap());
                    let [$($var),+] = archetype.columns_disjoint_mut(cols);
                    let ($(mut $var,)+) = ($(<$T as QueryTerm>::iter($var),)+);
                    loop {
                        match ($($var.next(),)+) {
                            ($(::core::option::Option::Some($var),)+) => f(($($var,)+)),
                            _ => break,
                        }
                    }
                }
            }
        }
    };
}

impl_query_tuple!(A ca va, B cb vb);
impl_query_tuple!(A ca va, B cb vb, C cc vc);
impl_query_tuple!(A ca va, B cb vb, C cc vc, D cd vd);
impl_query_tuple!(A ca va, B cb vb, C cc vc, D cd vd, E ce ve);
impl_query_tuple!(A ca va, B cb vb, C cc vc, D cd vd, E ce ve, F cf vf);

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
        let a = <&Pos as QueryData>::access();
        assert!(a.reads.contains(&TypeId::of::<Pos>()));
        assert!(a.writes.is_empty());

        let b = <&mut Pos as QueryData>::access();
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
