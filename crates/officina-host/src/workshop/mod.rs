//! The isolated workshop on the host (AD-97): what the window, doctor and
//! the CLI read and run for it. Every rule lives in `officina_core::workshop`.

/// The folder of the Linux programs, for development and tests (AD-100);
/// the window's own is `<exe dir>/linux/`.
pub const ENV_LINUX_BIN_DIR: &str = "OFFICINA_LINUX_BIN_DIR";

pub mod doctor;
pub mod engine;
pub mod files;
pub mod git;
#[cfg(feature = "window-authority")]
pub mod threads;
