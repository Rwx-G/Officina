//! Proposal ingestion (AD-91, AD-182, D-133, D-145, D-152, D-155): the window
//! reads `<home>/orchestrator/proposals/`, validates each file through
//! `orders::proposal`, stores it once by its id and deletes it. Compiled
//! only with the `window-authority` feature: only the window writes, and
//! `officina mcp` only writes files.
//!
//! The order is what makes a crash or a restart lose and duplicate
//! nothing: a file is inserted with `INSERT OR IGNORE` on its id and
//! committed before it is deleted, so a file left after its commit is
//! ignored by the key the next pass, and a temporary file (`.tmp`) is never
//! read, only deleted once older than [`TMP_FILE_TTL_MS`].

use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use officina_core::orders::card::{ProposalOrder, ProposalRefusal};
use officina_core::orders::proposal::{
    FileRead, KnownProposal, PROPOSAL_FILE_MAX_BYTES, PROPOSAL_HOUR_MS, ProposalFile,
    ProposalRecord, ProposalTarget, ProposedSubject, ProposedSubjectView, RegistryRefs,
    TMP_FILE_TTL_MS, expired_answers, id_of_file, ingestion_refusal, is_expired, is_temporary_file,
    proposal_id, record_of, superseded_pending, written_at,
};
use officina_core::registry::types::{ProposalKind, ProposalStatus};
use officina_core::relay::catalogue::CanonicalCatalogue;
use officina_core::untrusted::Untrusted;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use super::read;
use super::store::Writer;
use super::write::RegistryError;

/// What one ingestion pass did (D-155).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProposalPass {
    /// Files stored as a row, whatever their status.
    pub ingested: u32,
    /// Files whose name is no proposal's, deleted without a row.
    pub misnamed: u32,
    /// Files past the hourly or the pending limit, deleted without a row.
    pub over_limit: u32,
    /// Temporary files older than [`TMP_FILE_TTL_MS`], deleted unread.
    pub stale_tmp: u32,
    /// Pending work order proposals turned `expired`.
    pub expired: u32,
    /// Pending recaps and answers a newer one of the same kind on the
    /// same target superseded.
    pub superseded: u32,
}

impl ProposalPass {
    /// True when the pass changed a row, so the views it shows change.
    pub fn changed_rows(&self) -> bool {
        self.ingested + self.expired + self.superseded > 0
    }
}

/// Reads at most [`PROPOSAL_FILE_MAX_BYTES`] plus one byte of `path`: a
/// larger file is `Oversized`, never read whole.
fn read_bounded(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(PROPOSAL_FILE_MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// When `path` was last written, in Unix milliseconds.
fn modified_ms(path: &Path) -> Option<i64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    let since = modified.duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(since.as_millis()).ok()
}

/// The proposal files of `dir` by id, sorted; misnamed files and stale
/// temporary files are deleted on the way and counted in `pass`. A folder
/// that does not exist holds none.
fn proposal_files(dir: &Path, now_ms: i64, pass: &mut ProposalPass) -> Vec<(String, PathBuf)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        if !entry.metadata().is_ok_and(|meta| meta.is_file()) {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_temporary_file(&name) {
            let stale = modified_ms(&path).is_some_and(|at| now_ms - at >= TMP_FILE_TTL_MS);
            if stale && fs::remove_file(&path).is_ok() {
                pass.stale_tmp += 1;
            }
            continue;
        }
        match id_of_file(&name) {
            Some(id) => found.push((id.to_owned(), path)),
            None => {
                if fs::remove_file(&path).is_ok() {
                    pass.misnamed += 1;
                }
            }
        }
    }
    found.sort();
    found
}

/// The proposals the limits count before the earliest of `ids` (D-769):
/// every pending row, and every row written within the hour before it.
fn known_proposals(conn: &Connection, earliest: &str) -> rusqlite::Result<Vec<KnownProposal>> {
    let from =
        written_at(earliest).map_or_else(String::new, |at| proposal_id(at - PROPOSAL_HOUR_MS, 0));
    conn.prepare_cached("SELECT id, status FROM proposal WHERE status = ?1 OR id >= ?2")?
        .query_map(params![ProposalStatus::Pending.code(), from], |row| {
            Ok(KnownProposal {
                id: row.get(0)?,
                pending: row.get::<_, String>(1)? == ProposalStatus::Pending.code(),
            })
        })?
        .collect()
}

fn json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("a map of strings always serializes")
}

