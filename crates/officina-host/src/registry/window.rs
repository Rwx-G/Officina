//! The window's registry (AD-38, AD-41, AD-42, D-47, D-91): what the
//! collector thread holds, whatever the open found. Only a writable
//! registry ingests or deletes; one written by a newer Officina, one that
//! could not be opened and one found damaged while the window runs
//! (unavailable until the next start) do neither, and the board says why.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use officina_core::agents::AgentRecord;
use officina_core::board::{Board, ProjectRule, SessionSettlement};
use officina_core::ipc::{IpcError, IpcErrorCode};
use officina_core::registry::confirm::{AuthorAction, ConfirmItem, ItemAnswer};
use officina_core::registry::dry_run::DryRunState;
use officina_core::registry::fold::{AgentsChanges, AgentsSnapshot};
use officina_core::registry::model::{EnableRefusal, ModelEnv};
use officina_core::registry::types::{RegistryReport, RegistryState, SubjectRef};
use officina_core::registry::view::{DiscardedSession, RegistryView, SubjectView, unread_view};

use super::ingest::{IngestContext, IngestReport};
use super::read::{self, Reader};
use super::retention::PassReport;
use super::store::{Writer, WriterOpen, open_writer};
use super::triage::{CallResult, DryRunRefusal, NextCall, Triage};
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
    /// The last error that did not make the registry unavailable (a disk
    /// full, a busy file), cleared by the next success.
    last_error: Option<String>,
    /// The previous `claude agents` snapshot, in memory only (AD-46);
    /// seeded from the registry at the first refresh (D-51).
    agents: Option<AgentsSnapshot>,
    /// The dry run running, and how the last one ended (AD-52).
    triage: Triage,
    /// Whether the model can be called, and the `claude` binary's stamp,
    /// as the collector last saw them (AD-51, D-66).
    model_env: ModelEnv,
}

impl WindowRegistry {
    /// Opens the registry of `officina_home` for the window (story 3.1).
    pub fn open(officina_home: &Path, onedrive_roots: &[PathBuf], now_ms: i64) -> WindowRegistry {
        WindowRegistry::from_open(open_writer(officina_home, onedrive_roots, now_ms))
    }

    /// The window's registry from what an open found.
    pub fn from_open(opened: WriterOpen) -> WindowRegistry {
        // How the database came to be readable is recorded by the open
        // itself (a restore writes its notice, D-93); the window keeps the
        // slot only.
        let slot = match opened {
            WriterOpen::Writable { writer, .. } => Slot::Writable(writer),
            WriterOpen::ReadOnly { reader } => Slot::ReadOnly(reader),
            WriterOpen::Network => Slot::Network,
            WriterOpen::Unavailable { error } => Slot::Unavailable(error.to_string()),
        };
        WindowRegistry {
            slot,
            last_error: None,
            agents: None,
            triage: Triage::default(),
            model_env: ModelEnv::default(),
        }
    }

