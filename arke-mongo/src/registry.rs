//! Registry type-erased komponen terdaftar + pembangunan dokumen (RFC-0035 §5).
//!
//! Seluruh modul ini **murni**: ia tahu `World` dan tipe `mongodb` seperti
//! `IndexModel`/`Document`, tetapi tak melakukan I/O apa pun — sehingga dapat
//! diuji tanpa database.

use arke::{Entity, Value, World};
use mongodb::bson::{Document, doc};
use mongodb::{IndexModel, options::IndexOptions};

use crate::bson_map::{bson_to_value, validate_names, value_to_bson};
use crate::{IndexDef, MongoComponent, MongoError};

/// Operasi type-erased untuk satu tipe komponen terdaftar.
#[derive(Clone)]
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

/// Kumpulan tipe komponen yang dipersist. `Clone` murah (fn-pointer + `&'static`)
/// — dipakai `MongoStore::fork`.
#[derive(Default, Clone)]
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
    ///
    /// Panic juga bila `T::NAME` bukan nama BSON yang sah — kosong,
    /// mengandung `.`, atau berawalan `$` (RFC-0035 Am. 2). `cmp_doc`
    /// memakai `NAME` sebagai kunci literal sementara `update_ops` memakainya
    /// sebagai path bertitik (`cmp.<NAME>`); `NAME` yang mengandung `.` sudah
    /// menyasar tempat berbeda di kedua jalur itu, sehingga setiap update
    /// hilang diam-diam tanpa error di mana pun. Ini pun bug programmer di
    /// sebuah `const`, tertangkap di saat registrasi, seawal mungkin.
    pub fn push<T: MongoComponent>(&mut self) {
        assert!(
            !T::NAME.is_empty() && !T::NAME.contains('.') && !T::NAME.starts_with('$'),
            "MongoComponent::NAME `{}` bukan nama BSON yang sah — tak boleh \
             kosong, mengandung `.`, atau berawalan `$` (lihat RFC-0035 Am. 2)",
            T::NAME
        );
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
    /// # Precondition
    ///
    /// `entity` harus hidup di `world` (`world.contains(entity)`). `World::
    /// insert` mengabaikan sisipan ke entity mati tanpa error, sehingga tanpa
    /// pemeriksaan ini `apply` akan mengembalikan `Ok(())` padahal tak
    /// menulis apa pun — sukses palsu. Ini bug pemanggil, bukan kegagalan
    /// data, jadi ditangkap lewat `debug_assert!`, bukan varian error.
    ///
    /// Kunci yang **tak terdaftar diabaikan** — dokumen bisa ditulis service
    /// lain atau versi aplikasi lain, dan kehadirannya bukan kesalahan.
    /// Komponen terdaftar yang bentuknya tak cocok menghasilkan
    /// [`MongoError::Decode`], **bukan** dilewati diam-diam: kehilangan
    /// komponen tanpa suara akan lolos ke `save` berikutnya dan menjadi
    /// kehilangan data permanen (RFC-0035 §6).
    ///
    /// # Error parsial
    ///
    /// Bila sebuah komponen gagal di-decode, komponen-komponen terdaftar
    /// yang diproses **sebelumnya** dalam pemanggilan ini sudah tersisip ke
    /// `entity` — `apply` tidak transaksional atas satu entity. Saat `Err`
    /// dikembalikan, `entity` bisa dalam keadaan separuh terisi; pemanggil
    /// tidak boleh menganggapnya utuh dan sebaiknya membuangnya (pola yang
    /// akan dipakai `fetch`/`load` pada task selanjutnya).
    pub fn apply(
        &self,
        world: &mut World,
        entity: Entity,
        pid: crate::Pid,
        cmp: &Document,
    ) -> Result<(), MongoError> {
        debug_assert!(
            world.contains(entity),
            "Registry::apply dipanggil dengan entity yang tak hidup di world \
             — entity harus hidup sebelum apply dipanggil"
        );
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

    /// Membangun dokumen update untuk `entity`: `$set` per sub-field komponen
    /// yang dimiliki, `$unset` untuk komponen **terdaftar** yang hilang, dan
    /// `$inc` pada `version`.
    ///
    /// `$set` sengaja menyasar `cmp.<nama>` alih-alih mengganti `cmp` utuh —
    /// mengganti utuh akan menghapus diam-diam komponen yang ditulis service
    /// lain atau versi aplikasi lain (RFC-0035 §5). Karena itu `$unset` pun
    /// hanya menyasar komponen terdaftar.
    pub fn update_ops(&self, world: &World, entity: Entity) -> Result<Document, MongoError> {
        let mut set = Document::new();
        let mut unset = Document::new();
        for r in &self.registered {
            let path = format!("cmp.{}", r.name);
            match (r.dump_one)(world, entity) {
                Some(value) => {
                    debug_check_indexes(r.name, r.indexes, &value);
                    validate_names(r.name, &value)?;
                    set.insert(path, value_to_bson(&value));
                }
                None => {
                    unset.insert(path, "");
                }
            }
        }
        let mut ops = doc! { "$inc": { "version": 1i64 } };
        if !set.is_empty() {
            ops.insert("$set", set);
        }
        if !unset.is_empty() {
            ops.insert("$unset", unset);
        }
        Ok(ops)
    }

    /// Spesifikasi indeks untuk seluruh komponen terdaftar, atas path bersarang
    /// `cmp.<nama>.<field>`.
    pub fn index_models(&self) -> Vec<IndexModel> {
        let mut models = Vec::new();
        for r in &self.registered {
            for idx in r.indexes {
                let key = format!("cmp.{}.{}", r.name, idx.field);
                let mut opts = IndexOptions::default();
                if idx.unique {
                    opts.unique = Some(true);
                }
                models.push(
                    IndexModel::builder()
                        .keys(doc! { key: idx.dir.as_i32() })
                        .options(opts)
                        .build(),
                );
            }
        }
        models
    }
}
