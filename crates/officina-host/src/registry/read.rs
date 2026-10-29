//! Reading the registry (AD-38): the [`Reader`] every command line opens,
//! and the queries it shares with the window's writer, each turning rows
//! into core records.

use std::collections::BTreeMap;
use std::path::Path;

use officina_core::board::{Board, SessionSettlement};
use officina_core::dates::dated_file_day;
use officina_core::orders::proposal::{
    ProposalContent, ProposalView, ProposedSubject, ProposedSubjectView, TargetView, written_at,
};
use officina_core::registry::confirm::TriageContext;
use officina_core::registry::dry_run::{DryRunState, Evaluation, Tally};
use officina_core::registry::fold::pivot;
#[cfg(feature = "window-authority")]
use officina_core::registry::fold::{Mark, MarkQuery};
use officina_core::registry::model::{
    LastCall, ModelFacts, ModelPause, ModelSetting, StoredAnswer,
};
use officina_core::registry::proposal::{ModelInput, Proposal};
use officina_core::registry::rates::VerdictRecord;
use officina_core::registry::triage_call::{COUNT_WINDOW_MS, CallRecord};
use officina_core::registry::types::ModelVerdict;
use officina_core::registry::types::SessionFact;
use officina_core::registry::types::{
    ConfirmationRecord, EventFileRecord, ExcerptRecord, FactRow, FactValue, RegistryReport,
    RestoreNotice, SessionRecord, SessionRow, SettingKey, Settlement, Source, Sourced,
    SubjectRecord, SubjectRef, SubjectStatus, TriageOutcome, TriagePurpose, TriageRefusal, Verdict,
};
#[cfg(feature = "window-authority")]
use officina_core::registry::types::{HookEnd, HookStart};
use officina_core::registry::types::{
    LaunchOutcome, OrchestratorStartOutcome, ProposalKind, ProposalStatus, WorkOrderRecord,
    WorkOrderSource,
};
use officina_core::registry::verdict::{Row, VerdictInput, rows};
use officina_core::registry::view::{
    AutoArchiveSetting, DiscardedSession, EvidenceInput, IngestionView, RegistryView, SubjectView,
    ViewInput, archived_subjects, build_view,
};
use officina_core::untrusted::Untrusted;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OptionalExtension};

use super::store::{self, Location};

/// What every rule over the rows reads, loaded once from one connection
/// (quality audit H2): the view, a confirmation, auto-archive and the
/// model's queue each build their rows from this, so the proposal shown
/// and the proposal checked come from the same inputs (AD-49).
#[derive(Debug, Clone)]
pub struct VerdictSnapshot {
    pub subjects: Vec<SubjectRecord>,
    pub sessions: Vec<SessionRecord>,
    pub confirmations: Vec<ConfirmationRecord>,
    pub excerpts: Vec<ExcerptRecord>,
    /// The model's facts, with `unavailable` as the caller knows it: it
    /// decides whether stored answers serve, so a confirmation and the
    /// queue pass the window's, and a read that only needs the settings
    /// passes `None`.
    pub facts: ModelFacts,
    /// The model's part of the proposals, from `facts`.
    pub model: ModelInput,
}

impl VerdictSnapshot {
    /// Loads the snapshot from `conn`.
    pub fn load(
        conn: &Connection,
        unavailable: Option<TriageRefusal>,
    ) -> rusqlite::Result<VerdictSnapshot> {
        let subjects = subjects(conn)?;
        let sessions = session_records(conn)?;
        let confirmations = confirmations(conn)?;
        let excerpts = excerpts(conn)?;
        let facts = model_facts(conn, unavailable)?;
        let model = facts.input(&sessions, &excerpts);
        Ok(VerdictSnapshot {
            subjects,
            sessions,
            confirmations,
            excerpts,
            facts,
            model,
        })
    }

    /// The rows that need a verdict now (AD-49, D-76).
    pub fn rows(&self) -> Vec<Row> {
        rows(&VerdictInput {
            subjects: &self.subjects,
            sessions: &self.sessions,
            confirmations: &self.confirmations,
            model: &self.model,
        })
    }

    /// What a confirmation is planned against, with the projects the
    /// registry knows.
    pub fn context<'a>(&'a self, known_projects: &'a [String]) -> TriageContext<'a> {
        TriageContext {
            subjects: &self.subjects,
            sessions: &self.sessions,
            confirmations: &self.confirmations,
            excerpts: &self.excerpts,
            model: &self.model,
            known_projects,
        }
    }
}

/// One `setting` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingRow {
    pub key: SettingKey,
    /// The value as text: JSON for structured settings.
    pub value: String,
    pub src: Source,
    pub at_ms: i64,
}

/// A read-only connection to the registry (AD-38, AD-39), opened by
/// [`store::open_reader`]: every command line's, and the window's when the
/// database was written by a newer Officina. It has no writing method;
/// calling one does not compile:
///
/// ```compile_fail
/// fn write_through(reader: &mut officina_host::registry::read::Reader) {
///     let _ = reader.copy_if_due(0);
/// }
/// ```
///
/// while the same function reading compiles:
///
/// ```
/// fn read_through(reader: &mut officina_host::registry::read::Reader) {
///     let _ = reader.settings();
/// }
/// ```
#[derive(Debug)]
pub struct Reader {
    conn: Connection,
    location: Location,
}

impl Reader {
    pub(crate) fn new(conn: Connection, location: Location) -> Reader {
        Reader { conn, location }
    }

    /// The read-only connection, for the readers of other modules.
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Where the registry lies; a synced folder names what to exclude.
    pub fn location(&self) -> &Location {
        &self.location
    }

    /// The schema version the database is at.
    pub fn user_version(&self) -> rusqlite::Result<i64> {
        user_version(&self.conn)
    }

    /// The value a setting of this connection reads back, by its name.
    pub fn pragma(&self, name: &str) -> Option<String> {
        pragma_value(&self.conn, name)
    }

    /// `PRAGMA quick_check`'s findings, which only doctor asks a reader for
    /// (D-47).
    pub fn quick_check(&self) -> rusqlite::Result<Vec<String>> {
        store::quick_check(&self.conn)
    }

