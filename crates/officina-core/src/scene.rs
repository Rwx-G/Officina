//! The scene state (v0.4, AD-167): which zone each live session belongs in
//! and whether it is in alert, decided once, here, from the board's facts.
//! The scene places the board's sessions and never classifies (AD-13): the
//! frontend reads [`SceneState`] and re-implements none of these rules.
//!
//! Pure: time is the board's `now_ms`, the threshold the board's own, and
//! the previous board's states an input, so a stale snapshot never clears
//! an alert (D-494).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::board::{Badge, Card, Column, Columns, SessionEvents};
use crate::events::FRESH_START_SOURCE;
use crate::floor::Priority;
use crate::relay::approvals::ApprovalView;

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Where a live session stands in the scene (AD-167).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum Zone {
        /// Started from the command line, no prompt yet.
        Reception,
        /// Working, at its desk.
        Office,
        /// Waiting for the author, queued at the decision office.
        Decision,
        /// Waiting, and in alert.
        Alert,
        /// Its round finished: the author's turn.
        Lounge,
        /// The data cannot place it: it stays where it was.
        Unknown,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why a waiting session is in alert (FR-10).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum AlertCause {
        /// Its project is high priority.
        Priority,
        /// It has waited at least the floor's threshold.
        Waited,
    }
}

/// The scene's view of one live card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SceneState {
    pub zone: Zone,
    /// When the wait began, when it is known: the latest blocking
    /// notification after the last prompt and stop, or the earliest
    /// pending relay request of an approval-only wait.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub waiting_since_ms: Option<i64>,
    pub alert: Option<AlertCause>,
    /// The session waits on a pending relay request only (D-496).
    pub approval: bool,
}

/// What [`place`] needs beyond the cards.
pub struct SceneFacts<'a> {
    /// The agents snapshot failed: the cards are the last known ones.
    pub stale: bool,
    pub now_ms: i64,
    pub alert_after_ms: i64,
    /// The pending relay requests (AD-35); only the valid ones waiting on
    /// the author, not on a review, make a wait (D-571).
    pub approvals: &'a [ApprovalView],
    /// The scene state of each card key on the last board built.
    pub previous: &'a BTreeMap<String, SceneState>,
}

/// The events a card was built from, by card key, as [`place`] reads them.
pub type CardEvents<'a> = BTreeMap<String, &'a SessionEvents>;

/// True when the session's latest `SessionStart` came from the command line
/// and no prompt followed it: positive evidence of an arrival (D-495).
fn arriving(events: &SessionEvents) -> bool {
    let (Some(source), Some(started)) = (
        events.last_session_start_source.as_deref(),
        events.last_session_start_at_ms,
    ) else {
        return false;
    };
    source == FRESH_START_SOURCE && events.last_prompt_at_ms.is_none_or(|p| p < started)
}

/// The latest blocking notification after the last prompt and stop: the
/// start of a Needs you wait, when the hook saw it.
fn blocking_since(events: &SessionEvents) -> Option<i64> {
    let notified = events.last_blocking_notification_at_ms?;
    (events.last_prompt_at_ms.is_none_or(|p| notified > p)
        && events.last_stop_at_ms.is_none_or(|s| notified > s))
    .then_some(notified)
}

/// The scene state of `card`, by AD-167's rules, first match wins.
fn state_of(card: &Card, events: Option<&SessionEvents>, facts: &SceneFacts) -> SceneState {
    let unknown = || {
        let previous = facts.previous.get(&card.key);
        SceneState {
            zone: Zone::Unknown,
            waiting_since_ms: previous.and_then(|p| p.waiting_since_ms),
            alert: previous.and_then(|p| p.alert),
            approval: false,
        }
    };
    if facts.stale || card.badge == Some(Badge::Unknown) {
        return unknown();
    }
    let requested = card.session_id.as_deref().and_then(|sid| {
        facts
            .approvals
            .iter()
            .filter(|a| a.waits_on_the_author() && a.session_id.as_deref() == Some(sid))
            .map(|a| a.requested_at_ms)
            .min()
    });
    let high = card.project.priority == Priority::High;
    let waiting = |since: Option<i64>, approval: bool| {
        let alert = if high {
            Some(AlertCause::Priority)
        } else if !approval && since.is_some_and(|s| facts.now_ms - s >= facts.alert_after_ms) {
            Some(AlertCause::Waited)
        } else {
            None
        };
        SceneState {
            zone: if alert.is_some() {
                Zone::Alert
            } else {
                Zone::Decision
            },
            waiting_since_ms: since,
            alert,
            approval,
        }
    };
    match card.column {
        Column::NeedsYou => waiting(events.and_then(blocking_since), false),
        Column::Working if requested.is_some() => waiting(requested, true),
        Column::YourTurn if events.is_some_and(arriving) => SceneState {
            zone: Zone::Reception,
            waiting_since_ms: None,
            alert: None,
            approval: false,
        },
        Column::YourTurn => SceneState {
            zone: Zone::Lounge,
            waiting_since_ms: None,
            alert: None,
            approval: false,
        },
        Column::Working => SceneState {
            zone: Zone::Office,
            waiting_since_ms: None,
            alert: None,
            approval: false,
        },
        Column::ToResume | Column::Done => unknown(),
    }
}

