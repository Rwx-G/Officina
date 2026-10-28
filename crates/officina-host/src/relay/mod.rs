//! The relay's files and the window's authority (AD-21, AD-25).
//!
//! [`authority`] holds what only the author may decide. It is compiled only
//! with the `window-authority` feature, which `officina-app` enables and the
//! `officina` binary does not: the exe a session can call cannot approve.

pub mod approvals;
#[cfg(feature = "window-authority")]
pub mod authority;
pub mod credential;
pub mod doctor;
pub mod environment;
pub mod gate;
pub mod home;
pub mod http;
pub mod journal;
pub mod known_projects;
pub mod run;
pub mod tree;
pub mod view;
