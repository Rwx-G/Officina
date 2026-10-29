//! Officina host layer: every file, process and platform call (AD-1). The two
//! binaries are thin shells over this crate.

pub mod collector;
pub mod doctor;
pub mod files;
pub mod focus;
pub mod git;
pub mod paths;
pub mod platform;
pub mod process;
pub mod registry;
pub mod relay;
pub mod spawn;
#[cfg(feature = "window-authority")]
pub mod watch;
pub mod writers;
