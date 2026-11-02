//! The isolated workshop (v0.6, AD-97): every rule a workshop container
//! obeys, decided in core before the engine sees it. Pure, like the rest of
//! the crate, and compiled for the musl target too.

pub mod battery;
pub mod bundle;
pub mod doctor;
pub mod engine;
pub mod floor;
pub mod gateway;
pub mod image;
pub mod inventory;
pub mod launch;
pub mod lifecycle;
pub mod names;
pub mod output;
pub mod proxy;
pub mod review;
pub mod run;
pub mod scan;
pub mod stream;
pub mod tar;
pub mod vector;

/// The workshop's folder under the Officina home (AD-124): run folders,
/// build contexts, the battery's mount and the empty hooks folder. A
/// guarded session's command naming it is denied (D-13).
pub const WORKSHOP_DIR: &str = "workshop";
