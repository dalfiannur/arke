//! Sistem dan penjadwalan deterministik (RFC-0003).
//!
//! Sebuah [`System`] membungkus logika `FnMut(&mut World)` beserta deklarasi
//! akses eksplisit. [`Schedule`] menghitung urutan eksekusi deterministik.
//! Eksekusi serial via [`Schedule::run`] (stage demi stage); eksekusi
//! **paralel** tingkat-sistem via [`Schedule::run_parallel`] lewat
//! **graf-ketergantungan** (RFC-0018) — tiap sistem mulai segera setelah
//! pendahulu berkonfliknya selesai, bukan menunggu barrier stage. `unsafe` di
//! modul ini terkurung pada `unsafe impl Sync for SyncWorld` (sound karena
//! sistem yang berjalan bersamaan dijamin tak-konflik oleh graf). Diverifikasi
//! miri.

#![allow(unsafe_code)]

use std::any::TypeId;
use std::sync::{Condvar, Mutex};

use crate::CommandBuffer;
use crate::Component;
use crate::World;
use crate::query::{Access, QueryData, QueryFilter, QueryState};

/// Closure sistem `SharedCmd`: baca `&World`, rekam ke buffer (RFC-0019).
type CmdRunner = Box<dyn FnMut(&World, &mut CommandBuffer) + Send>;

/// Cara sebuah sistem mengakses `World`.
enum Runner {
    /// Akses eksklusif `&mut World` (sistem opaque/resource) — **serial-saja**.
    Exclusive(Box<dyn FnMut(&mut World) + Send>),
    /// Akses berbagi `&World` (sistem bertipe) — **paralel-mampu** (RFC-0016).
    Shared(Box<dyn FnMut(&World) + Send>),
    /// Akses berbagi `&World` + merekam ke [`CommandBuffer`] milik sistem —
    /// **paralel-mampu**; buffer di-apply di akhir run (RFC-0019).
    SharedCmd(CmdRunner),
}

/// Unit logika yang berjalan atas sebuah [`World`].
pub struct System {
    runner: Runner,
    access: Access,
    /// Buffer command tertunda milik sistem (dipakai `SharedCmd`); kosong untuk
    /// sistem lain. Di-apply di akhir run, urutan registrasi (RFC-0019).
    commands: CommandBuffer,
}

impl System {
    /// Membangun `System` dari runner + access, dengan buffer command kosong.
    fn with(runner: Runner, access: Access) -> Self {
        Self {
            runner,
            access,
            commands: CommandBuffer::new(),
        }
    }

    /// Membuat sistem opaque dari closure `FnMut(&mut World)` (serial-saja).
    pub fn new(run: impl FnMut(&mut World) + Send + 'static) -> Self {
        Self::with(Runner::Exclusive(Box::new(run)), Access::default())
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
        Self::with(
            Runner::Shared(Box::new(move |world: &World| {
                Q::each_cached::<()>(world, &mut state, &mut f);
            })),
            Q::access(),
        )
    }

    /// Seperti [`System::each`], tetapi hanya untuk entity yang lolos filter `F`
    /// (`With`/`Without`, RFC-0014). Filter tak menyumbang akses.
    pub fn each_filtered<Q: QueryData, F: QueryFilter + 'static>(
        mut f: impl FnMut(Q::Item<'_>) + Send + 'static,
    ) -> Self {
        // Cache archetype yang cocok, persist lintas-run per-sistem (RFC-0017).
        let mut state = QueryState::default();
        Self::with(
            Runner::Shared(Box::new(move |world: &World| {
                Q::each_cached::<F>(world, &mut state, &mut f);
            })),
            Q::access(),
        )
    }

    /// Membangun sistem **paralel-mampu** yang mengiterasi query `Q` dan dapat
    /// merekam **mutasi struktural tertunda** ke sebuah [`CommandBuffer`]
    /// (RFC-0019). Buffer di-apply di **akhir run**, urutan registrasi.
    ///
    /// Contoh: `System::each_cmd::<&Health>(|h, cmd| if h.0 <= 0 { cmd.spawn(); })`.
    pub fn each_cmd<Q: QueryData>(
        mut f: impl FnMut(Q::Item<'_>, &mut CommandBuffer) + Send + 'static,
    ) -> Self {
        let mut state = QueryState::default();
        Self::with(
            Runner::SharedCmd(Box::new(move |world: &World, cmds: &mut CommandBuffer| {
                Q::each_cached::<()>(world, &mut state, |item| f(item, cmds));
            })),
            Q::access(),
        )
    }

    /// Membangun sistem yang memutasi resource `R` sekali per run (serial-saja),
    /// dengan akses tersimpul (tulis `R`). No-op bila resource tak ada (RFC-0010).
    pub fn resource<R: 'static + Send>(mut f: impl FnMut(&mut R) + Send + 'static) -> Self {
        Self::with(
            Runner::Exclusive(Box::new(move |world: &mut World| {
                if let Some(r) = world.resource_mut::<R>() {
                    f(r);
                }
            })),
            Access::new().with_resource_write::<R>(),
        )
    }

