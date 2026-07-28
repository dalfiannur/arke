//! Snapshot keadaan `World` ke format terbuka berversi (RFC-0007).
//!
//! [`Snapshot`] bersifat entity-centric dan dikunci oleh **nama tipe** komponen
//! agar portabel. `schema_version` wajib ada (STD-0001). Round-trip
//! `World` → `Snapshot` → `World` setia secara observasional (STD-0002).

use std::collections::HashMap;

use crate::component::ComponentId;
use crate::entity::Entity;
use crate::serialize::{Serialize, Value};
use crate::storage::{Column, TypedColumn};
use crate::world::World;

/// Versi format snapshot saat ini (STD-0001).
pub(crate) const SCHEMA_VERSION: u32 = 1;

/// Menserialisasi komponen pada `(kolom, baris)` menjadi [`Value`].
type ToValueFn = fn(&dyn Column, usize) -> Value;
/// Mendeserialisasi [`Value`] ke komponen lalu menyisipkannya; `false` bila gagal.
type InserterFn = fn(&mut World, Entity, &Value) -> bool;

/// Vtable serialisasi type-erased untuk satu tipe komponen terdaftar.
struct SerdeInfo {
    type_name: &'static str,
    to_value: ToValueFn,
}

/// Registry tipe komponen yang dapat di-snapshot (opt-in via
/// [`World::register_serializable`]).
#[derive(Default)]
pub(crate) struct SerdeRegistry {
    by_id: HashMap<ComponentId, SerdeInfo>,
    inserter_by_name: HashMap<&'static str, InserterFn>,
}

impl SerdeRegistry {
    /// Mendaftarkan vtable serialisasi untuk `T` (dengan `ComponentId` `cid`).
    pub(crate) fn register<T: Serialize>(&mut self, cid: ComponentId) {
        let type_name = std::any::type_name::<T>();
        self.by_id.insert(
            cid,
            SerdeInfo {
                type_name,
                to_value: |column, row| {
                    let typed = column
                        .as_any()
                        .downcast_ref::<TypedColumn<T>>()
                        .expect("tipe kolom tak cocok saat snapshot");
                    typed.data()[row].to_value()
                },
            },
        );
        self.inserter_by_name
            .insert(type_name, |world, entity, value| {
                match T::from_value(value) {
                    Some(component) => {
                        world.insert(entity, component);
                        true
                    }
                    None => false,
                }
            });
    }

    /// Nama tipe + serializer untuk komponen `cid`, bila terdaftar.
    pub(crate) fn info_for(&self, cid: ComponentId) -> Option<(&'static str, ToValueFn)> {
        self.by_id.get(&cid).map(|i| (i.type_name, i.to_value))
    }

    /// Fungsi penyisip untuk `type_name`, bila terdaftar.
    pub(crate) fn inserter(&self, type_name: &str) -> Option<InserterFn> {
        self.inserter_by_name.get(type_name).copied()
    }
}

/// Snapshot keadaan sebuah `World` (RFC-0007 §4).
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub(crate) schema_version: u32,
    pub(crate) entities: Vec<EntitySnapshot>,
}

/// Snapshot satu entity: handle + komponen-komponennya yang terserialisasi.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EntitySnapshot {
    pub(crate) index: u32,
    pub(crate) generation: u32,
    pub(crate) components: Vec<(String, Value)>,
}

impl EntitySnapshot {
    fn to_value(&self) -> Value {
        Value::Map(vec![
            ("index".to_string(), Value::Int(i64::from(self.index))),
            (
                "generation".to_string(),
                Value::Int(i64::from(self.generation)),
            ),
            (
                "components".to_string(),
                Value::Map(self.components.clone()),
            ),
        ])
    }

    fn from_value(value: &Value) -> Option<Self> {
        let index = u32::try_from(value.get("index")?.as_int()?).ok()?;
        let generation = u32::try_from(value.get("generation")?.as_int()?).ok()?;
        let components = value.get("components")?.as_map()?.to_vec();
        Some(Self {
            index,
            generation,
            components,
        })
    }
}

impl Snapshot {
    fn to_value(&self) -> Value {
        Value::Map(vec![
            (
                "schema_version".to_string(),
                Value::Int(i64::from(self.schema_version)),
            ),
            (
                "entities".to_string(),
                Value::List(self.entities.iter().map(EntitySnapshot::to_value).collect()),
            ),
        ])
    }

    fn from_value(value: &Value) -> Option<Self> {
        // `schema_version` wajib (STD-0001).
        let schema_version = u32::try_from(value.get("schema_version")?.as_int()?).ok()?;
        let entities = value
            .get("entities")?
            .as_list()?
            .iter()
            .map(EntitySnapshot::from_value)
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            schema_version,
            entities,
        })
    }

    /// Versi format snapshot ini.
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Meng-*encode* snapshot menjadi teks JSON (memuat `schema_version`).
    pub fn to_json(&self) -> String {
        self.to_value().to_json()
    }

    /// Mem-*parse* snapshot dari teks JSON; `None` bila tak valid atau
    /// `schema_version` hilang.
    pub fn from_json(json: &str) -> Option<Self> {
        Self::from_value(&Value::from_json(json)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guard stabilitas format snapshot (STD-0001, RFC-0028). Menaikkan
    /// `SCHEMA_VERSION` menggeser format on-disk — perubahan yang **wajib**
    /// disengaja: sertakan jalur migrasi (baca versi lama) + entri CHANGELOG,
    /// lalu perbarui angka ini. Uji ini sengaja gagal agar kenaikan tak-sengaja
    /// tertangkap.
    #[test]
    fn schema_version_terkunci_ke_1() {
        assert_eq!(
            SCHEMA_VERSION, 1,
            "format snapshot berubah — lihat STD-0001/RFC-0028: butuh migrasi + CHANGELOG"
        );
        // Snapshot baru harus melapor versi yang sama (round-trip STD-0002).
        let snap = World::new().snapshot();
        assert_eq!(snap.schema_version(), SCHEMA_VERSION);
    }
}
