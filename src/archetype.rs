//! Tabel per-kombinasi-komponen (RFC-0002 §3.5).
//!
//! Semua entity dengan himpunan komponen yang persis sama berbagi satu
//! [`Archetype`]. Komponen disimpan sebagai kolom kontigu yang sejajar dengan
//! `component_ids` (terurut), dan `entities[row]` memetakan setiap baris kembali
//! ke entity pemiliknya.

use crate::component::ComponentId;
use crate::entity::Entity;
use crate::storage::{Column, TypedColumn};

/// Satu tabel penyimpanan untuk satu himpunan komponen unik.
pub(crate) struct Archetype {
    /// Himpunan komponen archetype ini, **terurut menaik**.
    component_ids: Vec<ComponentId>,
    /// Kolom komponen, sejajar indeks dengan `component_ids`.
    columns: Vec<Box<dyn Column>>,
    /// `entities[row]` = entity pemilik baris tersebut.
    entities: Vec<Entity>,
}

impl Archetype {
    /// Membuat archetype kosong dengan himpunan komponen `component_ids`
    /// (harus terurut) dan `columns` yang sejajar dengannya.
    pub(crate) fn new(component_ids: Vec<ComponentId>, columns: Vec<Box<dyn Column>>) -> Self {
        debug_assert!(component_ids.windows(2).all(|w| w[0] < w[1]));
        debug_assert_eq!(component_ids.len(), columns.len());
        Self {
            component_ids,
            columns,
            entities: Vec::new(),
        }
    }

    /// Himpunan komponen archetype ini (terurut).
    pub(crate) fn component_ids(&self) -> &[ComponentId] {
        &self.component_ids
    }

    /// Posisi kolom untuk komponen `id`, bila archetype ini memilikinya.
    pub(crate) fn column_index(&self, id: ComponentId) -> Option<usize> {
        self.component_ids.iter().position(|&c| c == id)
    }

    /// Referensi kolom pada posisi `col`.
    pub(crate) fn column(&self, col: usize) -> &dyn Column {
        &*self.columns[col]
    }

    /// Menambahkan baris untuk `entity` dan mengembalikan indeks barisnya.
    ///
    /// Kolom-kolom diisi terpisah oleh pemanggil (yang memegang nilai bertipe).
    pub(crate) fn push_entity(&mut self, entity: Entity) -> usize {
        let row = self.entities.len();
        self.entities.push(entity);
        row
    }

    /// Mendorong `value` ke kolom pada posisi `col`.
    ///
    /// Tipe `T` harus cocok dengan tipe kolom tersebut.
    pub(crate) fn push_component<T: 'static>(&mut self, col: usize, value: T) {
        self.columns[col]
            .as_any_mut()
            .downcast_mut::<TypedColumn<T>>()
            .expect("push_component: tipe kolom tak cocok")
            .0
            .push(value);
    }

    /// Memindahkan seluruh komponen entity pada `row` ke `dst` (yang memuat
    /// superset komponen archetype ini), lalu menghapus baris dari archetype ini
    /// via `swap_remove`.
    ///
    /// Mengembalikan entity yang kini menempati `row` akibat `swap_remove`
    /// (baris terakhir digeser ke `row`), bila ada — pemanggil wajib memperbarui
    /// lokasinya.
    pub(crate) fn move_row_to(&mut self, row: usize, dst: &mut Archetype) -> Option<Entity> {
        for i in 0..self.component_ids.len() {
            let id = self.component_ids[i];
            let dst_col = dst
                .column_index(id)
                .expect("dst harus superset dari archetype sumber");
            dst.columns[dst_col].push_from(&mut *self.columns[i], row);
        }
        self.entities.swap_remove(row);
        self.entities.get(row).copied()
    }

    /// Memindahkan baris `row` ke `dst` (subset komponen archetype ini tanpa
    /// `removed`), mengekstrak nilai komponen `removed` bertipe `T`, lalu
    /// menghapus baris via `swap_remove`.
    ///
    /// Mengembalikan nilai yang diekstrak dan entity yang tergeser ke `row`.
    pub(crate) fn move_row_to_removing<T: 'static>(
        &mut self,
        row: usize,
        dst: &mut Archetype,
        removed: ComponentId,
    ) -> (T, Option<Entity>) {
        let mut taken = None;
        for i in 0..self.component_ids.len() {
            let id = self.component_ids[i];
            if id == removed {
                taken = Some(
                    self.columns[i]
                        .as_any_mut()
                        .downcast_mut::<TypedColumn<T>>()
                        .expect("tipe kolom tak cocok")
                        .0
                        .swap_remove(row),
                );
            } else {
                let dst_col = dst
                    .column_index(id)
                    .expect("dst harus subset tanpa `removed`");
                dst.columns[dst_col].push_from(&mut *self.columns[i], row);
            }
        }
        self.entities.swap_remove(row);
        (
            taken.expect("komponen `removed` ada di archetype"),
            self.entities.get(row).copied(),
        )
    }

    /// Menghapus baris `row` dari archetype berkolom-tunggal, mengembalikan
    /// nilai komponen tunggalnya (bertipe `T`) dan entity yang tergeser.
    pub(crate) fn take_single_row<T: 'static>(&mut self, row: usize) -> (T, Option<Entity>) {
        debug_assert_eq!(self.columns.len(), 1);
        let value = self.columns[0]
            .as_any_mut()
            .downcast_mut::<TypedColumn<T>>()
            .expect("tipe kolom tak cocok")
            .0
            .swap_remove(row);
        self.entities.swap_remove(row);
        (value, self.entities.get(row).copied())
    }
}
