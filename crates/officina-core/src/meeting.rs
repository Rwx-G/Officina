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
//! [`Untrusted`]. The recap section is the orchestrator's latest pending
//! recap for the session or its subject (story 5.17), its texts untrusted
//! too.

use serde::{Deserialize, Serialize};

use std::collections::BTreeSet;

use crate::agents::{AgentRecord, LifecycleState};
use crate::board::{Badge, Board, Card, Column, SessionKindView, card_key};
use crate::dates::{DAY_MS, event_file_name, utc_date};
use crate::orders::proposal::{ProposalView, latest_answer, latest_recap};
use crate::registry::types::{ExcerptRecord, SessionRecord, Source};
use crate::relay::approvals::ApprovalView;
use crate::relay::journal::{JournalEntry, JournalLine};
use crate::untrusted::Untrusted;

/// The meeting view's format; a field change bumps it.
pub const MEETING_VIEW_SCHEMA: u32 = 3;

/// The most diff text a meeting shows (AD-84): the diff is cut at the last
/// whole line within it, and says so.
pub const DIFF_CAP_BYTES: usize = 256 * 1024;

/// The most of `claude logs` a meeting keeps once played on a screen
/// ([`crate::screen::render`]): the end of the text, where the session's
/// latest turn is.
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
    /// The subject the session is filed under, for the recap section.
    pub subject_id: Option<i64>,
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
            subject_id: card.subject.as_ref().map(|subject| subject.id),
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
            subject_id: record.subject.as_ref().and_then(|s| s.value),
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

/// What [`delivery`] decides from: the card, the folder `claude agents`
/// reported for its session, if any (D-168), and the `claude` version the
/// collector last read (D-139). A folder an event line names is never
/// one: any session can append event lines.
#[derive(Debug, Clone, Copy)]
pub struct MeetingTarget<'a> {
    pub card: &'a MeetingCard,
    pub agents_folder: Option<&'a str>,
    pub claude_version: Option<&'a str>,
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

/// How an answer would reach the session of `target` (AD-84, AD-85,
/// D-139, D-168, D-635).
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
/// let target = MeetingTarget { card: &card, agents_folder: None, claude_version: None };
/// assert_eq!(delivery(&target), Delivery::FocusOnly);
/// ```
pub fn delivery(target: &MeetingTarget<'_>) -> Delivery {
    let card = target.card;
    let ended = has_ended(card);
    match (is_background(card), ended) {
        (true, false) if waits(card) => Delivery::CopyAndAttach,
        // A live idle background session takes the answer as its next
        // turn only where gate 3 found it delivered: on a recent enough
        // `claude` (D-139), from the folder `claude agents` reported
        // (D-635); else the answer is copied and attach opens (FR-15).
        (true, false)
            if RESUME_LIVE_DELIVERS
                && resumes_with_a_prompt(target.claude_version)
                && target.agents_folder.is_some() =>
        {
            Delivery::ResumeLive
        }
        (true, false) => Delivery::CopyAndAttach,
        (false, false) => Delivery::FocusOnly,
        (_, true) if target.agents_folder.is_some() => Delivery::ResumeEnded,
        (true, true) => Delivery::CopyAndAttach,
        (false, true) => Delivery::CopyOnly,
    }
}

/// True when the session of `target` takes an answer the orchestrator
/// proposes (AD-182, PRD section 9 item 3): a background session live and
/// idle in its turn, waiting on no question or dialog, whose answer goes
/// as its next turn (`ResumeLive`). The board puts such a session in Done
/// with a pid (E1, D-802); Your turn is named too, for a background card a
/// later classification would put there. A busy one (Working) takes none,
/// although its delivery reads `ResumeLive` too.
///
/// ```
/// use officina_core::agents::parse_agents;
/// use officina_core::board::{BoardInput, build_board};
/// use officina_core::meeting::{MeetingCard, MeetingTarget, takes_a_proposed_answer};
/// let listed = |status: &str, state: &str| BoardInput {
///     agents: Some(parse_agents(&format!(
///         r#"[{{"id":"bg1","pid":7,"sessionId":"s","kind":"background","cwd":"C:/w","status":"{status}","state":"{state}","startedAt":1}}]"#,
///     )).unwrap()),
///     ..Default::default()
/// };
/// for (status, state, takes) in [("idle", "done", true), ("busy", "working", false), ("waiting", "blocked", false)] {
///     let board = build_board(&listed(status, state), 10);
///     let card = MeetingCard::from(board.columns.all().next().unwrap());
///     let target = MeetingTarget { card: &card, agents_folder: Some("C:/w"), claude_version: Some("2.1.312") };
///     assert_eq!(takes_a_proposed_answer(&target), takes, "{status}");
/// }
/// ```
pub fn takes_a_proposed_answer(target: &MeetingTarget<'_>) -> bool {
    matches!(target.card.column, Column::YourTurn | Column::Done)
        && delivery(target) == Delivery::ResumeLive
}

