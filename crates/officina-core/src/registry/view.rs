//! The registry as the Subjects view and `officina subjects --json` show it
//! (FR-6, AD-53, AD-54, D-93, D-96): built here from the registry's records
//! and a board, so the window and the command line print the same thing.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::board::{Badge, Board, Card, Column, SessionKindView};

use super::filing::excerpt_time;
use super::fold::SUPERSESSION_WINDOW_MS;
use super::retention::EXCERPT_TTL_MS;
use super::types::{
    Ending, ExcerptRecord, RegistryReport, RestoreNotice, SessionRecord, Source, SubjectRecord,
    SubjectStatus,
};

/// Version of the view's JSON contract. Stays 1 through Epic 3 (P-13).
pub const REGISTRY_VIEW_SCHEMA: u32 = 1;

/// How long the view names the copy a damaged registry was restored from
/// (D-93).
pub const RESTORE_NOTICE_MS: i64 = 7 * 86_400_000;

/// How many non-interactive runs each project lists, newest first; the
/// rest are counted (D-96).
pub const NON_INTERACTIVE_LISTED: usize = 10;

/// The registry for the Subjects view (AD-54): its state, the ingestion's
/// lag, the active and `to_resume` subjects with their sessions and latest
/// excerpts, the unfiled sessions per project. Archived subjects are listed
/// on demand only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RegistryView {
    pub schema: u32,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub generated_at_ms: i64,
    /// Why nothing is shown when the registry could not be read, and the
    /// folder to exclude from a sync client when it sits in one (AD-39).
    pub registry: RegistryReport,
    pub ingestion: IngestionView,
    /// `active` first, then `to_resume`, each by its latest activity.
    pub subjects: Vec<SubjectView>,
    /// Sessions under no subject, by project.
    pub unfiled: Vec<UnfiledProject>,
    /// Every project the registry knows, those of its sessions and of its
    /// subjects, archived ones included: where a subject may be created
    /// (D-95).
    pub projects: Vec<String>,
    /// The copy a damaged registry was restored from, for
    /// [`RESTORE_NOTICE_MS`] after the restore (D-93).
    pub restore_notice: Option<RestoreNotice>,
    pub thresholds: Thresholds,
}

/// How far the registry lags behind the event files (AD-53, D-84).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct IngestionView {
    /// Bytes of event files not ingested yet: the lag behind a closed
    /// window.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub pending_bytes: u64,
    /// Event files not yet complete in the registry.
    pub incomplete_files: u32,
    /// Event files found shorter than their offset and read again (D-84).
    pub rewritten_files: u32,
    /// Event lines that could not be read.
    pub corrupt_lines: u32,
}

/// Numbers the view is built with (AD-5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Thresholds {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub restore_notice_ms: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub excerpt_ttl_ms: i64,
    pub non_interactive_listed: u32,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub supersession_window_ms: i64,
}

/// One subject that is not archived.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SubjectView {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub id: i64,
    pub project: String,
    pub name: String,
    pub status: SubjectStatus,
    pub status_src: Source,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub status_at_ms: i64,
    /// "Routine, could have run alone", set when it was archived (FR-13,
    /// P-10).
    pub routine: bool,
    /// Its sessions, the latest seen first.
    pub sessions: Vec<SessionView>,
    /// The latest prompt and answer of its sessions (FR-4).
    pub excerpts: Option<ExcerptView>,
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What ended a session (AD-47).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum EndHow {
        /// Its `SessionEnd`, with its reason.
        SessionEnd,
        /// It left `claude agents` without one.
        Interrupted,
        /// A background session's state, given as the reason.
        State,
    }
}

/// How a session ended, with where that came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EndView {
    pub how: EndHow,
    pub reason: Option<String>,
    pub src: Source,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub at_ms: i64,
}

/// A session's live board card, joined by session id (AD-54).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LiveCard {
    pub key: String,
    pub column: Column,
    pub badge: Option<Badge>,
    pub waiting_for: Option<String>,
}

/// One session of the registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SessionView {
    pub session_id: String,
    pub project: String,
    pub name: Option<String>,
    pub kind: Option<SessionKindView>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub first_seen_ms: i64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub last_seen_ms: i64,
    pub ended: Option<EndView>,
    pub superseded: bool,
    pub non_interactive: bool,
    /// Its card on the board, while it has one.
    pub card: Option<LiveCard>,
}

