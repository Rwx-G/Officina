//! The meeting (FR-5, AD-84 as D-566 reads it, D-138, D-154, D-168,
//! D-169): what the author sees when he receives an employee that waits on
//! him. [`build`] makes the [`MeetingView`] from what the host gathered on
//! Receive (the card, the latest `Notification` message, the journal's
//! lines back to the session's start, its pending approvals, what `git` and
//! `claude logs` printed); [`delivery`] says how an answer would reach the
//! session.
//!
//! No input carries a transcript path or its content (framing principle
//! 3): every text comes from the board, the journal, `git` or `claude
//! logs`, and every session text, commit subject, path and log line is
//! [`Untrusted`].

use serde::{Deserialize, Serialize};

use crate::agents::LifecycleState;
use crate::board::{Badge, Card, Column, SessionKindView};
use crate::dates::{DAY_MS, event_file_name, utc_date};
use crate::registry::types::{ExcerptRecord, SessionRecord, Source};
use crate::relay::approvals::ApprovalView;
use crate::relay::journal::{JournalEntry, JournalLine};
use crate::untrusted::Untrusted;

/// The meeting view's format; a field change bumps it.
pub const MEETING_VIEW_SCHEMA: u32 = 1;

/// The most diff text a meeting shows (AD-84): the diff is cut at the last
/// whole line within it, and says so.
pub const DIFF_CAP_BYTES: usize = 256 * 1024;

/// The most of `claude logs` a meeting keeps once its escape sequences are
/// stripped: the end of the output, where the session's latest turn is.
pub const LOGS_KEPT_BYTES: usize = 64 * 1024;

/// How far back the meeting reads the journal for a session's runs: the
/// journal's own retention (D-169).
pub const RUNS_MAX_DAYS: i64 = crate::relay::journal::JOURNAL_RETENTION_DAYS;

/// The most untracked files a meeting lists by name and size; never read.
pub const UNTRACKED_LISTED: usize = 200;

/// What marks a line of `claude logs` as an error, the panel's jump going
/// to the first line holding one: the words Claude Code's rendered screen
/// prints for a failed tool call or API call, and a program's usual error
/// and panic headers.
pub const LOG_ERROR_MARKERS: &[&str] = &[
    "API Error",
    "Error:",
    "error:",
    "ERROR",
    "FAILED",
    "panicked at",
    "Traceback (most recent call last)",
];

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// How an answer reaches the session (AD-84, AD-85, D-139, D-168).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum Delivery {
        /// A live background session not waiting: `claude --resume` sends
        /// the answer as its next turn.
        ResumeLive,
        /// An ended session in the folder `claude agents` reported for it:
        /// `claude --resume` there.
        ResumeEnded,
        /// The answer is copied and `claude attach` opens: Claude Code
        /// sends no prompt to a session waiting on a question or a dialog.
        CopyAndAttach,
        /// An ended interactive session no `claude agents` folder is known
        /// for: the answer is copied, nothing is spawned, and the author
        /// resumes it by hand in its folder.
        CopyOnly,
        /// A live interactive session: Focus, no composer.
        FocusOnly,
    }
}

/// What the meeting knows of the session it receives: its board card's
/// fields, or, for a session the board no longer shows, what the
/// registry holds of it (D-138).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeetingCard {
    pub key: String,
    pub name: Untrusted<String>,
    pub kind: Option<SessionKindView>,
    pub column: Column,
    pub badge: Option<Badge>,
    pub session_id: Option<String>,
    pub background_id: Option<String>,
    pub pid: Option<u32>,
    pub waiting_for: Option<String>,
    pub last_prompt: Option<Untrusted<String>>,
    pub last_message: Option<Untrusted<String>>,
    pub last_message_tail: Option<Untrusted<String>>,
    pub started_at_ms: Option<i64>,
}

impl From<&Card> for MeetingCard {
    fn from(card: &Card) -> MeetingCard {
        MeetingCard {
            key: card.key.clone(),
            name: card.name.clone(),
            kind: card.kind,
            column: card.column,
            badge: card.badge,
            session_id: card.session_id.clone(),
            background_id: card.background_id.clone(),
            pid: card.pid,
            waiting_for: card.waiting_for.clone(),
            last_prompt: card.last_prompt.clone(),
            last_message: card.last_message.clone(),
            last_message_tail: card.last_message_tail.clone(),
            started_at_ms: card.started_at_ms,
        }
    }
}

impl MeetingCard {
    /// The meeting card of a session the board no longer shows, from its
    /// registry record and excerpts, under the key `sid:<session id>`
    /// (D-138): an ended session, its excerpts left out with
    /// `no_excerpts`.
    pub fn from_record(
        record: &SessionRecord,
        excerpt: Option<&ExcerptRecord>,
        no_excerpts: bool,
    ) -> MeetingCard {
        let state = record.state.as_ref().map(|s| s.value);
        let badge = if record.ended.is_some() {
            Some(Badge::Ended)
        } else if state == Some(LifecycleState::Stopped) {
            Some(Badge::Stopped)
        } else {
            Some(Badge::Interrupted)
        };
        let text = |value: Option<&String>| {
            (!no_excerpts)
                .then(|| value.map(|v| Untrusted::new(v.clone())))
                .flatten()
        };
        MeetingCard {
            key: format!("sid:{}", record.session_id),
            name: Untrusted::new(
                record
                    .name
                    .as_ref()
                    .map_or_else(|| record.project.clone(), |n| n.value.clone()),
            ),
            kind: record.kind.as_ref().map(|k| k.value.into()),
            column: Column::ToResume,
            badge,
            session_id: Some(record.session_id.clone()),
            background_id: record.background_id.as_ref().map(|b| b.value.clone()),
            pid: None,
            waiting_for: None,
            last_prompt: text(excerpt.and_then(|e| e.prompt.as_ref())),
            last_message: text(excerpt.and_then(|e| e.answer_head.as_ref())),
            last_message_tail: text(excerpt.and_then(|e| e.answer_tail.as_ref())),
            started_at_ms: Some(record.first_seen_ms),
        }
    }
}

