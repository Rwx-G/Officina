//! Classification and board construction (FR-6, FR-7, AD-11, AD-13).
//!
//! `build_board` is the one function that decides what the author sees. It is
//! pure: the same input gives the same board, whoever calls it (the CLI, the
//! GUI collector, the vector tests).

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::agents::{AgentRecord, LifecycleState, LiveStatus, SessionKind};
use crate::events::EventLine;
use crate::floor::{ALERT_AFTER_MINUTES_DEFAULT, Priority, ProjectColor};
use crate::orders::orchestrator::{OrchestratorPresence, is_orchestrator};
use crate::quota::{QuotaFile, QuotaWindow};
use crate::registry::types::{RegistryReport, ReviewMode, Settlement, SubjectRef};
use crate::relay::approvals::{ApprovalState, ApprovalView};
use crate::relay::auto::{Escalation, EscalationCause};
use crate::relay::catalogue::ActionClass;
use crate::scene::{SceneFacts, SceneState};
use crate::untrusted::Untrusted;

/// Version of the board JSON contract. 2: approval requests (AD-35). 3:
/// the registry's state (v0.3, AD-54); later v0.3 stories extend the board
/// under this number, since no release ships in between (P-13). 4: the
/// scene's inputs (v0.4, AD-68), extended under this number by the later
/// v0.4 stories (P-22 as amended by D-461). 5: an approval's class and
/// state (v0.5, AD-92), extended under this number by the later v0.5
/// stories (P-26), the orchestrator's presence among them (story 5.16).
pub const BOARD_SCHEMA: u32 = 5;

/// What a pending `auto` request's card says of why it is the author's
/// (AD-87, D-143): in shadow mode, a cause found before any call as it is
/// and anything a call came to as `auto_approval_off`, the reason every
/// `auto` request waits for the author while approval is off, so a
/// review's answer never shows before the author decides; in live and
/// rule modes, the cause as it is, since the request is his because of it.
///
/// ```
/// use officina_core::board::shown_escalation;
/// use officina_core::registry::types::ReviewMode;
/// use officina_core::relay::auto::{Escalation, EscalationCause};
/// let shadow = |cause| Some(Escalation { cause, mode: ReviewMode::Shadow });
/// assert_eq!(shown_escalation(shadow(EscalationCause::NoSessionId)), Some(EscalationCause::NoSessionId));
/// assert_eq!(shown_escalation(shadow(EscalationCause::ReviewRejected)), Some(EscalationCause::AutoApprovalOff));
/// let live = Some(Escalation { cause: EscalationCause::ReviewRejected, mode: ReviewMode::Live });
/// assert_eq!(shown_escalation(live), Some(EscalationCause::ReviewRejected));
/// ```
pub fn shown_escalation(escalation: Option<Escalation>) -> Option<EscalationCause> {
    escalation.map(|Escalation { cause, mode }| {
        if cause.before_any_call() || mode != ReviewMode::Shadow {
            cause
        } else {
            EscalationCause::AutoApprovalOff
        }
    })
}

/// Sessions that ended, failed or finished are listed for this long.
pub const DONE_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;

/// A session seen in hook events but not yet in `claude agents` output is
/// presumed alive for this long: the hook fires before the next agents
/// refresh, and a fresh session must not flash as "interrupted".
pub const UNLISTED_GRACE_MS: i64 = 30_000;

/// The project of a session whose `cwd` lies in none of the floor's
/// projects. Never filed by rule (AD-48, D-53).
pub const OTHER_PROJECT: &str = "other";

/// The project of a session whose `cwd` is unknown. Never filed by rule
/// (AD-48, D-53).
pub const UNKNOWN_PROJECT: &str = "unknown";

/// The project names a new session never joins a subject under (AD-48): a
/// folder bearing one of them, with no floor, is treated as that name
/// (D-53).
pub const NEVER_FILED_PROJECTS: [&str; 2] = [OTHER_PROJECT, UNKNOWN_PROJECT];

/// The most running sub-agents a card lists (AD-176).
pub const MAX_SUBAGENTS: usize = 16;

/// Notification types that mean a decision blocks the session.
pub const BLOCKING_NOTIFICATIONS: [&str; 2] = ["permission_prompt", "elicitation_dialog"];

/// The board's groups, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum Column {
    NeedsYou,
    YourTurn,
    Working,
    ToResume,
    Done,
}

impl Column {
    /// Every column, in display order.
    pub const ALL: [Column; 5] = [
        Self::NeedsYou,
        Self::YourTurn,
        Self::Working,
        Self::ToResume,
        Self::Done,
    ];
}

/// Qualifier shown on a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum Badge {
    /// Officina cannot tell the state; never counts as done.
    Unknown,
    /// Failed, or the process vanished without a `SessionEnd`.
    Interrupted,
    /// Background session stopped by the author.
    Stopped,
    /// Interactive session the author exited.
    Ended,
}

impl Badge {
    /// Every badge.
    pub const ALL: [Badge; 4] = [Self::Unknown, Self::Interrupted, Self::Stopped, Self::Ended];
}

/// Where one session lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub column: Column,
    pub badge: Option<Badge>,
}

/// The project a card belongs to, or one the floor declares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ProjectView {
    pub name: String,
    pub color: Option<ProjectColor>,
    /// From the floor (AD-168); `normal` for a project it does not declare.
    pub priority: Priority,
}

/// One session on the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Card {
    pub key: String,
    pub column: Column,
    pub badge: Option<Badge>,
    pub kind: Option<SessionKindView>,
    pub name: Untrusted<String>,
    pub project: ProjectView,
    pub session_id: Option<String>,
    /// Background session id, the argument of `claude attach`.
    pub background_id: Option<String>,
    pub pid: Option<u32>,
    pub cwd: Option<String>,
    pub waiting_for: Option<String>,
    pub last_prompt: Option<Untrusted<String>>,
    pub last_message: Option<Untrusted<String>>,
    /// The end of the answer `last_message` starts, when the hook could not
    /// keep it whole (D-182): from the same `Stop` line, so head and tail
    /// always come from one answer.
    pub last_message_tail: Option<Untrusted<String>>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub last_event_at_ms: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub started_at_ms: Option<i64>,
    /// True when a click can open `claude attach` for this card.
    pub attach_available: bool,
    /// The subject the card's session is filed under, by its session id
    /// (AD-54): for a live card, the id `claude agents` reports (E14).
    /// `None` when unfiled, or when the registry could not be read, which
    /// `diagnostics.registry` says.
    pub subject: Option<SubjectRef>,
    /// Where the scene places a live card (AD-167); `None` for To resume
    /// and Done, which the scene does not draw.
    pub scene: Option<SceneState>,
    /// Why an ended card sits in Done although Claude Code did not say it
    /// finished (story 4.15): the author's verdict; `None` otherwise.
    pub settled: Option<Settled>,
    /// The sub-agents of a live card's session that Claude Code reported
    /// started and not yet stopped (AD-176, D-547), in the order of each
    /// one's first start, at most [`MAX_SUBAGENTS`]; empty for To resume
    /// and Done.
    pub subagents: Vec<SubagentView>,
}

/// A running sub-agent of a card's session (AD-176): only what its
/// `SubagentStart` line carries, each cut to 64 characters by the hook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SubagentView {
    pub agent_id: String,
    /// Session-supplied text (D-130): the frontend renders it as text only.
    pub agent_type: Option<Untrusted<String>>,
}

/// Why the author's verdict, not Claude Code, put an ended card in Done
/// (story 4.15, D-536, D-538).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum Settled {
    /// The author marked the session done from the board.
    MarkedDone,
    /// The author archived the subject the session is filed under, from
    /// the Subjects view or a triage confirmation.
    SubjectArchived,
}

/// What the registry says settles one ended session (story 4.15): the
/// author's own verdict from the board and when it was given, and when its
/// subject was archived, if it is. Each covers only what the session did
/// before it (D-538).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionSettlement {
    pub settled: Option<(Settlement, i64)>,
    pub subject_archived_at_ms: Option<i64>,
}

/// Session kind as shown on a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum SessionKindView {
    Interactive,
    Background,
}

impl From<SessionKind> for SessionKindView {
    fn from(kind: SessionKind) -> Self {
        match kind {
            SessionKind::Interactive => Self::Interactive,
            SessionKind::Background => Self::Background,
        }
    }
}

/// The five groups of cards.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Columns {
    pub needs_you: Vec<Card>,
    pub your_turn: Vec<Card>,
    pub working: Vec<Card>,
    pub to_resume: Vec<Card>,
    pub done: Vec<Card>,
}

impl Columns {
    fn get_mut(&mut self, column: Column) -> &mut Vec<Card> {
        match column {
            Column::NeedsYou => &mut self.needs_you,
            Column::YourTurn => &mut self.your_turn,
            Column::Working => &mut self.working,
            Column::ToResume => &mut self.to_resume,
            Column::Done => &mut self.done,
        }
    }

    /// Every card, columns in display order.
    pub fn all(&self) -> impl Iterator<Item = &Card> {
        self.needs_you
            .iter()
            .chain(&self.your_turn)
            .chain(&self.working)
            .chain(&self.to_resume)
            .chain(&self.done)
    }
}

/// Why the session list is not current.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Staleness {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub since_ms: i64,
    pub error: String,
}

/// Quota as the header shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct QuotaView {
    pub five_hour: Option<QuotaWindow>,
    pub seven_day: Option<QuotaWindow>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub written_at_ms: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub stale_after_ms: i64,
}

/// Contract canary result for the running Claude Code version (FR-12).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ContractView {
    pub claude_version: Option<String>,
    /// `None` until a check ran.
    pub held: Option<bool>,
    pub missing: Vec<String>,
    /// Hook payload fields of `contract::HOOK_FIELDS` this version was seen
    /// leaving out, as `Event.field` (AD-56, D-71).
    pub hook_missing: Vec<String>,
}

/// Things the author should know about the data itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Diagnostics {
    /// Unparsable event buffer lines (AD-10): never silently ignored.
    pub corrupt_event_lines: u32,
    /// Enum values from `claude agents` that Officina does not know.
    pub unrecognized_values: Vec<String>,
    /// Why the floor file was ignored, if it was.
    pub floor_error: Option<String>,
    /// The registry's state (AD-54, D-47): why it is not read or written
    /// when it is not. `None` when the board was built without it.
    pub registry: Option<RegistryReport>,
}