/// The latest excerpts a subject keeps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ExcerptView {
    pub session_id: String,
    pub prompt: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub prompt_at_ms: Option<i64>,
    pub answer_head: Option<String>,
    pub answer_tail: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub answer_at_ms: Option<i64>,
}

/// The sessions of one project under no subject (D-96).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct UnfiledProject {
    pub project: String,
    /// Its sessions that are not non-interactive runs, the latest first.
    pub sessions: Vec<SessionView>,
    /// How many non-interactive runs it has.
    pub non_interactive_count: u32,
    /// The [`NON_INTERACTIVE_LISTED`] most recent of them.
    pub non_interactive: Vec<SessionView>,
}

/// What the view is built from.
#[derive(Debug, Clone)]
pub struct ViewInput<'a> {
    /// The registry's state: in any state but `ready`, the lists are empty.
    pub registry: RegistryReport,
    pub ingestion: IngestionView,
    pub subjects: &'a [SubjectRecord],
    pub sessions: &'a [SessionRecord],
    pub excerpts: &'a [ExcerptRecord],
    /// The board the live cards come from, when there is one.
    pub board: Option<&'a Board>,
    pub restore_notice: Option<RestoreNotice>,
}

fn end_view(record: &SessionRecord) -> Option<EndView> {
    let ending = record.ending()?;
    let (how, reason) = match ending.value {
        Ending::SessionEnd { reason } => (EndHow::SessionEnd, reason),
        Ending::Interrupted => (EndHow::Interrupted, None),
        Ending::State { state } => (
            EndHow::State,
            serde_json::to_value(state)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned)),
        ),
    };
    Some(EndView {
        how,
        reason,
        src: ending.src,
        at_ms: ending.at_ms,
    })
}

fn session_view(record: &SessionRecord, cards: &BTreeMap<&str, &Card>) -> SessionView {
    SessionView {
        session_id: record.session_id.clone(),
        project: record.project.clone(),
        name: record.name.as_ref().map(|n| n.value.clone()),
        kind: record.kind.as_ref().map(|k| k.value.into()),
        first_seen_ms: record.first_seen_ms,
        last_seen_ms: record.last_seen_ms,
        ended: end_view(record),
        superseded: record.is_superseded(),
        non_interactive: record.is_non_interactive(),
        card: cards.get(record.session_id.as_str()).map(|card| LiveCard {
            key: card.key.clone(),
            column: card.column,
            badge: card.badge,
            waiting_for: card.waiting_for.clone(),
        }),
    }
}

/// The latest seen first, then by id.
fn latest_first(sessions: &mut [SessionView]) {
    sessions.sort_by(|a, b| {
        b.last_seen_ms
            .cmp(&a.last_seen_ms)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });
}

/// The board's cards by session id, for joining (AD-54).
fn cards_by_session(board: Option<&Board>) -> BTreeMap<&str, &Card> {
    board
        .iter()
        .flat_map(|board| board.columns.all())
        .filter_map(|card| Some((card.session_id.as_deref()?, card)))
        .collect()
}

/// One subject with `records`, the sessions counting under it, and its
/// latest activity, which orders the list.
fn subject_view(
    subject: &SubjectRecord,
    records: &[&SessionRecord],
    excerpts: &BTreeMap<&str, &ExcerptRecord>,
    cards: &BTreeMap<&str, &Card>,
) -> (i64, SubjectView) {
    let latest_excerpt = records
        .iter()
        .filter_map(|r| excerpts.get(r.session_id.as_str()))
        .max_by_key(|e| (excerpt_time(e), e.session_id.clone()))
        .map(|e| excerpt_view(e));
    let activity = records
        .iter()
        .map(|r| r.last_seen_ms)
        .max()
        .unwrap_or(subject.status.at_ms);
    let mut sessions: Vec<SessionView> = records.iter().map(|r| session_view(r, cards)).collect();
    latest_first(&mut sessions);
    (
        activity,
        SubjectView {
            id: subject.id,
            project: subject.project.clone(),
            name: subject.name.clone(),
            status: subject.status.value,
            status_src: subject.status.src,
            status_at_ms: subject.status.at_ms,
            routine: subject.routine,
            sessions,
            excerpts: latest_excerpt,
        },
    )
}

