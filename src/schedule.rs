//! Sistem dan penjadwalan deterministik (RFC-0003).
//!
//! Sebuah [`System`] membungkus logika `FnMut(&mut World)` beserta deklarasi
//! akses eksplisit. [`Schedule`] menghitung urutan eksekusi deterministik dan
//! mengelompokkan sistem tak-konflik ke dalam *stage* yang aman diparalelkan.
//! Eksekusi serial via [`Schedule::run`]; eksekusi **paralel** tingkat-sistem
//! via [`Schedule::run_parallel`] (RFC-0016). `unsafe` di modul ini terkurung
//! pada `unsafe impl Sync for SyncWorld` (sound karena sistem satu stage
//! mengakses komponen disjoint). Diverifikasi miri.

#![allow(unsafe_code)]

use std::any::TypeId;

use crate::Component;
use crate::World;
use crate::query::{Access, QueryData, QueryFilter, QueryState};

/// Cara sebuah sistem mengakses `World`.
enum Runner {
    /// Akses eksklusif `&mut World` (sistem opaque/resource) — **serial-saja**.
    Exclusive(Box<dyn FnMut(&mut World) + Send>),
    /// Akses berbagi `&World` (sistem bertipe) — **paralel-mampu** (RFC-0016).
    Shared(Box<dyn FnMut(&World) + Send>),
}

/// Unit logika yang berjalan atas sebuah [`World`].
pub struct System {
    runner: Runner,
    access: Access,
}

