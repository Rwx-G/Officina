//! What only the author may decide, from the window (AD-21): approving the
//! catalogue, approving or rejecting a `human` action's request, stopping
//! and resuming the relay, and acknowledging an anomaly. Each decision is
//! also kept in the window's record (`relay/decisions.json`, D-173), so one
//! the window did not make shows. Compiled only with the
//! `window-authority` feature, so the `officina` binary a session can call
//! holds none of it (P-5).

use std::io;
use std::path::Path;

use officina_core::relay::StopFlag;
use officina_core::relay::approvals::{DecideRefusal, Decision, FileKind, decidable, file_name};
use officina_core::relay::catalogue::{ApprovedCatalogue, KnownProjects};
use officina_core::relay::journal::{ApprovalEvent, CatalogueApproved, JournalEntry};
use officina_core::relay::record::DecisionRecord;
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

/// The window's record as it stands, or a new one taking the approved
/// catalogue and the stop flag as they are: what the window finds when it
/// first runs (D-173).
fn current_record(home: &RelayHome, now: i64) -> DecisionRecord {
    home.read_record().unwrap_or_else(|| {
        let stopped = super::view::read_stop(home);
        DecisionRecord::start(
            now,
            home.read_approved().approved().map(|a| a.hash.clone()),
            match stopped {
                officina_core::relay::view::StopState::Stopped { since_ms } => {
                    Some(since_ms.unwrap_or(now))
                }
                officina_core::relay::view::StopState::Running => None,
            },
        )
    })
}

/// Applies `change` to the window's record and writes it back, pruned.
/// Called under the journal lock, as every decision is.
fn update_record(
    home: &RelayHome,
    now: i64,
    change: impl FnOnce(&mut DecisionRecord),
) -> Result<(), AuthorityError> {
    let mut record = current_record(home, now);
    change(&mut record);
    record.prune(now);
    write_json(&home.decisions_file(), &record)
}

/// Starts the window's record when there is none (D-173): what the window
/// does as it opens, so later decisions are judged from then.
pub fn ensure_record(home: &RelayHome) -> Result<(), AuthorityError> {
    if home.read_record().is_some() {
        return Ok(());
    }
    let _journal =
        JournalLock::acquire(home).map_err(|e| AuthorityError::Journal(e.to_string()))?;
    update_record(home, now_ms(), |_| {})
}

/// Acknowledges the anomaly `key` (D-173): the Relay view keeps listing it
/// as acknowledged, and doctor no longer fails on it.
pub fn acknowledge_anomaly(home: &RelayHome, key: &str) -> Result<(), AuthorityError> {
    let _journal =
        JournalLock::acquire(home).map_err(|e| AuthorityError::Journal(e.to_string()))?;
    let now = now_ms();
    update_record(home, now, |record| {
        record.acknowledged.insert(key.to_owned(), now);
    })
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
    update_record(home, approved.approved_at_ms, |record| {
        record.approved_hash = Some(approved.hash.clone());
    })?;
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
        project: Some(request.project.clone()),
        action: Some(request.action.clone()),
        dry_run: None,
    };
    let (kind, line) = if approve {
        (FileKind::Approved, JournalEntry::ApprovalGiven(event))
    } else {
        (FileKind::Rejected, JournalEntry::ApprovalRejected(event))
    };
    // Recorded first: a decision file the record does not name is what an
    // anomaly is (D-173).
    update_record(home, now, |record| {
        record.decided.insert(id.to_owned(), now);
    })?;
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
    // Recorded first both ways: a record saying stopped while the flag is
    // gone is the anomaly, never the other way round (D-173).
    update_record(home, now, |record| {
        record.stopped_since_ms = stopped.then_some(now);
    })?;
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
