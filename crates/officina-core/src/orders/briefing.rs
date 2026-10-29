//! The briefing (AD-83, FR-1): what is open, what waits for the author and
//! what changed since a cursor, built by Officina's own code from the
//! board, the relay view, the registry view and the orders state, with no
//! model involved. The window and `officina mcp` call the same [`build`],
//! so the author and the orchestrator read the same briefing.
//!
//! Session names and every session text are [`Untrusted`]; subject,
//! template, project and action names are the author's own text, kept
//! plain. The serialized briefing is bounded by [`BRIEFING_MAX_BYTES`]:
//! while over, the globally oldest list item goes and its section counts
//! it in `dropped`; `count` always holds the whole number.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::board::{Card, Column, Columns};
use crate::orders::proposal::ProposalView;
use crate::registry::types::{
    LaunchOutcome, ProposalKind, ProposalStatus, RowKind, SubjectRecord, SubjectRef, SubjectStatus,
    WorkOrderRecord,
};
use crate::registry::view::{SessionView, SubjectView, TriageRow};
use crate::relay::approvals::ApprovalView;
use crate::relay::view::{RecentEntry, RecentEvent, RunState, Tone};
use crate::untrusted::Untrusted;

/// The most bytes a serialized briefing holds (AD-83), in the serialization
/// mode of the caller: inside [`crate::untrusted::wrapped`] the marked
/// form is the one measured.
pub const BRIEFING_MAX_BYTES: usize = 32 * 1024;

/// Since when `changed` counts (AD-83): the window's last reading of the
/// Orchestrator view, the orchestrator start before the current one for
/// `officina mcp`, or none on a first reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Cursor {
    /// `setting.briefing_read_at_ms`, the window's.
    ReadAt { at_ms: i64 },
    /// The previous orchestrator start, `officina mcp`'s (story 5.15).
    PreviousStart { at_ms: i64 },
    /// No reading yet: every item with a time is new.
    None,
}

impl Cursor {
    /// The cursor's time, when it has one.
    pub fn at_ms(self) -> Option<i64> {
        match self {
            Cursor::ReadAt { at_ms } | Cursor::PreviousStart { at_ms } => Some(at_ms),
            Cursor::None => None,
        }
    }

    /// True when something that happened at `at_ms` follows the cursor.
    ///
    /// ```
    /// use officina_core::orders::briefing::Cursor;
    /// assert!(Cursor::ReadAt { at_ms: 10 }.follows(11));
    /// assert!(!Cursor::ReadAt { at_ms: 10 }.follows(10));
    /// assert!(Cursor::None.follows(0));
    /// ```
    pub fn follows(self, at_ms: i64) -> bool {
        self.at_ms().is_none_or(|cursor| at_ms > cursor)
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Where a session stands, as the briefing and the work order history
    /// show it: its board column while the board shows it, else ended or
    /// unknown from the registry (missing data is unknown, never done).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum SessionPhase {
        NeedsYou,
        YourTurn,
        Working,
        ToResume,
        Done,
        /// Off the board, and the registry holds its end.
        Ended,
        /// Off the board, and nothing says how it stands.
        Unknown,
    }
}

impl From<Column> for SessionPhase {
    fn from(column: Column) -> SessionPhase {
        match column {
            Column::NeedsYou => SessionPhase::NeedsYou,
            Column::YourTurn => SessionPhase::YourTurn,
            Column::Working => SessionPhase::Working,
            Column::ToResume => SessionPhase::ToResume,
            Column::Done => SessionPhase::Done,
        }
    }
}

/// A session as a briefing line names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SessionBrief {
    pub session_id: String,
    pub name: Option<Untrusted<String>>,
    pub phase: SessionPhase,
}

/// One list of the briefing: `count` items in all, the `dropped` oldest of
/// them left out to hold the bound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Section<T> {
    pub count: u32,
    pub dropped: u32,
    pub items: Vec<T>,
}

impl<T> Section<T> {
    fn of(items: Vec<T>) -> Section<T> {
        Section {
            count: u32::try_from(items.len()).unwrap_or(u32::MAX),
            dropped: 0,
            items,
        }
    }
}

/// An open subject (`active` or `to_resume`) with its sessions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SubjectItem {
    pub id: i64,
    pub project: String,
    pub name: String,
    pub status: SubjectStatus,
    /// Its sessions, the latest seen first.
    pub sessions: Vec<SessionBrief>,
    /// Its status time or its latest session's sighting, whichever is later.
    pub at_ms: i64,
}

/// A Needs you or Your turn session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct WaitingItem {
    pub key: String,
    pub session_id: Option<String>,
    pub name: Untrusted<String>,
    pub project: String,
    pub column: Column,
    pub waiting_for: Option<String>,
    pub subject: Option<SubjectRef>,
    /// When it began to wait, when known.
    pub at_ms: Option<i64>,
}

/// A pending relay request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ApprovalItem {
    pub id: String,
    pub project: String,
    pub action: String,
    pub session_name: Option<Untrusted<String>>,
    pub valid: bool,
    pub expires_at_ms: i64,
    /// When it was requested.
    pub at_ms: i64,
}

/// A triage row waiting for the author's verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TriageItem {
    pub key: String,
    pub kind: RowKind,
    pub project: String,
    pub subject: Option<SubjectRef>,
    pub session_name: Option<Untrusted<String>>,
    /// When the session it rests on ended, when known.
    pub at_ms: Option<i64>,
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// How a relay entry of the runs section went wrong.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum RunKind {
        /// A run that ended without doing what it was asked.
        Failed,
        /// A run whose end line never came (AD-32).
        Abandoned,
        /// `officina run` refused before starting anything.
        Refused,
        /// The gate denied a command.
        GateDenied,
        /// The gate could not decide.
        GateError,
    }
}

