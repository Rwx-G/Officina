//! The relay's state as the window and `officina relay --json` show it
//! (AD-35, D-24): one [`RelayView`], built from what the host read. Separate
//! from the board so the journal tail never churns the board contract.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::catalogue::{
    ActionIssue, ApprovedCatalogue, CANONICAL_FORMAT, CanonicalAutoReview, CanonicalCatalogue,
    CatalogueDiff, Notice, RelayFloor, TIMEOUT_RANGE_S, diff, notices, relay_in_use,
};
use super::gate::DenyKind;
// Named by `GateDeniedView`'s documentation, which is its TypeScript
// type's too and so cannot carry a path.
use super::approvals::ApprovalBy;
#[cfg(doc)]
use super::auto::STATS_WINDOW_MS;
use super::auto::{
    AutoApprovalSetting, AutoMode, AutoOffReason, EscalationCause, MARK_WINDOW_MS, ReviewDecision,
    ShadowStats, StatsRecord, shadow_stats,
};
#[cfg(doc)]
use super::journal::COMMAND_EXCERPT_CHARS;
use super::journal::{
    ApprovalEvent, AutoModeDenied, CatalogueApproved, GateDenied, GateError, GateFault,
    JournalEntry, JournalLine, RefusalReason, Refused, RunFinished, RunStarted,
};
use super::record::Anomaly;
use super::run::SnapshotState;
use super::template::ModelAlias;
use crate::dates::{DAY_MS, day_start_ms, utc_date, utc_day_text};
use crate::registry::triage_call::CALLS_PER_DAY;
use crate::registry::types::{AuthorDecision, AuthorReason, ReviewMode, ReviewOutcome, Source};
use crate::untrusted::Untrusted;

/// Version of the relay view contract: 2 added templates, the review model,
/// `auto_max_per_hour` and each issue's table (story 5.4, P-26); 3 the
/// `auto` section and each decision's review (story 5.14).
pub const RELAY_VIEW_SCHEMA: u32 = 3;
/// How many entries [`RelayView::recent`] keeps, a run counting once.
pub const RECENT_ENTRIES: usize = 50;
/// A run with no end line this long past its timeout reads abandoned
/// (AD-32).
pub const ABANDONED_GRACE_MS: i64 = 60_000;
/// The window over which gate errors are counted.
pub const GATE_ERRORS_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;
/// The window over which auto-mode denials are counted per session.
pub const DENIALS_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;
/// How many of a session's latest denial reasons the view carries.
pub const DENIAL_REASONS_SHOWN: usize = 5;

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
        | RecentEvent::RelayResumed
        | RecentEvent::AutoModeDenied(_) => None,
    }
}

// The three entries below are their journal lines as the view shows them:
// the session's text in each is `Untrusted` (AD-74, D-130), wrapped here
// where the view is built, while the parse records keep plain strings. Each
// keeps its journal type's name, fields, serde attributes and documentation,
// so its bytes and its TypeScript type are those of the journal line.

/// `officina run` refused before starting anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "Refused"))]
pub struct RefusedView {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    pub action: Untrusted<String>,
    // The session's parameters, names included: an MCP output marks the
    // whole map, since a JSON key cannot carry the wrapped form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Untrusted<BTreeMap<String, String>>>,
    pub reason: RefusalReason,
    pub detail: Untrusted<String>,
}

impl From<&Refused> for RefusedView {
    fn from(line: &Refused) -> RefusedView {
        RefusedView {
            project: line.project.clone(),
            session_id: line.session_id.clone(),
            action: Untrusted::new(line.action.clone()),
            params: line.params.clone().map(Untrusted::new),
            reason: line.reason,
            detail: Untrusted::new(line.detail.clone()),
        }
    }
}

/// The gate denied a tool call: a shell command, or a file write under the
/// Officina home (AD-26, D-171, D-173).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "GateDenied"))]
pub struct GateDeniedView {
    pub project: String,
    #[serde(default)]
    pub session_id: Option<String>,
    pub deny: DenyKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched: Option<String>,
    /// The command's first [`COMMAND_EXCERPT_CHARS`] characters; absent
    /// when excerpts are off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_excerpt: Option<Untrusted<String>>,
}

impl From<&GateDenied> for GateDeniedView {
    fn from(line: &GateDenied) -> GateDeniedView {
        GateDeniedView {
            project: line.project.clone(),
            session_id: line.session_id.clone(),
            deny: line.deny,
            matched: line.matched.clone(),
            command_excerpt: line.command_excerpt.clone().map(Untrusted::new),
        }
    }
}

/// The gate met a fault: an input it could not read (D-15) or an internal
/// error (AD-28). The decision it took stands beside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "GateError"))]
pub struct GateErrorView {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    pub cause: GateFault,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_excerpt: Option<Untrusted<String>>,
}

