//! Ingesting the event files into the registry (AD-43, D-49, D-52, D-79,
//! D-84, D-97): every pass covers every incomplete file, oldest first, lines
//! in file order, one transaction per chunk; each line creates its session
//! at first sight, stores its excerpt and its facts, and links a session
//! `/clear` or `/resume` replaced. Compiled only with the
//! `window-authority` feature: only the window writes.

use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

use officina_core::board::{ProjectRule, project_for};
use officina_core::contract::HookSightings;
use officina_core::dates::dated_file_day;
use officina_core::events::is_subagent_event;
use officina_core::events::{EventLine, ParsedLine, parse_line};
use officina_core::registry::confirm::wake;
use officina_core::registry::filing::filing_for_new_session;
use officina_core::registry::fold::{candidates_needed, line_facts, link_for_line};
use officina_core::registry::ingestion::{
    ExcerptObservation, FileOpening, close_event_file, excerpt_observation, in_review_workdir,
    in_triage_workdir, line_too_long, open_event_file, recorded_at, starts_a_chunk,
};
use officina_core::registry::retention::excerpt_expired;
use officina_core::registry::types::{EventFileRecord, SessionRecord};
use rusqlite::{Connection, TransactionBehavior, params};

use super::orders::bound_subject_of;
use super::read;
use super::store::Writer;
use super::write::{RegistryError, apply_wake, prune_excerpts, write_fact};

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
    /// What the lines read said about the hook payloads (AD-56): the
    /// window folds it into the running version's contract record.
    pub hook_sightings: HookSightings,
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

/// One complete line of an event file, its newline included: its bytes,
/// or only its length when it is longer than the hook ever writes, so a
/// line of any size costs no more memory than the hook's bound.
#[derive(Debug, PartialEq, Eq)]
enum RawLine {
    Bytes(Vec<u8>),
    TooLong(usize),
}

impl RawLine {
    fn len(&self) -> usize {
        match self {
            RawLine::Bytes(bytes) => bytes.len(),
            RawLine::TooLong(len) => *len,
        }
    }
}

/// Reads an event file from an offset, one complete line at a time, in
/// the buffer's windows rather than the whole rest of the file at once
/// (quality audit H3); what follows the last newline is the unfinished
/// line, whose length [`LineReader::partial`] gives at the end.
struct LineReader<R> {
    input: R,
    pending: Vec<u8>,
    pending_len: usize,
    too_long: bool,
}

impl<R: BufRead> LineReader<R> {
    fn new(input: R) -> LineReader<R> {
        LineReader {
            input,
            pending: Vec::new(),
            pending_len: 0,
            too_long: false,
        }
    }

    /// The next complete line; `None` at the end of the input.
    fn next_line(&mut self) -> std::io::Result<Option<RawLine>> {
        loop {
            let window = self.input.fill_buf()?;
            if window.is_empty() {
                return Ok(None);
            }
            let (taken, ends) = match window.iter().position(|b| *b == b'\n') {
                Some(at) => (at + 1, true),
                None => (window.len(), false),
            };
            self.pending_len += taken;
            if !self.too_long && !line_too_long(self.pending_len) {
                self.pending.extend_from_slice(&window[..taken]);
            } else {
                self.too_long = true;
                self.pending.clear();
            }
            self.input.consume(taken);
            if ends {
                let line = if self.too_long {
                    RawLine::TooLong(self.pending_len)
                } else {
                    RawLine::Bytes(std::mem::take(&mut self.pending))
                };
                self.pending.clear();
                self.pending_len = 0;
                self.too_long = false;
                return Ok(Some(line));
            }
        }
    }

    /// The length of the unfinished line after the last newline read.
    fn partial(&self) -> u64 {
        self.pending_len as u64
    }
}

/// The lines of `path` from `offset`, read as [`LineReader`] does.
fn read_lines_from(path: &Path, offset: u64) -> std::io::Result<LineReader<BufReader<fs::File>>> {
    let mut file = fs::File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    Ok(LineReader::new(BufReader::with_capacity(64 * 1024, file)))
}

/// The line `raw` holds: a line longer than the hook writes is corrupt
/// whatever it holds.
fn parse_raw(raw: &RawLine) -> ParsedLine {
    match raw {
        RawLine::Bytes(bytes) => parse_line(&bytes[..bytes.len() - 1]),
        RawLine::TooLong(_) => ParsedLine::Corrupt,
    }
}

