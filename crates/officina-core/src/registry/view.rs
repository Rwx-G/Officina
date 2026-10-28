//! The registry as the Subjects view and `officina subjects --json` show it
//! (FR-6, FR-7, AD-53, AD-54, D-93, D-96): built here from the registry's
//! records and a board, so the window and the command line print the same
//! thing, the rows that need a verdict included.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::board::{Badge, Board, Card, Column, SessionKindView};

use super::dry_run::{DryRunProgress, DryRunState, DryRunStop, Evaluation};
use super::filing::excerpt_time;
use super::model::{ModelFacts, PAUSE_AFTER_FAILED_CALLS, RowModel, row_model};
use super::proposal::Proposal;
use super::rates::{
    AUTO_COUNT_AFTER_MS, GateState, Gates, SourceRate, VerdictRecord, gates, rates,
};
use super::triage_call::{COUNT_WINDOW_MS, CallRecord, CallsView, count_calls};
use super::types::{
    ConfirmationRecord, Ending, ExcerptRecord, RegistryReport, RestoreNotice, RowKind,
    SessionRecord, Source, SubjectRecord, SubjectRef, SubjectStatus, TriageOutcome, TriageRefusal,
    Verdict,
};
use super::verdict::{Row, VerdictInput, rows};

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
    /// What needs a verdict, the latest first (FR-7, AD-49, D-76).
    pub triage: Vec<TriageRow>,
    /// How many rows need a verdict: the window's "N to sort" chip.
    pub to_sort: u32,
    /// The copy a damaged registry was restored from, for
    /// [`RESTORE_NOTICE_MS`] after the restore (D-93).
    pub restore_notice: Option<RestoreNotice>,
    /// The triage model's state and evidence (FR-10, AD-52).
    pub model: ModelView,
    /// Each proposal source's measured error (FR-11, AD-50).
    pub rates: Vec<SourceRate>,
    /// The three gates, with their reasons (AD-50).
    pub gates: Gates,
    /// The auto-archive switch, and why it last changed (FR-12, D-69).
    pub auto_archive: AutoArchiveView,
    /// The subjects auto-archived over the last [`AUTO_COUNT_AFTER_MS`],
    /// the latest first, which the author may reopen (D-70).
    pub recent_auto_archives: Vec<AutoArchived>,
    /// The subjects archived as routine, merged ones excepted: candidates
    /// for the v0.7 catalogue (FR-13, D-55).
    pub routine_candidates: Vec<RoutineCandidate>,
    pub thresholds: Thresholds,
}

/// The value of the [`SettingKey::AutoArchive`] setting (D-69): whether it
/// is on, and the gate state that turned it off, when rules did.
///
/// [`SettingKey::AutoArchive`]: super::types::SettingKey::AutoArchive
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoArchiveSetting {
    pub on: bool,
    #[serde(default)]
    pub reason: Option<GateState>,
}

/// The auto-archive switch as the window shows it (FR-12, AD-55).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AutoArchiveView {
    pub on: bool,
    /// Who changed it last: the author, or `rules` when its gate broke.
    pub src: Option<Source>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub at_ms: Option<i64>,
    /// The gate state that turned it off, when rules did.
    pub reason: Option<GateState>,
}

/// One subject auto-archive archived (D-70).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AutoArchived {
    pub subject: SubjectRef,
    pub project: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub archived_at_ms: i64,
    /// When the subject was reopened after it, by the author or a session
    /// alive again: a flip of the `state` source (D-77).
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub reopened_at_ms: Option<i64>,
    /// True while the subject is still archived: it can be reopened.
    pub archived: bool,
}

/// A subject archived as "routine, could have run alone" (FR-13).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RoutineCandidate {
    pub subject: SubjectRef,
    pub project: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub archived_at_ms: i64,
}

