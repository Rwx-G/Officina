//! `relay/known-projects.json`, the gate's last readable state (D-15): the
//! window keeps it current, and a write that fails is journaled as a
//! `gate_error`, since a stale record is what a broken floor falls back to.

use std::collections::BTreeSet;
use std::io;

use officina_core::floor::parse_floor;
use officina_core::relay::catalogue::{KNOWN_PROJECTS_SCHEMA, KnownProjects, relay_in_use};
use officina_core::relay::journal::{GateError, GateFault, JournalEntry};

use super::home::RelayHome;
use super::journal;
use crate::files::atomic_write;

/// Keeps the record current: on every valid read of the relay floor
/// (phase 1) or of the snapshot, it holds the project paths of both and the
/// approved hash, the recorded one when the snapshot cannot be read, and is
/// written atomically when its content changes. Only while the relay is in
/// use (a valid snapshot, a record already there, or a floor that declares
/// an action or a guarded project): a record makes a broken floor guard
/// every known project, which a workstation without the relay must not get.
/// The error is the write's, when it fails.
pub fn keep(home: &RelayHome) -> io::Result<()> {
    let approved = home.read_approved();
    let floor_path = home.relay_floor(&approved);
    let floor = std::fs::read_to_string(&floor_path)
        .ok()
        .and_then(|content| parse_floor(&content).ok());
    let snapshot = approved.approved();
    let existing = home.read_known_projects();
    let existing = existing.valid();
    let floor_in_use = floor.as_ref().is_some_and(relay_in_use);
    if floor.is_none() && snapshot.is_none() {
        return Ok(());
    }
    if snapshot.is_none() && existing.is_none() && !floor_in_use {
        return Ok(());
    }
    let mut paths: BTreeSet<String> = floor
        .as_ref()
        .map(|f| KnownProjects::from_floor(f, &floor_path.to_string_lossy(), None).project_paths)
        .unwrap_or_default()
        .into_iter()
        .collect();
    if let Some(snapshot) = snapshot {
        paths.extend(snapshot.canonical.projects.values().map(|p| p.path.clone()));
    }
    let record = KnownProjects {
        schema: KNOWN_PROJECTS_SCHEMA,
        approved_hash: snapshot
            .map(|s| s.hash.clone())
            .or_else(|| existing.and_then(|k| k.approved_hash.clone())),
        project_paths: paths.into_iter().collect(),
    };
    if existing == Some(&record) {
        return Ok(());
    }
    let bytes = serde_json::to_vec_pretty(&record).map_err(io::Error::other)?;
    atomic_write(&home.known_projects_file(), &bytes)
}

/// What the window holds between two passes: the failure it last journaled,
/// so a record that cannot be written makes one line, not one per pass.
#[derive(Debug, Default)]
pub struct KnownProjectsKeeper {
    failing: Option<String>,
}

impl KnownProjectsKeeper {
    /// [`keep`], journaling a `known_projects_unwritable` gate error when a
    /// write first fails, or fails another way; a write that succeeds again
    /// ends the failure. Returns the failure, if one stands.
    pub fn keep(&mut self, home: &RelayHome, now_ms: i64) -> Option<&str> {
        match keep(home) {
            Ok(()) => self.failing = None,
            Err(e) => {
                let detail = format!("cannot write {}: {e}", home.known_projects_file().display());
                if self.failing.as_deref() != Some(&detail) {
                    let line = JournalEntry::GateError(GateError {
                        project: None,
                        session_id: None,
                        cause: GateFault::KnownProjectsUnwritable,
                        detail: detail.clone(),
                        command_excerpt: None,
                    });
                    let _ = journal::append(home, &[line], now_ms);
                }
                self.failing = Some(detail);
            }
        }
        self.failing.as_deref()
    }
}
