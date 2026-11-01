//! The isolated workshop on the host (AD-97): what the window, doctor and
//! the CLI read and run for it. Every rule lives in `officina_core::workshop`.

pub mod engine;
pub mod files;
pub mod git;
#[cfg(feature = "window-authority")]
pub mod threads;
