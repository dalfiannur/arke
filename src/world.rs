//! Otoritas atas seluruh entity, komponen, dan resource (RFC-0002 §3.4).
//!
//! [`World`] adalah satu-satunya sumber kebenaran keadaan. Segala mutasi
//! bermuara padanya, sehingga sebuah snapshot atas `World` cukup untuk merekam
//! seluruh keadaan (invarian *kepemilikan & portabilitas data*).
//!
//! Isi struktur ini — tabel slot entity, registry komponen, dan penyimpanan
//! archetype — ditambahkan secara test-first selama Milestone M-1.

use crate::entity::Entity;

/// Metadata per-slot entity.
struct EntityMeta {
    generation: u32,
    alive: bool,
}

/// Wadah pemilik semua entity, komponen, dan resource.
///
/// Kemampuan insert/remove komponen dan query dikembangkan pada Milestone M-1
/// (lihat `docs/MILESTONE_1.md`).
#[derive(Default)]
pub struct World {
    entities: Vec<EntityMeta>,
    /// Indeks slot bebas, dikelola sebagai tumpukan LIFO agar alokasi
    /// deterministik (STD-0005).
    free: Vec<u32>,
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
            Entity::new(index, meta.generation)
        } else {
            let index = self.entities.len() as u32;
            self.entities.push(EntityMeta {
                generation: 0,
                alive: true,
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
        let meta = &mut self.entities[index as usize];
        meta.alive = false;
        meta.generation += 1;
        self.free.push(index);
    }

    /// Mengembalikan `true` bila `entity` masih hidup di `World` ini.
    pub fn contains(&self, entity: Entity) -> bool {
        self.entities
            .get(entity.index() as usize)
            .is_some_and(|meta| meta.alive && meta.generation == entity.generation())
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
}
