//! The window's writes to the registry (AD-38): compiled only with the
//! `window-authority` feature (D-43), so `officina.exe` built alone holds
//! none of it. Author actions on subjects (AD-48, D-54 to D-58) are planned
//! by core `registry::confirm` and written here in one transaction, with
//! source `author` and the action's time (AD-39, AD-40).

use std::collections::BTreeSet;
use std::path::Path;

use officina_core::board::Board;
use officina_core::registry::confirm::{
    AuthorAction, AuthorContext, ConfirmItem, ConfirmPlan, ItemAnswer, ItemOutcome, Plan,
    SubjectOrigin, Target, TriageContext, Wake, confirm_item, plan,
};
use officina_core::registry::filing::excerpts_to_clear;
use officina_core::registry::fold::FactWrite;
pub use officina_core::registry::types::AuthorRefusal;
use officina_core::registry::types::{
    EventFileRecord, ExcerptRecord, FactValue, RegistryReport, RegistryState, SessionRecord,
    SessionRow, SettingKey, Source, SubjectRecord, SubjectStatus,
};
use officina_core::registry::view::RegistryView;
use rusqlite::config::DbConfig;
use rusqlite::types::Value;
use rusqlite::{Connection, ErrorCode, TransactionBehavior};

use super::read::{self, SettingRow};
use super::store::Writer;

/// Text every registry writer error carries. CI searches the release
/// binaries for it: absent from `officina.exe` (D-83). It lives in the
/// error type the writer returns, so `lto` and `strip` keep it in any
/// binary that calls the writer.
pub const REGISTRY_MARKER: &str = "officina-registry-writer";

/// Why the window's registry could not be opened or written.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("{REGISTRY_MARKER}: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("{REGISTRY_MARKER}: {path}: {error}")]
    Io { path: String, error: std::io::Error },
    /// `quick_check` found problems.
    #[error("{REGISTRY_MARKER}: the integrity check failed: {0}")]
    Damaged(String),
    /// The damaged database could not be set aside: nothing was deleted,
    /// and the registry stays unavailable until the next start (D-97).
    #[error(
        "{REGISTRY_MARKER}: the damaged registry could not be set aside ({path}: {error}); nothing was deleted"
    )]
    QuarantineFailed { path: String, error: std::io::Error },
}

impl RegistryError {
    pub(crate) fn io(path: &Path, error: std::io::Error) -> RegistryError {
        RegistryError::Io {
            path: path.display().to_string(),
            error,
        }
    }

    /// True when the database itself is damaged: `quick_check` failed, or
    /// SQLite reported `SQLITE_CORRUPT` or `SQLITE_NOTADB` (AD-42).
    pub fn is_corruption(&self) -> bool {
        match self {
            RegistryError::Damaged(_) => true,
            RegistryError::Sqlite(rusqlite::Error::SqliteFailure(error, _)) => matches!(
                error.code,
                ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase
            ),
            _ => false,
        }
    }
}

