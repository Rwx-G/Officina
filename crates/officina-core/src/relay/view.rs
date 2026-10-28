//! The relay's state as the window and `officina relay --json` show it
//! (AD-35, D-24): one [`RelayView`], built from what the host read. Separate
//! from the board so the journal tail never churns the board contract.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::catalogue::{
    ActionIssue, ApprovedCatalogue, CANONICAL_FORMAT, CanonicalCatalogue, CatalogueDiff, Notice,
    RelayFloor, TIMEOUT_RANGE_S, diff, notices, relay_in_use,
};
use super::journal::{
    ApprovalEvent, CatalogueApproved, GateDenied, GateError, JournalEntry, JournalLine, Refused,
    RunFinished, RunStarted,
};
use super::record::Anomaly;
use super::run::SnapshotState;

/// Version of the relay view contract.
pub const RELAY_VIEW_SCHEMA: u32 = 1;
/// How many entries [`RelayView::recent`] keeps, a run counting once.
pub const RECENT_ENTRIES: usize = 50;
/// A run with no end line this long past its timeout reads abandoned
/// (AD-32).
pub const ABANDONED_GRACE_MS: i64 = 60_000;
/// The window over which gate errors are counted.
pub const GATE_ERRORS_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;

/// A catalogue change waiting for the author: the hash the Approve button
/// sends (AD-24), what changed since the approved snapshot, and the
/// catalogue it would approve, so an added action shows what it runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PendingChange {
    pub hash: String,
    pub diff: CatalogueDiff,
    pub current: CanonicalCatalogue,
}

/// Where a run stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    /// Started, no end line yet, still within its timeout plus the grace.
    Running,
    Finished,
    /// No end line past its timeout plus [`ABANDONED_GRACE_MS`]: killed
    /// with its caller, or the end line was lost (AD-32).
    Abandoned,
}

/// One run, its start and end lines paired by `run_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RunView {
    pub state: RunState,
    pub started: RunStarted,
    pub finished: Option<RunFinished>,
    pub finished_at_ms: Option<i64>,
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// How a recent entry ended, for the window's colour (AD-35): computed
    /// here, so the window holds no relay rule.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum Tone {
        /// A run that did what it was asked ([`RunFinished::succeeded`]).
        Ok,
        /// A run that did not, a run abandoned, or a gate error.
        Failed,
        /// A refusal of `officina run` or a deny of the gate.
        Refused,
    }
}

/// The tone of an entry; `None` for one that is no outcome (an approval, a
/// stop) or a run still going.
pub fn tone_of(event: &RecentEvent) -> Option<Tone> {
    match event {
        RecentEvent::Run(run) => match (run.state, &run.finished) {
            (RunState::Finished, Some(finished)) if finished.succeeded() => Some(Tone::Ok),
            (RunState::Finished | RunState::Abandoned, _) => Some(Tone::Failed),
            (RunState::Running, _) => None,
        },
        RecentEvent::Refused(_) | RecentEvent::GateDenied(_) => Some(Tone::Refused),
        RecentEvent::GateError(_) => Some(Tone::Failed),
        RecentEvent::CatalogueApproved(_)
        | RecentEvent::ApprovalRequested(_)
        | RecentEvent::ApprovalGiven(_)
        | RecentEvent::ApprovalRejected(_)
        | RecentEvent::RelayStopped
        | RecentEvent::RelayResumed => None,
    }
}

/// What one recent entry records: a journal line, runs paired.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecentEvent {
    /// Boxed: a run carries far more than any other entry.
    Run(Box<RunView>),
    Refused(Refused),
    GateDenied(GateDenied),
    GateError(GateError),
    CatalogueApproved(CatalogueApproved),
    ApprovalRequested(ApprovalEvent),
    ApprovalGiven(ApprovalEvent),
    ApprovalRejected(ApprovalEvent),
    RelayStopped,
    RelayResumed,
}

/// One entry of the recent activity: when it happened (a run's start) and
/// what.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RecentEntry {
    /// Unix milliseconds.
    pub ts: i64,
    pub event: RecentEvent,
    /// How it ended ([`tone_of`]).
    pub tone: Option<Tone>,
}

/// A credential of the approved catalogue (AD-36, D-41): where it may go,
/// which actions send it, and whether the Credential Manager holds it. Never
/// its secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CredentialView {
    pub name: String,
    pub hosts: Vec<String>,
    /// The approved actions naming it, sorted.
    pub actions: Vec<String>,
    /// Whether `officina:<name>` is in the Credential Manager; `null` when
    /// not checked: `officina relay --json` never reads the store, the
    /// window checks on its own schedule (D-41).
    pub present: Option<bool>,
}

