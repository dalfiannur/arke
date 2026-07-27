//! Sistem dan penjadwalan deterministik (RFC-0003).
//!
//! Sebuah [`System`] membungkus logika `FnMut(&mut World)` beserta deklarasi
//! akses eksplisit. [`Schedule`] menghitung urutan eksekusi deterministik dan
//! mengelompokkan sistem tak-konflik ke dalam *stage* yang aman diparalelkan.
//! Eksekusi M-2 bersifat serial (lihat [ADR-0003](../../docs/ADR/ADR-0003-deterministic-scheduler.md)).

use std::any::TypeId;

use crate::Component;
use crate::World;
use crate::query::{Access, QueryData, QueryFilter};

/// Unit logika yang berjalan atas sebuah [`World`].
pub struct System {
    run: Box<dyn FnMut(&mut World)>,
    access: Access,
}

impl System {
    /// Membuat sistem dari closure `FnMut(&mut World)`.
    pub fn new(run: impl FnMut(&mut World) + 'static) -> Self {
        Self {
            run: Box::new(run),
            access: Access::default(),
        }
    }

    /// Menandai bahwa sistem membaca komponen `T`.
    pub fn reads<T: Component>(mut self) -> Self {
        self.access.reads.push(TypeId::of::<T>());
        self
    }

    /// Menandai bahwa sistem menulis komponen `T`.
    pub fn writes<T: Component>(mut self) -> Self {
        self.access.writes.push(TypeId::of::<T>());
        self
    }

    /// Membangun sistem yang menerapkan `f` pada setiap entity yang cocok dengan
    /// query `Q`, dengan **akses tersimpul dari tipe** `Q` (RFC-0005).
    ///
    /// Contoh: `System::each::<(&Position, &mut Velocity)>(|(p, v)| v.0 += p.0)`
    /// otomatis terdaftar sebagai membaca `Position` dan menulis `Velocity`.
    pub fn each<Q: QueryData>(mut f: impl FnMut(Q::Item<'_>) + 'static) -> Self {
        Self {
            run: Box::new(move |world: &mut World| Q::each(world, &mut f)),
            access: Q::access(),
        }
    }

    /// Seperti [`System::each`], tetapi hanya untuk entity yang lolos filter `F`
    /// (`With`/`Without`, RFC-0014). Filter tak menyumbang akses.
    pub fn each_filtered<Q: QueryData, F: QueryFilter + 'static>(
        mut f: impl FnMut(Q::Item<'_>) + 'static,
    ) -> Self {
        Self {
            run: Box::new(move |world: &mut World| Q::each_filtered::<F>(world, &mut f)),
            access: Q::access(),
        }
    }

    /// Membangun sistem yang memutasi resource `R` sekali per run, dengan akses
    /// tersimpul (tulis `R`). No-op bila resource tak ada (RFC-0010).
    pub fn resource<R: 'static + Send>(mut f: impl FnMut(&mut R) + 'static) -> Self {
        Self {
            run: Box::new(move |world: &mut World| {
                if let Some(r) = world.resource_mut::<R>() {
                    f(r);
                }
            }),
            access: Access::new().with_resource_write::<R>(),
        }
    }

    /// Membangun sistem yang **membaca** resource `R` sambil mengiterasi query
    /// `Q` per entity. Akses tersimpul: baca `R` + akses `Q`.
    ///
    /// Aman tanpa `unsafe`: resource diambil keluar sementara selama iterasi
    /// lalu dikembalikan. No-op bila resource tak ada.
    pub fn each_res<R: 'static + Send, Q: QueryData>(
        mut f: impl FnMut(&R, Q::Item<'_>) + 'static,
    ) -> Self {
        Self {
            run: Box::new(move |world: &mut World| {
                if let Some(r) = world.remove_resource::<R>() {
                    Q::each(world, |item| f(&r, item));
                    world.insert_resource(r);
                }
            }),
            access: Q::access().with_resource_read::<R>(),
        }
    }
}

/// Kumpulan sistem yang dijalankan dalam urutan deterministik.
#[derive(Default)]
pub struct Schedule {
    systems: Vec<System>,
}

impl Schedule {
    /// Membuat schedule kosong.
    pub fn new() -> Self {
        Self::default()
    }

    /// Menambahkan sebuah sistem ke schedule.
    pub fn add(&mut self, system: System) -> &mut Self {
        self.systems.push(system);
        self
    }

    /// Menjalankan semua sistem terhadap `world`, **stage demi stage**.
    ///
    /// Di M-2 sistem dalam satu stage dijalankan serial (urutannya tak penting
    /// karena tak-konflik); di M-3 stage yang sama dijalankan paralel dengan
    /// hasil identik (STD-0006). Eksekusi stage-demi-stage setara dengan
    /// eksekusi serial urutan registrasi.
    pub fn run(&mut self, world: &mut World) {
        for stage in self.stages() {
            for idx in stage {
                (self.systems[idx].run)(world);
            }
        }
    }

