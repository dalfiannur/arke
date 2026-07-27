//! Otoritas atas seluruh entity, komponen, dan resource (RFC-0002 §3.4).
//!
//! [`World`] adalah satu-satunya sumber kebenaran keadaan. Segala mutasi
//! bermuara padanya, sehingga sebuah snapshot atas `World` cukup untuk merekam
//! seluruh keadaan (invarian *kepemilikan & portabilitas data*).
//!
//! Isi struktur ini — tabel slot entity, registry komponen, dan penyimpanan
//! archetype — ditambahkan secara test-first selama Milestone M-1.

use std::any::{Any, TypeId};
use std::collections::HashMap;

use crate::archetype::Archetype;
use crate::component::{Component, ComponentId, ComponentRegistry};
use crate::entity::Entity;
use crate::error::EcsError;
use crate::serialize::Serialize;
use crate::snapshot::{EntitySnapshot, SCHEMA_VERSION, SerdeRegistry, Snapshot};
use crate::storage::TypedColumn;

/// Lokasi sebuah entity di dalam penyimpanan archetype.
#[derive(Clone, Copy)]
struct Location {
    archetype: usize,
    row: usize,
}

/// Metadata per-slot entity.
struct EntityMeta {
    generation: u32,
    alive: bool,
    /// Lokasi komponen entity ini, atau `None` bila entity tak punya komponen.
    location: Option<Location>,
}

/// Wadah pemilik semua entity, komponen, dan resource.
///
/// Kemampuan query dikembangkan pada Milestone M-1 (lihat `docs/MILESTONE_1.md`).
#[derive(Default)]
pub struct World {
    entities: Vec<EntityMeta>,
    /// Indeks slot bebas, dikelola sebagai tumpukan LIFO agar alokasi
    /// deterministik (STD-0005).
    free: Vec<u32>,
    registry: ComponentRegistry,
    archetypes: Vec<Archetype>,
    serde: SerdeRegistry,
    /// State global singleton-per-tipe (RFC-0010).
    resources: HashMap<TypeId, Box<dyn Any + Send>>,
}

impl World {
    /// Membuat `World` kosong.
    pub fn new() -> Self {
        Self::default()
    }

    /// Membuat sebuah entity baru tanpa komponen dan mengembalikan handle-nya.
    ///
    /// Bila ada slot bekas despawn, slot itu dipakai ulang dengan generasi yang
    /// dinaikkan sehingga handle lama ke slot tersebut menjadi basi (STD-0007).
    pub fn spawn(&mut self) -> Entity {
        if let Some(index) = self.free.pop() {
            let meta = &mut self.entities[index as usize];
            meta.alive = true;
            meta.location = None;
            Entity::new(index, meta.generation)
        } else {
            let index = self.entities.len() as u32;
            self.entities.push(EntityMeta {
                generation: 0,
                alive: true,
                location: None,
            });
            Entity::new(index, 0)
        }
    }

    /// Menghapus `entity`. Setelah ini `entity` tidak lagi hidup, dan slot-nya
    /// dapat dipakai ulang oleh `spawn` berikutnya.
    pub fn despawn(&mut self, entity: Entity) {
        if !self.contains(entity) {
            return;
        }
        let index = entity.index();
        // Bersihkan baris komponen entity dari archetype-nya, bila ada.
        if let Some(loc) = self.entities[index as usize].location {
            let moved = self.archetypes[loc.archetype].swap_remove_row(loc.row);
            self.fix_swapped(moved, loc.archetype, loc.row);
        }
        let meta = &mut self.entities[index as usize];
        meta.alive = false;
        meta.generation += 1;
        meta.location = None;
        self.free.push(index);
    }

    /// Mengembalikan `true` bila `entity` masih hidup di `World` ini.
    pub fn contains(&self, entity: Entity) -> bool {
        self.entities
            .get(entity.index() as usize)
            .is_some_and(|meta| meta.alive && meta.generation == entity.generation())
    }