/// The relay contract shared by `officina relay --json` and the window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RelayView {
    pub schema: u32,
    /// True while `relay/stopped` exists or cannot be looked at (AD-34).
    pub stopped: bool,
    /// When the author stopped the relay, when the flag says so.
    pub stopped_since_ms: Option<i64>,
    /// The relay floor's path as read (AD-24), the one approval hashes
    /// (D-8).
    pub floor: Option<String>,
    /// Why the relay floor cannot be used, when it exists but cannot be read
    /// or fails phase 1, or is gone while a catalogue is approved.
    pub floor_error: Option<String>,
    pub approved_hash: Option<String>,
    pub approved_at_ms: Option<i64>,
    /// Why `relay/approved.json` cannot be read, if it cannot.
    pub snapshot_error: Option<String>,
    /// The approved snapshot (D-24): what `officina run` serves.
    pub catalogue: Option<CanonicalCatalogue>,
    /// The current floor's differences from the approved snapshot, when they
    /// mean something different (D-24).
    pub pending: Option<PendingChange>,
    /// Phase 2 problems of the current floor, with their lines (AD-23).
    pub issues: Vec<ActionIssue>,
    /// The last [`RECENT_ENTRIES`] entries of the two days read, newest
    /// first.
    pub recent: Vec<RecentEntry>,
    pub journal_corrupt_lines: u32,
    /// The gate errors of the last `gate_errors_window_ms`.
    pub gate_errors: u32,
    /// The window `gate_errors` covers: [`GATE_ERRORS_WINDOW_MS`], carried
    /// so the window says it without restating it.
    pub gate_errors_window_ms: i64,
    /// The approved catalogue's credentials, by name.
    pub credentials: Vec<CredentialView>,
    /// What the programs do with some values (D-172): the current floor's
    /// notices, or the approved snapshot's when the floor cannot be read.
    pub notices: Vec<Notice>,
    /// Decisions the window did not make (D-173), acknowledged or not.
    pub anomalies: Vec<Anomaly>,
}

/// `relay/stopped` as the host found it (AD-34).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopState {
    Running,
    /// `since_ms` when the flag is readable and well formed.
    Stopped {
        since_ms: Option<i64>,
    },
}

/// The relay floor as the host found it.
#[derive(Debug, Clone, Copy)]
pub enum FloorRead<'a> {
    /// No file at the resolved path.
    Missing,
    /// Why it exists but cannot be read, or fails phase 1.
    Unreadable(&'a str),
    Valid(&'a RelayFloor),
}

/// Everything [`build`] needs, read by the host.
#[derive(Debug, Clone, Copy)]
pub struct ViewInputs<'a> {
    pub stop: StopState,
    pub snapshot: SnapshotState<'a>,
    /// The resolved relay floor path (AD-24).
    pub floor_path: &'a str,
    pub floor: FloorRead<'a>,
    /// The journal lines of the last two days, in file order.
    pub lines: &'a [JournalLine],
    pub journal_corrupt_lines: u32,
    pub now_ms: i64,
    /// Whether each credential is in the Credential Manager, by name, when
    /// the caller checked; `None` reports every credential unchecked.
    pub credential_presence: Option<&'a BTreeMap<String, bool>>,
    /// What `relay::record::anomalies` found.
    pub anomalies: &'a [Anomaly],
}

/// What a first approval compares with: nothing approved.
fn nothing_approved() -> CanonicalCatalogue {
    CanonicalCatalogue {
        format: CANONICAL_FORMAT,
        floor_path: String::new(),
        guarded_programs: Vec::new(),
        projects: BTreeMap::new(),
        actions: BTreeMap::new(),
        credentials: BTreeMap::new(),
    }
}

/// The pending change (D-24): the current floor against the approved
/// snapshot when their hashes differ; against nothing when no snapshot is
/// readable, as long as the floor asks something of the relay
/// ([`relay_in_use`]) or a broken
/// snapshot needs a new approval.
fn pending(snapshot: SnapshotState<'_>, current: &RelayFloor) -> Option<PendingChange> {
    let base = match snapshot {
        SnapshotState::Valid(approved) if approved.hash == current.hash => return None,
        SnapshotState::Valid(approved) => approved.canonical.clone(),
        SnapshotState::Missing if !relay_in_use(&current.floor) => return None,
        SnapshotState::Missing | SnapshotState::Unreadable(_) => nothing_approved(),
    };
    Some(PendingChange {
        hash: current.hash.clone(),
        diff: diff(&base, &current.canonical),
        current: current.canonical.clone(),
    })
}