/// The folder `claude agents` reported for `record`'s session, from the
/// registry's `cwd` fact when that fact's source is `claude agents`
/// (D-168); a folder an event line named is never one.
pub fn agents_folder_of(record: &SessionRecord) -> Option<&str> {
    record
        .cwd
        .as_ref()
        .filter(|cwd| cwd.src == Source::Agents)
        .map(|cwd| cwd.value.as_str())
}

/// The session of `subject` the meeting opens (D-76, D-138): its latest
/// one that saw a prompt.
pub fn latest_prompted(records: &[SessionRecord], subject: i64) -> Option<&str> {
    records
        .iter()
        .filter(|r| r.subject.as_ref().is_some_and(|s| s.value == Some(subject)))
        .filter_map(|r| r.last_prompt_at.as_ref().map(|at| (at.value, r)))
        .max_by_key(|(at, r)| (*at, r.session_id.as_str()))
        .map(|(_, r)| r.session_id.as_str())
}

/// What [`delivery`] decides from: the card, and the folder `claude
/// agents` reported for its session, if any (D-168). A folder an event
/// line names is never one: any session can append event lines.
#[derive(Debug, Clone, Copy)]
pub struct MeetingTarget<'a> {
    pub card: &'a MeetingCard,
    pub agents_folder: Option<&'a str>,
}

/// True when `card` is a background session.
fn is_background(card: &MeetingCard) -> bool {
    card.kind == Some(SessionKindView::Background)
}

/// True when the session behind `card` has ended: an interactive one ended
/// or interrupted, a background one stopped or failed. A `done` background
/// session with a pid is live and idle (E1), and a background session in
/// Done without one has ended.
pub fn has_ended(card: &MeetingCard) -> bool {
    if matches!(
        card.badge,
        Some(Badge::Stopped | Badge::Interrupted | Badge::Ended)
    ) {
        return true;
    }
    match card.column {
        Column::ToResume => true,
        Column::Done => !(is_background(card) && card.pid.is_some()),
        Column::NeedsYou | Column::YourTurn | Column::Working => false,
    }
}

/// True when a background session waits on the author: on a question, a
/// permission prompt or a dialog (`waiting`, `blocked`, or `waitingFor`
/// present), where Claude Code sends no prompt (S3, S4).
fn waits(card: &MeetingCard) -> bool {
    card.column == Column::NeedsYou || card.waiting_for.is_some()
}

/// How an answer would reach the session of `target` (AD-84, D-168).
///
/// ```
/// use officina_core::agents::parse_agents;
/// use officina_core::board::{BoardInput, build_board};
/// use officina_core::meeting::{Delivery, MeetingCard, MeetingTarget, delivery};
/// let input = BoardInput {
///     agents: Some(parse_agents(
///         r#"[{"pid":7,"sessionId":"s","kind":"interactive","status":"idle","startedAt":1}]"#,
///     ).unwrap()),
///     ..Default::default()
/// };
/// let board = build_board(&input, 10);
/// let card = MeetingCard::from(&board.columns.your_turn[0]);
/// let target = MeetingTarget { card: &card, agents_folder: None };
/// assert_eq!(delivery(&target), Delivery::FocusOnly);
/// ```
pub fn delivery(target: &MeetingTarget<'_>) -> Delivery {
    let card = target.card;
    let ended = has_ended(card);
    match (is_background(card), ended) {
        (true, false) if waits(card) => Delivery::CopyAndAttach,
        (true, false) => Delivery::ResumeLive,
        (false, false) => Delivery::FocusOnly,
        (_, true) if target.agents_folder.is_some() => Delivery::ResumeEnded,
        (true, true) => Delivery::CopyAndAttach,
        (false, true) => Delivery::CopyOnly,
    }
}

/// `unix_ms` as ISO 8601 in UTC to the second, as `git log --since` reads
/// it: `YYYY-MM-DDTHH:MM:SSZ`.
///
/// ```
/// assert_eq!(officina_core::meeting::since_iso(0), "1970-01-01T00:00:00Z");
/// ```
pub fn since_iso(unix_ms: i64) -> String {
    let (y, m, d) = utc_date(unix_ms);
    let seconds = unix_ms.rem_euclid(DAY_MS) / 1000;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

/// How many UTC days of journal to read for a session started at
/// `started_at_ms`: back to its start, today included, at most
/// [`RUNS_MAX_DAYS`]; two without a start, as the relay's readers read.
pub fn journal_days(started_at_ms: Option<i64>, now_ms: i64) -> i64 {
    started_at_ms.map_or(2, |started| {
        let days = now_ms.div_euclid(DAY_MS) - started.div_euclid(DAY_MS) + 1;
        days.clamp(1, RUNS_MAX_DAYS)
    })
}

// ------------------------------------------------------------------ view

/// The meeting of one card (AD-84).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MeetingView {
    pub schema: u32,
    /// The card key the meeting was opened for, `sid:<session id>` for a
    /// session the board no longer shows.
    pub key: String,
    pub name: Untrusted<String>,
    pub kind: Option<SessionKindView>,
    pub column: Column,
    pub badge: Option<Badge>,
    pub session_id: Option<String>,
    pub background_id: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub started_at_ms: Option<i64>,
    pub decision: DecisionExpected,
    /// This session's runs since its start, newest first.
    pub runs: Vec<MeetingRun>,
    /// Its pending approval requests.
    pub approvals: Vec<ApprovalView>,
    pub changes: Changes,
    /// `claude logs`, for a background session only.
    pub logs: Option<Logs>,
    pub delivery: Delivery,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub generated_at_ms: i64,
}