    /// Membangun sistem (serial-saja) yang **membaca** resource `R` sambil
    /// mengiterasi query `Q`. Akses tersimpul: baca `R` + akses `Q`.
    pub fn each_res<R: 'static + Send, Q: QueryData>(
        mut f: impl FnMut(&R, Q::Item<'_>) + Send + 'static,
    ) -> Self {
        Self::with(
            Runner::Exclusive(Box::new(move |world: &mut World| {
                if let Some(r) = world.remove_resource::<R>() {
                    Q::each(world, |item| f(&r, item));
                    world.insert_resource(r);
                }
            })),
            Q::access().with_resource_read::<R>(),
        )
    }

    /// Apakah sistem ini paralel-mampu (berbagi `&World`).
    fn is_shared(&self) -> bool {
        matches!(self.runner, Runner::Shared(_) | Runner::SharedCmd(_))
    }

    /// Menjalankan sistem dengan akses eksklusif (serial).
    fn run(&mut self, world: &mut World) {
        match &mut self.runner {
            Runner::Exclusive(f) => f(world),
            Runner::Shared(f) => f(&*world),
            // Merekam ke buffer sendiri; di-apply belakangan oleh scheduler.
            Runner::SharedCmd(f) => f(&*world, &mut self.commands),
        }
    }

    /// Menjalankan sistem `Shared`/`SharedCmd` lewat `&World` (jalur paralel).
    fn run_shared(&mut self, world: &World) {
        match &mut self.runner {
            Runner::Shared(f) => f(world),
            Runner::SharedCmd(f) => f(world, &mut self.commands),
            Runner::Exclusive(_) => {}
        }
    }

    /// Meng-apply buffer command milik sistem (di akhir run, RFC-0019).
    fn apply_commands(&mut self, world: &mut World) {
        self.commands.apply(world);
    }
}

/// Pembungkus agar `&World` dapat dibagi lintas-thread pada jalur paralel.
struct SyncWorld<'a>(&'a World);

// SAFETY: `SyncWorld` hanya dibagikan ke sistem-sistem yang **berjalan
// bersamaan**, yang menurut graf-ketergantungan (RFC-0018) dijamin **tak-konflik**
// (sisi menghubungkan tiap pasangan berkonflik → tak pernah bersamaan). Sistem
// tak-konflik mengakses komponen **disjoint** (analisis M-2/M-4); akses bersamaan
// lewat `&World` ke kolom berbeda tak beralias (interior-mutability via
// `UnsafeCell`, RFC-0015/0016). Diverifikasi miri di CI.
unsafe impl Sync for SyncWorld<'_> {}

