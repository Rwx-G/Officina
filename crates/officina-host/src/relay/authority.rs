//! What only the author may decide, from the window (AD-21): approving the
//! catalogue, approving or rejecting a `human` action's request, stopping
//! and resuming the relay. Compiled only with the
//! `window-authority` feature, so the `officina` binary a session can call
//! holds none of it (P-5).

use std::io;
use std::path::Path;

use officina_core::relay::StopFlag;
use officina_core::relay::approvals::{DecideRefusal, Decision, FileKind, decidable, file_name};
use officina_core::relay::catalogue::{ApprovedCatalogue, KnownProjects};
use officina_core::relay::journal::{ApprovalEvent, CatalogueApproved, JournalEntry};
use serde::Serialize;

use super::approvals::{self, ContextSources};
use super::home::{RelayFloorError, RelayHome, load_relay_floor};
use super::journal::JournalLock;
use crate::files::{atomic_write, now_ms};

/// Text every authority error carries. CI searches the release binaries for
/// it: absent from `officina.exe`, present in `officina-app.exe` (D-9). It
/// lives in the error type the authority functions return, so `lto` and
/// `strip` keep it in any binary that calls them.
pub const AUTHORITY_MARKER: &str = "officina-window-authority";

/// Why the window's authority refused or failed. Nothing was approved.
#[derive(Debug, thiserror::Error)]
pub enum AuthorityError {
    #[error("{AUTHORITY_MARKER}: {0}")]
    Floor(RelayFloorError),
    #[error(
        "{AUTHORITY_MARKER}: the catalogue changed since it was displayed (displayed {displayed}, current {current}); review the new diff"
    )]
    CatalogueChanged { displayed: String, current: String },
    #[error("{AUTHORITY_MARKER}: cannot write {path}: {error}")]
    Write { path: String, error: String },
    /// The journal could not be locked (nothing was approved), or the
    /// approval was written and its line could not be.
    #[error("{AUTHORITY_MARKER}: journal: {0}")]
    Journal(String),
    /// The request cannot be decided: unknown, decided, expired or invalid
    /// (AD-35, D-27).
    #[error("{AUTHORITY_MARKER}: {0}")]
    Decide(DecideRefusal),
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), AuthorityError> {
    let failed = |error: String| AuthorityError::Write {
        path: path.display().to_string(),
        error,
    };
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| failed(e.to_string()))?;
    atomic_write(path, &bytes).map_err(|e| failed(e.to_string()))
}

/// Approves the relay's catalogue if it is still the one the author saw.
///
/// Resolves the relay floor (AD-24, D-8), recomputes its hash, and writes
/// `relay/approved.json` atomically only when it equals `displayed_hash`: a
/// floor edited between display and click is not approved. Invalid actions
/// do not block approval; they are not part of the catalogue.
///
/// `relay/known-projects.json` is written first (D-15). If the approval
/// write then fails, that record names a hash with no `approved.json`
/// behind it, which the gate reads as a broken snapshot and guards every
/// project: a failure can only tighten the gate.
///
/// The journal lock is taken before anything is read and held until the
/// `catalogue_approved` line (AD-32) is written, so an approval and its line
/// go together: a journal that stays locked approves nothing.
pub fn approve_catalogue(
    home: &RelayHome,
    displayed_hash: &str,
) -> Result<ApprovedCatalogue, AuthorityError> {
    let journal = JournalLock::acquire(home).map_err(|e| AuthorityError::Journal(e.to_string()))?;
    let previous = home.read_approved();
    let floor_path = home.relay_floor(&previous);
    let current = load_relay_floor(&floor_path).map_err(AuthorityError::Floor)?;
    if current.hash != displayed_hash {
        return Err(AuthorityError::CatalogueChanged {
            displayed: displayed_hash.to_owned(),
            current: current.hash,
        });
    }
    let known = KnownProjects::from_floor(
        &current.floor,
        &floor_path.to_string_lossy(),
        Some(current.hash.clone()),
    );
    write_json(&home.known_projects_file(), &known)?;
    let approved = ApprovedCatalogue {
        approved_at_ms: now_ms(),
        hash: current.hash,
        floor_file: Some(floor_path.to_string_lossy().into_owned()),
        canonical: current.canonical,
    };
    write_json(&home.approved_file(), &approved)?;
    let line = JournalEntry::CatalogueApproved(CatalogueApproved {
        hash: approved.hash.clone(),
        previous_hash: previous.approved().map(|p| p.hash.clone()),
    });
    journal
        .append(&[line], approved.approved_at_ms)
        .map_err(|e| AuthorityError::Journal(e.to_string()))?;
    Ok(approved)
}

/// Approves or rejects request `id` (FR-9, AD-33, AD-35): writes
/// `<id>.approved.json` or `<id>.rejected.json` (`decided_at_ms`,
/// `request_key`) atomically and journals `approval_given` or
/// `approval_rejected`. Refused, writing nothing, for a request that is
/// unknown, already decided, expired, or that does not rebuild from the
/// approved catalogue, a request of another catalogue included (D-27).
///
/// The journal lock is held across the check, the decision and its line, so
/// two clicks decide once.
pub fn decide_approval(home: &RelayHome, id: &str, approve: bool) -> Result<(), AuthorityError> {
    let journal = JournalLock::acquire(home).map_err(|e| AuthorityError::Journal(e.to_string()))?;
    let files = approvals::read_files(home);
    let sources = ContextSources::read(home);
    let now = now_ms();
    let request = decidable(&files, &sources.context(), id, now).map_err(AuthorityError::Decide)?;
    let event = ApprovalEvent {
        approval_id: id.to_owned(),
        request_key: request.request_key.clone(),
    };
    let (kind, line) = if approve {
        (FileKind::Approved, JournalEntry::ApprovalGiven(event))
    } else {
        (FileKind::Rejected, JournalEntry::ApprovalRejected(event))
    };
    write_json(
        &home.approvals_dir().join(file_name(id, kind)),
        &Decision {
            decided_at_ms: now,
            request_key: request.request_key.clone(),
        },
    )?;
    journal
        .append(&[line], now)
        .map_err(|e| AuthorityError::Journal(e.to_string()))
}

/// Stops or resumes the relay (FR-11, AD-34, D-26): stopping writes
/// `relay/stopped` (`{"since_ms":...}`) atomically and journals
/// `relay_stopped`; resuming deletes it and journals `relay_resumed`. A call
/// that changes nothing (stopping a stopped relay) writes and journals
/// nothing. Returns whether the state changed.
///
/// The journal lock is held across the check, the change and its line, so
/// two calls at once journal one change.
pub fn set_relay_stopped(home: &RelayHome, stopped: bool) -> Result<bool, AuthorityError> {
    let journal = JournalLock::acquire(home).map_err(|e| AuthorityError::Journal(e.to_string()))?;
    let flag = home.stopped_file();
    if home.relay_stopped() == stopped {
        return Ok(false);
    }
    let now = now_ms();
    let line = if stopped {
        write_json(&flag, &StopFlag { since_ms: now })?;
        JournalEntry::RelayStopped
    } else {
        match std::fs::remove_file(&flag) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(AuthorityError::Write {
                    path: flag.display().to_string(),
                    error: e.to_string(),
                });
            }
        }
        JournalEntry::RelayResumed
    };
    journal
        .append(&[line], now)
        .map_err(|e| AuthorityError::Journal(e.to_string()))?;
    Ok(true)
}