    /// Every `setting` row.
    pub fn settings(&self) -> rusqlite::Result<Vec<SettingRow>> {
        settings(&self.conn)
    }

    /// Which copy replaced a damaged registry, and when (D-93).
    pub fn restore_notice(&self) -> Option<RestoreNotice> {
        restore_notice(&self.conn)
    }

    /// Every `event_file` row, by name.
    pub fn event_files(&self) -> rusqlite::Result<Vec<EventFileRecord>> {
        event_files(&self.conn)
    }

    /// Every `session` row, by session id.
    pub fn sessions(&self) -> rusqlite::Result<Vec<SessionRow>> {
        sessions(&self.conn)
    }

    /// Every `session_excerpt` row, by session id.
    pub fn excerpts(&self) -> rusqlite::Result<Vec<ExcerptRecord>> {
        excerpts(&self.conn)
    }

    /// Every `session_fact` row, by session id and fact.
    pub fn facts(&self) -> rusqlite::Result<Vec<FactRow>> {
        facts(&self.conn)
    }

    /// Every session with its facts, by session id (AD-40).
    pub fn session_records(&self) -> rusqlite::Result<Vec<SessionRecord>> {
        session_records(&self.conn)
    }

    /// Every `subject` row, by id.
    pub fn subjects(&self) -> rusqlite::Result<Vec<SubjectRecord>> {
        subjects(&self.conn)
    }

    /// Every `confirmation` row, as far as deciding what needs a verdict
    /// reads it, by id.
    pub fn confirmations(&self) -> rusqlite::Result<Vec<ConfirmationRecord>> {
        confirmations(&self.conn)
    }

    /// The subject of each filed session, by session id: what a board card
    /// shows (AD-54).
    pub fn card_subjects(&self) -> rusqlite::Result<BTreeMap<String, SubjectRef>> {
        card_subjects(&self.conn)
    }

    /// What settles each ended session, by session id: what moves a board
    /// card out of To resume (story 4.15).
    pub fn card_settlements(&self) -> rusqlite::Result<BTreeMap<String, SessionSettlement>> {
        card_settlements(&self.conn)
    }

    /// The sessions the author discarded from the board, the latest first.
    pub fn discarded(&self) -> rusqlite::Result<Vec<DiscardedSession>> {
        discarded(&self.conn)
    }

    /// The registry as the Subjects view shows it (AD-53, AD-54), in `ctx`:
    /// a reader runs no dry run, so `ctx.dry_run` is the default.
    pub fn view(&self, ctx: ViewContext, now_ms: i64) -> rusqlite::Result<RegistryView> {
        view(&self.conn, ctx, now_ms)
    }

    /// The triage model's setting, pause, stored answers and last calls
    /// (AD-52), with why it cannot be called, when it cannot.
    pub fn model_facts(&self, unavailable: Option<TriageRefusal>) -> rusqlite::Result<ModelFacts> {
        model_facts(&self.conn, unavailable)
    }

    /// The `triage_call` rows started since `from_ms` (AD-52).
    pub fn triage_calls_since(&self, from_ms: i64) -> rusqlite::Result<Vec<CallRecord>> {
        triage_calls_since(&self.conn, from_ms)
    }

    /// The latest finished dry run, when there is one.
    pub fn latest_evaluation(&self) -> rusqlite::Result<Option<Evaluation>> {
        latest_evaluation(&self.conn)
    }

    /// The archived subjects the author may reopen, with the live cards of
    /// `board` (AD-54, D-55).
    pub fn archived(&self, board: Option<&Board>) -> rusqlite::Result<Vec<SubjectView>> {
        archived(&self.conn, board)
    }

    /// The latest work orders, newest first, with their binding and the
    /// bound session's record (AD-82, D-130).
    pub fn work_orders(&self) -> rusqlite::Result<Vec<WorkOrderRecord>> {
        work_orders(&self.conn)
    }

    /// When the author last read the briefing in the window (AD-83).
    pub fn briefing_read_at(&self) -> rusqlite::Result<Option<i64>> {
        briefing_read_at(&self.conn)
    }

    /// The event lines not ingested yet, from the files in `events_dir`
    /// against their offsets (AD-53): the lag behind a closed window.
    pub fn ingestion(&self, events_dir: &Path) -> rusqlite::Result<IngestionView> {
        ingestion_view(&self.conn, events_dir)
    }

    /// Every `orchestrator_start` row, newest first (AD-82, D-144).
    pub fn orchestrator_starts(&self) -> rusqlite::Result<Vec<OrchestratorStartRow>> {
        orchestrator_starts(&self.conn)
    }

    /// `officina mcp`'s briefing cursor (AD-83, D-144): the start of the
    /// newest orchestrator spawned before the newest one, the current
    /// session's; refused and failed starts are never one.
    pub fn previous_orchestrator_start(&self) -> rusqlite::Result<Option<i64>> {
        self.conn
            .prepare_cached(
                "SELECT started_at_ms FROM orchestrator_start WHERE outcome = ?1
                 ORDER BY started_at_ms DESC, id DESC LIMIT 1 OFFSET 1",
            )?
            .query_row([OrchestratorStartOutcome::Spawned.code()], |row| row.get(0))
            .optional()
    }

    /// Every proposal row, newest first (AD-91), each text untrusted.
    pub fn proposals(&self) -> rusqlite::Result<Vec<ProposalView>> {
        proposals(&self.conn)
    }
}

/// A proposal row's columns, the order [`proposal_row`] reads.
const PROPOSAL_COLUMNS: &str = "id, kind, status, invalid_reason, received_at_ms,      orchestrator_session_id, target_subject_id, target_session_id, template, project,      values_json, subject_choice_json, texts_json";