/// Menjalankan sekumpulan sistem `Shared` lewat **graf-ketergantungan**: tiap
/// sistem mulai segera setelah pendahulu berkonflik selesai (RFC-0018 §2).
///
/// Model **thread-per-sistem** (`std::thread::scope`): tiap thread memegang
/// `&mut System` disjoint (miliknya sendiri) dan menunggu di `Condvar` sampai
/// seluruh pendahulunya (`pending[i] == 0`) selesai, lalu berjalan, lalu
/// mengurangi `pending` tiap suksesor. Graf **asiklik** (sisi hanya `j → i`
/// dengan `j < i`) → bebas deadlock. Pasangan berkonflik tak pernah bersamaan
/// → hasil **identik** eksekusi serial (STD-0006). **Tanpa `unsafe` baru**.
fn run_graph_shared(systems: &mut [System], world: &World) {
    let n = systems.len();
    // Bangun graf: sisi j→i untuk tiap j<i yang berkonflik (RFC-0018 §1).
    let mut pending = vec![0usize; n];
    let mut successors: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 0..n {
        for j in 0..i {
            if systems[i].access.conflicts(&systems[j].access) {
                successors[j].push(i);
                pending[i] += 1;
            }
        }
    }

    let sync_world = SyncWorld(world);
    let pending = Mutex::new(pending);
    let signal = Condvar::new();

    std::thread::scope(|scope| {
        for (i, system) in systems.iter_mut().enumerate() {
            let pending = &pending;
            let signal = &signal;
            let successors = &successors;
            let sync_world = &sync_world;
            scope.spawn(move || {
                // Tunggu semua pendahulu berkonflik selesai.
                {
                    let mut guard = pending.lock().unwrap();
                    while guard[i] > 0 {
                        guard = signal.wait(guard).unwrap();
                    }
                }
                // Jalankan sistem (thread ini pemilik tunggal `&mut System`).
                system.run_shared(sync_world.0);
                // Tandai selesai; rilis penghitung suksesor.
                {
                    let mut guard = pending.lock().unwrap();
                    for &s in &successors[i] {
                        guard[s] -= 1;
                    }
                }
                signal.notify_all();
            });
        }
    });
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
                self.systems[idx].run(world);
            }
        }
        self.apply_commands(world);
    }

    /// Menjalankan schedule secara **paralel** lewat graf-ketergantungan
    /// (RFC-0018): tiap sistem mulai segera setelah pendahulu yang berkonflik
    /// dengannya selesai — bukan menunggu barrier stage penuh.
    ///
    /// Schedule disegmen pada batas sistem `Exclusive` (opaque/resource): tiap
    /// *run* maksimal sistem `Shared` dijalankan lewat graf (`std::thread::scope`
    /// berbagi `&World`), tiap `Exclusive` dijalankan **serial** sebagai barrier
    /// `&mut World`. Karena pasangan berkonflik tetap terurut registrasi, hasil
    /// **identik** dengan eksekusi serial (STD-0006), hanya lebih paralel.
    pub fn run_parallel(&mut self, world: &mut World) {
        let n = self.systems.len();
        let mut i = 0;
        while i < n {
            if self.systems[i].is_shared() {
                let start = i;
                while i < n && self.systems[i].is_shared() {
                    i += 1;
                }
                // Segmen [start, i): semua `Shared` → jalankan lewat graf.
                if i - start == 1 {
                    self.systems[start].run(world);
                } else {
                    run_graph_shared(&mut self.systems[start..i], &*world);
                }
            } else {
                // `Exclusive`: barrier serial (`&mut World`).
                self.systems[i].run(world);
                i += 1;
            }
        }
        self.apply_commands(world);
    }

    /// Meng-apply buffer command tiap sistem, **urutan registrasi**, di akhir
    /// run (RFC-0019). No-op bila tak ada `SharedCmd`.
    fn apply_commands(&mut self, world: &mut World) {
        for system in &mut self.systems {
            system.apply_commands(world);
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

    /// Graf-ketergantungan: untuk tiap sistem, indeks **pendahulu** (registrasi
    /// lebih awal) yang **berkonflik** dengannya (RFC-0018 §1).
    ///
    /// Sisi selalu mengarah dari registrasi lebih-awal ke lebih-akhir → pasangan
    /// berkonflik terurut deterministik. Sistem tanpa pendahulu berkonflik dapat
    /// mulai segera (tak terikat barrier stage).
    pub fn dependencies(&self) -> Vec<Vec<usize>> {
        let mut deps: Vec<Vec<usize>> = vec![Vec::new(); self.systems.len()];
        for (i, dep) in deps.iter_mut().enumerate() {
            let access_i = &self.systems[i].access;
            for (j, sys_j) in self.systems[..i].iter().enumerate() {
                if access_i.conflicts(&sys_j.access) {
                    dep.push(j);
                }
            }
        }
        deps
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

    #[test]
    fn each_cmd_spawn_via_run_serial() {
        let mut world = World::new();
        for i in 0..3 {
            let e = world.spawn();
            world.insert(e, Counter(i));
        }
        let mut s = Schedule::new();
        // Untuk tiap Counter, spawn entity baru dengan Tally(counter*10).
        s.add(System::each_cmd::<&Counter>(|c, cmd| {
            cmd.spawn().insert(Tally(c.0 * 10));
        }));
        s.run(&mut world);

        assert_eq!(world.query::<Tally>().count(), 3);
        let mut tallies: Vec<i32> = world.query::<Tally>().map(|t| t.0).collect();
        tallies.sort();
        assert_eq!(tallies, vec![0, 10, 20]);
    }

    #[test]
    fn each_cmd_run_parallel_setara_serial() {
        fn setup() -> World {
            let mut w = World::new();
            for i in 0..16 {
                let e = w.spawn();
                w.insert(e, Counter(i));
            }
            w
        }
        fn sched() -> Schedule {
            let mut s = Schedule::new();
            s.add(System::each_cmd::<&Counter>(|c, cmd| {
                cmd.spawn().insert(Tally(c.0));
            }));
            s
        }
        let mut serial = setup();
        sched().run(&mut serial);
        let mut parallel = setup();
        sched().run_parallel(&mut parallel);

        let mut ss: Vec<i32> = serial.query::<Tally>().map(|t| t.0).collect();
        ss.sort();
        let mut ps: Vec<i32> = parallel.query::<Tally>().map(|t| t.0).collect();
        ps.sort();
        assert_eq!(ss, ps);
        assert_eq!(ss.len(), 16);
    }

    // Uji **stress** STD-0006: berapa pun kali `run_parallel` dijalankan, hasilnya
    // WAJIB identik dengan `run` serial — mengguncang interleaving thread pada
    // eksekutor graf (RFC-0018). Kecil di bawah miri, besar di `cargo test`.
    #[test]
    fn stress_run_parallel_selalu_setara_serial() {
        #[derive(PartialEq, Debug, Clone, Copy)]
        struct A(i64);
        #[derive(PartialEq, Debug, Clone, Copy)]
        struct Bc(i64);
        #[derive(PartialEq, Debug, Clone, Copy)]
        struct Cc(i64);

        fn build(n: usize) -> World {
            let mut w = World::new();
            for i in 0..n {
                let e = w.spawn();
                w.insert(e, A(i as i64));
                w.insert(e, Bc(i as i64 + 1));
                w.insert(e, Cc(0));
            }
            w
        }
        // Schedule dengan graf-ketergantungan non-trivial (konflik campuran).
        fn sched() -> Schedule {
            let mut s = Schedule::new();
            s.add(System::each::<&mut A>(|a| a.0 = a.0.wrapping_add(1)));
            s.add(System::each::<&mut Bc>(|b| b.0 = b.0.wrapping_mul(3)));
            s.add(System::each::<(&A, &mut Cc)>(|(a, c)| {
                c.0 = c.0.wrapping_add(a.0)
            }));
            s.add(System::each::<&mut A>(|a| a.0 = a.0.wrapping_add(7)));
            s.add(System::each::<(&Cc, &mut Bc)>(|(c, b)| {
                b.0 = b.0.wrapping_add(c.0)
            }));
            s
        }
        fn snap(w: &World) -> Vec<(i64, i64, i64)> {
            let a: Vec<i64> = w.query::<A>().map(|x| x.0).collect();
            let b: Vec<i64> = w.query::<Bc>().map(|x| x.0).collect();
            let c: Vec<i64> = w.query::<Cc>().map(|x| x.0).collect();
            a.into_iter()
                .zip(b)
                .zip(c)
                .map(|((a, b), c)| (a, b, c))
                .collect()
        }

        let (n, iters) = if cfg!(miri) {
            (4usize, 3)
        } else {
            (64usize, 300)
        };

        let mut serial = build(n);
        sched().run(&mut serial);
        let expected = snap(&serial);

        for it in 0..iters {
            let mut par = build(n);
            sched().run_parallel(&mut par);
            assert_eq!(snap(&par), expected, "iterasi {it}: run_parallel != serial");
        }
    }

    #[test]
    fn each_cmd_despawn_self_via_entity_term() {
        use crate::Entity;
        let mut world = World::new();
        let mut ids = Vec::new();
        for i in 0..6 {
            let e = world.spawn();
            world.insert(e, Counter(i - 3)); // health: -3..2
            ids.push(e);
        }
        let mut s = Schedule::new();
        // Despawn entity yang "mati" (Counter <= 0), pakai handle Entity.
        s.add(System::each_cmd::<(Entity, &Counter)>(|(e, c), cmd| {
            if c.0 <= 0 {
                cmd.despawn(e);
            }
        }));
        s.run(&mut world);

        // Counter -3,-2,-1,0 → despawn (4); tersisa 1,2 (ids[4], ids[5]).
        assert!(!world.contains(ids[0]));
        assert!(!world.contains(ids[3]));
        assert!(world.contains(ids[4]));
        assert!(world.contains(ids[5]));
        assert_eq!(world.query::<Counter>().count(), 2);
    }

    #[test]
    fn each_cmd_despawn_self_run_parallel_setara_serial() {
        use crate::Entity;
        fn setup() -> World {
            let mut w = World::new();
            for i in 0..20 {
                let e = w.spawn();
                w.insert(e, Counter(i % 5)); // 0..4 berulang
            }
            w
        }
        fn sched() -> Schedule {
            let mut s = Schedule::new();
            s.add(System::each_cmd::<(Entity, &Counter)>(|(e, c), cmd| {
                if c.0 == 0 {
                    cmd.despawn(e);
                }
            }));
            s
        }
        let mut serial = setup();
        sched().run(&mut serial);
        let mut parallel = setup();
        sched().run_parallel(&mut parallel);

        let mut ss: Vec<i32> = serial.query::<Counter>().map(|c| c.0).collect();
        ss.sort();
        let mut ps: Vec<i32> = parallel.query::<Counter>().map(|c| c.0).collect();
        ps.sort();
        assert_eq!(ss, ps);
        assert_eq!(ss.len(), 16); // 4 dari 20 (Counter==0) ter-despawn
    }

    #[test]
    fn each_cmd_buffer_terkuras_antar_run() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Counter(0));
        let mut s = Schedule::new();
        s.add(System::each_cmd::<&Counter>(|_, cmd| {
            cmd.spawn().insert(Tally(1));
        }));
        s.run(&mut world);
        s.run(&mut world);
        // Tiap run cocok 1 Counter → spawn 1 Tally; 2 run → 2 (bukan menumpuk).
        assert_eq!(world.query::<Tally>().count(), 2);
    }

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

    // DAG: rantai konflik (0→1→2, semua tulis Counter) HARUS berurutan walau
    // paralel, sementara cabang independen (Tally) boleh bersamaan. Hasil == serial.
    #[test]
    fn run_parallel_rantai_konflik_setara_serial() {
        fn setup() -> World {
            let mut w = World::new();
            for i in 0..8 {
                let e = w.spawn();
                w.insert(e, Counter(i));
                w.insert(e, Tally(i));
            }
            w
        }
        fn sched() -> Schedule {
            let mut s = Schedule::new();
            s.add(System::each::<&mut Counter>(|c| c.0 += 1)); // 0
            s.add(System::each::<&mut Counter>(|c| c.0 *= 2)); // 1 (konflik 0)
            s.add(System::each::<&mut Counter>(|c| c.0 += 5)); // 2 (konflik 0,1)
            s.add(System::each::<&mut Tally>(|t| t.0 *= 3)); // 3 (independen)
            s
        }
        let mut serial = setup();
        sched().run(&mut serial);
        let mut parallel = setup();
        sched().run_parallel(&mut parallel);

        let sc: Vec<i32> = serial.query::<Counter>().map(|c| c.0).collect();
        let pc: Vec<i32> = parallel.query::<Counter>().map(|c| c.0).collect();
        assert_eq!(sc, pc); // ((i+1)*2)+5, terurut walau paralel
        let st: Vec<i32> = serial.query::<Tally>().map(|t| t.0).collect();
        let pt: Vec<i32> = parallel.query::<Tally>().map(|t| t.0).collect();
        assert_eq!(st, pt);
    }

    // DAG: sistem Exclusive (resource) di antara sistem Shared → segmentasi;
    // efek ketiganya benar & deterministik (== serial).
    #[test]
    fn run_parallel_segmentasi_exclusive_setara_serial() {
        fn setup() -> World {
            let mut w = World::new();
            w.insert_resource(Tally(0));
            for i in 0..6 {
                let e = w.spawn();
                w.insert(e, Counter(i));
            }
            w
        }
        fn sched() -> Schedule {
            let mut s = Schedule::new();
            s.add(System::each::<&mut Counter>(|c| c.0 += 1)); // Shared
            s.add(System::resource::<Tally>(|t| t.0 += 100)); // Exclusive (barrier)
            s.add(System::each::<&mut Counter>(|c| c.0 *= 2)); // Shared
            s
        }
        let mut serial = setup();
        sched().run(&mut serial);
        let mut parallel = setup();
        sched().run_parallel(&mut parallel);

        let sc: Vec<i32> = serial.query::<Counter>().map(|c| c.0).collect();
        let pc: Vec<i32> = parallel.query::<Counter>().map(|c| c.0).collect();
        assert_eq!(sc, pc);
        assert_eq!(parallel.resource::<Tally>(), Some(&Tally(100)));
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
    fn dependencies_hanya_pendahulu_yang_berkonflik() {
        let mut s = Schedule::new();
        s.add(System::each::<&mut Position>(|_| {})); // 0: tulis Position
        s.add(System::each::<&mut Velocity>(|_| {})); // 1: tulis Velocity (tak konflik 0)
        s.add(System::each::<&Position>(|_| {})); // 2: baca Position → konflik 0 saja

        let deps = s.dependencies();
        assert_eq!(deps[0], Vec::<usize>::new());
        assert_eq!(deps[1], Vec::<usize>::new());
        assert_eq!(deps[2], vec![0]); // bergantung 0, TIDAK 1
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
