//! Entitas sebagai *generational index* (RFC-0002 §1).
//!
//! Sebuah [`Entity`] adalah handle ringan: indeks slot di dalam sebuah `World`
//! ditambah nomor generasi. Ketika sebuah slot dipakai ulang setelah `despawn`,
//! generasinya dinaikkan, sehingga handle lama dengan generasi usang dapat
//! terdeteksi sebagai basi dan tidak pernah mengembalikan data entity lain
//! (invarian *struktural aman*, STD-0007).

/// Referensi stabil ke sebuah entity di dalam sebuah `World`.
///
/// Terdiri dari indeks slot (`u32`) dan nomor generasi (`u32`). Handle bersifat
/// `Copy` dan murah disalin; ia tidak memiliki data — validitasnya diperiksa
/// terhadap `World` yang menerbitkannya.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Entity {
    index: u32,
    generation: u32,
}

impl Entity {
    pub(crate) fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    /// Merekonstruksi handle dari nilai mentah `(index, generation)` — mis. saat
    /// **deserialisasi** relasi persisten (RFC-0031) di luar `World`.
    ///
    /// Handle hasil rekonstruksi bisa saja **basi** (slot mati/daur-ulang); itu
    /// **aman** — [`World::get`](crate::World::get) & kawan memvalidasi generation
    /// dan menolak handle basi (mengembalikan `None`, STD-0007). Sama seperti
    /// [`World::spawn_at`](crate::World::spawn_at), ini primitif rekonstruksi;
    /// pengguna umumnya memakai handle dari `spawn`.
    pub fn from_raw(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    /// Indeks slot entity ini di `World`-nya (mis. untuk persistensi eksternal,
    /// RFC-0021). Bersama [`Self::generation`] membentuk identitas stabil.
    pub fn index(self) -> u32 {
        self.index
    }

    /// Nomor generasi entity ini (naik saat slot dipakai ulang, STD-0007).
    pub fn generation(self) -> u32 {
        self.generation
    }
}