impl From<&GateError> for GateErrorView {
    fn from(line: &GateError) -> GateErrorView {
        GateErrorView {
            project: line.project.clone(),
            session_id: line.session_id.clone(),
            cause: line.cause,
            detail: line.detail.clone(),
            command_excerpt: line.command_excerpt.clone().map(Untrusted::new),
        }
    }
}

/// Claude Code's auto mode denied a tool call (AD-181): the session, the
/// tool and the classifier's reason, which is shown as text only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "AutoModeDenied"))]
pub struct AutoModeDeniedView {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    pub reason: Untrusted<String>,
}

impl From<&AutoModeDenied> for AutoModeDeniedView {
    fn from(line: &AutoModeDenied) -> AutoModeDeniedView {
        AutoModeDeniedView {
            session_id: line.session_id.clone(),
            tool_name: line.tool_name.clone(),
            reason: Untrusted::new(line.reason.clone()),
        }
    }
}

/// One session's auto-mode denials over the last [`DENIALS_WINDOW_MS`]
/// (AD-181): how many, when the latest was, and the latest reasons, at
/// most [`DENIAL_REASONS_SHOWN`], newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SessionDenials {
    pub session_id: String,
    pub count: u32,
    pub last_at_ms: i64,
    pub reasons: Vec<Untrusted<String>>,
}

/// The auto-mode denials of the last [`DENIALS_WINDOW_MS`] by session, the
/// session denied most recently first.
fn denials(lines: &[JournalLine], now_ms: i64) -> Vec<SessionDenials> {
    let since = now_ms - DENIALS_WINDOW_MS;
    let mut sessions: Vec<SessionDenials> = Vec::new();
    for line in lines.iter().rev().filter(|line| line.ts >= since) {
        let JournalEntry::AutoModeDenied(denied) = &line.entry else {
            continue;
        };
        let at = sessions
            .iter()
            .position(|s| s.session_id == denied.session_id)
            .unwrap_or_else(|| {
                sessions.push(SessionDenials {
                    session_id: denied.session_id.clone(),
                    count: 0,
                    last_at_ms: line.ts,
                    reasons: Vec::new(),
                });
                sessions.len() - 1
            });
        let session = &mut sessions[at];
        session.count = session.count.saturating_add(1);
        session.last_at_ms = session.last_at_ms.max(line.ts);
        if session.reasons.len() < DENIAL_REASONS_SHOWN {
            session.reasons.push(Untrusted::new(denied.reason.clone()));
        }
    }
    sessions.sort_by_key(|s| std::cmp::Reverse(s.last_at_ms));
    sessions
}

/// What one recent entry records: a journal line, runs paired.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecentEvent {
    /// Boxed: a run carries far more than any other entry.
    Run(Box<RunView>),
    Refused(RefusedView),
    GateDenied(GateDeniedView),
    GateError(GateErrorView),
    CatalogueApproved(CatalogueApproved),
    ApprovalRequested(ApprovalEvent),
    ApprovalGiven(ApprovalEvent),
    ApprovalRejected(ApprovalEvent),
    RelayStopped,
    RelayResumed,
    AutoModeDenied(AutoModeDeniedView),
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
    /// On the author's decision of an `auto` request: what its review
    /// said, shown once he decided and never before (D-130, D-143).
    pub review: Option<ReviewShown>,
}

/// What a review said of one request, as the Relay view shows it once the
/// author decided, or once the review or the rule approved it (AD-88,
/// D-143): its decision, its reason (model text), and the rule baseline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ReviewShown {
    pub mode: ReviewMode,
    pub outcome: ReviewOutcome,
    pub decision: Option<ReviewDecision>,
    /// Absent from `officina relay --json`, which every session can run
    /// (D-143, review F17), and once retention cleared it.
    pub reason: Option<Untrusted<String>>,
    pub rule_baseline: bool,
}

/// One `auto_review` row as the relay view reads it (AD-88): the host
/// maps the registry's row, its reason left out by `officina relay
/// --json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoRow {
    pub approval_id: String,
    pub project: String,
    pub action: String,
    pub requested_at_ms: i64,
    pub mode: ReviewMode,
    pub outcome: ReviewOutcome,
    pub cause: Option<EscalationCause>,
    pub decision: Option<ReviewDecision>,
    pub reason: Option<Untrusted<String>>,
    pub rule_baseline: bool,
    pub call_started_at_ms: Option<i64>,
    pub author_decision: Option<AuthorDecision>,
    pub author_reason: Option<AuthorReason>,
    pub run_id: Option<String>,
    pub marked_at_ms: Option<i64>,
}

impl AutoRow {
    /// The row as the shadow statistics read it.
    pub fn stats(&self) -> StatsRecord {
        StatsRecord {
            requested_at_ms: self.requested_at_ms,
            mode: self.mode,
            outcome: self.outcome,
            decision: self.decision,
            rule_baseline: self.rule_baseline,
            author_decision: self.author_decision,
            author_reason: self.author_reason,
            marked_at_ms: self.marked_at_ms,
        }
    }

