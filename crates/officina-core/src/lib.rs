//! Officina core: the session model, hook events, the quota contract, the
//! board computation, the relay's catalogue and the registry's rules. Pure
//! logic and file formats: no filesystem, no process, no clock, no
//! environment (AD-1, enforced by this crate's `clippy.toml`).

use sha2::{Digest, Sha256};

/// Declares a fieldless enum together with an `ALL` constant listing every
/// variant, so coverage tests can iterate a list that cannot miss one.
///
/// When the enum's first attribute is `#[serde(rename_all = "...")]`, the
/// macro also generates `code()`, the name serde writes for each variant,
/// spelled at compile time from the same literal, so a message never needs a
/// serde round trip to name a variant, and `from_code()`, its inverse.
/// Defined before the modules so they see it by textual scope; exported so
/// the host's enums (reader states, writer refusals) get an `ALL` too
/// (D-42).
///
/// ```
/// officina_core::closed_enum! {
///     #[serde(rename_all = "snake_case")]
///     #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
///     pub enum Light {
///         Green,
///         AmberFlashing,
///     }
/// }
/// assert_eq!(Light::ALL.len(), 2);
/// assert_eq!(Light::AmberFlashing.code(), "amber_flashing");
/// assert_eq!(Light::from_code("green"), Some(Light::Green));
/// assert_eq!(Light::from_code("Green"), None);
/// ```
#[macro_export]
macro_rules! closed_enum {
    (
        #[serde(rename_all = $case:literal)]
        $(#[$meta:meta])*
        pub enum $name:ident {
            $( $(#[$variant_meta:meta])* $variant:ident, )+
        }
    ) => {
        $(#[$meta])*
        #[serde(rename_all = $case)]
        pub enum $name {
            $( $(#[$variant_meta])* $variant, )+
        }

        impl $name {
            /// Every variant, generated with the enum so it cannot miss one.
            pub const ALL: &[$name] = &[$($name::$variant),+];

            /// The variant's name on the wire, as serde writes it.
            pub const fn code(self) -> &'static str {
                match self {
                    $( $name::$variant => {
                        const LEN: usize = $crate::wire::len(stringify!($variant), $case);
                        const BYTES: [u8; LEN] = $crate::wire::spell(stringify!($variant), $case);
                        const NAME: &str = $crate::wire::text(&BYTES);
                        NAME
                    } )+
                }
            }

            /// The variant whose wire name is `code`, exactly; `None` for
            /// any other text.
            pub fn from_code(code: &str) -> Option<$name> {
                $name::ALL.iter().copied().find(|variant| variant.code() == code)
            }
        }
    };
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

pub mod agents;
pub mod board;
pub mod contract;
pub mod dates;
pub mod events;
pub mod floor;
pub mod focus;
pub mod ipc;
pub mod mcp;
pub mod meeting;
pub mod orders;
pub mod permissions;
pub mod quota;
pub mod registry;
pub mod relay;
pub mod scene;
pub mod spawn;
pub mod untrusted;
// Public only so `closed_enum!` expands in other crates (`$crate::wire`).
#[doc(hidden)]
pub mod wire;

/// True when this build holds one of the mutants of AD-95 (D-148): core
/// features that each skip one check of the launch path, turned on only by
/// CI's `mutants` job to prove the injection battery catches them. The
/// window asserts it false at compile time, so no build of the app can
/// link one.
pub const MUTANTS_ENABLED: bool = cfg!(any(
    feature = "mutant-accept-any-value",
    feature = "mutant-accept-superseded",
    feature = "mutant-skip-prompt-hash",
));

pub(crate) fn hex(digest: &[u8]) -> String {
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
///     officina_core::sha256_hex(b""),
///     "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
/// );
/// ```
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}
