//! `officina mcp` (AD-90, AD-91, FR-11): the orchestrator's only tools.
//! A stdio loop, newline-delimited, each line read with a bound of
//! [`MAX_LINE_BYTES`] plus one byte (D-161) and routed by core's
//! [`mcp::route`]; each tool call reads what Officina records, read-only
//! (AD-53, D-82), and answers through [`mcp::dispatch`], so every
//! untrusted field reaches the client marked (D-153).
//!
//! It writes nothing but proposal files ([`write_proposal`]), which are
//! untrusted by construction: the window validates them at ingestion. It
//! spawns nothing but `claude agents` through `spawn`'s read half, whose
//! session rule removes the orchestrator's session variables, its
//! messaging socket and token among them (D-128, D-693); it reads two of
//! its own variables, [`ENV_PROJECT_DIR`] for the start check and
//! [`ENV_SESSION_ID`] as a label, and logs none.

use std::fs;
use std::hash::{BuildHasher, Hasher, RandomState};
use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use officina_core::board::Board;
use officina_core::mcp::{
    self, CallRefusal, MAX_LINE_BYTES, McpTool, Proposed, RelayActivity, Route, SubjectsOutput,
    ToolArgs, ToolCall,
};
use officina_core::orders::briefing::{self, BriefingInput, Cursor};
use officina_core::orders::proposal::{
    self as proposal, PROPOSAL_FILE_MAX_BYTES, ProposalFile, ProposalView, file_name, id_of_file,
    proposal_id, rate_refusal, written_at,
};
use officina_core::orders::view::template_options;
use officina_core::orders::{ORCHESTRATOR_DIR, PROPOSALS_DIR};
use officina_core::registry::types::{ProposalStatus, SubjectRecord, WorkOrderRecord};
use officina_core::registry::view::IngestionView;
use officina_core::relay::view::RelayView;
use serde_json::Value;

use crate::collector::Collector;
use crate::files::{atomic_write_staged, now_ms};
use crate::paths::officina_home_from_env;
use crate::registry::commands::{read_registry, registry_view};
use crate::registry::read::Reader;
use crate::registry::store::ReaderFailure;
use crate::relay::view::read_view;

/// Claude Code's project folder for the server (E11): the start check.
pub const ENV_PROJECT_DIR: &str = "CLAUDE_PROJECT_DIR";

/// The orchestrator session's id (E11): a label written into proposal
/// files, never a credential (AD-91).
pub const ENV_SESSION_ID: &str = "CLAUDE_CODE_SESSION_ID";

/// How long one `claude agents` answer serves the board (AD-90).
pub const AGENTS_CACHE: Duration = Duration::from_secs(5);

/// `<home>/orchestrator/proposals`, where the proposal tools write.
pub fn proposals_dir(officina_home: &Path) -> PathBuf {
    officina_home.join(ORCHESTRATOR_DIR).join(PROPOSALS_DIR)
}

/// Reads one line of `input` without its newline (nor a carriage return
/// before it), keeping at most `max + 1` bytes: a longer line is read to
/// its newline and discarded past that, so its tail never reads as a line
/// of its own (D-161). `None` at the end of the input.
pub fn read_line_bounded(input: &mut impl BufRead, max: usize) -> io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    let mut read_any = false;
    loop {
        let buffer = input.fill_buf()?;
        if buffer.is_empty() {
            return Ok(read_any.then_some(line));
        }
        read_any = true;
        let (chunk, used, ended) = match buffer.iter().position(|b| *b == b'\n') {
            Some(at) => (&buffer[..at], at + 1, true),
            None => (buffer, buffer.len(), false),
        };
        let room = (max + 1).saturating_sub(line.len());
        line.extend_from_slice(&chunk[..chunk.len().min(room)]);
        input.consume(used);
        if ended {
            break;
        }
    }
    if line.len() <= max && line.last() == Some(&b'\r') {
        line.pop();
    }
    Ok(Some(line))
}

/// A random part for a proposal id, from std's per-process hashing keys
/// (no dependency), the clock and a counter.
fn nonce() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    hasher.write_u32(std::process::id());
    if let Ok(since) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        hasher.write_u128(since.as_nanos());
    }
    hasher.finish()
}