/// The proposal rows (AD-82, AD-91), newest first. The columns hold a
/// file's fields as ingestion stores them: the target in its two columns,
/// `values_json` the values' map, `subject_choice_json` the subject as the
/// file writes it, and `texts_json` an object of the kind's texts by field
/// name (a recap's three, a work order's `rationale`, an answer's `text`
/// and `rationale`, its session in `target_session_id`). A row whose fields
/// do not read back that way, or with no kind, shows without its content.
pub(crate) fn proposals(conn: &Connection) -> rusqlite::Result<Vec<ProposalView>> {
    conn.prepare_cached(&format!(
        "SELECT {PROPOSAL_COLUMNS} FROM proposal ORDER BY id DESC"
    ))?
    .query_map([], proposal_row)?
    .collect()
}

fn proposal_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProposalView> {
    let id: String = row.get(0)?;
    let kind = row
        .get::<_, Option<String>>(1)?
        .map(|code| {
            ProposalKind::from_code(&code).ok_or_else(|| {
                rusqlite::Error::InvalidColumnType(1, code, rusqlite::types::Type::Text)
            })
        })
        .transpose()?;
    let text = |index: usize| row.get::<_, Option<String>>(index);
    let texts: Option<BTreeMap<String, String>> = text(12)?
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok());
    let field = |name: &str| {
        texts
            .as_ref()
            .and_then(|t| t.get(name))
            .map(|t| Untrusted::new(t.clone()))
    };
    let content = match kind {
        Some(ProposalKind::Recap) => {
            let target = match (row.get::<_, Option<i64>>(6)?, text(7)?) {
                (Some(subject), None) => Some(TargetView::SubjectId(subject)),
                (None, Some(session)) => Some(TargetView::SessionId(Untrusted::new(session))),
                _ => None,
            };
            target.zip(field("done")).and_then(|(target, done)| {
                Some(ProposalContent::Recap {
                    target,
                    done,
                    blocking: field("blocking")?,
                    decision_expected: field("decision_expected")?,
                })
            })
        }
        Some(ProposalKind::WorkOrder) => {
            let values: Option<BTreeMap<String, String>> =
                text(10)?.and_then(|json| serde_json::from_str(&json).ok());
            let subject: Option<ProposedSubject> =
                text(11)?.and_then(|json| serde_json::from_str(&json).ok());
            match (text(8)?, text(9)?, values, subject, field("rationale")) {
                (Some(template), Some(project), Some(values), Some(subject), Some(rationale)) => {
                    Some(ProposalContent::WorkOrder {
                        template: Untrusted::new(template),
                        project: Untrusted::new(project),
                        values: Untrusted::new(values),
                        subject: ProposedSubjectView::of(&subject),
                        rationale,
                    })
                }
                _ => None,
            }
        }
        Some(ProposalKind::Answer) => text(7)?.zip(field("text")).and_then(|(session, text)| {
            Some(ProposalContent::Answer {
                session_id: Untrusted::new(session),
                text,
                rationale: field("rationale")?,
            })
        }),
        None => None,
    };
    let received_at_ms: i64 = row.get(4)?;
    Ok(ProposalView {
        written_at_ms: written_at(&id).unwrap_or(received_at_ms),
        id,
        kind,
        status: code_of(row, 2, ProposalStatus::from_code)?,
        invalid_reason: text(3)?.map(Untrusted::new),
        received_at_ms: Some(received_at_ms),
        orchestrator_session_id: text(5)?.map(Untrusted::new),
        content,
    })
}

/// When the author last left the Orchestrator view or closed the window
/// (AD-83): `setting.briefing_read_at_ms`, `None` before the first reading
/// or when this build cannot read its value.
pub(crate) fn briefing_read_at(conn: &Connection) -> rusqlite::Result<Option<i64>> {
    Ok(setting_of::<i64>(conn, SettingKey::BriefingReadAtMs)?.map(|read| read.value))
}

const SUBJECT_COLUMNS: &str =
    "id, project, name, status, status_src, status_at_ms, created_at_ms, routine, merged_into";

fn subject_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SubjectRecord> {
    Ok(SubjectRecord {
        id: row.get(0)?,
        project: row.get(1)?,
        name: row.get(2)?,
        // Both columns' `CHECK` list every value this build knows.
        status: Sourced {
            value: code_of(row, 3, SubjectStatus::from_code)?,
            src: code_of(row, 4, Source::from_code)?,
            at_ms: row.get(5)?,
        },
        created_at_ms: row.get(6)?,
        routine: bool_of(row, 7)?,
        merged_into: row.get(8)?,
    })
}

/// Every `subject` row, by id.
pub(crate) fn subjects(conn: &Connection) -> rusqlite::Result<Vec<SubjectRecord>> {
    conn.prepare_cached(&format!(
        "SELECT {SUBJECT_COLUMNS} FROM subject ORDER BY id"
    ))?
    .query_map([], subject_row)?
    .collect()
}

/// The subject of each filed session, by session id.
pub(crate) fn card_subjects(conn: &Connection) -> rusqlite::Result<BTreeMap<String, SubjectRef>> {
    conn.prepare_cached(
        "SELECT session.session_id, subject.id, subject.name
         FROM session JOIN subject ON subject.id = session.subject_id",
    )?
    .query_map([], |row| {
        Ok((
            row.get(0)?,
            SubjectRef {
                id: row.get(1)?,
                name: row.get(2)?,
            },
        ))
    })?
    .collect()
}

/// What settles each session the board may show in To resume, by session
/// id (story 4.15, D-536): the author's board verdict and its time, and
/// when the subject it is filed under was archived. One query, run with
/// [`card_subjects`] on every registry report, so it reads only the rows
/// that settle something.
pub(crate) fn card_settlements(
    conn: &Connection,
) -> rusqlite::Result<BTreeMap<String, SessionSettlement>> {
    conn.prepare_cached(
        "SELECT session.session_id, fact.value, fact.at_ms, subject.status_at_ms
         FROM session
         LEFT JOIN session_fact AS fact
             ON fact.session_id = session.session_id AND fact.fact = ?1
         LEFT JOIN subject ON subject.id = session.subject_id AND subject.status = ?2
         WHERE fact.session_id IS NOT NULL OR subject.id IS NOT NULL",
    )?
    .query_map(
        [SessionFact::Settled.code(), SubjectStatus::Archived.code()],
        |row| {
            let value: Option<String> = row.get(1)?;
            let at_ms: Option<i64> = row.get(2)?;
            Ok((
                row.get(0)?,
                SessionSettlement {
                    settled: value.as_deref().and_then(Settlement::from_code).zip(at_ms),
                    subject_archived_at_ms: row.get(3)?,
                },
            ))
        },
    )?
    .collect()
}