/// The board contract shared by `officina board --json` and the window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Board {
    pub schema: u32,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub generated_at_ms: i64,
    pub floor: Option<String>,
    pub stale: Option<Staleness>,
    /// Needs you's session cards plus the valid approval requests (AD-35):
    /// what the title, the taskbar overlay and the waiting pill count.
    pub waiting_count: u32,
    pub columns: Columns,
    /// The pending approval requests Needs you lists first, oldest first
    /// (AD-35).
    pub approvals: Vec<ApprovalView>,
    pub quota: QuotaView,
    pub contract: ContractView,
    pub diagnostics: Diagnostics,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub done_window_ms: i64,
    /// The floor's projects in file order (AD-68, D-99), each as a card of
    /// a session inside it would carry it; empty without a floor, or when
    /// the floor was ignored.
    pub floor_projects: Vec<ProjectView>,
    /// The escalation threshold of the alert, from the floor (AD-168), so
    /// no frontend constant repeats it.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub alert_after_ms: i64,
    /// Live cards in alert, a last known alert on unknown data included
    /// (AD-167, D-494): what the band, the title and the badge count.
    pub alert_count: u32,
    /// When a Needs you wait next reaches the threshold, for the collector
    /// to recompute then (AD-168).
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub next_escalation_at_ms: Option<i64>,
    /// The orchestrator while an agents entry shows it (AD-89, D-166):
    /// never a card, never counted in `waiting_count`.
    #[serde(default)]
    pub orchestrator: Option<OrchestratorPresence>,
}

/// A declared project, path already resolved and normalised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRule {
    pub name: String,
    pub path: String,
    pub color: Option<ProjectColor>,
    pub priority: Priority,
}

impl ProjectRule {
    /// The view of this declared project: the one constructor both a card
    /// inside it and `Board::floor_projects` use (D-99), so an office's
    /// colour and priority always equal its cards'.
    pub fn view(&self) -> ProjectView {
        ProjectView {
            name: self.name.clone(),
            color: Some(self.color.unwrap_or_else(|| derived_color(&self.name))),
            priority: self.priority,
        }
    }
}

/// Everything `build_board` needs. Gathered by the host.
#[derive(Debug, Clone, Default)]
pub struct BoardInput {
    /// `None` when `claude agents` never answered yet.
    pub agents: Option<Vec<AgentRecord>>,
    pub stale: Option<Staleness>,
    pub events: Vec<EventLine>,
    pub corrupt_event_lines: u32,
    pub quota: Option<QuotaFile>,
    pub floor_name: Option<String>,
    pub projects: Vec<ProjectRule>,
    pub contract: ContractView,
    /// Drop prompt and assistant excerpts from cards.
    pub no_excerpts: bool,
    /// The pending approval requests, from `relay::approvals::pending`.
    pub approvals: Vec<ApprovalView>,
    /// Why each `auto` request the window looked at is the author's, by
    /// approval id (AD-87): the board shows only what [`shown_escalation`]
    /// lets through.
    pub auto_escalations: BTreeMap<String, Escalation>,
    /// The registry's state as the caller found it.
    pub registry: Option<RegistryReport>,
    /// The subject of each filed session, by session id, from the registry
    /// (AD-54); empty when it could not be read.
    pub subjects: BTreeMap<String, SubjectRef>,
    /// What settles each ended session the registry knows of, by session
    /// id (story 4.15); empty when it could not be read.
    pub settlements: BTreeMap<String, SessionSettlement>,
    /// The floor's `[alert] after_minutes`, already validated; `None`
    /// without a floor, which gives [`ALERT_AFTER_MINUTES_DEFAULT`].
    pub alert_after_minutes: Option<u32>,
    /// The scene state of each card key on the board the caller built
    /// last (D-494): an unknown card keeps its alert from it. Empty for a
    /// first board.
    pub previous_scene: BTreeMap<String, SceneState>,
    /// The Officina home, whose `orchestrator/workdir` the orchestrator's
    /// agents entry works in (AD-89); `None` shows it as any session.
    pub officina_home: Option<String>,
}

/// Events of one session, reduced to what a card shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionEvents {
    pub last_event_at_ms: Option<i64>,
    pub last_prompt_at_ms: Option<i64>,
    pub last_stop_at_ms: Option<i64>,
    pub last_blocking_notification_at_ms: Option<i64>,
    pub last_prompt: Option<String>,
    pub last_message: Option<String>,
    /// The tail of the answer `last_message` holds the head of.
    pub last_message_tail: Option<String>,
    pub cwd: Option<String>,
    /// Reason of the last `SessionEnd`, if the session ended.
    pub end_reason: Option<String>,
    pub ended_at_ms: Option<i64>,
    pub saw_prompt: bool,
    /// The latest `SessionStart`'s time and source (`None` when its line
    /// carried none), for the scene's reception (D-495).
    pub last_session_start_at_ms: Option<i64>,
    pub last_session_start_source: Option<String>,
    /// Id of the `claude` process the hook ran under (E14), if recorded.
    pub claude_pid: Option<u32>,
    /// Time of the session's first event.
    pub first_event_at_ms: Option<i64>,
    /// Its running sub-agents, as [`Card::subagents`] lists them.
    pub subagents: Vec<SubagentView>,
}

/// One session's sub-agents as its `SubagentStart` and `SubagentStop`
/// lines leave them (AD-176, D-547): each `agent_id` runs from a start to
/// the next stop, a start while it runs is the same sub-agent, and a later
/// start brings it back with its first start's rank.
#[derive(Debug, Default)]
struct SubagentFold {
    /// Every id ever started, by first start.
    order: Vec<String>,
    /// The running ids and their latest known type.
    running: BTreeMap<String, Option<String>>,
}

impl SubagentFold {
    fn apply(&mut self, event: &EventLine) {
        let Some(id) = &event.agent_id else {
            return;
        };
        if event.event == crate::events::SUBAGENT_EVENTS[0] {
            if !self.order.contains(id) {
                self.order.push(id.clone());
            }
            let known = self.running.entry(id.clone()).or_default();
            if event.agent_type.is_some() {
                known.clone_from(&event.agent_type);
            }
        } else {
            // A stop without a start (a prompt suggestion, `/btw`) finds
            // nothing to end.
            self.running.remove(id);
        }
    }

    /// The session ended: so did every sub-agent it ran.
    fn end(&mut self) {
        self.running.clear();
    }

    fn running(&self) -> Vec<SubagentView> {
        self.order
            .iter()
            .filter_map(|id| {
                self.running.get(id).map(|agent_type| SubagentView {
                    agent_id: id.clone(),
                    agent_type: agent_type.clone().map(Untrusted::new),
                })
            })
            .take(MAX_SUBAGENTS)
            .collect()
    }
}

fn later(a: Option<i64>, b: i64) -> Option<i64> {
    Some(a.map_or(b, |a| a.max(b)))
}

/// Groups events by session id. Events without a session id are dropped.
pub fn group_events(events: &[EventLine]) -> BTreeMap<String, SessionEvents> {
    let mut sorted: Vec<&EventLine> = events.iter().collect();
    sorted.sort_by_key(|e| e.ts);
    let mut grouped: BTreeMap<String, SessionEvents> = BTreeMap::new();
    let mut folds: BTreeMap<String, SubagentFold> = BTreeMap::new();
    for event in sorted {
        let Some(sid) = &event.session_id else {
            continue;
        };
        // A sub-agent's start or stop is not the session's own activity
        // (AD-176): it would reorder Needs you and move a waiting lead. It
        // only folds into the session's sub-agents.
        if crate::events::is_subagent_event(&event.event) {
            folds.entry(sid.clone()).or_default().apply(event);
            continue;
        }
        let entry = grouped.entry(sid.clone()).or_default();
        entry.last_event_at_ms = later(entry.last_event_at_ms, event.ts);
        entry.first_event_at_ms = Some(
            entry
                .first_event_at_ms
                .map_or(event.ts, |f| f.min(event.ts)),
        );
        if event.claude_pid.is_some() {
            entry.claude_pid = event.claude_pid;
        }
        if event.cwd.is_some() {
            entry.cwd.clone_from(&event.cwd);
        }
        match event.event.as_str() {
            "UserPromptSubmit" => {
                entry.saw_prompt = true;
                entry.last_prompt_at_ms = Some(event.ts);
                if event.prompt.is_some() {
                    entry.last_prompt.clone_from(&event.prompt);
                }
                entry.end_reason = None;
                entry.ended_at_ms = None;
            }
            "Stop" => {
                entry.last_stop_at_ms = Some(event.ts);
                // The tail goes with its head, absent included: a short
                // answer never shows an older answer's end (D-182).
                if event.last_message.is_some() {
                    entry.last_message.clone_from(&event.last_message);
                    entry.last_message_tail.clone_from(&event.last_message_tail);
                }
            }
            "Notification" => {
                if event
                    .notification_type
                    .as_deref()
                    .is_some_and(|t| BLOCKING_NOTIFICATIONS.contains(&t))
                {
                    entry.last_blocking_notification_at_ms = Some(event.ts);
                }
            }
            "SessionEnd" => {
                entry.end_reason = Some(event.reason.clone().unwrap_or_default());
                entry.ended_at_ms = Some(event.ts);
                if let Some(fold) = folds.get_mut(sid) {
                    fold.end();
                }
            }
            "SessionStart" => {
                entry.last_session_start_at_ms = Some(event.ts);
                entry.last_session_start_source.clone_from(&event.source);
            }
            _ => {}
        }
    }
    // Sub-agent lines alone make no session: only a session with its own
    // events gets its sub-agents.
    for (sid, fold) in folds {
        if let Some(entry) = grouped.get_mut(&sid) {
            entry.subagents = fold.running();
        }
    }
    grouped
}

fn notification_blocks(events: &SessionEvents) -> bool {
    let Some(notified) = events.last_blocking_notification_at_ms else {
        return false;
    };
    events.last_prompt_at_ms.is_none_or(|p| notified > p)
        && events.last_stop_at_ms.is_none_or(|s| notified > s)
}

fn from_state(state: LifecycleState) -> Placement {
    let (column, badge) = match state {
        LifecycleState::Blocked => (Column::NeedsYou, None),
        LifecycleState::Working => (Column::Working, None),
        LifecycleState::Done => (Column::Done, None),
        LifecycleState::Failed => (Column::ToResume, Some(Badge::Interrupted)),
        LifecycleState::Stopped => (Column::ToResume, Some(Badge::Stopped)),
    };
    Placement { column, badge }
}

const UNKNOWN: Placement = Placement {
    column: Column::Working,
    badge: Some(Badge::Unknown),
};

