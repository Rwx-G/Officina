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

/// Version of the board JSON contract.
pub const BOARD_SCHEMA: u32 = 1;

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
    pub waiting_count: u32,
    pub columns: Columns,
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
        let events = record
            .session_id
            .as_ref()
            .and_then(|sid| grouped.get(sid))
            .unwrap_or(&empty);
        let Some(card) = listed_card(record, events, input) else {
            continue;
        };
        if let Some(sid) = &record.session_id {
            listed.insert(sid);
        }
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

    Board {
        schema: BOARD_SCHEMA,
        generated_at_ms: now_ms,
        floor: input.floor_name.clone(),
        stale: input.stale.clone(),
        waiting_count: u32::try_from(columns.needs_you.len()).unwrap_or(u32::MAX),
        columns,
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
    fn no_unlisted_cards_before_the_first_agents_answer() {
        let input = BoardInput {
            agents: None,
            events: vec![event(1, "s", "UserPromptSubmit")],
            ..Default::default()
        };
        assert_eq!(build_board(&input, 10_000_000).columns.all().count(), 0);
    }
}
