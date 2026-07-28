//! Query berbasis-tipe dan deklarasi akses tersimpul (RFC-0005).
//!
//! [`Access`] menyatakan komponen apa yang dibaca/ditulis sebuah query atau
//! sistem — dipakai [`crate::schedule`] untuk analisis konflik. [`QueryData`]
//! menyimpulkan `Access` dari **tipe** query dan menyediakan iterasi internal.
//!
//! `unsafe` di modul ini **terkurung** pada [`QueryTerm::iter_shared`] untuk
//! term `&mut T` (memanggil `TypedColumn::data_mut_shared`); sound karena term
//! query mengakses kolom **distinct** (cek-alias) dan penjadwal menjamin akses
//! disjoint lintas-sistem (RFC-0016). Diverifikasi miri di CI.

#![allow(unsafe_code)]

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

/// Cache archetype yang cocok untuk sebuah query (RFC-0017).
///
/// Diperluas **inkremental**: karena archetype bersifat append-only dan
/// set-komponennya immutable, sebuah archetype yang cocok tetap cocok selamanya
/// — cache hanya perlu **diperluas** saat archetype baru muncul, tak pernah
/// diinvalidasi. Simpan lintas-run (mis. di sebuah [`System`](crate::System))
/// agar query berulang jadi O(archetype cocok), bukan O(semua archetype).
#[derive(Default)]
pub struct QueryState {
    /// Indeks archetype (di [`World::archetypes`](crate::World)) yang cocok.
    matched: Vec<usize>,
    /// Jumlah archetype yang sudah diperiksa (batas scan inkremental).
    scanned: usize,
}

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

    /// Menerapkan `f` pada setiap entity yang cocok **dan** lolos filter `F`,
    /// memakai `state` sebagai cache archetype yang di-scan **inkremental**
    /// (RFC-0017). Ini implementasi inti.
    ///
    /// Alur: resolve `ComponentId` (bila komponen fetch/`With` belum terdaftar,
    /// return tanpa memajukan `scanned`) → scan `archetype[state.scanned..]`,
    /// tambahkan yang cocok ke `state.matched` → iterasi **hanya**
    /// `state.matched`.
    fn each_cached<F: QueryFilter>(
        world: &World,
        state: &mut QueryState,
        f: impl FnMut(Self::Item<'_>),
    );

    /// Menerapkan `f` pada setiap entity yang cocok **dan** lolos filter `F`,
    /// lewat `&World` **berbagi** (RFC-0016). Wrapper sekali-pakai atas
    /// [`Self::each_cached`] dengan [`QueryState`] baru (memindai semua).
    fn each_filtered_shared<F: QueryFilter>(world: &World, f: impl FnMut(Self::Item<'_>)) {
        let mut state = QueryState::default();
        Self::each_cached::<F>(world, &mut state, f);
    }

    /// Seperti [`Self::each_filtered_shared`] tetapi dari `&mut World` eksklusif.
    fn each_filtered<F: QueryFilter>(world: &mut World, f: impl FnMut(Self::Item<'_>)) {
        Self::each_filtered_shared::<F>(&*world, f);
    }

    /// Menerapkan `f` pada setiap entity yang cocok (tanpa filter).
    fn each(world: &mut World, f: impl FnMut(Self::Item<'_>)) {
        Self::each_filtered::<()>(world, f);
    }
}

impl<T: Component> QueryData for &T {
    type Item<'w> = &'w T;

    fn access() -> Access {
        Access::new().with_read::<T>()
    }

    fn each_cached<F: QueryFilter>(world: &World, state: &mut QueryState, mut f: impl FnMut(&T)) {
        let Some(cid) = world.component_id::<T>() else {
            return;
        };
        let (with, without) = match resolve_filter::<F>(world) {
            Some(v) => v,
            None => return,
        };
        let archetypes = world.archetypes();
        for (i, archetype) in archetypes.iter().enumerate().skip(state.scanned) {
            if archetype.column_index(cid).is_some() && filter_matches(archetype, &with, &without) {
                state.matched.push(i);
            }
        }
        state.scanned = archetypes.len();
        for &ai in &state.matched {
            let archetype = &archetypes[ai];
            let col = archetype.column_index(cid);
            for item in <&T as QueryTerm>::iter_shared(archetype, col) {
                f(item);
            }
        }
    }
}

impl<T: Component> QueryData for &mut T {
    type Item<'w> = &'w mut T;

    fn access() -> Access {
        Access::new().with_write::<T>()
    }

    fn each_cached<F: QueryFilter>(
        world: &World,
        state: &mut QueryState,
        mut f: impl FnMut(&mut T),
    ) {
        let Some(cid) = world.component_id::<T>() else {
            return;
        };
        let (with, without) = match resolve_filter::<F>(world) {
            Some(v) => v,
            None => return,
        };
        let archetypes = world.archetypes();
        for (i, archetype) in archetypes.iter().enumerate().skip(state.scanned) {
            if archetype.column_index(cid).is_some() && filter_matches(archetype, &with, &without) {
                state.matched.push(i);
            }
        }
        state.scanned = archetypes.len();
        for &ai in &state.matched {
            let archetype = &archetypes[ai];
            let col = archetype.column_index(cid);
            for item in <&mut T as QueryTerm>::iter_shared(archetype, col) {
                f(item);
            }
        }
    }
}

impl QueryData for Entity {
    type Item<'w> = Entity;

    fn access() -> Access {
        Access::new() // handle entity: tak baca/tulis komponen (RFC-0020)
    }

    fn each_cached<F: QueryFilter>(
        world: &World,
        state: &mut QueryState,
        mut f: impl FnMut(Entity),
    ) {
        let (with, without) = match resolve_filter::<F>(world) {
            Some(v) => v,
            None => return,
        };
        let archetypes = world.archetypes();
        for (i, archetype) in archetypes.iter().enumerate().skip(state.scanned) {
            if filter_matches(archetype, &with, &without) {
                state.matched.push(i);
            }
        }
        state.scanned = archetypes.len();
        for &ai in &state.matched {
            for &entity in archetypes[ai].entities() {
                f(entity);
            }
        }
    }
}

// ---- Query tuple generik (RFC-0013) --------------------------------------

use crate::archetype::Archetype;
use crate::component::ComponentId;
use crate::entity::Entity;
use crate::error::EcsError;
use crate::storage::TypedColumn;

/// Syarat pencocokan sebuah [`QueryTerm`] terhadap archetype (RFC-0020).
enum Requirement {
    /// Butuh kolom komponen `cid` hadir (`&T` / `&mut T`).
    Column(ComponentId),
    /// Tak butuh kolom apa pun; cocok archetype mana saja (`Entity`).
    Any,
    /// Tipe komponen tak pernah terdaftar → query tak mungkin cocok.
    Never,
}

/// Satu elemen dari sebuah query tuple: `&T`, `&mut T`, atau [`Entity`]
/// (RFC-0013/0020).
///
/// Detail implementasi; pengguna tak perlu menyentuhnya langsung (dipakai
/// sebagai *bound* oleh impl `QueryData` tuple).
#[doc(hidden)]
#[allow(private_interfaces)]
pub trait QueryTerm {
    /// Item yang dihasilkan per baris.
    type Item<'w>;
    /// Menambahkan akses (baca/tulis) term ini. `Entity` tak menambah apa pun.
    fn access(access: &mut Access);
    /// Syarat pencocokan term ini terhadap archetype.
    fn requirement(world: &World) -> Requirement;
    /// Iterator atas item untuk sebuah `archetype`. `col` = indeks kolom
    /// teresolusi (untuk term komponen) atau `None` (untuk `Entity`).
    fn iter_shared(
        archetype: &Archetype,
        col: Option<usize>,
    ) -> impl Iterator<Item = Self::Item<'_>>;
}

#[allow(private_interfaces)]
impl<T: Component> QueryTerm for &T {
    type Item<'w> = &'w T;
    fn access(access: &mut Access) {
        access.reads.push(TypeId::of::<T>());
    }
    fn requirement(world: &World) -> Requirement {
        match world.component_id::<T>() {
            Some(cid) => Requirement::Column(cid),
            None => Requirement::Never,
        }
    }
    fn iter_shared(archetype: &Archetype, col: Option<usize>) -> impl Iterator<Item = &T> {
        let col = archetype.column(col.expect("term komponen butuh kolom teresolusi"));
        col.as_any()
            .downcast_ref::<TypedColumn<T>>()
            .expect("tipe kolom tak cocok")
            .data()
            .iter()
    }
}

#[allow(private_interfaces)]
impl<T: Component> QueryTerm for &mut T {
    type Item<'w> = &'w mut T;
    fn access(access: &mut Access) {
        access.writes.push(TypeId::of::<T>());
    }
    fn requirement(world: &World) -> Requirement {
        match world.component_id::<T>() {
            Some(cid) => Requirement::Column(cid),
            None => Requirement::Never,
        }
    }
    fn iter_shared(archetype: &Archetype, col: Option<usize>) -> impl Iterator<Item = &mut T> {
        let col = archetype.column(col.expect("term komponen butuh kolom teresolusi"));
        let typed = col
            .as_any()
            .downcast_ref::<TypedColumn<T>>()
            .expect("tipe kolom tak cocok");
        // SAFETY: term query mengakses kolom distinct (cek-alias) dan stage
        // penjadwal menjamin akses disjoint → tak ada peminjaman lain ke data
        // kolom ini (RFC-0016). Diverifikasi miri.
        unsafe { typed.data_mut_shared() }.iter_mut()
    }
}

#[allow(private_interfaces)]
impl QueryTerm for Entity {
    type Item<'w> = Entity;
    fn access(_access: &mut Access) {
        // Handle entity bersifat baca-saja; tak menyumbang akses komponen.
    }
    fn requirement(_world: &World) -> Requirement {
        Requirement::Any // cocok archetype mana saja
    }
    fn iter_shared(archetype: &Archetype, _col: Option<usize>) -> impl Iterator<Item = Entity> {
        archetype.entities().iter().copied()
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

// ---- Filter query: With / Without (RFC-0014) ------------------------------

use std::marker::PhantomData;

/// Filter: hanya cocokkan entity yang **memiliki** komponen `T` (tanpa mengambil
/// datanya).
pub struct With<T>(PhantomData<T>);

/// Filter: hanya cocokkan entity yang **tidak** memiliki komponen `T`.
pub struct Without<T>(PhantomData<T>);

/// Batasan pencocokan query yang tidak mengambil data (RFC-0014).
pub trait QueryFilter {
    /// Kumpulkan komponen yang harus **hadir** (`with`) dan **absen** (`without`).
    /// `false` bila sebuah `With` komponennya tak terdaftar (→ tak ada yang cocok).
    fn resolve(world: &World, with: &mut Vec<ComponentId>, without: &mut Vec<ComponentId>) -> bool;
}

impl<T: Component> QueryFilter for With<T> {
    fn resolve(
        world: &World,
        with: &mut Vec<ComponentId>,
        _without: &mut Vec<ComponentId>,
    ) -> bool {
        match world.component_id::<T>() {
            Some(c) => {
                with.push(c);
                true
            }
            None => false, // T tak terdaftar → tak ada archetype yang punya → tak cocok
        }
    }
}

impl<T: Component> QueryFilter for Without<T> {
    fn resolve(
        world: &World,
        _with: &mut Vec<ComponentId>,
        without: &mut Vec<ComponentId>,
    ) -> bool {
        if let Some(c) = world.component_id::<T>() {
            without.push(c);
        }
        true // tak terdaftar → pasti absen → terpenuhi
    }
}

impl QueryFilter for () {
    fn resolve(_: &World, _: &mut Vec<ComponentId>, _: &mut Vec<ComponentId>) -> bool {
        true
    }
}

macro_rules! impl_filter_tuple {
    ($($F:ident),+) => {
        impl<$($F: QueryFilter),+> QueryFilter for ($($F,)+) {
            fn resolve(
                world: &World,
                with: &mut Vec<ComponentId>,
                without: &mut Vec<ComponentId>,
            ) -> bool {
                $(<$F as QueryFilter>::resolve(world, with, without) &&)+ true
            }
        }
    };
}
impl_filter_tuple!(A);
impl_filter_tuple!(A, B);
impl_filter_tuple!(A, B, C);
impl_filter_tuple!(A, B, C, D);

/// Me-resolve `F` ke (komponen hadir, komponen absen); `None` bila `F` mustahil
/// cocok (mis. `With` tak terdaftar).
fn resolve_filter<F: QueryFilter>(world: &World) -> Option<(Vec<ComponentId>, Vec<ComponentId>)> {
    let mut with = Vec::new();
    let mut without = Vec::new();
    if F::resolve(world, &mut with, &mut without) {
        Some((with, without))
    } else {
        None
    }
}

/// Apakah `archetype` memuat semua `with` dan tak satupun `without`.
fn filter_matches(archetype: &Archetype, with: &[ComponentId], without: &[ComponentId]) -> bool {
    with.iter().all(|&c| archetype.contains(c)) && without.iter().all(|&c| !archetype.contains(c))
}

/// Menghasilkan `impl QueryData` untuk tuple dengan arity tertentu. Tiap term
/// (`&T`/`&mut T`/`Entity`) melapor [`Requirement`] lalu diiterasi lockstep;
/// term komponen mengakses kolom **distinct** (RFC-0013/0016/0020).
macro_rules! impl_query_tuple {
    ($($T:ident $req:ident $var:ident),+) => {
        impl<$($T: QueryTerm),+> QueryData for ($($T,)+) {
            type Item<'w> = ($($T::Item<'w>,)+);

            fn access() -> Access {
                let mut access = Access::new();
                $(<$T as QueryTerm>::access(&mut access);)+
                access
            }

            fn each_cached<Fil: QueryFilter>(
                world: &World,
                state: &mut QueryState,
                mut f: impl FnMut(Self::Item<'_>),
            ) {
                // Syarat per-term; bila ada `Never` → query tak mungkin cocok.
                $(let $req = <$T as QueryTerm>::requirement(world);)+
                if $(matches!($req, Requirement::Never) ||)+ false {
                    return;
                }
                // cid wajib (dari term `Column`) untuk alias-check & pencocokan;
                // term `Entity` (`Any`) tak menyumbang.
                let required_cids: Vec<ComponentId> = [$(&$req),+]
                    .into_iter()
                    .filter_map(|r| match r {
                        Requirement::Column(c) => ::core::option::Option::Some(*c),
                        _ => ::core::option::Option::None,
                    })
                    .collect();
                assert_no_alias(world, &required_cids);
                let (with, without) = match resolve_filter::<Fil>(world) {
                    ::core::option::Option::Some(v) => v,
                    ::core::option::Option::None => return,
                };

                let archetypes = world.archetypes();
                for (i, archetype) in archetypes.iter().enumerate().skip(state.scanned) {
                    let present = required_cids
                        .iter()
                        .all(|&c| archetype.column_index(c).is_some());
                    if present && filter_matches(archetype, &with, &without) {
                        state.matched.push(i);
                    }
                }
                state.scanned = archetypes.len();

                for &ai in &state.matched {
                    let archetype = &archetypes[ai];
                    // Kolom teresolusi per-term: `Column` → Some(idx), lainnya None.
                    let ($(mut $var,)+) = (
                        $(<$T as QueryTerm>::iter_shared(
                            archetype,
                            match &$req {
                                Requirement::Column(c) => archetype.column_index(*c),
                                _ => ::core::option::Option::None,
                            },
                        ),)+
                    );
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

impl_query_tuple!(A ra va, B rb vb);
impl_query_tuple!(A ra va, B rb vb, C rc vc);
impl_query_tuple!(A ra va, B rb vb, C rc vc, D rd vd);
impl_query_tuple!(A ra va, B rb vb, C rc vc, D rd vd, E re ve);
impl_query_tuple!(A ra va, B rb vb, C rc vc, D rd vd, E re ve, F rf vf);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Entity, World};

    #[test]
    fn tuple_entity_dan_komponen_menghasilkan_handle() {
        let mut world = World::new();
        let e1 = world.spawn();
        world.insert(e1, Pos(10));
        let e2 = world.spawn();
        world.insert(e2, Pos(20));
        let f = world.spawn();
        world.insert(f, Vel(0)); // tanpa Pos → tak cocok

        let mut got: Vec<(Entity, i32)> = Vec::new();
        <(Entity, &Pos)>::each(&mut world, |(e, p)| got.push((e, p.0)));
        got.sort_by_key(|&(_, v)| v);
        assert_eq!(got, vec![(e1, 10), (e2, 20)]);
    }

    #[test]
    fn entity_tunggal_mengiterasi_semua_ber_komponen() {
        let mut world = World::new();
        let e1 = world.spawn();
        world.insert(e1, Pos(1));
        let e2 = world.spawn();
        world.insert(e2, Vel(2));
        let _bare = world.spawn(); // tanpa komponen → tak ada di archetype

        let mut got: Vec<Entity> = Vec::new();
        <Entity>::each(&mut world, |e| got.push(e));
        got.sort_by_key(|e| e.index());
        assert_eq!(got, vec![e1, e2]);
    }

    #[test]
    fn entity_term_tak_menyumbang_akses() {
        // Entity tidak baca/tulis komponen apa pun.
        let acc = <(Entity, &Pos)>::access();
        assert!(acc.reads.contains(&TypeId::of::<Pos>()));
        assert!(acc.writes.is_empty());
    }

    #[test]
    fn each_cached_menangkap_archetype_baru_inkremental() {
        let mut world = World::new();
        let e1 = world.spawn();
        world.insert(e1, Pos(1));

        let mut state = QueryState::default();
        let mut sum = 0;
        <&Pos>::each_cached::<()>(&world, &mut state, |p| sum += p.0);
        assert_eq!(sum, 1);

        // Entity baru dengan komponen berbeda → archetype {Pos, Vel} baru.
        let e2 = world.spawn();
        world.insert(e2, Pos(10));
        world.insert(e2, Vel(0));

        let mut sum = 0;
        <&Pos>::each_cached::<()>(&world, &mut state, |p| sum += p.0);
        assert_eq!(sum, 11); // archetype baru tertangkap scan inkremental
    }

    #[test]
    fn each_cached_identik_dengan_each_tanpa_cache() {
        let mut world = World::new();
        for i in 0..5 {
            let e = world.spawn();
            world.insert(e, Pos(i));
            if i % 2 == 0 {
                world.insert(e, Vel(i * 10));
            }
        }

        let mut lewat_each = Vec::new();
        <&Pos>::each(&mut world, |p| lewat_each.push(p.0));

        let mut lewat_cache = Vec::new();
        let mut state = QueryState::default();
        <&Pos>::each_cached::<()>(&world, &mut state, |p| lewat_cache.push(p.0));

        assert_eq!(lewat_each, lewat_cache);

        // Memakai ulang QueryState yang sama → hasil identik & konsisten.
        let mut lagi = Vec::new();
        <&Pos>::each_cached::<()>(&world, &mut state, |p| lagi.push(p.0));
        assert_eq!(lewat_each, lagi);
    }

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
