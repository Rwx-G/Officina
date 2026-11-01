//! Officina's Linux programs for the isolated workshop (AD-100):
//! `officina-gateway`, the egress proxy every worker reaches the outside
//! through, and `officina-entry`, the fixed code a workshop container runs.
//! Both are built for `x86_64-unknown-linux-musl` and depend on
//! `officina-core` and `serde_json` only: every rule they obey is decided in
//! core, so the window and the container cannot disagree on one.

pub mod gateway;