/// A failed or refused run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RunItem {
    pub kind: RunKind,
    pub project: Option<String>,
    pub session_id: Option<String>,
    /// The action asked for: a refusal's is the session's own argument, so
    /// every one is held as untrusted.
    pub action: Option<Untrusted<String>>,
    pub exit_code: Option<i64>,
    /// The journal's code: the run's outcome, the refusal reason, the
    /// deny kind or the gate fault.
    pub reason: Option<String>,
    pub at_ms: i64,
}

/// A launched work order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OrderItem {
    pub id: String,
    pub name: String,
    pub template: String,
    pub project: String,
    pub outcome: LaunchOutcome,
    /// The session it bound to (AD-81 step 4), as it stands.
    pub session: Option<SessionBrief>,
    /// When it was launched.
    pub at_ms: i64,
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What kind of change a `changed` item is.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum ChangeKind {
        RunFinished,
        StartedWaiting,
        SubjectStatus,
        OrderLaunched,
        ProposalReceived,
    }
}

/// Something that happened after the cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChangedItem {
    /// A relay run ended, or was abandoned.
    RunFinished {
        project: String,
        action: String,
        succeeded: bool,
        at_ms: i64,
    },
    /// A session began to wait for the author.
    StartedWaiting {
        key: String,
        name: Untrusted<String>,
        column: Column,
        at_ms: i64,
    },
    /// A subject's status changed: archived, back to `to_resume` or
    /// `active` again (D-674).
    SubjectStatus {
        id: i64,
        project: String,
        name: String,
        status: SubjectStatus,
        at_ms: i64,
    },
    /// A work order was launched.
    OrderLaunched {
        id: String,
        name: String,
        outcome: LaunchOutcome,
        at_ms: i64,
    },
    /// The window received a proposal of the orchestrator (AD-91); `kind`
    /// is null for a file that did not parse.
    ProposalReceived {
        id: String,
        proposal: Option<ProposalKind>,
        status: ProposalStatus,
        at_ms: i64,
    },
}

impl ChangedItem {
    /// Its kind.
    pub fn kind(&self) -> ChangeKind {
        match self {
            ChangedItem::RunFinished { .. } => ChangeKind::RunFinished,
            ChangedItem::StartedWaiting { .. } => ChangeKind::StartedWaiting,
            ChangedItem::SubjectStatus { .. } => ChangeKind::SubjectStatus,
            ChangedItem::OrderLaunched { .. } => ChangeKind::OrderLaunched,
            ChangedItem::ProposalReceived { .. } => ChangeKind::ProposalReceived,
        }
    }

    /// When it happened.
    pub fn at_ms(&self) -> i64 {
        match self {
            ChangedItem::RunFinished { at_ms, .. }
            | ChangedItem::StartedWaiting { at_ms, .. }
            | ChangedItem::SubjectStatus { at_ms, .. }
            | ChangedItem::OrderLaunched { at_ms, .. }
            | ChangedItem::ProposalReceived { at_ms, .. } => *at_ms,
        }
    }
}

/// The briefing (AD-83).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Briefing {
    pub generated_at_ms: i64,
    pub cursor: Cursor,
    pub subjects: Section<SubjectItem>,
    pub waiting: Section<WaitingItem>,
    pub approvals: Section<ApprovalItem>,
    pub triage: Section<TriageItem>,
    pub runs: Section<RunItem>,
    pub work_orders: Section<OrderItem>,
    /// The orchestrator's pending proposals, newest first (AD-91), every
    /// text untrusted.
    pub proposals: Section<ProposalView>,
    /// Newest first.
    pub changed: Section<ChangedItem>,
}

/// What the briefing is built from (AD-83): the board's cards and pending
/// requests, the relay view's recent entries, the registry view's open
/// subjects and triage rows, and the orders state: every subject row, the
/// latest work orders and the proposal rows, newest first. A payload the
/// window does not hold yet is empty.
#[derive(Debug, Clone, Copy, Default)]
pub struct BriefingInput<'a> {
    pub columns: Option<&'a Columns>,
    pub approvals: &'a [ApprovalView],
    pub recent: &'a [RecentEntry],
    pub open_subjects: &'a [SubjectView],
    pub triage: &'a [TriageRow],
    pub subjects: &'a [SubjectRecord],
    pub work_orders: &'a [WorkOrderRecord],
    pub proposals: &'a [ProposalView],
}

impl BriefingInput<'_> {
    fn cards(&self) -> impl Iterator<Item = &Card> {
        self.columns.into_iter().flat_map(Columns::all)
    }

    fn waiting_cards(&self) -> impl Iterator<Item = &Card> {
        self.columns
            .into_iter()
            .flat_map(|columns| columns.needs_you.iter().chain(&columns.your_turn))
    }

    /// The board card of `session_id`, when the board shows it.
    fn card_of(&self, session_id: &str) -> Option<&Card> {
        self.cards()
            .find(|card| card.session_id.as_deref() == Some(session_id))
    }
}

/// When a waiting card began to wait: the scene's wait start, else its
/// latest event.
fn waiting_since(card: &Card) -> Option<i64> {
    card.scene
        .as_ref()
        .and_then(|scene| scene.waiting_since_ms)
        .or(card.last_event_at_ms)
}

fn session_brief(session: &SessionView) -> SessionBrief {
    let phase = match (&session.card, &session.ended) {
        (Some(card), _) => card.column.into(),
        (None, Some(_)) => SessionPhase::Ended,
        (None, None) => SessionPhase::Unknown,
    };
    SessionBrief {
        session_id: session.session_id.clone(),
        name: session.name.clone(),
        phase,
    }
}