    /// True for a request a review or the rule approved: decision
    /// `approve` in live or rule mode, with no cause (an approval the
    /// authority refused keeps the cause it escalated with).
    pub fn auto_approved(&self) -> bool {
        matches!(self.mode, ReviewMode::Live | ReviewMode::Rule)
            && self.decision == Some(ReviewDecision::Approve)
            && self.cause.is_none()
    }

    fn shown(&self) -> ReviewShown {
        ReviewShown {
            mode: self.mode,
            outcome: self.outcome,
            decision: self.decision,
            reason: self.reason.clone(),
            rule_baseline: self.rule_baseline,
        }
    }
}

/// The review of `approval_id` once the author decided it (D-143).
fn shown_review(rows: &[AutoRow], approval_id: &str) -> Option<ReviewShown> {
    rows.iter()
        .find(|row| row.approval_id == approval_id && row.author_decision.is_some())
        .map(AutoRow::shown)
}

/// What the `auto` section needs besides the journal, read by the host
/// (AD-88, AD-92).
#[derive(Debug, Clone, Copy)]
pub struct AutoInputs<'a> {
    pub approval: AutoApprovalSetting,
    /// Who last set it; `None` before anyone did.
    pub approval_src: Option<Source>,
    pub mode: AutoMode,
    /// The review rows of at least the last [`STATS_WINDOW_MS`], marked
    /// rows of that window included.
    pub rows: &'a [AutoRow],
    /// The journal lines of the last [`DECISION_DAYS`] UTC days.
    pub decision_lines: &'a [JournalLine],
}

/// How many UTC days [`AutoView::decisions`] covers (AD-92).
pub const DECISION_DAYS: i64 = 14;

/// One auto-approved run of the last [`MARK_WINDOW_MS`] (AD-88, FR-9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AutoRun {
    pub approval_id: String,
    /// The run its approval started, from `run_started.approval_id`;
    /// `None` until `officina run` consumed it.
    pub run_id: Option<String>,
    pub project: String,
    pub action: String,
    pub by: ApprovalBy,
    pub requested_at_ms: i64,
    /// When the author marked it "should not have run".
    pub marked_at_ms: Option<i64>,
    pub review: ReviewShown,
}

/// The decisions of one UTC day (AD-92, the Auto metric): the author's,
/// on `human` and `auto` requests alike, and those a review or the rule
/// gave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DayDecisions {
    /// `YYYY-MM-DD`.
    pub day: String,
    pub author: u32,
    pub auto: u32,
}

/// The Relay view's `auto` section (AD-88, AD-92, D-142, D-143).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AutoView {
    pub approval_on: bool,
    /// Who last set auto approval; `None` before anyone did.
    pub approval_src: Option<Source>,
    /// Why it went off, when a mark turned it off.
    pub approval_reason: Option<AutoOffReason>,
    pub mode: AutoMode,
    pub stats: ShadowStats,
    /// Requests reviewed or escalated since the UTC day began.
    pub reviews_today: u32,
    /// Review calls started since the UTC day began, against
    /// `calls_per_day`.
    pub calls_today: u32,
    pub calls_per_day: u32,
    /// The approved review model; `None` without an approved catalogue or
    /// with a failing `[auto_review]`.
    pub review_model: Option<ModelAlias>,
    /// The auto-approved runs of the last `mark_window_ms`, newest first.
    pub runs: Vec<AutoRun>,
    /// [`MARK_WINDOW_MS`], carried so the window states it.
    pub mark_window_ms: i64,
    /// The last [`DECISION_DAYS`] UTC days, oldest first, today last.
    pub decisions: Vec<DayDecisions>,
}

