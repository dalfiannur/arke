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
