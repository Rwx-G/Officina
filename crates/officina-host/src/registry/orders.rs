//! Work order records (AD-81, AD-82, D-135, D-136, D-158): the row a
//! launch inserts, its outcome, the expiry of rows whose outcome never came
//! back, the binding to the session the order started and the filing of
//! that session under the order's subject; and the briefing's cursor, when
//! the author last read it (AD-83). Compiled only with the
//! `window-authority` feature: only the window writes.

use officina_core::agents::AgentRecord;
use officina_core::orders::work_order::{
    BIND_WINDOW_MS, Binding, LAUNCHING_EXPIRY_MS, SubjectChoice, UnboundOrder, WorkOrder, bind,
    cut_error, cut_printed, order_name,
};
use officina_core::registry::filing::{NewSubjectFiling, refiles_on_binding, subject_for_new};
use officina_core::registry::types::{
    LaunchOutcome, OrchestratorStartOutcome, ProposalStatus, SettingKey, Source, SubjectStatus,
    WorkOrderRecord, WorkOrderSource,
};
use officina_core::spawn;
use officina_core::workshop::run::Runner;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::read;
use super::store::Writer;
use super::write::{RegistryError, prune_excerpts, setting_json};

/// What a launch records (AD-81 step 1): the validated order, the card it
/// consumed and what the card held.
#[derive(Debug, Clone, Copy)]
pub struct LaunchRecord<'a> {
    pub order: &'a WorkOrder,
    /// The card's id, which becomes the row's (D-136).
    pub card_id: &'a str,
    /// The `seq` the card store reserved at issue (D-135).
    pub seq: u32,
    pub source: WorkOrderSource,
    pub subject: &'a SubjectChoice,
    /// The proposal the card came from, for a `proposal` source.
    pub proposal_id: Option<&'a str>,
    /// Where the card's permission mode came from (D-980), so the row's
    /// argument vector is the one that ran.
    pub permission: officina_core::orders::permission::WorkOrderPermission,
}

/// The registry already holds the order's name, or its template's `seq`:
/// the launch is refused `name_taken` (D-135) and nothing is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NameTaken;

/// True when `error` is a `UNIQUE` constraint failing, which only `name`
/// and `(template, seq)` carry.
fn is_unique_violation(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _)
            if failure.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
    )
}

/// The subject of the order bound to `session_id`, when one is and it has
/// a subject: what D-79's creation function consults first (AD-81 step 5).
pub(crate) fn bound_subject_of(
    conn: &Connection,
    session_id: &str,
) -> rusqlite::Result<Option<i64>> {
    conn.prepare_cached(
        "SELECT subject_id FROM work_order
         WHERE session_id = ?1 AND subject_id IS NOT NULL
         ORDER BY bound_at_ms DESC, id LIMIT 1",
    )?
    .query_row([session_id], |row| row.get(0))
    .optional()
}

/// Binds the `started` orders not yet bound to the background sessions of
/// `records` (AD-81 step 4), inside the caller's transaction, and returns
/// the bindings written. An order is bound once.
pub(crate) fn bind_orders(
    conn: &Connection,
    records: &[AgentRecord],
    now_ms: i64,
) -> rusqlite::Result<Vec<Binding>> {
    let unbound: Vec<UnboundOrder> = conn
        .prepare_cached(
            "SELECT id, name, cwd, launched_at_ms FROM work_order
             WHERE outcome = ?1 AND bound_at_ms IS NULL AND launched_at_ms > ?2
             ORDER BY launched_at_ms, id",
        )?
        .query_map(
            params![LaunchOutcome::Started.code(), now_ms - BIND_WINDOW_MS],
            |row| {
                Ok(UnboundOrder {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    cwd: row.get(2)?,
                    launched_at_ms: row.get(3)?,
                })
            },
        )?
        .collect::<rusqlite::Result<_>>()?;
    if unbound.is_empty() {
        return Ok(Vec::new());
    }
    let bindings = bind(&unbound, records, now_ms);
    for binding in &bindings {
        conn.prepare_cached(
            "UPDATE work_order SET bg_id = ?2, session_id = ?3, bound_at_ms = ?4
             WHERE id = ?1 AND bound_at_ms IS NULL",
        )?
        .execute(params![
            binding.order_id,
            binding.bg_id,
            binding.session_id,
            now_ms
        ])?;
    }
    Ok(bindings)
}

