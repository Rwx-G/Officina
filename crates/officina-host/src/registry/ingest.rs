//! Ingesting the event files into the registry (AD-43, D-49, D-79, D-84,
//! D-97): every pass covers every incomplete file, oldest first, lines in
//! file order, one transaction per chunk. Compiled only with the
//! `window-authority` feature: only the window writes.

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use officina_core::board::{ProjectRule, project_for};
use officina_core::dates::dated_file_day;
use officina_core::events::{EventLine, ParsedLine, parse_line};
use officina_core::registry::ingestion::{
    CHUNK_MAX_BYTES, CHUNK_MAX_LINES, ExcerptObservation, complete_once_read, excerpt_observation,
    in_triage_workdir, partial_tail_is_corrupt,
};
use officina_core::registry::retention::excerpt_expired;
use officina_core::registry::types::EventFileRecord;
use rusqlite::{Connection, TransactionBehavior, params};

use super::read;
use super::store::Writer;
use super::write::RegistryError;

/// What a pass needs besides the registry.
#[derive(Debug, Clone, Copy)]
pub struct IngestContext<'a> {
    /// The event files' folder.
    pub events_dir: &'a Path,
    /// The Officina home, whose `triage/` folder's lines are skipped.
    pub officina_home: &'a Path,
    /// The window's floor's projects, which name a new session's project
    /// (D-53).
    pub projects: &'a [ProjectRule],
    /// No excerpt is stored: the environment or the window's floor says so.
    pub no_excerpts: bool,
}

/// What one pass did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IngestReport {
    /// Event lines applied.
    pub lines: u64,
    /// Lines newly counted corrupt.
    pub corrupt_lines: u64,
    /// Files found shorter than their offset, read again from their start
    /// (D-84).
    pub rewritten: Vec<String>,
    /// Files found complete in this pass.
    pub completed: Vec<String>,
}

/// A call made after each line a chunk's transaction read, with the count
/// of lines read in the pass so far: the crash test stalls there.
pub type LineObserver<'a> = &'a mut dyn FnMut(u64);

/// The event files of `dir`, valid names only, oldest first.
fn event_file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| dated_file_day(name).is_some())
        .collect();
    names.sort();
    names
}