/// Inserts `record` unless its id is stored already (AD-91: the file's id
/// is the key), inside the caller's transaction; true when it inserted.
fn insert(tx: &Transaction, record: &ProposalRecord) -> rusqlite::Result<bool> {
    let (subject_id, session_id) = match &record.target {
        Some(ProposalTarget::SubjectId(id)) => (Some(*id), None),
        Some(ProposalTarget::SessionId(id)) => (None, Some(id.as_str())),
        None => (None, None),
    };
    let inserted = tx.execute(
        "INSERT OR IGNORE INTO proposal (id, kind, status, invalid_reason, received_at_ms,
             expires_at_ms, orchestrator_session_id, target_subject_id, target_session_id,
             template, project, values_json, subject_choice_json, texts_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            record.id,
            record.kind.map(ProposalKind::code),
            record.status.code(),
            record.invalid_reason.map(|r| r.code()),
            record.received_at_ms,
            record.expires_at_ms,
            record.orchestrator_session_id,
            subject_id,
            session_id,
            record.template,
            record.project,
            record.values.as_ref().map(json),
            record.subject.as_ref().map(json),
            record.texts.as_ref().map(json),
        ],
    )?;
    Ok(inserted == 1)
}

/// Supersedes, inside the caller's transaction, every pending proposal of
/// `kind` on `target` but the newest (D-145, D-575); returns the ids
/// superseded.
fn supersede(
    tx: &Transaction,
    kind: ProposalKind,
    target: &ProposalTarget,
    now_ms: i64,
) -> rusqlite::Result<Vec<String>> {
    let (column, value): (&str, rusqlite::types::Value) = match target {
        ProposalTarget::SubjectId(id) => ("target_subject_id", (*id).into()),
        ProposalTarget::SessionId(id) => ("target_session_id", id.clone().into()),
    };
    let pending: Vec<(String, ProposalTarget)> = tx
        .prepare(&format!(
            "SELECT id FROM proposal WHERE kind = ?1 AND status = ?2 AND {column} = ?3"
        ))?
        .query_map(
            params![kind.code(), ProposalStatus::Pending.code(), value],
            |row| Ok((row.get::<_, String>(0)?, target.clone())),
        )?
        .collect::<rusqlite::Result<_>>()?;
    let superseded = superseded_pending(&pending);
    for id in &superseded {
        tx.execute(
            "UPDATE proposal SET status = ?2, decided_at_ms = ?3 WHERE id = ?1",
            params![id, ProposalStatus::Superseded.code(), now_ms],
        )?;
    }
    Ok(superseded)
}

/// The session ids among `bytes`'s recap target that the registry holds.
fn sessions_named(conn: &Connection, bytes: &[u8]) -> rusqlite::Result<BTreeSet<String>> {
    let Ok(ProposalFile::Recap(file)) = serde_json::from_slice::<ProposalFile>(bytes) else {
        return Ok(BTreeSet::new());
    };
    let ProposalTarget::SessionId(id) = file.target else {
        return Ok(BTreeSet::new());
    };
    let held = conn
        .prepare_cached("SELECT 1 FROM session WHERE session_id = ?1")?
        .query_row([&id], |_| Ok(()))
        .optional()?;
    Ok(held.map(|()| BTreeSet::from([id])).unwrap_or_default())
}

impl Writer {
    /// One ingestion pass of `dir` at `now_ms` (AD-91, D-155), validated
    /// against the approved snapshot `catalogue`: misnamed files deleted
    /// without a row, stale temporary files deleted unread; then each
    /// proposal file, oldest first, deleted without a row when past the
    /// hourly or pending limit, else read at most 16 KiB + 1 bytes,
    /// validated, inserted with `INSERT OR IGNORE` on its id and committed
    /// (a valid recap superseding the pending one on its target), and only
    /// then deleted. The pending work order proposals past their 24 hours
    /// are expired last. A file that cannot be read now is left for the
    /// next pass. No session takes a proposed answer in this pass: the
    /// window's passes name them ([`Writer::ingest_proposals_for`]).
    pub fn ingest_proposals(
        &mut self,
        dir: &Path,
        catalogue: Option<&CanonicalCatalogue>,
        now_ms: i64,
    ) -> Result<ProposalPass, RegistryError> {
        self.ingest_proposals_for(dir, catalogue, &BTreeSet::new(), now_ms)
    }

