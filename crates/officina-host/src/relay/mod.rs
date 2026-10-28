//! The relay's files and the window's authority (AD-21, AD-25).
//!
//! [`authority`] holds what only the author may decide. It is compiled only
//! with the `window-authority` feature, which `officina-app` enables and the
//! `officina` binary does not: the exe a session can call cannot approve.

#[cfg(feature = "window-authority")]
pub mod authority;
pub mod gate;
pub mod home;
pub mod journal;