/// The triage model as the Triage panel's evidence shows it (FR-10, AD-50,
/// AD-52, D-67, D-69, D-81, D-92): whether it proposes, who switched it and
/// why, whether it is paused or cannot be called, the latest dry run, the
/// one running and how the last one ended, and the calls made.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ModelView {
    /// Whether the model is on (FR-8); its answers take the default's place
    /// while it is also available.
    pub enabled: bool,
    /// Who switched it last: the author, or `rules` when it turned itself
    /// off.
    pub src: Option<Source>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub at_ms: Option<i64>,
    /// The gate state that turned it off, when rules did (D-69).
    pub reason: Option<GateState>,
    /// Why it cannot be called: it cannot be turned on, and proposes
    /// nothing (FR-10 as amended).
    pub unavailable: Option<TriageRefusal>,
    /// Since when failed calls keep it paused (D-81): nothing is queued
    /// until it is turned on again or `claude` changes.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub paused_at_ms: Option<i64>,
    /// Rows queued for it now.
    pub pending: u32,
    pub latest_dry_run: Option<Evaluation>,
    /// The dry run running now, with its progress.
    pub dry_run: Option<DryRunProgress>,
    /// How the last dry run since the window started ended.
    pub last_stop: Option<DryRunStop>,
    pub calls: CallsView,
}

/// What the evidence is built from, beside the registry's records: the
/// `triage_call` rows of the count window, the latest `model_evaluation`,
/// and what the window's collector knows of dry runs.
#[derive(Debug, Clone, Default)]
pub struct EvidenceInput {
    pub calls: Vec<CallRecord>,
    pub latest_evaluation: Option<Evaluation>,
    pub dry_run: DryRunState,
    /// Every confirmation, as the rates read it.
    pub verdicts: Vec<VerdictRecord>,
    /// The auto-archive setting, with who set it and when.
    pub auto_archive: Option<(AutoArchiveSetting, Source, i64)>,
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

/// The numbers the page writes in its own words (AD-5), each from the
/// constant that owns it (debt audit H1): the page reads them here and
/// writes none itself. A number the page does not write is not shipped;
/// the gates carry their own counts and the reasons theirs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Thresholds {
    /// The window the call counts cover beside today: [`COUNT_WINDOW_MS`].
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub count_window_ms: i64,
    /// How far back the recent auto-archives are listed:
    /// [`AUTO_COUNT_AFTER_MS`].
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub auto_count_after_ms: i64,
    /// How many failed calls in a row pause the model:
    /// [`PAUSE_AFTER_FAILED_CALLS`].
    pub pause_after_failed_calls: u32,
}

impl Thresholds {
    /// The thresholds of this build.
    pub fn current() -> Thresholds {
        Thresholds {
            count_window_ms: COUNT_WINDOW_MS,
            auto_count_after_ms: AUTO_COUNT_AFTER_MS,
            pause_after_failed_calls: PAUSE_AFTER_FAILED_CALLS,
        }
    }
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

/// One row that needs a verdict (AD-49, D-76): a subject whose sessions all
/// ended, an empty subject, or an unfiled session, with its proposal, the
/// excerpts and end it rests on, and for an unfiled row the subject to file
/// it under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TriageRow {
    /// `subject:<id>` or `session:<id>`, stable while the row stands.
    pub key: String,
    pub kind: RowKind,
    pub project: String,
    /// The subject, for a subject or empty-subject row.
    pub subject: Option<SubjectRef>,
    /// The anchor, for a subject row; the session, for an unfiled one.
    pub session_id: Option<String>,
    pub session_name: Option<String>,
    pub proposal: Proposal,
    /// What confirming as shown applies: none for `unsure`, which waits
    /// for the author's choice (D-59).
    pub verdict: Option<Verdict>,
    /// The anchor's excerpts, when it still holds them.
    pub excerpts: Option<ExcerptView>,
    pub end: Option<EndView>,
    /// For an unfiled row: the subject proposed, when the project has
    /// exactly one open subject.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub proposed_subject: Option<i64>,
    /// For an unfiled row: the project's open subjects.
    pub choices: Vec<SubjectRef>,
    /// Queued for the triage model: the row keeps the default until the
    /// answer comes (AD-52).
    pub model_pending: bool,
    /// The model's reason for its proposal: untrusted text, rendered as
    /// text under a "model" label, never read by a rule (AD-40).
    pub model_reason: Option<String>,
    /// The model's last call on the row's inputs gave no answer: the
    /// default stays, with why (AD-52).
    pub model_call: Option<ModelCall>,
}