/// Builds the `auto` section at `now_ms`.
fn auto_view(
    inputs: Option<&AutoInputs<'_>>,
    lines: &[JournalLine],
    approved: Option<&ApprovedCatalogue>,
    now_ms: i64,
) -> AutoView {
    let rows = inputs.map_or(&[][..], |i| i.rows);
    let day_start = day_start_ms(utc_date(now_ms));
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let run_of: BTreeMap<&str, &str> = lines
        .iter()
        .filter_map(|line| match &line.entry {
            JournalEntry::RunStarted(started) => started
                .approval_id
                .as_deref()
                .map(|id| (id, started.run_id.as_str())),
            _ => None,
        })
        .collect();
    let mut runs: Vec<AutoRun> = rows
        .iter()
        .filter(|row| row.auto_approved())
        .filter(|row| (0..MARK_WINDOW_MS).contains(&(now_ms - row.requested_at_ms)))
        .map(|row| AutoRun {
            approval_id: row.approval_id.clone(),
            run_id: row.run_id.clone().or_else(|| {
                run_of
                    .get(row.approval_id.as_str())
                    .map(|r| (*r).to_owned())
            }),
            project: row.project.clone(),
            action: row.action.clone(),
            by: if row.mode == ReviewMode::Rule {
                ApprovalBy::Rule
            } else {
                ApprovalBy::Review
            },
            requested_at_ms: row.requested_at_ms,
            marked_at_ms: row.marked_at_ms,
            review: row.shown(),
        })
        .collect();
    runs.sort_by_key(|run| std::cmp::Reverse(run.requested_at_ms));
    let decision_lines = inputs.map_or(&[][..], |i| i.decision_lines);
    let decisions = (0..DECISION_DAYS)
        .rev()
        .map(|back| {
            let start = day_start - back * DAY_MS;
            let of_day = |by_author: bool| {
                count(
                    decision_lines
                        .iter()
                        .filter(|line| (start..start + DAY_MS).contains(&line.ts))
                        .filter(|line| match &line.entry {
                            JournalEntry::ApprovalGiven(event)
                            | JournalEntry::ApprovalRejected(event) => {
                                (event.by.unwrap_or(ApprovalBy::Author) == ApprovalBy::Author)
                                    == by_author
                            }
                            _ => false,
                        })
                        .count(),
                )
            };
            DayDecisions {
                day: utc_day_text(start),
                author: of_day(true),
                auto: of_day(false),
            }
        })
        .collect();
    let stats: Vec<StatsRecord> = rows.iter().map(AutoRow::stats).collect();
    let approval = inputs.map(|i| i.approval).unwrap_or_default();
    AutoView {
        approval_on: approval.on,
        approval_src: inputs.and_then(|i| i.approval_src),
        approval_reason: approval.reason,
        mode: inputs.map(|i| i.mode).unwrap_or_default(),
        stats: shadow_stats(&stats, now_ms),
        reviews_today: count(
            rows.iter()
                .filter(|row| row.requested_at_ms >= day_start)
                .count(),
        ),
        calls_today: count(
            rows.iter()
                .filter(|row| row.outcome != ReviewOutcome::NoCall)
                .filter(|row| row.call_started_at_ms.is_some_and(|at| at >= day_start))
                .count(),
        ),
        calls_per_day: count(CALLS_PER_DAY),
        review_model: approved.and_then(|a| a.canonical.auto_review.model),
        runs,
        mark_window_ms: MARK_WINDOW_MS,
        decisions,
    }
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
    /// The `auto` class: approval, mode, gate and the runs it approved.
    pub auto: AutoView,
    /// Claude Code's auto-mode denials by session (AD-181, story 5.23),
    /// shown as neutral text: no schema number moves (P-26).
    pub auto_mode_denials: Vec<SessionDenials>,
    /// The window `auto_mode_denials` covers: [`DENIALS_WINDOW_MS`].
    pub auto_mode_denials_window_ms: i64,
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
    /// The registry's side of the `auto` section; `None` without a
    /// readable registry, the section then reporting approval off.
    pub auto: Option<&'a AutoInputs<'a>>,
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
        templates: BTreeMap::new(),
        auto_review: CanonicalAutoReview::default(),
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
fn recent(lines: &[JournalLine], now_ms: i64, rows: &[AutoRow]) -> Vec<RecentEntry> {
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
                JournalEntry::Refused(refused) => RecentEvent::Refused(refused.into()),
                JournalEntry::GateDenied(denied) => RecentEvent::GateDenied(denied.into()),
                JournalEntry::GateError(error) => RecentEvent::GateError(error.into()),
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
                JournalEntry::AutoModeDenied(denied) => RecentEvent::AutoModeDenied(denied.into()),
                // A launch is no relay entry: the Orchestrator view's
                // history shows work orders, from the registry (AD-92).
                // A review shows on its request's decision, and the
                // switch's state in the `auto` section. A routed prompt
                // has no window surface until the routing story (D-580).
                JournalEntry::WorkOrderLaunched(_)
                | JournalEntry::PromptRouted(_)
                | JournalEntry::PromptDecided(_)
                | JournalEntry::AutoEscalated(_)
                | JournalEntry::AutoReview(_)
                | JournalEntry::AutoApprovalSet(_)
                | JournalEntry::OrchestratorStarted(_)
                | JournalEntry::MeetingAnswer(_) => return None,
            };
            let review = match &event {
                RecentEvent::ApprovalGiven(decision) | RecentEvent::ApprovalRejected(decision)
                    if decision.by.unwrap_or(ApprovalBy::Author) == ApprovalBy::Author =>
                {
                    shown_review(rows, &decision.approval_id)
                }
                _ => None,
            };
            Some(RecentEntry {
                ts: line.ts,
                tone: tone_of(&event),
                event,
                review,
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
        recent: recent(
            inputs.lines,
            inputs.now_ms,
            inputs.auto.map_or(&[][..], |auto| auto.rows),
        ),
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
        auto: auto_view(inputs.auto, inputs.lines, approved, inputs.now_ms),
        auto_mode_denials: denials(inputs.lines, inputs.now_ms),
        auto_mode_denials_window_ms: DENIALS_WINDOW_MS,
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
            auto: None,
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

    fn denied(ts: i64, session: &str, reason: &str) -> JournalLine {
        JournalLine::new(
            ts,
            JournalEntry::AutoModeDenied(AutoModeDenied {
                session_id: session.to_owned(),
                tool_name: Some("Bash".to_owned()),
                reason: reason.to_owned(),
            }),
        )
    }

    /// Story 5.23 AC 3 (AD-181): the denials of the last
    /// [`DENIALS_WINDOW_MS`] are counted per session, at the bound
    /// included, the session denied most recently first; an older line
    /// counts for nothing, and the window is carried with the counts.
    #[test]
    fn denials_are_counted_per_session_over_24_h() {
        let lines = [
            denied(NOW - DENIALS_WINDOW_MS - 1, "s-a", "[Too Old]"),
            denied(NOW - DENIALS_WINDOW_MS, "s-a", "[Data Exfiltration]"),
            denied(NOW - 5_000, "s-b", "[Code from External]"),
            denied(NOW - 1_000, "s-a", "[Irreversible Local Destruction]"),
        ];
        let view = build(&inputs(SnapshotState::Missing, FloorRead::Missing, &lines));
        let counts: Vec<(&str, u32, i64)> = view
            .auto_mode_denials
            .iter()
            .map(|d| (d.session_id.as_str(), d.count, d.last_at_ms))
            .collect();
        assert_eq!(counts, [("s-a", 2, NOW - 1_000), ("s-b", 1, NOW - 5_000)]);
        assert_eq!(view.auto_mode_denials_window_ms, DENIALS_WINDOW_MS);
        assert_eq!(view.schema, RELAY_VIEW_SCHEMA);
        // Every line, the old one included, is a recent entry, toneless.
        let recent: Vec<&RecentEntry> = view
            .recent
            .iter()
            .filter(|e| matches!(e.event, RecentEvent::AutoModeDenied(_)))
            .collect();
        assert_eq!(recent.len(), lines.len());
        assert!(recent.iter().all(|e| e.tone.is_none()));
        let empty = build(&inputs(SnapshotState::Missing, FloorRead::Missing, &[]));
        assert!(empty.auto_mode_denials.is_empty());
    }

    /// Story 5.23 AC 3: a session keeps its latest
    /// [`DENIAL_REASONS_SHOWN`] reasons, newest first, each serialized as
    /// untrusted text, and the recent entry's reason too.
    #[test]
    fn the_reasons_are_untrusted_and_capped() {
        let lines: Vec<JournalLine> = (0..DENIAL_REASONS_SHOWN as i64 + 3)
            .map(|i| denied(NOW - 10_000 + i, "s-a", &format!("<b>reason {i}</b>")))
            .collect();
        let view = build(&inputs(SnapshotState::Missing, FloorRead::Missing, &lines));
        let session = &view.auto_mode_denials[0];
        assert_eq!(session.count as usize, lines.len());
        assert_eq!(session.reasons.len(), DENIAL_REASONS_SHOWN);
        let newest = lines.len() - 1;
        let expected: Vec<Untrusted<String>> = (0..DENIAL_REASONS_SHOWN)
            .map(|k| Untrusted::new(format!("<b>reason {}</b>", newest - k)))
            .collect();
        assert_eq!(session.reasons, expected);
        let wrapped =
            crate::untrusted::wrapped(|| serde_json::to_value(&view.auto_mode_denials).unwrap());
        assert_eq!(
            wrapped[0]["reasons"][0],
            serde_json::json!({"untrusted": format!("<b>reason {newest}</b>")})
        );
        let entry =
            crate::untrusted::wrapped(|| serde_json::to_value(&view.recent[0].event).unwrap());
        assert_eq!(entry["kind"], "auto_mode_denied");
        assert_eq!(
            entry["reason"],
            serde_json::json!({"untrusted": format!("<b>reason {newest}</b>")})
        );
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
        assert_eq!(oldest.action, crate::untrusted::text("a11"));
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
                        by: None,
                        reason: None,
                    };
                    match kind {
                        JournalKind::ApprovalRequested => JournalEntry::ApprovalRequested(event),
                        JournalKind::ApprovalGiven => JournalEntry::ApprovalGiven(event),
                        _ => JournalEntry::ApprovalRejected(event),
                    }
                }
                JournalKind::RelayStopped => JournalEntry::RelayStopped,
                JournalKind::RelayResumed => JournalEntry::RelayResumed,
                JournalKind::AutoModeDenied => JournalEntry::AutoModeDenied(AutoModeDenied {
                    session_id: "s".to_owned(),
                    tool_name: None,
                    reason: "[Data Exfiltration]".to_owned(),
                }),
                // Left out of the relay view, which `recent` checks by its
                // match: work orders show in the Orchestrator view.
                JournalKind::WorkOrderLaunched
                | JournalKind::PromptRouted
                | JournalKind::PromptDecided
                | JournalKind::AutoEscalated
                | JournalKind::AutoReview
                | JournalKind::AutoApprovalSet
                | JournalKind::OrchestratorStarted
                | JournalKind::MeetingAnswer => continue,
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

    /// Story 5.13 AC 5 (D-143): a review's lines never reach the Relay view
    /// of a request the author has not decided: no entry, no cause a call
    /// came to, no decision.
    #[test]
    fn an_undecided_request_shows_no_review_line() {
        use crate::registry::types::{ReviewMode, ReviewOutcome};
        use crate::relay::auto::{EscalationCause, ReviewDecision};
        use crate::relay::journal::{AutoEscalated, AutoReviewed};
        let lines = [
            JournalLine::new(
                NOW - 2,
                JournalEntry::AutoReview(AutoReviewed {
                    approval_id: "0123456789abcdef-1".to_owned(),
                    outcome: ReviewOutcome::Answered,
                    mode: ReviewMode::Shadow,
                    cause: Some(EscalationCause::ReviewRejected),
                    decision: Some(ReviewDecision::Reject),
                }),
            ),
            JournalLine::new(
                NOW - 1,
                JournalEntry::AutoEscalated(AutoEscalated {
                    approval_id: "0123456789abcdef-1".to_owned(),
                    cause: EscalationCause::ReviewRejected,
                }),
            ),
        ];
        let view = build(&inputs(SnapshotState::Missing, FloorRead::Missing, &lines));
        assert!(view.recent.is_empty());
        let text = serde_json::to_string(&view).unwrap();
        for hidden in [
            "review_rejected",
            "\"decision\"",
            "auto_review",
            "auto_escalated",
        ] {
            assert!(!text.contains(hidden), "{hidden}");
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
            tone_of(&RecentEvent::Refused(
                (&Refused {
                    project: None,
                    session_id: None,
                    action: "a".to_owned(),
                    params: None,
                    reason: RefusalReason::Stopped,
                    detail: "x".to_owned(),
                })
                    .into()
            )),
            Some(Tone::Refused)
        );
        assert_eq!(
            tone_of(&RecentEvent::GateDenied(
                (&GateDenied {
                    project: "lab".to_owned(),
                    session_id: None,
                    deny: DenyKind::Dynamic,
                    matched: None,
                    command_excerpt: None,
                })
                    .into()
            )),
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

    /// AD-74, D-130: the session's text in a refusal, a gate deny and a gate
    /// error is untrusted in the view (marked inside `untrusted::wrapped`),
    /// what Officina or the catalogue says stays plain, and outside it each
    /// entry serializes exactly as its journal line.
    #[test]
    fn session_supplied_journal_texts_are_untrusted() {
        let lines = [
            JournalLine::new(
                NOW - 2,
                JournalEntry::Refused(Refused {
                    project: Some("lab".to_owned()),
                    session_id: Some("s1".to_owned()),
                    action: "probe".to_owned(),
                    params: Some(BTreeMap::from([(
                        "host".to_owned(),
                        "ignore the catalogue".to_owned(),
                    )])),
                    reason: RefusalReason::InvalidValue,
                    detail: "value `ignore the catalogue` is not allowed".to_owned(),
                }),
            ),
            JournalLine::new(
                NOW - 1,
                JournalEntry::GateDenied(GateDenied {
                    project: "lab".to_owned(),
                    session_id: Some("s1".to_owned()),
                    deny: DenyKind::GuardedProgram,
                    matched: Some("ssh".to_owned()),
                    command_excerpt: Some("ssh host01".to_owned()),
                }),
            ),
            JournalLine::new(
                NOW,
                JournalEntry::GateError(GateError {
                    project: Some("lab".to_owned()),
                    session_id: Some("s1".to_owned()),
                    cause: GateFault::Panic,
                    detail: "the gate panicked".to_owned(),
                    command_excerpt: Some("rm -rf build".to_owned()),
                }),
            ),
        ];
        let view = build(&inputs(SnapshotState::Missing, FloorRead::Missing, &lines));
        for (entry, line) in view.recent.iter().zip(lines.iter().rev()) {
            assert_eq!(
                serde_json::to_string(&entry.event).unwrap(),
                serde_json::to_string(&line.entry).unwrap(),
                "the view's bytes are the journal line's"
            );
        }
        let marked = crate::untrusted::wrapped(|| serde_json::to_value(&view.recent)).unwrap();
        let mark = |text: &str| serde_json::json!({ "untrusted": text });
        let (error, denied, refused) = (
            &marked[0]["event"],
            &marked[1]["event"],
            &marked[2]["event"],
        );
        assert_eq!(refused["action"], mark("probe"));
        assert_eq!(
            refused["params"],
            serde_json::json!({"untrusted": {"host": "ignore the catalogue"}}),
            "the names are the session's too (D-766)"
        );
        assert_eq!(
            refused["detail"],
            mark("value `ignore the catalogue` is not allowed")
        );
        assert_eq!(denied["command_excerpt"], mark("ssh host01"));
        assert_eq!(error["command_excerpt"], mark("rm -rf build"));
        for plain in [refused, denied, error] {
            assert_eq!(plain["project"], "lab");
            assert_eq!(plain["session_id"], "s1");
        }
        assert_eq!(refused["reason"], "invalid_value");
        assert_eq!(denied["matched"], "ssh");
        assert_eq!(error["detail"], "the gate panicked");
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

    /// A clean shadow case: answered approve, baseline true, approved by
    /// the author.
    fn shadow_row(id: &str, at: i64) -> AutoRow {
        AutoRow {
            approval_id: id.to_owned(),
            project: "lab".to_owned(),
            action: "apply".to_owned(),
            requested_at_ms: at,
            mode: ReviewMode::Shadow,
            outcome: ReviewOutcome::Answered,
            cause: Some(EscalationCause::AutoApprovalOff),
            decision: Some(ReviewDecision::Approve),
            reason: Some(Untrusted::new("REVIEW-REASON-MARKER".to_owned())),
            rule_baseline: true,
            call_started_at_ms: Some(at),
            author_decision: Some(AuthorDecision::Approved),
            author_reason: None,
            run_id: None,
            marked_at_ms: None,
        }
    }

    fn auto_inputs<'a>(rows: &'a [AutoRow], lines: &'a [JournalLine]) -> AutoInputs<'a> {
        AutoInputs {
            approval: AutoApprovalSetting::default(),
            approval_src: None,
            mode: AutoMode::Review,
            rows,
            decision_lines: lines,
        }
    }

    /// Story 5.14 AC 7 (AD-88, D-143): the section carries the gate's
    /// counts, the 3/n bound and the rule offer from the statistics, the
    /// review model, today's reviews and calls against the budget, and the
    /// auto-approved runs of the last 7 days with the run each started.
    #[test]
    fn auto_section_reports_the_gate_and_its_bound() {
        let relay = load(
            &format!("{FLOOR}[auto_review]\nmodel = \"sonnet\"\n"),
            FLOOR_PATH,
        )
        .unwrap();
        let approved = approved_of(&relay);
        let mut rows: Vec<AutoRow> = (0..20)
            .map(|i| shadow_row(&format!("s{i}"), NOW - (i + 1) * MIN))
            .collect();
        let live = AutoRow {
            mode: ReviewMode::Live,
            cause: None,
            author_decision: None,
            ..shadow_row("live", NOW - 2 * MIN)
        };
        let refused = AutoRow {
            cause: Some(EscalationCause::HourlyCap),
            ..live.clone()
        };
        let old = AutoRow {
            requested_at_ms: NOW - MARK_WINDOW_MS - 1,
            call_started_at_ms: Some(NOW - MARK_WINDOW_MS - 1),
            ..live.clone()
        };
        rows.extend([live, refused, old]);
        let lines = [JournalLine::new(
            NOW - MIN,
            JournalEntry::RunStarted(RunStarted {
                approval_id: Some("live".to_owned()),
                ..run_started("r-live")
            }),
        )];
        let auto = AutoInputs {
            approval: AutoApprovalSetting {
                on: true,
                reason: None,
            },
            approval_src: Some(Source::Author),
            ..auto_inputs(&rows, &[])
        };
        let mut read = inputs(
            SnapshotState::Valid(&approved),
            FloorRead::Valid(&relay),
            &lines,
        );
        read.auto = Some(&auto);
        let view = build(&read).auto;
        assert!(view.approval_on);
        assert_eq!(view.approval_src, Some(Source::Author));
        assert_eq!(view.review_model, Some(ModelAlias::Sonnet));
        assert_eq!((view.stats.cases, view.stats.disagreements), (20, 0));
        assert!(view.stats.gate && view.stats.rule_offer);
        assert_eq!(view.stats.false_approve_bound_percent, Some(15.0));
        assert_eq!(view.calls_per_day, u32::try_from(CALLS_PER_DAY).unwrap());
        assert_eq!(view.mark_window_ms, MARK_WINDOW_MS);
        assert_eq!(
            view.runs
                .iter()
                .map(|run| (run.approval_id.as_str(), run.run_id.as_deref(), run.by))
                .collect::<Vec<_>>(),
            [("live", Some("r-live"), ApprovalBy::Review)],
            "an approval the authority refused, or older than the window, offers no mark"
        );
        let empty = build(&inputs(SnapshotState::Missing, FloorRead::Missing, &[])).auto;
        assert!(!empty.approval_on && empty.runs.is_empty());
        assert_eq!(empty.stats.false_approve_bound_percent, None);
        assert_eq!(empty.review_model, None);
    }

    fn run_started(run_id: &str) -> RunStarted {
        RunStarted {
            run_id: run_id.to_owned(),
            project: "lab".to_owned(),
            session_id: None,
            action: "apply".to_owned(),
            class: ActionClass::Auto,
            params: BTreeMap::new(),
            argv: Vec::new(),
            method: None,
            url: None,
            credential: None,
            catalogue_hash: "00".to_owned(),
            approval_id: None,
            tree: None,
            timeout_s: Some(60),
        }
    }

    fn decision_line(ts: i64, given: bool, by: Option<ApprovalBy>) -> JournalLine {
        let event = ApprovalEvent {
            approval_id: format!("0123456789abcdef-{ts}"),
            request_key: "ab".to_owned(),
            project: Some("lab".to_owned()),
            action: Some("apply".to_owned()),
            dry_run: None,
            by,
            reason: None,
        };
        JournalLine::new(
            ts,
            if given {
                JournalEntry::ApprovalGiven(event)
            } else {
                JournalEntry::ApprovalRejected(event)
            },
        )
    }

    /// Story 5.14 AC 7 (AD-92, D-142): the author's decisions per UTC day
    /// over the last 14, a line without `by` counting as his, apart from
    /// the review's and the rule's approvals; a request is no decision.
    #[test]
    fn author_decisions_per_day_count_human_and_auto() {
        let today = day_start_ms(utc_date(NOW));
        let lines = [
            decision_line(today + 1, true, None),
            decision_line(today + 2, false, Some(ApprovalBy::Author)),
            decision_line(today + 3, true, Some(ApprovalBy::Review)),
            decision_line(today + 4, true, Some(ApprovalBy::Rule)),
            decision_line(today - DAY_MS + 5, true, None),
            decision_line(today - DECISION_DAYS * DAY_MS + 6, true, None),
        ];
        let auto = auto_inputs(&[], &lines);
        let mut read = inputs(SnapshotState::Missing, FloorRead::Missing, &[]);
        read.auto = Some(&auto);
        let days = build(&read).auto.decisions;
        assert_eq!(days.len(), usize::try_from(DECISION_DAYS).unwrap());
        let last = days.last().unwrap();
        assert_eq!(last.day, utc_day_text(NOW));
        assert_eq!((last.author, last.auto), (2, 2));
        let yesterday = &days[days.len() - 2];
        assert_eq!((yesterday.author, yesterday.auto), (1, 0));
        assert_eq!(
            days.iter().map(|d| d.author + d.auto).sum::<u32>(),
            5,
            "a day older than the 14 does not count"
        );
    }

    /// Story 5.14 AC 7 (D-130, D-143): the author's decision of an `auto`
    /// request shows its review, reason included, once he decided; a
    /// review the author has not met shows on no entry.
    #[test]
    fn a_review_shows_only_after_the_author_decided() {
        let decided = shadow_row("0123456789abcdef-1", NOW - 2 * MIN);
        let pending = AutoRow {
            author_decision: None,
            approval_id: "0123456789abcdef-2".to_owned(),
            ..decided.clone()
        };
        let with_id = |mut line: JournalLine, id: &str| {
            if let JournalEntry::ApprovalGiven(event) | JournalEntry::ApprovalRejected(event) =
                &mut line.entry
            {
                event.approval_id = id.to_owned();
            }
            line
        };
        // The author's rejection of `pending` is journaled; the window has
        // not copied it onto the row yet.
        let lines = [
            with_id(
                decision_line(NOW - 2 * MIN, false, Some(ApprovalBy::Author)),
                &pending.approval_id,
            ),
            with_id(
                decision_line(NOW - MIN, true, Some(ApprovalBy::Author)),
                &decided.approval_id,
            ),
        ];
        let rows = [decided, pending];
        let auto = auto_inputs(&rows, &[]);
        let mut read = inputs(SnapshotState::Missing, FloorRead::Missing, &lines);
        read.auto = Some(&auto);
        let view = build(&read);
        let shown: Vec<_> = view.recent.iter().map(|e| e.review.clone()).collect();
        assert_eq!(shown.len(), 2);
        let review = shown[0]
            .as_ref()
            .expect("the decided request shows its review");
        assert_eq!(review.decision, Some(ReviewDecision::Approve));
        assert!(review.rule_baseline);
        let text = serde_json::to_string(&view.recent[0]).unwrap();
        assert!(text.contains("REVIEW-REASON-MARKER"));
        assert_eq!(shown[1], None, "not before the row holds his decision");
        let text = serde_json::to_string(&view.recent[1]).unwrap();
        assert!(!text.contains("REVIEW-REASON-MARKER"));
    }
}