/// Writes `file` into `dir` as `<now_ms>-<16 hex>.json` with AD-10's
/// [`crate::files::atomic_write`] (a temporary name, `sync_all`, a rename), so a reader
/// never sees half a file; returns its id. A file over
/// [`PROPOSAL_FILE_MAX_BYTES`] is not written. The one writer of proposal
/// files: the tools and the restart test's helper call it.
pub fn write_proposal(dir: &Path, file: &ProposalFile, now_ms: i64) -> io::Result<String> {
    write_proposal_staged(dir, file, now_ms, &mut |_| {})
}

/// [`write_proposal`] with `before_rename` called on the temporary file,
/// written and synced, right before its rename: the restart test's stall
/// point (story 5.17, D-89).
pub fn write_proposal_staged(
    dir: &Path,
    file: &ProposalFile,
    now_ms: i64,
    before_rename: &mut dyn FnMut(&Path),
) -> io::Result<String> {
    let bytes = file.to_bytes();
    if bytes.len() > PROPOSAL_FILE_MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the proposal file would be over its bound",
        ));
    }
    loop {
        let id = proposal_id(now_ms, nonce());
        let path = dir.join(file_name(&id));
        if path.exists() {
            continue;
        }
        atomic_write_staged(&path, &bytes, before_rename)?;
        return Ok(id);
    }
}

/// The proposal files in `dir`, by id; a temporary or misnamed file is
/// none.
fn proposal_files(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut found: Vec<(String, PathBuf)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let id = id_of_file(&name)?.to_owned();
            Some((id, entry.path()))
        })
        .collect();
    found.sort();
    found
}