/// The session `order` bound to, as it stands: its board card, else its
/// registry record's end.
pub fn order_session(input: &BriefingInput, order: &WorkOrderRecord) -> Option<SessionBrief> {
    let session_id = order.session_id.as_ref()?;
    let card = input.card_of(session_id);
    let phase = match (card, &order.session) {
        (Some(card), _) => card.column.into(),
        (None, Some(record)) if record.is_ended() => SessionPhase::Ended,
        (None, _) => SessionPhase::Unknown,
    };
    let name = card.map(|card| card.name.clone()).or_else(|| {
        order
            .session
            .as_ref()
            .and_then(|record| record.name.as_ref())
            .map(|name| Untrusted::new(name.value.clone()))
    });
    Some(SessionBrief {
        session_id: session_id.clone(),
        name,
        phase,
    })
}

fn subject_item(subject: &SubjectView) -> SubjectItem {
    let latest = subject.sessions.iter().map(|s| s.last_seen_ms).max();
    SubjectItem {
        id: subject.id,
        project: subject.project.clone(),
        name: subject.name.clone(),
        status: subject.status,
        sessions: subject.sessions.iter().map(session_brief).collect(),
        at_ms: latest.map_or(subject.status_at_ms, |l| l.max(subject.status_at_ms)),
    }
}

fn waiting_item(card: &Card) -> WaitingItem {
    WaitingItem {
        key: card.key.clone(),
        session_id: card.session_id.clone(),
        name: card.name.clone(),
        project: card.project.name.clone(),
        column: card.column,
        waiting_for: card.waiting_for.clone(),
        subject: card.subject.clone(),
        at_ms: waiting_since(card),
    }
}

fn approval_item(approval: &ApprovalView) -> ApprovalItem {
    ApprovalItem {
        id: approval.id.clone(),
        project: approval.project.clone(),
        action: approval.action.clone(),
        session_name: approval.session_name.clone(),
        valid: approval.valid,
        expires_at_ms: approval.expires_at_ms,
        at_ms: approval.requested_at_ms,
    }
}

fn triage_item(row: &TriageRow) -> TriageItem {
    TriageItem {
        key: row.key.clone(),
        kind: row.kind,
        project: row.project.clone(),
        subject: row.subject.clone(),
        session_name: row.session_name.clone(),
        at_ms: row.end.as_ref().map(|end| end.at_ms),
    }
}

/// The entry as a runs section line, when it failed or was refused.
fn run_item(entry: &RecentEntry) -> Option<RunItem> {
    let item = |kind, project, session_id, action, reason: &str| RunItem {
        kind,
        project,
        session_id,
        action,
        exit_code: None,
        reason: Some(reason.to_owned()),
        at_ms: entry.ts,
    };
    match &entry.event {
        RecentEvent::Run(run) => {
            let kind = match (run.state, &run.finished) {
                (RunState::Abandoned, _) => RunKind::Abandoned,
                (RunState::Finished, Some(finished)) if !finished.succeeded() => RunKind::Failed,
                _ => return None,
            };
            Some(RunItem {
                kind,
                project: Some(run.started.project.clone()),
                session_id: run.started.session_id.clone(),
                action: Some(Untrusted::new(run.started.action.clone())),
                exit_code: run.finished.as_ref().and_then(|f| f.exit_code),
                reason: run.finished.as_ref().map(|f| f.outcome.code().to_owned()),
                at_ms: run.finished_at_ms.unwrap_or(entry.ts),
            })
        }
        RecentEvent::Refused(refused) => Some(item(
            RunKind::Refused,
            refused.project.clone(),
            refused.session_id.clone(),
            Some(refused.action.clone()),
            refused.reason.code(),
        )),
        RecentEvent::GateDenied(denied) => Some(item(
            RunKind::GateDenied,
            Some(denied.project.clone()),
            denied.session_id.clone(),
            None,
            denied.deny.code(),
        )),
        RecentEvent::GateError(error) => Some(item(
            RunKind::GateError,
            error.project.clone(),
            error.session_id.clone(),
            None,
            error.cause.code(),
        )),
        RecentEvent::CatalogueApproved(_)
        | RecentEvent::ApprovalRequested(_)
        | RecentEvent::ApprovalGiven(_)
        | RecentEvent::ApprovalRejected(_)
        | RecentEvent::RelayStopped
        | RecentEvent::RelayResumed => None,
    }
}

fn order_item(input: &BriefingInput, order: &WorkOrderRecord) -> OrderItem {
    OrderItem {
        id: order.id.clone(),
        name: order.name.clone(),
        template: order.template.clone(),
        project: order.project.clone(),
        outcome: order.outcome,
        session: order_session(input, order),
        at_ms: order.launched_at_ms,
    }
}