/// What the session expects of the author (AD-84, AD-45).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DecisionExpected {
    /// What `claude agents` says the session waits for.
    pub waiting_for: Option<String>,
    /// The latest `Notification` message of its event lines.
    pub notification: Option<Untrusted<String>>,
    /// The end of its last answer: the hook's tail when it could not keep
    /// the answer whole, else the answer itself (AD-45).
    pub answer_end: Option<Untrusted<String>>,
    pub last_prompt: Option<Untrusted<String>>,
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// How one of the session's runs went.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum RunResult {
        /// What the relay counts a success: exited 0, or an HTTP response
        /// that succeeded.
        Succeeded,
        /// Another exit code, a timeout, a start or connection failure.
        Failed,
        /// `officina run` refused before starting anything.
        Refused,
        /// Started, no end line yet.
        Unfinished,
    }
}

/// One run of the session (AD-84), with its journal reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MeetingRun {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub at_ms: i64,
    /// The session's own argument to `officina run` (D-632).
    pub action: Untrusted<String>,
    pub project: Option<String>,
    pub result: RunResult,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub exit_code: Option<i64>,
    /// The refusal's code, for a refused run.
    pub refusal: Option<String>,
    /// The run's id, which the Relay view's entry carries; absent for a
    /// refusal, which has none.
    pub run_id: Option<String>,
    /// The journal's day file holding the line (`YYYY-MM-DD.jsonl`).
    pub journal_file: String,
}

/// The changes section (AD-84, D-154, D-168).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Changes {
    /// No folder `claude agents` reported for the session: no git ran.
    FolderUnknown,
    /// The folder lies outside any git work tree.
    OutsideGit,
    /// Git could not be read: not installed, failed, or past its deadline.
    Unavailable {
        reason: String,
    },
    Read(Box<ChangesRead>),
}

/// What git said of the session's folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ChangesRead {
    /// The commits since the session's start, newest first.
    pub commits: Vec<Commit>,
    pub diff: DiffView,
    /// Why the diff covers the commits only, when it does: a filter
    /// driver is configured, and diffing the working tree would run it.
    pub diff_scope_note: Option<String>,
    /// Untracked files by name and size, never read.
    pub untracked: Vec<UntrackedFile>,
    /// More untracked files than [`UNTRACKED_LISTED`] existed.
    pub untracked_more: bool,
}

/// One commit since the session's start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Commit {
    pub hash: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub at_ms: i64,
    pub subject: Untrusted<String>,
    /// Each changed file as `git --name-status` prints it: its status
    /// letter, then its path or paths.
    pub files: Vec<Untrusted<String>>,
}

/// The diff as text, with where each file and hunk starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DiffView {
    pub text: Untrusted<String>,
    pub files: Vec<DiffFile>,
    /// The diff was longer than [`DIFF_CAP_BYTES`] and was cut.
    pub truncated: bool,
}

/// One file of the diff: its path, the line its section starts on, and the
/// line of each of its hunks, every line counted from 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DiffFile {
    pub path: Untrusted<String>,
    pub line: u32,
    pub hunks: Vec<u32>,
}

/// An untracked file, listed by name and size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct UntrackedFile {
    pub path: Untrusted<String>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub bytes: Option<u64>,
}

/// The logs section (AD-84, E23).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Logs {
    Shown(LogsText),
    /// `claude logs` gave nothing to show, with why.
    Unavailable {
        reason: String,
    },
}

/// The end of `claude logs`, its escape sequences stripped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LogsText {
    pub text: Untrusted<String>,
    /// How many escape sequences were removed.
    pub escapes_stripped: u32,
    /// Bytes dropped from the start to keep [`LOGS_KEPT_BYTES`].
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub dropped_bytes: u64,
    /// The first line, from 1, holding a [`LOG_ERROR_MARKERS`] entry.
    pub first_error_line: Option<u32>,
}

// ---------------------------------------------------------------- inputs

/// What `git` printed on Receive, as the host read it (AD-84, D-154).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitRead {
    FolderUnknown,
    OutsideGit,
    Unavailable(String),
    Read {
        /// `git log --name-status` in [`LOG_FORMAT`].
        log: Vec<u8>,
        /// The diff, at most [`DIFF_CAP_BYTES`] of it, and whether more
        /// came.
        diff: Vec<u8>,
        diff_over: bool,
        /// A filter driver is configured: `diff` covers the commits only.
        filter_driver: bool,
        /// `git ls-files --others --exclude-standard` names, with their
        /// sizes as the host stat'ed them.
        untracked: Vec<(String, Option<u64>)>,
        untracked_more: bool,
    },
}

/// What `claude logs` gave on Receive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogsRead {
    /// Its stdout.
    Output(Vec<u8>),
    Failed(String),
}

/// The `git log` format the host asks for: a record separator, then the
/// hash, the committer time in seconds and the subject separated by unit
/// separators; `--name-status` lines follow each record.
pub const LOG_FORMAT: &str = "--format=%x1e%H%x1f%ct%x1f%s";

/// Everything [`build`] reads: none of it is a transcript.
#[derive(Debug, Clone)]
pub struct MeetingInput<'a> {
    pub card: &'a MeetingCard,
    pub notification: Option<&'a str>,
    /// Journal lines read back to the session's start (D-169).
    pub journal: &'a [JournalLine],
    pub approvals: &'a [ApprovalView],
    pub git: GitRead,
    pub logs: Option<LogsRead>,
    pub delivery: Delivery,
    pub now_ms: i64,
}

/// The meeting view of `input` (AD-84).
pub fn build(input: &MeetingInput<'_>) -> MeetingView {
    let card = input.card;
    let session = card.session_id.as_deref();
    MeetingView {
        schema: MEETING_VIEW_SCHEMA,
        key: card.key.clone(),
        name: card.name.clone(),
        kind: card.kind,
        column: card.column,
        badge: card.badge,
        session_id: card.session_id.clone(),
        background_id: card.background_id.clone(),
        started_at_ms: card.started_at_ms,
        decision: DecisionExpected {
            waiting_for: card.waiting_for.clone(),
            notification: input.notification.map(|m| Untrusted::new(m.to_owned())),
            answer_end: card
                .last_message_tail
                .clone()
                .or_else(|| card.last_message.clone()),
            last_prompt: card.last_prompt.clone(),
        },
        runs: session.map_or_else(Vec::new, |sid| {
            runs_of(sid, card.started_at_ms, input.journal)
        }),
        approvals: session.map_or_else(Vec::new, |sid| {
            input
                .approvals
                .iter()
                .filter(|a| a.session_id.as_deref() == Some(sid))
                .cloned()
                .collect()
        }),
        changes: changes_of(&input.git),
        logs: is_background(card).then(|| logs_of(input.logs.as_ref())),
        delivery: input.delivery,
        generated_at_ms: input.now_ms,
    }
}