/// A file not ingested yet as the `proposals` tool lists it: read at most
/// [`PROPOSAL_FILE_MAX_BYTES`] plus one byte, `pending`, without content
/// when it is over that or does not parse.
fn file_view(id: &str, path: &Path) -> ProposalView {
    let mut bytes = Vec::new();
    let read = fs::File::open(path).and_then(|file| {
        file.take(PROPOSAL_FILE_MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
    });
    match read {
        Ok(len) if len <= PROPOSAL_FILE_MAX_BYTES => serde_json::from_slice::<ProposalFile>(&bytes)
            .map_or_else(
                |_| ProposalView::unreadable_file(id),
                |file| file.pending_view(id),
            ),
        _ => ProposalView::unreadable_file(id),
    }
}

/// What one call read: the registry, read-only, and the board.
struct Reading {
    opened: Result<Reader, ReaderFailure>,
    board: Board,
    ingestion: Option<IngestionView>,
}

impl Reading {
    fn reader(&self) -> Option<&Reader> {
        self.opened.as_ref().ok()
    }

    fn subjects(&self) -> Vec<SubjectRecord> {
        self.reader()
            .and_then(|r| r.subjects().ok())
            .unwrap_or_default()
    }

    fn work_orders(&self) -> Vec<WorkOrderRecord> {
        self.reader()
            .and_then(|r| r.work_orders().ok())
            .unwrap_or_default()
    }

    fn cursor(&self) -> Cursor {
        match self
            .reader()
            .and_then(|r| r.previous_orchestrator_start().ok())
            .flatten()
        {
            Some(at_ms) => Cursor::PreviousStart { at_ms },
            None => Cursor::None,
        }
    }

    fn proposal_rows(&self) -> Vec<ProposalView> {
        self.reader()
            .and_then(|r| r.proposals().ok())
            .unwrap_or_default()
    }
}

/// The server's state between two lines.
pub struct Server {
    collector: Collector,
    /// The orchestrator session's id, a label for proposal files.
    session: Option<String>,
    version: String,
    agents_read_at: Option<Instant>,
}

impl Server {
    /// A server reading through `collector`, labelling its proposals with
    /// `session`, answering `version` in `initialize`.
    pub fn new(collector: Collector, session: Option<String>, version: &str) -> Server {
        Server {
            collector,
            session,
            version: version.to_owned(),
            agents_read_at: None,
        }
    }

    fn proposals_dir(&self) -> PathBuf {
        proposals_dir(&self.collector.paths().officina_home)
    }

    /// The response to `line` at `now_ms`, `None` when nothing is
    /// answered.
    pub fn answer(&mut self, line: &[u8], now_ms: i64) -> Option<Value> {
        match mcp::route(line, &self.version) {
            Route::Call(call) => Some(self.call(call, now_ms)),
            other => other.response(),
        }
    }

    /// Opens the registry read-only and builds the board, `claude agents`
    /// asked again only past [`AGENTS_CACHE`].
    fn read(&mut self, now_ms: i64) -> Reading {
        let opened = read_registry(&mut self.collector);
        if self
            .agents_read_at
            .is_none_or(|at| at.elapsed() >= AGENTS_CACHE)
        {
            self.collector.refresh_agents(now_ms);
            self.agents_read_at = Some(Instant::now());
        }
        let board = self.collector.board(now_ms);
        let events = self.collector.paths().events_dir();
        let ingestion = opened
            .as_ref()
            .ok()
            .and_then(|reader| reader.ingestion(&events).ok());
        Reading {
            opened,
            board,
            ingestion,
        }
    }

    fn relay(&self, now_ms: i64) -> Option<RelayView> {
        read_view(self.collector.relay(), now_ms).ok()
    }

    fn call(&mut self, call: ToolCall, now_ms: i64) -> Value {
        let id = &call.id;
        let tool = call.args.tool();
        let reading = self.read(now_ms);
        let ingestion = reading.ingestion.as_ref();
        let refuse =
            |refusal: CallRefusal| mcp::refusal_response(id, refusal.code(), &refusal.reason());
        match call.args {
            ToolArgs::Briefing => {
                let relay = self.relay(now_ms);
                let (view, _) =
                    registry_view(&self.collector, &reading.opened, &reading.board, now_ms);
                let subjects = reading.subjects();
                let work_orders = reading.work_orders();
                // The rows and the files not ingested yet, newest first.
                let mut proposals = self.known_proposals(&reading);
                proposals.reverse();
                let input = BriefingInput {
                    columns: Some(&reading.board.columns),
                    approvals: &reading.board.approvals,
                    recent: relay.as_ref().map_or(&[], |r| &r.recent),
                    open_subjects: &view.subjects,
                    triage: &view.triage,
                    subjects: &subjects,
                    work_orders: &work_orders,
                    proposals: &proposals,
                };
                let cursor = reading.cursor();
                mcp::dispatch(id, tool, ingestion, || {
                    briefing::build(&input, cursor, now_ms)
                })
            }
            ToolArgs::Subjects => {
                let (view, _) =
                    registry_view(&self.collector, &reading.opened, &reading.board, now_ms);
                mcp::dispatch(id, tool, ingestion, || SubjectsOutput::of(&view))
            }
            ToolArgs::Subject { id: subject } => {
                let (view, _) =
                    registry_view(&self.collector, &reading.opened, &reading.board, now_ms);
                let archived = reading
                    .reader()
                    .and_then(|r| r.archived(Some(&reading.board)).ok())
                    .unwrap_or_default();
                match mcp::subject_by_id(&view, &archived, subject) {
                    Some(found) => mcp::dispatch(id, tool, ingestion, || found),
                    None => refuse(CallRefusal::UnknownSubject),
                }
            }
            ToolArgs::Board => mcp::dispatch(id, tool, ingestion, || &reading.board),
            ToolArgs::RelayActivity { limit } => match self.relay(now_ms) {
                Some(relay) => {
                    mcp::dispatch(id, tool, ingestion, || RelayActivity::of(&relay, limit))
                }
                None => refuse(CallRefusal::RelayUnreadable),
            },
            ToolArgs::Templates => match self.relay(now_ms) {
                Some(relay) => mcp::dispatch(id, tool, ingestion, || {
                    template_options(relay.catalogue.as_ref())
                }),
                None => refuse(CallRefusal::RelayUnreadable),
            },
            ToolArgs::Proposals { status } => {
                let known = self.known_proposals(&reading);
                mcp::dispatch(id, tool, ingestion, || {
                    proposal::listed(known, status, now_ms)
                })
            }
            ToolArgs::ProposeRecap(recap) => {
                let file = ProposalFile::recap(recap, self.session.clone(), now_ms);
                self.propose(id, tool, &reading, &file, now_ms)
            }
            ToolArgs::ProposeWorkOrder(order) => {
                let file = ProposalFile::work_order(order, self.session.clone(), now_ms);
                self.propose(id, tool, &reading, &file, now_ms)
            }
            ToolArgs::ProposeAnswer(answer) => {
                let file = ProposalFile::answer(answer, self.session.clone(), now_ms);
                self.propose(id, tool, &reading, &file, now_ms)
            }
        }
    }

    /// Every proposal known: the registry's rows and the files not
    /// ingested yet, each id once.
    fn known_proposals(&self, reading: &Reading) -> Vec<ProposalView> {
        let files = proposal_files(&self.proposals_dir())
            .iter()
            .map(|(id, path)| file_view(id, path))
            .collect();
        proposal::merged(reading.proposal_rows(), files)
    }

    /// Checks the rate limits on the files' ids and the registry's rows
    /// (AD-90), then writes `file`; answers its id or why not.
    fn propose(
        &self,
        id: &Value,
        tool: McpTool,
        reading: &Reading,
        file: &ProposalFile,
        now_ms: i64,
    ) -> Value {
        let dir = self.proposals_dir();
        let file_ids: Vec<String> = proposal_files(&dir).into_iter().map(|(id, _)| id).collect();
        let (times, pending) = limits_of(&reading.proposal_rows(), &file_ids);
        if let Some(refusal) = rate_refusal(now_ms, &times, pending) {
            return mcp::refusal_response(id, refusal.code(), &refusal.reason());
        }
        match write_proposal(&dir, file, now_ms) {
            Ok(written) => mcp::dispatch(id, tool, reading.ingestion.as_ref(), || Proposed {
                id: &written,
            }),
            Err(_) => mcp::refusal_response(
                id,
                CallRefusal::WriteFailed.code(),
                &CallRefusal::WriteFailed.reason(),
            ),
        }
    }
}

/// What the rate limits count (AD-90): when each known proposal was
/// written, from its id, the registry's `rows` and the `files` not
/// ingested yet each once; and how many wait, a file counting as pending
/// until the window gives it a status.
fn limits_of(rows: &[ProposalView], files: &[String]) -> (Vec<i64>, usize) {
    let mut known: Vec<(&str, bool)> = rows
        .iter()
        .map(|row| (row.id.as_str(), row.status == ProposalStatus::Pending))
        .collect();
    for id in files {
        if !rows.iter().any(|row| row.id == *id) {
            known.push((id, true));
        }
    }
    let times = known.iter().filter_map(|(id, _)| written_at(id)).collect();
    let pending = known.iter().filter(|(_, pending)| *pending).count();
    (times, pending)
}

/// Answers each line of `input` on `output`, one response a line, until
/// the end of the input.
pub fn serve(
    server: &mut Server,
    mut input: impl BufRead,
    mut output: impl Write,
) -> io::Result<()> {
    while let Some(line) = read_line_bounded(&mut input, MAX_LINE_BYTES)? {
        if let Some(response) = server.answer(&line, now_ms()) {
            serde_json::to_writer(&mut output, &response)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
    Ok(())
}

/// Why `officina mcp` does not serve: one line on stderr, exit 1.
#[derive(Debug, thiserror::Error)]
pub enum StartRefusal {
    #[error("no Officina home: set OFFICINA_HOME or USERPROFILE")]
    NoHome,
    #[error(
        "CLAUDE_PROJECT_DIR is not <Officina home>/orchestrator/workdir: this server serves the orchestrator session only"
    )]
    NotTheOrchestrator,
    #[error("cannot read standard input or write standard output: {0}")]
    Io(#[from] io::Error),
}

/// `officina mcp [--floor <path>]`: checks it runs as the orchestrator's
/// server, then serves stdin until it ends. The environment is read for
/// [`ENV_PROJECT_DIR`], [`ENV_SESSION_ID`] and what the collector reads
/// (the home, the floor, `OFFICINA_CLAUDE_BIN`, `OFFICINA_NO_EXCERPTS`),
/// and never printed.
pub fn run(floor: Option<&Path>, version: &str) -> Result<(), StartRefusal> {
    let home = officina_home_from_env().ok_or(StartRefusal::NoHome)?;
    let serves = std::env::var_os(ENV_PROJECT_DIR)
        .is_some_and(|dir| mcp::serves_in(&dir.to_string_lossy(), &home.to_string_lossy()));
    if !serves {
        return Err(StartRefusal::NotTheOrchestrator);
    }
    let collector = Collector::new(floor).map_err(|_| StartRefusal::NoHome)?;
    let session = std::env::var(ENV_SESSION_ID).ok().filter(|s| !s.is_empty());
    let mut server = Server::new(collector, session, version);
    serve(&mut server, io::stdin().lock(), io::stdout().lock())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor as Bytes;

    use officina_core::orders::proposal::{ProposalTarget, RecapProposal};

    use super::*;

    /// D-161: a line over the bound keeps the bound plus one byte, its
    /// tail up to the newline is discarded, and the next line reads whole.
    #[test]
    fn an_oversized_line_is_discarded_to_its_newline() {
        let max = 12;
        let mut input = Bytes::new(b"short\r\n0123456789abcdef-tail\n{\"next\":1}\nlast".to_vec());
        let mut lines = Vec::new();
        while let Some(line) = read_line_bounded(&mut input, max).unwrap() {
            lines.push(line);
        }
        assert_eq!(
            lines,
            [
                b"short".to_vec(),
                b"0123456789abc".to_vec(),
                b"{\"next\":1}".to_vec(),
                b"last".to_vec()
            ]
        );
        assert!(lines[1].len() > max, "the core sees it over the bound");
        let mut small_buffer =
            io::BufReader::with_capacity(3, Bytes::new(b"0123456789abcdef-tail\nok\n".to_vec()));
        assert_eq!(
            read_line_bounded(&mut small_buffer, max).unwrap().unwrap(),
            b"0123456789abc"
        );
        assert_eq!(
            read_line_bounded(&mut small_buffer, max).unwrap().unwrap(),
            b"ok"
        );
        assert_eq!(read_line_bounded(&mut small_buffer, max).unwrap(), None);
    }

    fn recap() -> ProposalFile {
        ProposalFile::recap(
            RecapProposal {
                target: ProposalTarget::SubjectId(1),
                done: "done".into(),
                blocking: "none".into(),
                decision_expected: "none".into(),
            },
            Some("label".into()),
            1_761_700_000_000,
        )
    }

    /// AD-91: many files in the same millisecond each get a name of their
    /// own, matching the proposal file's pattern, and no temporary file is
    /// left; each holds the file as written.
    #[test]
    fn proposal_file_names_are_unique_and_atomic() {
        let home = tempfile::tempdir().unwrap();
        let dir = proposals_dir(home.path());
        let file = recap();
        let ids: Vec<String> = (0..200)
            .map(|_| write_proposal(&dir, &file, 1_761_700_000_000).unwrap())
            .collect();
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), ids.len());
        let names: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), ids.len(), "no temporary file is left");
        for name in &names {
            let id = id_of_file(name).unwrap_or_else(|| panic!("{name}"));
            assert_eq!(written_at(id), Some(1_761_700_000_000));
            let back: ProposalFile =
                serde_json::from_slice(&fs::read(dir.join(name)).unwrap()).unwrap();
            assert_eq!(back, file);
        }
    }

    #[test]
    fn a_file_over_its_bound_is_not_written() {
        let home = tempfile::tempdir().unwrap();
        let dir = proposals_dir(home.path());
        let ProposalFile::Recap(mut file) = recap() else {
            unreachable!()
        };
        file.orchestrator_session_id = Some("x".repeat(PROPOSAL_FILE_MAX_BYTES));
        let error = write_proposal(&dir, &ProposalFile::Recap(file), 1).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(proposal_files(&dir).is_empty());
    }

    /// The `proposals` tool's view of files: a file that does not parse or
    /// is over its bound is pending without content; a temporary file is
    /// none.
    #[test]
    fn files_read_back_as_pending_views() {
        let home = tempfile::tempdir().unwrap();
        let dir = proposals_dir(home.path());
        let good = write_proposal(&dir, &recap(), 1_761_700_000_000).unwrap();
        let broken = proposal_id(1_761_700_000_001, 1);
        fs::write(dir.join(file_name(&broken)), b"{\"kind\": \"recap\"").unwrap();
        let big = proposal_id(1_761_700_000_002, 2);
        fs::write(
            dir.join(file_name(&big)),
            vec![b' '; PROPOSAL_FILE_MAX_BYTES + 1],
        )
        .unwrap();
        fs::write(dir.join(format!("{}.42.tmp", file_name(&good))), b"{}").unwrap();
        let views: Vec<ProposalView> = proposal_files(&dir)
            .iter()
            .map(|(id, path)| file_view(id, path))
            .collect();
        assert_eq!(views.len(), 3);
        assert!(views.iter().all(|v| v.status == ProposalStatus::Pending));
        let content = |id: &str| views.iter().find(|v| v.id == id).unwrap().content.is_some();
        assert!(content(&good));
        assert!(!content(&broken));
        assert!(!content(&big));
    }

    /// D-145, D-130, AD-90: the registry's proposal rows read back with
    /// their status, every text untrusted, the invalid reason included,
    /// and count toward the limits with the files not ingested yet, each
    /// id once.
    #[cfg(feature = "window-authority")]
    #[test]
    fn registry_rows_read_back_untrusted_and_count_toward_the_limits() {
        use officina_core::orders::proposal::PROPOSAL_HOUR_MS;
        use serde_json::json;

        use crate::registry::RegistryPaths;
        use crate::registry::store::{open_reader, open_writer};

        const MARKER: &str = "ROW-MARKER-4e1a";
        let home = tempfile::tempdir().unwrap();
        let now = 1_761_700_000_000;
        drop(open_writer(home.path(), &[], now).writer().unwrap());
        let conn = rusqlite::Connection::open(RegistryPaths::new(home.path()).database()).unwrap();
        let recap = proposal_id(now - 1_000, 1);
        let order = proposal_id(now - 2_000, 2);
        let invalid = proposal_id(now - 3_000, 3);
        let old = proposal_id(now - 2 * PROPOSAL_HOUR_MS, 4);
        let insert = "INSERT INTO proposal (id, kind, status, invalid_reason, received_at_ms,
             orchestrator_session_id, target_subject_id, target_session_id, template, project,
             values_json, subject_choice_json, texts_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)";
        let texts = json!({"done": MARKER, "blocking": "b", "decision_expected": "e"}).to_string();
        conn.execute(
            insert,
            rusqlite::params![
                recap,
                "recap",
                "pending",
                None::<String>,
                now,
                "s-1",
                4,
                None::<String>,
                None::<String>,
                None::<String>,
                None::<String>,
                None::<String>,
                texts
            ],
        )
        .unwrap();
        conn.execute(
            insert,
            rusqlite::params![
                order,
                "work_order",
                "pending",
                None::<String>,
                now,
                "s-1",
                None::<i64>,
                None::<String>,
                "probe-notes",
                "lab",
                json!({"host": MARKER}).to_string(),
                json!({"new": MARKER}).to_string(),
                json!({"rationale": "r"}).to_string()
            ],
        )
        .unwrap();
        conn.execute(
            insert,
            rusqlite::params![
                invalid,
                None::<String>,
                "invalid",
                MARKER,
                now,
                None::<String>,
                None::<i64>,
                None::<String>,
                None::<String>,
                None::<String>,
                None::<String>,
                None::<String>,
                None::<String>
            ],
        )
        .unwrap();
        conn.execute(
            insert,
            rusqlite::params![
                old,
                "recap",
                "accepted",
                None::<String>,
                now,
                None::<String>,
                4,
                None::<String>,
                None::<String>,
                None::<String>,
                None::<String>,
                None::<String>,
                None::<String>
            ],
        )
        .unwrap();
        let rows = open_reader(home.path(), &[]).unwrap().proposals().unwrap();
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                recap.as_str(),
                order.as_str(),
                invalid.as_str(),
                old.as_str()
            ]
        );
        assert!(rows[0].content.is_some() && rows[1].content.is_some());
        assert!(rows[2].content.is_none() && rows[3].content.is_none());
        let marked = officina_core::untrusted::wrapped(|| serde_json::to_value(&rows)).unwrap();
        assert_eq!(
            marked[0]["content"]["recap"]["done"],
            json!({"untrusted": MARKER})
        );
        assert_eq!(
            marked[1]["content"]["work_order"]["values"],
            json!({"untrusted": {"host": MARKER}})
        );
        assert_eq!(
            marked[1]["content"]["work_order"]["subject"],
            json!({"new": {"untrusted": MARKER}})
        );
        assert_eq!(marked[2]["invalid_reason"], json!({"untrusted": MARKER}));
        assert_eq!(marked[2]["status"], "invalid");
        let fresh = proposal_id(now, 5);
        let (times, pending) = limits_of(&rows, &[recap.clone(), fresh]);
        assert_eq!(times.len(), 5, "each id once");
        assert_eq!(
            times
                .iter()
                .filter(|t| **t > now - PROPOSAL_HOUR_MS)
                .count(),
            4
        );
        assert_eq!(pending, 3, "two pending rows and the file not ingested");
    }
}