/// Why an author action wrote nothing: refused by a rule, the registry
/// unchanged, or the registry itself failed (D-57). The refusal enum is
/// fieldless; only the registry's error carries the marker.
#[derive(Debug, thiserror::Error)]
pub enum AuthorError {
    #[error("refused: {}", .0.code())]
    Refused(AuthorRefusal),
    #[error(transparent)]
    Registry(#[from] RegistryError),
}

impl From<rusqlite::Error> for AuthorError {
    fn from(error: rusqlite::Error) -> AuthorError {
        AuthorError::Registry(error.into())
    }
}

impl AuthorError {
    /// The refusal, when a rule refused the action.
    pub fn refusal(&self) -> Option<AuthorRefusal> {
        match self {
            AuthorError::Refused(refusal) => Some(*refusal),
            AuthorError::Registry(_) => None,
        }
    }
}

/// The sessions and subjects an action reads beyond every subject: those it
/// names, and the sessions of the subjects it touches.
fn involved(action: &AuthorAction) -> (Vec<String>, Vec<i64>) {
    match action {
        AuthorAction::Create {
            origin: SubjectOrigin::Sessions(ids),
            ..
        } => (ids.clone(), Vec::new()),
        AuthorAction::Create { .. } => (Vec::new(), Vec::new()),
        AuthorAction::Rename { subject, .. }
        | AuthorAction::Archive { subject, .. }
        | AuthorAction::Reopen { subject } => (Vec::new(), vec![*subject]),
        AuthorAction::Merge { source, target } => (Vec::new(), vec![*source, *target]),
        AuthorAction::File { sessions, subject } => (sessions.clone(), vec![*subject]),
        AuthorAction::Unfile { sessions } => (sessions.clone(), Vec::new()),
    }
}

/// Stamps `reopened_at_ms` on `subject`'s latest confirmation, unless it
/// was already reopened (D-58): the first reopen is the one the flip rate
/// reads (AD-50).
fn stamp_reopen(conn: &Connection, subject: i64, at_ms: i64) -> rusqlite::Result<()> {
    conn.prepare_cached(
        "UPDATE confirmation SET reopened_at_ms = ?2
         WHERE id = (SELECT id FROM confirmation WHERE subject_id = ?1
                     ORDER BY confirmed_at_ms DESC, id DESC LIMIT 1)
           AND reopened_at_ms IS NULL",
    )?
    .execute(rusqlite::params![subject, at_ms])?;
    Ok(())
}

/// Wakes a subject a session alive again counts under (D-77): `active`,
/// source `rules`, at `at_ms`, and its latest confirmation stamped
/// reopened when it was archived (D-58).
pub(crate) fn apply_wake(conn: &Connection, wake: &Wake, at_ms: i64) -> rusqlite::Result<()> {
    conn.prepare_cached(
        "UPDATE subject SET status = ?2, status_src = ?3, status_at_ms = ?4 WHERE id = ?1",
    )?
    .execute(rusqlite::params![
        wake.subject,
        SubjectStatus::Active.code(),
        Source::Rules.code(),
        at_ms
    ])?;
    if wake.stamp_reopen {
        stamp_reopen(conn, wake.subject, at_ms)?;
    }
    Ok(())
}

/// Keeps the latest excerpts only among `subject`'s sessions (FR-4, D-50):
/// the core says which go.
pub(crate) fn prune_excerpts(conn: &Connection, subject: i64) -> rusqlite::Result<()> {
    let mut sessions = Vec::new();
    let mut excerpts = Vec::new();
    for id in read::session_ids_of_subject(conn, subject)? {
        if let Some(excerpt) = read::excerpt_of(conn, &id)? {
            excerpts.push(excerpt);
        }
        sessions.extend(read::session_record(conn, &id)?);
    }
    if excerpts.len() < 2 {
        return Ok(());
    }
    for id in excerpts_to_clear(&sessions, &excerpts) {
        conn.prepare_cached("DELETE FROM session_excerpt WHERE session_id = ?1")?
            .execute([id])?;
    }
    Ok(())
}

/// Writes an accepted plan at `now_ms`, source `author`; returns the
/// subject it created, if any.
fn apply_plan(conn: &Connection, plan: &Plan, now_ms: i64) -> rusqlite::Result<Option<i64>> {
    let author = Source::Author.code();
    let created = match &plan.create {
        Some(new) => {
            conn.prepare_cached(
                "INSERT INTO subject (project, name, status, status_src, status_at_ms, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            )?
            .execute(rusqlite::params![
                new.project,
                new.name,
                SubjectStatus::Active.code(),
                author,
                now_ms
            ])?;
            Some(conn.last_insert_rowid())
        }
        None => None,
    };
    let resolve = |target: Target| match target {
        Target::Existing(id) => Some(id),
        Target::Created => created,
    };
    for change in &plan.changes {
        conn.prepare_cached(
            "UPDATE subject SET name = coalesce(?2, name),
                 status = coalesce(?3, status),
                 status_src = CASE WHEN ?3 IS NULL THEN status_src ELSE ?6 END,
                 status_at_ms = CASE WHEN ?3 IS NULL THEN status_at_ms ELSE ?7 END,
                 routine = coalesce(?4, routine),
                 merged_into = coalesce(?5, merged_into)
             WHERE id = ?1",
        )?
        .execute(rusqlite::params![
            change.id,
            change.name,
            change.status.map(SubjectStatus::code),
            change.routine.map(i64::from),
            change.merged_into,
            author,
            now_ms
        ])?;
    }
    for (session_id, target) in &plan.filings {
        conn.prepare_cached(
            "UPDATE session SET subject_id = ?2, subject_src = ?3, subject_at_ms = ?4
             WHERE session_id = ?1",
        )?
        .execute(rusqlite::params![
            session_id,
            target.and_then(resolve),
            author,
            now_ms
        ])?;
    }
    for session_id in &plan.clear_excerpts {
        conn.prepare_cached("DELETE FROM session_excerpt WHERE session_id = ?1")?
            .execute([session_id])?;
    }
    if let Some(subject) = plan.stamp_reopen {
        stamp_reopen(conn, subject, now_ms)?;
    }
    if let Some(subject) = plan.prune.and_then(resolve) {
        prune_excerpts(conn, subject)?;
    }
    Ok(created)
}

/// Writes the verdict of a confirmation plan on `subject` at `now_ms`,
/// source `author`: the subject's status and routine box, its excerpts
/// cleared on `finished`, and the confirmation with the input it keeps
/// (AD-49, D-85, D-88).
fn apply_decision(
    conn: &Connection,
    planned: &ConfirmPlan,
    subject: i64,
    now_ms: i64,
) -> rusqlite::Result<()> {
    let Ok(decision) = &planned.decision else {
        return Ok(());
    };
    conn.prepare_cached(
        "UPDATE subject SET status = ?2, status_src = ?3, status_at_ms = ?4,
             routine = coalesce(?5, routine)
         WHERE id = ?1",
    )?
    .execute(rusqlite::params![
        subject,
        decision.status.code(),
        Source::Author.code(),
        now_ms,
        decision.routine.map(i64::from)
    ])?;
    for session_id in &decision.clear_excerpts {
        conn.prepare_cached("DELETE FROM session_excerpt WHERE session_id = ?1")?
            .execute([session_id])?;
    }
    conn.prepare_cached(
        "INSERT INTO confirmation (subject_id, session_id, proposed_verdict, proposed_source,
             proposed_reason, call_id, verdict, auto, confirmed_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)",
    )?
    .execute(rusqlite::params![
        subject,
        decision.session_id,
        decision.proposal.verdict.code(),
        decision.proposal.source.code(),
        decision.proposal.reason.code(),
        decision.proposal.call_id,
        decision.verdict.code(),
        now_ms
    ])?;
    if let Some(input) = &decision.input {
        let confirmation = conn.last_insert_rowid();
        conn.prepare_cached(
            "INSERT INTO confirmation_input (confirmation_id, prompt, answer_head, answer_tail,
                 end_reason, cut_mid_turn, expires_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?
        .execute(rusqlite::params![
            confirmation,
            input.prompt,
            input.answer_head,
            input.answer_tail,
            input.end_reason,
            i64::from(input.cut_mid_turn),
            input.expires_at_ms
        ])?;
    }
    Ok(())
}

/// Confirms one item inside the caller's transaction: the core plans it
/// from the registry as it stands after the items before it, so a row
/// confirmed earlier in the batch is gone for a later item with its key.
/// Returns the outcome and whether excerpts were cleared.
fn confirm_one(
    conn: &Connection,
    item: &ConfirmItem,
    now_ms: i64,
) -> rusqlite::Result<(ItemOutcome, bool)> {
    let subjects = read::subjects(conn)?;
    let sessions = read::session_records(conn)?;
    let confirmations = read::confirmations(conn)?;
    let excerpts = read::excerpts(conn)?;
    let known_projects = read::known_projects(conn)?;
    let model = read::model_input();
    let context = TriageContext {
        subjects: &subjects,
        sessions: &sessions,
        confirmations: &confirmations,
        excerpts: &excerpts,
        model: &model,
        known_projects: &known_projects,
    };
    let planned = match confirm_item(item, &context) {
        Ok(planned) => planned,
        Err(refused) => return Ok((refused, false)),
    };
    let created = match &planned.filing {
        Some(filing) => apply_plan(conn, filing, now_ms)?,
        None => None,
    };
    let subject = match planned.subject {
        Target::Existing(id) => id,
        Target::Created => created.unwrap_or_default(),
    };
    apply_decision(conn, &planned, subject, now_ms)?;
    Ok((planned.outcome(subject), planned.checkpoint()))
}

impl Writer {
    /// Confirms triage rows at `now_ms` in one transaction, committed
    /// `FULL` (AD-39, AD-49), and answers per item: each is checked against
    /// the proposal computed now and refused when it moved (D-63), an
    /// unfiled row is filed first (D-60). A `finished` verdict clears the
    /// subject's excerpts, and the WAL is then truncated so the cleared
    /// text leaves the log (AD-44); the confirmation keeps its own copy
    /// until those excerpts were due to expire (D-85).
    pub fn confirm_triage(
        &mut self,
        items: &[ConfirmItem],
        now_ms: i64,
    ) -> Result<Vec<ItemAnswer>, RegistryError> {
        self.confirm_triage_observed(items, now_ms, &mut |_| {})
    }

    /// [`Writer::confirm_triage`], calling `observe` with each item's index
    /// once it is applied, before the commit: the crash helper stalls there
    /// (D-89).
    pub fn confirm_triage_observed(
        &mut self,
        items: &[ConfirmItem],
        now_ms: i64,
        observe: &mut dyn FnMut(usize),
    ) -> Result<Vec<ItemAnswer>, RegistryError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut answers = Vec::with_capacity(items.len());
        let mut cleared = false;
        for (index, item) in items.iter().enumerate() {
            let (outcome, cleared_now) = confirm_one(&tx, item, now_ms)?;
            cleared |= cleared_now;
            answers.push(ItemAnswer {
                key: item.key.clone(),
                outcome,
            });
            observe(index);
        }
        tx.commit()?;
        if cleared {
            self.checkpoint()?;
        }
        Ok(answers)
    }

    /// The registry as the Subjects view shows it, read on the writer's own
    /// connection, with the live cards of `board` and the lag behind the
    /// event files of `events_dir` (AD-54).
    pub fn view(
        &self,
        board: Option<&Board>,
        events_dir: &Path,
        now_ms: i64,
    ) -> Result<RegistryView, RegistryError> {
        let report = RegistryReport {
            state: RegistryState::Ready,
            detail: None,
            exclude_from_sync: self
                .location()
                .exclude()
                .map(|path| path.display().to_string()),
        };
        Ok(read::view(&self.conn, report, board, events_dir, now_ms)?)
    }
    /// Runs one author action at `now_ms` in one transaction, as every
    /// write is, with `synchronous = FULL` (AD-39): the core plans it from
    /// the registry's subjects and the sessions it involves, this writes it
    /// with source `author`; a refusal writes nothing. Returns the subject
    /// created, if any. An archive then truncates the WAL, so the cleared
    /// text leaves the log (AD-44).
    pub fn author(
        &mut self,
        action: &AuthorAction,
        now_ms: i64,
    ) -> Result<Option<i64>, AuthorError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let subjects = read::subjects(&tx)?;
        let (named, touched) = involved(action);
        let mut ids: BTreeSet<String> = named.into_iter().collect();
        for subject in touched {
            ids.extend(read::session_ids_of_subject(&tx, subject)?);
        }
        let mut sessions = Vec::new();
        for id in &ids {
            sessions.extend(read::session_record(&tx, id)?);
        }
        let known_projects = read::known_projects(&tx)?;
        let planned = plan(
            action,
            &AuthorContext {
                subjects: &subjects,
                sessions: &sessions,
                known_projects: &known_projects,
            },
        )
        .map_err(AuthorError::Refused)?;
        let created = apply_plan(&tx, &planned, now_ms)?;
        tx.commit()?;
        if planned.checkpoint {
            self.checkpoint()?;
        }
        Ok(created)
    }