/// The session's runs and refusals since `started_at_ms`, newest first.
fn runs_of(session: &str, started_at_ms: Option<i64>, journal: &[JournalLine]) -> Vec<MeetingRun> {
    let since = started_at_ms.unwrap_or(i64::MIN);
    let ends: std::collections::BTreeMap<&str, &crate::relay::journal::RunFinished> = journal
        .iter()
        .filter_map(|line| match &line.entry {
            JournalEntry::RunFinished(finished) => Some((finished.run_id.as_str(), finished)),
            _ => None,
        })
        .collect();
    let mut runs: Vec<MeetingRun> = journal
        .iter()
        .filter(|line| line.ts >= since)
        .filter_map(|line| match &line.entry {
            JournalEntry::RunStarted(started) if started.session_id.as_deref() == Some(session) => {
                let end = ends.get(started.run_id.as_str());
                let (result, exit_code) = match end {
                    None => (RunResult::Unfinished, None),
                    Some(end) => (run_result(end), end.exit_code),
                };
                Some(MeetingRun {
                    at_ms: line.ts,
                    action: Untrusted::new(started.action.clone()),
                    project: Some(started.project.clone()),
                    result,
                    exit_code,
                    refusal: None,
                    run_id: Some(started.run_id.clone()),
                    journal_file: event_file_name(line.ts),
                })
            }
            JournalEntry::Refused(refused) if refused.session_id.as_deref() == Some(session) => {
                Some(MeetingRun {
                    at_ms: line.ts,
                    action: Untrusted::new(refused.action.clone()),
                    project: refused.project.clone(),
                    result: RunResult::Refused,
                    exit_code: None,
                    refusal: Some(refused.reason.code().to_owned()),
                    run_id: None,
                    journal_file: event_file_name(line.ts),
                })
            }
            _ => None,
        })
        .collect();
    runs.sort_by_key(|run| std::cmp::Reverse(run.at_ms));
    runs
}

/// The relay's own rule ([`RunFinished::succeeded`]), which the exit code
/// of `officina run` and the Relay view's colour share.
///
/// [`RunFinished::succeeded`]: crate::relay::journal::RunFinished::succeeded
fn run_result(end: &crate::relay::journal::RunFinished) -> RunResult {
    if end.succeeded() {
        RunResult::Succeeded
    } else {
        RunResult::Failed
    }
}

fn changes_of(git: &GitRead) -> Changes {
    match git {
        GitRead::FolderUnknown => Changes::FolderUnknown,
        GitRead::OutsideGit => Changes::OutsideGit,
        GitRead::Unavailable(reason) => Changes::Unavailable {
            reason: reason.clone(),
        },
        GitRead::Read {
            log,
            diff,
            diff_over,
            filter_driver,
            untracked,
            untracked_more,
        } => Changes::Read(Box::new(ChangesRead {
            commits: parse_log(log),
            diff: shape_diff(diff, *diff_over),
            diff_scope_note: filter_driver.then(|| {
                "a filter driver is configured in this repository: diffing the working tree would run it, so the diff covers the commits only".to_owned()
            }),
            untracked: untracked
                .iter()
                .take(UNTRACKED_LISTED)
                .map(|(path, bytes)| UntrackedFile {
                    path: Untrusted::new(path.clone()),
                    bytes: *bytes,
                })
                .collect(),
            untracked_more: *untracked_more || untracked.len() > UNTRACKED_LISTED,
        })),
    }
}

/// The commits of `git log` output in [`LOG_FORMAT`] with `--name-status`.
fn parse_log(log: &[u8]) -> Vec<Commit> {
    String::from_utf8_lossy(log)
        .split('\u{1e}')
        .filter_map(|record| {
            let (head, rest) = record.split_once('\n').unwrap_or((record, ""));
            let mut fields = head.splitn(3, '\u{1f}');
            let hash = fields.next()?.trim();
            if hash.is_empty() {
                return None;
            }
            let at_ms = fields.next()?.trim().parse::<i64>().ok()? * 1000;
            let subject = fields.next().unwrap_or_default();
            Some(Commit {
                hash: hash.to_owned(),
                at_ms,
                subject: Untrusted::new(subject.to_owned()),
                files: rest
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .map(|line| Untrusted::new(line.replace('\t', " ")))
                    .collect(),
            })
        })
        .collect()
}

/// `diff` as the panel shows it (AD-84): cut at the last whole line within
/// [`DIFF_CAP_BYTES`] when longer or when `over`, with each file's first
/// line and each hunk's line.
///
/// ```
/// use officina_core::meeting::shape_diff;
/// let diff = b"diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n";
/// let shaped = shape_diff(diff, false);
/// assert_eq!(shaped.files.len(), 1);
/// assert_eq!((shaped.files[0].line, shaped.files[0].hunks.clone()), (0, vec![3]));
/// ```
pub fn shape_diff(diff: &[u8], over: bool) -> DiffView {
    let (bytes, truncated) = if diff.len() > DIFF_CAP_BYTES || over {
        let within = &diff[..diff.len().min(DIFF_CAP_BYTES)];
        let cut = within
            .iter()
            .rposition(|b| *b == b'\n')
            .map_or(0, |at| at + 1);
        (&within[..cut], true)
    } else {
        (diff, false)
    };
    let text = String::from_utf8_lossy(bytes).into_owned();
    let mut files: Vec<DiffFile> = Vec::new();
    for (at, line) in text.lines().enumerate() {
        let at = u32::try_from(at).unwrap_or(u32::MAX);
        if let Some(header) = line.strip_prefix("diff --git ") {
            files.push(DiffFile {
                path: Untrusted::new(diff_path(header)),
                line: at,
                hunks: Vec::new(),
            });
        } else if line.starts_with("@@")
            && let Some(file) = files.last_mut()
        {
            file.hunks.push(at);
        }
    }
    DiffView {
        text: Untrusted::new(text),
        files,
        truncated,
    }
}