fn excerpt_view(e: &ExcerptRecord) -> ExcerptView {
    ExcerptView {
        session_id: e.session_id.clone(),
        prompt: e.prompt.clone(),
        prompt_at_ms: e.prompt_at_ms,
        answer_head: e.answer_head.clone(),
        answer_tail: e.answer_tail.clone(),
        answer_at_ms: e.answer_at_ms,
    }
}

/// The projects of `subjects` and `sessions`, sorted, each once: those the
/// writer accepts for a new subject (D-95).
fn known_projects(subjects: &[SubjectRecord], sessions: &[SessionRecord]) -> Vec<String> {
    let known: std::collections::BTreeSet<&str> = subjects
        .iter()
        .map(|s| s.project.as_str())
        .chain(sessions.iter().map(|s| s.project.as_str()))
        .collect();
    known.into_iter().map(str::to_owned).collect()
}

/// True for a subject the archived list shows: archived by the author or a
/// verdict, not merged into another (D-55).
pub fn is_listed_archived(subject: &SubjectRecord) -> bool {
    subject.status.value == SubjectStatus::Archived && subject.merged_into.is_none()
}

/// The archived subjects the author may reopen (`list_archived`, AD-54),
/// the latest archived first, each with its sessions and their live cards;
/// merged subjects are left out (D-55).
///
/// ```
/// use officina_core::registry::view::archived_subjects;
/// assert!(archived_subjects(&[], &[], None).is_empty());
/// ```
pub fn archived_subjects(
    subjects: &[SubjectRecord],
    sessions: &[SessionRecord],
    board: Option<&Board>,
) -> Vec<SubjectView> {
    let cards = cards_by_session(board);
    let mut listed: Vec<SubjectView> = subjects
        .iter()
        .filter(|s| is_listed_archived(s))
        .map(|subject| {
            let records: Vec<&SessionRecord> = sessions
                .iter()
                .filter(|r| r.filed_subject() == Some(subject.id))
                .collect();
            subject_view(subject, &records, &BTreeMap::new(), &cards).1
        })
        .collect();
    listed.sort_by(|a, b| {
        b.status_at_ms
            .cmp(&a.status_at_ms)
            .then_with(|| a.id.cmp(&b.id))
    });
    listed
}

/// The view with nothing read, saying why in `report` (D-47): what a
/// command line prints, and the window shows, while the registry cannot be
/// read.
///
/// ```
/// use officina_core::registry::types::{RegistryReport, RegistryState};
/// use officina_core::registry::view::unread_view;
/// let report = RegistryReport { state: RegistryState::Network, detail: None, exclude_from_sync: None };
/// let view = unread_view(report, 1_000);
/// assert_eq!(view.registry.state, RegistryState::Network);
/// assert!(view.subjects.is_empty());
/// ```
pub fn unread_view(report: RegistryReport, now_ms: i64) -> RegistryView {
    build_view(
        &ViewInput {
            registry: report,
            ingestion: IngestionView::default(),
            subjects: &[],
            sessions: &[],
            excerpts: &[],
            board: None,
            restore_notice: None,
        },
        now_ms,
    )
}