/// Everything with a time after `cursor`, newest first.
fn changed(input: &BriefingInput, cursor: Cursor) -> Vec<ChangedItem> {
    let runs = input.recent.iter().filter_map(|entry| match &entry.event {
        RecentEvent::Run(run) if run.state != RunState::Running => Some(ChangedItem::RunFinished {
            project: run.started.project.clone(),
            action: run.started.action.clone(),
            succeeded: entry.tone == Some(Tone::Ok),
            at_ms: run.finished_at_ms.unwrap_or(entry.ts),
        }),
        _ => None,
    });
    let waiting = input.waiting_cards().filter_map(|card| {
        Some(ChangedItem::StartedWaiting {
            key: card.key.clone(),
            name: card.name.clone(),
            column: card.column,
            at_ms: waiting_since(card)?,
        })
    });
    // A subject whose status never moved since its creation did not change;
    // a merged one left the lists (D-55).
    let subjects = input
        .subjects
        .iter()
        .filter(|s| s.merged_into.is_none() && s.status.at_ms != s.created_at_ms)
        .map(|s| ChangedItem::SubjectStatus {
            id: s.id,
            project: s.project.clone(),
            name: s.name.clone(),
            status: s.status.value,
            at_ms: s.status.at_ms,
        });
    let orders = input
        .work_orders
        .iter()
        .map(|order| ChangedItem::OrderLaunched {
            id: order.id.clone(),
            name: order.name.clone(),
            outcome: order.outcome,
            at_ms: order.launched_at_ms,
        });
    // A file read since the cursor; one waiting as a file has no receipt.
    let proposals = input.proposals.iter().filter_map(|proposal| {
        Some(ChangedItem::ProposalReceived {
            id: proposal.id.clone(),
            proposal: proposal.kind,
            status: proposal.status,
            at_ms: proposal.received_at_ms?,
        })
    });
    let mut items: Vec<ChangedItem> = runs
        .chain(waiting)
        .chain(subjects)
        .chain(orders)
        .chain(proposals)
        .filter(|item| cursor.follows(item.at_ms()))
        .collect();
    items.sort_by_key(|item| std::cmp::Reverse(item.at_ms()));
    items
}

/// The briefing at `now_ms` (AD-83): each section from `input`, `changed`
/// from `cursor`, held within [`BRIEFING_MAX_BYTES`]. The same input gives
/// the same bytes.
pub fn build(input: &BriefingInput, cursor: Cursor, now_ms: i64) -> Briefing {
    let mut briefing = assemble(input, cursor, now_ms);
    hold_the_bound(&mut briefing);
    briefing
}

/// Every section whole, before the bound.
fn assemble(input: &BriefingInput, cursor: Cursor, now_ms: i64) -> Briefing {
    Briefing {
        generated_at_ms: now_ms,
        cursor,
        subjects: Section::of(input.open_subjects.iter().map(subject_item).collect()),
        waiting: Section::of(input.waiting_cards().map(waiting_item).collect()),
        approvals: Section::of(input.approvals.iter().map(approval_item).collect()),
        triage: Section::of(input.triage.iter().map(triage_item).collect()),
        runs: Section::of(input.recent.iter().filter_map(run_item).collect()),
        work_orders: Section::of(
            input
                .work_orders
                .iter()
                .map(|order| order_item(input, order))
                .collect(),
        ),
        proposals: Section::of(
            input
                .proposals
                .iter()
                .filter(|p| p.status == ProposalStatus::Pending)
                .cloned()
                .collect(),
        ),
        changed: Section::of(changed(input, cursor)),
    }
}

/// An item's time for the drop order; an unknown time is the oldest.
trait Dated {
    fn dated(&self) -> Option<i64>;
}

impl Dated for SubjectItem {
    fn dated(&self) -> Option<i64> {
        Some(self.at_ms)
    }
}

impl Dated for WaitingItem {
    fn dated(&self) -> Option<i64> {
        self.at_ms
    }
}

impl Dated for ApprovalItem {
    fn dated(&self) -> Option<i64> {
        Some(self.at_ms)
    }
}

impl Dated for TriageItem {
    fn dated(&self) -> Option<i64> {
        self.at_ms
    }
}

impl Dated for RunItem {
    fn dated(&self) -> Option<i64> {
        Some(self.at_ms)
    }
}

impl Dated for OrderItem {
    fn dated(&self) -> Option<i64> {
        Some(self.at_ms)
    }
}

impl Dated for ProposalView {
    fn dated(&self) -> Option<i64> {
        Some(self.written_at_ms)
    }
}

impl Dated for ChangedItem {
    fn dated(&self) -> Option<i64> {
        Some(self.at_ms())
    }
}

/// One list item as the bound sees it.
struct Candidate {
    at_ms: Option<i64>,
    section: usize,
    index: usize,
    bytes: usize,
}

fn serialized_len<T: Serialize + ?Sized>(value: &T) -> usize {
    serde_json::to_vec(value).map_or(0, |bytes| bytes.len())
}

fn candidates_of<T: Serialize + Dated>(
    section: &Section<T>,
    number: usize,
    out: &mut Vec<Candidate>,
) {
    out.extend(
        section
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| Candidate {
                at_ms: item.dated(),
                section: number,
                index,
                bytes: serialized_len(item),
            }),
    );
}

/// Removes the items of `section` (number `number`) that `gone` names and
/// counts them in `dropped`.
fn drop_from<T>(section: &mut Section<T>, number: usize, gone: &BTreeSet<(usize, usize)>) {
    let before = section.items.len();
    let mut index = 0;
    section.items.retain(|_| {
        let keep = !gone.contains(&(number, index));
        index += 1;
        keep
    });
    let removed = before - section.items.len();
    section.dropped += u32::try_from(removed).unwrap_or(u32::MAX);
}

/// How many lists [`Briefing::candidates`] numbers.
const SECTION_LISTS: usize = 8;

impl Briefing {
    /// Every list item, numbered by section in the payload's order.
    fn candidates(&self) -> Vec<Candidate> {
        let mut out = Vec::new();
        candidates_of(&self.subjects, 0, &mut out);
        candidates_of(&self.waiting, 1, &mut out);
        candidates_of(&self.approvals, 2, &mut out);
        candidates_of(&self.triage, 3, &mut out);
        candidates_of(&self.runs, 4, &mut out);
        candidates_of(&self.work_orders, 5, &mut out);
        candidates_of(&self.proposals, 6, &mut out);
        candidates_of(&self.changed, 7, &mut out);
        out
    }

