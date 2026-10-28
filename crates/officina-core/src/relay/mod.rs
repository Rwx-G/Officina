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

pub mod approvals;
pub mod argv;
pub mod catalogue;
pub mod classes;
pub mod gate;
pub mod journal;
pub mod params;
pub mod run;
pub mod shell;
pub mod view;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The Officina home's directory name under the user's home, when
/// `OFFICINA_HOME` does not set another one (AD-7).
pub const HOME_DIR_NAME: &str = ".officina";
/// The relay's directory under the Officina home (AD-25).
pub const RELAY_DIR: &str = "relay";
/// The journal's directory under the Officina home (AD-25, AD-32).
pub const JOURNAL_DIR: &str = "journal";

/// Content of `relay/stopped` (AD-25, AD-34), written by the window only.
/// The file's presence stops the relay, whatever it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopFlag {
    /// When the author stopped the relay, Unix milliseconds.
    pub since_ms: i64,
}

impl StopFlag {
    /// The flag's time, when its content is a well-formed flag.
    ///
    /// ```
    /// use officina_core::relay::StopFlag;
    /// assert_eq!(StopFlag::parse(br#"{"since_ms":5}"#), Some(StopFlag { since_ms: 5 }));
    /// assert_eq!(StopFlag::parse(b""), None);
    /// ```
    pub fn parse(content: &[u8]) -> Option<StopFlag> {
        serde_json::from_slice(content).ok()
    }
}

/// SHA-256 over bytes that arrive in pieces, as `officina run` hashes an
/// output stream (AD-31).
///
/// ```
/// use officina_core::relay::{StreamHash, sha256_hex};
/// let mut hash = StreamHash::new();
/// hash.update(b"ab");
/// hash.update(b"c");
/// assert_eq!(hash.hex(), sha256_hex(b"abc"));
/// ```
#[derive(Debug, Clone, Default)]
pub struct StreamHash(Sha256);

impl StreamHash {
    pub fn new() -> StreamHash {
        StreamHash(Sha256::new())
    }

    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    /// The hash of every byte so far, lowercase hex. The stream can go on.
    pub fn hex(&self) -> String {
        hex(&self.0.clone().finalize())
    }
}

fn hex(digest: &[u8]) -> String {
    use std::fmt::Write;
    digest
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// SHA-256 of `bytes`, lowercase hex.
///
/// ```
/// assert_eq!(
///     officina_core::relay::sha256_hex(b""),
///     "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
/// );
/// ```
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}
