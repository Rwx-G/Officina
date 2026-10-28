//! Reading the registry (AD-38): the [`Reader`] every command line opens,
//! and the queries it shares with the window's writer, each turning rows
//! into core records.

use std::collections::BTreeMap;
use std::path::Path;

use officina_core::board::Board;
use officina_core::dates::dated_file_day;
use officina_core::registry::dry_run::{DryRunState, Evaluation, Tally};
use officina_core::registry::fold::pivot;
#[cfg(feature = "window-authority")]
use officina_core::registry::fold::{Mark, MarkQuery};
use officina_core::registry::proposal::{ModelInput, Proposal};
use officina_core::registry::rates::VerdictRecord;
use officina_core::registry::triage_call::{COUNT_WINDOW_MS, CallRecord};
use officina_core::registry::types::{
    ConfirmationRecord, EventFileRecord, ExcerptRecord, FactRow, FactValue, RegistryReport,
    RestoreNotice, SessionRecord, SessionRow, SettingKey, Source, Sourced, SubjectRecord,
    SubjectRef, SubjectStatus, TriageOutcome, TriagePurpose, TriageRefusal, Verdict,
};
#[cfg(feature = "window-authority")]
use officina_core::registry::types::{HookEnd, HookStart, SessionFact};
use officina_core::registry::view::{
    AutoArchiveSetting, EvidenceInput, IngestionView, RegistryView, SubjectView, ViewInput,
    archived_subjects, build_view,
};
use rusqlite::Connection;
#[cfg(feature = "window-authority")]
use rusqlite::OptionalExtension;
use rusqlite::types::ValueRef;

use super::store::{self, Location};

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

    /// The registry as the Subjects view shows it (AD-53, AD-54), with the
    /// lag behind the event files of `events_dir` and the live cards of
    /// `board`.
    pub fn view(
        &self,
        report: RegistryReport,
        board: Option<&Board>,
        events_dir: &Path,
        now_ms: i64,
    ) -> rusqlite::Result<RegistryView> {
        view(
            &self.conn,
            report,
            board,
            events_dir,
            DryRunState::default(),
            now_ms,
        )
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
}

const SUBJECT_COLUMNS: &str =
    "id, project, name, status, status_src, status_at_ms, created_at_ms, routine, merged_into";

fn subject_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SubjectRecord> {
    let text = |index: usize, value: String| {
        rusqlite::Error::InvalidColumnType(index, value, rusqlite::types::Type::Text)
    };
    let status: String = row.get(3)?;
    let src: String = row.get(4)?;
    Ok(SubjectRecord {
        id: row.get(0)?,
        project: row.get(1)?,
        name: row.get(2)?,
        // Both columns' `CHECK` list every value this build knows.
        status: Sourced {
            value: SubjectStatus::from_code(&status).ok_or_else(|| text(3, status.clone()))?,
            src: Source::from_code(&src).ok_or_else(|| text(4, src.clone()))?,
            at_ms: row.get(5)?,
        },
        created_at_ms: row.get(6)?,
        routine: row.get::<_, i64>(7)? == 1,
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

/// The registry as the Subjects view shows it, with what the window's
/// collector knows of dry runs.
pub(crate) fn view(
    conn: &Connection,
    report: RegistryReport,
    board: Option<&Board>,
    events_dir: &Path,
    dry_run: DryRunState,
    now_ms: i64,
) -> rusqlite::Result<RegistryView> {
    // One read transaction, so the subjects, sessions and excerpts come from
    // the same commit while the window writes beside a command line (WAL);
    // it is never written in, and ends when dropped.
    let snapshot = conn.unchecked_transaction()?;
    let subjects = subjects(&snapshot)?;
    let sessions = session_records(&snapshot)?;
    let excerpts = excerpts(&snapshot)?;
    let confirmations = confirmations(&snapshot)?;
    let evidence = EvidenceInput {
        calls: triage_calls_since(&snapshot, now_ms - COUNT_WINDOW_MS)?,
        latest_evaluation: latest_evaluation(&snapshot)?,
        dry_run,
        verdicts: verdict_records(&snapshot)?,
        auto_archive: auto_archive_setting(&snapshot),
    };
    Ok(build_view(
        &ViewInput {
            registry: report,
            ingestion: ingestion_view(&snapshot, events_dir)?,
            subjects: &subjects,
            sessions: &sessions,
            excerpts: &excerpts,
            confirmations: &confirmations,
            model: &model_input(),
            board,
            restore_notice: restore_notice(&snapshot),
            evidence: &evidence,
        },
        now_ms,
    ))
}

/// The triage model's part of the proposals, the same for the view and
/// for the writer that checks a confirmation against them (AD-49). Its
/// runner and its answers come with stories 3.9 and 3.11: until then it is
/// disabled.
pub(crate) fn model_input() -> ModelInput {
    ModelInput::default()
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
    let src: String = row.get(3)?;
    Ok(FactRow {
        session_id: row.get(0)?,
        fact: row.get(1)?,
        value,
        // The column's `CHECK` lists every source this build knows.
        src: Source::from_code(&src).ok_or_else(|| {
            rusqlite::Error::InvalidColumnType(3, src.clone(), rusqlite::types::Type::Text)
        })?,
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

#[cfg(feature = "window-authority")]
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
    use officina_core::registry::types::{ModelVerdict, ProposalSource, ReasonCode};
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
                cut_mid_turn: row.get::<_, i64>(12)? == 1,
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
/// The outcome and input hash of the latest call about `case_key`, when
/// there is one (D-87).
pub(crate) fn last_call_for(
    conn: &Connection,
    case_key: &str,
) -> rusqlite::Result<Option<(TriageOutcome, Option<String>)>> {
    conn.prepare_cached(
        "SELECT outcome, input_hash FROM triage_call WHERE case_key = ?1
         ORDER BY started_at_ms DESC, id DESC LIMIT 1",
    )?
    .query_map([case_key], |row| {
        Ok((code_of(row, 0, TriageOutcome::from_code)?, row.get(1)?))
    })?
    .next()
    .transpose()
}

/// Every confirmation as the rates read it (AD-50), by id.
pub(crate) fn verdict_records(conn: &Connection) -> rusqlite::Result<Vec<VerdictRecord>> {
    use officina_core::registry::types::{ModelVerdict, ProposalSource, ReasonCode};
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
            auto: row.get::<_, i64>(7)? == 1,
            confirmed_at_ms: row.get(8)?,
            reopened_at_ms: row.get(9)?,
        })
    })?
    .collect()
}

/// The auto-archive setting, with who set it and when, when one is
/// recorded and readable (D-69).
pub(crate) fn auto_archive_setting(conn: &Connection) -> Option<(AutoArchiveSetting, Source, i64)> {
    settings(conn)
        .ok()?
        .into_iter()
        .find(|row| row.key == SettingKey::AutoArchive)
        .and_then(|row| Some((serde_json::from_str(&row.value).ok()?, row.src, row.at_ms)))
}
