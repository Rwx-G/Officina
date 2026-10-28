//! The window's registry (AD-38, AD-41, AD-42, D-47, D-91): what the
//! collector thread holds, whatever the open found. Only a writable
//! registry ingests or deletes; one written by a newer Officina, one that
//! could not be opened and one found damaged while the window runs
//! (unavailable until the next start) do neither, and the board says why.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use officina_core::agents::AgentRecord;
use officina_core::board::{Board, ProjectRule};
use officina_core::ipc::{IpcError, IpcErrorCode};
use officina_core::registry::confirm::AuthorAction;
use officina_core::registry::fold::{AgentsChanges, AgentsSnapshot};
use officina_core::registry::types::{RegistryReport, RegistryState, SubjectRef};
use officina_core::registry::view::{RegistryView, SubjectView, unread_view};

use super::ingest::{IngestContext, IngestReport};
use super::read::{self, Reader};
use super::retention::PassReport;
use super::store::{Integrity, Writer, WriterOpen, open_writer};
use super::write::{AuthorError, RegistryError};

#[derive(Debug)]
enum Slot {
    Writable(Writer),
    /// Written by a newer Officina.
    ReadOnly(Reader),
    /// Why it is not usable, until the next start.
    Unavailable(String),
    Network,
}

/// The registry as the window's collector thread holds it.
#[derive(Debug)]
pub struct WindowRegistry {
    slot: Slot,
    /// How the database came to be readable at open, when it was opened
    /// writable.
    integrity: Option<Integrity>,
    /// The last error that did not make the registry unavailable (a disk
    /// full, a busy file), cleared by the next success.
    last_error: Option<String>,
    /// The previous `claude agents` snapshot, in memory only (AD-46);
    /// seeded from the registry at the first refresh (D-51).
    agents: Option<AgentsSnapshot>,
}

impl WindowRegistry {
    /// Opens the registry of `officina_home` for the window (story 3.1).
    pub fn open(officina_home: &Path, onedrive_roots: &[PathBuf], now_ms: i64) -> WindowRegistry {
        WindowRegistry::from_open(open_writer(officina_home, onedrive_roots, now_ms))
    }

    /// The window's registry from what an open found.
    pub fn from_open(opened: WriterOpen) -> WindowRegistry {
        let (slot, integrity) = match opened {
            WriterOpen::Writable { writer, integrity } => (Slot::Writable(writer), Some(integrity)),
            WriterOpen::ReadOnly { reader } => (Slot::ReadOnly(reader), None),
            WriterOpen::Network => (Slot::Network, None),
            WriterOpen::Unavailable { error } => (Slot::Unavailable(error.to_string()), None),
        };
        WindowRegistry {
            slot,
            integrity,
            last_error: None,
            agents: None,
        }
    }

    /// How the database came to be readable at open.
    pub fn integrity(&self) -> Option<&Integrity> {
        self.integrity.as_ref()
    }

    /// The writer, while the registry is writable.
    pub fn writer(&self) -> Option<&Writer> {
        match &self.slot {
            Slot::Writable(writer) => Some(writer),
            _ => None,
        }
    }

    /// The subject of each filed session, by session id, while the
    /// registry is writable; empty otherwise, and the board says why
    /// (AD-54, D-47).
    pub fn card_subjects(&self) -> BTreeMap<String, SubjectRef> {
        self.writer()
            .and_then(|writer| read::card_subjects(&writer.conn).ok())
            .unwrap_or_default()
    }

    /// What the board says of the registry (`Diagnostics.registry`).
    pub fn report(&self) -> RegistryReport {
        let (state, detail, location) = match &self.slot {
            Slot::Writable(writer) => (
                RegistryState::Ready,
                self.last_error.clone(),
                Some(writer.location()),
            ),
            Slot::ReadOnly(reader) => (RegistryState::Newer, None, Some(reader.location())),
            Slot::Unavailable(reason) => (RegistryState::Unavailable, Some(reason.clone()), None),
            Slot::Network => (RegistryState::Network, None, None),
        };
        RegistryReport {
            state,
            detail,
            exclude_from_sync: location
                .and_then(|l| l.exclude())
                .map(|p| p.display().to_string()),
        }
    }

    /// Takes a writer error: one that says the database is damaged makes
    /// the registry unavailable until the next start (AD-42); any other is
    /// reported and the registry stays.
    fn failed(&mut self, error: &RegistryError) {
        if error.is_corruption() {
            let found = std::mem::replace(&mut self.slot, Slot::Unavailable(error.to_string()));
            if let Slot::Writable(writer) = found {
                writer.abandon();
            }
        } else {
            self.last_error = Some(error.to_string());
        }
    }

