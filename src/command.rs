//! Mutasi struktural **tertunda** via [`CommandBuffer`] (RFC-0019).
//!
//! Spawn/despawn/insert/remove butuh `&mut World`, jadi tak bisa dilakukan
//! sistem paralel (`&World`). [`CommandBuffer`] merekam operasi sebagai penutup
//! dan meng-apply-nya nanti saat `&mut World` tersedia — **urutan-rekam**,
//! deterministik (STD-0005). 100% aman.

use crate::component::Component;
use crate::entity::Entity;
use crate::world::World;

/// Konfigurator entity hasil `spawn` (dijalankan dengan `Entity` nyata).
type SpawnConfig = Box<dyn FnOnce(&mut World, Entity) + Send>;
/// Operasi tertunda atas entity yang sudah ada (despawn/insert/remove).
type Op = Box<dyn FnOnce(&mut World) + Send>;

/// Satu command tertunda.
enum Command {
    /// Spawn entity baru, lalu jalankan konfigurator dengan `Entity` hasilnya.
    Spawn(Vec<SpawnConfig>),
    /// Operasi atas entity existing.
    Op(Op),
}

/// Perekam **mutasi struktural tertunda** (RFC-0019).
///
/// Merekam spawn/despawn/insert/remove untuk di-[`apply`](Self::apply) nanti
/// saat `&mut World` tersedia. Command dijalankan **urutan-rekam** →
/// deterministik. `Send`, sehingga tiap sistem paralel dapat memegang buffer
/// sendiri (RFC-0018/0019).
#[derive(Default)]
pub struct CommandBuffer {
    commands: Vec<Command>,
}

impl CommandBuffer {
    /// Buffer kosong.
    pub fn new() -> Self {
        Self::default()
    }

    /// Merekam spawn entity baru; kembalikan builder untuk mengonfigurasinya.
    ///
    /// Konfigurasi ([`EntityCommands::insert`]) dijalankan saat
    /// [`apply`](Self::apply) dengan `Entity` **nyata** hasil spawn.
    pub fn spawn(&mut self) -> EntityCommands<'_> {
        let at = self.commands.len();
        self.commands.push(Command::Spawn(Vec::new()));
        EntityCommands { buffer: self, at }
    }

    /// Merekam despawn `entity`.
    pub fn despawn(&mut self, entity: Entity) {
        self.commands
            .push(Command::Op(Box::new(move |w| w.despawn(entity))));
    }

    /// Merekam insert `component` ke `entity`.
    pub fn insert<T: Component>(&mut self, entity: Entity, component: T) {
        self.commands
            .push(Command::Op(Box::new(move |w| w.insert(entity, component))));
    }

    /// Merekam remove komponen `T` dari `entity`.
    pub fn remove<T: Component>(&mut self, entity: Entity) {
        self.commands.push(Command::Op(Box::new(move |w| {
            w.remove::<T>(entity);
        })));
    }

    /// Apakah tak ada command terekam.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Jumlah command terekam.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Buang semua command tanpa menjalankannya.
    pub fn clear(&mut self) {
        self.commands.clear();
    }

    /// Jalankan lalu **kuras** semua command, **urutan-rekam** (STD-0005).
    /// Buffer kosong lagi setelahnya (dapat dipakai ulang).
    pub fn apply(&mut self, world: &mut World) {
        for command in self.commands.drain(..) {
            match command {
                Command::Spawn(configs) => {
                    let entity = world.spawn();
                    for config in configs {
                        config(world, entity);
                    }
                }
                Command::Op(op) => op(world),
            }
        }
    }
}

/// Builder untuk entity yang akan di-spawn oleh [`CommandBuffer::spawn`].
///
/// Meminjam buffer secara mutabel selama rantai `.insert(..)`; lepas saat
/// builder di-drop.
pub struct EntityCommands<'a> {
    buffer: &'a mut CommandBuffer,
    at: usize,
}

impl EntityCommands<'_> {
    /// Rekam agar `component` ditempelkan ke entity baru saat apply.
    pub fn insert<T: Component>(self, component: T) -> Self {
        if let Command::Spawn(configs) = &mut self.buffer.commands[self.at] {
            configs.push(Box::new(move |w, e| w.insert(e, component)));
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::World;

    #[derive(PartialEq, Debug)]
    struct Health(i32);
    #[derive(PartialEq, Debug)]
    struct Score(i32);
    struct Tag;

    #[test]
    fn apply_menjalankan_command_urutan_rekam() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Health(100));

        let mut cmd = CommandBuffer::new();
        cmd.spawn().insert(Health(1)).insert(Tag); // entity baru terkonfigurasi
        cmd.insert(e, Score(5)); // insert ke entity existing
        cmd.remove::<Health>(e); // remove dari entity existing
        assert!(!cmd.is_empty());

        cmd.apply(&mut world);
        assert!(cmd.is_empty()); // buffer terkuras → dapat dipakai ulang

        // Entity existing: Health dihapus, Score ditambah.
        assert_eq!(world.get::<Health>(e), None);
        assert_eq!(world.get::<Score>(e), Some(&Score(5)));
        // Entity baru: satu, dengan Health(1) + Tag.
        assert_eq!(world.query::<Health>().filter(|h| h.0 == 1).count(), 1);
        assert_eq!(world.query::<Tag>().count(), 1);
    }

    #[test]
    fn despawn_tertunda_menghapus_entity() {
        let mut world = World::new();
        let a = world.spawn();
        world.insert(a, Health(1));
        let b = world.spawn();
        world.insert(b, Health(2));

        let mut cmd = CommandBuffer::new();
        cmd.despawn(a);
        cmd.apply(&mut world);

        assert!(!world.contains(a));
        assert!(world.contains(b));
    }
}
