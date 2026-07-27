//! Kolom komponen yang kontigu & bertipe, di-*type-erase* di balik trait
//! [`Column`] (RFC-0002 §3).
//!
//! Setiap kolom menyimpan nilai satu tipe komponen dalam `Vec<T>` yang kontigu.
//! Query men-*downcast* kolom **sekali per archetype** menjadi `&[T]`/`&mut [T]`,
//! sehingga iterasi berjalan atas slice bertipe konkret — jalur ergonomis
//! sekaligus jalur cepat (invarian §2). Trait `Column` sendiri hanya dipakai
//! untuk menyimpan kolom-kolom heterogen dalam satu `Vec`.

use std::any::Any;

/// Kolom komponen yang tersimpan dalam sebuah archetype.
///
/// Type-erased agar archetype dapat menyimpan kolom dari tipe-tipe berbeda
/// dalam satu koleksi. Operasi bertipe (baca/tulis nilai) dilakukan lewat
/// [`Column::as_any`]/[`Column::as_any_mut`] lalu `downcast`.
pub(crate) trait Column: Any {
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

/// Kolom konkret untuk komponen bertipe `T`.
pub(crate) struct TypedColumn<T>(pub(crate) Vec<T>);

impl<T> TypedColumn<T> {
    /// Membuat kolom kosong.
    pub(crate) fn new() -> Self {
        Self(Vec::new())
    }
}

impl<T: 'static> Column for TypedColumn<T> {
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
        self.0.push(src.0.swap_remove(row));
    }

    fn swap_remove(&mut self, row: usize) {
        self.0.swap_remove(row);
    }
}