/// The bytes of `path` from `offset` to its end.
fn read_from(path: &Path, offset: u64) -> std::io::Result<Vec<u8>> {
    let mut file = fs::File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Records the session of `session_id`, seen at `seen_ms` in `cwd`: the one
/// function that creates a session record, whichever source sees it first
/// (D-79), inside the caller's transaction. A new session's project comes
/// from its `cwd` and the window's floor (D-53), and is never rewritten.
/// Returns true when the session is new.
pub(crate) fn ensure_session(
    conn: &Connection,
    session_id: &str,
    cwd: Option<&str>,
    seen_ms: i64,
    projects: &[ProjectRule],
) -> rusqlite::Result<bool> {
    let created =
        conn.prepare_cached(
            "INSERT INTO session (session_id, project, first_seen_ms, last_seen_ms)
             VALUES (?1, ?2, ?3, ?3) ON CONFLICT (session_id) DO NOTHING",
        )?
        .execute(params![
            session_id,
            project_for(cwd, projects).name,
            seen_ms
        ])? == 1;
    if !created {
        conn.prepare_cached(
            "UPDATE session SET first_seen_ms = min(first_seen_ms, ?2), last_seen_ms = max(last_seen_ms, ?2)
             WHERE session_id = ?1",
        )?
        .execute(params![session_id, seen_ms])?;
    }
    Ok(created)
}

/// Stores the excerpt `line` carries, replacing the stored one only when
/// the line is newer (AD-43: whatever the order lines arrive in).
fn store_excerpt(conn: &Connection, session_id: &str, line: &EventLine) -> rusqlite::Result<()> {
    match excerpt_observation(line) {
        Some(ExcerptObservation::Prompt { text }) => {
            conn.prepare_cached(
                "INSERT INTO session_excerpt (session_id, prompt, prompt_at_ms) VALUES (?1, ?2, ?3)
                 ON CONFLICT (session_id) DO UPDATE SET prompt = excluded.prompt, prompt_at_ms = excluded.prompt_at_ms
                 WHERE session_excerpt.prompt_at_ms IS NULL OR excluded.prompt_at_ms > session_excerpt.prompt_at_ms",
            )?
            .execute(params![session_id, text, line.ts])?;
        }
        Some(ExcerptObservation::Answer { head, tail }) => {
            conn.prepare_cached(
                "INSERT INTO session_excerpt (session_id, answer_head, answer_tail, answer_at_ms) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (session_id) DO UPDATE SET answer_head = excluded.answer_head,
                     answer_tail = excluded.answer_tail, answer_at_ms = excluded.answer_at_ms
                 WHERE session_excerpt.answer_at_ms IS NULL OR excluded.answer_at_ms > session_excerpt.answer_at_ms",
            )?
            .execute(params![session_id, head, tail, line.ts])?;
        }
        None => {}
    }
    Ok(())
}

fn store_offset(conn: &Connection, row: &EventFileRecord) -> rusqlite::Result<()> {
    conn.prepare_cached(
        "INSERT INTO event_file (name, ingested_bytes, complete_at_ms, corrupt_lines, rewrites)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (name) DO UPDATE SET ingested_bytes = excluded.ingested_bytes,
             complete_at_ms = excluded.complete_at_ms, corrupt_lines = excluded.corrupt_lines,
             rewrites = excluded.rewrites",
    )?
    .execute(params![
        row.name,
        i64::try_from(row.ingested_bytes).unwrap_or(i64::MAX),
        row.complete_at_ms,
        row.corrupt_lines,
        row.rewrites
    ])?;
    Ok(())
}

/// The complete lines of `bytes` (each with its `\n`), and the length of
/// the unfinished line after them.
fn complete_lines(bytes: &[u8]) -> (Vec<&[u8]>, usize) {
    let mut lines = Vec::new();
    let mut start = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            lines.push(&bytes[start..=index]);
            start = index + 1;
        }
    }
    (lines, bytes.len() - start)
}

impl Writer {
    /// One ingestion pass (AD-43, D-84): every incomplete event file of
    /// `ctx.events_dir`, oldest first.
    pub fn ingest(
        &mut self,
        ctx: &IngestContext,
        now_ms: i64,
    ) -> Result<IngestReport, RegistryError> {
        self.ingest_observed(ctx, now_ms, &mut |_| {})
    }

    /// [`Writer::ingest`], calling `observe` after each line read inside a
    /// chunk's transaction.
    pub fn ingest_observed(
        &mut self,
        ctx: &IngestContext,
        now_ms: i64,
        observe: LineObserver<'_>,
    ) -> Result<IngestReport, RegistryError> {
        let mut report = IngestReport::default();
        let mut stored: std::collections::BTreeMap<String, EventFileRecord> =
            read::event_files(&self.conn)?
                .into_iter()
                .map(|row| (row.name.clone(), row))
                .collect();
        let mut read_so_far = 0u64;
        for name in event_file_names(ctx.events_dir) {
            let mut row = stored.remove(&name).unwrap_or_else(|| EventFileRecord {
                name: name.clone(),
                ..EventFileRecord::default()
            });
            if row.complete_at_ms.is_some() {
                continue;
            }
            let path = ctx.events_dir.join(&name);
            // Only a file is read; one gone since the listing (an older
            // `board --json` deleting it) waits for nothing. Neither may stop
            // the pass, or every later file would wait with it.
            let Some(meta) = fs::metadata(&path).ok().filter(|m| m.is_file()) else {
                continue;
            };
            let mut changed = false;
            if meta.len() < row.ingested_bytes {
                row.ingested_bytes = 0;
                row.corrupt_lines = 0;
                row.rewrites += 1;
                report.rewritten.push(name.clone());
                changed = true;
            }
            let bytes = match read_from(&path, row.ingested_bytes) {
                Ok(bytes) => bytes,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(RegistryError::io(&path, e)),
            };
            let (lines, partial) = complete_lines(&bytes);
            for chunk in chunks(&lines) {
                let tx = self
                    .conn
                    .transaction_with_behavior(TransactionBehavior::Immediate)?;
                for raw in chunk {
                    match parse_line(&raw[..raw.len() - 1]) {
                        ParsedLine::Event(line) => {
                            if apply(&tx, &line, ctx, now_ms)? {
                                report.lines += 1;
                            }
                        }
                        ParsedLine::Corrupt => {
                            row.corrupt_lines += 1;
                            report.corrupt_lines += 1;
                        }
                        ParsedLine::Blank | ParsedLine::OtherVersion => {}
                    }
                    row.ingested_bytes += raw.len() as u64;
                    read_so_far += 1;
                    observe(read_so_far);
                }
                store_offset(&tx, &row)?;
                tx.commit()?;
                changed = false;
            }
            if partial > 0 && partial_tail_is_corrupt(&name, now_ms) {
                row.ingested_bytes += partial as u64;
                row.corrupt_lines += 1;
                report.corrupt_lines += 1;
                changed = true;
            }
            let fully_read = partial == 0 || partial_tail_is_corrupt(&name, now_ms);
            if fully_read && complete_once_read(&name, now_ms) {
                row.complete_at_ms = Some(now_ms);
                report.completed.push(name.clone());
                changed = true;
            }
            if changed {
                store_offset(&self.conn, &row)?;
            }
        }
        Ok(report)
    }
}