    /// Menempelkan komponen `component` ke `entity`.
    ///
    /// Bila `entity` tidak hidup, operasi diabaikan. Tipe komponen didaftarkan
    /// otomatis pada pemakaian pertama.
    pub fn insert<T: Component>(&mut self, entity: Entity, component: T) {
        if !self.contains(entity) {
            return;
        }
        let cid = self.registry.register::<T>();
        let index = entity.index() as usize;

        let Some(loc) = self.entities[index].location else {
            // Entity belum punya komponen → archetype tujuan = {cid}.
            let arch_idx = self.find_or_create_archetype(&[cid]);
            let archetype = &mut self.archetypes[arch_idx];
            let row = archetype.push_entity(entity);
            let col = archetype
                .column_index(cid)
                .expect("archetype baru dibuat memuat kolom cid");
            archetype.push_component(col, component);
            self.entities[index].location = Some(Location {
                archetype: arch_idx,
                row,
            });
            return;
        };

        // Entity sudah punya komponen: pindahkan ke archetype {komponen lama ∪ cid}.
        let mut ids = self.archetypes[loc.archetype].component_ids().to_vec();
        ids.push(cid);
        ids.sort_unstable();
        let dst_idx = self.find_or_create_archetype(&ids);

        let (moved, dst_row) = {
            let (src, dst) = split_two(&mut self.archetypes, loc.archetype, dst_idx);
            let dst_row = dst.push_entity(entity);
            let moved = src.move_row_to(loc.row, dst);
            let col = dst
                .column_index(cid)
                .expect("archetype tujuan memuat kolom cid");
            dst.push_component(col, component);
            (moved, dst_row)
        };

        self.entities[index].location = Some(Location {
            archetype: dst_idx,
            row: dst_row,
        });
        self.fix_swapped(moved, loc.archetype, loc.row);
    }

    /// Menghapus komponen `T` dari `entity`, mengembalikan nilainya bila ada.
    ///
    /// Mengembalikan `None` bila `entity` tidak hidup atau tidak memiliki `T`.
    pub fn remove<T: Component>(&mut self, entity: Entity) -> Option<T> {
        if !self.contains(entity) {
            return None;
        }
        let index = entity.index() as usize;
        let loc = self.entities[index].location?;
        let cid = self.registry.get::<T>()?;
        let src_ids = self.archetypes[loc.archetype].component_ids().to_vec();
        if !src_ids.contains(&cid) {
            return None;
        }
        let dst_ids: Vec<_> = src_ids.into_iter().filter(|&c| c != cid).collect();

        // Kasus: komponen terakhir → entity menjadi tanpa komponen.
        if dst_ids.is_empty() {
            let src = &mut self.archetypes[loc.archetype];
            let (value, moved) = src.take_single_row::<T>(loc.row);
            self.entities[index].location = None;
            self.fix_swapped(moved, loc.archetype, loc.row);
            return Some(value);
        }

        // Kasus umum: pindah ke archetype subset, ekstrak komponen yang dihapus.
        let dst_idx = self.find_or_create_archetype(&dst_ids);
        let (value, moved, dst_row) = {
            let (src, dst) = split_two(&mut self.archetypes, loc.archetype, dst_idx);
            let dst_row = dst.push_entity(entity);
            let (value, moved) = src.move_row_to_removing::<T>(loc.row, dst, cid);
            (value, moved, dst_row)
        };
        self.entities[index].location = Some(Location {
            archetype: dst_idx,
            row: dst_row,
        });
        self.fix_swapped(moved, loc.archetype, loc.row);
        Some(value)
    }

    /// Mengiterasi referensi ke komponen `T` pada setiap entity yang memilikinya.
    ///
    /// Urutan iterasi deterministik: archetype dalam urutan pembuatan, baris
    /// `0..len` (STD-0005). Downcast kolom dilakukan sekali per archetype.
    pub fn query<T: Component>(&self) -> impl Iterator<Item = &T> {
        let cid = self.registry.get::<T>();
        self.archetypes
            .iter()
            .filter_map(move |arch| {
                let col = arch.column_index(cid?)?;
                Some(arch.slice::<T>(col).iter())
            })
            .flatten()
    }

    /// Mengiterasi referensi mutabel ke komponen `T` pada setiap entity yang
    /// memilikinya. Urutan deterministik seperti [`World::query`].
    pub fn query_mut<T: Component>(&mut self) -> impl Iterator<Item = &mut T> {
        let cid = self.registry.get::<T>();
        self.archetypes
            .iter_mut()
            .filter_map(move |arch| {
                let col = arch.column_index(cid?)?;
                Some(arch.slice_mut::<T>(col).iter_mut())
            })
            .flatten()
    }