/// The sessions of `board` that take a proposed answer now
/// ([`takes_a_proposed_answer`]), each card's folder the `cwd` of the
/// `agents` record listing it, by its key, else its session id (D-804),
/// and the `claude` version the collector last read (D-139). What
/// ingestion validates an answer proposal against, and what each board
/// keeps a pending one alive with (AD-182).
pub fn answerable_sessions(
    board: &Board,
    agents: &[AgentRecord],
    claude_version: Option<&str>,
) -> BTreeSet<String> {
    board
        .columns
        .all()
        .filter_map(|card| {
            let session = card.session_id.as_deref()?;
            let folder = agents
                .iter()
                .find(|r| card_key(r).as_deref() == Some(card.key.as_str()))
                .or_else(|| {
                    agents
                        .iter()
                        .find(|r| r.session_id.as_deref() == Some(session))
                })
                .and_then(|r| r.cwd.as_deref());
            let meeting = MeetingCard::from(card);
            let target = MeetingTarget {
                card: &meeting,
                agents_folder: folder,
                claude_version,
            };
            takes_a_proposed_answer(&target).then(|| session.to_owned())
        })
        .collect()
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
    /// The orchestrator's latest pending recap for this session or its
    /// subject ([`latest_recap`]), shown under its label; a proposal, never
    /// the author's text.
    pub recap: Option<ProposalView>,
    /// The orchestrator's pending answer proposal for this session
    /// ([`latest_answer`], AD-182): shown beside the composer under its
    /// label, never copied into it, sent only by the author's click.
    pub proposed_answer: Option<ProposalView>,
    pub changes: Changes,
    /// `claude logs`, for a background session only.
    pub logs: Option<Logs>,
    pub delivery: Delivery,
    /// The most characters an answer holds ([`ANSWER_MAX_CHARS`]), for
    /// the composer's counter.
    pub answer_max_chars: u32,
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

/// The end of `claude logs` as its final screen shows it (D-1020): the
/// output played on [`crate::screen`], no escape sequence or control
/// character left.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LogsText {
    pub text: Untrusted<String>,
    /// How many escape sequences the screen read; none reaches the text.
    pub escapes_stripped: u32,
    /// Bytes dropped from the start to keep [`LOGS_KEPT_BYTES`].
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub dropped_bytes: u64,
    /// The first line, from 1, holding a [`LOG_ERROR_MARKERS`] entry.
    pub first_error_line: Option<u32>,
}

// ---------------------------------------------------------------- answers

/// The most characters an answer holds (FR-6).
pub const ANSWER_MAX_CHARS: usize = 4_000;

/// The first Claude Code version whose `claude --resume` sends a prompt to
/// a live background session as its next turn (S3, FR-15, D-139): below
/// it, or when the version is unknown, a live idle background session's
/// answer is copied and `claude attach` opens.
pub const RESUME_PROMPT_MIN_VERSION: [u32; 3] = [2, 1, 285];

/// What gate 3 recorded (research notes E21, D-127): a resume delivers the
/// answer to a live background session as its next turn, from the
/// session's folder. Were it false, [`delivery`] would never return
/// [`Delivery::ResumeLive`].
pub const RESUME_LIVE_DELIVERS: bool = true;

/// How long after an answer's spawn the collector watches the session for
/// `busy`, its confirmation (AD-85, D-160).
pub const CONFIRM_WINDOW_MS: u64 = 30_000;

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why an answer's text is refused (AD-85, FR-6). E20 found the text
    /// delivered byte for byte after `--`, so a leading `-` is allowed
    /// (D-127).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum AnswerRefusal {
        /// No character, or whitespace only.
        Empty,
        /// Over [`ANSWER_MAX_CHARS`] characters.
        TooLong,
        /// A control character other than newline and tab.
        ControlCharacter,
        /// A character of `FORBIDDEN_FORMAT_RANGES`: bidi or invisible
        /// formatting (AD-30).
        FormatCharacter,
        /// Starts, after leading whitespace, with `/`: Claude Code would
        /// read a command (S3).
        LeadingSlash,
        /// Starts, after leading whitespace, with `!`: Claude Code would
        /// run a shell command (S3).
        LeadingBang,
    }
}

