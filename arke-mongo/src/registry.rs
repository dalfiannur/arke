//! Registry type-erased komponen terdaftar + pembangunan dokumen (RFC-0035 §5).
//!
//! Seluruh modul ini **murni**: ia tahu `World`, tetapi tak menyentuh driver
//! maupun I/O — sehingga dapat diuji tanpa database.

use arke::{Entity, Value, World};
use mongodb::bson::Document;

use crate::bson_map::{bson_to_value, validate_names, value_to_bson};
use crate::{IndexDef, MongoComponent, MongoError};

/// Operasi type-erased untuk satu tipe komponen terdaftar.
struct Registered {
    name: &'static str,
    indexes: &'static [IndexDef],
    /// Nilai komponen `T` milik `entity`, bila ada.
    dump_one: fn(&World, Entity) -> Option<Value>,
    /// Rekonstruksi komponen dari `Value` lalu sisipkan; `false` bila bentuknya
    /// tak cocok.
    apply: fn(&mut World, Entity, &Value) -> bool,
}

fn dump_one_of<T: MongoComponent>(world: &World, entity: Entity) -> Option<Value> {
    world.get::<T>(entity).map(arke::Serialize::to_value)
}

fn apply_of<T: MongoComponent>(world: &mut World, entity: Entity, value: &Value) -> bool {
    match T::from_value(value) {
        Some(component) => {
            world.insert(entity, component);
            true
        }
        None => false,
    }
}

/// Memastikan tiap `IndexDef::field` benar-benar ada sebagai field komponen
/// (RFC-0035 §2, Amandemen 1).
///
/// Salah-ketik nama field tak tertangkap kompilasi, dan MongoDB dengan senang
/// hati mengindeks path yang tak pernah terisi — indeks yang diam-diam kosong.
/// `debug_assert!` menangkapnya di build debug dan di seluruh uji, tanpa biaya
/// di rilis dan tanpa menyeret crate logging.
fn debug_check_indexes(name: &'static str, indexes: &[IndexDef], value: &Value) {
    if cfg!(debug_assertions) {
        let Value::Map(entries) = value else {
            return; // komponen non-map tak punya field untuk di-index
        };
        for idx in indexes {
            debug_assert!(
                entries.iter().any(|(k, _)| k == idx.field),
                "IndexDef pada komponen `{name}` menyebut field `{}` yang tak \
                 ada — indeks akan dibuat atas path yang tak pernah terisi",
                idx.field
            );
        }
    }
}

/// Kumpulan tipe komponen yang dipersist.
#[derive(Default)]
pub struct Registry {
    registered: Vec<Registered>,
}

impl Registry {
    /// Registry kosong.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mendaftarkan tipe komponen `T`.
    ///
    /// # Panics
    ///
    /// Panic bila `T::NAME` sudah dipakai komponen lain. Dua komponen dengan
    /// nama sama adalah bug programmer, bukan kegagalan data — gagal sedini
    /// mungkin (RFC-0035 Am. 1).
    pub fn push<T: MongoComponent>(&mut self) {
        assert!(
            !self.registered.iter().any(|r| r.name == T::NAME),
            "nama komponen `{}` sudah terdaftar — tiap MongoComponent::NAME \
             harus unik",
            T::NAME
        );
        self.registered.push(Registered {
            name: T::NAME,
            indexes: T::INDEXES,
            dump_one: dump_one_of::<T>,
            apply: apply_of::<T>,
        });
    }

    /// Membangun sub-dokumen `cmp` untuk `entity`: satu kunci per komponen
    /// terdaftar yang dimiliki entity itu.
    pub fn cmp_doc(&self, world: &World, entity: Entity) -> Result<Document, MongoError> {
        let mut doc = Document::new();
        for r in &self.registered {
            if let Some(value) = (r.dump_one)(world, entity) {
                debug_check_indexes(r.name, r.indexes, &value);
                validate_names(r.name, &value)?;
                doc.insert(r.name, value_to_bson(&value));
            }
        }
        Ok(doc)
    }

    /// Menyisipkan komponen dari sub-dokumen `cmp` ke `entity`.
    ///
    /// Kunci yang **tak terdaftar diabaikan** — dokumen bisa ditulis service
    /// lain atau versi aplikasi lain, dan kehadirannya bukan kesalahan.
    /// Komponen terdaftar yang bentuknya tak cocok menghasilkan
    /// [`MongoError::Decode`], **bukan** dilewati diam-diam: kehilangan
    /// komponen tanpa suara akan lolos ke `save` berikutnya dan menjadi
    /// kehilangan data permanen (RFC-0035 §6).
    pub fn apply(
        &self,
        world: &mut World,
        entity: Entity,
        pid: crate::Pid,
        cmp: &Document,
    ) -> Result<(), MongoError> {
        for r in &self.registered {
            let Some(bson) = cmp.get(r.name) else {
                continue;
            };
            let value = bson_to_value(bson).ok_or(MongoError::Decode {
                pid,
                component: r.name,
            })?;
            if !(r.apply)(world, entity, &value) {
                return Err(MongoError::Decode {
                    pid,
                    component: r.name,
                });
            }
        }
        Ok(())
    }
}