/// The path of a `diff --git a/<path> b/<path>` header: the part after its
/// last ` b/`, or the whole header when git quoted it otherwise.
fn diff_path(header: &str) -> String {
    header
        .rsplit_once(" b/")
        .map_or_else(|| header.to_owned(), |(_, path)| path.to_owned())
}

/// `bytes` with every escape sequence removed (AD-84): CSI (`ESC [` to its
/// final byte), OSC (`ESC ]` to BEL or `ESC \`), the other string controls
/// (`ESC P`, `ESC X`, `ESC ^`, `ESC _` to `ESC \`), two-byte sequences, and
/// the C1 introducers written as single bytes; the count of sequences
/// removed. Carriage returns before a newline go; other control characters
/// but newline and tab become nothing.
///
/// ```
/// use officina_core::meeting::strip_escapes;
/// let (text, count) = strip_escapes(b"\x1b[1;31mred\x1b[0m \x1b]0;title\x07done\r\n");
/// assert_eq!((text.as_str(), count), ("red done\n", 3));
/// ```
pub fn strip_escapes(bytes: &[u8]) -> (String, u32) {
    let text = String::from_utf8_lossy(bytes);
    let mut out = String::with_capacity(text.len());
    let mut count = 0u32;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => {
                count += 1;
                match chars.next() {
                    Some('[') => skip_csi(&mut chars),
                    Some(']') => skip_string(&mut chars, true),
                    Some('P' | 'X' | '^' | '_') => skip_string(&mut chars, false),
                    // A character set designation takes one more byte.
                    Some('(' | ')' | '*' | '+' | '-' | '.' | '/' | '#' | '%') => {
                        chars.next();
                    }
                    _ => {}
                }
            }
            '\u{9b}' => {
                count += 1;
                skip_csi(&mut chars);
            }
            '\u{9d}' => {
                count += 1;
                skip_string(&mut chars, true);
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\t' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    (out, count)
}

/// Skips a CSI sequence's parameter and intermediate bytes and its final
/// byte (`@` to `~`).
fn skip_csi(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    for c in chars.by_ref() {
        if ('\u{40}'..='\u{7e}').contains(&c) {
            return;
        }
    }
}

/// Skips a string sequence to its terminator: `ESC \`, or BEL when `bel`
/// ends it too (OSC).
fn skip_string(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, bel: bool) {
    while let Some(c) = chars.next() {
        match c {
            '\u{7}' if bel => return,
            '\u{9c}' => return,
            '\u{1b}' if chars.peek() == Some(&'\\') => {
                chars.next();
                return;
            }
            _ => {}
        }
    }
}

/// `text` with each line's trailing spaces dropped and every run of blank
/// lines made one: `claude logs` prints the session's screen, padded to
/// its width and height (E23), so the kept end holds more of what was
/// said.
///
/// ```
/// use officina_core::meeting::squeeze_blank_lines;
/// assert_eq!(squeeze_blank_lines("a   \n\n  \n\nb\n"), "a\n\nb\n");
/// ```
pub fn squeeze_blank_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank_run = false;
    for line in text.split_inclusive('\n') {
        let ends = line.ends_with('\n');
        let content = line.trim_end_matches('\n').trim_end();
        if content.is_empty() && ends {
            if blank_run {
                continue;
            }
            blank_run = true;
        } else {
            blank_run = false;
        }
        out.push_str(content);
        if ends {
            out.push('\n');
        }
    }
    out
}

/// The last [`LOGS_KEPT_BYTES`] of `text`, from a line start when the cut
/// falls inside one; the bytes dropped.
fn keep_end(text: &str) -> (&str, u64) {
    if text.len() <= LOGS_KEPT_BYTES {
        return (text, 0);
    }
    let mut from = text.len() - LOGS_KEPT_BYTES;
    while !text.is_char_boundary(from) {
        from += 1;
    }
    if let Some(newline) = text[from..].find('\n') {
        from += newline + 1;
    }
    (&text[from..], from as u64)
}

