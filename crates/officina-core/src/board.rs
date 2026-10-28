//! Classification and board construction (FR-6, FR-7, AD-11, AD-13).
//!
//! `build_board` is the one function that decides what the author sees. It is
//! pure: the same input gives the same board, whoever calls it (the CLI, the
//! GUI collector, the vector tests).

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::agents::{AgentRecord, LifecycleState, LiveStatus, SessionKind};
use crate::events::EventLine;
use crate::floor::ProjectColor;
use crate::quota::{QuotaFile, QuotaWindow};
use crate::relay::approvals::ApprovalView;

/// Version of the board JSON contract. 2: approval requests (AD-35).
pub const BOARD_SCHEMA: u32 = 2;

/// Sessions that ended, failed or finished are listed for this long.
pub const DONE_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;

/// A session seen in hook events but not yet in `claude agents` output is
/// presumed alive for this long: the hook fires before the next agents
/// refresh, and a fresh session must not flash as "interrupted".
pub const UNLISTED_GRACE_MS: i64 = 30_000;

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

/// The project a card belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ProjectView {
    pub name: String,
    pub color: Option<ProjectColor>,
}

/// One session on the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Card {
    pub key: String,
    pub column: Column,
    pub badge: Option<Badge>,
    pub kind: Option<SessionKindView>,
    pub name: String,
    pub project: ProjectView,
    pub session_id: Option<String>,
    /// Background session id, the argument of `claude attach`.
    pub background_id: Option<String>,
    pub pid: Option<u32>,
    pub cwd: Option<String>,
    pub waiting_for: Option<String>,
    pub last_prompt: Option<String>,
    pub last_message: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub last_event_at_ms: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub started_at_ms: Option<i64>,
    /// True when a click can open `claude attach` for this card.
    pub attach_available: bool,
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
}

/// A declared project, path already resolved and normalised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRule {
    pub name: String,
    pub path: String,
    pub color: Option<ProjectColor>,
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
    pub cwd: Option<String>,
    /// Reason of the last `SessionEnd`, if the session ended.
    pub end_reason: Option<String>,
    pub ended_at_ms: Option<i64>,
    pub saw_prompt: bool,
    /// Id of the `claude` process the hook ran under (E14), if recorded.
    pub claude_pid: Option<u32>,
    /// Time of the session's first event.
    pub first_event_at_ms: Option<i64>,
}

fn later(a: Option<i64>, b: i64) -> Option<i64> {
    Some(a.map_or(b, |a| a.max(b)))
}

/// Groups events by session id. Events without a session id are dropped.
pub fn group_events(events: &[EventLine]) -> BTreeMap<String, SessionEvents> {
    let mut sorted: Vec<&EventLine> = events.iter().collect();
    sorted.sort_by_key(|e| e.ts);
    let mut grouped: BTreeMap<String, SessionEvents> = BTreeMap::new();
    for event in sorted {
        let Some(sid) = &event.session_id else {
            continue;
        };
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
                if event.last_message.is_some() {
                    entry.last_message.clone_from(&event.last_message);
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
            }
            _ => {}
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
            name: "unknown".into(),
            color: None,
        };
    };
    if projects.is_empty() {
        let name = folder_name(cwd).to_owned();
        let color = Some(derived_color(&name));
        return ProjectView { name, color };
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
                name: "other".into(),
                color: None,
            },
            |p| ProjectView {
                name: p.name.clone(),
                color: Some(p.color.unwrap_or_else(|| derived_color(&p.name))),
            },
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
            text.clone()
        }
    };
    Card {
        key: facts.key,
        column: placement.column,
        badge: placement.badge,
        kind: facts.kind,
        name: facts.name.unwrap_or_else(|| project.name.clone()),
        project,
        session_id: facts.session_id,
        background_id: facts.background_id,
        pid: facts.pid,
        cwd: facts.cwd,
        waiting_for: facts.waiting_for,
        last_prompt: excerpt(&events.last_prompt),
        last_message: excerpt(&events.last_message),
        last_event_at_ms: facts.last_event_at_ms,
        started_at_ms: facts.started_at_ms,
        attach_available: facts.attach_available,
    }
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
    let mut listed: HashSet<&str> = HashSet::new();
    let mut unrecognized: Vec<String> = Vec::new();

    for record in input.agents.iter().flatten() {
        unrecognized.extend(record.unrecognized.iter().cloned());
        let own = record.session_id.as_deref();
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
        listed.extend(same_process);
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
            if let Some(card) =
                unlisted_card(sid, events, input, now_ms).filter(|c| on_board(c, now_ms))
            {
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
            ..approval.clone()
        })
        .collect();
    let waiting = columns.needs_you.len() + approvals.iter().filter(|a| a.valid).count();

    Board {
        schema: BOARD_SCHEMA,
        generated_at_ms: now_ms,
        floor: input.floor_name.clone(),
        stale: input.stale.clone(),
        waiting_count: u32::try_from(waiting).unwrap_or(u32::MAX),
        columns,
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
        },
        done_window_ms: DONE_WINDOW_MS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::parse_agents;

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
            claude_pid: None,
            truncated: false,
        }
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
            },
            ProjectRule {
                name: "api".into(),
                path: "c:/work/api".into(),
                color: Some(ProjectColor::Blue),
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
            assert_eq!(card.name, "api", "{}", card.key);
            assert_eq!(card.last_prompt.as_deref(), Some("deploy"), "{}", card.key);
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
            board.columns.your_turn[0].last_prompt.as_deref(),
            Some("after clear")
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
            let relay = load(FLOOR, "C:/path/to/officina.toml").unwrap();
            ApprovedCatalogue {
                approved_at_ms: NOW - 60 * MIN,
                hash: relay.hash,
                floor_file: None,
                canonical: relay.canonical,
            }
        }

        fn fingerprint(text: &str) -> TreeFingerprint {
            TreeFingerprint {
                sha256: crate::relay::sha256_hex(text.as_bytes()),
                untracked_by_size: 0,
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
            let names: Vec<Option<&str>> = board
                .approvals
                .iter()
                .map(|a| a.session_name.as_deref())
                .collect();
            assert_eq!(names, [Some("deploy-lab"), None, None]);
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
                Decision {
                    decided_at_ms: NOW,
                    request_key: decided.request_key,
                },
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
            assert_eq!(dry_run.files_unchanged, Some(true));
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
