//! Tipe error dengan konteks (RFC-0008).
//!
//! Setiap varian menyebut **nama tipe komponen** yang terlibat sehingga
//! kegagalan menjelaskan dirinya sendiri (Philosophy §3, STD-0008).

use std::fmt;

/// Kesalahan operasi `World` yang membawa konteks komponen.
#[derive(Debug, Clone, PartialEq, Eq)]
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