    /// Records whether the triage model can be called and the `claude`
    /// binary's stamp, which every view, confirmation and call then reads
    /// (AD-51, D-66).
    pub fn set_model_env(&mut self, env: ModelEnv) {
        if let Slot::Writable(writer) = &mut self.slot {
            writer.set_model_env(env.clone());
        }
        self.model_env = env;
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
    /// (AD-54, D-47): a read that fails is taken as
    /// [`WindowRegistry::failed`] says, so the report names it.
    pub fn card_subjects(&mut self) -> BTreeMap<String, SubjectRef> {
        self.on_writer(|writer| Ok(read::card_subjects(&writer.conn)?))
            .unwrap_or_default()
    }

    /// What settles each ended session, by session id, while the registry
    /// is writable; empty otherwise (story 4.15), as [`Self::card_subjects`].
    pub fn card_settlements(&mut self) -> BTreeMap<String, SessionSettlement> {
        self.on_writer(|writer| Ok(read::card_settlements(&writer.conn)?))
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

    /// Runs one author command on the writer (quality audit M3): refused
    /// when the registry cannot be written; a success clears the last
    /// error; a failure is taken as [`WindowRegistry::failed`] says and
    /// told as [`WindowRegistry::access_failed`] words it. `step` answers
    /// a refusal of its own as the inner error.
    fn with_writer<T>(
        &mut self,
        step: impl FnOnce(&mut Writer) -> Result<Result<T, IpcError>, RegistryError>,
    ) -> Result<T, IpcError> {
        let Slot::Writable(writer) = &mut self.slot else {
            return Err(self.not_writable());
        };
        match step(writer) {
            Ok(answer) => {
                self.last_error = None;
                answer
            }
            Err(error) => Err(self.access_failed(&error, IpcErrorCode::WriteFailed)),
        }
    }

    /// What the window is told when the registry failed it: unavailable
    /// when the database is damaged (and so until the next start, AD-42),
    /// else `code`, the registry staying usable.
    fn access_failed(&mut self, error: &RegistryError, code: IpcErrorCode) -> IpcError {
        let code = if error.is_corruption() {
            IpcErrorCode::RegistryUnavailable
        } else {
            code
        };
        let failure = IpcError::new(code, error.to_string());
        self.failed(error);
        failure
    }

    /// Runs one author action at `now_ms` on the window's one connection
    /// (AD-38): the subject it created, if any; a refusal names its rule
    /// and writes nothing (D-57).
    pub fn act(&mut self, action: &AuthorAction, now_ms: i64) -> Result<Option<i64>, IpcError> {
        self.with_writer(|writer| match writer.author(action, now_ms) {
            Ok(created) => Ok(Ok(created)),
            Err(AuthorError::Refused(refusal)) => Ok(Err(IpcError::refused_action(refusal))),
            Err(AuthorError::Registry(error)) => Err(error),
        })
    }

    /// Confirms triage rows at `now_ms` on the window's one connection
    /// (AD-38, AD-49), answered per item; the registry failing writes
    /// nothing and says why, as an author action does.
    pub fn confirm(
        &mut self,
        items: &[ConfirmItem],
        now_ms: i64,
    ) -> Result<Vec<ItemAnswer>, IpcError> {
        self.with_writer(|writer| Ok(Ok(writer.confirm_triage(items, now_ms)?)))
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
        match read {
            Ok(listed) => {
                self.last_error = None;
                Ok(listed)
            }
            Err(error) => {
                Err(self.access_failed(&RegistryError::from(error), IpcErrorCode::ReadFailed))
            }
        }
    }

    /// The sessions the author discarded from the board, the latest first
    /// (story 4.15), for the archived list's Restore.
    pub fn discarded(&mut self) -> Result<Vec<DiscardedSession>, IpcError> {
        let read = match &self.slot {
            Slot::Writable(writer) => read::discarded(&writer.conn),
            Slot::ReadOnly(reader) => reader.discarded(),
            Slot::Unavailable(_) | Slot::Network => return Err(self.not_writable()),
        };
        match read {
            Ok(listed) => {
                self.last_error = None;
                Ok(listed)
            }
            Err(error) => {
                Err(self.access_failed(&RegistryError::from(error), IpcErrorCode::ReadFailed))
            }
        }
    }

    /// The Subjects view (AD-54), with the live cards of `board`, the lag
    /// behind the event files of `events_dir` and the dry run running; with
    /// nothing read, the view says why (D-47).
    pub fn view(&mut self, board: Option<&Board>, events_dir: &Path, now_ms: i64) -> RegistryView {
        let report = self.report();
        let dry_run = self.triage.state();
        let ctx = read::ViewContext {
            report: report.clone(),
            board,
            events_dir,
            dry_run,
            unavailable: self.model_env.unavailable,
            no_excerpts: self.model_env.no_excerpts,
        };
        let read = match &self.slot {
            Slot::Writable(writer) => read::view(&writer.conn, ctx, now_ms),
            Slot::ReadOnly(reader) => reader.view(ctx, now_ms),
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

    /// Turns auto-archive on or off at `now_ms` (FR-12): refused, with the
    /// gate's state, unless its gate holds to turn it on.
    pub fn set_auto_archive(&mut self, on: bool, now_ms: i64) -> Result<(), IpcError> {
        self.with_writer(|writer| {
            Ok(writer.set_auto_archive(on, now_ms)?.map_err(|state| {
                IpcError::new(
                    IpcErrorCode::Refused,
                    format!(
                        "auto-archive cannot be turned on: its gate does not hold ({})",
                        state.code()
                    ),
                )
            }))
        })
    }

    /// Turns the triage model on or off at `now_ms` (FR-10 as amended,
    /// D-69, D-81): refused while it cannot be called or its enable gate
    /// does not hold, with why; turning it on lifts a pause.
    pub fn set_triage_model(&mut self, on: bool, now_ms: i64) -> Result<(), IpcError> {
        self.with_writer(|writer| {
            Ok(writer
                .set_triage_model(on, now_ms)?
                .map_err(|refusal: EnableRefusal| {
                    IpcError::new(IpcErrorCode::Refused, refusal.to_string())
                }))
        })
    }

    /// One auto-archive pass at `now_ms`, when the registry is writable:
    /// true when it archived a subject or turned itself off.
    pub fn auto_archive(&mut self, now_ms: i64) -> bool {
        self.on_writer(|writer| writer.auto_archive(now_ms))
            .is_some_and(|run| !run.archived.is_empty() || run.turned_off.is_some())
    }

    /// What the view shows of dry runs.
    pub fn triage_state(&self) -> DryRunState {
        self.triage.state()
    }

    /// Queues a dry run at `now_ms` (AD-52, D-92): answered once queued,
    /// with how many cases it holds; refused while one runs, with no case,
    /// or when the registry cannot be written.
    pub fn evaluate(&mut self, now_ms: i64) -> Result<u32, IpcError> {
        let mut triage = std::mem::take(&mut self.triage);
        let queued = self.with_writer(|writer| {
            Ok(triage
                .start_dry_run(writer, now_ms)?
                .map_err(|refusal: DryRunRefusal| {
                    IpcError::new(IpcErrorCode::Refused, refusal.to_string())
                }))
        });
        self.triage = triage;
        queued
    }

    /// The next triage call to start at `now_ms`, when one may (AD-52).
    pub fn next_call(&mut self, now_ms: i64) -> Option<NextCall> {
        let mut triage = std::mem::take(&mut self.triage);
        let next = self
            .on_writer(|writer| triage.next_call(writer, now_ms))
            .flatten();
        self.triage = triage;
        next
    }

    /// The `triage` thread is gone, at `now_ms` (quality audit H1): see
    /// `Triage::worker_gone`.
    pub fn triage_worker_gone(&mut self, now_ms: i64) {
        let mut triage = std::mem::take(&mut self.triage);
        self.on_writer(|writer| triage.worker_gone(writer, now_ms));
        self.triage = triage;
    }

    /// Records a call's result at `now_ms`, when the registry is writable.
    pub fn take_call_result(&mut self, result: &CallResult, now_ms: i64) {
        let mut triage = std::mem::take(&mut self.triage);
        self.on_writer(|writer| triage.take_result(writer, result, now_ms));
        self.triage = triage;
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
