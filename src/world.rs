//! Otoritas atas seluruh entity, komponen, dan resource (RFC-0002 §3.4).
//!
//! [`World`] adalah satu-satunya sumber kebenaran keadaan. Segala mutasi
//! bermuara padanya, sehingga sebuah snapshot atas `World` cukup untuk merekam
//! seluruh keadaan (invarian *kepemilikan & portabilitas data*).
//!
//! Isi struktur ini — tabel slot entity, registry komponen, dan penyimpanan
//! archetype — ditambahkan secara test-first selama Milestone M-1.

/// Wadah pemilik semua entity, komponen, dan resource.
///
/// Saat ini berupa kerangka; kemampuan spawn/despawn, insert/remove komponen,
/// dan query dikembangkan pada Milestone M-1 (lihat `docs/MILESTONE_1.md`).
#[derive(Default)]
pub struct World {}

impl World {
    /// Membuat `World` kosong.
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_baru_dapat_dibuat() {
        let _world = World::new();
    }
}