/// The author's answer to a session, validated (AD-85, FR-6): the only
/// text the `Answer` vector puts on a command line.
#[derive(Clone, PartialEq, Eq)]
pub struct Answer(String);

impl std::fmt::Debug for Answer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Answer({} chars)", self.0.chars().count())
    }
}

impl Answer {
    /// `text` as an answer, or the first rule it breaks, in the order of
    /// [`AnswerRefusal`].
    ///
    /// ```
    /// use officina_core::meeting::{Answer, AnswerRefusal};
    /// assert!(Answer::parse("Keep the 1.x line.").is_ok());
    /// assert_eq!(Answer::parse("  /clear").unwrap_err(), AnswerRefusal::LeadingSlash);
    /// ```
    pub fn parse(text: &str) -> Result<Answer, AnswerRefusal> {
        if text.trim().is_empty() {
            return Err(AnswerRefusal::Empty);
        }
        if text.chars().count() > ANSWER_MAX_CHARS {
            return Err(AnswerRefusal::TooLong);
        }
        if text
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(AnswerRefusal::ControlCharacter);
        }
        if text.chars().any(crate::relay::params::is_forbidden_format) {
            return Err(AnswerRefusal::FormatCharacter);
        }
        match text.trim_start().chars().next() {
            Some('/') => Err(AnswerRefusal::LeadingSlash),
            Some('!') => Err(AnswerRefusal::LeadingBang),
            _ => Ok(Answer(text.to_owned())),
        }
    }

    /// The text, for the `Answer` vector only.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// SHA-256 of the text, lowercase hex: what the journal keeps of it.
    pub fn sha256(&self) -> String {
        crate::sha256_hex(self.0.as_bytes())
    }
}

/// True when `version`, as `claude --version` prints it, sends a resumed
/// prompt to a live background session; false when older or unreadable.
///
/// ```
/// use officina_core::meeting::resumes_with_a_prompt;
/// assert!(resumes_with_a_prompt(Some("2.1.312 (Claude Code)")));
/// assert!(!resumes_with_a_prompt(Some("2.1.284 (Claude Code)")));
/// assert!(!resumes_with_a_prompt(None));
/// ```
pub fn resumes_with_a_prompt(version: Option<&str>) -> bool {
    version
        .and_then(crate::contract::claude_version)
        .is_some_and(|v| v >= RESUME_PROMPT_MIN_VERSION)
}

/// Why `send_answer` sent nothing (AD-85).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum AnswerRefused {
    /// The delivery the latest board gives differs from the one shown
    /// (D-63's pattern): the panel reads the meeting again.
    StateChanged,
    /// The text breaks a rule.
    Answer(AnswerRefusal),
    /// A live interactive session: Focus only, no composer.
    NoComposer,
    /// The session is no longer on the board, nor in the registry.
    UnknownSession,
    /// The proposal sent is not a pending answer proposal for this
    /// session: unknown, of another kind or session, or already decided
    /// (AD-182, D-575).
    ProposalNotPending,
}