/// A model call that gave a row no answer (AD-51, D-87).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ModelCall {
    pub outcome: TriageOutcome,
    /// Why it was refused, for a `refused` call.
    pub refusal: Option<TriageRefusal>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub at_ms: i64,
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
    /// The confirmations, which answer rows until reopened (D-58).
    pub confirmations: &'a [ConfirmationRecord],
    /// The triage model: its setting, pause, availability, stored answers
    /// and last calls (AD-52).
    pub model: &'a ModelFacts,
    /// The board the live cards come from, when there is one.
    pub board: Option<&'a Board>,
    pub restore_notice: Option<RestoreNotice>,
    pub evidence: &'a EvidenceInput,
    /// `no_excerpts` is on: no excerpt is shown, whatever the registry
    /// still holds until the next pass clears it (D-85).
    pub no_excerpts: bool,
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

/// A row as the view shows it, its subject, session and excerpts joined,
/// with what it shows of the model.
fn triage_row(
    row: Row,
    model: RowModel,
    subjects: &BTreeMap<i64, &SubjectRecord>,
    sessions: &BTreeMap<&str, &SessionRecord>,
    excerpts: &BTreeMap<&str, &ExcerptRecord>,
) -> TriageRow {
    let (model_pending, model_reason, model_call) = match model {
        RowModel::None => (false, None, None),
        RowModel::Pending => (true, None, None),
        RowModel::Answered { reason } => (false, Some(reason), None),
        RowModel::NoAnswer {
            outcome,
            refusal,
            at_ms,
        } => (
            false,
            None,
            Some(ModelCall {
                outcome,
                refusal,
                at_ms,
            }),
        ),
    };
    let reference = |id: i64| {
        subjects.get(&id).map(|s| SubjectRef {
            id,
            name: s.name.clone(),
        })
    };
    let session = row.session_id.as_deref().and_then(|id| sessions.get(id));
    TriageRow {
        kind: row.kind,
        project: row.project,
        subject: row.subject.and_then(reference),
        session_name: session.and_then(|s| s.name.as_ref().map(|n| n.value.clone())),
        end: session.and_then(|s| end_view(s)),
        excerpts: row
            .session_id
            .as_deref()
            .and_then(|id| excerpts.get(id))
            .map(|e| excerpt_view(e)),
        verdict: row.proposal.definite(),
        proposal: row.proposal,
        proposed_subject: row.proposed_subject,
        choices: row.choices.into_iter().filter_map(reference).collect(),
        session_id: row.session_id,
        key: row.key,
        model_pending,
        model_reason,
        model_call,
    }
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
            confirmations: &[],
            model: &ModelFacts::default(),
            board: None,
            restore_notice: None,
            evidence: &EvidenceInput::default(),
            no_excerpts: false,
        },
        now_ms,
    )
}

/// The subjects auto-archived over the last [`AUTO_COUNT_AFTER_MS`] at
/// `now_ms`, the latest first (D-70).
fn recent_auto_archives(
    verdicts: &[VerdictRecord],
    subjects: &BTreeMap<i64, &SubjectRecord>,
    now_ms: i64,
) -> Vec<AutoArchived> {
    let mut recent: Vec<AutoArchived> = verdicts
        .iter()
        .filter(|v| v.auto && now_ms - v.confirmed_at_ms < AUTO_COUNT_AFTER_MS)
        .filter_map(|v| {
            let subject = subjects.get(&v.subject_id)?;
            Some(AutoArchived {
                subject: SubjectRef {
                    id: subject.id,
                    name: subject.name.clone(),
                },
                project: subject.project.clone(),
                archived_at_ms: v.confirmed_at_ms,
                reopened_at_ms: v.reopened_at_ms,
                archived: subject.status.value == SubjectStatus::Archived
                    && subject.merged_into.is_none(),
            })
        })
        .collect();
    recent.sort_by(|a, b| {
        b.archived_at_ms
            .cmp(&a.archived_at_ms)
            .then_with(|| a.subject.id.cmp(&b.subject.id))
    });
    recent
}