/// The logs section of `read`, or why there is none.
pub fn logs_of(read: Option<&LogsRead>) -> Logs {
    match read {
        None => Logs::Unavailable {
            reason: "the session has no background id to read its logs by".to_owned(),
        },
        Some(LogsRead::Failed(reason)) => Logs::Unavailable {
            reason: reason.clone(),
        },
        Some(LogsRead::Output(bytes)) => {
            let (stripped, escapes_stripped) = strip_escapes(bytes);
            let stripped = squeeze_blank_lines(&stripped);
            let (kept, dropped_bytes) = keep_end(&stripped);
            let text = Untrusted::new(kept.to_owned());
            let first_error_line = text.first_line_matching(LOG_ERROR_MARKERS);
            Logs::Shown(LogsText {
                text,
                escapes_stripped,
                dropped_bytes,
                first_error_line,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::parse_agents;
    use crate::board::{BoardInput, build_board};
    use crate::relay::journal::{RefusalReason, Refused, RunFinished, RunStarted};

    const NOW: i64 = 1_793_102_400_000;

    /// The one card of a board built from `agents_json`.
    fn card_of(agents_json: &str) -> MeetingCard {
        let input = BoardInput {
            agents: Some(parse_agents(agents_json).unwrap()),
            ..Default::default()
        };
        let board = build_board(&input, NOW);
        let card: &Card = board.columns.all().next().expect("one card");
        MeetingCard::from(card)
    }

    fn background(status: &str, state: &str, extra: &str) -> MeetingCard {
        card_of(&format!(
            r#"[{{"id":"bg01","sessionId":"0f1e2d3c-4b5a-4978-8796-a5b4c3d2e1f0","kind":"background","status":"{status}","state":"{state}","cwd":"C:/path/to/repo","startedAt":{started}{extra}}}]"#,
            started = NOW - 1000
        ))
    }

    fn started(run_id: &str, session: &str, action: &str) -> JournalEntry {
        JournalEntry::RunStarted(RunStarted {
            run_id: run_id.to_owned(),
            project: "backend".to_owned(),
            session_id: Some(session.to_owned()),
            action: action.to_owned(),
            class: crate::relay::catalogue::ActionClass::Free,
            params: Default::default(),
            argv: vec!["C:/bin/tool.exe".to_owned()],
            method: None,
            url: None,
            credential: None,
            catalogue_hash: "00".repeat(32),
            approval_id: None,
            tree: None,
            timeout_s: Some(60),
        })
    }

    fn finished(run_id: &str, exit: i64) -> JournalEntry {
        let end: RunFinished = serde_json::from_value(serde_json::json!({
            "run_id": run_id, "outcome": "exited", "exit_code": exit, "duration_ms": 10,
        }))
        .unwrap();
        JournalEntry::RunFinished(end)
    }

    fn line(ts: i64, entry: JournalEntry) -> JournalLine {
        JournalLine::new(ts, entry)
    }

    fn input<'a>(card: &'a MeetingCard, journal: &'a [JournalLine]) -> MeetingInput<'a> {
        MeetingInput {
            card,
            notification: Some("Claude needs your permission to use Bash"),
            journal,
            approvals: &[],
            git: GitRead::FolderUnknown,
            logs: None,
            delivery: Delivery::ResumeLive,
            now_ms: NOW,
        }
    }

    #[test]
    fn decision_expected_holds_waiting_for_notification_tail_and_prompt() {
        let mut card = background("waiting", "blocked", r#","waitingFor":"input needed""#);
        card.last_prompt = Some(Untrusted::new("Bump the dependency".to_owned()));
        card.last_message = Some(Untrusted::new("Head of the answer".to_owned()));
        card.last_message_tail = Some(Untrusted::new("...which test?".to_owned()));
        let view = build(&input(&card, &[]));
        let decision = serde_json::to_value(&view.decision).unwrap();
        assert_eq!(decision["waiting_for"], "input needed");
        assert_eq!(
            decision["notification"],
            "Claude needs your permission to use Bash"
        );
        assert_eq!(decision["answer_end"], "...which test?");
        assert_eq!(decision["last_prompt"], "Bump the dependency");
        // Without a tail, the answer is whole in its head.
        card.last_message_tail = None;
        let view = build(&input(&card, &[]));
        assert_eq!(
            serde_json::to_value(&view.decision).unwrap()["answer_end"],
            "Head of the answer"
        );
    }

    #[test]
    fn runs_are_this_session_only_with_their_journal_reference() {
        let card = background("idle", "done", r#","pid":7"#);
        let sid = card.session_id.clone().unwrap();
        let journal = [
            line(NOW - 5000, started("r-before", &sid, "check")),
            line(NOW - 900, started("r-ok", &sid, "check")),
            line(NOW - 800, finished("r-ok", 0)),
            line(NOW - 700, started("r-other", "another-session", "apply")),
            line(NOW - 600, started("r-bad", &sid, "apply")),
            line(NOW - 500, finished("r-bad", 2)),
            line(
                NOW - 400,
                JournalEntry::Refused(Refused {
                    project: Some("backend".to_owned()),
                    session_id: Some(sid.clone()),
                    action: "deploy".to_owned(),
                    params: None,
                    reason: RefusalReason::ALL[0],
                    detail: "no".to_owned(),
                }),
            ),
            line(NOW - 300, started("r-open", &sid, "check")),
        ];
        let view = build(&input(&card, &journal));
        let got: Vec<(Option<&str>, RunResult, Option<i64>)> = view
            .runs
            .iter()
            .map(|r| (r.run_id.as_deref(), r.result, r.exit_code))
            .collect();
        assert_eq!(
            got,
            [
                (Some("r-open"), RunResult::Unfinished, None),
                (None, RunResult::Refused, None),
                (Some("r-bad"), RunResult::Failed, Some(2)),
                (Some("r-ok"), RunResult::Succeeded, Some(0)),
            ],
            "newest first, this session's only, nothing before its start"
        );
        assert!(
            view.runs
                .iter()
                .all(|r| r.journal_file == crate::dates::event_file_name(r.at_ms))
        );
        assert_eq!(
            view.runs[1].refusal.as_deref(),
            Some(RefusalReason::ALL[0].code())
        );
    }

    #[test]
    fn every_text_is_untrusted() {
        // Every session text, subject, path and log line serializes from
        // an `Untrusted` field: a marker put in each input comes out only
        // where the type says untrusted text goes.
        let marker = "MARK-7f3a";
        let mut card = background("idle", "done", r#","pid":7"#);
        card.last_prompt = Some(Untrusted::new(format!("{marker} prompt")));
        card.last_message = Some(Untrusted::new(format!("{marker} answer")));
        let sid = card.session_id.clone().unwrap();
        let journal = [line(NOW - 10, started("r1", &sid, marker))];
        let log = format!("\u{1e}abc\u{1f}1\u{1f}{marker} subject\nM\t{marker}.rs\n");
        let diff = format!("diff --git a/{marker} b/{marker}\n@@ -1 +1 @@\n+{marker}\n");
        let mut meeting = input(&card, &journal);
        meeting.notification = Some(marker);
        meeting.git = GitRead::Read {
            log: log.into_bytes(),
            diff: diff.into_bytes(),
            diff_over: false,
            filter_driver: false,
            untracked: vec![(format!("{marker}.tmp"), Some(3))],
            untracked_more: false,
        };
        meeting.logs = Some(LogsRead::Output(format!("{marker} log\n").into_bytes()));
        let view = build(&meeting);
        let texts = [
            serde_json::to_value(&view.decision.notification).unwrap(),
            serde_json::to_value(&view.decision.last_prompt).unwrap(),
            serde_json::to_value(&view.decision.answer_end).unwrap(),
            serde_json::to_value(&view.runs[0].action).unwrap(),
        ];
        for text in texts {
            assert!(text.as_str().unwrap().contains(marker), "{text}");
        }
        let Changes::Read(read) = &view.changes else {
            panic!("{:?}", view.changes)
        };
        // The fields carrying the marker are `Untrusted` by type: these
        // bindings only compile against that type.
        let untrusted: [&Untrusted<String>; 5] = [
            &read.commits[0].subject,
            &read.commits[0].files[0],
            &read.diff.text,
            &read.diff.files[0].path,
            &read.untracked[0].path,
        ];
        for text in untrusted {
            assert!(
                serde_json::to_string(text).unwrap().contains(marker),
                "{text:?}"
            );
        }
        let Some(Logs::Shown(logs)) = &view.logs else {
            panic!("{:?}", view.logs)
        };
        assert!(serde_json::to_string(&logs.text).unwrap().contains(marker));
        // No field holds a transcript: nothing named so reaches the view.
        let json = serde_json::to_string(&view).unwrap();
        assert!(!json.contains("transcript"), "{json}");
    }

    #[test]
    fn since_is_iso_8601_utc_of_started_at() {
        assert_eq!(since_iso(NOW), "2026-10-27T12:00:00Z");
        assert_eq!(since_iso(NOW + 3_723_999), "2026-10-27T13:02:03Z");
        assert_eq!(since_iso(-1000), "1969-12-31T23:59:59Z");
        assert_eq!(journal_days(Some(NOW), NOW), 1);
        assert_eq!(journal_days(Some(NOW - 3 * DAY_MS), NOW), 4);
        assert_eq!(journal_days(Some(0), NOW), RUNS_MAX_DAYS);
        assert_eq!(journal_days(None, NOW), 2);
    }

    #[test]
    fn the_diff_is_capped_with_per_file_offsets() {
        let one = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1 +1 @@\n-a\n+b\n@@ -9 +9 @@\n-c\n+d\n";
        let two = "diff --git a/notes.md b/notes.md\n@@ -1 +1 @@\n+line\n";
        let shaped = shape_diff(format!("{one}{two}").as_bytes(), false);
        assert!(!shaped.truncated);
        let files: Vec<(String, u32, Vec<u32>)> = shaped
            .files
            .iter()
            .map(|f| {
                (
                    serde_json::to_value(&f.path)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    f.line,
                    f.hunks.clone(),
                )
            })
            .collect();
        assert_eq!(
            files,
            [
                ("src/a.rs".to_owned(), 0, vec![3, 6]),
                ("notes.md".to_owned(), 9, vec![10]),
            ]
        );
        // Longer than the cap: cut at the last whole line within it.
        let filler = "+".repeat(99) + "\n";
        let long = format!("{one}{}", filler.repeat(DIFF_CAP_BYTES / filler.len() + 10));
        let shaped = shape_diff(long.as_bytes(), false);
        assert!(shaped.truncated);
        let text = serde_json::to_value(&shaped.text).unwrap();
        let text = text.as_str().unwrap();
        assert!(text.len() <= DIFF_CAP_BYTES && text.ends_with('\n'));
        // The host saying more came truncates too.
        assert!(shape_diff(one.as_bytes(), true).truncated);
    }

    #[test]
    fn outside_git_is_said() {
        let card = background("idle", "done", r#","pid":7"#);
        let mut meeting = input(&card, &[]);
        for (git, expected) in [
            (GitRead::OutsideGit, Changes::OutsideGit),
            (GitRead::FolderUnknown, Changes::FolderUnknown),
            (
                GitRead::Unavailable("no git".to_owned()),
                Changes::Unavailable {
                    reason: "no git".to_owned(),
                },
            ),
        ] {
            meeting.git = git;
            assert_eq!(build(&meeting).changes, expected);
        }
        let json = serde_json::to_value(Changes::OutsideGit).unwrap();
        assert_eq!(json["state"], "outside_git");
    }

    /// One sample of each sequence family the stripper knows, built from
    /// its introducer.
    pub(crate) fn escape_families() -> Vec<String> {
        vec![
            "\u{1b}[38;5;208m".to_owned(),
            "\u{1b}[?25l".to_owned(),
            "\u{1b}]0;window title\u{7}".to_owned(),
            "\u{1b}]8;;https://example.com\u{1b}\\".to_owned(),
            "\u{1b}P1$r0m\u{1b}\\".to_owned(),
            "\u{1b}(B".to_owned(),
            "\u{1b}7".to_owned(),
            "\u{9b}2J".to_owned(),
        ]
    }

    #[test]
    fn escape_sequences_are_stripped() {
        let families = escape_families();
        let raw: String = families
            .iter()
            .enumerate()
            .map(|(at, seq)| format!("{seq}line {at}\r\n"))
            .collect();
        let (text, count) = strip_escapes(raw.as_bytes());
        assert_eq!(count as usize, families.len());
        assert!(!text.contains('\u{1b}') && !text.contains('\r'), "{text:?}");
        let expected: String = (0..families.len())
            .map(|at| format!("line {at}\n"))
            .collect();
        assert_eq!(text, expected);
        // An unterminated sequence at the end drops what follows it only.
        let (text, count) = strip_escapes(b"kept\x1b]0;never closed");
        assert_eq!((text.as_str(), count), ("kept", 1));
    }

    #[test]
    fn only_the_last_64_kib_is_kept() {
        let line = "x".repeat(99) + "\n";
        let raw = line.repeat(LOGS_KEPT_BYTES / line.len() + 50) + "the end\n";
        let Logs::Shown(logs) = logs_of(Some(&LogsRead::Output(raw.clone().into_bytes()))) else {
            panic!()
        };
        let kept = serde_json::to_value(&logs.text).unwrap();
        let kept = kept.as_str().unwrap();
        assert!(kept.len() <= LOGS_KEPT_BYTES);
        assert!(kept.ends_with("the end\n"));
        assert!(kept.starts_with('x'), "cut at a line start");
        assert_eq!(logs.dropped_bytes as usize + kept.len(), raw.len());
        // A screen padded with spaces and blank lines keeps its text.
        let padded = format!(
            "{}said{}\n{}",
            "\n".repeat(30),
            " ".repeat(200),
            "   \n".repeat(30)
        );
        let Logs::Shown(squeezed) = logs_of(Some(&LogsRead::Output(padded.into_bytes()))) else {
            panic!()
        };
        assert_eq!(serde_json::to_value(&squeezed.text).unwrap(), "\nsaid\n\n");
        // Short output is kept whole.
        let Logs::Shown(short) = logs_of(Some(&LogsRead::Output(b"a\nb\n".to_vec()))) else {
            panic!()
        };
        assert_eq!(short.dropped_bytes, 0);
    }

    #[test]
    fn the_first_error_line_is_found() {
        for marker in LOG_ERROR_MARKERS {
            let raw = format!(
                "\u{1b}[2mstarting\u{1b}[0m\nworking\n  {marker} it broke\n{marker} again\n"
            );
            let Logs::Shown(logs) = logs_of(Some(&LogsRead::Output(raw.into_bytes()))) else {
                panic!()
            };
            assert_eq!(logs.first_error_line, Some(3), "{marker}");
        }
        let Logs::Shown(clean) = logs_of(Some(&LogsRead::Output(b"all good\n".to_vec()))) else {
            panic!()
        };
        assert_eq!(clean.first_error_line, None);
        assert!(matches!(
            logs_of(Some(&LogsRead::Failed("exit 1".to_owned()))),
            Logs::Unavailable { .. }
        ));
    }

    #[test]
    fn logs_are_shown_for_a_background_session_only() {
        let interactive = card_of(
            r#"[{"pid":7,"sessionId":"s","kind":"interactive","status":"idle","startedAt":1}]"#,
        );
        let mut meeting = input(&interactive, &[]);
        meeting.logs = Some(LogsRead::Output(b"x\n".to_vec()));
        assert_eq!(build(&meeting).logs, None);
    }

    #[test]
    fn each_card_shape_gets_its_delivery() {
        let folder = Some("C:/path/to/repo");
        let cases: [(MeetingCard, Option<&str>, Delivery); 9] = [
            (
                background("idle", "done", r#","pid":7"#),
                None,
                Delivery::ResumeLive,
            ),
            (
                background("busy", "working", r#","pid":7"#),
                None,
                Delivery::ResumeLive,
            ),
            (
                background("waiting", "blocked", r#","pid":7"#),
                None,
                Delivery::CopyAndAttach,
            ),
            (
                background("idle", "working", r#","pid":7,"waitingFor":"input needed""#),
                None,
                Delivery::CopyAndAttach,
            ),
            (
                background("idle", "stopped", ""),
                folder,
                Delivery::ResumeEnded,
            ),
            (
                background("idle", "failed", ""),
                None,
                Delivery::CopyAndAttach,
            ),
            (
                card_of(
                    r#"[{"pid":7,"sessionId":"s","kind":"interactive","status":"waiting","startedAt":1}]"#,
                ),
                folder,
                Delivery::FocusOnly,
            ),
            (ended_interactive(), folder, Delivery::ResumeEnded),
            (ended_interactive(), None, Delivery::CopyOnly),
        ];
        for (card, agents_folder, expected) in &cases {
            let target = MeetingTarget {
                card,
                agents_folder: *agents_folder,
            };
            assert_eq!(delivery(&target), *expected, "{}", card.key);
        }
    }

    /// An interactive card the author exited.
    fn ended_interactive() -> MeetingCard {
        let mut card = card_of(
            r#"[{"pid":7,"sessionId":"s","kind":"interactive","status":"idle","startedAt":1}]"#,
        );
        card.column = Column::ToResume;
        card.badge = Some(Badge::Ended);
        card
    }

    #[test]
    fn a_done_background_session_with_a_pid_is_resume_live() {
        let live = background("idle", "done", r#","pid":7"#);
        assert_eq!(live.column, Column::Done, "E1: idle and done sits in Done");
        assert!(!has_ended(&live));
        let target = MeetingTarget {
            card: &live,
            agents_folder: None,
        };
        assert_eq!(delivery(&target), Delivery::ResumeLive);
        let gone = background("idle", "done", "");
        assert!(has_ended(&gone), "without a pid it has ended");
    }

    #[test]
    fn an_ended_session_without_an_agents_folder_is_never_resumed() {
        for card in [
            ended_interactive(),
            background("idle", "stopped", ""),
            background("idle", "failed", ""),
            background("idle", "done", ""),
        ] {
            let target = MeetingTarget {
                card: &card,
                agents_folder: None,
            };
            let found = delivery(&target);
            assert!(
                !matches!(found, Delivery::ResumeEnded | Delivery::ResumeLive),
                "{}: {found:?}",
                card.key
            );
        }
    }

    #[test]
    fn every_delivery_has_a_case() {
        let folder = Some("C:/path/to/repo");
        let reached: Vec<Delivery> = [
            (background("idle", "done", r#","pid":7"#), None),
            (background("waiting", "blocked", r#","pid":7"#), None),
            (background("idle", "stopped", ""), folder),
            (ended_interactive(), None),
            (
                card_of(r#"[{"pid":7,"sessionId":"s","kind":"interactive","status":"idle","startedAt":1}]"#),
                None,
            ),
        ]
        .iter()
        .map(|(card, agents_folder)| {
            delivery(&MeetingTarget {
                card,
                agents_folder: *agents_folder,
            })
        })
        .collect();
        for variant in Delivery::ALL {
            assert!(reached.contains(variant), "{variant:?} has no case");
        }
    }
}