    /// Rencana eksekusi paralel deterministik: kelompok indeks sistem yang
    /// aman berjalan bersamaan (RFC-0003 §3).
    ///
    /// Sistem `i` ditempatkan pada stage `1 + max stage pendahulu yang
    /// berkonflik` (atau `0` bila tak ada). Sistem dalam stage yang sama dijamin
    /// pairwise tak-konflik.
    pub fn stages(&self) -> Vec<Vec<usize>> {
        let mut stage_of = vec![0usize; self.systems.len()];
        for i in 0..self.systems.len() {
            let access_i = &self.systems[i].access;
            let mut stage = 0;
            for (j, sys_j) in self.systems[..i].iter().enumerate() {
                if access_i.conflicts(&sys_j.access) {
                    stage = stage.max(stage_of[j] + 1);
                }
            }
            stage_of[i] = stage;
        }
        let stage_count = stage_of.iter().max().map_or(0, |&m| m + 1);
        let mut stages = vec![Vec::new(); stage_count];
        for (i, &stage) in stage_of.iter().enumerate() {
            stages[stage].push(i);
        }
        stages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::World;

    #[derive(PartialEq, Debug)]
    struct Counter(i32);
    struct Position;
    struct Velocity;
    #[derive(PartialEq, Debug)]
    struct Tally(i32);

    #[test]
    fn system_resource_memutasi_resource_tiap_run() {
        let mut world = World::new();
        world.insert_resource(Tally(0));
        let mut s = Schedule::new();
        s.add(System::resource::<Tally>(|t| t.0 += 3));
        s.run(&mut world);
        s.run(&mut world);
        assert_eq!(world.resource::<Tally>(), Some(&Tally(6)));
    }

    #[test]
    fn system_each_res_baca_resource_saat_iterasi_lalu_kembalikan() {
        let mut world = World::new();
        world.insert_resource(Tally(10));
        let e = world.spawn();
        world.insert(e, Counter(1));

        let mut s = Schedule::new();
        s.add(System::each_res::<Tally, &mut Counter>(|t, c| c.0 += t.0));
        s.run(&mut world);

        assert_eq!(world.get::<Counter>(e), Some(&Counter(11)));
        // Resource dikembalikan setelah iterasi.
        assert_eq!(world.resource::<Tally>(), Some(&Tally(10)));
    }

    #[test]
    fn dua_sistem_menulis_resource_sama_stage_berbeda() {
        let mut s = Schedule::new();
        s.add(System::resource::<Tally>(|_| {}));
        s.add(System::resource::<Tally>(|_| {}));
        assert_eq!(s.stages(), vec![vec![0], vec![1]]);
    }

    #[test]
    fn komponen_dan_resource_tipe_sama_tak_konflik() {
        // Sistem 0 menulis KOMPONEN Counter; sistem 1 menulis RESOURCE Counter.
        // TypeId sama, namespace berbeda → tak konflik → satu stage.
        let mut s = Schedule::new();
        s.add(System::each::<&mut Counter>(|_| {}));
        s.add(System::resource::<Counter>(|_| {}));
        assert_eq!(s.stages(), vec![vec![0, 1]]);
    }

    #[test]
    fn stage_mengelompokkan_tak_konflik_dan_memisahkan_yang_konflik() {
        let mut s = Schedule::new();
        s.add(System::new(|_| {}).writes::<Position>()); // 0
        s.add(System::new(|_| {}).reads::<Position>()); // 1 — konflik R-W dg 0
        s.add(System::new(|_| {}).writes::<Velocity>()); // 2 — independen

        // 0 & 2 independen → stage 0; 1 konflik dg 0 → stage 1.
        assert_eq!(s.stages(), vec![vec![0, 2], vec![1]]);
    }

    #[test]
    fn system_berjalan_dan_memutasi_world() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Counter(0));

        let mut schedule = Schedule::new();
        schedule.add(System::new(|w: &mut World| {
            for c in w.query_mut::<Counter>() {
                c.0 += 1;
            }
        }));
        schedule.run(&mut world);

        assert_eq!(world.get::<Counter>(e), Some(&Counter(1)));
    }

    #[test]
    fn system_each_menerapkan_dan_menyimpulkan_akses() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Counter(0));

        let mut s = Schedule::new();
        s.add(System::each::<&mut Counter>(|c| c.0 += 5));
        s.run(&mut world);

        assert_eq!(world.get::<Counter>(e), Some(&Counter(5)));
    }

    #[test]
    fn stage_dari_akses_tersimpul_tipe() {
        let mut s = Schedule::new();
        s.add(System::each::<&mut Counter>(|_| {})); // tulis Counter → stage 0
        s.add(System::each::<&Counter>(|_| {})); // baca Counter → konflik → stage 1
        assert_eq!(s.stages(), vec![vec![0], vec![1]]);
    }

    #[test]
    fn baca_baca_berbagi_stage() {
        let mut s = Schedule::new();
        s.add(System::new(|_| {}).reads::<Position>());
        s.add(System::new(|_| {}).reads::<Position>());
        // Dua pembaca komponen sama tidak berkonflik → satu stage.
        assert_eq!(s.stages(), vec![vec![0, 1]]);
    }

    #[test]
    fn menjalankan_schedule_konflik_deterministik_dan_terurut() {
        fn jalankan() -> i32 {
            let mut world = World::new();
            let e = world.spawn();
            world.insert(e, Counter(0));
            let mut s = Schedule::new();
            s.add(
                System::new(|w| {
                    for c in w.query_mut::<Counter>() {
                        c.0 += 1;
                    }
                })
                .writes::<Counter>(),
            );
            s.add(
                System::new(|w| {
                    for c in w.query_mut::<Counter>() {
                        c.0 *= 3;
                    }
                })
                .writes::<Counter>(),
            );
            s.run(&mut world);
            world.get::<Counter>(e).unwrap().0
        }
        // Dua penulis Counter berkonflik → stage berurutan → (0+1)*3 = 3.
        assert_eq!(jalankan(), 3);
        assert_eq!(jalankan(), jalankan());
    }
}