    /// Mengiterasi pasangan `(&A, &mut B)` pada setiap entity yang memiliki
    /// **kedua** komponen. Membaca `A`, memodifikasi `B`.
    ///
    /// # Panics
    ///
    /// Panik bila `A` dan `B` adalah tipe komponen yang sama — itu berarti alias
    /// `&mut` ke komponen yang sama, yang ditolak (invarian *paralelisme aman*).
    ///
    /// Kedua kolom dipinjam disjoint via `split_at_mut` — tanpa `unsafe`.
    pub fn query_pair<A: Component, B: Component>(&mut self) -> impl Iterator<Item = (&A, &mut B)> {
        let ca = self.registry.get::<A>();
        let cb = self.registry.get::<B>();
        if let (Some(a), Some(b)) = (ca, cb)
            && a == b
        {
            panic!(
                "{}",
                EcsError::QueryConflict {
                    component: std::any::type_name::<A>(),
                }
            );
        }
        self.archetypes
            .iter_mut()
            .filter_map(move |arch| {
                let ia = arch.column_index(ca?)?;
                let ib = arch.column_index(cb?)?;
                let (col_a, col_b) = arch.columns_two_mut(ia, ib);
                let a_slice: &[A] = col_a
                    .as_any()
                    .downcast_ref::<TypedColumn<A>>()
                    .expect("tipe kolom A tak cocok")
                    .data();
                let b_slice: &mut [B] = col_b
                    .as_any_mut()
                    .downcast_mut::<TypedColumn<B>>()
                    .expect("tipe kolom B tak cocok")
                    .data_mut();
                Some(a_slice.iter().zip(b_slice.iter_mut()))
            })
            .flatten()
    }

    /// Menerapkan `f` pada komponen `T` setiap entity pemiliknya, **secara
    /// paralel** (RFC-0004).
    ///
    /// Baris tiap archetype dibagi menjadi chunk yang diproses di
    /// `std::thread::scope` — aman via `chunks_mut`, tanpa `unsafe`. `f` harus
    /// memproses tiap elemen secara independen; dengan syarat itu hasilnya
    /// deterministik dan setara eksekusi serial (STD-0006).
    pub fn par_for_each<T: Component>(&mut self, f: impl Fn(&mut T) + Sync) {
        let Some(cid) = self.registry.get::<T>() else {
            return;
        };
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
        let f = &f;
        std::thread::scope(|scope| {
            for archetype in self.archetypes.iter_mut() {
                let Some(col) = archetype.column_index(cid) else {
                    continue;
                };
                let slice = archetype.slice_mut::<T>(col);
                if slice.is_empty() {
                    continue;
                }
                let chunk_size = slice.len().div_ceil(threads);
                for chunk in slice.chunks_mut(chunk_size) {
                    scope.spawn(move || {
                        for item in chunk {
                            f(item);
                        }
                    });
                }
            }
        });
    }

    /// Mengiterasi pasangan `(&A, &B)` pada setiap entity yang memiliki **kedua**
    /// komponen. Keduanya dibaca (dua peminjaman bersama, tanpa `unsafe`).
    pub fn query_pair_ref<A: Component, B: Component>(&self) -> impl Iterator<Item = (&A, &B)> {
        let ca = self.registry.get::<A>();
        let cb = self.registry.get::<B>();
        self.archetypes
            .iter()
            .filter_map(move |arch| {
                let ia = arch.column_index(ca?)?;
                let ib = arch.column_index(cb?)?;
                Some(arch.slice::<A>(ia).iter().zip(arch.slice::<B>(ib).iter()))
            })
            .flatten()
    }

    /// Mengembalikan referensi ke komponen `T` milik `entity`, bila ada.
    pub fn get<T: Component>(&self, entity: Entity) -> Option<&T> {
        if !self.contains(entity) {
            return None;
        }
        let location = self.entities[entity.index() as usize].location?;
        let cid = self.registry.get::<T>()?;
        let archetype = &self.archetypes[location.archetype];
        let col = archetype.column_index(cid)?;
        archetype
            .column(col)
            .as_any()
            .downcast_ref::<TypedColumn<T>>()?
            .data()
            .get(location.row)
    }