    /// [`Writer::ingest_proposals`] with `answerable`, the sessions that
    /// take a proposed answer on the latest board (AD-182): an answer
    /// naming another is stored `invalid`; a valid one supersedes the
    /// pending answer of its session (D-575).
    pub fn ingest_proposals_for(
        &mut self,
        dir: &Path,
        catalogue: Option<&CanonicalCatalogue>,
        answerable: &BTreeSet<String>,
        now_ms: i64,
    ) -> Result<ProposalPass, RegistryError> {
        let mut pass = ProposalPass::default();
        let files = proposal_files(dir, now_ms, &mut pass);
        if let Some((earliest, _)) = files.first() {
            let mut known = known_proposals(&self.conn, earliest)?;
            let subjects = read::subjects(&self.conn)?;
            for (id, path) in &files {
                let stored = self
                    .conn
                    .prepare_cached("SELECT 1 FROM proposal WHERE id = ?1")?
                    .query_row([id], |_| Ok(()))
                    .optional()?
                    .is_some();
                if !stored && ingestion_refusal(id, &known).is_some() {
                    if fs::remove_file(path).is_ok() {
                        pass.over_limit += 1;
                    }
                    continue;
                }
                if !stored {
                    let Ok(bytes) = read_bounded(path) else {
                        continue;
                    };
                    let read = if bytes.len() > PROPOSAL_FILE_MAX_BYTES {
                        FileRead::Oversized
                    } else {
                        FileRead::Bytes(&bytes)
                    };
                    let sessions = match read {
                        FileRead::Bytes(bytes) => sessions_named(&self.conn, bytes)?,
                        FileRead::Oversized => BTreeSet::new(),
                    };
                    let refs = RegistryRefs {
                        subjects: &subjects,
                        sessions: &sessions,
                        answerable,
                    };
                    let record = record_of(id, read, catalogue, &refs, now_ms);
                    let tx = self.conn.transaction()?;
                    let inserted = insert(&tx, &record)?;
                    let superseded = match record.superseding() {
                        Some((kind, target)) if inserted => supersede(&tx, kind, target, now_ms)?,
                        _ => Vec::new(),
                    };
                    tx.commit()?;
                    if inserted {
                        pass.ingested += 1;
                        pass.superseded += u32::try_from(superseded.len()).unwrap_or(u32::MAX);
                        for known in known.iter_mut().filter(|k| superseded.contains(&k.id)) {
                            known.pending = false;
                        }
                        known.push(KnownProposal {
                            id: id.clone(),
                            pending: record.status == ProposalStatus::Pending
                                && !superseded.contains(id),
                        });
                    }
                }
                // Committed before deleted: a file left by a crash here is
                // ignored by its key at the next pass.
                let _ = fs::remove_file(path);
            }
        }
        pass.expired = self.expire_proposals(now_ms)?;
        Ok(pass)
    }

    /// Turns the pending proposals whose expiry is past at `now_ms` into
    /// `expired` (D-145); returns how many.
    pub fn expire_proposals(&mut self, now_ms: i64) -> Result<u32, RegistryError> {
        let expired = self.conn.execute(
            "UPDATE proposal SET status = ?1, decided_at_ms = ?2
             WHERE status = ?3 AND expires_at_ms IS NOT NULL AND expires_at_ms <= ?2",
            params![
                ProposalStatus::Expired.code(),
                now_ms,
                ProposalStatus::Pending.code()
            ],
        )?;
        Ok(u32::try_from(expired).unwrap_or(u32::MAX))
    }