/// The subjects archived as routine, merged sources excepted (FR-13,
/// D-55), the latest archived first.
pub fn routine_candidates(subjects: &[SubjectRecord]) -> Vec<RoutineCandidate> {
    let mut found: Vec<RoutineCandidate> = subjects
        .iter()
        .filter(|s| s.routine && is_listed_archived(s))
        .map(|s| RoutineCandidate {
            subject: SubjectRef {
                id: s.id,
                name: s.name.clone(),
            },
            project: s.project.clone(),
            archived_at_ms: s.status.at_ms,
        })
        .collect();
    found.sort_by(|a, b| {
        b.archived_at_ms
            .cmp(&a.archived_at_ms)
            .then_with(|| a.subject.id.cmp(&b.subject.id))
    });
    found
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
///         subjects: &[], sessions: &[], excerpts: &[], confirmations: &[],
///         model: &Default::default(), board: None, restore_notice: None,
///         evidence: &Default::default(), no_excerpts: false,
///     },
///     1_000,
/// );
/// assert!(view.subjects.is_empty() && view.unfiled.is_empty());
/// ```
pub fn build_view(input: &ViewInput, now_ms: i64) -> RegistryView {
    let cards = cards_by_session(input.board);
    let shown_excerpts: &[ExcerptRecord] = if input.no_excerpts {
        &[]
    } else {
        input.excerpts
    };
    let excerpts: BTreeMap<&str, &ExcerptRecord> = shown_excerpts
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
    let by_id: BTreeMap<i64, &SubjectRecord> = input.subjects.iter().map(|s| (s.id, s)).collect();
    let by_session: BTreeMap<&str, &SessionRecord> = input
        .sessions
        .iter()
        .map(|s| (s.session_id.as_str(), s))
        .collect();
    let model_input = input.model.input(input.sessions, input.excerpts);
    let triage: Vec<TriageRow> = rows(&VerdictInput {
        subjects: input.subjects,
        sessions: input.sessions,
        confirmations: input.confirmations,
        model: &model_input,
    })
    .into_iter()
    .map(|row| {
        let model = row_model(&row, input.model, input.sessions, input.excerpts);
        triage_row(row, model, &by_id, &by_session, &excerpts)
    })
    .collect();
    let setting = input.model.setting.as_ref();
    let pending =
        u32::try_from(triage.iter().filter(|r| r.model_pending).count()).unwrap_or(u32::MAX);
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
        to_sort: u32::try_from(triage.len()).unwrap_or(u32::MAX),
        triage,
        restore_notice: input
            .restore_notice
            .clone()
            .filter(|notice| now_ms - notice.restored_at_ms < RESTORE_NOTICE_MS),
        model: ModelView {
            enabled: input.model.on(),
            src: setting.map(|s| s.src),
            at_ms: setting.map(|s| s.at_ms),
            reason: setting.and_then(|s| s.value.reason),
            unavailable: input.model.unavailable,
            paused_at_ms: input
                .model
                .pause
                .as_ref()
                .filter(|p| p.value.paused)
                .map(|p| p.at_ms),
            pending,
            latest_dry_run: input.evidence.latest_evaluation,
            dry_run: input.evidence.dry_run.running,
            last_stop: input.evidence.dry_run.last_stop,
            calls: count_calls(&input.evidence.calls, now_ms),
        },
        rates: rates(&input.evidence.verdicts, now_ms),
        gates: gates(
            &input.evidence.verdicts,
            input.evidence.latest_evaluation.as_ref(),
            input.model.self_disabled_at(),
            now_ms,
        ),
        auto_archive: input
            .evidence
            .auto_archive
            .map(|(setting, src, at_ms)| AutoArchiveView {
                on: setting.on,
                src: Some(src),
                at_ms: Some(at_ms),
                reason: setting.reason,
            })
            .unwrap_or_default(),
        recent_auto_archives: recent_auto_archives(&input.evidence.verdicts, &by_id, now_ms),
        routine_candidates: routine_candidates(input.subjects),
        thresholds: Thresholds::current(),
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
                confirmations: &[],
                model: &ModelFacts::default(),
                board: None,
                restore_notice: None,
                evidence: &EvidenceInput::default(),
                no_excerpts: false,
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
                    confirmations: &[],
                    model: &ModelFacts::default(),
                    board: None,
                    restore_notice: None,
                    evidence: &EvidenceInput::default(),
                    no_excerpts: false,
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
                    confirmations: &[],
                    model: &ModelFacts::default(),
                    board: None,
                    restore_notice: Some(notice.clone()),
                    evidence: &EvidenceInput::default(),
                    no_excerpts: false,
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
                confirmations: &[],
                model: &ModelFacts::default(),
                board: None,
                restore_notice: None,
                evidence: &EvidenceInput::default(),
                no_excerpts: false,
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

    /// A session of `project` that saw a prompt at `at - 100` and ended at
    /// `at` with a normal exit.
    fn ended(id: &str, project: &str, subject: Option<i64>, at: i64) -> SessionRecord {
        SessionRecord {
            listed: sourced(at - 200, Source::Agents, at - 200),
            last_prompt_at: sourced(at - 100, Source::Hook, at - 100),
            last_stop_at: sourced(at - 50, Source::Hook, at - 50),
            ended: sourced(
                HookEnd {
                    reason: Some("prompt_input_exit".to_owned()),
                    claude_pid: None,
                },
                Source::Hook,
                at,
            ),
            ..session(id, project, subject, at)
        }
    }

    /// FR-7, AD-49: the view carries the rows and their count, recomputed
    /// from what the registry holds: a session alive again takes its
    /// subject's row away, a confirmation answers it.
    #[test]
    fn rows_and_their_count_follow_the_registry() {
        let subjects = [subject(1, SubjectStatus::Active, NOW)];
        let mut sessions = vec![
            ended("filed", "api", Some(1), NOW - 1_000),
            ended("loose", "api", None, NOW - 500),
        ];
        let excerpts = [ExcerptRecord {
            session_id: "filed".into(),
            prompt: Some("tighten the alert rules".into()),
            prompt_at_ms: Some(NOW - 1_100),
            ..ExcerptRecord::default()
        }];
        let build = |sessions: &[SessionRecord], confirmations: &[ConfirmationRecord]| {
            build_view(
                &ViewInput {
                    registry: ready(),
                    ingestion: IngestionView::default(),
                    subjects: &subjects,
                    sessions,
                    excerpts: &excerpts,
                    confirmations,
                    model: &ModelFacts::default(),
                    board: None,
                    restore_notice: None,
                    evidence: &EvidenceInput::default(),
                    no_excerpts: false,
                },
                NOW,
            )
        };
        let shown = build(&sessions, &[]);
        let keys: Vec<&str> = shown.triage.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, ["session:loose", "subject:1"], "the latest first");
        assert_eq!(shown.to_sort, 2);
        let loose = &shown.triage[0];
        assert_eq!(loose.kind, RowKind::UnfiledSession);
        assert_eq!(loose.proposed_subject, Some(1));
        assert_eq!(
            loose.choices,
            [SubjectRef {
                id: 1,
                name: "subject 1".into()
            }]
        );
        let filed = &shown.triage[1];
        assert_eq!(filed.subject.as_ref().map(|s| s.id), Some(1));
        assert_eq!(filed.session_id.as_deref(), Some("filed"));
        assert_eq!(filed.verdict, Some(Verdict::ToResume));
        assert_eq!(
            filed.excerpts.as_ref().and_then(|e| e.prompt.as_deref()),
            Some("tighten the alert rules")
        );
        assert_eq!(
            filed.end.as_ref().map(|e| (e.how, e.at_ms)),
            Some((EndHow::SessionEnd, NOW - 1_000))
        );

        let answered = ConfirmationRecord {
            subject_id: 1,
            session_id: Some("filed".into()),
            confirmed_at_ms: NOW - 900,
            reopened_at_ms: None,
        };
        assert_eq!(build(&sessions, &[answered]).to_sort, 1);
        sessions[0].last_prompt_at = sourced(NOW - 10, Source::Hook, NOW - 10);
        let alive = build(&sessions, &[]);
        assert_eq!(alive.to_sort, 1, "a prompt after its end: alive again");
        assert_eq!(alive.triage[0].key, "session:loose");
        assert_eq!(unread_view(ready(), NOW).to_sort, 0);
    }

    /// FR-13, D-55: subjects archived with the routine box are candidates,
    /// the latest first; a merged source, though archived, is not, and
    /// neither is an open or a plainly archived subject.
    #[test]
    fn routine_candidates_exclude_merged_sources() {
        let routine = |id, at| SubjectRecord {
            routine: true,
            ..subject(id, SubjectStatus::Archived, at)
        };
        let mut merged = routine(3, NOW);
        merged.merged_into = Some(1);
        let open_routine = SubjectRecord {
            routine: true,
            ..subject(5, SubjectStatus::ToResume, NOW)
        };
        let subjects = [
            routine(1, NOW - 20),
            subject(2, SubjectStatus::Archived, NOW),
            merged,
            routine(4, NOW - 10),
            open_routine,
        ];
        let listed: Vec<i64> = view(&subjects, &[])
            .routine_candidates
            .iter()
            .map(|c| c.subject.id)
            .collect();
        assert_eq!(listed, [4, 1]);
    }
}