/// Sets `Card::scene` on every live card (Needs you, Your turn, Working),
/// `None` on the others, which the scene does not draw.
pub fn place(columns: &mut Columns, events: &CardEvents, facts: &SceneFacts) {
    for card in columns
        .needs_you
        .iter_mut()
        .chain(columns.your_turn.iter_mut())
        .chain(columns.working.iter_mut())
    {
        let card_events = events.get(card.key.as_str()).copied();
        card.scene = Some(state_of(card, card_events, facts));
    }
    for card in columns.to_resume.iter_mut().chain(columns.done.iter_mut()) {
        card.scene = None;
    }
}

/// How many live cards are in alert, a last known alert included.
pub fn alert_count(columns: &Columns) -> u32 {
    let count = columns
        .all()
        .filter(|card| card.scene.is_some_and(|s| s.alert.is_some()))
        .count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// When the next Needs you wait reaches the threshold: the earliest
/// `waiting_since_ms + alert_after_ms` over Needs you cards in a known zone,
/// not in alert, with a known start. Approval-only and unknown cards never
/// escalate by time (D-494, D-496).
pub fn next_escalation_at_ms(columns: &Columns, alert_after_ms: i64) -> Option<i64> {
    columns
        .needs_you
        .iter()
        .filter_map(|card| card.scene)
        .filter(|s| s.zone == Zone::Decision && s.alert.is_none() && !s.approval)
        .filter_map(|s| s.waiting_since_ms)
        .map(|since| since + alert_after_ms)
        .min()
}

/// The scene state of every card of `columns` that has one, by key: what
/// the next board receives as its previous states.
pub fn states_by_key(columns: &Columns) -> BTreeMap<String, SceneState> {
    columns
        .all()
        .filter_map(|card| Some((card.key.clone(), card.scene?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::parse_agents;
    use crate::board::{BoardInput, Staleness, build_board};
    use crate::events::EventLine;

    const NOW: i64 = 1_793_102_400_000;

    fn blocking(sid: &str, ts: i64) -> EventLine {
        serde_json::from_value(serde_json::json!({
            "v": 1, "ts": ts, "event": "Notification", "session_id": sid,
            "notification_type": crate::board::BLOCKING_NOTIFICATIONS[0],
        }))
        .unwrap()
    }

    /// Two Needs you sessions, the first in board order waiting longer, the
    /// second on a high-priority project; a done background session.
    fn input() -> BoardInput {
        BoardInput {
            agents: Some(
                parse_agents(
                    r#"[{"pid":1,"sessionId":"a","kind":"interactive","status":"waiting","cwd":"C:/work/api","startedAt":1},
                        {"pid":2,"sessionId":"b","kind":"interactive","status":"waiting","cwd":"C:/work/web","startedAt":1},
                        {"id":"d","sessionId":"d","kind":"background","state":"done","cwd":"C:/work/api","startedAt":1793102300000}]"#,
                )
                .unwrap(),
            ),
            events: vec![blocking("a", NOW - 60_000), blocking("b", NOW - 30_000)],
            projects: vec![
                crate::board::ProjectRule {
                    name: "api".into(),
                    path: "c:/work/api".into(),
                    color: None,
                    priority: Priority::Normal,
                },
                crate::board::ProjectRule {
                    name: "web".into(),
                    path: "c:/work/web".into(),
                    color: None,
                    priority: Priority::High,
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn ended_cards_have_no_scene_state() {
        let board = build_board(&input(), NOW);
        assert!(!board.columns.done.is_empty());
        assert!(board.columns.done.iter().all(|c| c.scene.is_none()));
        let live = board
            .columns
            .needs_you
            .iter()
            .chain(&board.columns.your_turn)
            .chain(&board.columns.working);
        assert!(live.into_iter().all(|c| c.scene.is_some()));
    }

    /// AD-167, D-465: an alert moves nobody in board order, so the digits
    /// keep acting on the same sessions.
    #[test]
    fn board_order_is_unchanged_by_alerts() {
        let board = build_board(&input(), NOW);
        let keys: Vec<&str> = board
            .columns
            .needs_you
            .iter()
            .map(|c| c.key.as_str())
            .collect();
        assert_eq!(keys, ["pid:1:1", "pid:2:1"]);
        assert_eq!(
            board.columns.needs_you[1].scene.unwrap().alert,
            Some(AlertCause::Priority)
        );
        assert_eq!(board.columns.needs_you[0].scene.unwrap().alert, None);
    }

    #[test]
    fn alert_count_counts_alerted_cards() {
        let board = build_board(&input(), NOW);
        assert_eq!(board.alert_count, 1);
        assert_eq!(
            board.waiting_count, 2,
            "waiting_count is unchanged by alerts"
        );
        let later = build_board(&input(), NOW - 60_000 + board.alert_after_ms);
        assert_eq!(later.alert_count, 2);
    }

    /// D-494: a stale board keeps the alerts of the previous one on its
    /// unknown cards, and counts them.
    #[test]
    fn alert_count_counts_unknown_cards_that_keep_their_alert() {
        let first = build_board(&input(), NOW);
        let stale = BoardInput {
            stale: Some(Staleness {
                since_ms: NOW,
                error: "claude agents failed".into(),
            }),
            previous_scene: states_by_key(&first.columns),
            ..input()
        };
        let board = build_board(&stale, NOW + 1_000);
        assert!(
            board
                .columns
                .needs_you
                .iter()
                .all(|c| c.scene.unwrap().zone == Zone::Unknown)
        );
        assert_eq!(board.alert_count, first.alert_count);
        assert_eq!(board.next_escalation_at_ms, None);
    }
}
