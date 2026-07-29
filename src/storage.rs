//! Kolom komponen yang kontigu & bertipe, di-*type-erase* di balik trait
//! [`Column`] (RFC-0002 §3).
//!
//! Data kolom disimpan di balik [`UnsafeCell`] (RFC-0015) agar `&mut [T]`
//! interior dapat dibentuk dari `&World` bersama — prasyarat eksekutor paralel
//! tingkat-sistem. **Seluruh `unsafe` proyek dikurung di modul ini.**
//!
//! Kontrak keamanan (ditegakkan pemanggil crate):
//! - Jalur ber-`&mut self` ([`TypedColumn::data_mut`]) bersifat **aman** —
//!   `UnsafeCell::get_mut` menjamin eksklusivitas.
//! - Jalur baca ber-`&self` ([`TypedColumn::data`]) sound karena dalam eksekusi
//!   **serial** hanya satu akses aktif, dan dalam **paralel** penjadwal menjamin
//!   akses disjoint (tak ada penulis komponen yang sama).

#![allow(unsafe_code)]

use std::any::Any;
use std::cell::UnsafeCell;

/// Kolom komponen yang tersimpan dalam sebuah archetype.
///
/// Type-erased agar archetype dapat menyimpan kolom dari tipe-tipe berbeda
/// dalam satu koleksi.
pub(crate) trait Column: Any + Send {
    /// Referensi `Any` untuk downcast ke `TypedColumn<T>` konkret.
    fn as_any(&self) -> &dyn Any;
    /// Referensi `Any` mutabel untuk downcast ke `TypedColumn<T>` konkret.
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// Memindahkan nilai pada `row` dari `src` ke akhir kolom ini.
    ///
    /// `src` harus kolom bertipe sama; nilainya diambil via `swap_remove`.
    fn push_from(&mut self, src: &mut dyn Column, row: usize);
    /// Membuang nilai pada `row` via `swap_remove`.
    fn swap_remove(&mut self, row: usize);
}

/// Kolom konkret untuk komponen bertipe `T`, data di balik `UnsafeCell`.
pub(crate) struct TypedColumn<T>(UnsafeCell<Vec<T>>);

impl<T> TypedColumn<T> {
    /// Membuat kolom kosong.
    pub(crate) fn new() -> Self {
        Self(UnsafeCell::new(Vec::new()))
    }

    /// Akses **mutabel** ke data (aman — butuh `&mut self`).
    pub(crate) fn data_mut(&mut self) -> &mut Vec<T> {
        self.0.get_mut()
    }

    /// Akses **baca** ke data lewat `&self`.
    ///
    /// Aman dipakai crate selama tak ada `&mut` konkuren ke data yang sama
    /// (dijamin oleh eksekusi serial atau disjoint-nya penjadwal, RFC-0015).
    pub(crate) fn data(&self) -> &Vec<T> {
        // SAFETY: lihat kontrak modul & doc di atas — tak ada `&mut` beralias.
        unsafe { &*self.0.get() }
    }

    /// Akses **mutabel** ke data lewat `&self` (jalur paralel, RFC-0016).
    ///
    /// # Safety
    ///
    /// Pemanggil menjamin **tak ada** akses lain (baca maupun tulis) ke data
    /// kolom ini yang aktif bersamaan — dijamin oleh cek-alias query (kolom
    /// distinct) dan disjoint-nya stage penjadwal.
    #[allow(clippy::mut_from_ref)] // interior mutability via UnsafeCell (RFC-0016)
    pub(crate) unsafe fn data_mut_shared(&self) -> &mut Vec<T> {
        // SAFETY: eksklusivitas dijamin pemanggil (kontrak di atas).
        unsafe { &mut *self.0.get() }
    }
}

impl<T: 'static + Send> Column for TypedColumn<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn push_from(&mut self, src: &mut dyn Column, row: usize) {
        let src = src
            .as_any_mut()
            .downcast_mut::<TypedColumn<T>>()
            .expect("push_from: kolom sumber bertipe berbeda");
        let value = src.data_mut().swap_remove(row);
        self.data_mut().push(value);
    }

    fn swap_remove(&mut self, row: usize) {
        self.data_mut().swap_remove(row);
    }
}
