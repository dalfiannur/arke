//! Bundle komponen (RFC-0022): menyisipkan beberapa komponen sekaligus dalam
//! **satu** perpindahan archetype.
//!
//! [`Bundle`] diimplementasikan untuk **tuple arity 1–8** dari [`Component`]
//! yang distinct; dipakai [`World::spawn_bundle`](crate::World::spawn_bundle)
//! dan [`World::insert_bundle`](crate::World::insert_bundle). Bare `T` bukan
//! bundle (pakai `insert`) — tuple sudah menjadi `Component` (blanket impl), jadi
//! `Bundle` di-impl untuk *bentuk tuple* saja, menghindari tumpang-tindih.

use crate::archetype::Archetype;
use crate::component::{Component, ComponentId, ComponentRegistry};

/// Sekumpulan komponen yang disisipkan bersama (RFC-0022).
///
/// Detail implementasi; pengguna tak perlu menyentuhnya langsung (bound untuk
/// `spawn_bundle`/`insert_bundle`).
#[allow(private_interfaces)]
pub trait Bundle: crate::sealed::BundleSealed {
    /// Registrasikan tipe komponen bundle; kembalikan id-nya (urut tuple).
    #[doc(hidden)]
    fn ids(registry: &mut ComponentRegistry) -> Vec<ComponentId>;
    /// Dorong tiap komponen ke kolomnya di `archetype`. `cids` = id komponen
    /// bundle (urut tuple, dari [`Self::ids`]) — menghindari lookup registry ulang.
    #[doc(hidden)]
    fn push(self, archetype: &mut Archetype, cids: &[ComponentId]);
}

macro_rules! impl_bundle {
    ($($T:ident $idx:tt),+) => {
        impl<$($T: Component),+> crate::sealed::BundleSealed for ($($T,)+) {}
        #[allow(private_interfaces)]
        impl<$($T: Component),+> Bundle for ($($T,)+) {
            fn ids(registry: &mut ComponentRegistry) -> Vec<ComponentId> {
                ::std::vec![$(registry.register::<$T>()),+]
            }
            fn push(self, archetype: &mut Archetype, cids: &[ComponentId]) {
                $(
                    let col = archetype
                        .column_index(cids[$idx])
                        .expect("archetype tujuan memuat kolom bundle");
                    archetype.push_component(col, self.$idx);
                )+
            }
        }
    };
}

impl_bundle!(A 0);
impl_bundle!(A 0, B 1);
impl_bundle!(A 0, B 1, C 2);
impl_bundle!(A 0, B 1, C 2, D 3);
impl_bundle!(A 0, B 1, C 2, D 3, E 4);
impl_bundle!(A 0, B 1, C 2, D 3, E 4, F 5);
impl_bundle!(A 0, B 1, C 2, D 3, E 4, F 5, G 6);
impl_bundle!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7);