    /// Creates a subject named `name` (D-95): from the selected sessions,
    /// which give it their one project and are filed under it, or in one of
    /// the projects the registry knows. Returns its id.
    pub fn create_subject(
        &mut self,
        name: &str,
        origin: SubjectOrigin,
        now_ms: i64,
    ) -> Result<i64, AuthorError> {
        let action = AuthorAction::Create {
            name: name.to_owned(),
            origin,
        };
        Ok(self.author(&action, now_ms)?.unwrap_or_default())
    }

    /// Renames a subject that is not archived (D-57).
    pub fn rename_subject(
        &mut self,
        subject: i64,
        name: &str,
        now_ms: i64,
    ) -> Result<(), AuthorError> {
        let action = AuthorAction::Rename {
            subject,
            name: name.to_owned(),
        };
        self.author(&action, now_ms).map(|_| ())
    }

    /// Moves the sessions of `source` to `target` and archives `source`
    /// with `merged_into` (D-55).
    pub fn merge_subjects(
        &mut self,
        source: i64,
        target: i64,
        now_ms: i64,
    ) -> Result<(), AuthorError> {
        self.author(&AuthorAction::Merge { source, target }, now_ms)
            .map(|_| ())
    }

    /// Files one or several sessions under `subject` (D-56).
    pub fn file_sessions(
        &mut self,
        sessions: &[String],
        subject: i64,
        now_ms: i64,
    ) -> Result<(), AuthorError> {
        let action = AuthorAction::File {
            sessions: sessions.to_vec(),
            subject,
        };
        self.author(&action, now_ms).map(|_| ())
    }