/// `lines` cut into chunks of at most [`CHUNK_MAX_LINES`] lines and
/// [`CHUNK_MAX_BYTES`] bytes; a longer line is a chunk of its own.
fn chunks<'a>(lines: &[&'a [u8]]) -> Vec<Vec<&'a [u8]>> {
    let mut chunks: Vec<Vec<&[u8]>> = Vec::new();
    let mut bytes = 0;
    for line in lines {
        let full = chunks.last().is_none_or(|last| {
            last.len() >= CHUNK_MAX_LINES || bytes + line.len() > CHUNK_MAX_BYTES
        });
        if full {
            chunks.push(Vec::new());
            bytes = 0;
        }
        if let Some(last) = chunks.last_mut() {
            last.push(line);
        }
        bytes += line.len();
    }
    chunks
}

/// Applies one event line; returns false for a line that records nothing:
/// no session id, or a `cwd` in the triage workdir (AD-43, D-97).
fn apply(
    conn: &Connection,
    line: &EventLine,
    ctx: &IngestContext,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    let Some(session_id) = line.session_id.as_deref().filter(|id| !id.is_empty()) else {
        return Ok(false);
    };
    let home = ctx.officina_home.to_string_lossy();
    if line
        .cwd
        .as_deref()
        .is_some_and(|cwd| in_triage_workdir(cwd, &home))
    {
        return Ok(false);
    }
    ensure_session(conn, session_id, line.cwd.as_deref(), line.ts, ctx.projects)?;
    if !ctx.no_excerpts && !excerpt_expired(line.ts, now_ms) {
        store_excerpt(conn, session_id, line)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_cut_at_their_newline_and_chunked_by_count_and_size() {
        let (lines, partial) = complete_lines(b"a\nbc\n\nd");
        assert_eq!(lines, [&b"a\n"[..], b"bc\n", b"\n"]);
        assert_eq!(partial, 1);
        assert_eq!(complete_lines(b"").0.len(), 0);

        let line = b"x\n".as_slice();
        let many = vec![line; CHUNK_MAX_LINES + 1];
        let cut = chunks(&many);
        assert_eq!(
            cut.iter().map(Vec::len).collect::<Vec<_>>(),
            [CHUNK_MAX_LINES, 1]
        );

        let big = vec![b'y'; CHUNK_MAX_BYTES / 2 + 1];
        let wide = [big.as_slice(), big.as_slice(), big.as_slice()];
        assert_eq!(chunks(&wide).len(), 3, "a chunk stays within its bytes");
        let huge = vec![b'z'; CHUNK_MAX_BYTES + 1];
        assert_eq!(
            chunks(&[huge.as_slice()]).len(),
            1,
            "a longer line is a chunk of its own"
        );
    }
}