/// Places one listed session (FR-6). A live `status` is authoritative; hook
/// events only count when `status` is absent.
pub fn classify(record: &AgentRecord, events: &SessionEvents) -> Placement {
    match record.status {
        Some(LiveStatus::Waiting) => Placement {
            column: Column::NeedsYou,
            badge: None,
        },
        Some(LiveStatus::Busy) => Placement {
            column: Column::Working,
            badge: None,
        },
        Some(LiveStatus::Idle) => match (record.kind, record.state) {
            (Some(SessionKind::Interactive), _) => Placement {
                column: Column::YourTurn,
                badge: None,
            },
            (_, Some(state)) => from_state(state),
            _ => UNKNOWN,
        },
        None => {
            if notification_blocks(events) {
                Placement {
                    column: Column::NeedsYou,
                    badge: None,
                }
            } else {
                record.state.map_or(UNKNOWN, from_state)
            }
        }
    }
}

/// Places a session seen only in hook events (AD-11). `None` means "do not
/// show": superseded by `/clear` or `--resume`, a non-interactive run, or too
/// recent to be judged.
pub fn classify_unlisted(events: &SessionEvents, now_ms: i64) -> Option<Placement> {
    if !events.saw_prompt {
        return None;
    }
    let to_resume = |badge| {
        Some(Placement {
            column: Column::ToResume,
            badge: Some(badge),
        })
    };
    match events.end_reason.as_deref() {
        Some("prompt_input_exit" | "logout") => to_resume(Badge::Ended),
        Some(_) => None,
        None => {
            let last = events.last_event_at_ms?;
            if now_ms - last < UNLISTED_GRACE_MS {
                None
            } else {
                to_resume(Badge::Interrupted)
            }
        }
    }
}

/// Card key rules of AD-11.
pub fn card_key(record: &AgentRecord) -> Option<String> {
    match (record.kind, record.pid, &record.id, &record.session_id) {
        (Some(SessionKind::Interactive), Some(pid), _, _) => Some(format!(
            "pid:{pid}:{}",
            record.started_at_ms.unwrap_or_default()
        )),
        (_, _, Some(id), _) => Some(format!("bg:{id}")),
        (_, _, _, Some(sid)) => Some(format!("sid:{sid}")),
        _ => None,
    }
}

/// Last path segment of a `cwd`, either separator.
pub fn folder_name(cwd: &str) -> &str {
    cwd.trim_end_matches(['\\', '/'])
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(cwd)
}

/// Stable colour for a project without a declared one.
pub fn derived_color(name: &str) -> ProjectColor {
    let index = crate::quota::fnv1a(&name.to_lowercase()) % ProjectColor::ALL.len() as u64;
    ProjectColor::ALL[index as usize]
}

/// The project of a `cwd`: the longest declared project path containing it;
/// "other" when projects are declared and none matches; the folder name when
/// nothing is declared.
pub fn project_for(cwd: Option<&str>, projects: &[ProjectRule]) -> ProjectView {
    let Some(cwd) = cwd else {
        return ProjectView {
            name: UNKNOWN_PROJECT.into(),
            color: None,
            priority: Priority::default(),
        };
    };
    if projects.is_empty() {
        let name = folder_name(cwd).to_owned();
        let color = Some(derived_color(&name));
        return ProjectView {
            name,
            color,
            priority: Priority::default(),
        };
    }
    let normalised = crate::quota::normalise_path(cwd);
    projects
        .iter()
        .filter(|p| {
            normalised == p.path
                || normalised
                    .strip_prefix(&p.path)
                    .is_some_and(|rest| rest.starts_with('/'))
        })
        .max_by_key(|p| p.path.len())
        .map_or(
            ProjectView {
                name: OTHER_PROJECT.into(),
                color: None,
                priority: Priority::default(),
            },
            ProjectRule::view,
        )
}

/// Finished sessions (Done, To resume) stay on the board for
/// [`DONE_WINDOW_MS`] after their last activity; the others always do.
fn on_board(card: &Card, now_ms: i64) -> bool {
    !matches!(card.column, Column::Done | Column::ToResume)
        || card
            .last_event_at_ms
            .or(card.started_at_ms)
            .is_some_and(|t| now_ms - t <= DONE_WINDOW_MS)
}

/// What one source knows about a session. Each source states every field;
/// what all sources share (placement, project, name fallback, excerpts) is
/// derived once, in [`new_card`].
struct CardFacts {
    key: String,
    kind: Option<SessionKindView>,
    name: Option<String>,
    session_id: Option<String>,
    background_id: Option<String>,
    pid: Option<u32>,
    cwd: Option<String>,
    waiting_for: Option<String>,
    last_event_at_ms: Option<i64>,
    started_at_ms: Option<i64>,
    attach_available: bool,
}

/// Assembles a board [`Card`]; every source goes through here.
fn new_card(
    facts: CardFacts,
    placement: Placement,
    events: &SessionEvents,
    input: &BoardInput,
) -> Card {
    let project = project_for(facts.cwd.as_deref(), &input.projects);
    let excerpt = |text: &Option<String>| {
        if input.no_excerpts {
            None
        } else {
            text.clone().map(Untrusted::new)
        }
    };
    let subject = facts
        .session_id
        .as_ref()
        .and_then(|sid| input.subjects.get(sid))
        .cloned();
    Card {
        key: facts.key,
        column: placement.column,
        badge: placement.badge,
        kind: facts.kind,
        name: Untrusted::new(facts.name.unwrap_or_else(|| project.name.clone())),
        project,
        session_id: facts.session_id,
        background_id: facts.background_id,
        pid: facts.pid,
        cwd: facts.cwd,
        waiting_for: facts.waiting_for,
        last_prompt: excerpt(&events.last_prompt),
        last_message: excerpt(&events.last_message),
        last_message_tail: excerpt(&events.last_message_tail),
        last_event_at_ms: facts.last_event_at_ms,
        started_at_ms: facts.started_at_ms,
        attach_available: facts.attach_available,
        subject,
        scene: None,
        settled: None,
        // An ended lead has no running helper, whatever the buffer holds
        // (D-547).
        subagents: if matches!(
            placement.column,
            Column::NeedsYou | Column::YourTurn | Column::Working
        ) {
            events.subagents.clone()
        } else {
            Vec::new()
        },
    }
}

/// Applies the author's verdicts to an ended card (story 4.15, D-536,
/// D-538): discarded, it leaves the board; marked done, or its subject
/// archived, a To resume card moves to Done. A verdict covers only what the
/// session did before it, so a session that lived again after it is placed
/// by its sources alone; a live card always is, whatever the registry says.
fn settle(mut card: Card, input: &BoardInput) -> Option<Card> {
    if !matches!(card.column, Column::ToResume | Column::Done) {
        return Some(card);
    }
    let Some(found) = card
        .session_id
        .as_ref()
        .and_then(|sid| input.settlements.get(sid))
    else {
        return Some(card);
    };
    let covers = |at_ms: i64| card.last_event_at_ms.is_none_or(|last| at_ms >= last);
    let by_author = found.settled.filter(|&(_, at_ms)| covers(at_ms));
    let settled = match by_author {
        Some((Settlement::Discarded, _)) => return None,
        Some((Settlement::Done, _)) => Some(Settled::MarkedDone),
        None => found
            .subject_archived_at_ms
            .filter(|&at_ms| covers(at_ms))
            .map(|_| Settled::SubjectArchived),
    };
    if let Some(settled) = settled {
        if card.column == Column::ToResume || settled == Settled::MarkedDone {
            card.settled = Some(settled);
        }
        card.column = Column::Done;
    }
    Some(card)
}

/// The card of a session `claude agents` lists. `None` when the record has
/// no card key.
fn listed_card(record: &AgentRecord, events: &SessionEvents, input: &BoardInput) -> Option<Card> {
    let facts = CardFacts {
        key: card_key(record)?,
        kind: record.kind.map(Into::into),
        name: record.name.clone(),
        session_id: record.session_id.clone(),
        background_id: record.id.clone(),
        pid: record.pid,
        cwd: record.cwd.clone().or_else(|| events.cwd.clone()),
        waiting_for: record.waiting_for.clone(),
        last_event_at_ms: events.last_event_at_ms,
        started_at_ms: record.started_at_ms,
        attach_available: record.kind == Some(SessionKind::Background) && record.id.is_some(),
    };
    Some(new_card(facts, classify(record, events), events, input))
}

/// The card of an interactive session seen only in hook events (AD-11).
/// `None` when [`classify_unlisted`] does not show it.
fn unlisted_card(
    session_id: &str,
    events: &SessionEvents,
    input: &BoardInput,
    now_ms: i64,
) -> Option<Card> {
    let placement = classify_unlisted(events, now_ms)?;
    let facts = CardFacts {
        key: format!("sid:{session_id}"),
        kind: Some(SessionKindView::Interactive),
        name: None,
        session_id: Some(session_id.to_owned()),
        background_id: None,
        pid: None,
        cwd: events.cwd.clone(),
        waiting_for: None,
        last_event_at_ms: events.ended_at_ms.or(events.last_event_at_ms),
        started_at_ms: None,
        attach_available: false,
    };
    Some(new_card(facts, placement, events, input))
}

fn sort_column(column: Column, cards: &mut [Card]) {
    let activity = |c: &Card| c.last_event_at_ms.or(c.started_at_ms).unwrap_or(0);
    match column {
        Column::NeedsYou | Column::YourTurn => {
            cards.sort_by(|a, b| activity(a).cmp(&activity(b)).then(a.key.cmp(&b.key)));
        }
        Column::Working => cards.sort_by(|a, b| {
            b.started_at_ms
                .cmp(&a.started_at_ms)
                .then(a.key.cmp(&b.key))
        }),
        Column::ToResume | Column::Done => {
            cards.sort_by(|a, b| activity(b).cmp(&activity(a)).then(a.key.cmp(&b.key)));
        }
    }
}

