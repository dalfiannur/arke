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
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ComponentId(u32);