/// The approved catalogue's credentials with the actions naming them and
/// what the caller found in the store.
fn credentials(
    approved: Option<&ApprovedCatalogue>,
    presence: Option<&BTreeMap<String, bool>>,
) -> Vec<CredentialView> {
    let Some(approved) = approved else {
        return Vec::new();
    };
    let canonical = &approved.canonical;
    canonical
        .credentials
        .iter()
        .map(|(name, credential)| CredentialView {
            name: name.clone(),
            hosts: credential.hosts.clone(),
            actions: canonical
                .actions
                .iter()
                .filter(|(_, action)| {
                    action
                        .http
                        .as_ref()
                        .is_some_and(|http| http.credential.as_deref() == Some(name))
                })
                .map(|(action, _)| action.clone())
                .collect(),
            present: presence.and_then(|found| found.get(name).copied()),
        })
        .collect()
}

/// Pairs run lines and keeps the last [`RECENT_ENTRIES`] entries, newest
/// first by time, file order among equal times. A run is placed at its
/// start; an end line whose start is older than the lines read is dropped. A run without an end reads abandoned
/// once its timeout (the longest one for lines that do not record it) plus
/// [`ABANDONED_GRACE_MS`] has passed.
fn recent(lines: &[JournalLine], now_ms: i64) -> Vec<RecentEntry> {
    let ends: BTreeMap<&str, (&RunFinished, i64)> = lines
        .iter()
        .filter_map(|line| match &line.entry {
            JournalEntry::RunFinished(finished) => {
                Some((finished.run_id.as_str(), (finished, line.ts)))
            }
            _ => None,
        })
        .collect();
    let mut entries: Vec<RecentEntry> = lines
        .iter()
        .filter_map(|line| {
            let event = match &line.entry {
                JournalEntry::RunStarted(started) => {
                    let end = ends.get(started.run_id.as_str());
                    let timeout_s = started.timeout_s.map_or(*TIMEOUT_RANGE_S.end(), i64::from);
                    let state = match end {
                        Some(_) => RunState::Finished,
                        None if now_ms > line.ts + timeout_s * 1000 + ABANDONED_GRACE_MS => {
                            RunState::Abandoned
                        }
                        None => RunState::Running,
                    };
                    RecentEvent::Run(Box::new(RunView {
                        state,
                        started: started.clone(),
                        finished: end.map(|(finished, _)| (*finished).clone()),
                        finished_at_ms: end.map(|(_, ts)| *ts),
                    }))
                }
                JournalEntry::RunFinished(_) => return None,
                JournalEntry::Refused(refused) => RecentEvent::Refused(refused.clone()),
                JournalEntry::GateDenied(denied) => RecentEvent::GateDenied(denied.clone()),
                JournalEntry::GateError(error) => RecentEvent::GateError(error.clone()),
                JournalEntry::CatalogueApproved(approved) => {
                    RecentEvent::CatalogueApproved(approved.clone())
                }
                JournalEntry::ApprovalRequested(event) => {
                    RecentEvent::ApprovalRequested(event.clone())
                }
                JournalEntry::ApprovalGiven(event) => RecentEvent::ApprovalGiven(event.clone()),
                JournalEntry::ApprovalRejected(event) => {
                    RecentEvent::ApprovalRejected(event.clone())
                }
                JournalEntry::RelayStopped => RecentEvent::RelayStopped,
                JournalEntry::RelayResumed => RecentEvent::RelayResumed,
            };
            Some(RecentEntry {
                ts: line.ts,
                tone: tone_of(&event),
                event,
            })
        })
        .collect();
    entries.sort_by_key(|entry| entry.ts);
    let keep_from = entries.len().saturating_sub(RECENT_ENTRIES);
    let mut kept = entries.split_off(keep_from);
    kept.reverse();
    kept
}