    /// Menyisipkan (atau menimpa) resource global bertipe `T` (RFC-0010).
    pub fn insert_resource<T: 'static + Send>(&mut self, resource: T) {
        self.resources.insert(TypeId::of::<T>(), Box::new(resource));
    }

    /// Referensi ke resource `T`, bila ada.
    pub fn resource<T: 'static + Send>(&self) -> Option<&T> {
        self.resources
            .get(&TypeId::of::<T>())
            .and_then(|b| b.downcast_ref::<T>())
    }

    /// Referensi mutabel ke resource `T`, bila ada.
    pub fn resource_mut<T: 'static + Send>(&mut self) -> Option<&mut T> {
        self.resources
            .get_mut(&TypeId::of::<T>())
            .and_then(|b| b.downcast_mut::<T>())
    }

    /// Menghapus dan mengembalikan resource `T`, bila ada.
    pub fn remove_resource<T: 'static + Send>(&mut self) -> Option<T> {
        self.resources
            .remove(&TypeId::of::<T>())
            .and_then(|b| b.downcast::<T>().ok())
            .map(|b| *b)
    }

    /// Apakah resource `T` ada.
    pub fn contains_resource<T: 'static + Send>(&self) -> bool {
        self.resources.contains_key(&TypeId::of::<T>())
    }

    /// Mendaftarkan komponen `T` agar ikut dalam snapshot (opt-in, RFC-0007).
    pub fn register_serializable<T: Serialize>(&mut self) {
        let cid = self.registry.register::<T>();
        self.serde.register::<T>(cid);
    }

    /// Menghasilkan [`Snapshot`] keadaan `World` saat ini.
    ///
    /// Hanya entity hidup dan komponen yang telah didaftarkan lewat
    /// [`World::register_serializable`] yang disertakan.
    pub fn snapshot(&self) -> Snapshot {
        let mut entities = Vec::new();
        for (index, meta) in self.entities.iter().enumerate() {
            if !meta.alive {
                continue;
            }
            let mut components = Vec::new();
            if let Some(loc) = meta.location {
                let archetype = &self.archetypes[loc.archetype];
                for (col, &cid) in archetype.component_ids().iter().enumerate() {
                    if let Some((type_name, to_value)) = self.serde.info_for(cid) {
                        let value = to_value(archetype.column(col), loc.row);
                        components.push((type_name.to_string(), value));
                    }
                }
            }
            entities.push(EntitySnapshot {
                index: index as u32,
                generation: meta.generation,
                components,
            });
        }
        Snapshot {
            schema_version: SCHEMA_VERSION,
            entities,
        }
    }

    /// Seperti [`World::snapshot`], tetapi **menolak** bila ada entity hidup
    /// dengan komponen yang belum di-`register_serializable` — mengembalikan
    /// [`EcsError::ComponentNotRegistered`] yang menyebut namanya (STD-0008),
    /// mencegah kehilangan data diam.
    pub fn try_snapshot(&self) -> Result<Snapshot, EcsError> {
        for meta in &self.entities {
            if !meta.alive {
                continue;
            }
            if let Some(loc) = meta.location {
                for &cid in self.archetypes[loc.archetype].component_ids() {
                    if self.serde.info_for(cid).is_none() {
                        return Err(EcsError::ComponentNotRegistered {
                            component: self.registry.name(cid),
                        });
                    }
                }
            }
        }
        Ok(self.snapshot())
    }

    /// Memuat `snapshot` ke `World` ini, merekonstruksi entity (dengan handle
    /// yang sama) dan komponennya. Tipe komponen harus sudah didaftarkan lewat
    /// [`World::register_serializable`].
    pub fn load_snapshot(&mut self, snapshot: &Snapshot) {
        for entity_snap in &snapshot.entities {
            let entity = self.allocate_at(entity_snap.index, entity_snap.generation);
            for (type_name, value) in &entity_snap.components {
                if let Some(inserter) = self.serde.inserter(type_name) {
                    inserter(self, entity, value);
                }
            }
        }
    }

    /// Membuat entity pada slot `index` dengan `generation` tertentu (untuk
    /// restore). Slot antara diisi placeholder mati.
    fn allocate_at(&mut self, index: u32, generation: u32) -> Entity {
        let idx = index as usize;
        while self.entities.len() <= idx {
            self.entities.push(EntityMeta {
                generation: 0,
                alive: false,
                location: None,
            });
        }
        self.entities[idx] = EntityMeta {
            generation,
            alive: true,
            location: None,
        };
        Entity::new(index, generation)
    }

    /// `ComponentId` untuk `T` bila terdaftar (dipakai query generik RFC-0013).
    pub(crate) fn component_id<T: Component>(&self) -> Option<ComponentId> {
        self.registry.get::<T>()
    }

    /// Nama tipe komponen `cid` (untuk pesan error query berkonteks).
    pub(crate) fn component_name(&self, cid: ComponentId) -> &'static str {
        self.registry.name(cid)
    }

    /// Slice bersama seluruh archetype (jalur query berbagi, RFC-0016).
    pub(crate) fn archetypes(&self) -> &[Archetype] {
        &self.archetypes
    }

    /// Menemukan archetype dengan himpunan komponen `ids` (terurut), atau
    /// membuatnya bila belum ada. Mengembalikan indeksnya.
    fn find_or_create_archetype(&mut self, ids: &[ComponentId]) -> usize {
        if let Some(i) = self
            .archetypes
            .iter()
            .position(|a| a.component_ids() == ids)
        {
            return i;
        }
        let columns = ids.iter().map(|&id| self.registry.new_column(id)).collect();
        self.archetypes.push(Archetype::new(ids.to_vec(), columns));
        self.archetypes.len() - 1
    }

    /// Memperbarui lokasi entity yang tergeser ke `row` di `archetype` akibat
    /// `swap_remove`, bila ada.
    fn fix_swapped(&mut self, moved: Option<Entity>, archetype: usize, row: usize) {
        if let Some(swapped) = moved {
            self.entities[swapped.index() as usize].location = Some(Location { archetype, row });
        }
    }
}

