//! Officina core: the session model, hook events, the quota contract, the
//! board computation and the relay's catalogue. Pure logic and file formats:
//! no filesystem, no process, no clock, no environment (AD-1, enforced by
//! this crate's `clippy.toml`).

/// Declares a fieldless enum together with an `ALL` constant listing every
/// variant, so coverage tests can iterate a list that cannot miss one.
///
/// When the enum's first attribute is `#[serde(rename_all = "...")]`, the
/// macro also generates `code()`, the name serde writes for each variant,
/// spelled at compile time from the same literal, so a message never needs a
/// serde round trip to name a variant. Defined before the modules so they
/// see it by textual scope.
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
pub mod quota;
pub mod relay;
mod wire;