/// Builds the relay view (AD-35). `catalogue` is the approved snapshot;
/// the current floor's differences are `pending` (D-24); issues are the
/// current floor's.
pub fn build(inputs: &ViewInputs<'_>) -> RelayView {
    let (stopped, stopped_since_ms) = match inputs.stop {
        StopState::Running => (false, None),
        StopState::Stopped { since_ms } => (true, since_ms),
    };
    let approved = match inputs.snapshot {
        SnapshotState::Valid(approved) => Some(approved),
        SnapshotState::Missing | SnapshotState::Unreadable(_) => None,
    };
    let snapshot_error = match inputs.snapshot {
        SnapshotState::Unreadable(why) => Some(why.to_owned()),
        SnapshotState::Missing | SnapshotState::Valid(_) => None,
    };
    let (floor, floor_error, pending, issues) = match inputs.floor {
        FloorRead::Valid(current) => (
            Some(inputs.floor_path.to_owned()),
            None,
            pending(inputs.snapshot, current),
            current.catalogue.issues.clone(),
        ),
        FloorRead::Unreadable(why) => (
            Some(inputs.floor_path.to_owned()),
            Some(why.to_owned()),
            None,
            Vec::new(),
        ),
        FloorRead::Missing => {
            let error = approved.map(|_| {
                format!(
                    "the relay floor {} is gone; `officina run` refuses every action until it is back",
                    inputs.floor_path
                )
            });
            (None, error, None, Vec::new())
        }
    };
    let since = inputs.now_ms - GATE_ERRORS_WINDOW_MS;
    let gate_errors = inputs
        .lines
        .iter()
        .filter(|line| matches!(line.entry, JournalEntry::GateError(_)) && line.ts >= since)
        .count();
    RelayView {
        schema: RELAY_VIEW_SCHEMA,
        stopped,
        stopped_since_ms,
        floor,
        floor_error,
        approved_hash: approved.map(|a| a.hash.clone()),
        approved_at_ms: approved.map(|a| a.approved_at_ms),
        snapshot_error,
        catalogue: approved.map(|a| a.canonical.clone()),
        pending,
        issues,
        recent: recent(inputs.lines, inputs.now_ms),
        journal_corrupt_lines: inputs.journal_corrupt_lines,
        gate_errors: u32::try_from(gate_errors).unwrap_or(u32::MAX),
        gate_errors_window_ms: GATE_ERRORS_WINDOW_MS,
        credentials: credentials(approved, inputs.credential_presence),
        notices: match inputs.floor {
            FloorRead::Valid(current) => notices(&current.canonical),
            FloorRead::Missing | FloorRead::Unreadable(_) => {
                approved.map(|a| notices(&a.canonical)).unwrap_or_default()
            }
        },
        anomalies: inputs.anomalies.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::catalogue::{ActionClass, ApprovedCatalogue, load};
    use crate::relay::gate::DenyKind;
    use crate::relay::journal::{GateFault, RefusalReason, RunOutcome, StreamSummary};

    const NOW: i64 = 1_793_102_400_000;
    const MIN: i64 = 60_000;
    use crate::relay::fixtures::FLOOR_PATH;
    const FLOOR: &str = "schema = 2\n[[project]]\nname = \"lab\"\npath = \"Lab\"\nactions = [\"probe\"]\n[[action]]\nname = \"probe\"\ndescription = \"Reads one host\"\nclass = \"free\"\nprogram = \"C:/path/to/tool.exe\"\nargs = [\"{host}\"]\n[action.params.host]\nvalues = [\"web01\"]\n";

    fn approved_of(relay: &RelayFloor) -> ApprovedCatalogue {
        ApprovedCatalogue::approving(relay, FLOOR_PATH, NOW - 60 * MIN)
    }

    fn inputs<'a>(
        snapshot: SnapshotState<'a>,
        floor: FloorRead<'a>,
        lines: &'a [JournalLine],
    ) -> ViewInputs<'a> {
        ViewInputs {
            stop: StopState::Running,
            snapshot,
            floor_path: FLOOR_PATH,
            floor,
            lines,
            journal_corrupt_lines: 0,
            now_ms: NOW,
            credential_presence: None,
            anomalies: &[],
        }
    }

    fn started(ts: i64, run_id: &str, timeout_s: Option<u32>) -> JournalLine {
        JournalLine::new(
            ts,
            JournalEntry::RunStarted(RunStarted {
                run_id: run_id.to_owned(),
                project: "lab".to_owned(),
                session_id: None,
                action: "probe".to_owned(),
                class: ActionClass::Free,
                params: BTreeMap::from([("host".to_owned(), "web01".to_owned())]),
                argv: vec!["C:/path/to/tool.exe".to_owned(), "web01".to_owned()],
                method: None,
                url: None,
                credential: None,
                catalogue_hash: "00".to_owned(),
                approval_id: None,
                tree: None,
                timeout_s,
            }),
        )
    }

    fn finished(ts: i64, run_id: &str) -> JournalLine {
        let stream = StreamSummary {
            bytes: 0,
            sha256: crate::relay::sha256_hex(b""),
        };
        JournalLine::new(
            ts,
            JournalEntry::RunFinished(RunFinished {
                body_broke_off: false,
                run_id: run_id.to_owned(),
                outcome: RunOutcome::Exited,
                exit_code: Some(0),
                duration_ms: ts,
                stdout: Some(stream.clone()),
                stderr: Some(stream),
                dropped_bytes: 0,
                status: None,
                body_bytes: None,
                body_sha256: None,
                detail: None,
            }),
        )
    }

    fn gate_error(ts: i64) -> JournalLine {
        JournalLine::new(
            ts,
            JournalEntry::GateError(GateError {
                project: None,
                session_id: None,
                cause: GateFault::FloorInvalid,
                detail: "x".to_owned(),
                command_excerpt: None,
            }),
        )
    }

    fn runs(view: &RelayView) -> Vec<(&str, RunState)> {
        view.recent
            .iter()
            .filter_map(|entry| match &entry.event {
                RecentEvent::Run(run) => Some((run.started.run_id.as_str(), run.state)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn catalogue_is_the_approved_snapshot_and_nothing_is_pending_when_equal() {
        let current = load(FLOOR, FLOOR_PATH).unwrap();
        let approved = approved_of(&current);
        let view = build(&inputs(
            SnapshotState::Valid(&approved),
            FloorRead::Valid(&current),
            &[],
        ));
        assert_eq!(view.schema, RELAY_VIEW_SCHEMA);
        assert_eq!(view.catalogue.as_ref(), Some(&approved.canonical));
        assert_eq!(view.approved_hash.as_deref(), Some(current.hash.as_str()));
        assert_eq!(view.approved_at_ms, Some(approved.approved_at_ms));
        assert_eq!(view.pending, None);
        assert_eq!(view.floor.as_deref(), Some(FLOOR_PATH));
        assert!(!view.stopped && view.stopped_since_ms.is_none());
    }

    #[test]
    fn a_floor_edit_is_pending_and_the_catalogue_stays_approved() {
        let approved = approved_of(&load(FLOOR, FLOOR_PATH).unwrap());
        let edited = load(
            &FLOOR.replace("[\"web01\"]", "[\"web01\", \"db01\"]"),
            FLOOR_PATH,
        )
        .unwrap();
        let view = build(&inputs(
            SnapshotState::Valid(&approved),
            FloorRead::Valid(&edited),
            &[],
        ));
        let pending = view.pending.unwrap();
        assert_eq!(pending.hash, edited.hash);
        assert_eq!(pending.diff.actions.changed[0].name, "probe");
        assert_eq!(pending.diff.actions.changed[0].fields[0].field, "params");
        assert_eq!(
            view.catalogue.unwrap(),
            approved.canonical,
            "D-24: the snapshot"
        );
    }

    #[test]
    fn a_first_catalogue_is_pending_as_all_added() {
        let current = load(FLOOR, FLOOR_PATH).unwrap();
        let view = build(&inputs(
            SnapshotState::Missing,
            FloorRead::Valid(&current),
            &[],
        ));
        let pending = view.pending.unwrap();
        assert_eq!(pending.hash, current.hash);
        assert_eq!(pending.diff.actions.added, ["probe"]);
        assert_eq!(
            pending.current, current.canonical,
            "an added action shows what it runs"
        );
        assert_eq!(pending.diff.projects.added, ["lab"]);
        assert!(view.catalogue.is_none() && view.approved_hash.is_none());

        let v0_1 = load(
            "schema = 1\n[[project]]\nname = \"lab\"\npath = \"Lab\"\n",
            FLOOR_PATH,
        )
        .unwrap();
        let nothing = build(&inputs(
            SnapshotState::Missing,
            FloorRead::Valid(&v0_1),
            &[],
        ));
        assert_eq!(nothing.pending, None, "a v0.1 floor has nothing to approve");

        let broken = build(&inputs(
            SnapshotState::Unreadable("corrupt"),
            FloorRead::Valid(&v0_1),
            &[],
        ));
        assert_eq!(broken.snapshot_error.as_deref(), Some("corrupt"));
        assert!(
            broken.pending.is_some(),
            "a broken snapshot can always be approved again"
        );
    }

    #[test]
    fn floor_problems_are_stated() {
        let current = load(FLOOR, FLOOR_PATH).unwrap();
        let approved = approved_of(&current);
        let unreadable = build(&inputs(
            SnapshotState::Valid(&approved),
            FloorRead::Unreadable("line 1: expected `=`"),
            &[],
        ));
        assert_eq!(
            unreadable.floor_error.as_deref(),
            Some("line 1: expected `=`")
        );
        assert!(unreadable.pending.is_none() && unreadable.catalogue.is_some());

        let gone = build(&inputs(
            SnapshotState::Valid(&approved),
            FloorRead::Missing,
            &[],
        ));
        assert!(gone.floor_error.unwrap().contains("is gone"));
        let fresh = build(&inputs(SnapshotState::Missing, FloorRead::Missing, &[]));
        assert_eq!(
            (fresh.floor, fresh.floor_error, fresh.pending),
            (None, None, None),
            "no floor and nothing approved is a workstation without the relay"
        );
    }

    #[test]
    fn issues_come_from_the_current_floor_with_lines() {
        let with_broken = format!(
            "{FLOOR}[[action]]\nname = \"broken\"\nclass = \"free\"\nprogram = \"relative.exe\"\n"
        );
        let current = load(&with_broken, FLOOR_PATH).unwrap();
        let view = build(&inputs(
            SnapshotState::Missing,
            FloorRead::Valid(&current),
            &[],
        ));
        assert_eq!(view.issues, current.catalogue.issues);
        assert_eq!(view.issues[0].name, "broken");
        assert_eq!(view.issues[0].line, Some(14), "the `[[action]]` line");
    }

    #[test]
    fn stop_flag_is_stated_with_its_time_when_known() {
        let mut stopped = inputs(SnapshotState::Missing, FloorRead::Missing, &[]);
        stopped.stop = StopState::Stopped {
            since_ms: Some(NOW - MIN),
        };
        let view = build(&stopped);
        assert!(view.stopped);
        assert_eq!(view.stopped_since_ms, Some(NOW - MIN));
        stopped.stop = StopState::Stopped { since_ms: None };
        let unreadable = build(&stopped);
        assert!(unreadable.stopped && unreadable.stopped_since_ms.is_none());
    }

    #[test]
    fn runs_are_paired_and_abandoned_past_their_timeout() {
        let lines = [
            started(NOW - 30 * MIN, "done", Some(60)),
            started(NOW - 3 * MIN, "gone", Some(60)),
            finished(NOW - 29 * MIN, "done"),
            started(NOW - MIN, "going", Some(60)),
            started(NOW - 61 * MIN, "old-format", None),
            finished(NOW, "never-started"),
        ];
        let view = build(&inputs(SnapshotState::Missing, FloorRead::Missing, &lines));
        assert_eq!(
            runs(&view),
            [
                ("going", RunState::Running),
                ("gone", RunState::Abandoned),
                ("done", RunState::Finished),
                ("old-format", RunState::Running),
            ],
            "placed at their start, newest first; a line without a timeout waits the longest one"
        );
        let RecentEvent::Run(done) = &view.recent[2].event else {
            panic!("{:?}", view.recent[2]);
        };
        assert_eq!(done.finished_at_ms, Some(NOW - 29 * MIN));
        assert_eq!(done.finished.as_ref().unwrap().exit_code, Some(0));
        assert_eq!(view.recent.len(), 4, "an end without its start is dropped");

        let abandoned_at = NOW - 60_000 - ABANDONED_GRACE_MS - 1;
        let edge = build(&inputs(
            SnapshotState::Missing,
            FloorRead::Missing,
            &[started(abandoned_at, "edge", Some(1))],
        ));
        assert_eq!(runs(&edge), [("edge", RunState::Abandoned)]);
    }

    #[test]
    fn recent_keeps_the_last_entries_newest_first() {
        let mut lines: Vec<JournalLine> = (0..RECENT_ENTRIES as i64 + 10)
            .map(|i| {
                JournalLine::new(
                    NOW - 100 * MIN + i,
                    JournalEntry::Refused(Refused {
                        project: None,
                        session_id: None,
                        action: format!("a{i}"),
                        params: None,
                        reason: RefusalReason::OutsideProjects,
                        detail: "x".to_owned(),
                    }),
                )
            })
            .collect();
        lines.push(JournalLine::new(
            NOW,
            JournalEntry::GateDenied(GateDenied {
                project: "lab".to_owned(),
                session_id: None,
                deny: DenyKind::GuardedProgram,
                matched: Some("ssh".to_owned()),
                command_excerpt: None,
            }),
        ));
        let view = build(&inputs(SnapshotState::Missing, FloorRead::Missing, &lines));
        assert_eq!(view.recent.len(), RECENT_ENTRIES);
        assert!(matches!(view.recent[0].event, RecentEvent::GateDenied(_)));
        let RecentEvent::Refused(oldest) = &view.recent[RECENT_ENTRIES - 1].event else {
            panic!("the oldest kept entry is the refusal a11");
        };
        assert_eq!(oldest.action, "a11");
    }

    #[test]
    fn every_journal_kind_has_a_recent_entry() {
        use crate::relay::journal::JournalKind;
        for kind in JournalKind::ALL {
            let entry = match kind {
                JournalKind::RunStarted => started(NOW, "r", Some(5)).entry,
                // An end line is no entry of its own: it joins its start
                // line's run, which `runs_are_paired_and_abandoned_past_their_timeout`
                // checks.
                JournalKind::RunFinished => continue,
                JournalKind::Refused => JournalEntry::Refused(Refused {
                    project: None,
                    session_id: None,
                    action: "a".to_owned(),
                    params: None,
                    reason: RefusalReason::Stopped,
                    detail: "x".to_owned(),
                }),
                JournalKind::GateDenied => JournalEntry::GateDenied(GateDenied {
                    project: "lab".to_owned(),
                    session_id: None,
                    deny: DenyKind::Dynamic,
                    matched: None,
                    command_excerpt: None,
                }),
                JournalKind::GateError => gate_error(NOW).entry,
                JournalKind::CatalogueApproved => {
                    JournalEntry::CatalogueApproved(CatalogueApproved {
                        hash: "01".to_owned(),
                        previous_hash: None,
                    })
                }
                JournalKind::ApprovalRequested
                | JournalKind::ApprovalGiven
                | JournalKind::ApprovalRejected => {
                    let event = ApprovalEvent {
                        approval_id: "0123456789abcdef-1".to_owned(),
                        request_key: "ab".to_owned(),
                        project: Some("lab".to_owned()),
                        action: Some("probe".to_owned()),
                        dry_run: None,
                    };
                    match kind {
                        JournalKind::ApprovalRequested => JournalEntry::ApprovalRequested(event),
                        JournalKind::ApprovalGiven => JournalEntry::ApprovalGiven(event),
                        _ => JournalEntry::ApprovalRejected(event),
                    }
                }
                JournalKind::RelayStopped => JournalEntry::RelayStopped,
                JournalKind::RelayResumed => JournalEntry::RelayResumed,
            };
            let lines = [JournalLine::new(NOW, entry)];
            let view = build(&inputs(SnapshotState::Missing, FloorRead::Missing, &lines));
            let tag = serde_json::to_value(&view.recent[0].event).unwrap()["kind"].clone();
            let expected = match kind {
                JournalKind::RunStarted => serde_json::json!("run"),
                other => serde_json::to_value(other).unwrap(),
            };
            assert_eq!(tag, expected, "{kind:?}");
        }
    }

    /// AC 7 of story 2.15: each recent entry's tone is computed here, every
    /// run outcome included, so the window renders it and computes nothing.
    #[test]
    fn tone_for_every_outcome_kind() {
        let run = |state: RunState, finished: Option<RunFinished>| {
            let JournalEntry::RunStarted(started) = started(NOW, "r", Some(5)).entry else {
                unreachable!("started() builds a run_started line")
            };
            RecentEvent::Run(Box::new(RunView {
                state,
                started,
                finished_at_ms: finished.as_ref().map(|_| NOW),
                finished,
            }))
        };
        let ended = |outcome: RunOutcome, exit_code: Option<i64>, status: Option<u16>| {
            let JournalEntry::RunFinished(mut finished) = finished(NOW, "r").entry else {
                unreachable!("finished() builds a run_finished line")
            };
            finished.outcome = outcome;
            finished.exit_code = exit_code;
            finished.status = status;
            finished
        };
        for outcome in RunOutcome::ALL {
            let failed = ended(*outcome, Some(1), Some(500));
            assert_eq!(
                tone_of(&run(RunState::Finished, Some(failed))),
                Some(Tone::Failed),
                "{outcome:?}"
            );
        }
        let ok = [
            ended(RunOutcome::Exited, Some(0), None),
            ended(RunOutcome::Http, None, Some(204)),
        ];
        for finished in ok {
            assert_eq!(
                tone_of(&run(RunState::Finished, Some(finished.clone()))),
                Some(Tone::Ok),
                "{finished:?}"
            );
        }
        let mut broke_off = ended(RunOutcome::Http, None, Some(200));
        broke_off.body_broke_off = true;
        assert_eq!(
            tone_of(&run(RunState::Finished, Some(broke_off))),
            Some(Tone::Failed)
        );
        assert_eq!(tone_of(&run(RunState::Running, None)), None);
        assert_eq!(tone_of(&run(RunState::Abandoned, None)), Some(Tone::Failed));

        let lines = [
            started(NOW - 2, "r", Some(5)),
            finished(NOW - 1, "r"),
            gate_error(NOW),
        ];
        let view = build(&inputs(SnapshotState::Missing, FloorRead::Missing, &lines));
        let tones: Vec<Option<Tone>> = view.recent.iter().map(|e| e.tone).collect();
        assert_eq!(
            tones,
            [Some(Tone::Failed), Some(Tone::Ok)],
            "the view carries them"
        );
        assert_eq!(
            tone_of(&RecentEvent::Refused(Refused {
                project: None,
                session_id: None,
                action: "a".to_owned(),
                params: None,
                reason: RefusalReason::Stopped,
                detail: "x".to_owned(),
            })),
            Some(Tone::Refused)
        );
        assert_eq!(
            tone_of(&RecentEvent::GateDenied(GateDenied {
                project: "lab".to_owned(),
                session_id: None,
                deny: DenyKind::Dynamic,
                matched: None,
                command_excerpt: None,
            })),
            Some(Tone::Refused)
        );
        assert_eq!(tone_of(&RecentEvent::RelayStopped), None);
    }

    const HTTP: &str = r#"
[[credential]]
name = "monitoring"
hosts = ["Monitoring.example.com", "mon2.example.com"]
header = "Authorization"
scheme = "Bearer"

[[credential]]
name = "unused"
hosts = ["example.com"]
header = "X-Api-Key"
scheme = "Token"

[[action]]
name = "health"
kind = "http"
class = "free"
method = "GET"
url = "https://monitoring.example.com/health"
credential = "monitoring"

[[action]]
name = "ready"
kind = "http"
class = "free"
method = "HEAD"
url = "https://mon2.example.com/ready"
credential = "monitoring"
"#;

    #[test]
    fn credentials_list_their_actions() {
        let current = load(&format!("{FLOOR}{HTTP}"), FLOOR_PATH).unwrap();
        assert!(
            current.catalogue.issues.is_empty(),
            "{:#?}",
            current.catalogue.issues
        );
        let approved = approved_of(&current);
        let unchecked = build(&inputs(
            SnapshotState::Valid(&approved),
            FloorRead::Valid(&current),
            &[],
        ));
        assert_eq!(
            unchecked.credentials,
            [
                CredentialView {
                    name: "monitoring".to_owned(),
                    hosts: vec![
                        "mon2.example.com".to_owned(),
                        "monitoring.example.com".to_owned()
                    ],
                    actions: vec!["health".to_owned(), "ready".to_owned()],
                    present: None,
                },
                CredentialView {
                    name: "unused".to_owned(),
                    hosts: vec!["example.com".to_owned()],
                    actions: Vec::new(),
                    present: None,
                },
            ]
        );

        let found = BTreeMap::from([("monitoring".to_owned(), true)]);
        let mut checked = inputs(
            SnapshotState::Valid(&approved),
            FloorRead::Valid(&current),
            &[],
        );
        checked.credential_presence = Some(&found);
        let view = build(&checked);
        assert_eq!(view.credentials[0].present, Some(true));
        assert_eq!(view.credentials[1].present, None, "not checked");
        let http = view.catalogue.unwrap().actions["health"]
            .http
            .clone()
            .unwrap();
        assert_eq!(http.url, "https://monitoring.example.com/health");

        let nothing = build(&inputs(
            SnapshotState::Missing,
            FloorRead::Valid(&current),
            &[],
        ));
        assert!(
            nothing.credentials.is_empty(),
            "only the approved catalogue's"
        );
    }

    #[test]
    fn notices_come_from_the_current_floor() {
        let ssh = FLOOR
            .replace("C:/path/to/tool.exe", "C:/path/to/ssh.exe")
            .replace(
                "args = [\"{host}\"]",
                "args = [\"web01\", \"echo\", \"{host}\"]",
            );
        let current = load(&ssh, FLOOR_PATH).unwrap();
        let view = build(&inputs(
            SnapshotState::Missing,
            FloorRead::Valid(&current),
            &[],
        ));
        assert_eq!(view.notices.len(), 1, "{:?}", view.notices);
        assert_eq!(view.notices[0].action, "probe");
        let approved = approved_of(&current);
        let gone = build(&inputs(
            SnapshotState::Valid(&approved),
            FloorRead::Missing,
            &[],
        ));
        assert_eq!(
            gone.notices, view.notices,
            "the snapshot's when the floor is gone"
        );
    }

    #[test]
    fn gate_errors_are_counted_over_their_window() {
        let lines = [
            gate_error(NOW - GATE_ERRORS_WINDOW_MS - 1),
            gate_error(NOW - GATE_ERRORS_WINDOW_MS),
            gate_error(NOW - MIN),
        ];
        let mut read = inputs(SnapshotState::Missing, FloorRead::Missing, &lines);
        read.journal_corrupt_lines = 3;
        let view = build(&read);
        assert_eq!(view.gate_errors, 2);
        assert_eq!(view.gate_errors_window_ms, GATE_ERRORS_WINDOW_MS);
        assert_eq!(view.journal_corrupt_lines, 3);
    }
}
