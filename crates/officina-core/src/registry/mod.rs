//! The registry's rules (PRD v0.3, AD-38): the durable memory of sessions,
//! subjects and verdicts. Pure, like the rest of the crate: the host opens
//! the SQLite file, reads the event files and passes `now_ms`; what is kept,
//! for how long and in which order is decided here.

pub mod confirm;
pub mod dry_run;
pub mod filing;
pub mod fold;
pub mod ingestion;
pub mod proposal;
pub mod retention;
pub mod triage_call;
pub mod types;
pub mod verdict;
pub mod view;