/// The sessions the author discarded from the board, the latest first
/// (story 4.15), with the name `claude agents` gave and the latest prompt
/// the registry still keeps.
pub(crate) fn discarded(conn: &Connection) -> rusqlite::Result<Vec<DiscardedSession>> {
    conn.prepare_cached(
        "SELECT fact.session_id, session.project, name.value, excerpt.prompt, fact.at_ms
         FROM session_fact AS fact
         JOIN session ON session.session_id = fact.session_id
         LEFT JOIN session_fact AS name
             ON name.session_id = fact.session_id AND name.fact = ?3
         LEFT JOIN session_excerpt AS excerpt ON excerpt.session_id = fact.session_id
         WHERE fact.fact = ?1 AND fact.value = ?2
         ORDER BY fact.at_ms DESC, fact.session_id",
    )?
    .query_map(
        [
            SessionFact::Settled.code(),
            Settlement::Discarded.code(),
            SessionFact::Name.code(),
        ],
        |row| {
            Ok(DiscardedSession {
                session_id: row.get(0)?,
                project: row.get(1)?,
                name: row.get::<_, Option<String>>(2)?.map(Untrusted::new),
                prompt: row.get::<_, Option<String>>(3)?.map(Untrusted::new),
                discarded_at_ms: row.get(4)?,
            })
        },
    )?
    .collect()
}

/// How far the registry lags behind the event files of `events_dir`, from
/// their sizes against the stored offsets (AD-53, D-84).
pub(crate) fn ingestion_view(
    conn: &Connection,
    events_dir: &Path,
) -> rusqlite::Result<IngestionView> {
    let rows: BTreeMap<String, EventFileRecord> = event_files(conn)?
        .into_iter()
        .map(|row| (row.name.clone(), row))
        .collect();
    let mut view = IngestionView {
        rewritten_files: u32::try_from(rows.values().filter(|row| row.rewrites > 0).count())
            .unwrap_or(u32::MAX),
        corrupt_lines: rows.values().map(|row| row.corrupt_lines).sum(),
        ..IngestionView::default()
    };
    for entry in std::fs::read_dir(events_dir)
        .into_iter()
        .flatten()
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(meta) = entry.metadata().ok().filter(|m| m.is_file()) else {
            continue;
        };
        if dated_file_day(&name).is_none() {
            continue;
        }
        let row = rows.get(&name);
        if row.is_some_and(|row| row.complete_at_ms.is_some()) {
            continue;
        }
        view.incomplete_files += 1;
        let offset = row.map_or(0, |row| row.ingested_bytes);
        // Shorter than its offset, the file was rewritten: the window reads
        // it again from its start (D-84).
        let read_from = if meta.len() < offset { 0 } else { offset };
        view.pending_bytes += meta.len() - read_from;
    }
    Ok(view)
}

/// What a view is built with besides the registry: its state, the board
/// whose live cards it joins, the event files whose lag it shows, and what
/// the window's collector knows of dry runs, of the model's availability
/// and of `no_excerpts`.
#[derive(Debug, Clone)]
pub struct ViewContext<'a> {
    pub report: RegistryReport,
    pub board: Option<&'a Board>,
    pub events_dir: &'a Path,
    pub dry_run: DryRunState,
    pub unavailable: Option<TriageRefusal>,
    /// No excerpt is shown while it is on (D-85).
    pub no_excerpts: bool,
}

/// The registry as the Subjects view shows it, in `ctx`.
pub(crate) fn view(
    conn: &Connection,
    ctx: ViewContext,
    now_ms: i64,
) -> rusqlite::Result<RegistryView> {
    // One read transaction, so the subjects, sessions and excerpts come from
    // the same commit while the window writes beside a command line (WAL);
    // it is never written in, and ends when dropped.
    let snapshot = conn.unchecked_transaction()?;
    let read = VerdictSnapshot::load(&snapshot, ctx.unavailable)?;
    let evidence = EvidenceInput {
        calls: triage_calls_since(&snapshot, now_ms - COUNT_WINDOW_MS)?,
        latest_evaluation: latest_evaluation(&snapshot)?,
        dry_run: ctx.dry_run,
        verdicts: verdict_records(&snapshot)?,
        auto_archive: auto_archive_setting(&snapshot)?,
    };
    Ok(build_view(
        &ViewInput {
            registry: ctx.report,
            ingestion: ingestion_view(&snapshot, ctx.events_dir)?,
            subjects: &read.subjects,
            sessions: &read.sessions,
            excerpts: &read.excerpts,
            confirmations: &read.confirmations,
            model: &read.facts,
            board: ctx.board,
            restore_notice: restore_notice(&snapshot),
            evidence: &evidence,
            no_excerpts: ctx.no_excerpts,
        },
        now_ms,
    ))
}

