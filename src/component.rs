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
use std::hash::{BuildHasherDefault, Hasher};

use crate::storage::{Column, TypedColumn};

/// Hasher cepat untuk kunci `TypeId` (algoritma FxHash, dipakai rustc).
///
/// `TypeId` sudah berupa hash 128-bit berkualitas; SipHash default boros untuk
/// kunci sekecil ini. Deterministik (tanpa seed acak) — hanya untuk *lookup*
/// titik (register/get), tak memengaruhi urutan (STD-0005). Menghemat sebagian
/// besar biaya resolusi komponen di jalur panas spawn/query.
#[derive(Default)]
pub(crate) struct TypeIdHasher(u64);

const FX_K: u64 = 0x51_7c_c1_b7_27_22_0a_95;

impl Hasher for TypeIdHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write_u64(&mut self, i: u64) {
        self.0 = (self.0.rotate_left(5) ^ i).wrapping_mul(FX_K);
    }
    fn write_u128(&mut self, i: u128) {
        self.write_u64(i as u64);
        self.write_u64((i >> 64) as u64);
    }
    fn write(&mut self, bytes: &[u8]) {
        // Fallback (jarang; `TypeId` memakai `write_u64`/`write_u128`).
        for chunk in bytes.chunks(8) {
            let mut buf = [0u8; 8];
            buf[..chunk.len()].copy_from_slice(chunk);
            self.write_u64(u64::from_le_bytes(buf));
        }
    }
}

/// `HashMap` berkunci `TypeId` dengan hasher cepat.
pub(crate) type TypeIdMap<V> = HashMap<TypeId, V, BuildHasherDefault<TypeIdHasher>>;

/// Pemetaan tipe komponen ke [`ComponentId`], sekaligus pabrik kolom kosong
/// untuk tiap tipe yang terdaftar.
///
/// Registrasi bersifat otomatis: sebuah tipe terdaftar saat pertama kali
/// dipakai. `ComponentId` diberikan berurutan (0, 1, 2, …) sehingga deterministik
/// terhadap urutan operasi.
#[derive(Default)]
pub(crate) struct ComponentRegistry {
    ids: TypeIdMap<ComponentId>,
    /// Konstruktor kolom kosong per komponen, terindeks oleh `ComponentId`.
    constructors: Vec<fn() -> Box<dyn Column>>,
    /// Nama tipe per komponen, terindeks oleh `ComponentId` (untuk error berkonteks).
    names: Vec<&'static str>,
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
        self.names.push(std::any::type_name::<T>());
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

    /// Nama tipe komponen `id` (untuk pesan error berkonteks).
    pub(crate) fn name(&self, id: ComponentId) -> &'static str {
        self.names[id.raw() as usize]
    }
}