    /// Turns the pending answer proposals whose session is not in
    /// `answerable`, the sessions that take a proposed answer on the board
    /// just built, into `expired` (AD-182); returns how many.
    pub fn expire_answers(
        &mut self,
        answerable: &BTreeSet<String>,
        now_ms: i64,
    ) -> Result<u32, RegistryError> {
        let pending: Vec<(String, String)> = self
            .conn
            .prepare_cached(
                "SELECT id, COALESCE(target_session_id, '') FROM proposal
                 WHERE kind = ?1 AND status = ?2",
            )?
            .query_map(
                params![ProposalKind::Answer.code(), ProposalStatus::Pending.code()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?
            .collect::<rusqlite::Result<_>>()?;
        let expired = expired_answers(&pending, answerable);
        for id in &expired {
            self.decide_pending(id, ProposalStatus::Expired, now_ms)?;
        }
        Ok(u32::try_from(expired.len()).unwrap_or(u32::MAX))
    }

    /// Moves the pending proposal `id` to `status` at `now_ms`: an answer
    /// `accepted` when the author's click sent it, `expired` when its
    /// session no longer takes it (AD-182); false when it was not pending.
    pub fn decide_pending(
        &mut self,
        id: &str,
        status: ProposalStatus,
        now_ms: i64,
    ) -> Result<bool, RegistryError> {
        let changed = self.conn.execute(
            "UPDATE proposal SET status = ?2, decided_at_ms = ?3 WHERE id = ?1 AND status = ?4",
            params![id, status.code(), now_ms, ProposalStatus::Pending.code()],
        )?;
        Ok(changed == 1)
    }

    /// The stored text of `id` when it is a pending answer proposal to
    /// `session_id` (AD-182, D-575): the text the window sends, never one
    /// the page gives back; `None` for any other proposal.
    pub fn pending_answer(
        &mut self,
        id: &str,
        session_id: &str,
    ) -> Result<Option<String>, RegistryError> {
        let texts: Option<Option<String>> = self
            .conn
            .prepare_cached(
                "SELECT texts_json FROM proposal
                 WHERE id = ?1 AND kind = ?2 AND status = ?3 AND target_session_id = ?4",
            )?
            .query_row(
                params![
                    id,
                    ProposalKind::Answer.code(),
                    ProposalStatus::Pending.code(),
                    session_id
                ],
                |row| row.get(0),
            )
            .optional()?;
        Ok(texts
            .flatten()
            .and_then(|json| {
                serde_json::from_str::<std::collections::BTreeMap<String, String>>(&json).ok()
            })
            .and_then(|mut texts| texts.remove("text")))
    }

    /// Dismisses proposal `id` at `now_ms` (FR-12): a pending or invalid
    /// proposal becomes `dismissed`; false when none was.
    pub fn dismiss_proposal(&mut self, id: &str, now_ms: i64) -> Result<bool, RegistryError> {
        let changed = self.conn.execute(
            "UPDATE proposal SET status = ?2, decided_at_ms = ?3
             WHERE id = ?1 AND status IN (?4, ?5)",
            params![
                id,
                ProposalStatus::Dismissed.code(),
                now_ms,
                ProposalStatus::Pending.code(),
                ProposalStatus::Invalid.code()
            ],
        )?;
        Ok(changed == 1)
    }

    /// The pending work order proposal `id` as a card is issued from it at
    /// `now_ms` (AD-91, D-152), or why none is: unknown, a recap, past its
    /// 24 hours (expired here when it was still pending), or not pending.
    /// Its names and values are those ingestion validated and stored, the
    /// template's own strings; its rationale and suggested name stay
    /// untrusted.
    pub fn proposal_order(
        &mut self,
        id: &str,
        now_ms: i64,
    ) -> Result<Result<ProposalOrder, ProposalRefusal>, RegistryError> {
        type Row = (
            Option<String>,
            String,
            Option<i64>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        );
        let row: Option<Row> = self
            .conn
            .prepare_cached(
                "SELECT kind, status, expires_at_ms, template, project, values_json,
                     subject_choice_json, texts_json
                 FROM proposal WHERE id = ?1",
            )?
            .query_row([id], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            })
            .optional()?;
        let Some((kind, status, expires, template, project, values, subject, texts)) = row else {
            return Ok(Err(ProposalRefusal::UnknownProposal));
        };
        if kind.as_deref() != Some(ProposalKind::WorkOrder.code()) {
            return Ok(Err(ProposalRefusal::NotAWorkOrder));
        }
        let pending = status == ProposalStatus::Pending.code();
        if pending && is_expired(expires, now_ms) {
            self.expire_proposals(now_ms)?;
            return Ok(Err(ProposalRefusal::Expired));
        }
        if status == ProposalStatus::Expired.code() {
            return Ok(Err(ProposalRefusal::Expired));
        }
        if !pending {
            return Ok(Err(ProposalRefusal::NotPending));
        }
        fn parsed<T: serde::de::DeserializeOwned>(json: Option<String>) -> Option<T> {
            json.and_then(|j| serde_json::from_str(&j).ok())
        }
        let texts: Option<std::collections::BTreeMap<String, String>> = parsed(texts);
        let subject: Option<ProposedSubject> = parsed(subject);
        match (template, project, parsed(values), subject, texts) {
            (Some(template), Some(project), Some(values), Some(subject), Some(mut texts)) => {
                Ok(Ok(ProposalOrder {
                    id: id.to_owned(),
                    template,
                    project,
                    values,
                    subject: ProposedSubjectView::of(&subject),
                    rationale: Untrusted::new(texts.remove("rationale").unwrap_or_default()),
                }))
            }
            // A row that does not read back as ingestion writes it opens
            // no card.
            _ => Ok(Err(ProposalRefusal::NotPending)),
        }
    }
}
