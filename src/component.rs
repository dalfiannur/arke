//! Komponen dan identitasnya (RFC-0002 §2).
//!
//! Setiap tipe `'static + Send` otomatis memenuhi syarat sebagai komponen —
//! tidak ada derive atau registrasi manual yang diperlukan (invarian
//! *ergonomis = cepat*). Identitas internal sebuah tipe komponen diwakili
//! [`ComponentId`], yang diberikan otomatis saat komponen di-*insert* pertama
//! kali ke sebuah `World`.

/// Penanda untuk tipe yang dapat dipakai sebagai komponen.
///
/// Diterapkan secara *blanket* ke semua tipe `'static + Send`. Batas `Send`
/// menyiapkan paralelisme (Milestone M-2) tanpa perlu mengubah API kelak.
pub trait Component: 'static + Send {}

impl<T: 'static + Send> Component for T {}

/// Identitas internal-proses sebuah tipe komponen (RFC-0002 §2).
///
/// Diberikan berurutan saat sebuah tipe komponen pertama kali dipakai. Nilai
/// numeriknya bersifat internal ke satu proses; serialisasi memakai nama tipe
/// yang stabil, bukan `ComponentId`, agar snapshot tetap portabel.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct ComponentId(u32);

impl ComponentId {
    pub(crate) fn raw(self) -> u32 {
        self.0
    }
}

use std::any::TypeId;
use std::collections::HashMap;

use crate::storage::{Column, TypedColumn};

/// Pemetaan tipe komponen ke [`ComponentId`], sekaligus pabrik kolom kosong
/// untuk tiap tipe yang terdaftar.
///
/// Registrasi bersifat otomatis: sebuah tipe terdaftar saat pertama kali
/// dipakai. `ComponentId` diberikan berurutan (0, 1, 2, …) sehingga deterministik
/// terhadap urutan operasi.
#[derive(Default)]
pub(crate) struct ComponentRegistry {
    ids: HashMap<TypeId, ComponentId>,
    /// Konstruktor kolom kosong per komponen, terindeks oleh `ComponentId`.
    constructors: Vec<fn() -> Box<dyn Column>>,
}

impl ComponentRegistry {
    /// Mengembalikan `ComponentId` untuk `T`, mendaftarkannya bila belum ada.
    pub(crate) fn register<T: Component>(&mut self) -> ComponentId {
        let type_id = TypeId::of::<T>();
        if let Some(&id) = self.ids.get(&type_id) {
            return id;
        }
        let id = ComponentId(self.constructors.len() as u32);
        self.ids.insert(type_id, id);
        self.constructors.push(|| Box::new(TypedColumn::<T>::new()));
        id
    }

    /// Mengembalikan `ComponentId` untuk `T` bila sudah terdaftar.
    pub(crate) fn get<T: Component>(&self) -> Option<ComponentId> {
        self.ids.get(&TypeId::of::<T>()).copied()
    }

    /// Membuat kolom kosong untuk komponen `id`.
    pub(crate) fn new_column(&self, id: ComponentId) -> Box<dyn Column> {
        (self.constructors[id.raw() as usize])()
    }
}
