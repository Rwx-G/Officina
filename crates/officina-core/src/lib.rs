//! Officina core: the session model, hook events, the quota contract and the
//! board computation. Pure logic and file formats: no filesystem, no process,
//! no clock, no environment (AD-1, enforced by this crate's `clippy.toml`).

pub mod agents;
pub mod board;
pub mod contract;
pub mod dates;
pub mod events;
pub mod floor;
pub mod focus;
pub mod quota;