    /// Unfiles one or several sessions (D-56).
    pub fn unfile_sessions(&mut self, sessions: &[String], now_ms: i64) -> Result<(), AuthorError> {
        let action = AuthorAction::Unfile {
            sessions: sessions.to_vec(),
        };
        self.author(&action, now_ms).map(|_| ())
    }

    /// Archives `subject`, `routine` when it could have run alone, and
    /// clears the excerpts of its sessions from the file and its WAL (FR-4,
    /// AD-44).
    pub fn archive_subject(
        &mut self,
        subject: i64,
        routine: bool,
        now_ms: i64,
    ) -> Result<(), AuthorError> {
        self.author(&AuthorAction::Archive { subject, routine }, now_ms)
            .map(|_| ())
    }

    /// Reopens an archived subject as `to_resume` and stamps its latest
    /// confirmation reopened (AD-48, D-58).
    pub fn reopen_subject(&mut self, subject: i64, now_ms: i64) -> Result<(), AuthorError> {
        self.author(&AuthorAction::Reopen { subject }, now_ms)
            .map(|_| ())
    }

    /// Every `subject` row, by id.
    pub fn subjects(&self) -> Result<Vec<SubjectRecord>, RegistryError> {
        Ok(read::subjects(&self.conn)?)
    }

    /// Records `value` under `key`, replacing what was there.
    pub fn set_setting(
        &mut self,
        key: SettingKey,
        value: &str,
        src: Source,
        now_ms: i64,
    ) -> Result<(), RegistryError> {
        self.conn
            .prepare_cached(
                "INSERT INTO setting (key, value, src, at_ms) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value, src = excluded.src, at_ms = excluded.at_ms",
            )?
            .execute(rusqlite::params![key.code(), value, src.code(), now_ms])?;
        Ok(())
    }

