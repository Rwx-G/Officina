//! The relay (PRD v0.2, AD-21): the catalogue of closed actions a guarded
//! repository may run, its validation, parameter values and the argument
//! vectors built from them. Pure: the host reads the files and runs the
//! programs.

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
pub mod params;
