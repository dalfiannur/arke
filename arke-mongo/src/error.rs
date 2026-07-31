//! Error adapter, berkonteks (STD-0008): tiap varian menyebut entity dan/atau
//! komponen yang terlibat.

use crate::Pid;

/// Kegagalan operasi `arke-mongo`.
#[derive(Debug)]
pub enum MongoError {
    /// Kegagalan dari driver MongoDB.
    Driver(mongodb::error::Error),
    /// Versi dokumen bergeser sejak dibaca (optimistic-lock, RFC-0035 §5).
    Conflict {
        /// Entity yang gagal ditulis.
        pid: Pid,
        /// Versi yang diharapkan pemanggil.
        expected: i64,
        /// Versi yang sebenarnya ada; `None` bila dokumen sudah terhapus.
        ///
        /// Diperoleh lewat *round-trip kedua* ([`crate::MongoStore::version_of`])
        /// setelah `find_one_and_update` gagal cocok, bukan dibaca atomik
        /// bersama kegagalan itu — antara kedua round-trip, penulis lain bisa
        /// saja mengubah atau menghapus dokumennya lagi. Perlakukan ini
        /// sebagai observasi *belakangan* untuk membantu debugging/retry,
        /// bukan sebagai versi yang benar-benar menyebabkan konfliknya.
        actual: Option<i64>,
    },
    /// Sub-dokumen komponen tak bisa direkonstruksi menjadi tipe Rust-nya.
    Decode {
        /// Entity yang dokumennya gagal dibaca.
        pid: Pid,
        /// Nama komponen (`MongoComponent::NAME`) yang gagal.
        component: &'static str,
    },
    /// Nama field melanggar batas BSON (mengandung `.` atau berawalan `$`).
    InvalidName {
        /// Nama komponen pemilik field.
        component: &'static str,
        /// Nama field yang melanggar.
        field: String,
    },
    /// Dua field komponen memetakan ke nama BSON yang sama — dokumen akan
    /// kehilangan salah satunya secara diam-diam (mis. dua `#[arke(rename)]`
    /// yang bertabrakan).
    DuplicateField {
        /// Nama komponen pemilik field.
        component: &'static str,
        /// Nama BSON yang muncul lebih dari sekali.
        field: String,
    },
    /// Dokumen `pid` tak ada, sehingga tulisan tak mengenai apa pun. Dibedakan
    /// dari [`MongoError::Conflict`]: bukan versi yang bergeser, melainkan
    /// entity yang sudah tak ada (mis. dihapus penulis lain).
    Missing {
        /// Entity yang dokumennya tak ditemukan.
        pid: Pid,
    },
}

impl From<mongodb::error::Error> for MongoError {
    fn from(e: mongodb::error::Error) -> Self {
        MongoError::Driver(e)
    }
}

impl std::fmt::Display for MongoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MongoError::Driver(e) => write!(f, "kegagalan driver MongoDB: {e}"),
            MongoError::Conflict {
                pid,
                expected,
                actual,
            } => write!(
                f,
                "konflik versi pada entity {pid:?}: diharapkan {expected}, \
                 sebenarnya {actual:?}"
            ),
            MongoError::Decode { pid, component } => write!(
                f,
                "komponen `{component}` pada entity {pid:?} tak bisa \
                 direkonstruksi dari dokumen"
            ),
            MongoError::InvalidName { component, field } => write!(
                f,
                "field `{field}` pada komponen `{component}` bukan nama BSON \
                 yang sah (tak boleh mengandung `.` atau berawalan `$`)"
            ),
            MongoError::DuplicateField { component, field } => write!(
                f,
                "field `{field}` pada komponen `{component}` muncul lebih \
                 dari sekali setelah pemetaan — salah satunya akan hilang \
                 diam-diam di dalam `Document` (mis. dua `#[arke(rename)]` \
                 yang bertabrakan)"
            ),
            MongoError::Missing { pid } => write!(
                f,
                "dokumen entity {pid:?} tak ditemukan — tulisan tak mengenai \
                 apa pun (entity mungkin sudah dihapus penulis lain)"
            ),
        }
    }
}

impl std::error::Error for MongoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MongoError::Driver(e) => Some(e),
            _ => None,
        }
    }
}