/// Files the session of `binding` under its order's subject, source
/// `author`, when the registry holds the session and the rules filed it
/// elsewhere or not at all (AD-81 step 5: `author` beats `rules`).
pub(crate) fn file_bound_session(
    conn: &Connection,
    binding: &Binding,
    now_ms: i64,
) -> rusqlite::Result<()> {
    let Some(subject) = bound_subject_of(conn, &binding.session_id)? else {
        return Ok(());
    };
    let Some(session) = read::session_record(conn, &binding.session_id)? else {
        return Ok(());
    };
    if !refiles_on_binding(session.subject.as_ref(), subject) {
        return Ok(());
    }
    conn.prepare_cached(
        "UPDATE session SET subject_id = ?2, subject_src = ?3, subject_at_ms = ?4
         WHERE session_id = ?1",
    )?
    .execute(params![
        binding.session_id,
        subject,
        Source::Author.code(),
        now_ms
    ])?;
    prune_excerpts(conn, subject)
}

/// The subject a `started` order files under (AD-81 step 5, D-158): the
/// chosen one, or its `New` name created now, never at the click.
fn settle_subject(conn: &Connection, id: &str, now_ms: i64) -> rusqlite::Result<()> {
    let row: Option<(Option<i64>, Option<String>, String)> = conn
        .prepare_cached(
            "SELECT subject_id, new_subject_name, project FROM work_order WHERE id = ?1",
        )?
        .query_row([id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .optional()?;
    let Some((None, Some(name), project)) = row else {
        return Ok(());
    };
    let subject = match subject_for_new(&name, &project, &read::subjects(conn)?) {
        NewSubjectFiling::Existing(subject) => subject,
        NewSubjectFiling::Create(name) => {
            conn.prepare_cached(
                "INSERT INTO subject (project, name, status, status_src, status_at_ms, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            )?
            .execute(params![
                project,
                name,
                SubjectStatus::Active.code(),
                Source::Author.code(),
                now_ms
            ])?;
            conn.last_insert_rowid()
        }
        NewSubjectFiling::Unfiled(_) => return Ok(()),
    };
    conn.prepare_cached("UPDATE work_order SET subject_id = ?2 WHERE id = ?1")?
        .execute(params![id, subject])?;
    Ok(())
}

/// Inserts `launch`'s `work_order` row in `tx` with outcome `launching`,
/// runner `runner` and `argv` as the vector that runs, and sets the
/// proposal it came from `accepted` (AD-81 step 1, AD-91, AD-119). A name
/// or `(template, seq)` already held is [`NameTaken`], nothing written.
pub(crate) fn insert_order_row(
    tx: &Connection,
    launch: &LaunchRecord,
    argv: &[String],
    runner: Runner,
    now_ms: i64,
) -> Result<Result<(), NameTaken>, RegistryError> {
    let order = launch.order;
    let (subject_id, new_subject_name) = match launch.subject {
        SubjectChoice::Existing(id) => (Some(*id), None),
        SubjectChoice::New(name) => (None, Some(name.as_str())),
    };
    let inserted = tx.execute(
        "INSERT INTO work_order (id, seq, template, project, cwd, values_json, argv_json,
             prompt_sha256, catalogue_hash, name, subject_id, new_subject_name, source,
             proposal_id, launched_at_ms, outcome, runner)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        params![
            launch.card_id,
            launch.seq,
            order.template(),
            order.project(),
            order.project_folder(),
            json(order.values()),
            json(&argv),
            order.prompt_sha256(),
            order.catalogue_hash(),
            order_name(order.template(), launch.seq),
            subject_id,
            new_subject_name,
            launch.source.code(),
            launch.proposal_id,
            now_ms,
            LaunchOutcome::Launching.code(),
            runner.code()
        ],
    );
    match inserted {
        Ok(_) => {}
        Err(error) if is_unique_violation(&error) => return Ok(Err(NameTaken)),
        Err(error) => return Err(error.into()),
    }
    if let Some(proposal) = launch.proposal_id {
        tx.execute(
            "UPDATE proposal SET status = ?2, decided_at_ms = ?3, work_order_id = ?4
             WHERE id = ?1 AND status != ?2",
            params![
                proposal,
                ProposalStatus::Accepted.code(),
                now_ms,
                launch.card_id
            ],
        )?;
    }
    Ok(Ok(()))
}

impl Writer {
    /// The highest `seq` the registry holds for `template`, which the card
    /// store reads when it reserves a card's `seq` (D-135).
    pub fn highest_work_order_seq(&self, template: &str) -> Result<Option<u32>, RegistryError> {
        let highest: Option<i64> = self.conn.query_row(
            "SELECT max(seq) FROM work_order WHERE template = ?1",
            [template],
            |row| row.get(0),
        )?;
        Ok(highest.and_then(|seq| u32::try_from(seq).ok()))
    }

    /// True when the registry holds `<template>-<seq>` as a name, or
    /// `seq` for `template`: a launch of that card is refused `name_taken`
    /// (D-135), the last of AD-78's checks.
    pub fn work_order_name_taken(&self, template: &str, seq: u32) -> Result<bool, RegistryError> {
        Ok(self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM work_order
                 WHERE name = ?1 OR (template = ?2 AND seq = ?3))",
            params![order_name(template, seq), template, seq],
            |row| row.get(0),
        )?)
    }

    /// Records a launch at `now_ms` with outcome `launching` (AD-81 step
    /// 1): the order's project folder as `cwd`, its values and the argument
    /// vector that runs as JSON, the prompt's SHA-256, the catalogue hash
    /// and `name` = `<template>-<seq>`. A name or `(template, seq)` the
    /// registry already holds is refused [`NameTaken`], nothing written.
    /// A launch from a proposal sets it `accepted`, with the order's id, in
    /// the same transaction (AD-91): a card from it launched.
    pub fn insert_work_order(
        &mut self,
        launch: &LaunchRecord,
        now_ms: i64,
    ) -> Result<Result<(), NameTaken>, RegistryError> {
        let argv = spawn::work_order(launch.order, launch.seq, None, launch.permission)
            .args()
            .to_vec();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Err(taken) = insert_order_row(&tx, launch, &argv, Runner::Host, now_ms)? {
            return Ok(Err(taken));
        }
        tx.commit()?;
        Ok(Ok(()))
    }

    /// Records how the launch of order `id` ended (AD-81 step 3), even
    /// after its row read `unknown` (D-135: a late outcome is still
    /// recorded): `printed` cut to its bound and `error` to its first
    /// characters, both stored as the untrusted text they are. At
    /// `started`, a `New` subject is created, or the subject of that name
    /// used (D-158), at `now_ms`. Returns false when no row has the id.
    pub fn record_launch_outcome(
        &mut self,
        id: &str,
        outcome: LaunchOutcome,
        exit_code: Option<i32>,
        error: Option<&str>,
        printed: Option<&str>,
        now_ms: i64,
    ) -> Result<bool, RegistryError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let updated =
            tx.prepare_cached(
                "UPDATE work_order SET outcome = ?2, exit_code = ?3, error = ?4, printed = ?5
                 WHERE id = ?1",
            )?
            .execute(params![
                id,
                outcome.code(),
                exit_code,
                error.map(cut_error),
                printed.map(cut_printed)
            ])? == 1;
        if updated && outcome == LaunchOutcome::Started {
            settle_subject(&tx, id, now_ms)?;
        }
        tx.commit()?;
        Ok(updated)
    }

    /// Turns the `launching` rows older than [`LAUNCHING_EXPIRY_MS`] at
    /// `now_ms` into `unknown` (D-136): the start sequence runs it before
    /// the first view, and the collector on its own tick. Returns how many.
    pub fn expire_launching(&mut self, now_ms: i64) -> Result<usize, RegistryError> {
        Ok(self.conn.execute(
            "UPDATE work_order SET outcome = ?1 WHERE outcome = ?2 AND launched_at_ms < ?3",
            params![
                LaunchOutcome::Unknown.code(),
                LaunchOutcome::Launching.code(),
                now_ms - LAUNCHING_EXPIRY_MS
            ],
        )?)
    }

    /// The latest work orders, newest first, with their binding and the
    /// bound session's record (AD-82, D-130).
    pub fn work_orders(&self) -> Result<Vec<WorkOrderRecord>, RegistryError> {
        Ok(read::work_orders(&self.conn)?)
    }

    /// Records that the author read the briefing at `now_ms` (AD-83):
    /// `setting.briefing_read_at_ms`, source `author`, the time as its
    /// value. The window's next briefing counts its changes from there.
    pub fn mark_briefing_read(&mut self, now_ms: i64) -> Result<(), RegistryError> {
        self.set_setting(
            SettingKey::BriefingReadAtMs,
            &setting_json(&now_ms),
            Source::Author,
            now_ms,
        )
    }

    /// When the author last read the briefing, `None` before the first
    /// reading (AD-83).
    pub fn briefing_read_at(&self) -> Result<Option<i64>, RegistryError> {
        Ok(read::briefing_read_at(&self.conn)?)
    }

    /// True when a work order the window launched is bound to background
    /// session `session_id` (AD-81 step 4): its prompts are the order's,
    /// which the author approved (D-960).
    pub fn is_work_order_session(&self, session_id: &str) -> Result<bool, RegistryError> {
        Ok(self
            .conn
            .prepare_cached("SELECT 1 FROM work_order WHERE session_id = ?1 LIMIT 1")?
            .query_row([session_id], |_| Ok(()))
            .optional()?
            .is_some())
    }

    /// Every `orchestrator_start` row, newest first (D-144).
    pub fn orchestrator_starts(&self) -> Result<Vec<read::OrchestratorStartRow>, RegistryError> {
        Ok(read::orchestrator_starts(&self.conn)?)
    }

    /// Records one Start of the orchestrator (AD-82, AD-89, D-144): when,
    /// how it ended, the kept pid and its creation time, and the cause of a
    /// refused or failed start. A spawned row's start is the next session's
    /// briefing cursor. Returns the row's id.
    pub fn record_orchestrator_start(
        &mut self,
        start: &OrchestratorStartRecord,
    ) -> Result<i64, RegistryError> {
        self.conn.execute(
            "INSERT INTO orchestrator_start (started_at_ms, pid, pid_created_at_ms, outcome, error)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                start.started_at_ms,
                start.pid,
                start.pid_created_at_ms,
                start.outcome.code(),
                start.error
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }
}

/// One Start of the orchestrator, as `orchestrator_start` records it
/// (AD-82, D-144).
#[derive(Debug, Clone, Copy)]
pub struct OrchestratorStartRecord<'a> {
    pub started_at_ms: i64,
    pub outcome: OrchestratorStartOutcome,
    pub pid: Option<u32>,
    pub pid_created_at_ms: Option<i64>,
    /// Why a start was refused or failed.
    pub error: Option<&'a str>,
}

/// `value` as JSON. A map of strings or a list of strings always
/// serialises, so a failure is a bug, never a quiet default (as
/// `write::setting_json`).
fn json<T: serde::Serialize + ?Sized>(value: &T) -> String {
    serde_json::to_string(value).expect("strings always serialise")
}