/// Records the session of `session_id`, seen at `seen_ms` in `cwd`: the one
/// function that creates a session record, whichever source sees it first
/// (D-79), inside the caller's transaction. A new session's project comes
/// from its `cwd` and the window's floor (D-53), and is never rewritten.
/// It is filed in the same transaction (AD-43, AD-48, D-75): under the
/// subject of the work order bound to it, source `author`, first (AD-81
/// step 5); else under the single active subject of its project, source
/// `rules`. Returns true when the session is new.
pub(crate) fn ensure_session(
    conn: &Connection,
    session_id: &str,
    cwd: Option<&str>,
    seen_ms: i64,
    projects: &[ProjectRule],
) -> rusqlite::Result<bool> {
    let project = project_for(cwd, projects).name;
    let created = conn
        .prepare_cached(
            "INSERT INTO session (session_id, project, first_seen_ms, last_seen_ms)
             VALUES (?1, ?2, ?3, ?3) ON CONFLICT (session_id) DO NOTHING",
        )?
        .execute(params![session_id, project, seen_ms])?
        == 1;
    if created {
        let filing = filing_for_new_session(
            bound_subject_of(conn, session_id)?,
            &project,
            &read::active_subjects_of(conn, &project)?,
        );
        if let Some((subject, src)) = filing {
            conn.prepare_cached(
                "UPDATE session SET subject_id = ?2, subject_src = ?3, subject_at_ms = ?4
                 WHERE session_id = ?1",
            )?
            .execute(params![session_id, subject, src.code(), seen_ms])?;
        }
    } else {
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
    /// chunk's transaction. Where a file starts and how it ends are core's
    /// [`open_event_file`] and [`close_event_file`]; this reads and writes.
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
            let row = stored.remove(&name).unwrap_or_else(|| EventFileRecord {
                name: name.clone(),
                ..EventFileRecord::default()
            });
            let path = ctx.events_dir.join(&name);
            // Only a file is read; one gone since the listing (an older
            // `board --json` deleting it) waits for nothing. Neither may stop
            // the pass, or every later file would wait with it.
            let Some(meta) = fs::metadata(&path).ok().filter(|m| m.is_file()) else {
                continue;
            };
            let FileOpening::Read { mut row, rewritten } = open_event_file(&row, meta.len()) else {
                continue;
            };
            if rewritten {
                report.rewritten.push(name.clone());
                // Recorded before the read, so a file gone in between
                // still counts its rewrite.
                store_offset(&self.conn, &row)?;
            }
            let mut lines = match read_lines_from(&path, row.ingested_bytes) {
                Ok(lines) => lines,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(RegistryError::io(&path, e)),
            };
            let mut chunk: Vec<RawLine> = Vec::new();
            let mut chunk_bytes = 0;
            loop {
                let next = lines.next_line().map_err(|e| RegistryError::io(&path, e))?;
                let full = next
                    .as_ref()
                    .is_none_or(|line| starts_a_chunk(chunk.len(), chunk_bytes, line.len()));
                if full && !chunk.is_empty() {
                    self.apply_chunk(
                        &chunk,
                        &mut row,
                        ctx,
                        now_ms,
                        &mut report,
                        &mut read_so_far,
                        observe,
                    )?;
                    chunk.clear();
                    chunk_bytes = 0;
                }
                let Some(line) = next else {
                    break;
                };
                chunk_bytes += line.len();
                chunk.push(line);
            }
            let closing = close_event_file(&name, lines.partial(), now_ms);
            if closing.tail_corrupt {
                row.ingested_bytes += lines.partial();
                row.corrupt_lines += 1;
                report.corrupt_lines += 1;
            }
            if closing.complete {
                row.complete_at_ms = Some(now_ms);
                report.completed.push(name.clone());
            }
            if closing.tail_corrupt || closing.complete {
                store_offset(&self.conn, &row)?;
            }
        }
        Ok(report)
    }

    /// Applies one chunk in its own transaction (AD-43): each line, the
    /// corrupt ones counted, then `row`'s new offset, committed together,
    /// so a crash replays the chunk whole.
    #[allow(clippy::too_many_arguments)] // The pass's state, threaded through one chunk.
    fn apply_chunk(
        &mut self,
        chunk: &[RawLine],
        row: &mut EventFileRecord,
        ctx: &IngestContext,
        now_ms: i64,
        report: &mut IngestReport,
        read_so_far: &mut u64,
        observe: LineObserver<'_>,
    ) -> Result<(), RegistryError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for raw in chunk {
            match parse_raw(raw) {
                ParsedLine::Event(mut line) => {
                    line.ts = recorded_at(line.ts, &row.name, now_ms);
                    report.hook_sightings.observe(&line);
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
            *read_so_far += 1;
            observe(*read_so_far);
        }
        store_offset(&tx, row)?;
        tx.commit()?;
        Ok(())
    }
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
    // A sub-agent's line describes no fact of its session (AD-176), and
    // whose session its id names is what Slice 0's gate 1 checks: it never
    // creates or touches a session record.
    if is_subagent_event(&line.event) {
        return Ok(false);
    }
    let home = ctx.officina_home.to_string_lossy();
    if line
        .cwd
        .as_deref()
        .is_some_and(|cwd| in_triage_workdir(cwd, &home) || in_review_workdir(cwd, &home))
    {
        return Ok(false);
    }
    ensure_session(conn, session_id, line.cwd.as_deref(), line.ts, ctx.projects)?;
    // Only a filed session can wake its subject or crowd out another's
    // excerpts; an unfiled one costs one lookup.
    let filed = read::subject_id_of(conn, session_id)?.is_some();
    let before = if filed && line.event == "UserPromptSubmit" {
        read::session_record(conn, session_id)?.filter(SessionRecord::is_ended)
    } else {
        None
    };
    if !ctx.no_excerpts && !excerpt_expired(line.ts, now_ms) {
        store_excerpt(conn, session_id, line)?;
    }
    for fact in line_facts(line) {
        write_fact(conn, &fact)?;
    }
    // The line's own facts are stored first, so the other line of a
    // supersession finds this one whichever comes first (D-52).
    if let Some(query) = candidates_needed(line)
        && let Some(link) = link_for_line(line, &read::marks(conn, query)?)
    {
        write_fact(conn, &link)?;
    }
    let excerpts_moved = excerpt_observation(line).is_some() || line.event == "SessionEnd";
    if filed && (before.is_some() || excerpts_moved) {
        let Some(after) = read::session_record(conn, session_id)? else {
            return Ok(true);
        };
        // D-77: a prompt newer than its end wakes the subject.
        if let Some(before) = &before
            && let Some(wake) = wake(before, &after, &read::subjects(conn)?)
        {
            apply_wake(conn, &wake, line.ts)?;
        }
        // D-50: new excerpts, or an end that shows the session was one the
        // author drove, leave the subject the latest only.
        if excerpts_moved && let Some(subject) = after.filed_subject() {
            prune_excerpts(conn, subject)?;
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use officina_core::events::MAX_LINE_BYTES;

    const NL: u8 = 10;

    fn lines_of(bytes: &[u8], window: usize) -> (Vec<RawLine>, u64) {
        let mut reader = LineReader::new(BufReader::with_capacity(window, bytes));
        let mut lines = Vec::new();
        while let Some(line) = reader.next_line().unwrap() {
            lines.push(line);
        }
        (lines, reader.partial())
    }

    fn with_newline(text: &str) -> Vec<u8> {
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(NL);
        bytes
    }

    /// Lines end at their newline whatever the read windows, and the
    /// unfinished one after the last newline is left as its length.
    #[test]
    fn lines_are_cut_at_their_newline_whatever_the_windows() {
        let input: Vec<u8> = [
            with_newline("a"),
            with_newline("bc"),
            with_newline(""),
            b"d".to_vec(),
        ]
        .concat();
        for window in [1, 2, 3, 64 * 1024] {
            let (lines, partial) = lines_of(&input, window);
            assert_eq!(
                lines,
                [
                    RawLine::Bytes(with_newline("a")),
                    RawLine::Bytes(with_newline("bc")),
                    RawLine::Bytes(with_newline(""))
                ],
                "window {window}"
            );
            assert_eq!(partial, 1);
        }
        assert_eq!(lines_of(b"", 8), (Vec::new(), 0));
    }

    /// Security audit M2, quality audit H3: a line longer than the hook
    /// writes is kept as its length only, and the next line is whole.
    #[test]
    fn an_overlong_line_is_kept_as_its_length() {
        let input = [
            with_newline(&"x".repeat(MAX_LINE_BYTES * 3)),
            with_newline("ok"),
        ]
        .concat();
        let (lines, partial) = lines_of(&input, 4096);
        assert_eq!(
            lines,
            [
                RawLine::TooLong(MAX_LINE_BYTES * 3 + 1),
                RawLine::Bytes(with_newline("ok"))
            ]
        );
        assert_eq!(partial, 0);
    }
}