    /// Runs `step` on the writer, when there is one, and takes its error
    /// as [`WindowRegistry::failed`] says.
    fn on_writer<T>(
        &mut self,
        step: impl FnOnce(&mut Writer) -> Result<T, RegistryError>,
    ) -> Option<T> {
        let Slot::Writable(writer) = &mut self.slot else {
            return None;
        };
        match step(writer) {
            Ok(done) => {
                self.last_error = None;
                Some(done)
            }
            Err(error) => {
                self.failed(&error);
                None
            }
        }
    }

    /// Why an author action cannot be written now, from the registry's
    /// state: read only, unavailable or on a network folder.
    fn not_writable(&self) -> IpcError {
        let report = self.report();
        let detail = report
            .detail
            .map(|detail| format!(" ({detail})"))
            .unwrap_or_default();
        IpcError::new(
            IpcErrorCode::RegistryUnavailable,
            format!(
                "the registry cannot be written: it is {}{detail}",
                report.state.code()
            ),
        )
    }

    /// Runs one author action at `now_ms` on the window's one connection
    /// (AD-38): the subject it created, if any; a refusal names its rule
    /// and writes nothing (D-57).
    pub fn act(&mut self, action: &AuthorAction, now_ms: i64) -> Result<Option<i64>, IpcError> {
        let Slot::Writable(writer) = &mut self.slot else {
            return Err(self.not_writable());
        };
        match writer.author(action, now_ms) {
            Ok(created) => {
                self.last_error = None;
                Ok(created)
            }
            Err(AuthorError::Refused(refusal)) => Err(IpcError::refused_action(refusal)),
            Err(AuthorError::Registry(error)) => {
                let code = if error.is_corruption() {
                    IpcErrorCode::RegistryUnavailable
                } else {
                    IpcErrorCode::WriteFailed
                };
                let failure = IpcError::new(code, error.to_string());
                self.failed(&error);
                Err(failure)
            }
        }
    }

    /// The archived subjects the author may reopen (AD-54), with the live
    /// cards of `board`; read from the writer, or from the read-only
    /// connection of a registry written by a newer Officina.
    pub fn archived(&mut self, board: Option<&Board>) -> Result<Vec<SubjectView>, IpcError> {
        let read = match &self.slot {
            Slot::Writable(writer) => read::archived(&writer.conn, board),
            Slot::ReadOnly(reader) => reader.archived(board),
            Slot::Unavailable(_) | Slot::Network => return Err(self.not_writable()),
        };
        read.map_err(|error| {
            let failure = IpcError::new(IpcErrorCode::RegistryUnavailable, error.to_string());
            self.failed(&RegistryError::from(error));
            failure
        })
    }

    /// The Subjects view (AD-54), with the live cards of `board` and the
    /// lag behind the event files of `events_dir`; with nothing read, the
    /// view says why (D-47).
    pub fn view(&mut self, board: Option<&Board>, events_dir: &Path, now_ms: i64) -> RegistryView {
        let report = self.report();
        let read = match &self.slot {
            Slot::Writable(writer) => {
                read::view(&writer.conn, report.clone(), board, events_dir, now_ms)
            }
            Slot::ReadOnly(reader) => reader.view(report.clone(), board, events_dir, now_ms),
            Slot::Unavailable(_) | Slot::Network => return unread_view(report, now_ms),
        };
        match read {
            Ok(view) => view,
            Err(error) => {
                self.failed(&RegistryError::from(error));
                unread_view(self.report(), now_ms)
            }
        }
    }

    /// One ingestion pass, when the registry is writable.
    pub fn ingest(&mut self, ctx: &IngestContext, now_ms: i64) -> Option<IngestReport> {
        self.on_writer(|writer| writer.ingest(ctx, now_ms))
    }

    /// Folds a successful `claude agents` refresh into the registry, when
    /// it is writable (AD-46). Only call it after a refresh that succeeded:
    /// a failed one keeps the previous snapshot and writes nothing.
    pub fn apply_agents(
        &mut self,
        records: &[AgentRecord],
        projects: &[ProjectRule],
        now_ms: i64,
    ) -> Option<AgentsChanges> {
        let mut snapshot = self.agents.take();
        let applied =
            self.on_writer(|writer| writer.apply_agents(&mut snapshot, records, projects, now_ms));
        self.agents = snapshot;
        applied
    }

    /// The registry's hourly pass, when the registry is writable: read-only
    /// or unavailable, nothing is ingested and nothing deleted.
    pub fn hourly_pass(&mut self, ctx: &IngestContext, now_ms: i64) -> Option<PassReport> {
        self.on_writer(|writer| writer.hourly_pass(ctx, now_ms))
    }
}