impl System {
    /// Membuat sistem opaque dari closure `FnMut(&mut World)` (serial-saja).
    pub fn new(run: impl FnMut(&mut World) + Send + 'static) -> Self {
        Self {
            runner: Runner::Exclusive(Box::new(run)),
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

    /// Membangun sistem bertipe (paralel-mampu) yang menerapkan `f` pada setiap
    /// entity yang cocok dengan query `Q`, dengan **akses tersimpul** (RFC-0005).
    ///
    /// Contoh: `System::each::<(&Position, &mut Velocity)>(|(p, v)| v.0 += p.0)`.
    pub fn each<Q: QueryData>(mut f: impl FnMut(Q::Item<'_>) + Send + 'static) -> Self {
        // Cache archetype yang cocok, persist lintas-run per-sistem (RFC-0017).
        let mut state = QueryState::default();
        Self {
            runner: Runner::Shared(Box::new(move |world: &World| {
                Q::each_cached::<()>(world, &mut state, &mut f);
            })),
            access: Q::access(),
        }
    }

    /// Seperti [`System::each`], tetapi hanya untuk entity yang lolos filter `F`
    /// (`With`/`Without`, RFC-0014). Filter tak menyumbang akses.
    pub fn each_filtered<Q: QueryData, F: QueryFilter + 'static>(
        mut f: impl FnMut(Q::Item<'_>) + Send + 'static,
    ) -> Self {
        // Cache archetype yang cocok, persist lintas-run per-sistem (RFC-0017).
        let mut state = QueryState::default();
        Self {
            runner: Runner::Shared(Box::new(move |world: &World| {
                Q::each_cached::<F>(world, &mut state, &mut f);
            })),
            access: Q::access(),
        }
    }

    /// Membangun sistem yang memutasi resource `R` sekali per run (serial-saja),
    /// dengan akses tersimpul (tulis `R`). No-op bila resource tak ada (RFC-0010).
    pub fn resource<R: 'static + Send>(mut f: impl FnMut(&mut R) + Send + 'static) -> Self {
        Self {
            runner: Runner::Exclusive(Box::new(move |world: &mut World| {
                if let Some(r) = world.resource_mut::<R>() {
                    f(r);
                }
            })),
            access: Access::new().with_resource_write::<R>(),
        }
    }

    /// Membangun sistem (serial-saja) yang **membaca** resource `R` sambil
    /// mengiterasi query `Q`. Akses tersimpul: baca `R` + akses `Q`.
    pub fn each_res<R: 'static + Send, Q: QueryData>(
        mut f: impl FnMut(&R, Q::Item<'_>) + Send + 'static,
    ) -> Self {
        Self {
            runner: Runner::Exclusive(Box::new(move |world: &mut World| {
                if let Some(r) = world.remove_resource::<R>() {
                    Q::each(world, |item| f(&r, item));
                    world.insert_resource(r);
                }
            })),
            access: Q::access().with_resource_read::<R>(),
        }
    }

    /// Apakah sistem ini paralel-mampu (berbagi `&World`).
    fn is_shared(&self) -> bool {
        matches!(self.runner, Runner::Shared(_))
    }

    /// Menjalankan sistem dengan akses eksklusif (serial).
    fn run(&mut self, world: &mut World) {
        match &mut self.runner {
            Runner::Exclusive(f) => f(world),
            Runner::Shared(f) => f(&*world),
        }
    }

    /// Menjalankan sistem `Shared` lewat `&World` berbagi (jalur paralel).
    fn run_shared(&mut self, world: &World) {
        if let Runner::Shared(f) = &mut self.runner {
            f(world);
        }
    }
}

/// Pembungkus agar `&World` dapat dibagi lintas-thread pada jalur paralel.
struct SyncWorld<'a>(&'a World);

// SAFETY: `SyncWorld` hanya dibagikan ke sistem-sistem satu stage yang, menurut
// analisis konflik (M-2/M-4), mengakses komponen **disjoint**. Akses bersamaan
// lewat `&World` ke kolom yang berbeda tak beralias (interior-mutability via
// `UnsafeCell`, RFC-0015/0016). Diverifikasi miri di CI.
unsafe impl Sync for SyncWorld<'_> {}

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
                self.systems[idx].run(world);
            }
        }
    }

    /// Menjalankan schedule dengan sistem tiap stage **paralel** (RFC-0016).
    ///
    /// Untuk tiap stage yang seluruh sistemnya bertipe (`Shared`), sistem
    /// dijalankan bersamaan di `std::thread::scope`; karena stage dijamin
    /// tak-konflik, aksesnya disjoint dan hasilnya **identik** dengan eksekusi
    /// serial (STD-0006). Stage yang memuat sistem opaque/resource (`Exclusive`)
    /// dijalankan serial.
    pub fn run_parallel(&mut self, world: &mut World) {
        for stage in self.stages() {
            let parallel = stage.len() > 1 && stage.iter().all(|&i| self.systems[i].is_shared());
            if !parallel {
                for idx in stage {
                    self.systems[idx].run(world);
                }
                continue;
            }
            let in_stage: std::collections::HashSet<usize> = stage.into_iter().collect();
            let sync_world = SyncWorld(&*world);
            std::thread::scope(|scope| {
                for (i, system) in self.systems.iter_mut().enumerate() {
                    if in_stage.contains(&i) {
                        let sync_world = &sync_world;
                        scope.spawn(move || system.run_shared(sync_world.0));
                    }
                }
            });
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
    struct Tag;
    #[derive(PartialEq, Debug)]
    struct Tally(i32);

    // STD-0006 tingkat-sistem: run_parallel == run (serial) untuk sistem disjoint.
    // Dua sistem menulis komponen berbeda pada archetype yang SAMA → berjalan di
    // dua thread mengakses kolom berbeda secara bersamaan (interior-mut RFC-0016).
    #[test]
    fn run_parallel_setara_serial() {
        fn setup() -> World {
            let mut w = World::new();
            for i in 0..8 {
                let e = w.spawn();
                w.insert(e, Counter(i));
                w.insert(e, Tally(i * 10));
            }
            w
        }
        fn sched() -> Schedule {
            let mut s = Schedule::new();
            s.add(System::each::<&mut Counter>(|c| c.0 += 1));
            s.add(System::each::<&mut Tally>(|t| t.0 *= 2));
            s
        }

        let mut serial = setup();
        sched().run(&mut serial);
        let mut parallel = setup();
        sched().run_parallel(&mut parallel);

        let sc: Vec<i32> = serial.query::<Counter>().map(|c| c.0).collect();
        let pc: Vec<i32> = parallel.query::<Counter>().map(|c| c.0).collect();
        assert_eq!(sc, pc);
        let st: Vec<i32> = serial.query::<Tally>().map(|t| t.0).collect();
        let pt: Vec<i32> = parallel.query::<Tally>().map(|t| t.0).collect();
        assert_eq!(st, pt);
    }

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
    fn system_each_cache_menangkap_entity_dan_archetype_baru_antar_run() {
        let mut world = World::new();
        let e1 = world.spawn();
        world.insert(e1, Counter(0));

        let mut s = Schedule::new();
        s.add(System::each::<&mut Counter>(|c| c.0 += 1));

        s.run(&mut world); // run 1: cache terisi archetype {Counter}
        assert_eq!(world.get::<Counter>(e1), Some(&Counter(1)));

        // Entity baru di archetype {Counter, Tag} baru, dibuat antar-run.
        let e2 = world.spawn();
        world.insert(e2, Counter(10));
        world.insert(e2, Tag);

        s.run(&mut world); // run 2: scan inkremental menangkap archetype baru
        assert_eq!(world.get::<Counter>(e1), Some(&Counter(2)));
        assert_eq!(world.get::<Counter>(e2), Some(&Counter(11)));
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