/// Computes the board.
///
/// ```
/// use officina_core::board::{build_board, BoardInput};
/// let input = BoardInput {
///     agents: Some(officina_core::agents::parse_agents(
///         r#"[{"pid":7,"sessionId":"s","kind":"interactive","status":"waiting",
///              "waitingFor":"permission prompt","cwd":"C:/work/api","startedAt":1}]"#,
///     ).unwrap()),
///     ..Default::default()
/// };
/// let board = build_board(&input, 1_000);
/// assert_eq!(board.waiting_count, 1);
/// assert_eq!(board.columns.needs_you[0].project.name, "api");
/// ```
pub fn build_board(input: &BoardInput, now_ms: i64) -> Board {
    let grouped = group_events(&input.events);
    let empty = SessionEvents::default();
    let mut columns = Columns::default();
    let mut card_events: crate::scene::CardEvents = BTreeMap::new();
    let mut listed: HashSet<&str> = HashSet::new();
    let mut unrecognized: Vec<String> = Vec::new();
    let mut orchestrator: Option<OrchestratorPresence> = None;

    for record in input.agents.iter().flatten() {
        unrecognized.extend(record.unrecognized.iter().cloned());
        let own = record.session_id.as_deref();
        // The orchestrator is a presence, never a card (AD-89, D-166); its
        // session is listed, so no line of it makes an unlisted card.
        if input
            .officina_home
            .as_deref()
            .is_some_and(|home| is_orchestrator(record, home))
        {
            orchestrator = orchestrator.or(Some(OrchestratorPresence::of(record)));
            listed.extend(own);
            continue;
        }
        // Sessions run by this live process (E14): after `/clear` the hook
        // reports a new session id while the process, and its card, stay.
        // Only sessions that began after the process started count, so a
        // reused pid cannot capture an older, ended session.
        let same_process: Vec<&str> = match (record.kind, record.pid) {
            (Some(SessionKind::Interactive), Some(pid)) => grouped
                .iter()
                .filter(|(sid, ev)| {
                    Some(sid.as_str()) != own
                        && ev.claude_pid == Some(pid)
                        && ev
                            .first_event_at_ms
                            .zip(record.started_at_ms)
                            .is_some_and(|(f, s)| f >= s)
                })
                .map(|(sid, _)| sid.as_str())
                .collect(),
            _ => Vec::new(),
        };
        let events = own
            .into_iter()
            .chain(same_process.iter().copied())
            .filter_map(|sid| grouped.get(sid))
            .max_by_key(|ev| ev.last_event_at_ms)
            .unwrap_or(&empty);
        let Some(card) = listed_card(record, events, input) else {
            continue;
        };
        listed.extend(own);
        listed.extend(same_process.iter().copied());
        let Some(card) = settle(card, input) else {
            continue;
        };
        card_events.insert(card.key.clone(), events);
        if on_board(&card, now_ms) {
            columns.get_mut(card.column).push(card);
        }
    }

    // Unlisted sessions are only judged once the session list is known:
    // before the first `claude agents` answer, every live session is unlisted.
    if input.agents.is_some() {
        for (sid, events) in &grouped {
            if listed.contains(sid.as_str()) {
                continue;
            }
            if let Some(card) = unlisted_card(sid, events, input, now_ms)
                .and_then(|card| settle(card, input))
                .filter(|c| on_board(c, now_ms))
            {
                card_events.insert(card.key.clone(), events);
                columns.get_mut(card.column).push(card);
            }
        }
    }

    for column in Column::ALL {
        sort_column(column, columns.get_mut(column));
    }
    unrecognized.sort();
    unrecognized.dedup();

    let approvals: Vec<ApprovalView> = input
        .approvals
        .iter()
        .map(|approval| ApprovalView {
            session_name: approval.session_id.as_deref().and_then(|sid| {
                columns
                    .all()
                    .find(|card| card.session_id.as_deref() == Some(sid))
                    .map(|card| card.name.clone())
            }),
            escalation: (approval.class == ActionClass::Auto
                && approval.state == ApprovalState::Pending)
                .then(|| shown_escalation(input.auto_escalations.get(&approval.id).copied()))
                .flatten(),
            ..approval.clone()
        })
        .collect();
    let waiting =
        columns.needs_you.len() + approvals.iter().filter(|a| a.waits_on_the_author()).count();
    let alert_after_ms = i64::from(
        input
            .alert_after_minutes
            .unwrap_or(ALERT_AFTER_MINUTES_DEFAULT),
    ) * 60_000;
    crate::scene::place(
        &mut columns,
        &card_events,
        &SceneFacts {
            stale: input.stale.is_some(),
            now_ms,
            alert_after_ms,
            approvals: &approvals,
            previous: &input.previous_scene,
        },
    );

    Board {
        schema: BOARD_SCHEMA,
        generated_at_ms: now_ms,
        floor: input.floor_name.clone(),
        stale: input.stale.clone(),
        waiting_count: u32::try_from(waiting).unwrap_or(u32::MAX),
        approvals,
        quota: QuotaView {
            five_hour: input.quota.as_ref().and_then(|q| q.five_hour),
            seven_day: input.quota.as_ref().and_then(|q| q.seven_day),
            written_at_ms: input.quota.as_ref().map(|q| q.written_at * 1000),
            stale_after_ms: crate::quota::STALE_AFTER_MS,
        },
        contract: input.contract.clone(),
        diagnostics: Diagnostics {
            corrupt_event_lines: input.corrupt_event_lines,
            unrecognized_values: unrecognized,
            floor_error: None,
            registry: input.registry.clone(),
        },
        done_window_ms: DONE_WINDOW_MS,
        floor_projects: input.projects.iter().map(ProjectRule::view).collect(),
        alert_after_ms,
        alert_count: crate::scene::alert_count(&columns),
        next_escalation_at_ms: crate::scene::next_escalation_at_ms(&columns, alert_after_ms),
        columns,
        orchestrator,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::parse_agents;
    use crate::untrusted::text;

    fn record(json: &str) -> AgentRecord {
        parse_agents(&format!("[{json}]")).unwrap().remove(0)
    }

    fn event(ts: i64, sid: &str, name: &str) -> EventLine {
        EventLine {
            v: 1,
            ts,
            event: name.into(),
            session_id: Some(sid.into()),
            cwd: Some("C:/work/api".into()),
            source: None,
            reason: None,
            notification_type: None,
            message: None,
            prompt: None,
            last_message: None,
            last_message_tail: None,
            claude_pid: None,
            agent_id: None,
            agent_type: None,
            permission_mode: None,
            absent: Vec::new(),
            truncated: false,
        }
    }

    const HOME: &str = r"C:\Users\user01\.officina";

    /// The orchestrator's agents entry with `status`, in its workdir.
    fn orchestrator_entry(status: &str) -> AgentRecord {
        record(&format!(
            r#"{{"pid":41,"sessionId":"orch","name":"officina-orchestrator","kind":"interactive","status":"{status}","cwd":"c:/users/USER01/.officina/orchestrator/workdir/","startedAt":3}}"#
        ))
    }

    /// Story 5.16 AC 5 (AD-89, D-166): the agents entry named
    /// `officina-orchestrator` in the workdir is the board's presence,
    /// never a card, and its hook lines make no card either.
    #[test]
    fn the_orchestrator_is_presence_not_a_card() {
        use crate::orders::orchestrator::OrchestratorPresence;
        let input = BoardInput {
            agents: Some(vec![orchestrator_entry("idle")]),
            events: vec![event(2, "orch", "UserPromptSubmit")],
            officina_home: Some(HOME.to_owned()),
            ..Default::default()
        };
        let board = build_board(&input, 10);
        assert_eq!(
            board.orchestrator,
            Some(OrchestratorPresence {
                pid: Some(41),
                status: Some(crate::agents::LiveStatus::Idle),
                started_at_ms: Some(3),
            })
        );
        assert_eq!(board.columns.all().count(), 0, "no card");
        // Without the home, the same entry is an ordinary session.
        let plain = build_board(
            &BoardInput {
                officina_home: None,
                ..input
            },
            10,
        );
        assert_eq!(plain.orchestrator, None);
        assert_eq!(plain.columns.all().count(), 1);
    }

    /// Story 5.16 AC 5 (AD-89): a waiting orchestrator is shown waiting
    /// and is no wait on the author's count.
    #[test]
    fn a_waiting_orchestrator_is_not_counted() {
        let input = BoardInput {
            agents: Some(vec![orchestrator_entry("waiting")]),
            officina_home: Some(HOME.to_owned()),
            ..Default::default()
        };
        let board = build_board(&input, 10);
        assert_eq!(
            board.orchestrator.and_then(|p| p.status),
            Some(crate::agents::LiveStatus::Waiting)
        );
        assert_eq!(board.waiting_count, 0);
        assert!(board.columns.needs_you.is_empty());
    }

    /// Story 5.16 AC 5 (D-166): another session under the orchestrator's
    /// folder, or named like it elsewhere, is an ordinary card.
    #[test]
    fn another_session_under_the_orchestrator_folder_is_a_card() {
        let input = BoardInput {
            agents: Some(vec![
                record(
                    r#"{"pid":5,"sessionId":"notes","name":"notes","kind":"interactive","status":"waiting","cwd":"C:/Users/user01/.officina/orchestrator/workdir","startedAt":1}"#,
                ),
                record(
                    r#"{"pid":6,"sessionId":"elsewhere","name":"officina-orchestrator","kind":"interactive","status":"idle","cwd":"C:/Users/user01/.officina/orchestrator/proposals","startedAt":1}"#,
                ),
            ]),
            officina_home: Some(HOME.to_owned()),
            ..Default::default()
        };
        let board = build_board(&input, 10);
        assert_eq!(board.orchestrator, None);
        let ids: Vec<&str> = board
            .columns
            .all()
            .filter_map(|c| c.session_id.as_deref())
            .collect();
        assert_eq!(ids.len(), 2, "{ids:?}");
        assert_eq!(board.waiting_count, 1, "the waiting one is the author's");
    }

    /// AD-54, D-47: the registry's state reaches the board as the caller
    /// found it, sessions or not; the board never reads the registry itself.
    #[test]
    fn registry_state_reaches_the_diagnostics() {
        use crate::registry::types::{RegistryReport, RegistryState};
        for state in RegistryState::ALL {
            let report = RegistryReport {
                state: *state,
                detail: Some(format!("why {}", state.code())),
                exclude_from_sync: Some("C:/path/to/registry".into()),
            };
            let input = BoardInput {
                agents: Some(vec![record(
                    r#"{"pid":7,"sessionId":"s","kind":"interactive","status":"idle","cwd":"C:/work/api","startedAt":1}"#,
                )]),
                registry: Some(report.clone()),
                ..Default::default()
            };
            let board = build_board(&input, 10);
            assert_eq!(board.diagnostics.registry, Some(report));
            assert_eq!(
                board.columns.your_turn.len(),
                1,
                "the board keeps its sessions"
            );
        }
        assert_eq!(
            build_board(&BoardInput::default(), 10).diagnostics.registry,
            None
        );
    }

    /// AD-54: a card carries the subject its session id is filed under,
    /// the live card by the id `claude agents` reports, the unlisted card
    /// by its own; a session the map does not hold has none.
    #[test]
    fn cards_carry_their_subject_by_session_id() {
        let mut gone_prompt = event(10, "gone", "UserPromptSubmit");
        gone_prompt.prompt = Some("deploy".into());
        let mut gone_end = event(20, "gone", "SessionEnd");
        gone_end.reason = Some("prompt_input_exit".into());
        let rollout = SubjectRef {
            id: 3,
            name: "alert-rules rollout".into(),
        };
        let input = BoardInput {
            agents: Some(
                parse_agents(
                    r#"[{"pid":1,"sessionId":"live","kind":"interactive","status":"idle","startedAt":1},
                        {"pid":2,"sessionId":"loose","kind":"interactive","status":"busy","startedAt":1}]"#,
                )
                .unwrap(),
            ),
            events: vec![gone_prompt, gone_end],
            subjects: BTreeMap::from([
                ("live".to_owned(), rollout.clone()),
                (
                    "gone".to_owned(),
                    SubjectRef {
                        id: 4,
                        name: "certificate renewal".into(),
                    },
                ),
            ]),
            ..Default::default()
        };
        let board = build_board(&input, UNLISTED_GRACE_MS + 100);
        let subject_of = |sid: &str| {
            board
                .columns
                .all()
                .find(|c| c.session_id.as_deref() == Some(sid))
                .unwrap()
                .subject
                .clone()
        };
        assert_eq!(subject_of("live"), Some(rollout));
        assert_eq!(subject_of("gone").map(|s| s.id), Some(4));
        assert_eq!(subject_of("loose"), None);
        let without = build_board(
            &BoardInput {
                subjects: BTreeMap::new(),
                ..input
            },
            UNLISTED_GRACE_MS + 100,
        );
        assert!(without.columns.all().all(|c| c.subject.is_none()));
    }

    /// AD-54, D-71: the hook fields the running version leaves out reach
    /// the board's contract view as the collector found them, beside the
    /// agents fields; none when nothing is missing.
    #[test]
    fn hook_missing_reaches_the_contract_view() {
        let (event, fields) = crate::contract::HOOK_FIELDS[0];
        let missing = vec![format!("{event}.{}", fields[0])];
        let input = BoardInput {
            contract: ContractView {
                claude_version: Some("9.9.9".into()),
                held: Some(true),
                missing: Vec::new(),
                hook_missing: missing.clone(),
            },
            ..Default::default()
        };
        let board = build_board(&input, 10);
        assert_eq!(board.contract.hook_missing, missing);
        assert_eq!(board.contract.held, Some(true), "the agents side holds");
        let json = serde_json::to_value(&board).unwrap();
        assert_eq!(json["contract"]["hook_missing"][0], missing[0]);
        assert!(
            build_board(&BoardInput::default(), 10)
                .contract
                .hook_missing
                .is_empty()
        );
    }

    /// AD-55, D-61: rows to sort block no session, so they leave
    /// `waiting_count`, and with it the window title and the taskbar
    /// overlay, as they were: a board built with the registry input of a
    /// registry holding rows to sort counts what one without it counts.
    #[test]
    fn rows_to_sort_leave_waiting_count_unchanged() {
        use crate::registry::proposal::ModelInput;
        use crate::registry::types::{
            HookEnd, RegistryReport, RegistryState, SessionRecord, Source, Sourced, SubjectRecord,
            SubjectStatus,
        };
        use crate::registry::verdict::{VerdictInput, rows};

        fn sourced<T>(value: T, at_ms: i64) -> Option<Sourced<T>> {
            Some(Sourced {
                value,
                src: Source::Hook,
                at_ms,
            })
        }
        let mut events = Vec::new();
        let mut sessions = Vec::new();
        for (i, sid) in ["done-a", "done-b", "loose"].into_iter().enumerate() {
            let at = 100 * (i as i64 + 1);
            let mut prompt = event(at, sid, "UserPromptSubmit");
            prompt.prompt = Some("tidy up".into());
            let mut end = event(at + 50, sid, "SessionEnd");
            end.reason = Some("prompt_input_exit".into());
            events.extend([prompt, event(at + 10, sid, "Stop"), end]);
            sessions.push(SessionRecord {
                session_id: sid.to_owned(),
                project: "api".to_owned(),
                subject: (sid != "loose").then_some(Sourced {
                    value: Some(1),
                    src: Source::Author,
                    at_ms: 1,
                }),
                last_prompt_at: sourced(at, at),
                last_stop_at: sourced(at + 10, at + 10),
                ended: sourced(
                    HookEnd {
                        reason: Some("prompt_input_exit".into()),
                        claude_pid: None,
                    },
                    at + 50,
                ),
                ..SessionRecord::default()
            });
        }
        let subjects = [SubjectRecord {
            id: 1,
            project: "api".into(),
            name: "cleanup".into(),
            status: Sourced {
                value: SubjectStatus::Active,
                src: Source::Author,
                at_ms: 1,
            },
            created_at_ms: 1,
            routine: false,
            merged_into: None,
        }];
        let to_sort = rows(&VerdictInput {
            subjects: &subjects,
            sessions: &sessions,
            confirmations: &[],
            model: &ModelInput::default(),
        });
        assert_eq!(
            to_sort.len(),
            2,
            "positive control: a subject and an unfiled session"
        );

        let waiting = record(
            r#"{"pid":7,"sessionId":"w","kind":"interactive","status":"waiting","waitingFor":"permission prompt","cwd":"C:/work/api","startedAt":1}"#,
        );
        let without = BoardInput {
            agents: Some(vec![waiting]),
            events,
            ..Default::default()
        };
        let with = BoardInput {
            registry: Some(RegistryReport {
                state: RegistryState::Ready,
                detail: None,
                exclude_from_sync: None,
            }),
            subjects: BTreeMap::from([
                (
                    "done-a".to_owned(),
                    SubjectRef {
                        id: 1,
                        name: "cleanup".into(),
                    },
                ),
                (
                    "done-b".to_owned(),
                    SubjectRef {
                        id: 1,
                        name: "cleanup".into(),
                    },
                ),
            ]),
            ..without.clone()
        };
        let now = UNLISTED_GRACE_MS + 1_000;
        let (plain, sorted) = (build_board(&without, now), build_board(&with, now));
        assert_eq!(plain.waiting_count, 1);
        assert_eq!(sorted.waiting_count, plain.waiting_count);
        assert_eq!(sorted.columns.needs_you, plain.columns.needs_you);
    }

    #[test]
    fn live_status_overrides_stale_notification() {
        let mut n = event(10, "s", "Notification");
        n.notification_type = Some("permission_prompt".into());
        let events = group_events(&[n]);
        let busy = record(r#"{"sessionId":"s","kind":"interactive","pid":1,"status":"busy"}"#);
        assert_eq!(classify(&busy, &events["s"]).column, Column::Working);
        let no_status = record(r#"{"sessionId":"s","kind":"interactive"}"#);
        assert_eq!(classify(&no_status, &events["s"]).column, Column::NeedsYou);
    }

    #[test]
    fn notification_answered_by_prompt_or_stop_no_longer_blocks() {
        let mut n = event(10, "s", "Notification");
        n.notification_type = Some("elicitation_dialog".into());
        let after_stop = group_events(&[n.clone(), event(20, "s", "Stop")]);
        let no_status = record(r#"{"sessionId":"s","kind":"interactive"}"#);
        assert_eq!(classify(&no_status, &after_stop["s"]), UNKNOWN);
    }

    #[test]
    fn interactive_idle_is_your_turn_background_idle_follows_state() {
        let e = SessionEvents::default();
        let int = record(r#"{"sessionId":"s","kind":"interactive","pid":1,"status":"idle"}"#);
        assert_eq!(classify(&int, &e).column, Column::YourTurn);
        let bg = record(r#"{"id":"b","kind":"background","status":"idle","state":"done"}"#);
        assert_eq!(classify(&bg, &e).column, Column::Done);
        let bg_unknown = record(r#"{"id":"b","kind":"background","status":"idle"}"#);
        assert_eq!(classify(&bg_unknown, &e), UNKNOWN);
    }

    #[test]
    fn unlisted_sessions_follow_session_end_reason() {
        let now = 1_000_000;
        let base = [event(1, "s", "UserPromptSubmit")];
        let mut end = event(2, "s", "SessionEnd");
        for (reason, expected) in [
            ("prompt_input_exit", Some(Badge::Ended)),
            ("logout", Some(Badge::Ended)),
            ("clear", None),
            ("resume", None),
            ("other", None),
        ] {
            end.reason = Some(reason.into());
            let g = group_events(&[base[0].clone(), end.clone()]);
            assert_eq!(
                classify_unlisted(&g["s"], now).and_then(|p| p.badge),
                expected,
                "{reason}"
            );
        }
        let g = group_events(&base);
        assert_eq!(
            classify_unlisted(&g["s"], now).unwrap().badge,
            Some(Badge::Interrupted)
        );
        assert_eq!(classify_unlisted(&g["s"], 1 + UNLISTED_GRACE_MS - 1), None);
        let no_prompt = group_events(&[event(1, "s", "SessionStart")]);
        assert_eq!(classify_unlisted(&no_prompt["s"], now), None);
    }

    /// Story 4.15: the board read with what the registry settled. Three
    /// ended sessions (`a` at 20, `b` at 30, `c` at 40), one live (`l`).
    fn settled_board(settlements: &[(&str, SessionSettlement)]) -> Board {
        let ended = |sid: &str, at: i64| {
            let mut end = event(at, sid, "SessionEnd");
            end.reason = Some("prompt_input_exit".into());
            [event(at - 5, sid, "UserPromptSubmit"), end]
        };
        let input = BoardInput {
            agents: Some(parse_agents(
                r#"[{"pid":1,"sessionId":"l","kind":"interactive","status":"busy","startedAt":1}]"#,
            )
            .unwrap()),
            events: [ended("a", 20), ended("b", 30), ended("c", 40)]
                .into_iter()
                .flatten()
                .chain([event(41, "l", "UserPromptSubmit")])
                .collect(),
            settlements: settlements
                .iter()
                .map(|(sid, s)| ((*sid).to_owned(), *s))
                .collect(),
            ..Default::default()
        };
        build_board(&input, 100)
    }

    fn sids(cards: &[Card]) -> Vec<&str> {
        cards
            .iter()
            .filter_map(|c| c.session_id.as_deref())
            .collect()
    }

    fn by_author(settlement: Settlement, at_ms: i64) -> SessionSettlement {
        SessionSettlement {
            settled: Some((settlement, at_ms)),
            subject_archived_at_ms: None,
        }
    }

    fn archived(at_ms: i64) -> SessionSettlement {
        SessionSettlement {
            settled: None,
            subject_archived_at_ms: Some(at_ms),
        }
    }

    /// Item 7 of the owner's feedback: archiving a subject in the Subjects
    /// view left its ended session in To resume, since the board never
    /// read the subject's status. It is now in Done, marked as archived.
    #[test]
    fn an_archived_subject_takes_its_ended_session_out_of_to_resume() {
        let before = settled_board(&[]);
        assert_eq!(sids(&before.columns.to_resume), ["c", "b", "a"]);
        let after = settled_board(&[("b", archived(50))]);
        assert_eq!(sids(&after.columns.to_resume), ["c", "a"]);
        assert_eq!(sids(&after.columns.done), ["b"]);
        assert_eq!(
            after.columns.done[0].settled,
            Some(Settled::SubjectArchived)
        );
    }

    #[test]
    fn the_author_marks_done_or_discards_an_ended_session() {
        let board = settled_board(&[
            ("a", by_author(Settlement::Done, 60)),
            ("b", by_author(Settlement::Discarded, 60)),
        ]);
        assert_eq!(sids(&board.columns.to_resume), ["c"]);
        assert_eq!(sids(&board.columns.done), ["a"]);
        assert_eq!(board.columns.done[0].settled, Some(Settled::MarkedDone));
        assert!(
            board
                .columns
                .all()
                .all(|c| c.session_id.as_deref() != Some("b"))
        );
    }

    /// The author's own verdict wins over the subject's: discarded, the
    /// session is gone even when its subject is archived.
    #[test]
    fn the_author_verdict_wins_over_the_archived_subject() {
        let board = settled_board(&[(
            "a",
            SessionSettlement {
                settled: Some((Settlement::Discarded, 60)),
                subject_archived_at_ms: Some(60),
            },
        )]);
        assert!(
            board
                .columns
                .all()
                .all(|c| c.session_id.as_deref() != Some("a"))
        );
    }

    /// D-538: a verdict covers what the session did before it. Given before
    /// the session's last event, it no longer holds, so the session is
    /// placed by its sources; a live session always is.
    #[test]
    fn a_verdict_older_than_the_last_event_and_a_live_card_are_ignored() {
        let board = settled_board(&[
            ("c", by_author(Settlement::Discarded, 39)),
            ("b", archived(29)),
            ("l", by_author(Settlement::Discarded, 99)),
        ]);
        assert_eq!(sids(&board.columns.to_resume), ["c", "b", "a"]);
        assert_eq!(sids(&board.columns.working), ["l"]);
        assert!(board.columns.all().all(|c| c.settled.is_none()));
    }

    /// Taking a verdict back (no settlement) puts the card back where its
    /// sources place it.
    #[test]
    fn no_settlement_leaves_the_card_where_its_sources_place_it() {
        let board = settled_board(&[("a", SessionSettlement::default())]);
        assert_eq!(sids(&board.columns.to_resume), ["c", "b", "a"]);
    }

    #[test]
    fn card_keys() {
        let int = record(r#"{"sessionId":"s","kind":"interactive","pid":4,"startedAt":9}"#);
        assert_eq!(card_key(&int).unwrap(), "pid:4:9");
        let bg = record(r#"{"id":"ab","sessionId":"s","kind":"background","pid":4}"#);
        assert_eq!(card_key(&bg).unwrap(), "bg:ab");
        let gone = record(r#"{"sessionId":"s","kind":"interactive"}"#);
        assert_eq!(card_key(&gone).unwrap(), "sid:s");
        assert_eq!(card_key(&record(r#"{"cwd":"x"}"#)), None);
    }

    #[test]
    fn projects_match_longest_declared_path_or_other() {
        let rules = vec![
            ProjectRule {
                name: "work".into(),
                path: "c:/work".into(),
                color: None,
                priority: Priority::Normal,
            },
            ProjectRule {
                name: "api".into(),
                path: "c:/work/api".into(),
                color: Some(ProjectColor::Blue),
                priority: Priority::Normal,
            },
        ];
        assert_eq!(project_for(Some(r"C:\Work\Api\src"), &rules).name, "api");
        assert_eq!(project_for(Some("C:/work/apiary"), &rules).name, "work");
        assert_eq!(project_for(Some("D:/x"), &rules).name, "other");
        assert_eq!(project_for(Some(r"C:\a\Lorica\"), &[]).name, "Lorica");
        assert_eq!(
            derived_color("Lorica"),
            project_for(Some("D:/lorica"), &[]).color.unwrap()
        );
    }

    #[test]
    fn excerpts_can_be_withheld_and_old_done_cards_drop() {
        let mut p = event(5, "s", "UserPromptSubmit");
        p.prompt = Some("deploy".into());
        let input = BoardInput {
            agents: Some(parse_agents(
                r#"[{"pid":1,"sessionId":"s","kind":"interactive","status":"busy","startedAt":1},
                    {"id":"old","sessionId":"o","kind":"background","state":"done","startedAt":1}]"#,
            )
            .unwrap()),
            events: vec![p],
            no_excerpts: true,
            ..Default::default()
        };
        let board = build_board(&input, DONE_WINDOW_MS + 10);
        assert_eq!(board.columns.working[0].last_prompt, None);
        assert!(board.columns.done.is_empty());
    }

    #[test]
    fn listed_and_unlisted_cards_derive_project_name_and_excerpts_alike() {
        let rules = vec![ProjectRule {
            name: "api".into(),
            path: "c:/work/api".into(),
            color: Some(ProjectColor::Blue),
            priority: Priority::Normal,
        }];
        let mut listed_prompt = event(10, "live", "UserPromptSubmit");
        listed_prompt.prompt = Some("deploy".into());
        let mut gone_prompt = event(10, "gone", "UserPromptSubmit");
        gone_prompt.prompt = Some("deploy".into());
        let mut gone_end = event(20, "gone", "SessionEnd");
        gone_end.reason = Some("prompt_input_exit".into());
        let mut input = BoardInput {
            agents: Some(
                parse_agents(
                    r#"[{"pid":1,"sessionId":"live","kind":"interactive","status":"idle",
                         "cwd":"C:/work/api/src","startedAt":1}]"#,
                )
                .unwrap(),
            ),
            events: vec![listed_prompt, gone_prompt, gone_end],
            projects: rules,
            ..Default::default()
        };
        let now = UNLISTED_GRACE_MS + 100;
        let board = build_board(&input, now);
        let listed = &board.columns.your_turn[0];
        let unlisted = &board.columns.to_resume[0];
        assert_eq!(unlisted.key, "sid:gone");
        for card in [listed, unlisted] {
            assert_eq!(card.project.name, "api", "{}", card.key);
            assert_eq!(card.project.color, Some(ProjectColor::Blue), "{}", card.key);
            assert_eq!(card.name, text("api"), "{}", card.key);
            assert_eq!(card.last_prompt, Some(text("deploy")), "{}", card.key);
        }

        input.no_excerpts = true;
        let board = build_board(&input, now);
        assert!(board.columns.all().all(|c| c.last_prompt.is_none()));
        assert_eq!(board.columns.all().count(), 2);
    }

    #[test]
    fn a_cleared_session_brings_its_latest_excerpts_to_the_live_card() {
        let mut before = event(10, "s1", "UserPromptSubmit");
        before.prompt = Some("before clear".into());
        before.claude_pid = Some(7);
        let mut after = event(30, "s2", "UserPromptSubmit");
        after.prompt = Some("after clear".into());
        after.claude_pid = Some(7);
        let input = BoardInput {
            agents: Some(
                parse_agents(
                    r#"[{"pid":7,"sessionId":"s1","kind":"interactive","status":"idle","startedAt":5}]"#,
                )
                .unwrap(),
            ),
            events: vec![before, after],
            ..Default::default()
        };
        let board = build_board(&input, 10_000_000);
        assert_eq!(board.columns.all().count(), 1);
        assert_eq!(
            board.columns.your_turn[0].last_prompt,
            Some(text("after clear"))
        );
    }

    mod approvals {
        use super::*;
        use crate::relay::approvals::{
            ApprovalContext, ApprovalFiles, ApprovalRequest, Decision, DryRunInputs, KeyInputs,
            PENDING_MS, pending, request_id, request_key,
        };
        use crate::relay::catalogue::{ApprovedCatalogue, load};
        use crate::relay::journal::TreeFingerprint;

        const NOW: i64 = 1_793_102_400_000;
        const MIN: i64 = 60_000;
        const FLOOR: &str = "schema = 2
[[project]]
name = \"lab\"
path = \"C:/work/lab\"
actions = [\"restart\", \"verify\", \"apply\"]
[[action]]
name = \"restart\"
description = \"Restart one service\"
class = \"human\"
program = \"C:/path/to/tool.exe\"
args = [\"{service}\"]
[action.params.service]
values = [\"web01\", \"db01\", \"cache01\"]
[[action]]
name = \"verify\"
class = \"free\"
program = \"C:/path/to/tool.exe\"
args = [\"--check\", \"{service}\"]
[action.params.service]
values = [\"web01\"]
[[action]]
name = \"apply\"
class = \"human\"
check = \"verify\"
program = \"C:/path/to/tool.exe\"
args = [\"{service}\"]
[action.params.service]
values = [\"web01\"]
";
        const WAITING: &str = "[{\"pid\":1,\"sessionId\":\"s1\",\"kind\":\"interactive\",\"status\":\"waiting\",\"name\":\"deploy-lab\",\"startedAt\":1}]";

        fn approved() -> ApprovedCatalogue {
            let path = crate::relay::fixtures::FLOOR_PATH;
            ApprovedCatalogue::approving(&load(FLOOR, path).unwrap(), path, NOW - 60 * MIN)
        }

        fn fingerprint(text: &str) -> TreeFingerprint {
            TreeFingerprint {
                sha256: crate::relay::sha256_hex(text.as_bytes()),
                files: 0,
                git: None,
            }
        }

        /// A request as `officina run` writes it, from session `sid`.
        fn request(
            approved: &ApprovedCatalogue,
            action: &str,
            service: &str,
            created_at_ms: i64,
            sid: Option<&str>,
        ) -> (String, ApprovalRequest) {
            let params = BTreeMap::from([("service".to_owned(), service.to_owned())]);
            let dry_run = action == "apply";
            let check_run_id = dry_run.then(|| "run-7".to_owned());
            let tree = dry_run.then(|| fingerprint("a"));
            let key = request_key(&KeyInputs {
                project: "lab",
                action,
                params: &params,
                catalogue_hash: &approved.hash,
                check_run_id: check_run_id.as_deref(),
                tree: tree.as_ref(),
            });
            (
                request_id(&key, created_at_ms),
                ApprovalRequest {
                    request_key: key,
                    created_at_ms,
                    project: "lab".to_owned(),
                    action: action.to_owned(),
                    params,
                    argv: vec!["C:/path/to/tool.exe".to_owned(), service.to_owned()],
                    http: None,
                    catalogue_hash: approved.hash.clone(),
                    session_id: sid.map(str::to_owned),
                    check_run_id,
                    tree,
                },
            )
        }

        fn board_of(files: &ApprovalFiles, dry_runs: &DryRunInputs, agents: &str) -> Board {
            let approved = approved();
            let dirs = BTreeMap::new();
            let ctx = ApprovalContext {
                approved: Some(&approved),
                project_dirs: &dirs,
                system_dir: "C:/Windows/System32",
            };
            let input = BoardInput {
                agents: Some(parse_agents(agents).unwrap()),
                approvals: pending(files, &ctx, dry_runs, NOW),
                ..Default::default()
            };
            build_board(&input, NOW)
        }

        fn files(requests: Vec<(String, ApprovalRequest)>) -> ApprovalFiles {
            ApprovalFiles {
                requests: requests.into_iter().collect(),
                ..Default::default()
            }
        }

        #[test]
        fn approvals_count_as_waiting() {
            let approved = approved();
            let board = board_of(
                &files(vec![
                    request(&approved, "restart", "db01", NOW - 2 * MIN, None),
                    request(&approved, "restart", "web01", NOW - 5 * MIN, None),
                ]),
                &DryRunInputs::default(),
                WAITING,
            );
            assert_eq!(board.schema, BOARD_SCHEMA);
            assert_eq!(board.approvals.len(), 2);
            assert_eq!(board.waiting_count, 3, "one session card and two approvals");
            assert_eq!(
                board.approvals[0].argv[1], "web01",
                "oldest first, the command rebuilt from the catalogue"
            );
            assert_eq!(
                board.columns.needs_you.len(),
                1,
                "approvals are not session cards"
            );
        }

        /// AD-92: each approval carries its class and state; every request
        /// is pending until a live review is deciding it.
        #[test]
        fn approvals_carry_their_class_and_state() {
            use crate::relay::approvals::ApprovalState;
            use crate::relay::catalogue::ActionClass;
            let approved = approved();
            let board = board_of(
                &files(vec![request(
                    &approved,
                    "restart",
                    "web01",
                    NOW - MIN,
                    None,
                )]),
                &DryRunInputs::default(),
                "[]",
            );
            let approval = &board.approvals[0];
            assert_eq!(
                (approval.class, approval.state),
                (ActionClass::Human, ApprovalState::Pending)
            );
            let value = serde_json::to_value(&board).unwrap();
            assert_eq!(value["approvals"][0]["class"], "human");
            assert_eq!(value["approvals"][0]["state"], "pending");
        }

        /// AD-92, D-571: a request a live review is deciding waits on the
        /// review, not on the author: it is not counted.
        #[test]
        fn reviewing_requests_are_not_counted_as_waiting() {
            use crate::relay::approvals::ApprovalState;
            let approved = approved();
            let ctx_dirs = BTreeMap::from([("lab".to_owned(), "C:/path/to/Lab".to_owned())]);
            let ctx = ApprovalContext {
                approved: Some(&approved),
                project_dirs: &ctx_dirs,
                system_dir: "C:/Windows/System32",
            };
            let all = files(vec![
                request(&approved, "restart", "web01", NOW - MIN, None),
                request(&approved, "restart", "db01", NOW - MIN, None),
            ]);
            let mut approvals = pending(&all, &ctx, &DryRunInputs::default(), NOW);
            approvals[0].state = ApprovalState::Reviewing;
            let board = build_board(
                &BoardInput {
                    approvals,
                    ..Default::default()
                },
                NOW,
            );
            assert_eq!(board.approvals.len(), 2, "both listed");
            assert_eq!(board.waiting_count, 1, "only the pending one waits");
        }

        /// Story 5.13 AC 5 (AD-88, D-143): a pending `auto` request whose
        /// review answered shows `auto_approval_off`, and the board carries
        /// no review field and no review text; a cause found before any call
        /// shows as it is.
        #[test]
        fn a_pending_auto_request_with_a_stored_review_carries_no_review_field() {
            use crate::relay::auto::EscalationCause;
            use crate::relay::catalogue::ActionClass;
            let approved = approved();
            let dirs = BTreeMap::from([("lab".to_owned(), "C:/path/to/Lab".to_owned())]);
            let ctx = ApprovalContext {
                approved: Some(&approved),
                project_dirs: &dirs,
                system_dir: "C:/Windows/System32",
            };
            let all = files(vec![
                request(&approved, "restart", "web01", NOW - MIN, None),
                request(&approved, "restart", "db01", NOW - MIN, None),
            ]);
            let mut approvals = pending(&all, &ctx, &DryRunInputs::default(), NOW);
            for approval in &mut approvals {
                approval.class = ActionClass::Auto;
            }
            let reviewed = approvals[0].id.clone();
            let prechecked = approvals[1].id.clone();
            let board = build_board(
                &BoardInput {
                    approvals,
                    auto_escalations: BTreeMap::from([
                        (
                            reviewed.clone(),
                            Escalation {
                                cause: EscalationCause::ReviewRejected,
                                mode: ReviewMode::Shadow,
                            },
                        ),
                        (
                            prechecked.clone(),
                            Escalation {
                                cause: EscalationCause::NoSessionId,
                                mode: ReviewMode::Shadow,
                            },
                        ),
                    ]),
                    ..Default::default()
                },
                NOW,
            );
            let by_id = |id: &str| board.approvals.iter().find(|a| a.id == id).unwrap();
            assert_eq!(
                by_id(&reviewed).escalation,
                Some(EscalationCause::AutoApprovalOff)
            );
            assert_eq!(
                by_id(&prechecked).escalation,
                Some(EscalationCause::NoSessionId)
            );
            let text = serde_json::to_string(&board).unwrap();
            for review_only in EscalationCause::ALL
                .iter()
                .filter(|c| !c.before_any_call() && **c != EscalationCause::AutoApprovalOff)
            {
                assert!(!text.contains(review_only.code()), "{review_only:?}");
            }
            for field in [
                "\"decision\"",
                "\"reason\"",
                "\"rule_baseline\"",
                "\"review_id\"",
                "\"mode\"",
            ] {
                assert!(!text.contains(field), "{field}");
            }
        }

        /// Story 5.14 AC 3 (D-143): a live or rule escalation shows its
        /// cause, a call's included, since the request is the author's
        /// because of it; a reviewing request shows none and does not wait.
        #[test]
        fn a_live_escalation_shows_its_cause_and_a_reviewing_request_none() {
            use crate::relay::auto::EscalationCause;
            use crate::relay::catalogue::ActionClass;
            let approved = approved();
            let dirs = BTreeMap::from([("lab".to_owned(), "C:/path/to/Lab".to_owned())]);
            let ctx = ApprovalContext {
                approved: Some(&approved),
                project_dirs: &dirs,
                system_dir: "C:/Windows/System32",
            };
            let all = files(vec![
                request(&approved, "restart", "web01", NOW - MIN, None),
                request(&approved, "restart", "db01", NOW - MIN, None),
            ]);
            let mut approvals = pending(&all, &ctx, &DryRunInputs::default(), NOW);
            for approval in &mut approvals {
                approval.class = ActionClass::Auto;
            }
            approvals[1].state = ApprovalState::Reviewing;
            let (live, reviewing) = (approvals[0].id.clone(), approvals[1].id.clone());
            let escalation = |mode| Escalation {
                cause: EscalationCause::ReviewRejected,
                mode,
            };
            for mode in [ReviewMode::Live, ReviewMode::Rule] {
                let board = build_board(
                    &BoardInput {
                        approvals: approvals.clone(),
                        auto_escalations: BTreeMap::from([
                            (live.clone(), escalation(mode)),
                            (reviewing.clone(), escalation(mode)),
                        ]),
                        ..Default::default()
                    },
                    NOW,
                );
                let by_id = |id: &str| board.approvals.iter().find(|a| a.id == id).unwrap();
                assert_eq!(
                    by_id(&live).escalation,
                    Some(EscalationCause::ReviewRejected)
                );
                assert_eq!(by_id(&reviewing).escalation, None);
                assert_eq!(board.waiting_count, 1, "{mode:?}");
            }
        }

        #[test]
        fn invalid_requests_do_not_count() {
            let approved = approved();
            let (id, mut forged) = request(&approved, "restart", "db01", NOW - MIN, None);
            forged
                .params
                .insert("service".to_owned(), "web01".to_owned());
            let board = board_of(&files(vec![(id, forged)]), &DryRunInputs::default(), "[]");
            assert_eq!(board.approvals.len(), 1);
            assert!(!board.approvals[0].valid);
            assert!(board.approvals[0].invalid_reason.is_some());
            assert_eq!(board.waiting_count, 0);
        }

        #[test]
        fn session_name_comes_from_the_cards() {
            let approved = approved();
            let board = board_of(
                &files(vec![
                    request(&approved, "restart", "web01", NOW - 2 * MIN, Some("s1")),
                    request(&approved, "restart", "db01", NOW - MIN, Some("gone")),
                    request(&approved, "restart", "cache01", NOW - MIN / 2, None),
                ]),
                &DryRunInputs::default(),
                WAITING,
            );
            let names: Vec<Option<Untrusted<String>>> = board
                .approvals
                .iter()
                .map(|a| a.session_name.clone())
                .collect();
            assert_eq!(names, [Some(text("deploy-lab")), None, None]);
        }

        #[test]
        fn expired_and_decided_requests_are_not_listed() {
            let approved = approved();
            let live = request(&approved, "restart", "web01", NOW - MIN, None);
            let expired = request(&approved, "restart", "db01", NOW - PENDING_MS - 1, None);
            let (decided_id, decided) = request(&approved, "restart", "cache01", NOW - MIN, None);
            let mut all = files(vec![
                live.clone(),
                expired,
                (decided_id.clone(), decided.clone()),
            ]);
            all.rejected.insert(
                decided_id,
                Decision::by_author(NOW, &decided.request_key, None),
            );
            let board = board_of(&all, &DryRunInputs::default(), "[]");
            let ids: Vec<&str> = board.approvals.iter().map(|a| a.id.as_str()).collect();
            assert_eq!(ids, [live.0.as_str()]);
        }

        #[test]
        fn approval_view_carries_the_dry_run_output() {
            let approved = approved();
            let mut dry_runs = DryRunInputs::default();
            dry_runs.outputs.insert(
                "run-7".to_owned(),
                Ok("TASK [restart] changed: [web01]\n".to_owned()),
            );
            dry_runs
                .trees
                .insert("lab".to_owned(), Some(fingerprint("a")));
            let board = board_of(
                &files(vec![request(&approved, "apply", "web01", NOW - MIN, None)]),
                &dry_runs,
                "[]",
            );
            let dry_run = board.approvals[0].dry_run.as_ref().expect("a dry run");
            assert_eq!(dry_run.check, "verify");
            assert_eq!(dry_run.run_id, "run-7");
            assert_eq!(
                dry_run.output.as_deref(),
                Some("TASK [restart] changed: [web01]\n")
            );
            assert_eq!(board.approvals[0].files_unchanged, Some(true));
            let plain = board_of(
                &files(vec![request(
                    &approved,
                    "restart",
                    "web01",
                    NOW - MIN,
                    None,
                )]),
                &dry_runs,
                "[]",
            );
            assert!(plain.approvals[0].dry_run.is_none(), "no check, no dry run");
        }
    }

    /// AD-176: a sub-agent's start or stop is not its lead's activity, so
    /// a lead's state, its activity time and Needs you's order ignore it.
    #[test]
    fn subagent_lines_leave_the_session_state_unchanged() {
        let mut blocking = event(10, "s", "Notification");
        blocking.notification_type = Some(BLOCKING_NOTIFICATIONS[0].into());
        let base = vec![event(5, "s", "UserPromptSubmit"), blocking];
        let mut with = base.clone();
        for (offset, name) in crate::events::SUBAGENT_EVENTS.iter().enumerate() {
            let mut line = event(20 + offset as i64, "s", name);
            line.agent_id = Some("a1".into());
            with.push(line);
        }
        assert_eq!(group_events(&with), group_events(&base));
        let only_subagents: Vec<EventLine> = with[2..].to_vec();
        assert!(group_events(&only_subagents).is_empty());
    }

    /// D-547: an ended lead lists no sub-agent, though the buffer holds a
    /// start without a stop; the same lines on a live lead list it.
    #[test]
    fn an_ended_card_lists_no_subagent() {
        let mut start = event(20, "s", crate::events::SUBAGENT_EVENTS[0]);
        start.agent_id = Some("a1".into());
        let input = |state: &str| BoardInput {
            agents: Some(vec![record(&format!(
                r#"{{"id":"b1","sessionId":"s","kind":"background","state":"{state}","cwd":"C:/work/api","startedAt":1}}"#
            ))]),
            events: vec![event(5, "s", "UserPromptSubmit"), start.clone()],
            ..Default::default()
        };
        let live = build_board(&input("working"), 100);
        assert_eq!(live.columns.working[0].subagents.len(), 1);
        let ended = build_board(&input("done"), 100);
        let card = ended.columns.all().next().expect("the ended card");
        assert!(!matches!(card.column, Column::Working), "{:?}", card.column);
        assert!(card.subagents.is_empty());
    }

    fn rule(
        name: &str,
        path: &str,
        color: Option<ProjectColor>,
        priority: Priority,
    ) -> ProjectRule {
        ProjectRule {
            name: name.into(),
            path: path.into(),
            color,
            priority,
        }
    }

    /// AD-168, D-99: a card carries its project's priority, the same view
    /// the floor's project list holds.
    #[test]
    fn a_card_carries_its_project_priority() {
        for priority in Priority::ALL {
            let input = BoardInput {
                agents: Some(vec![record(
                    r#"{"pid":7,"sessionId":"s","kind":"interactive","status":"waiting","cwd":"C:/work/api/src","startedAt":1}"#,
                )]),
                projects: vec![rule("api", "c:/work/api", None, *priority)],
                ..Default::default()
            };
            let board = build_board(&input, 10);
            assert_eq!(board.columns.needs_you[0].project.priority, *priority);
            assert_eq!(board.floor_projects[0], board.columns.needs_you[0].project);
        }
        let derived = build_board(
            &BoardInput {
                agents: Some(vec![record(
                    r#"{"pid":7,"sessionId":"s","kind":"interactive","status":"idle","cwd":"C:/work/api","startedAt":1}"#,
                )]),
                ..Default::default()
            },
            10,
        );
        assert_eq!(
            derived.columns.your_turn[0].project.priority,
            Priority::Normal
        );
    }

    /// AD-168: the threshold travels in the board, in ms, the default
    /// without a floor value.
    #[test]
    fn the_board_carries_the_alert_threshold() {
        assert_eq!(
            build_board(&BoardInput::default(), 10).alert_after_ms,
            i64::from(ALERT_AFTER_MINUTES_DEFAULT) * 60_000
        );
        let input = BoardInput {
            alert_after_minutes: Some(25),
            ..Default::default()
        };
        assert_eq!(build_board(&input, 10).alert_after_ms, 25 * 60_000);
    }

    /// AD-68, D-99: the floor's projects in file order, a repeated name
    /// passed through, with no session needed.
    #[test]
    fn floor_projects_keep_file_order() {
        let input = BoardInput {
            projects: vec![
                rule(
                    "zeta",
                    "c:/work/zeta",
                    Some(ProjectColor::Plum),
                    Priority::High,
                ),
                rule("alpha", "c:/work/alpha", None, Priority::Normal),
                rule("zeta", "c:/work/zeta-two", None, Priority::Normal),
            ],
            ..Default::default()
        };
        let board = build_board(&input, 10);
        let names: Vec<&str> = board
            .floor_projects
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, ["zeta", "alpha", "zeta"]);
        assert_eq!(board.floor_projects[0].color, Some(ProjectColor::Plum));
        assert_eq!(board.floor_projects[0].priority, Priority::High);
        assert_eq!(board.floor_projects[1].color, Some(derived_color("alpha")));
    }

    /// D-99: the floor's view of a project equals the project a card of a
    /// `cwd` inside it carries.
    #[test]
    fn floor_project_equals_the_card_project_of_a_cwd_inside() {
        let rules = vec![
            rule("api", "c:/work/api", None, Priority::High),
            rule(
                "docs",
                "c:/work/docs",
                Some(ProjectColor::Green),
                Priority::Normal,
            ),
        ];
        for (index, cwd) in ["C:/work/api/src/deep", r"C:\Work\Docs"].iter().enumerate() {
            let view = project_for(Some(cwd), &rules);
            assert_eq!(view, rules[index].view(), "{cwd}");
        }
    }

    #[test]
    fn floor_projects_are_empty_without_projects() {
        assert!(
            build_board(&BoardInput::default(), 10)
                .floor_projects
                .is_empty()
        );
    }

    fn stop(ts: i64, head: Option<&str>, tail: Option<&str>) -> EventLine {
        let mut line = event(ts, "s", "Stop");
        line.last_message = head.map(str::to_owned);
        line.last_message_tail = tail.map(str::to_owned);
        line
    }

    fn listed_with(events: Vec<EventLine>, no_excerpts: bool) -> Card {
        let input = BoardInput {
            agents: Some(vec![record(
                r#"{"pid":7,"sessionId":"s","kind":"interactive","status":"idle","cwd":"C:/work/api","startedAt":1}"#,
            )]),
            events,
            no_excerpts,
            ..Default::default()
        };
        build_board(&input, 10).columns.your_turn[0].clone()
    }

    /// D-182: a card carries the answer's tail beside its head, and
    /// `no_excerpts` withholds both.
    #[test]
    fn cards_carry_the_answer_tail_and_withhold_it_without_excerpts() {
        let lines = vec![stop(5, Some("head…"), Some("…tail"))];
        let card = listed_with(lines.clone(), false);
        assert_eq!(card.last_message, Some(text("head…")));
        assert_eq!(card.last_message_tail, Some(text("…tail")));
        let hidden = listed_with(lines, true);
        assert_eq!(
            (hidden.last_message, hidden.last_message_tail),
            (None, None)
        );
    }

    /// D-182: a later short answer, whole in its head, clears an older
    /// answer's tail; a `Stop` without an answer keeps both.
    #[test]
    fn a_short_answer_clears_an_older_tail() {
        let card = listed_with(
            vec![
                stop(5, Some("long head…"), Some("…old tail")),
                stop(9, Some("short"), None),
            ],
            false,
        );
        assert_eq!(card.last_message, Some(text("short")));
        assert_eq!(card.last_message_tail, None);
        let kept = listed_with(
            vec![
                stop(5, Some("long head…"), Some("…old tail")),
                stop(9, None, None),
            ],
            false,
        );
        assert_eq!(kept.last_message_tail, Some(text("…old tail")));
    }

    /// AD-74, D-130: a card's name and excerpts and an approval's session
    /// name are untrusted (marked inside `untrusted::wrapped`), while Claude
    /// Code's values and the project stay plain; outside it every one is the
    /// plain string it was.
    #[test]
    fn card_texts_serialize_as_plain_strings() {
        use serde_json::{Value, json};
        let mut prompt = event(5, "s", "UserPromptSubmit");
        prompt.prompt = Some("deploy".into());
        let input = BoardInput {
            agents: Some(vec![record(
                r#"{"pid":7,"sessionId":"s","kind":"interactive","status":"idle","name":"deploy-lab","cwd":"C:/work/api","startedAt":1}"#,
            )]),
            events: vec![prompt, stop(9, Some("head…"), Some("…tail"))],
            approvals: vec![ApprovalView {
                id: "0123456789abcdef-1".into(),
                project: "api".into(),
                action: "restart".into(),
                class: crate::relay::catalogue::ActionClass::Human,
                state: crate::relay::approvals::ApprovalState::Pending,
                escalation: None,
                description: String::new(),
                argv: Vec::new(),
                http: None,
                valid: false,
                invalid_reason: Some("no catalogue is approved".into()),
                session_id: Some("s".into()),
                session_name: None,
                requested_at_ms: 1,
                expires_at_ms: 2,
                dry_run: None,
                binding: None,
                bound_files: None,
                files_unchanged: None,
            }],
            ..Default::default()
        };
        let board = build_board(&input, 10);
        let texts = |board: &Value| {
            let card = &board["columns"]["your_turn"][0];
            vec![
                card["name"].clone(),
                card["last_prompt"].clone(),
                card["last_message"].clone(),
                card["last_message_tail"].clone(),
                board["approvals"][0]["session_name"].clone(),
            ]
        };
        let expected = ["deploy-lab", "deploy", "head…", "…tail", "deploy-lab"];
        let plain = serde_json::to_value(&board).unwrap();
        assert_eq!(texts(&plain), expected.map(|t| json!(t)));
        let marked = crate::untrusted::wrapped(|| serde_json::to_value(&board)).unwrap();
        assert_eq!(texts(&marked), expected.map(|t| json!({ "untrusted": t })));
        for board in [&plain, &marked] {
            let card = &board["columns"]["your_turn"][0];
            assert_eq!(card["key"], "pid:7:1");
            assert_eq!(card["session_id"], "s");
            assert_eq!(card["cwd"], "C:/work/api");
            assert_eq!(card["project"]["name"], "api");
            assert_eq!(board["approvals"][0]["action"], "restart");
        }
    }

    #[test]
    fn no_unlisted_cards_before_the_first_agents_answer() {
        let input = BoardInput {
            agents: None,
            events: vec![event(1, "s", "UserPromptSubmit")],
            ..Default::default()
        };
        assert_eq!(build_board(&input, 10_000_000).columns.all().count(), 0);
    }
}