    /// Every `setting` row.
    pub fn settings(&self) -> Result<Vec<SettingRow>, RegistryError> {
        Ok(read::settings(&self.conn)?)
    }

    /// Copies every page of the WAL into the database and truncates the WAL
    /// to zero bytes, so a cleared text does not linger in an old frame
    /// (AD-39). Returns false when a reader kept it from completing.
    pub fn checkpoint(&self) -> Result<bool, RegistryError> {
        let busy: i64 = self
            .conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))?;
        Ok(busy == 0)
    }

    /// The value a setting of this connection reads back, by its name.
    pub fn pragma(&self, name: &str) -> Option<String> {
        read::pragma_value(&self.conn, name)
    }

    /// Closes the connection without the checkpoint SQLite runs when the
    /// last connection closes, so a database found damaged is left as it
    /// was, its WAL beside it, for the next start to set aside (AD-42).
    pub fn abandon(self) {
        let _ = self
            .conn
            .set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true);
    }

    /// The schema version the database is at.
    pub fn user_version(&self) -> Result<i64, RegistryError> {
        Ok(read::user_version(&self.conn)?)
    }

    /// Every `event_file` row, by name.
    pub fn event_files(&self) -> Result<Vec<EventFileRecord>, RegistryError> {
        Ok(read::event_files(&self.conn)?)
    }

    /// Every `session` row, by session id.
    pub fn sessions(&self) -> Result<Vec<SessionRow>, RegistryError> {
        Ok(read::sessions(&self.conn)?)
    }

    /// Every `session_excerpt` row, by session id.
    pub fn excerpts(&self) -> Result<Vec<ExcerptRecord>, RegistryError> {
        Ok(read::excerpts(&self.conn)?)
    }

    /// Every session with its facts, by session id (AD-40).
    pub fn session_records(&self) -> Result<Vec<SessionRecord>, RegistryError> {
        Ok(read::session_records(&self.conn)?)
    }

    /// Records `writes` in one transaction, each replacing the stored fact
    /// only when it is later (AD-43). Every session must exist.
    pub fn record_facts(&mut self, writes: &[FactWrite]) -> Result<(), RegistryError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for write in writes {
            write_fact(&tx, write)?;
        }
        tx.commit()?;
        Ok(())
    }
}

/// Writes one fact, replacing the stored row only when
/// [`officina_core::registry::fold::replaces`] says so: whatever the order
/// observations arrive in, the latest wins, and a replayed one changes
/// nothing (AD-43).
pub(crate) fn write_fact(conn: &Connection, write: &FactWrite) -> rusqlite::Result<()> {
    let value = match &write.value {
        FactValue::Integer(i) => Value::Integer(*i),
        FactValue::Text(text) => Value::Text(text.clone()),
    };
    // The `WHERE` is `replaces` in SQL: a strictly later observation only.
    // The fold vectors replayed through the writer hold the two together.
    conn.prepare_cached(
        "INSERT INTO session_fact (session_id, fact, value, src, at_ms) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (session_id, fact) DO UPDATE SET value = excluded.value, src = excluded.src,
             at_ms = excluded.at_ms
         WHERE excluded.at_ms > session_fact.at_ms",
    )?
    .execute(rusqlite::params![
        write.session_id,
        write.fact.code(),
        value,
        write.src.code(),
        write.at_ms
    ])?;
    Ok(())
}