/// A setting's value, with who set it and when, when it is recorded and
/// reads as `T`: a value this build cannot read counts as absent.
pub(crate) fn setting_of<T: serde::de::DeserializeOwned>(
    conn: &Connection,
    key: SettingKey,
) -> rusqlite::Result<Option<Sourced<T>>> {
    let found = conn
        .prepare_cached("SELECT value, src, at_ms FROM setting WHERE key = ?1")?
        .query_row([key.code()], |row| {
            Ok((
                text_of(row.get_ref(0)?),
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .optional()?;
    Ok(found.and_then(|(value, src, at_ms)| {
        Some(Sourced {
            value: serde_json::from_str(&value).ok()?,
            src: Source::from_code(&src)?,
            at_ms,
        })
    }))
}

/// The triage model as the registry holds it (AD-52, D-66, D-69, D-87),
/// the same for the view and for the writer that checks a confirmation
/// against the proposals it shows (AD-49): its setting and pause, every
/// stored answer with the input hash of the call that gave it, and the
/// last proposal call of each case.
pub(crate) fn model_facts(
    conn: &Connection,
    unavailable: Option<TriageRefusal>,
) -> rusqlite::Result<ModelFacts> {
    let answers = conn
        .prepare_cached(
            "SELECT m.session_id, m.verdict, m.reason, m.call_id, m.at_ms, t.input_hash
             FROM model_answer m LEFT JOIN triage_call t ON t.id = m.call_id
             ORDER BY m.session_id",
        )?
        .query_map([], |row| {
            Ok(StoredAnswer {
                session_id: row.get(0)?,
                verdict: code_of(row, 1, ModelVerdict::from_code)?,
                reason: row.get(2)?,
                call_id: row.get(3)?,
                at_ms: row.get(4)?,
                input_hash: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    // SQLite takes a bare column's value from the row `max` picked, so each
    // case comes with its latest call.
    let last_calls = conn
        .prepare_cached(
            "SELECT case_key, outcome, refusal, input_hash, started_at_ms, max(id)
             FROM triage_call WHERE purpose = ?1 AND case_key IS NOT NULL GROUP BY case_key",
        )?
        .query_map([TriagePurpose::Proposal.code()], |row| {
            let refusal: Option<String> = row.get(2)?;
            Ok((
                row.get::<_, String>(0)?,
                LastCall {
                    outcome: code_of(row, 1, TriageOutcome::from_code)?,
                    refusal: refusal.as_deref().and_then(TriageRefusal::from_code),
                    input_hash: row.get(3)?,
                    started_at_ms: row.get(4)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
    Ok(ModelFacts {
        setting: setting_of::<ModelSetting>(conn, SettingKey::TriageModel)?,
        pause: setting_of::<ModelPause>(conn, SettingKey::TriageModelPause)?,
        unavailable,
        answers,
        last_calls,
    })
}

/// The archived subjects the author may reopen, from one read
/// transaction, like the view.
pub(crate) fn archived(
    conn: &Connection,
    board: Option<&Board>,
) -> rusqlite::Result<Vec<SubjectView>> {
    let snapshot = conn.unchecked_transaction()?;
    let subjects = subjects(&snapshot)?;
    let sessions = session_records(&snapshot)?;
    Ok(archived_subjects(&subjects, &sessions, board))
}

/// `subject_id`, `subject_src` and `subject_at_ms` as one value: `None`
/// when the session was never filed.
fn filing(
    subject_id: Option<i64>,
    src: Option<String>,
    at_ms: Option<i64>,
) -> Option<Sourced<Option<i64>>> {
    Some(Sourced {
        value: subject_id,
        src: Source::from_code(&src?)?,
        at_ms: at_ms?,
    })
}

const SESSION_COLUMNS: &str =
    "session_id, project, first_seen_ms, last_seen_ms, subject_id, subject_src, subject_at_ms";

fn session_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        session_id: row.get(0)?,
        project: row.get(1)?,
        first_seen_ms: row.get(2)?,
        last_seen_ms: row.get(3)?,
        subject: filing(row.get(4)?, row.get(5)?, row.get(6)?),
    })
}

fn fact_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FactRow> {
    let value = match row.get_ref(2)? {
        ValueRef::Integer(i) => FactValue::Integer(i),
        other => FactValue::Text(text_of(other)),
    };
    Ok(FactRow {
        session_id: row.get(0)?,
        fact: row.get(1)?,
        value,
        // The column's `CHECK` lists every source this build knows.
        src: code_of(row, 3, Source::from_code)?,
        at_ms: row.get(4)?,
    })
}

const FACT_COLUMNS: &str = "session_id, fact, value, src, at_ms";

/// Every `session_fact` row, by session id and fact.
pub(crate) fn facts(conn: &Connection) -> rusqlite::Result<Vec<FactRow>> {
    conn.prepare_cached(&format!(
        "SELECT {FACT_COLUMNS} FROM session_fact ORDER BY session_id, fact"
    ))?
    .query_map([], fact_row)?
    .collect()
}

/// Every session with its facts, by session id.
pub(crate) fn session_records(conn: &Connection) -> rusqlite::Result<Vec<SessionRecord>> {
    Ok(pivot(&sessions(conn)?, &facts(conn)?))
}

/// The session `session_id` with its facts, when the registry holds it.
pub(crate) fn session_record(
    conn: &Connection,
    session_id: &str,
) -> rusqlite::Result<Option<SessionRecord>> {
    let Some(row) = conn
        .prepare_cached(&format!(
            "SELECT {SESSION_COLUMNS} FROM session WHERE session_id = ?1"
        ))?
        .query_row([session_id], session_row)
        .optional()?
    else {
        return Ok(None);
    };
    let facts: Vec<FactRow> = conn
        .prepare_cached(&format!(
            "SELECT {FACT_COLUMNS} FROM session_fact WHERE session_id = ?1"
        ))?
        .query_map([session_id], fact_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(pivot(&[row], &facts).pop())
}

#[cfg(feature = "window-authority")]
/// The supersession marks (D-52) the registry holds within `query`'s range:
/// the host narrows by time, the core decides which link.
pub(crate) fn marks(conn: &Connection, query: MarkQuery) -> rusqlite::Result<Vec<Mark>> {
    let (fact, from_ms, to_ms) = match query {
        MarkQuery::Starts { from_ms, to_ms } => (SessionFact::Started, from_ms, to_ms),
        MarkQuery::Ends { from_ms, to_ms } => (SessionFact::Ended, from_ms, to_ms),
    };
    let rows: Vec<FactRow> = conn
        .prepare_cached(&format!(
            "SELECT {FACT_COLUMNS} FROM session_fact WHERE fact = ?1 AND at_ms BETWEEN ?2 AND ?3"
        ))?
        .query_map(rusqlite::params![fact.code(), from_ms, to_ms], fact_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let FactValue::Text(text) = &row.value else {
                return None;
            };
            match fact {
                SessionFact::Started => Mark::of_start(
                    &row.session_id,
                    &Sourced {
                        value: serde_json::from_str::<HookStart>(text).ok()?,
                        src: row.src,
                        at_ms: row.at_ms,
                    },
                ),
                _ => Mark::of_end(
                    &row.session_id,
                    &Sourced {
                        value: serde_json::from_str::<HookEnd>(text).ok()?,
                        src: row.src,
                        at_ms: row.at_ms,
                    },
                ),
            }
        })
        .collect())
}

/// Every `event_file` row, by name.
pub(crate) fn event_files(conn: &Connection) -> rusqlite::Result<Vec<EventFileRecord>> {
    conn.prepare_cached(
        "SELECT name, ingested_bytes, complete_at_ms, corrupt_lines, rewrites FROM event_file ORDER BY name",
    )?
    .query_map([], |row| {
        Ok(EventFileRecord {
            name: row.get(0)?,
            ingested_bytes: u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
            complete_at_ms: row.get(2)?,
            corrupt_lines: row.get(3)?,
            rewrites: row.get(4)?,
        })
    })?
    .collect()
}

/// Every `session` row, by session id.
pub(crate) fn sessions(conn: &Connection) -> rusqlite::Result<Vec<SessionRow>> {
    conn.prepare_cached(&format!(
        "SELECT {SESSION_COLUMNS} FROM session ORDER BY session_id"
    ))?
    .query_map([], session_row)?
    .collect()
}

/// Every `session_excerpt` row, by session id.
pub(crate) fn excerpts(conn: &Connection) -> rusqlite::Result<Vec<ExcerptRecord>> {
    conn.prepare_cached(
        "SELECT session_id, prompt, prompt_at_ms, answer_head, answer_tail, answer_at_ms
         FROM session_excerpt ORDER BY session_id",
    )?
    .query_map([], |row| {
        Ok(ExcerptRecord {
            session_id: row.get(0)?,
            prompt: row.get(1)?,
            prompt_at_ms: row.get(2)?,
            answer_head: row.get(3)?,
            answer_tail: row.get(4)?,
            answer_at_ms: row.get(5)?,
        })
    })?
    .collect()
}

/// Every `confirmation` row, as far as deciding what needs a verdict reads
/// it (AD-49), by id.
pub(crate) fn confirmations(conn: &Connection) -> rusqlite::Result<Vec<ConfirmationRecord>> {
    conn.prepare_cached(
        "SELECT subject_id, session_id, confirmed_at_ms, reopened_at_ms FROM confirmation ORDER BY id",
    )?
    .query_map([], |row| {
        Ok(ConfirmationRecord {
            subject_id: row.get(0)?,
            session_id: row.get(1)?,
            confirmed_at_ms: row.get(2)?,
            reopened_at_ms: row.get(3)?,
        })
    })?
    .collect()
}

/// `PRAGMA user_version`.
pub(crate) fn user_version(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
}

/// What `PRAGMA <name>` reads back, as text; `None` for a name that is not
/// a plain pragma name, or one SQLite does not answer.
pub(crate) fn pragma_value(conn: &Connection, name: &str) -> Option<String> {
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
        return None;
    }
    conn.query_row(&format!("PRAGMA {name}"), [], |row| {
        Ok(text_of(row.get_ref(0)?))
    })
    .ok()
}

fn text_of(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Null => String::new(),
        ValueRef::Integer(i) => i.to_string(),
        ValueRef::Real(r) => r.to_string(),
        ValueRef::Text(t) | ValueRef::Blob(t) => String::from_utf8_lossy(t).into_owned(),
    }
}

/// Every `setting` row whose key and source this build knows.
pub(crate) fn settings(conn: &Connection) -> rusqlite::Result<Vec<SettingRow>> {
    let mut statement =
        conn.prepare_cached("SELECT key, value, src, at_ms FROM setting ORDER BY key")?;
    let rows = statement.query_map([], |row| {
        let key: String = row.get(0)?;
        let src: String = row.get(2)?;
        Ok((key, text_of(row.get_ref(1)?), src, row.get::<_, i64>(3)?))
    })?;
    let mut found = Vec::new();
    for row in rows {
        let (key, value, src, at_ms) = row?;
        if let (Some(key), Some(src)) = (SettingKey::from_code(&key), Source::from_code(&src)) {
            found.push(SettingRow {
                key,
                value,
                src,
                at_ms,
            });
        }
    }
    Ok(found)
}

/// The restore notice, when one is recorded and readable.
pub(crate) fn restore_notice(conn: &Connection) -> Option<RestoreNotice> {
    settings(conn)
        .ok()?
        .into_iter()
        .find(|row| row.key == SettingKey::RestoreNotice)
        .and_then(|row| serde_json::from_str(&row.value).ok())
}

#[cfg(feature = "window-authority")]
/// The `active` subjects of `project`: those a new session of it may join
/// (AD-48).
pub(crate) fn active_subjects_of(
    conn: &Connection,
    project: &str,
) -> rusqlite::Result<Vec<SubjectRecord>> {
    conn.prepare_cached(&format!(
        "SELECT {SUBJECT_COLUMNS} FROM subject WHERE project = ?1 AND status = ?2 ORDER BY id"
    ))?
    .query_map([project, SubjectStatus::Active.code()], subject_row)?
    .collect()
}

#[cfg(feature = "window-authority")]
/// The sessions whose `subject_id` is `subject`.
pub(crate) fn session_ids_of_subject(
    conn: &Connection,
    subject: i64,
) -> rusqlite::Result<Vec<String>> {
    conn.prepare_cached("SELECT session_id FROM session WHERE subject_id = ?1 ORDER BY session_id")?
        .query_map([subject], |row| row.get(0))?
        .collect()
}

#[cfg(feature = "window-authority")]
/// The `subject_id` of `session_id`, when it is filed.
pub(crate) fn subject_id_of(conn: &Connection, session_id: &str) -> rusqlite::Result<Option<i64>> {
    Ok(conn
        .prepare_cached("SELECT subject_id FROM session WHERE session_id = ?1")?
        .query_row([session_id], |row| row.get(0))
        .optional()?
        .flatten())
}

#[cfg(feature = "window-authority")]
/// The `session_excerpt` row of `session_id`, when it has one.
pub(crate) fn excerpt_of(
    conn: &Connection,
    session_id: &str,
) -> rusqlite::Result<Option<ExcerptRecord>> {
    conn.prepare_cached(
        "SELECT session_id, prompt, prompt_at_ms, answer_head, answer_tail, answer_at_ms
         FROM session_excerpt WHERE session_id = ?1",
    )?
    .query_row([session_id], |row| {
        Ok(ExcerptRecord {
            session_id: row.get(0)?,
            prompt: row.get(1)?,
            prompt_at_ms: row.get(2)?,
            answer_head: row.get(3)?,
            answer_tail: row.get(4)?,
            answer_at_ms: row.get(5)?,
        })
    })
    .optional()
}

#[cfg(feature = "window-authority")]
/// The projects the registry knows: those of its sessions and subjects
/// (D-95).
pub(crate) fn known_projects(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    conn.prepare_cached("SELECT project FROM session UNION SELECT project FROM subject ORDER BY 1")?
        .query_map([], |row| row.get(0))?
        .collect()
}

/// How many work orders [`work_orders`] reads back (AD-82).
pub const WORK_ORDERS_READ: u32 = 50;

const WORK_ORDER_COLUMNS: &str = "id, seq, template, project, cwd, values_json, argv_json,
    prompt_sha256, catalogue_hash, name, subject_id, new_subject_name, source, proposal_id,
    launched_at_ms, outcome, exit_code, error, printed, bg_id, session_id, bound_at_ms";

/// A JSON column written by the window, read back; text that does not
/// parse is a type error, as for a code no variant parses.
fn json_of<T: serde::de::DeserializeOwned>(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<T> {
    let text: String = row.get(index)?;
    serde_json::from_str(&text).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn work_order_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkOrderRecord> {
    Ok(WorkOrderRecord {
        id: row.get(0)?,
        seq: row.get(1)?,
        template: row.get(2)?,
        project: row.get(3)?,
        cwd: row.get(4)?,
        values: json_of(row, 5)?,
        argv: json_of(row, 6)?,
        prompt_sha256: row.get(7)?,
        catalogue_hash: row.get(8)?,
        name: row.get(9)?,
        subject_id: row.get(10)?,
        new_subject_name: row.get(11)?,
        // Both columns' `CHECK` list every value this build knows.
        source: code_of(row, 12, WorkOrderSource::from_code)?,
        proposal_id: row.get(13)?,
        launched_at_ms: row.get(14)?,
        outcome: code_of(row, 15, LaunchOutcome::from_code)?,
        exit_code: row.get(16)?,
        error: row.get::<_, Option<String>>(17)?.map(Untrusted::new),
        printed: row.get::<_, Option<String>>(18)?.map(Untrusted::new),
        bg_id: row.get(19)?,
        session_id: row.get(20)?,
        bound_at_ms: row.get(21)?,
        session: None,
    })
}

/// The latest [`WORK_ORDERS_READ`] work orders, newest first, each with
/// the record of the session it bound to, from one read transaction
/// (AD-82); `printed` and `error` come back untrusted (D-130).
/// One `orchestrator_start` row (AD-82, D-144): when, how it ended, the
/// pid it kept with its creation time, and why it was refused or failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestratorStartRow {
    pub id: i64,
    pub started_at_ms: i64,
    pub outcome: OrchestratorStartOutcome,
    pub pid: Option<u32>,
    pub pid_created_at_ms: Option<i64>,
    pub error: Option<String>,
}

/// Every `orchestrator_start` row, newest first; retention keeps them 90
/// days (D-134), a few a day at most.
pub(crate) fn orchestrator_starts(
    conn: &Connection,
) -> rusqlite::Result<Vec<OrchestratorStartRow>> {
    conn.prepare_cached(
        "SELECT id, started_at_ms, outcome, pid, pid_created_at_ms, error FROM orchestrator_start
         ORDER BY started_at_ms DESC, id DESC",
    )?
    .query_map([], |row| {
        Ok(OrchestratorStartRow {
            id: row.get(0)?,
            started_at_ms: row.get(1)?,
            // The column's `CHECK` lists every value this build knows.
            outcome: code_of(row, 2, OrchestratorStartOutcome::from_code)?,
            pid: row.get(3)?,
            pid_created_at_ms: row.get(4)?,
            error: row.get(5)?,
        })
    })?
    .collect()
}

pub(crate) fn work_orders(conn: &Connection) -> rusqlite::Result<Vec<WorkOrderRecord>> {
    let snapshot = conn.unchecked_transaction()?;
    let mut orders: Vec<WorkOrderRecord> = snapshot
        .prepare_cached(&format!(
            "SELECT {WORK_ORDER_COLUMNS} FROM work_order
             ORDER BY launched_at_ms DESC, seq DESC, id LIMIT ?1"
        ))?
        .query_map([WORK_ORDERS_READ], work_order_row)?
        .collect::<rusqlite::Result<_>>()?;
    for order in &mut orders {
        if let Some(session_id) = &order.session_id {
            order.session = session_record(&snapshot, session_id)?;
        }
    }
    Ok(orders)
}

/// A 0 or 1 column (its `CHECK` allows nothing else), read back.
fn bool_of(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<bool> {
    Ok(row.get::<_, i64>(index)? == 1)
}

/// A text column holding a closed enum's code, read back; an unknown code
/// is a type error, as for the columns whose `CHECK` lists every value.
fn code_of<T>(
    row: &rusqlite::Row<'_>,
    index: usize,
    from_code: fn(&str) -> Option<T>,
) -> rusqlite::Result<T> {
    let text: String = row.get(index)?;
    from_code(&text).ok_or_else(|| {
        rusqlite::Error::InvalidColumnType(index, text.clone(), rusqlite::types::Type::Text)
    })
}

/// The `triage_call` rows started at or after `from_ms`, oldest first
/// (AD-52): the budget and the counts read them.
pub(crate) fn triage_calls_since(
    conn: &Connection,
    from_ms: i64,
) -> rusqlite::Result<Vec<CallRecord>> {
    conn.prepare_cached(
        "SELECT purpose, started_at_ms, outcome, refusal, input_tokens, output_tokens, cost_microusd
         FROM triage_call WHERE started_at_ms >= ?1 ORDER BY started_at_ms, id",
    )?
    .query_map([from_ms], |row| {
        let refusal: Option<String> = row.get(3)?;
        Ok(CallRecord {
            purpose: code_of(row, 0, TriagePurpose::from_code)?,
            started_at_ms: row.get(1)?,
            outcome: code_of(row, 2, TriageOutcome::from_code)?,
            refusal: refusal.as_deref().and_then(TriageRefusal::from_code),
            input_tokens: row.get(4)?,
            output_tokens: row.get(5)?,
            cost_microusd: row.get(6)?,
        })
    })?
    .collect()
}

/// The latest `model_evaluation` row: the last finished dry run.
pub(crate) fn latest_evaluation(conn: &Connection) -> rusqlite::Result<Option<Evaluation>> {
    conn.prepare_cached(
        "SELECT at_ms, cases, model_correct, default_correct, model_false_finished, unanswered,
             input_tokens, output_tokens, cost_microusd
         FROM model_evaluation ORDER BY at_ms DESC, id DESC LIMIT 1",
    )?
    .query_map([], |row| {
        Ok(Evaluation {
            at_ms: row.get(0)?,
            tally: Tally {
                cases: row.get(1)?,
                model_correct: row.get(2)?,
                default_correct: row.get(3)?,
                model_false_finished: row.get(4)?,
                unanswered: row.get(5)?,
                input_tokens: row.get(6)?,
                output_tokens: row.get(7)?,
                cost_microusd: row.get(8)?,
            },
        })
    })?
    .next()
    .transpose()
}

#[cfg(feature = "window-authority")]
/// Every confirmation as the dry run reads it (AD-52, D-85): the proposal
/// it answered, the author's verdict, and its input while it holds one.
pub(crate) fn evaluable_confirmations(
    conn: &Connection,
) -> rusqlite::Result<Vec<officina_core::registry::dry_run::EvaluableConfirmation>> {
    use officina_core::registry::confirm::ConfirmationInput;
    use officina_core::registry::dry_run::EvaluableConfirmation;
    use officina_core::registry::types::{ProposalSource, ReasonCode};
    conn.prepare_cached(
        "SELECT c.id, c.confirmed_at_ms, c.proposed_verdict, c.proposed_source, c.proposed_reason,
             c.call_id, c.verdict, i.confirmation_id, i.prompt, i.answer_head, i.answer_tail,
             i.end_reason, i.cut_mid_turn, i.expires_at_ms
         FROM confirmation c LEFT JOIN confirmation_input i ON i.confirmation_id = c.id
         ORDER BY c.id",
    )?
    .query_map([], |row| {
        let held: Option<i64> = row.get(7)?;
        let input = match held {
            Some(_) => Some(ConfirmationInput {
                prompt: row.get(8)?,
                answer_head: row.get(9)?,
                answer_tail: row.get(10)?,
                end_reason: row.get(11)?,
                cut_mid_turn: bool_of(row, 12)?,
                expires_at_ms: row.get(13)?,
            }),
            None => None,
        };
        Ok(EvaluableConfirmation {
            id: row.get(0)?,
            confirmed_at_ms: row.get(1)?,
            proposal: Proposal {
                verdict: code_of(row, 2, ModelVerdict::from_code)?,
                source: code_of(row, 3, ProposalSource::from_code)?,
                reason: code_of(row, 4, ReasonCode::from_code)?,
                call_id: row.get(5)?,
            },
            verdict: code_of(row, 6, Verdict::from_code)?,
            input,
        })
    })?
    .collect()
}

#[cfg(feature = "window-authority")]
/// The latest call about `case_key`, when there is one (D-87).
pub(crate) fn last_call_for(
    conn: &Connection,
    case_key: &str,
) -> rusqlite::Result<Option<LastCall>> {
    conn.prepare_cached(
        "SELECT outcome, refusal, input_hash, started_at_ms FROM triage_call WHERE case_key = ?1
         ORDER BY started_at_ms DESC, id DESC LIMIT 1",
    )?
    .query_map([case_key], |row| {
        let refusal: Option<String> = row.get(1)?;
        Ok(LastCall {
            outcome: code_of(row, 0, TriageOutcome::from_code)?,
            refusal: refusal.as_deref().and_then(TriageRefusal::from_code),
            input_hash: row.get(2)?,
            started_at_ms: row.get(3)?,
        })
    })?
    .next()
    .transpose()
}

/// Every confirmation as the rates read it (AD-50), by id.
pub(crate) fn verdict_records(conn: &Connection) -> rusqlite::Result<Vec<VerdictRecord>> {
    use officina_core::registry::types::{ProposalSource, ReasonCode};
    conn.prepare_cached(
        "SELECT id, subject_id, proposed_verdict, proposed_source, proposed_reason, call_id,
             verdict, auto, confirmed_at_ms, reopened_at_ms
         FROM confirmation ORDER BY id",
    )?
    .query_map([], |row| {
        Ok(VerdictRecord {
            id: row.get(0)?,
            subject_id: row.get(1)?,
            proposal: Proposal {
                verdict: code_of(row, 2, ModelVerdict::from_code)?,
                source: code_of(row, 3, ProposalSource::from_code)?,
                reason: code_of(row, 4, ReasonCode::from_code)?,
                call_id: row.get(5)?,
            },
            verdict: code_of(row, 6, Verdict::from_code)?,
            auto: bool_of(row, 7)?,
            confirmed_at_ms: row.get(8)?,
            reopened_at_ms: row.get(9)?,
        })
    })?
    .collect()
}

/// The auto-archive setting, with who set it and when, when one is
/// recorded in a form this build reads (D-69); a failed read is an error,
/// never "off" (quality audit M2).
pub(crate) fn auto_archive_setting(
    conn: &Connection,
) -> rusqlite::Result<Option<(AutoArchiveSetting, Source, i64)>> {
    Ok(
        setting_of::<AutoArchiveSetting>(conn, SettingKey::AutoArchive)?
            .map(|setting| (setting.value, setting.src, setting.at_ms)),
    )
}
