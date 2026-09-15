//! Kolam thread pekerja **persisten** untuk jalur paralel (`Schedule::run_parallel`,
//! `World::par_for_each`).
//!
//! Sebelumnya tiap panggilan memakai `std::thread::scope` → spawn thread OS
//! tiap run (~15–20 µs per thread; 12 inti ≈ 160 µs per `run_parallel` walau
//! sistemnya kosong). Kolam ini menyimpan `available_parallelism` thread yang
//! **parkir** di `Condvar` dan menjalankan pekerjaan yang **meminjam** data
//! pemanggil (`&World`, `&mut System`, `&mut [T]`) — semantik `scope`, tanpa
//! spawn ulang.
//!
//! `unsafe` di modul ini **terkurung** pada satu `transmute` masa-hidup di
//! [`Pool::scope`]; sound karena `scope` **memblokir** sampai setiap pekerjaan
//! selesai (atau panic) sebelum kembali — data pinjaman pasti masih hidup
//! selama pekerjaan berjalan. Kolam dimiliki pemanggil (`Schedule`/`World`) dan
//! **di-join saat drop**, sehingga tak ada thread yang bocor melewati pemiliknya
//! (ramah miri). Diverifikasi miri di CI.

#![allow(unsafe_code)]

use std::any::Any;
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::JoinHandle;

/// Pekerjaan ter-erase masa-hidup yang siap dijalankan pekerja.
type Job = Box<dyn FnOnce() + Send + 'static>;

struct State {
    queue: VecDeque<Job>,
    shutdown: bool,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

/// Kolam thread pekerja; lihat dokumentasi modul.
pub(crate) struct Pool {
    shared: Arc<Shared>,
    workers: Vec<JoinHandle<()>>,
}

/// Penghitung pekerjaan tersisa satu `scope` + payload panic pertama.
struct Latch {
    state: Mutex<(usize, Option<Box<dyn Any + Send>>)>,
    done: Condvar,
}

impl Latch {
    fn finish(&self, panic: Option<Box<dyn Any + Send>>) {
        let mut st = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        st.0 -= 1;
        if st.1.is_none() {
            st.1 = panic;
        }
        drop(st);
        self.done.notify_all();
    }
}

impl Pool {
    /// Kolam dengan `threads` pekerja (min 1), parkir sampai ada pekerjaan.
    pub(crate) fn new(threads: usize) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                queue: VecDeque::new(),
                shutdown: false,
            }),
            wake: Condvar::new(),
        });
        let workers = (0..threads.max(1))
            .map(|_| {
                let shared = Arc::clone(&shared);
                std::thread::spawn(move || {
                    loop {
                        let job = {
                            let mut st =
                                shared.state.lock().unwrap_or_else(PoisonError::into_inner);
                            loop {
                                if let Some(job) = st.queue.pop_front() {
                                    break job;
                                }
                                if st.shutdown {
                                    return;
                                }
                                st = shared.wake.wait(st).unwrap_or_else(PoisonError::into_inner);
                            }
                        };
                        // Pekerjaan sudah membungkus `catch_unwind` (lihat `scope`)
                        // → pekerja tak pernah mati karena panic pengguna.
                        job();
                    }
                })
            })
            .collect();
        Self { shared, workers }
    }

    /// Menjalankan `jobs` di kolam dan **memblokir** sampai semuanya selesai.
    /// Panic di salah satu pekerjaan dipropagasi ulang ke pemanggil (payload
    /// pertama) **setelah** semua pekerjaan lain selesai — kolam tetap sehat.
    ///
    /// Jangan memanggil `scope` dari dalam pekerjaan kolam yang sama (semua
    /// pekerja bisa saling menunggu). Pemakaian di crate ini tak bersarang.
    pub(crate) fn scope<'a>(&self, jobs: Vec<Box<dyn FnOnce() + Send + 'a>>) {
        if jobs.is_empty() {
            return;
        }
        let latch = Arc::new(Latch {
            state: Mutex::new((jobs.len(), None)),
            done: Condvar::new(),
        });
        {
            let mut st = self
                .shared
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            for job in jobs {
                // SAFETY: masa-hidup `'a` dihapus agar pekerjaan bisa disimpan di
                // antrean `'static` milik pekerja. Fungsi ini **tidak kembali**
                // sebelum `latch` melapor semua pekerjaan selesai (lihat `wait` di
                // bawah — dijamin juga oleh `WaitOnDrop` bila unwind), dan
                // `latch.finish` dipanggil hanya setelah closure pekerjaan
                // dikonsumsi/di-drop. Maka setiap pinjaman `'a` di dalam pekerjaan
                // hidup lebih lama dari pekerjaan itu sendiri.
                let job: Job = unsafe {
                    std::mem::transmute::<
                        Box<dyn FnOnce() + Send + 'a>,
                        Box<dyn FnOnce() + Send + 'static>,
                    >(job)
                };
                let latch = Arc::clone(&latch);
                st.queue.push_back(Box::new(move || {
                    let result = catch_unwind(AssertUnwindSafe(job));
                    latch.finish(result.err());
                }));
            }
        }

        /// Menunggu latch saat drop — titik tunggu normal **dan** bila pemanggil
        /// unwind sebelum sempat menunggu: pekerjaan yang meminjam `'a` tak boleh
        /// hidup melewati fungsi ini dalam keadaan apa pun.
        struct WaitOnDrop<'l>(&'l Latch);
        impl Drop for WaitOnDrop<'_> {
            fn drop(&mut self) {
                let mut st = self.0.state.lock().unwrap_or_else(PoisonError::into_inner);
                while st.0 > 0 {
                    st = self.0.done.wait(st).unwrap_or_else(PoisonError::into_inner);
                }
            }
        }
        let waiter = WaitOnDrop(&latch);
        self.shared.wake.notify_all();
        drop(waiter);

        let panic = latch
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .1
            .take();
        if let Some(payload) = panic {
            resume_unwind(payload);
        }
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        {
            let mut st = self
                .shared
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            st.shutdown = true;
        }
        self.shared.wake.notify_all();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn scope_menjalankan_semua_dan_memblokir() {
        let pool = Pool::new(3);
        let counter = AtomicUsize::new(0);
        let jobs: Vec<Box<dyn FnOnce() + Send + '_>> = (0..10)
            .map(|_| {
                Box::new(|| {
                    counter.fetch_add(1, Ordering::Relaxed);
                }) as Box<dyn FnOnce() + Send + '_>
            })
            .collect();
        pool.scope(jobs);
        assert_eq!(counter.load(Ordering::Relaxed), 10);
    }

    #[test]
    fn panic_dipropagasi_dan_kolam_tetap_sehat() {
        let pool = Pool::new(2);
        let ran = AtomicUsize::new(0);
        let r = catch_unwind(AssertUnwindSafe(|| {
            pool.scope(vec![
                Box::new(|| {
                    if ran.load(Ordering::Relaxed) < usize::MAX {
                        panic!("boom");
                    }
                }) as Box<dyn FnOnce() + Send + '_>,
                Box::new(|| {
                    ran.fetch_add(1, Ordering::Relaxed);
                }),
            ]);
        }));
        assert!(r.is_err());
        assert_eq!(ran.load(Ordering::Relaxed), 1);
        // Kolam masih bisa dipakai setelah panic.
        pool.scope(vec![Box::new(|| {
            ran.fetch_add(1, Ordering::Relaxed);
        }) as Box<dyn FnOnce() + Send + '_>]);
        assert_eq!(ran.load(Ordering::Relaxed), 2);
    }
}