/// Meminjam dua archetype berbeda secara mutabel sekaligus dari satu slice.
fn split_two(archetypes: &mut [Archetype], a: usize, b: usize) -> (&mut Archetype, &mut Archetype) {
    assert_ne!(a, b, "split_two membutuhkan indeks berbeda");
    if a < b {
        let (left, right) = archetypes.split_at_mut(b);
        (&mut left[a], &mut right[0])
    } else {
        let (left, right) = archetypes.split_at_mut(a);
        (&mut right[0], &mut left[b])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_baru_dapat_dibuat() {
        let _world = World::new();
    }

    #[test]
    fn spawn_menghasilkan_entity_berbeda() {
        let mut world = World::new();
        let a = world.spawn();
        let b = world.spawn();
        assert_ne!(a, b);
    }

    #[test]
    fn entity_hidup_setelah_spawn() {
        let mut world = World::new();
        let e = world.spawn();
        assert!(world.contains(e));
    }

    #[test]
    fn entity_tidak_hidup_setelah_despawn() {
        let mut world = World::new();
        let e = world.spawn();
        world.despawn(e);
        assert!(!world.contains(e));
    }

    // STD-0007: handle basi tidak boleh valid meski slot indeksnya dipakai ulang.
    #[test]
    fn handle_basi_terdeteksi_setelah_slot_dipakai_ulang() {
        let mut world = World::new();
        let old = world.spawn();
        world.despawn(old);
        let new = world.spawn();

        // Slot yang sama dipakai ulang...
        assert_eq!(old.index(), new.index());
        // ...tetapi handle lama berbeda dan terdeteksi basi.
        assert_ne!(old, new);
        assert!(!world.contains(old));
        assert!(world.contains(new));
    }

    // STD-0005 (guard): urutan operasi yang sama menghasilkan handle yang sama,
    // termasuk pemakaian ulang slot. Menjaga free-list tetap LIFO deterministik.
    #[test]
    fn alokasi_entity_deterministik_antar_run() {
        fn urutan() -> Vec<Entity> {
            let mut world = World::new();
            let a = world.spawn();
            let b = world.spawn();
            world.despawn(a);
            let c = world.spawn();
            vec![a, b, c]
        }
        assert_eq!(urutan(), urutan());
    }

    #[derive(PartialEq, Debug)]
    struct Position(i32, i32);
    #[derive(PartialEq, Debug)]
    struct Velocity(i32, i32);

    #[test]
    fn insert_lalu_get_mengembalikan_komponen() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Position(3, 5));
        assert_eq!(world.get::<Position>(e), Some(&Position(3, 5)));
    }

    #[test]
    fn insert_komponen_kedua_memindah_archetype_dan_keduanya_terbaca() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Position(1, 2));
        world.insert(e, Velocity(3, 4));
        assert_eq!(world.get::<Position>(e), Some(&Position(1, 2)));
        assert_eq!(world.get::<Velocity>(e), Some(&Velocity(3, 4)));
    }

    #[test]
    fn remove_mengembalikan_komponen_dan_menyisakan_yang_lain() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Position(1, 2));
        world.insert(e, Velocity(3, 4));

        let taken = world.remove::<Velocity>(e);

        assert_eq!(taken, Some(Velocity(3, 4)));
        assert_eq!(world.get::<Velocity>(e), None);
        assert_eq!(world.get::<Position>(e), Some(&Position(1, 2)));
    }

    #[test]
    fn remove_komponen_terakhir_menjadikan_entity_tanpa_komponen() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Position(7, 8));

        assert_eq!(world.remove::<Position>(e), Some(Position(7, 8)));
        assert_eq!(world.get::<Position>(e), None);
        assert!(world.contains(e));
    }

    #[test]
    fn query_ref_mengiterasi_semua_pemilik_komponen() {
        let mut world = World::new();
        let a = world.spawn();
        world.insert(a, Position(1, 1));
        let b = world.spawn();
        world.insert(b, Position(2, 2));
        let c = world.spawn();
        world.insert(c, Velocity(9, 9)); // bukan pemilik Position

        let mut xs: Vec<i32> = world.query::<Position>().map(|p| p.0).collect();
        xs.sort();
        assert_eq!(xs, vec![1, 2]);
    }

    #[test]
    fn query_tidak_menyertakan_entity_yang_didespawn() {
        let mut world = World::new();
        let a = world.spawn();
        world.insert(a, Position(1, 1));
        let b = world.spawn();
        world.insert(b, Position(2, 2));

        world.despawn(a);

        let xs: Vec<i32> = world.query::<Position>().map(|p| p.0).collect();
        assert_eq!(xs, vec![2]);
        // Entity b tetap terbaca dengan benar setelah slot a dibersihkan.
        assert_eq!(world.get::<Position>(b), Some(&Position(2, 2)));
    }

    #[test]
    fn query_mut_memodifikasi_komponen() {
        let mut world = World::new();
        let a = world.spawn();
        world.insert(a, Position(1, 1));
        let b = world.spawn();
        world.insert(b, Position(2, 2));

        for p in world.query_mut::<Position>() {
            p.0 += 10;
        }

        let mut xs: Vec<i32> = world.query::<Position>().map(|p| p.0).collect();
        xs.sort();
        assert_eq!(xs, vec![11, 12]);
    }

    #[test]
    fn query_pair_membaca_a_dan_memodifikasi_b_hanya_pada_yang_cocok() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Position(1, 2));
        world.insert(e, Velocity(10, 20));
        let f = world.spawn();
        world.insert(f, Velocity(5, 5)); // tanpa Position → tak cocok

        for (pos, vel) in world.query_pair::<Position, Velocity>() {
            vel.0 += pos.0;
        }

        assert_eq!(world.get::<Velocity>(e), Some(&Velocity(11, 20)));
        assert_eq!(world.get::<Velocity>(f), Some(&Velocity(5, 5)));
    }

    // Menolak alias &mut ke komponen yang sama; pesan menyebut tipe (STD-0008).
    #[test]
    #[should_panic(expected = "Position")]
    fn query_pair_menolak_alias_ke_komponen_sama() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Position(1, 1));
        let _ = world.query_pair::<Position, Position>().count();
    }

    #[derive(PartialEq, Debug)]
    struct Value(i32);
    #[derive(PartialEq, Debug)]
    struct Score(u32);

    #[test]
    fn resource_insert_get_mutate_remove() {
        let mut world = World::new();
        assert!(!world.contains_resource::<Score>());

        world.insert_resource(Score(10));
        assert!(world.contains_resource::<Score>());
        assert_eq!(world.resource::<Score>(), Some(&Score(10)));

        world.resource_mut::<Score>().unwrap().0 += 5;
        assert_eq!(world.resource::<Score>(), Some(&Score(15)));

        assert_eq!(world.remove_resource::<Score>(), Some(Score(15)));
        assert!(!world.contains_resource::<Score>());
        assert_eq!(world.resource::<Score>(), None);
    }

    #[test]
    fn par_for_each_menerapkan_ke_semua_pemilik() {
        let mut world = World::new();
        let mut ents = Vec::new();
        for i in 0..100 {
            let e = world.spawn();
            world.insert(e, Value(i));
            ents.push(e);
        }

        world.par_for_each::<Value>(|v| v.0 += 1000);

        for (i, e) in ents.iter().enumerate() {
            assert_eq!(world.get::<Value>(*e), Some(&Value(i as i32 + 1000)));
        }
    }

    // STD-0005 (guard): urutan iterasi query stabil antar-run.
    #[test]
    fn urutan_iterasi_query_deterministik() {
        fn run() -> Vec<i32> {
            let mut world = World::new();
            for i in 0..5 {
                let e = world.spawn();
                world.insert(e, Position(i, i));
            }
            world.query::<Position>().map(|p| p.0).collect()
        }
        assert_eq!(run(), run());
    }
}