    fn drop_items(&mut self, gone: &BTreeSet<(usize, usize)>) {
        drop_from(&mut self.subjects, 0, gone);
        drop_from(&mut self.waiting, 1, gone);
        drop_from(&mut self.approvals, 2, gone);
        drop_from(&mut self.triage, 3, gone);
        drop_from(&mut self.runs, 4, gone);
        drop_from(&mut self.work_orders, 5, gone);
        drop_from(&mut self.proposals, 6, gone);
        drop_from(&mut self.changed, 7, gone);
    }
}

/// The order items go in: oldest first; equal times in payload order, the
/// later item of a list first, as lists run newest first.
fn drop_order(a: &Candidate, b: &Candidate) -> std::cmp::Ordering {
    a.at_ms
        .cmp(&b.at_ms)
        .then(a.section.cmp(&b.section))
        .then(b.index.cmp(&a.index))
}

fn digits(n: u32) -> usize {
    n.checked_ilog10().map_or(1, |log| log as usize + 1)
}

/// Drops the globally oldest list items, one at a time, until the
/// serialized briefing fits [`BRIEFING_MAX_BYTES`] (AD-83). Each drop's
/// saving is known from the item's own bytes, its comma and the digits its
/// section's `dropped` gains, so the briefing is serialized once to start
/// and once to confirm, not once per item.
fn hold_the_bound(briefing: &mut Briefing) {
    loop {
        let mut total = serialized_len(briefing);
        if total <= BRIEFING_MAX_BYTES {
            return;
        }
        let mut candidates = briefing.candidates();
        if candidates.is_empty() {
            return;
        }
        candidates.sort_by(drop_order);
        let mut left: Vec<usize> = vec![0; SECTION_LISTS];
        for candidate in &candidates {
            left[candidate.section] += 1;
        }
        let mut dropped: Vec<u32> = vec![0; SECTION_LISTS];
        let mut gone = BTreeSet::new();
        for candidate in &candidates {
            if total <= BRIEFING_MAX_BYTES {
                break;
            }
            let section = candidate.section;
            let comma = usize::from(left[section] > 1);
            total -= candidate.bytes + comma;
            left[section] -= 1;
            let before = dropped[section];
            dropped[section] += 1;
            total += digits(dropped[section]) - digits(before);
            gone.insert((section, candidate.index));
        }
        briefing.drop_items(&gone);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Card;
    use crate::registry::types::{Source, Sourced, SubjectStatus};
    use crate::registry::view::{EndHow, EndView, LiveCard};
    use crate::untrusted::wrapped;

    const NOW: i64 = 1_800_000_000_000;
    const MIN: i64 = 60_000;

    fn card(key: &str, column: Column, name: &str, at_ms: i64) -> Card {
        serde_json::from_value(serde_json::json!({
            "key": key,
            "column": column,
            "name": name,
            "project": { "name": "backend", "color": null, "priority": "normal" },
            "session_id": format!("s-{key}"),
            "last_event_at_ms": at_ms,
            "attach_available": false,
            "subagents": [],
        }))
        .unwrap()
    }

    fn session(id: &str, name: &str, card: Option<Column>, ended: bool) -> SessionView {
        SessionView {
            session_id: id.to_owned(),
            project: "backend".to_owned(),
            name: Some(Untrusted::new(name.to_owned())),
            kind: None,
            first_seen_ms: NOW - 60 * MIN,
            last_seen_ms: NOW - 10 * MIN,
            ended: ended.then_some(EndView {
                how: EndHow::SessionEnd,
                reason: None,
                src: Source::Hook,
                at_ms: NOW - 10 * MIN,
            }),
            superseded: false,
            non_interactive: false,
            card: card.map(|column| LiveCard {
                key: id.to_owned(),
                column,
                badge: None,
                waiting_for: None,
            }),
        }
    }

    fn open_subject(id: i64, name: &str, sessions: Vec<SessionView>) -> SubjectView {
        SubjectView {
            id,
            project: "backend".to_owned(),
            name: name.to_owned(),
            status: SubjectStatus::Active,
            status_src: Source::Author,
            status_at_ms: NOW - 120 * MIN,
            routine: false,
            sessions,
            excerpts: None,
        }
    }

    fn subject_row(id: i64, status: SubjectStatus, at_ms: i64) -> SubjectRecord {
        SubjectRecord {
            id,
            project: "backend".to_owned(),
            name: format!("subject {id}"),
            status: Sourced {
                value: status,
                src: Source::Author,
                at_ms,
            },
            created_at_ms: NOW - 600 * MIN,
            routine: false,
            merged_into: None,
        }
    }

    fn recent(json: serde_json::Value) -> RecentEntry {
        let mut entry: RecentEntry = serde_json::from_value(json).unwrap();
        entry.tone = crate::relay::view::tone_of(&entry.event);
        entry
    }

    fn finished_run(ts: i64, exit_code: i64) -> RecentEntry {
        recent(serde_json::json!({
            "ts": ts,
            "tone": null,
            "event": {
                "kind": "run",
                "state": "finished",
                "started": {
                    "run_id": format!("r{ts}"),
                    "project": "backend",
                    "session_id": "s-a",
                    "action": "lint",
                    "class": "free",
                    "params": {},
                    "argv": ["lint"],
                    "catalogue_hash": "h",
                },
                "finished": {
                    "run_id": format!("r{ts}"),
                    "outcome": "exited",
                    "exit_code": exit_code,
                    "duration_ms": 10,
                },
                "finished_at_ms": ts + 10,
            }
        }))
    }

    fn refused(ts: i64, action: &str) -> RecentEntry {
        recent(serde_json::json!({
            "ts": ts,
            "tone": null,
            "event": {
                "kind": "refused",
                "project": "backend",
                "session_id": "s-a",
                "action": action,
                "reason": "action_not_allowed",
                "detail": "no such action",
            }
        }))
    }

    fn order(id: &str, launched_at_ms: i64, session_id: Option<&str>) -> WorkOrderRecord {
        WorkOrderRecord {
            id: id.to_owned(),
            seq: 1,
            template: "dependency-bump".to_owned(),
            project: "backend".to_owned(),
            cwd: "C:/work/backend".to_owned(),
            values: std::collections::BTreeMap::new(),
            argv: Vec::new(),
            prompt_sha256: String::new(),
            catalogue_hash: String::new(),
            name: format!("dependency-bump-{id}"),
            subject_id: None,
            new_subject_name: None,
            source: crate::registry::types::WorkOrderSource::Composer,
            proposal_id: None,
            launched_at_ms,
            outcome: LaunchOutcome::Started,
            exit_code: Some(0),
            error: None,
            printed: None,
            bg_id: session_id.map(|_| "bg1".to_owned()),
            session_id: session_id.map(str::to_owned),
            bound_at_ms: session_id.map(|_| launched_at_ms + 1_000),
            session: None,
        }
    }

    struct Fixture {
        columns: Columns,
        approvals: Vec<ApprovalView>,
        recent: Vec<RecentEntry>,
        open: Vec<SubjectView>,
        triage: Vec<TriageRow>,
        subjects: Vec<SubjectRecord>,
        orders: Vec<WorkOrderRecord>,
        proposals: Vec<ProposalView>,
    }

    impl Fixture {
        fn input(&self) -> BriefingInput<'_> {
            BriefingInput {
                columns: Some(&self.columns),
                approvals: &self.approvals,
                recent: &self.recent,
                open_subjects: &self.open,
                triage: &self.triage,
                subjects: &self.subjects,
                work_orders: &self.orders,
                proposals: &self.proposals,
            }
        }
    }

    fn approval(id: &str, at_ms: i64) -> ApprovalView {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "project": "backend",
            "action": "deploy",
            "class": "human",
            "state": "pending",
            "description": "",
            "argv": [],
            "http": null,
            "valid": true,
            "invalid_reason": null,
            "session_id": "s-a",
            "session_name": "<b>deployer</b>",
            "requested_at_ms": at_ms,
            "expires_at_ms": at_ms + 10 * MIN,
            "dry_run": null,
            "binding": null,
            "bound_files": null,
            "files_unchanged": null,
        }))
        .unwrap()
    }

    fn triage_row(key: &str, at_ms: i64) -> TriageRow {
        serde_json::from_value(serde_json::json!({
            "key": key,
            "kind": "unfiled_session",
            "project": "backend",
            "subject": null,
            "session_id": "s-t",
            "session_name": "<i>triaged</i>",
            "proposal": { "verdict": "to_resume", "source": "rules", "reason": "default_to_resume" },
            "verdict": "to_resume",
            "excerpts": null,
            "end": { "how": "session_end", "reason": null, "src": "hook", "at_ms": at_ms },
            "proposed_subject": null,
            "choices": [],
            "model_pending": false,
            "model_reason": null,
            "model_call": null,
        }))
        .unwrap()
    }

    fn day() -> Fixture {
        let mut columns = Columns::default();
        columns.needs_you.push(card(
            "a",
            Column::NeedsYou,
            "<script>a</script>",
            NOW - 5 * MIN,
        ));
        columns
            .your_turn
            .push(card("b", Column::YourTurn, "turn", NOW - 30 * MIN));
        columns
            .working
            .push(card("c", Column::Working, "busy", NOW - MIN));
        Fixture {
            columns,
            approvals: vec![approval("req1", NOW - 3 * MIN)],
            recent: vec![
                finished_run(NOW - 2 * MIN, 1),
                refused(NOW - 40 * MIN, "<img src=x>"),
                finished_run(NOW - 50 * MIN, 0),
            ],
            open: vec![open_subject(
                1,
                "Rollout",
                vec![
                    session("s-c", "busy", Some(Column::Working), false),
                    session("s-old", "old", None, true),
                    session("s-lost", "lost", None, false),
                ],
            )],
            triage: vec![triage_row("session:s-t", NOW - 90 * MIN)],
            subjects: vec![
                subject_row(1, SubjectStatus::Active, NOW - 600 * MIN),
                subject_row(2, SubjectStatus::Archived, NOW - 20 * MIN),
                subject_row(3, SubjectStatus::ToResume, NOW - 200 * MIN),
            ],
            orders: vec![order("o1", NOW - 15 * MIN, Some("s-c"))],
            proposals: vec![
                proposal(
                    "0000000000003-0000000000000001",
                    ProposalStatus::Pending,
                    NOW - 60 * MIN,
                ),
                proposal(
                    "0000000000002-0000000000000001",
                    ProposalStatus::Dismissed,
                    NOW - 70 * MIN,
                ),
            ],
        }
    }

    /// A recap of subject 1 received at `received_at_ms`, with `status`.
    fn proposal(id: &str, status: ProposalStatus, received_at_ms: i64) -> ProposalView {
        use crate::orders::proposal::{ProposalFile, ProposalTarget, RecapProposal};
        let file = ProposalFile::recap(
            RecapProposal {
                target: ProposalTarget::SubjectId(1),
                done: "<b>merged</b>".into(),
                blocking: "nothing".into(),
                decision_expected: "release?".into(),
            },
            None,
            received_at_ms,
        );
        ProposalView {
            status,
            received_at_ms: Some(received_at_ms),
            ..file.pending_view(id)
        }
    }

    /// AD-83, AD-91: the pending proposals are the briefing's section,
    /// texts untrusted; every proposal the window received since the
    /// cursor is a change, a file not received yet none.
    #[test]
    fn pending_proposals_are_listed_and_received_ones_are_changed() {
        let mut fixture = day();
        let waiting = ProposalView {
            received_at_ms: None,
            ..proposal(
                "0000000000004-0000000000000001",
                ProposalStatus::Pending,
                NOW,
            )
        };
        fixture.proposals.push(waiting);
        let briefing = build(
            &fixture.input(),
            Cursor::ReadAt {
                at_ms: NOW - 65 * MIN,
            },
            NOW,
        );
        let listed: Vec<&str> = briefing
            .proposals
            .items
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(
            listed,
            [
                "0000000000003-0000000000000001",
                "0000000000004-0000000000000001"
            ],
            "pending only"
        );
        let received: Vec<&ChangedItem> = briefing
            .changed
            .items
            .iter()
            .filter(|c| c.kind() == ChangeKind::ProposalReceived)
            .collect();
        assert_eq!(
            received,
            [&ChangedItem::ProposalReceived {
                id: "0000000000003-0000000000000001".into(),
                proposal: Some(ProposalKind::Recap),
                status: ProposalStatus::Pending,
                at_ms: NOW - 60 * MIN,
            }],
            "after the cursor, received"
        );
        let marked = wrapped(|| serde_json::to_value(&briefing)).unwrap();
        assert_eq!(
            marked["proposals"]["items"][0]["content"]["recap"]["done"],
            serde_json::json!({ "untrusted": "<b>merged</b>" })
        );
    }

    #[test]
    fn each_section_lists_its_items() {
        let fixture = day();
        let briefing = build(&fixture.input(), Cursor::None, NOW);
        assert_eq!(briefing.subjects.items[0].name, "Rollout");
        let phases: Vec<SessionPhase> = briefing.subjects.items[0]
            .sessions
            .iter()
            .map(|s| s.phase)
            .collect();
        assert_eq!(
            phases,
            [
                SessionPhase::Working,
                SessionPhase::Ended,
                SessionPhase::Unknown
            ]
        );
        let waiting: Vec<(&str, Column)> = briefing
            .waiting
            .items
            .iter()
            .map(|w| (w.key.as_str(), w.column))
            .collect();
        assert_eq!(
            waiting,
            [("a", Column::NeedsYou), ("b", Column::YourTurn)],
            "Needs you and Your turn only"
        );
        assert_eq!(briefing.approvals.items[0].id, "req1");
        assert_eq!(briefing.triage.items[0].key, "session:s-t");
        let runs: Vec<(RunKind, Option<i64>)> = briefing
            .runs
            .items
            .iter()
            .map(|r| (r.kind, r.exit_code))
            .collect();
        assert_eq!(
            runs,
            [(RunKind::Failed, Some(1)), (RunKind::Refused, None)],
            "the run that succeeded is no failure"
        );
        let order = &briefing.work_orders.items[0];
        assert_eq!(order.id, "o1");
        assert_eq!(
            order.session.as_ref().map(|s| s.phase),
            Some(SessionPhase::Working),
            "the bound session's board column"
        );
        assert_eq!(briefing.waiting.count, 2);
        for count in [
            briefing.subjects.count,
            briefing.approvals.count,
            briefing.triage.count,
            briefing.work_orders.count,
        ] {
            assert_eq!(count, 1);
        }
        assert_eq!(briefing.runs.count, 2);
    }

    #[test]
    fn changed_holds_only_items_after_the_cursor() {
        let fixture = day();
        let cursor = Cursor::ReadAt {
            at_ms: NOW - 25 * MIN,
        };
        let briefing = build(&fixture.input(), cursor, NOW);
        let changed: Vec<(ChangeKind, i64)> = briefing
            .changed
            .items
            .iter()
            .map(|c| (c.kind(), c.at_ms()))
            .collect();
        assert_eq!(
            changed,
            [
                (ChangeKind::RunFinished, NOW - 2 * MIN + 10),
                (ChangeKind::StartedWaiting, NOW - 5 * MIN),
                (ChangeKind::OrderLaunched, NOW - 15 * MIN),
                (ChangeKind::SubjectStatus, NOW - 20 * MIN),
            ],
            "newest first, nothing at or before the cursor"
        );
        assert_eq!(briefing.cursor, cursor);
        // The sections do not depend on the cursor.
        let none = build(&fixture.input(), Cursor::None, NOW);
        assert_eq!(none.waiting, briefing.waiting);
        assert_eq!(none.runs, briefing.runs);
    }

    #[test]
    fn none_cursor_marks_everything_new() {
        let fixture = day();
        let briefing = build(&fixture.input(), Cursor::None, NOW);
        let kinds: BTreeSet<&str> = briefing
            .changed
            .items
            .iter()
            .map(|c| c.kind().code())
            .collect();
        assert_eq!(
            kinds,
            ChangeKind::ALL.iter().map(|k| k.code()).collect(),
            "every kind with a time"
        );
        // Every timed item: two finished runs, two waiting cards, two
        // subjects whose status moved, one order, two proposals received.
        assert_eq!(briefing.changed.count, 9);
        let times: Vec<i64> = briefing
            .changed
            .items
            .iter()
            .map(ChangedItem::at_ms)
            .collect();
        assert!(times.windows(2).all(|pair| pair[0] >= pair[1]));
    }

    #[test]
    fn session_texts_are_untrusted() {
        let fixture = day();
        let briefing = build(&fixture.input(), Cursor::None, NOW);
        let marked = wrapped(|| serde_json::to_value(&briefing)).unwrap();
        assert_eq!(
            marked["waiting"]["items"][0]["name"],
            serde_json::json!({ "untrusted": "<script>a</script>" })
        );
        assert_eq!(
            marked["approvals"]["items"][0]["session_name"],
            serde_json::json!({ "untrusted": "<b>deployer</b>" })
        );
        assert_eq!(
            marked["triage"]["items"][0]["session_name"],
            serde_json::json!({ "untrusted": "<i>triaged</i>" })
        );
        assert_eq!(
            marked["runs"]["items"][1]["action"],
            serde_json::json!({ "untrusted": "<img src=x>" })
        );
        assert_eq!(
            marked["subjects"]["items"][0]["sessions"][0]["name"],
            serde_json::json!({ "untrusted": "busy" })
        );
        // Author text stays plain.
        assert_eq!(marked["subjects"]["items"][0]["name"], "Rollout");
        assert_eq!(
            marked["work_orders"]["items"][0]["template"],
            "dependency-bump"
        );
        let started = marked["changed"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["kind"] == "started_waiting" && c["key"] == "a")
            .unwrap();
        assert_eq!(
            started["name"],
            serde_json::json!({ "untrusted": "<script>a</script>" })
        );
    }

    /// Many waiting sessions, each newer than the last, with long names.
    fn crowded(cards: usize) -> Fixture {
        let mut fixture = day();
        fixture.columns.needs_you = (0..cards)
            .map(|i| {
                let at = NOW - 10_000 * MIN + i64::try_from(i).unwrap() * MIN;
                card(&format!("k{i:04}"), Column::NeedsYou, &"n".repeat(200), at)
            })
            .collect();
        fixture
    }

    fn dropped_total(briefing: &Briefing) -> u32 {
        [
            briefing.subjects.dropped,
            briefing.waiting.dropped,
            briefing.approvals.dropped,
            briefing.triage.dropped,
            briefing.runs.dropped,
            briefing.work_orders.dropped,
            briefing.proposals.dropped,
            briefing.changed.dropped,
        ]
        .iter()
        .sum()
    }

    #[test]
    fn an_oversized_briefing_drops_the_oldest_items_first() {
        let fixture = crowded(300);
        let briefing = build(&fixture.input(), Cursor::None, NOW);
        let bytes = serde_json::to_vec(&briefing).unwrap();
        assert!(bytes.len() <= BRIEFING_MAX_BYTES, "{}", bytes.len());
        assert!(briefing.waiting.dropped > 0 && briefing.changed.dropped > 0);
        let whole = assemble(&fixture.input(), Cursor::None, NOW);
        let all = whole.candidates();
        let kept = briefing.candidates();
        let dropped = all.len() - kept.len();
        assert_eq!(usize::try_from(dropped_total(&briefing)).unwrap(), dropped);
        // Every item kept is at least as new as every item dropped.
        let kept_oldest = kept.iter().map(|c| c.at_ms).min().unwrap();
        let mut times: Vec<Option<i64>> = all.iter().map(|c| c.at_ms).collect();
        times.sort_unstable();
        assert!(
            times[..dropped].iter().all(|&t| t <= kept_oldest),
            "the oldest went first"
        );
        let keys: Vec<&str> = briefing
            .waiting
            .items
            .iter()
            .map(|w| w.key.as_str())
            .collect();
        assert!(
            keys.contains(&"k0299") && keys.contains(&"b") && !keys.contains(&"k0000"),
            "the newest waiting cards stay, the oldest go: {keys:?}"
        );
        // No more than it must: putting back the newest item dropped would
        // go over the bound again.
        let mut one_fewer = whole.clone();
        let mut order = all;
        order.sort_by(drop_order);
        let gone: BTreeSet<(usize, usize)> = order[..dropped - 1]
            .iter()
            .map(|c| (c.section, c.index))
            .collect();
        one_fewer.drop_items(&gone);
        assert!(serde_json::to_vec(&one_fewer).unwrap().len() > BRIEFING_MAX_BYTES);
    }

    #[test]
    fn counts_survive_every_drop() {
        for cards in [150, 300, 600] {
            let fixture = crowded(cards);
            let briefing = build(&fixture.input(), Cursor::None, NOW);
            // The crowd in Needs you, and the fixture's Your turn card.
            let total = u32::try_from(cards).unwrap() + 1;
            assert_eq!(briefing.waiting.count, total);
            assert_eq!(
                briefing.waiting.count,
                u32::try_from(briefing.waiting.items.len()).unwrap() + briefing.waiting.dropped
            );
            assert_eq!(
                briefing.changed.count,
                u32::try_from(briefing.changed.items.len()).unwrap() + briefing.changed.dropped
            );
            assert!(serde_json::to_vec(&briefing).unwrap().len() <= BRIEFING_MAX_BYTES);
        }
    }

    #[test]
    fn same_input_same_bytes() {
        let fixture = crowded(400);
        let first = serde_json::to_vec(&build(&fixture.input(), Cursor::None, NOW)).unwrap();
        let second = serde_json::to_vec(&build(&fixture.input(), Cursor::None, NOW)).unwrap();
        assert_eq!(first, second);
        // The marked form is bounded in its own mode.
        let marked =
            wrapped(|| serde_json::to_vec(&build(&fixture.input(), Cursor::None, NOW))).unwrap();
        assert!(marked.len() <= BRIEFING_MAX_BYTES);
    }
}