/// How an answer went (AD-85): what the window shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "outcome", content = "value", rename_all = "snake_case")]
pub enum AnswerResult {
    Refused(AnswerRefused),
    /// Copied only: nothing was started (D-168).
    Copied,
    /// Copied, and `claude attach` opened.
    Attached,
    /// Resumed, and the session turned busy within the window.
    Confirmed,
    /// Resumed, but the session was not seen busy within the window.
    NotConfirmed,
    /// The console could not start, with why.
    SpawnFailed(String),
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
    /// The registry's proposal rows, newest first (AD-91).
    pub proposals: &'a [ProposalView],
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
        recap: session
            .and_then(|sid| latest_recap(input.proposals, sid, card.subject_id))
            .cloned(),
        proposed_answer: session
            .and_then(|sid| latest_answer(input.proposals, sid))
            .cloned(),
        changes: changes_of(&input.git),
        logs: is_background(card).then(|| logs_of(input.logs.as_ref())),
        delivery: input.delivery,
        answer_max_chars: u32::try_from(ANSWER_MAX_CHARS).unwrap_or(u32::MAX),
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

/// `text` with each line's trailing spaces dropped and every run of blank
/// lines made one: the session's final screen keeps the blank rows between
/// what was said and its footer (E23, E100), so the kept end holds more of
/// what was said.
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
            let rendered = crate::screen::render(bytes);
            let squeezed = squeeze_blank_lines(&rendered.text);
            let (kept, dropped_bytes) = keep_end(&squeezed);
            let text = Untrusted::new(kept.to_owned());
            let first_error_line = text.first_line_matching(LOG_ERROR_MARKERS);
            Logs::Shown(LogsText {
                text,
                escapes_stripped: rendered.sequences,
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

    /// A `claude` that resumes with a prompt.
    const NEW_CLAUDE: &str = "2.1.312 (Claude Code)";

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
            proposals: &[],
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

    /// Story 5.17 AC 5 (AD-84, FR-5, D-791): the recap section is the
    /// latest pending recap for the session or its subject, a proposal
    /// whose texts stay the orchestrator's; a decided one, another
    /// session's and a work order proposal are never it.
    #[test]
    fn the_latest_recap_shows_under_its_label() {
        use crate::orders::proposal::{
            ProposalContent, ProposalFile, ProposalTarget, ProposedSubject, RecapProposal,
            WorkOrderProposal, proposal_id,
        };
        use crate::registry::types::{ProposalKind, ProposalStatus};
        let mut card = background("idle", "done", r#","pid":7"#);
        let sid = card.session_id.clone().unwrap();
        let recap = |at: i64, target: ProposalTarget, done: &str, status| ProposalView {
            status,
            ..ProposalFile::recap(
                RecapProposal {
                    target,
                    done: done.to_owned(),
                    blocking: "nothing".to_owned(),
                    decision_expected: "merge?".to_owned(),
                },
                None,
                at,
            )
            .pending_view(&proposal_id(at, 1))
        };
        let order = ProposalFile::work_order(
            WorkOrderProposal {
                template: "dependency-bump".to_owned(),
                project: "backend".to_owned(),
                values: std::collections::BTreeMap::new(),
                subject: ProposedSubject::Id(4),
                rationale: "later".to_owned(),
            },
            None,
            NOW - 1,
        )
        .pending_view(&proposal_id(NOW - 1, 1));
        // Never the recap: a work order, a decided recap of the session,
        // another session's pending recap.
        let never = vec![
            order,
            recap(
                NOW - 2,
                ProposalTarget::SessionId(sid.clone()),
                "dismissed",
                ProposalStatus::Dismissed,
            ),
            recap(
                NOW - 3,
                ProposalTarget::SessionId("another-session".to_owned()),
                "theirs",
                ProposalStatus::Pending,
            ),
        ];
        let mut rows = never.clone();
        rows.extend([
            recap(
                NOW - 4,
                ProposalTarget::SubjectId(4),
                "the subject's <b>recap</b>",
                ProposalStatus::Pending,
            ),
            recap(
                NOW - 5,
                ProposalTarget::SessionId(sid.clone()),
                "the session's recap",
                ProposalStatus::Pending,
            ),
        ]);
        let done_of = |view: &MeetingView| match view.recap.as_ref().and_then(|r| r.content.clone())
        {
            Some(ProposalContent::Recap { done, .. }) => serde_json::to_value(&done).unwrap(),
            other => panic!("{other:?}"),
        };
        let with = |card: &MeetingCard, rows: &[ProposalView]| {
            build(&MeetingInput {
                proposals: rows,
                ..input(card, &[])
            })
        };
        // Unfiled, the session's own recap.
        let view = with(&card, &rows);
        assert_eq!(done_of(&view), "the session's recap");
        // Filed under subject 4, the subject's newer one.
        card.subject_id = Some(4);
        let view = with(&card, &rows);
        let shown = view.recap.as_ref().unwrap();
        assert_eq!(
            (shown.kind, shown.status),
            (Some(ProposalKind::Recap), ProposalStatus::Pending)
        );
        assert_eq!(done_of(&view), "the subject's <b>recap</b>");
        // Its texts go out as the orchestrator wrote them, under `recap`.
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(
            json["recap"]["content"]["recap"]["done"],
            "the subject's <b>recap</b>"
        );
        // No pending recap of its own or its subject's: none.
        assert!(with(&card, &never).recap.is_none());
        assert!(build(&input(&card, &[])).recap.is_none());
    }

    /// Story 5.21 AC 4 (AD-182): the meeting carries the session's pending
    /// answer proposal, its texts the orchestrator's; a decided one and
    /// another session's never show, and the recap section ignores it.
    #[test]
    fn the_pending_answer_shows_beside_the_composer() {
        use crate::orders::proposal::{AnswerProposal, ProposalContent, ProposalFile, proposal_id};
        use crate::registry::types::ProposalStatus;
        let card = background("idle", "done", r#","pid":7"#);
        let sid = card.session_id.clone().unwrap();
        let answer = |at: i64, session: &str, text: &str, status| ProposalView {
            status,
            ..ProposalFile::answer(
                AnswerProposal {
                    session_id: session.to_owned(),
                    text: text.to_owned(),
                    rationale: "It asked which line.".to_owned(),
                },
                None,
                at,
            )
            .pending_view(&proposal_id(at, 1))
        };
        let rows = vec![
            answer(NOW - 1, &sid, "dismissed", ProposalStatus::Dismissed),
            answer(
                NOW - 2,
                "another-session",
                "theirs",
                ProposalStatus::Pending,
            ),
            answer(
                NOW - 3,
                &sid,
                "Keep the <b>1.x</b> line.",
                ProposalStatus::Pending,
            ),
        ];
        let view = build(&MeetingInput {
            proposals: &rows,
            ..input(&card, &[])
        });
        assert!(view.recap.is_none());
        let shown = view.proposed_answer.as_ref().expect("the pending answer");
        assert_eq!(shown.id, rows[2].id);
        let Some(ProposalContent::Answer { text, .. }) = &shown.content else {
            panic!("{shown:?}")
        };
        assert_eq!(
            serde_json::to_value(text).unwrap(),
            "Keep the <b>1.x</b> line."
        );
        assert!(
            build(&MeetingInput {
                proposals: &rows[..2],
                ..input(&card, &[])
            })
            .proposed_answer
            .is_none()
        );
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

    #[test]
    fn the_logs_show_the_final_screen_once() {
        let frame =
            |body: &str| format!("\u{1b}[H\u{1b}[1mbanner\u{1b}[0m\u{1b}[K\r\n{body}\u{1b}[K\r\n");
        let raw = format!(
            "\u{1b}[2J{}{}",
            frame("Determining… (3s)"),
            frame("the answer")
        );
        let escapes = raw.matches('\u{1b}').count();
        let Logs::Shown(logs) = logs_of(Some(&LogsRead::Output(raw.into_bytes()))) else {
            panic!()
        };
        assert_eq!(
            serde_json::to_value(&logs.text).unwrap(),
            "banner\nthe answer\n"
        );
        assert_eq!(logs.escapes_stripped as usize, escapes);
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
        assert_eq!(serde_json::to_value(&squeezed.text).unwrap(), "said\n");
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
                folder,
                Delivery::ResumeLive,
            ),
            (
                background("busy", "working", r#","pid":7"#),
                folder,
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
                claude_version: Some(NEW_CLAUDE),
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
            agents_folder: Some("C:/path/to/repo"),
            claude_version: Some(NEW_CLAUDE),
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
                claude_version: Some(NEW_CLAUDE),
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
            (background("idle", "done", r#","pid":7"#), folder),
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
                claude_version: Some(NEW_CLAUDE),
            })
        })
        .collect();
        for variant in Delivery::ALL {
            assert!(reached.contains(variant), "{variant:?} has no case");
        }
    }

    #[test]
    fn answer_bounds() {
        assert_eq!(Answer::parse("").unwrap_err(), AnswerRefusal::Empty);
        assert_eq!(Answer::parse(" \n\t ").unwrap_err(), AnswerRefusal::Empty);
        assert!(Answer::parse("y").is_ok());
        let longest = "é".repeat(ANSWER_MAX_CHARS);
        assert!(
            Answer::parse(&longest).is_ok(),
            "the limit counts characters"
        );
        let over = format!("{longest}a");
        assert_eq!(Answer::parse(&over).unwrap_err(), AnswerRefusal::TooLong);
        // A leading dash stays allowed: E20's `--resume` cases passed.
        assert!(Answer::parse("-x keep it").is_ok());
        assert!(Answer::parse("line one\n\tline two").is_ok());
    }

    #[test]
    fn answer_refuses_controls_and_format_characters() {
        for c in ('\u{0}'..='\u{9f}').filter(|c| c.is_control() && *c != '\n' && *c != '\t') {
            assert_eq!(
                Answer::parse(&format!("ok {c} ok")).unwrap_err(),
                AnswerRefusal::ControlCharacter,
                "{:?}",
                c
            );
        }
        for range in &crate::relay::params::FORBIDDEN_FORMAT_RANGES {
            for c in [*range.start(), *range.end()] {
                let found = Answer::parse(&format!("ok {c} ok")).unwrap_err();
                // A format character that is also a control is refused as
                // one or the other, never accepted.
                assert!(
                    matches!(
                        found,
                        AnswerRefusal::FormatCharacter | AnswerRefusal::ControlCharacter
                    ),
                    "{c:?}: {found:?}"
                );
            }
        }
    }

    #[test]
    fn answer_refuses_a_leading_slash_or_bang_after_whitespace() {
        for (text, refusal) in [
            ("/clear", AnswerRefusal::LeadingSlash),
            ("  \n\t/compact", AnswerRefusal::LeadingSlash),
            ("!rm -rf x", AnswerRefusal::LeadingBang),
            ("\n !ls", AnswerRefusal::LeadingBang),
        ] {
            assert_eq!(Answer::parse(text).unwrap_err(), refusal, "{text:?}");
        }
        assert!(Answer::parse("Use /tmp, then run !important checks.").is_ok());
    }

    #[test]
    fn every_answer_refusal_has_a_case() {
        let cases = [
            String::new(),
            "x".repeat(ANSWER_MAX_CHARS + 1),
            "a\u{7}b".to_owned(),
            "a\u{202e}b".to_owned(),
            "/x".to_owned(),
            "!x".to_owned(),
        ];
        let reached: Vec<AnswerRefusal> = cases
            .iter()
            .map(|text| Answer::parse(text).unwrap_err())
            .collect();
        for refusal in AnswerRefusal::ALL {
            assert!(reached.contains(refusal), "{refusal:?} has no case");
        }
    }

    /// A live idle background session, listed with its folder.
    fn live_idle() -> MeetingCard {
        background("idle", "done", r#","pid":7"#)
    }

    #[test]
    fn an_old_claude_falls_back_to_copy_and_attach() {
        let card = live_idle();
        let at = |version: &str| {
            delivery(&MeetingTarget {
                card: &card,
                agents_folder: Some("C:/path/to/repo"),
                claude_version: Some(version),
            })
        };
        assert_eq!(at("2.1.284 (Claude Code)"), Delivery::CopyAndAttach);
        assert_eq!(at("2.1.285 (Claude Code)"), Delivery::ResumeLive);
        assert_eq!(at("2.2.0 (Claude Code)"), Delivery::ResumeLive);
        // An ended session resumes whatever the version.
        let ended = background("idle", "stopped", "");
        let target = MeetingTarget {
            card: &ended,
            agents_folder: Some("C:/path/to/repo"),
            claude_version: Some("2.1.200 (Claude Code)"),
        };
        assert_eq!(delivery(&target), Delivery::ResumeEnded);
    }

    #[test]
    fn an_unknown_version_falls_back_to_copy_and_attach() {
        let card = live_idle();
        for version in [None, Some("Claude Code"), Some("")] {
            let target = MeetingTarget {
                card: &card,
                agents_folder: Some("C:/path/to/repo"),
                claude_version: version,
            };
            assert_eq!(delivery(&target), Delivery::CopyAndAttach, "{version:?}");
        }
    }

    /// D-127, E21: a live idle background session in a folder `claude
    /// agents` reported, on a recent enough `claude`, is resumed live
    /// exactly while the recorded gate 3 says a resume delivers (E21
    /// passed); without that folder, D-635 copies and attaches whatever the
    /// gate.
    #[test]
    fn resume_live_follows_gate_3() {
        let live = if RESUME_LIVE_DELIVERS {
            Delivery::ResumeLive
        } else {
            Delivery::CopyAndAttach
        };
        let card = live_idle();
        let with = |folder| {
            delivery(&MeetingTarget {
                card: &card,
                agents_folder: folder,
                claude_version: Some(NEW_CLAUDE),
            })
        };
        assert_eq!(with(Some("C:/path/to/repo")), live);
        assert_eq!(with(None), Delivery::CopyAndAttach);
    }
}
