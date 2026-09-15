//! Tipe error dengan konteks (RFC-0008).
//!
//! Setiap varian menyebut **nama tipe komponen** yang terlibat sehingga
//! kegagalan menjelaskan dirinya sendiri (Philosophy §3, STD-0008).

use std::fmt;

/// Kesalahan operasi `World` yang membawa konteks komponen.
///
/// `#[non_exhaustive]`: varian baru dapat ditambahkan tanpa perubahan mayor —
/// `match` di luar crate wajib punya lengan `_`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EcsError {
    /// Sebuah komponen diminta sebagai `&mut` bersama akses lain dalam satu query.
    QueryConflict {
        /// Nama tipe komponen yang beralias.
        component: &'static str,
    },
    /// Komponen tak terdaftar untuk operasi yang membutuhkannya (mis. snapshot).
    ComponentNotRegistered {
        /// Nama tipe komponen yang belum terdaftar.
        component: &'static str,
    },
    /// `schema_version` snapshot tak didukung `World` ini (mis. ditulis versi
    /// `arke` yang lebih baru).
    SchemaVersionUnsupported {
        /// Versi yang tertulis di snapshot.
        found: u32,
        /// Versi yang didukung.
        supported: u32,
    },
    /// Snapshot memuat komponen dengan kunci/nama yang tak terdaftar di `World`
    /// (`register_serializable`/`register_serializable_alias`).
    UnknownComponent {
        /// Kunci komponen sebagaimana tertulis di snapshot.
        component: String,
    },
    /// Nilai komponen di snapshot tak dapat di-decode (`from_value` → `None`).
    ComponentDecodeFailed {
        /// Kunci komponen sebagaimana tertulis di snapshot.
        component: String,
        /// Indeks slot entity pemilik nilai tersebut.
        index: u32,
    },
}

impl fmt::Display for EcsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EcsError::QueryConflict { component } => write!(
                f,
                "konflik query: komponen `{component}` diminta &mut bersama akses lain dalam satu query"
            ),
            EcsError::ComponentNotRegistered { component } => write!(
                f,
                "komponen `{component}` belum terdaftar untuk operasi ini (panggil register_serializable)"
            ),
            EcsError::SchemaVersionUnsupported { found, supported } => write!(
                f,
                "schema_version snapshot {found} tak didukung (didukung: {supported})"
            ),
            EcsError::UnknownComponent { component } => write!(
                f,
                "snapshot memuat komponen `{component}` yang tak terdaftar (register_serializable / register_serializable_alias)"
            ),
            EcsError::ComponentDecodeFailed { component, index } => write!(
                f,
                "nilai komponen `{component}` pada entity index {index} gagal di-decode"
            ),
        }
    }
}

impl std::error::Error for EcsError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_menyebut_nama_komponen() {
        let err = EcsError::ComponentNotRegistered {
            component: "game::Position",
        };
        assert!(err.to_string().contains("game::Position"));

        let conflict = EcsError::QueryConflict {
            component: "game::Velocity",
        };
        assert!(conflict.to_string().contains("game::Velocity"));
    }
}