/// Builds the view (AD-54).
///
/// ```
/// use officina_core::registry::types::{RegistryReport, RegistryState};
/// use officina_core::registry::view::{IngestionView, ViewInput, build_view};
/// let view = build_view(
///     &ViewInput {
///         registry: RegistryReport { state: RegistryState::Absent, detail: None, exclude_from_sync: None },
///         ingestion: IngestionView::default(),
///         subjects: &[], sessions: &[], excerpts: &[], board: None, restore_notice: None,
///     },
///     1_000,
/// );
/// assert!(view.subjects.is_empty() && view.unfiled.is_empty());
/// ```
pub fn build_view(input: &ViewInput, now_ms: i64) -> RegistryView {
    let cards = cards_by_session(input.board);
    let excerpts: BTreeMap<&str, &ExcerptRecord> = input
        .excerpts
        .iter()
        .map(|e| (e.session_id.as_str(), e))
        .collect();
    let mut by_subject: BTreeMap<i64, Vec<&SessionRecord>> = BTreeMap::new();
    let mut unfiled: BTreeMap<&str, UnfiledProject> = BTreeMap::new();
    for record in input.sessions {
        match record.filed_subject() {
            Some(subject) => by_subject.entry(subject).or_default().push(record),
            None => {
                let project =
                    unfiled
                        .entry(record.project.as_str())
                        .or_insert_with(|| UnfiledProject {
                            project: record.project.clone(),
                            sessions: Vec::new(),
                            non_interactive_count: 0,
                            non_interactive: Vec::new(),
                        });
                let view = session_view(record, &cards);
                if view.non_interactive {
                    project.non_interactive_count += 1;
                    project.non_interactive.push(view);
                } else {
                    project.sessions.push(view);
                }
            }
        }
    }
    let mut subjects: Vec<(i64, SubjectView)> = input
        .subjects
        .iter()
        .filter(|s| s.status.value != SubjectStatus::Archived)
        .map(|subject| {
            let records = by_subject.remove(&subject.id).unwrap_or_default();
            subject_view(subject, &records, &excerpts, &cards)
        })
        .collect();
    subjects.sort_by(|(a_at, a), (b_at, b)| {
        let rank = |s: &SubjectView| s.status != SubjectStatus::Active;
        rank(a)
            .cmp(&rank(b))
            .then_with(|| b_at.cmp(a_at))
            .then_with(|| a.id.cmp(&b.id))
    });
    let unfiled = unfiled
        .into_values()
        .map(|mut project| {
            latest_first(&mut project.sessions);
            latest_first(&mut project.non_interactive);
            project.non_interactive.truncate(NON_INTERACTIVE_LISTED);
            project
        })
        .collect();
    RegistryView {
        schema: REGISTRY_VIEW_SCHEMA,
        generated_at_ms: now_ms,
        registry: input.registry.clone(),
        ingestion: input.ingestion.clone(),
        subjects: subjects.into_iter().map(|(_, s)| s).collect(),
        unfiled,
        projects: known_projects(input.subjects, input.sessions),
        restore_notice: input
            .restore_notice
            .clone()
            .filter(|notice| now_ms - notice.restored_at_ms < RESTORE_NOTICE_MS),
        thresholds: Thresholds {
            restore_notice_ms: RESTORE_NOTICE_MS,
            excerpt_ttl_ms: EXCERPT_TTL_MS,
            non_interactive_listed: u32::try_from(NON_INTERACTIVE_LISTED).unwrap_or(u32::MAX),
            supersession_window_ms: SUPERSESSION_WINDOW_MS,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::parse_agents;
    use crate::board::{BoardInput, build_board};
    use crate::registry::types::{HookEnd, NON_INTERACTIVE_END_REASON, RegistryState, Sourced};

    const NOW: i64 = 1_793_102_400_000;

    fn ready() -> RegistryReport {
        RegistryReport {
            state: RegistryState::Ready,
            detail: None,
            exclude_from_sync: None,
        }
    }

    fn sourced<T>(value: T, src: Source, at_ms: i64) -> Option<Sourced<T>> {
        Some(Sourced { value, src, at_ms })
    }

    fn subject(id: i64, status: SubjectStatus, at_ms: i64) -> SubjectRecord {
        SubjectRecord {
            id,
            project: "api".to_owned(),
            name: format!("subject {id}"),
            status: Sourced {
                value: status,
                src: Source::Author,
                at_ms,
            },
            created_at_ms: at_ms,
            routine: false,
            merged_into: None,
        }
    }

    fn session(id: &str, project: &str, subject: Option<i64>, last_seen_ms: i64) -> SessionRecord {
        SessionRecord {
            session_id: id.to_owned(),
            project: project.to_owned(),
            first_seen_ms: last_seen_ms - 1_000,
            last_seen_ms,
            subject: subject.map(|id| Sourced {
                value: Some(id),
                src: Source::Author,
                at_ms: last_seen_ms,
            }),
            ..SessionRecord::default()
        }
    }

    fn non_interactive(id: &str, last_seen_ms: i64) -> SessionRecord {
        SessionRecord {
            ended: sourced(
                HookEnd {
                    reason: Some(NON_INTERACTIVE_END_REASON.to_owned()),
                    claude_pid: None,
                },
                Source::Hook,
                last_seen_ms,
            ),
            ..session(id, "api", None, last_seen_ms)
        }
    }

    fn view(subjects: &[SubjectRecord], sessions: &[SessionRecord]) -> RegistryView {
        build_view(
            &ViewInput {
                registry: ready(),
                ingestion: IngestionView::default(),
                subjects,
                sessions,
                excerpts: &[],
                board: None,
                restore_notice: None,
            },
            NOW,
        )
    }

    #[test]
    fn active_and_to_resume_come_first() {
        let subjects = [
            subject(1, SubjectStatus::ToResume, NOW - 10),
            subject(2, SubjectStatus::Active, NOW - 50),
            subject(3, SubjectStatus::Active, NOW - 20),
            subject(4, SubjectStatus::ToResume, NOW - 5),
        ];
        let sessions = [
            session("s2", "api", Some(2), NOW - 1),
            session("loose", "api", None, NOW),
        ];
        let shown = view(&subjects, &sessions);
        let order: Vec<i64> = shown.subjects.iter().map(|s| s.id).collect();
        assert_eq!(
            order,
            [2, 3, 4, 1],
            "active first, each by its latest activity"
        );
        assert_eq!(shown.subjects[0].sessions[0].session_id, "s2");
        assert_eq!(shown.schema, REGISTRY_VIEW_SCHEMA);
    }

    #[test]
    fn sessions_join_live_cards_by_session_id() {
        let board = build_board(
            &BoardInput {
                agents: Some(
                    parse_agents(
                        r#"[{"pid":7,"sessionId":"s-live","kind":"interactive","status":"waiting",
                             "waitingFor":"permission prompt","cwd":"C:/work/api","startedAt":1}]"#,
                    )
                    .unwrap(),
                ),
                ..Default::default()
            },
            NOW,
        );
        let subjects = [subject(1, SubjectStatus::Active, NOW)];
        let sessions = [
            session("s-live", "api", Some(1), NOW),
            session("s-old", "api", Some(1), NOW - 5_000),
        ];
        let shown = build_view(
            &ViewInput {
                board: Some(&board),
                ..ViewInput {
                    registry: ready(),
                    ingestion: IngestionView::default(),
                    subjects: &subjects,
                    sessions: &sessions,
                    excerpts: &[],
                    board: None,
                    restore_notice: None,
                }
            },
            NOW,
        );
        let listed = &shown.subjects[0].sessions;
        let card = listed[0].card.as_ref().expect("joined");
        assert_eq!(
            (card.column, card.waiting_for.as_deref()),
            (Column::NeedsYou, Some("permission prompt"))
        );
        assert_eq!(listed[1].session_id, "s-old");
        assert!(listed[1].card.is_none());
    }

    #[test]
    fn unfiled_are_grouped_by_project() {
        let sessions = [
            session("a1", "api", None, NOW - 3),
            session("w1", "web", None, NOW - 1),
            session("a2", "api", None, NOW - 2),
            session("filed", "api", Some(1), NOW),
        ];
        let shown = view(&[subject(1, SubjectStatus::Active, NOW)], &sessions);
        let grouped: Vec<(&str, Vec<&str>)> = shown
            .unfiled
            .iter()
            .map(|p| {
                (
                    p.project.as_str(),
                    p.sessions.iter().map(|s| s.session_id.as_str()).collect(),
                )
            })
            .collect();
        assert_eq!(grouped, [("api", vec!["a2", "a1"]), ("web", vec!["w1"])]);
    }

    #[test]
    fn non_interactive_runs_are_counted_and_bounded() {
        let mut sessions: Vec<SessionRecord> = (0..NON_INTERACTIVE_LISTED as i64 + 3)
            .map(|i| non_interactive(&format!("run{i:02}"), NOW - 100 + i))
            .collect();
        sessions.push(session("a1", "api", None, NOW));
        // Filed by rule at first sight, a run counts as unfiled (AD-47).
        let mut filed_run = non_interactive("filed-run", NOW - 200);
        filed_run.subject = sourced(Some(1), Source::Rules, NOW - 200);
        sessions.push(filed_run);
        let shown = view(&[subject(1, SubjectStatus::Active, NOW)], &sessions);
        let api = &shown.unfiled[0];
        assert_eq!(
            api.non_interactive_count as usize,
            NON_INTERACTIVE_LISTED + 4
        );
        assert_eq!(api.non_interactive.len(), NON_INTERACTIVE_LISTED);
        assert_eq!(api.non_interactive[0].session_id, "run12", "newest first");
        assert_eq!(api.sessions.len(), 1);
        assert!(shown.subjects[0].sessions.is_empty());
    }

    #[test]
    fn restore_notice_shows_for_7_days() {
        let notice = RestoreNotice {
            copy: "registry-2026-10-26.db".to_owned(),
            restored_at_ms: NOW,
        };
        let at = |now: i64| {
            build_view(
                &ViewInput {
                    registry: ready(),
                    ingestion: IngestionView::default(),
                    subjects: &[],
                    sessions: &[],
                    excerpts: &[],
                    board: None,
                    restore_notice: Some(notice.clone()),
                },
                now,
            )
            .restore_notice
        };
        assert_eq!(at(NOW + RESTORE_NOTICE_MS - 1), Some(notice.clone()));
        assert_eq!(at(NOW + RESTORE_NOTICE_MS), None);
    }

    #[test]
    fn archived_are_left_out() {
        let mut merged = subject(3, SubjectStatus::Archived, NOW);
        merged.merged_into = Some(1);
        let subjects = [
            subject(1, SubjectStatus::Active, NOW),
            subject(2, SubjectStatus::Archived, NOW),
            merged,
        ];
        let sessions = [session("s2", "api", Some(2), NOW)];
        let shown = view(&subjects, &sessions);
        assert_eq!(shown.subjects.iter().map(|s| s.id).collect::<Vec<_>>(), [1]);
        assert!(
            shown.unfiled.is_empty(),
            "an archived subject's session is filed"
        );
    }

    #[test]
    fn a_subject_shows_the_latest_excerpts_of_its_sessions() {
        let subjects = [subject(1, SubjectStatus::ToResume, NOW)];
        let sessions = [
            session("old", "api", Some(1), NOW - 10),
            session("new", "api", Some(1), NOW),
        ];
        let excerpts = [
            ExcerptRecord {
                session_id: "old".into(),
                prompt: Some("first".into()),
                prompt_at_ms: Some(NOW - 20),
                ..ExcerptRecord::default()
            },
            ExcerptRecord {
                session_id: "new".into(),
                answer_head: Some("Done.".into()),
                answer_at_ms: Some(NOW - 5),
                ..ExcerptRecord::default()
            },
        ];
        let shown = build_view(
            &ViewInput {
                registry: ready(),
                ingestion: IngestionView::default(),
                subjects: &subjects,
                sessions: &sessions,
                excerpts: &excerpts,
                board: None,
                restore_notice: None,
            },
            NOW,
        );
        let latest = shown.subjects[0].excerpts.as_ref().unwrap();
        assert_eq!(
            (latest.session_id.as_str(), latest.answer_head.as_deref()),
            ("new", Some("Done."))
        );
    }

    /// D-55, AD-54: `list_archived` lists the archived subjects, latest
    /// archived first, with their sessions and routine flag; a merged one
    /// is left out.
    #[test]
    fn archived_list_leaves_merged_out_latest_first() {
        let mut merged = subject(3, SubjectStatus::Archived, NOW);
        merged.merged_into = Some(1);
        let mut routine = subject(4, SubjectStatus::Archived, NOW - 5);
        routine.routine = true;
        let subjects = [
            subject(1, SubjectStatus::Active, NOW),
            subject(2, SubjectStatus::Archived, NOW - 10),
            merged,
            routine,
        ];
        let sessions = [
            session("s2", "api", Some(2), NOW - 20),
            session("s4", "api", Some(4), NOW - 30),
        ];
        let listed = archived_subjects(&subjects, &sessions, None);
        assert_eq!(listed.iter().map(|s| s.id).collect::<Vec<_>>(), [4, 2]);
        assert!(listed[0].routine && !listed[1].routine);
        assert_eq!(listed[1].sessions[0].session_id, "s2");
        assert!(listed.iter().all(|s| s.excerpts.is_none()));
        assert!(
            view(&subjects, &sessions)
                .subjects
                .iter()
                .all(|s| s.status != SubjectStatus::Archived)
        );
    }

    /// D-95: the projects a subject may be created in are those of every
    /// session and subject, archived ones included.
    #[test]
    fn projects_are_those_of_every_session_and_subject() {
        let mut archived = subject(2, SubjectStatus::Archived, NOW);
        archived.project = "old".to_owned();
        let subjects = [subject(1, SubjectStatus::Active, NOW), archived];
        let sessions = [
            session("w1", "web", None, NOW),
            session("a1", "api", Some(1), NOW),
        ];
        assert_eq!(view(&subjects, &sessions).projects, ["api", "old", "web"]);
        assert!(view(&[], &[]).projects.is_empty());
    }
}
