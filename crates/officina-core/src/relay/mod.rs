//! The relay (PRD v0.2, AD-21): the catalogue of closed actions a guarded
//! repository may run, its validation and canonical form, parameter values
//! and the argument vectors built from them. Pure: the host reads the files
//! and runs the programs.

/// Declares a fieldless enum together with an `ALL` constant listing every
/// variant, so coverage tests can iterate a list that cannot miss one.
/// Defined before the submodules so they see it by textual scope.
macro_rules! closed_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $( $(#[$variant_meta:meta])* $variant:ident, )+
        }
    ) => {
        $(#[$meta])*
        pub enum $name {
            $( $(#[$variant_meta])* $variant, )+
        }

        impl $name {
            /// Every variant, generated with the enum so it cannot miss one.
            pub const ALL: &[$name] = &[$($name::$variant),+];
        }
    };
}

pub mod argv;
pub mod catalogue;
pub mod gate;
pub mod journal;
pub mod params;
pub mod shell;

use sha2::{Digest, Sha256};

/// The Officina home's directory name under the user's home, when
/// `OFFICINA_HOME` does not set another one (AD-7).
pub const HOME_DIR_NAME: &str = ".officina";
/// The relay's directory under the Officina home (AD-25).
pub const RELAY_DIR: &str = "relay";
/// The journal's directory under the Officina home (AD-25, AD-32).
pub const JOURNAL_DIR: &str = "journal";

/// SHA-256 of `bytes`, lowercase hex.
///
/// ```
/// assert_eq!(
///     officina_core::relay::sha256_hex(b""),
///     "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
/// );
/// ```
pub fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}
