//! Orchestrator proposals (AD-90, AD-91, AD-182, FR-11, FR-12): what a proposal
//! tool of `officina mcp` asks for, the file it writes under
//! `<home>/orchestrator/proposals/`, its name, its bounds and the rate
//! limits, and the view the window and the `proposals` tool carry
//! ([`ProposalView`], D-576).
//!
//! A file is a parse record (D-130): its strings stay plain until the
//! window validates them at ingestion (story 5.17), which a forged file
//! meets the same way. Every string a view carries from a file or a row is
//! [`Untrusted`]: the orchestrator wrote it.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::meeting::{ANSWER_MAX_CHARS, Answer};
use crate::registry::confirm::{NAME_MAX_CHARS, checked_name};
use crate::registry::types::{
    AuthorRefusal, ProposalKind, ProposalStatus, SubjectRecord, SubjectStatus,
};
use crate::relay::catalogue::CanonicalCatalogue;
use crate::relay::params::{TEMPLATE_VALUE_MAX_CHARS, is_forbidden_format};
use crate::untrusted::Untrusted;

/// The proposal file's format version (AD-91).
pub const PROPOSAL_FILE_SCHEMA: u32 = 1;

/// The most bytes a proposal file holds (AD-91): the writer refuses a
/// larger one, ingestion reads one byte more and stores it `invalid`.
pub const PROPOSAL_FILE_MAX_BYTES: usize = 16 * 1024;

/// The most characters of a proposal's text: each recap field and a work
/// order's rationale (AD-90).
pub const PROPOSAL_TEXT_MAX_CHARS: usize = 500;

/// The most characters of a proposed answer's text: the composer's own
/// bound (AD-182, FR-6).
pub const PROPOSAL_ANSWER_MAX_CHARS: usize = ANSWER_MAX_CHARS;

/// The most characters of a proposed value: a template value's own bound
/// (AD-90, FR-2).
pub const PROPOSAL_VALUE_MAX_CHARS: usize = TEMPLATE_VALUE_MAX_CHARS;

/// The most characters of a proposed new subject's name: a subject name's
/// own bound (AD-90, D-57).
pub const PROPOSAL_NEW_SUBJECT_MAX_CHARS: usize = NAME_MAX_CHARS;

/// The most values a work order proposal names (AD-90).
pub const PROPOSAL_MAX_VALUES: usize = 16;

/// The most proposals written within [`PROPOSAL_HOUR_MS`] (AD-90).
pub const PROPOSALS_PER_HOUR: usize = 30;

/// The window the hourly limit counts over.
pub const PROPOSAL_HOUR_MS: i64 = 60 * 60 * 1000;

/// The most proposals pending at once, files not ingested yet included
/// (AD-90, D-145: recaps count too).
pub const PROPOSALS_PENDING_MAX: usize = 200;

/// How far back the `proposals` tool lists (D-145).
pub const PROPOSALS_LISTED_FOR_MS: i64 = 7 * 24 * PROPOSAL_HOUR_MS;

/// The most proposals the `proposals` tool lists (D-145).
pub const PROPOSALS_LISTED: usize = 50;

/// Digits of a proposal id's time part: Unix milliseconds until 2286.
const ID_TIME_DIGITS: usize = 13;

/// Hex digits of a proposal id's random part.
const ID_NONCE_DIGITS: usize = 16;

/// The file name extension of a proposal file.
const FILE_EXTENSION: &str = ".json";

/// The id of a proposal written at `now_ms` with `nonce`:
/// `<now_ms>-<16 hex>` (AD-91).
///
/// ```
/// use officina_core::orders::proposal::{proposal_id, written_at};
/// let id = proposal_id(1_761_700_000_000, 0xab);
/// assert_eq!(id, "1761700000000-00000000000000ab");
/// assert_eq!(written_at(&id), Some(1_761_700_000_000));
/// ```
pub fn proposal_id(now_ms: i64, nonce: u64) -> String {
    format!("{now_ms:0ID_TIME_DIGITS$}-{nonce:0ID_NONCE_DIGITS$x}")
}

/// When the proposal `id` was written, when `id` is one (`^[0-9]{13}-
/// [0-9a-f]{16}$`, D-155); `None` for any other text.
///
/// ```
/// use officina_core::orders::proposal::written_at;
/// assert_eq!(written_at("0000000000001-0123456789abcdef"), Some(1));
/// assert_eq!(written_at("0000000000001-0123456789ABCDEF"), None);
/// assert_eq!(written_at("1-0123456789abcdef"), None);
/// assert_eq!(written_at("../0000000001-0123456789abcdef"), None);
/// ```
pub fn written_at(id: &str) -> Option<i64> {
    let (time, nonce) = id.split_once('-')?;
    let digits = time.len() == ID_TIME_DIGITS && time.bytes().all(|b| b.is_ascii_digit());
    let hex = nonce.len() == ID_NONCE_DIGITS
        && nonce
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if digits && hex {
        time.parse().ok()
    } else {
        None
    }
}

/// The file name of proposal `id`.
pub fn file_name(id: &str) -> String {
    format!("{id}{FILE_EXTENSION}")
}

/// The proposal id a file named `name` holds, when the name is exactly a
/// proposal file's: a temporary file, or any other, holds none.
///
/// ```
/// use officina_core::orders::proposal::id_of_file;
/// let name = "1761700000000-00000000000000ab.json";
/// assert_eq!(id_of_file(name), Some("1761700000000-00000000000000ab"));
/// assert_eq!(id_of_file("1761700000000-00000000000000ab.json.42.tmp"), None);
/// ```
pub fn id_of_file(name: &str) -> Option<&str> {
    name.strip_suffix(FILE_EXTENSION)
        .filter(|id| written_at(id).is_some())
}

/// The files of `proposals/` ingestion deleted unread since the window
/// started (D-155): misnamed ones, and those past the hourly or pending
/// limits. Counted in memory and shown in the Orchestrator view.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DeletedFiles {
    pub misnamed: u32,
    pub over_limit: u32,
}

/// What a recap is about: a session or a subject (AD-90).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ProposalTarget {
    SessionId(String),
    SubjectId(i64),
}

/// The subject a work order proposal files under: an existing one by id,
/// or a new one by a name the author may type (D-152).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ProposedSubject {
    Id(i64),
    New(String),
}

/// `propose_recap`'s arguments (AD-90).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecapProposal {
    pub target: ProposalTarget,
    pub done: String,
    pub blocking: String,
    pub decision_expected: String,
}

/// `propose_work_order`'s arguments (AD-90).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkOrderProposal {
    pub template: String,
    pub project: String,
    /// Each parameter's proposed value.
    pub values: BTreeMap<String, String>,
    pub subject: ProposedSubject,
    pub rationale: String,
}

/// `propose_answer`'s arguments (AD-182): the session to answer, the text
/// proposed as its next prompt and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerProposal {
    pub session_id: String,
    pub text: String,
    pub rationale: String,
}

/// A recap's file (AD-91): its schema, its fields, the session that wrote
/// it and when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecapFile {
    pub schema: u32,
    pub target: ProposalTarget,
    pub done: String,
    pub blocking: String,
    pub decision_expected: String,
    /// `CLAUDE_CODE_SESSION_ID` of the server's process: a label, not a
    /// credential (AD-91).
    pub orchestrator_session_id: Option<String>,
    pub written_at_ms: i64,
}

/// A work order proposal's file (AD-91).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkOrderFile {
    pub schema: u32,
    pub template: String,
    pub project: String,
    pub values: BTreeMap<String, String>,
    pub subject: ProposedSubject,
    pub rationale: String,
    pub orchestrator_session_id: Option<String>,
    pub written_at_ms: i64,
}

/// An answer proposal's file (AD-182).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerFile {
    pub schema: u32,
    pub session_id: String,
    pub text: String,
    pub rationale: String,
    pub orchestrator_session_id: Option<String>,
    pub written_at_ms: i64,
}

/// A proposal file as written and parsed (AD-91), its kind the tag:
/// one flat object, unknown fields refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProposalFile {
    Recap(RecapFile),
    WorkOrder(WorkOrderFile),
    Answer(AnswerFile),
}

impl ProposalFile {
    /// The file of a recap proposed at `now_ms` by `session`.
    pub fn recap(proposal: RecapProposal, session: Option<String>, now_ms: i64) -> ProposalFile {
        ProposalFile::Recap(RecapFile {
            schema: PROPOSAL_FILE_SCHEMA,
            target: proposal.target,
            done: proposal.done,
            blocking: proposal.blocking,
            decision_expected: proposal.decision_expected,
            orchestrator_session_id: session,
            written_at_ms: now_ms,
        })
    }

    /// The file of a work order proposed at `now_ms` by `session`.
    pub fn work_order(
        proposal: WorkOrderProposal,
        session: Option<String>,
        now_ms: i64,
    ) -> ProposalFile {
        ProposalFile::WorkOrder(WorkOrderFile {
            schema: PROPOSAL_FILE_SCHEMA,
            template: proposal.template,
            project: proposal.project,
            values: proposal.values,
            subject: proposal.subject,
            rationale: proposal.rationale,
            orchestrator_session_id: session,
            written_at_ms: now_ms,
        })
    }

    /// The file of an answer proposed at `now_ms` by `session` (AD-182).
    pub fn answer(proposal: AnswerProposal, session: Option<String>, now_ms: i64) -> ProposalFile {
        ProposalFile::Answer(AnswerFile {
            schema: PROPOSAL_FILE_SCHEMA,
            session_id: proposal.session_id,
            text: proposal.text,
            rationale: proposal.rationale,
            orchestrator_session_id: session,
            written_at_ms: now_ms,
        })
    }

    /// Its kind.
    pub fn kind(&self) -> ProposalKind {
        match self {
            ProposalFile::Recap(_) => ProposalKind::Recap,
            ProposalFile::WorkOrder(_) => ProposalKind::WorkOrder,
            ProposalFile::Answer(_) => ProposalKind::Answer,
        }
    }

    /// The file's bytes: one line of JSON.
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("a proposal file always serializes")
    }

    /// The view of this file before the window ingested it: `pending`, its
    /// texts untrusted.
    pub fn pending_view(&self, id: &str) -> ProposalView {
        let (session, written_at_ms, content) = match self {
            ProposalFile::Recap(file) => (
                &file.orchestrator_session_id,
                file.written_at_ms,
                ProposalContent::Recap {
                    target: TargetView::of(&file.target),
                    done: untrusted(&file.done),
                    blocking: untrusted(&file.blocking),
                    decision_expected: untrusted(&file.decision_expected),
                },
            ),
            ProposalFile::WorkOrder(file) => (
                &file.orchestrator_session_id,
                file.written_at_ms,
                ProposalContent::WorkOrder {
                    template: untrusted(&file.template),
                    project: untrusted(&file.project),
                    values: Untrusted::new(file.values.clone()),
                    subject: ProposedSubjectView::of(&file.subject),
                    rationale: untrusted(&file.rationale),
                },
            ),
            ProposalFile::Answer(file) => (
                &file.orchestrator_session_id,
                file.written_at_ms,
                ProposalContent::Answer {
                    session_id: untrusted(&file.session_id),
                    text: untrusted(&file.text),
                    rationale: untrusted(&file.rationale),
                },
            ),
        };
        ProposalView {
            id: id.to_owned(),
            kind: Some(self.kind()),
            status: ProposalStatus::Pending,
            invalid_reason: None,
            written_at_ms: written_at(id).unwrap_or(written_at_ms),
            received_at_ms: None,
            orchestrator_session_id: session.as_deref().map(untrusted),
            content: Some(content),
        }
    }
}

fn untrusted(text: &str) -> Untrusted<String> {
    Untrusted::new(text.to_owned())
}

/// A recap's target as a view carries it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum TargetView {
    SessionId(Untrusted<String>),
    SubjectId(i64),
}

impl TargetView {
    /// The view of `target`.
    pub fn of(target: &ProposalTarget) -> TargetView {
        match target {
            ProposalTarget::SessionId(id) => TargetView::SessionId(untrusted(id)),
            ProposalTarget::SubjectId(id) => TargetView::SubjectId(*id),
        }
    }
}

/// A work order proposal's subject as a view carries it: a proposed new
/// name is a suggestion, never a default (D-152).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum ProposedSubjectView {
    Id(i64),
    New(Untrusted<String>),
}

impl ProposedSubjectView {
    /// The view of `subject`.
    pub fn of(subject: &ProposedSubject) -> ProposedSubjectView {
        match subject {
            ProposedSubject::Id(id) => ProposedSubjectView::Id(*id),
            ProposedSubject::New(name) => ProposedSubjectView::New(untrusted(name)),
        }
    }
}

/// What a proposal proposes, every text the orchestrator's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum ProposalContent {
    Recap {
        target: TargetView,
        done: Untrusted<String>,
        blocking: Untrusted<String>,
        decision_expected: Untrusted<String>,
    },
    WorkOrder {
        template: Untrusted<String>,
        project: Untrusted<String>,
        /// Each parameter's proposed value, names included: the whole map
        /// is the orchestrator's text.
        values: Untrusted<BTreeMap<String, String>>,
        subject: ProposedSubjectView,
        rationale: Untrusted<String>,
    },
    /// A text proposed as a session's next prompt (AD-182): shown beside
    /// the author's composer, never copied into it, sent only by his click.
    Answer {
        session_id: Untrusted<String>,
        text: Untrusted<String>,
        rationale: Untrusted<String>,
    },
}

/// One orchestrator proposal as the views and the `proposals` tool carry
/// it (D-576): a file not ingested yet is `pending`, a registry row has the
/// status the window gave it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ProposalView {
    pub id: String,
    /// `null` for a file that did not parse (D-133).
    pub kind: Option<ProposalKind>,
    pub status: ProposalStatus,
    pub invalid_reason: Option<Untrusted<String>>,
    /// When it was written, from its id.
    pub written_at_ms: i64,
    /// When the window ingested it; `null` while it waits as a file.
    pub received_at_ms: Option<i64>,
    /// The orchestrator session that wrote it, a label.
    pub orchestrator_session_id: Option<Untrusted<String>>,
    /// `null` when it did not parse.
    pub content: Option<ProposalContent>,
}

impl ProposalView {
    /// A proposal file that waits for ingestion but does not parse: its
    /// status is the window's to give.
    pub fn unreadable_file(id: &str) -> ProposalView {
        ProposalView {
            id: id.to_owned(),
            kind: None,
            status: ProposalStatus::Pending,
            invalid_reason: None,
            written_at_ms: written_at(id).unwrap_or_default(),
            received_at_ms: None,
            orchestrator_session_id: None,
            content: None,
        }
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why a proposal tool writes no file: a limit of AD-90 reached.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum RateRefusal {
        /// [`PROPOSALS_PER_HOUR`] written within the hour.
        HourlyLimit,
        /// [`PROPOSALS_PENDING_MAX`] waiting for the author.
        PendingLimit,
    }
}

impl RateRefusal {
    /// What the tool says, from the limit's constant.
    pub fn reason(self) -> String {
        match self {
            RateRefusal::HourlyLimit => format!(
                "{PROPOSALS_PER_HOUR} proposals were written in the last hour; write the next one later"
            ),
            RateRefusal::PendingLimit => format!(
                "{PROPOSALS_PENDING_MAX} proposals wait for the author; write the next one once some are decided"
            ),
        }
    }
}

/// The rate limits (AD-90): `written` holds when each known proposal was
/// written (the files and the registry rows, each id once), `pending` how
/// many wait. The hourly limit is checked first. A time after `now_ms` is
/// not within the hour: an id's time is its writer's choice, and a file
/// dated ahead would otherwise count as written this hour until retention
/// removed it, stopping the orchestrator's proposal tools (D-964).
///
/// ```
/// use officina_core::orders::proposal::{PROPOSALS_PER_HOUR, RateRefusal, rate_refusal};
/// let now = 10_000_000;
/// let written = vec![now - 1; PROPOSALS_PER_HOUR];
/// assert_eq!(rate_refusal(now, &written, 0), Some(RateRefusal::HourlyLimit));
/// assert_eq!(rate_refusal(now, &written[1..], 0), None);
/// ```
pub fn rate_refusal(now_ms: i64, written: &[i64], pending: usize) -> Option<RateRefusal> {
    let this_hour = written
        .iter()
        .filter(|at| (now_ms - PROPOSAL_HOUR_MS + 1..=now_ms).contains(*at))
        .count();
    if this_hour >= PROPOSALS_PER_HOUR {
        Some(RateRefusal::HourlyLimit)
    } else if pending >= PROPOSALS_PENDING_MAX {
        Some(RateRefusal::PendingLimit)
    } else {
        None
    }
}

/// What the `proposals` tool lists (D-145): the proposals of the last
/// [`PROPOSALS_LISTED_FOR_MS`], whatever session wrote them, of `status`
/// when one is asked, newest first, at most [`PROPOSALS_LISTED`].
pub fn listed(
    mut proposals: Vec<ProposalView>,
    status: Option<ProposalStatus>,
    now_ms: i64,
) -> Vec<ProposalView> {
    proposals.retain(|p| {
        p.written_at_ms > now_ms - PROPOSALS_LISTED_FOR_MS && status.is_none_or(|s| p.status == s)
    });
    proposals.sort_by(|a, b| {
        b.written_at_ms
            .cmp(&a.written_at_ms)
            .then_with(|| b.id.cmp(&a.id))
    });
    proposals.truncate(PROPOSALS_LISTED);
    proposals
}

/// The proposals known from `rows` (the registry's) and `files` (written,
/// not ingested yet), each id once: a row wins, since its status is the
/// window's.
pub fn merged(rows: Vec<ProposalView>, files: Vec<ProposalView>) -> Vec<ProposalView> {
    let mut by_id: BTreeMap<String, ProposalView> =
        files.into_iter().map(|p| (p.id.clone(), p)).collect();
    by_id.extend(rows.into_iter().map(|p| (p.id.clone(), p)));
    by_id.into_values().collect()
}

/// How long a work order proposal stays `pending` after the window
/// received it (AD-91, D-145): past it, it is `expired`. Recaps do not
/// expire.
pub const WORK_ORDER_PROPOSAL_TTL_MS: i64 = 24 * PROPOSAL_HOUR_MS;

/// How old a temporary file of `proposals/` gets before ingestion deletes
/// it, unread (D-155): a writer killed between its write and its rename
/// left it.
pub const TMP_FILE_TTL_MS: i64 = 24 * PROPOSAL_HOUR_MS;

/// How many invalid proposals the Orchestrator view lists, newest first
/// (D-155).
pub const PROPOSALS_SHOWN_INVALID: usize = 50;

/// How many decided proposals (accepted, dismissed, expired, superseded)
/// the Orchestrator view lists, newest first (AD-92).
pub const PROPOSALS_SHOWN_DECIDED: usize = 50;

/// The extension of a temporary file: `atomic_write`'s
/// `<name>.<pid>.tmp`, which ingestion never reads (D-155).
const TMP_EXTENSION: &str = ".tmp";

/// True when the file named `name` is a temporary file, never read and
/// deleted once older than [`TMP_FILE_TTL_MS`].
///
/// ```
/// use officina_core::orders::proposal::is_temporary_file;
/// assert!(is_temporary_file("1761700000000-00000000000000ab.json.42.tmp"));
/// assert!(!is_temporary_file("1761700000000-00000000000000ab.json"));
/// ```
pub fn is_temporary_file(name: &str) -> bool {
    name.ends_with(TMP_EXTENSION)
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why the window stored a proposal `invalid` (AD-91, D-133, D-155):
    /// the file's reading, or the first check of [`validate`] that failed,
    /// in its order. `proposal.invalid_reason` has no `CHECK`, so a reason
    /// added here needs no migration.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ProposalInvalidReason {
        /// Not JSON, an unknown field or kind, or another schema: stored
        /// with no kind (D-133).
        Unparsable,
        /// Over [`PROPOSAL_FILE_MAX_BYTES`]: stored with no kind and no
        /// texts (D-155).
        Oversized,
        /// A recap field or a rationale over [`PROPOSAL_TEXT_MAX_CHARS`].
        TextTooLong,
        /// A text holds a control, bidirectional or invisible character.
        ForbiddenCharacters,
        /// The recap's session is not in the registry.
        UnknownSession,
        /// The subject named is not in the registry.
        UnknownSubject,
        /// The work order's subject is archived or merged.
        SubjectClosed,
        /// The work order's subject belongs to another project.
        SubjectOtherProject,
        /// The proposed new subject's name breaks one of D-57's rules.
        SubjectNameInvalid,
        /// The approved snapshot holds no template of that name.
        UnknownTemplate,
        /// The project is not one of the template's.
        ProjectNotAllowed,
        /// A parameter the template does not declare.
        UnknownParameter,
        /// A parameter the template declares has no value.
        MissingParameter,
        /// A value is not, byte for byte, an entry of its list.
        ValueNotInList,
        /// An answer proposal's rationale is empty (AD-182).
        RationaleMissing,
        /// An answer proposal's text breaks one of the composer's rules
        /// ([`Answer::parse`]); which one is not stored, since the panel
        /// shows the text itself (AD-182).
        AnswerRefused,
        /// The session an answer names takes none now: not a live
        /// background session idle in its turn whose delivery is
        /// `ResumeLive` (AD-182).
        SessionNotAnswerable,
    }
}

/// What a proposal's names are checked against (AD-91, AD-182): every
/// subject row, the sessions the registry holds among those the files
/// name, and the sessions that take a proposed answer now, from the
/// latest board ([`crate::meeting::takes_a_proposed_answer`]).
#[derive(Debug, Clone, Copy)]
pub struct RegistryRefs<'a> {
    pub subjects: &'a [SubjectRecord],
    pub sessions: &'a BTreeSet<String>,
    pub answerable: &'a BTreeSet<String>,
}

/// A proposal that passed [`validate`]: a work order's names and values
/// are the approved template's own strings, never the file's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Validated {
    Recap,
    Answer,
    WorkOrder {
        template: String,
        project: String,
        values: BTreeMap<String, String>,
    },
}

/// The checks of a text the orchestrator wrote: its bound, then no
/// control character but a line end or a tab, and no bidirectional or
/// invisible one.
fn checked_text(text: &str) -> Result<(), ProposalInvalidReason> {
    if text.chars().count() > PROPOSAL_TEXT_MAX_CHARS {
        return Err(ProposalInvalidReason::TextTooLong);
    }
    let forbidden =
        |c: char| (c.is_control() && !matches!(c, '\n' | '\r' | '\t')) || is_forbidden_format(c);
    if text.chars().any(forbidden) {
        return Err(ProposalInvalidReason::ForbiddenCharacters);
    }
    Ok(())
}

/// Validates `raw` against the approved snapshot `catalogue` and the
/// registry (AD-91, FR-12): the window's check at ingestion, never the
/// tool's, so a file written by hand meets the same rules (amendment 7).
/// A recap's texts are within their bounds and its session or subject is
/// the registry's. A work order's rationale is within its bound, its
/// template is the snapshot's (none without a snapshot), its project one
/// of the template's, each declared parameter given once with a value of
/// its list, and its subject an open subject of the project or a new name
/// D-57 accepts (a taken name files into the subject holding it, D-158).
/// An answer's rationale is 1 to [`PROPOSAL_TEXT_MAX_CHARS`] characters
/// under the texts' character rules, its text passes the composer's
/// [`Answer::parse`] unchanged, and its session takes a proposed answer
/// now (AD-182). The first failure is the reason.
pub fn validate(
    raw: &ProposalFile,
    catalogue: Option<&CanonicalCatalogue>,
    refs: &RegistryRefs,
) -> Result<Validated, ProposalInvalidReason> {
    match raw {
        ProposalFile::Recap(file) => {
            for text in [&file.done, &file.blocking, &file.decision_expected] {
                checked_text(text)?;
            }
            match &file.target {
                ProposalTarget::SessionId(id) if !refs.sessions.contains(id) => {
                    Err(ProposalInvalidReason::UnknownSession)
                }
                ProposalTarget::SubjectId(id) if !refs.subjects.iter().any(|s| s.id == *id) => {
                    Err(ProposalInvalidReason::UnknownSubject)
                }
                _ => Ok(Validated::Recap),
            }
        }
        ProposalFile::Answer(file) => {
            if file.rationale.is_empty() {
                return Err(ProposalInvalidReason::RationaleMissing);
            }
            checked_text(&file.rationale)?;
            Answer::parse(&file.text).map_err(|_| ProposalInvalidReason::AnswerRefused)?;
            if refs.answerable.contains(&file.session_id) {
                Ok(Validated::Answer)
            } else {
                Err(ProposalInvalidReason::SessionNotAnswerable)
            }
        }
        ProposalFile::WorkOrder(file) => {
            checked_text(&file.rationale)?;
            let (name, template) = catalogue
                .and_then(|c| c.templates.get_key_value(&file.template))
                .ok_or(ProposalInvalidReason::UnknownTemplate)?;
            let project = template
                .projects
                .iter()
                .find(|p| **p == file.project)
                .ok_or(ProposalInvalidReason::ProjectNotAllowed)?;
            if file
                .values
                .keys()
                .any(|param| !template.params.contains_key(param))
            {
                return Err(ProposalInvalidReason::UnknownParameter);
            }
            let mut values = BTreeMap::new();
            for (param, list) in &template.params {
                let given = file
                    .values
                    .get(param)
                    .ok_or(ProposalInvalidReason::MissingParameter)?;
                let entry = list
                    .iter()
                    .find(|entry| entry.as_bytes() == given.as_bytes())
                    .ok_or(ProposalInvalidReason::ValueNotInList)?;
                values.insert(param.clone(), entry.clone());
            }
            checked_subject(&file.subject, project, refs.subjects)?;
            Ok(Validated::WorkOrder {
                template: name.clone(),
                project: project.clone(),
                values,
            })
        }
    }
}

/// The work order's subject: an open subject of `project`, or a new name
/// D-57 accepts.
fn checked_subject(
    subject: &ProposedSubject,
    project: &str,
    subjects: &[SubjectRecord],
) -> Result<(), ProposalInvalidReason> {
    match subject {
        ProposedSubject::Id(id) => {
            let found = subjects
                .iter()
                .find(|s| s.id == *id)
                .ok_or(ProposalInvalidReason::UnknownSubject)?;
            if found.project != project {
                Err(ProposalInvalidReason::SubjectOtherProject)
            } else if found.status.value == SubjectStatus::Archived {
                Err(ProposalInvalidReason::SubjectClosed)
            } else {
                Ok(())
            }
        }
        ProposedSubject::New(name) => match checked_name(name, project, None, subjects) {
            Ok(_) | Err(AuthorRefusal::NameTaken) => Ok(()),
            Err(_) => Err(ProposalInvalidReason::SubjectNameInvalid),
        },
    }
}

/// When a proposal of `kind` received at `received_at_ms` expires: a work
/// order's [`WORK_ORDER_PROPOSAL_TTL_MS`] later; a recap never (D-145); an
/// answer at no set time, but as soon as its session no longer takes one
/// (AD-182), which each board checks ([`expired_answers`]).
pub fn expires_at_ms(kind: ProposalKind, received_at_ms: i64) -> Option<i64> {
    match kind {
        ProposalKind::WorkOrder => Some(received_at_ms + WORK_ORDER_PROPOSAL_TTL_MS),
        ProposalKind::Recap | ProposalKind::Answer => None,
    }
}

/// True when a pending proposal whose expiry is `expires_at_ms` is
/// `expired` at `now_ms`.
pub fn is_expired(expires_at_ms: Option<i64>, now_ms: i64) -> bool {
    expires_at_ms.is_some_and(|at| now_ms >= at)
}

/// How a file of `proposals/` was read: its bytes, at most
/// [`PROPOSAL_FILE_MAX_BYTES`], or more than that (D-155).
#[derive(Debug, Clone, Copy)]
pub enum FileRead<'a> {
    Bytes(&'a [u8]),
    Oversized,
}

/// The `proposal` row ingestion stores for one file (AD-82, D-762): the
/// target in its two columns, the values' map, the subject as the file
/// writes it and the kind's texts by field name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalRecord {
    pub id: String,
    /// `None` for a file that did not parse or was oversized (D-133).
    pub kind: Option<ProposalKind>,
    pub status: ProposalStatus,
    pub invalid_reason: Option<ProposalInvalidReason>,
    pub received_at_ms: i64,
    pub expires_at_ms: Option<i64>,
    pub orchestrator_session_id: Option<String>,
    pub target: Option<ProposalTarget>,
    pub template: Option<String>,
    pub project: Option<String>,
    pub values: Option<BTreeMap<String, String>>,
    pub subject: Option<ProposedSubject>,
    pub texts: Option<BTreeMap<String, String>>,
}

impl ProposalRecord {
    /// The record of a file that holds no proposal the window can read:
    /// no kind, no texts.
    fn unread(id: &str, reason: ProposalInvalidReason, now_ms: i64) -> ProposalRecord {
        ProposalRecord {
            id: id.to_owned(),
            kind: None,
            status: ProposalStatus::Invalid,
            invalid_reason: Some(reason),
            received_at_ms: now_ms,
            expires_at_ms: None,
            orchestrator_session_id: None,
            target: None,
            template: None,
            project: None,
            values: None,
            subject: None,
            texts: None,
        }
    }

    /// What a valid recap or answer supersedes on (D-145, D-575): its
    /// kind and its target, for a pending recap or answer only.
    pub fn superseding(&self) -> Option<(ProposalKind, &ProposalTarget)> {
        let kind = self
            .kind
            .filter(|k| matches!(k, ProposalKind::Recap | ProposalKind::Answer))?;
        (self.status == ProposalStatus::Pending)
            .then_some(self.target.as_ref())
            .flatten()
            .map(|target| (kind, target))
    }
}

/// The row of the file `id`, read as `read`, at `now_ms` (AD-91, D-155):
/// `invalid` with `oversized` or `unparsable` and no kind when it holds no
/// proposal; else its kind and fields, `pending` when [`validate`] passes
/// (a work order's names and values then the template's strings, its
/// expiry set) or `invalid` with the reason and the texts as written.
pub fn record_of(
    id: &str,
    read: FileRead,
    catalogue: Option<&CanonicalCatalogue>,
    refs: &RegistryRefs,
    now_ms: i64,
) -> ProposalRecord {
    let bytes = match read {
        FileRead::Oversized => {
            return ProposalRecord::unread(id, ProposalInvalidReason::Oversized, now_ms);
        }
        FileRead::Bytes(bytes) => bytes,
    };
    let Some(raw) = serde_json::from_slice::<ProposalFile>(bytes)
        .ok()
        .filter(|raw| raw.schema() == PROPOSAL_FILE_SCHEMA)
    else {
        return ProposalRecord::unread(id, ProposalInvalidReason::Unparsable, now_ms);
    };
    let outcome = validate(&raw, catalogue, refs);
    let kind = raw.kind();
    let (status, invalid_reason, expires) = match &outcome {
        Ok(_) => (ProposalStatus::Pending, None, expires_at_ms(kind, now_ms)),
        Err(reason) => (ProposalStatus::Invalid, Some(*reason), None),
    };
    let mut record = ProposalRecord {
        id: id.to_owned(),
        kind: Some(kind),
        status,
        invalid_reason,
        received_at_ms: now_ms,
        expires_at_ms: expires,
        orchestrator_session_id: None,
        target: None,
        template: None,
        project: None,
        values: None,
        subject: None,
        texts: None,
    };
    match raw {
        ProposalFile::Recap(file) => {
            record.orchestrator_session_id = file.orchestrator_session_id;
            record.target = Some(file.target);
            record.texts = Some(BTreeMap::from([
                ("done".to_owned(), file.done),
                ("blocking".to_owned(), file.blocking),
                ("decision_expected".to_owned(), file.decision_expected),
            ]));
        }
        ProposalFile::WorkOrder(file) => {
            record.orchestrator_session_id = file.orchestrator_session_id;
            let (template, project, values) = match outcome {
                Ok(Validated::WorkOrder {
                    template,
                    project,
                    values,
                }) => (template, project, values),
                _ => (file.template, file.project, file.values),
            };
            record.template = Some(template);
            record.project = Some(project);
            record.values = Some(values);
            record.subject = Some(file.subject);
            record.texts = Some(BTreeMap::from([("rationale".to_owned(), file.rationale)]));
        }
        ProposalFile::Answer(file) => {
            record.orchestrator_session_id = file.orchestrator_session_id;
            record.target = Some(ProposalTarget::SessionId(file.session_id));
            record.texts = Some(BTreeMap::from([
                ("text".to_owned(), file.text),
                ("rationale".to_owned(), file.rationale),
            ]));
        }
    }
    record
}

impl ProposalFile {
    /// The file's format version.
    pub fn schema(&self) -> u32 {
        match self {
            ProposalFile::Recap(file) => file.schema,
            ProposalFile::WorkOrder(file) => file.schema,
            ProposalFile::Answer(file) => file.schema,
        }
    }
}

/// The pending proposals a newer valid one of the same kind on the same
/// target supersedes (D-145, D-575): of `pending`, all of one kind, every
/// one but the newest by id of its target.
///
/// ```
/// use officina_core::orders::proposal::{ProposalTarget, superseded_pending};
/// let pending = vec![
///     ("0000000000001-0000000000000001".to_owned(), ProposalTarget::SubjectId(4)),
///     ("0000000000002-0000000000000001".to_owned(), ProposalTarget::SubjectId(4)),
///     ("0000000000001-0000000000000002".to_owned(), ProposalTarget::SubjectId(5)),
/// ];
/// assert_eq!(superseded_pending(&pending), ["0000000000001-0000000000000001"]);
/// ```
pub fn superseded_pending(pending: &[(String, ProposalTarget)]) -> Vec<String> {
    let mut newest: BTreeMap<&ProposalTarget, &str> = BTreeMap::new();
    for (id, target) in pending {
        let kept = newest.entry(target).or_insert(id);
        if id.as_str() > *kept {
            *kept = id;
        }
    }
    let mut superseded: Vec<String> = pending
        .iter()
        .filter(|(id, target)| newest.get(target) != Some(&id.as_str()))
        .map(|(id, _)| id.clone())
        .collect();
    superseded.sort();
    superseded
}

/// A proposal the registry or this ingestion pass knows, for the limits
/// held again at ingestion (AD-90, D-769).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownProposal {
    pub id: String,
    pub pending: bool,
}

/// The rate limit the file `id` breaks at ingestion (AD-90, D-155), counted
/// as the tool counted when it wrote it: the known proposals written
/// before it, within the hour before its own time, and those of them
/// pending. A file written directly is held to the same limits.
pub fn ingestion_refusal(id: &str, known: &[KnownProposal]) -> Option<RateRefusal> {
    let at = written_at(id)?;
    let earlier = || known.iter().filter(|k| k.id.as_str() < id);
    let times: Vec<i64> = earlier().filter_map(|k| written_at(&k.id)).collect();
    let pending = earlier().filter(|k| k.pending).count();
    rate_refusal(at, &times, pending)
}

/// The proposals the Orchestrator view lists (AD-92, D-155), of `rows`
/// newest first: every pending one, the [`PROPOSALS_SHOWN_INVALID`] latest
/// invalid ones and the [`PROPOSALS_SHOWN_DECIDED`] latest decided ones.
pub fn shown(rows: &[ProposalView]) -> Vec<ProposalView> {
    let mut invalid = 0;
    let mut decided = 0;
    rows.iter()
        .filter(|row| match row.status {
            ProposalStatus::Pending => true,
            ProposalStatus::Invalid => {
                invalid += 1;
                invalid <= PROPOSALS_SHOWN_INVALID
            }
            ProposalStatus::Accepted
            | ProposalStatus::Dismissed
            | ProposalStatus::Expired
            | ProposalStatus::Superseded => {
                decided += 1;
                decided <= PROPOSALS_SHOWN_DECIDED
            }
        })
        .cloned()
        .collect()
}

/// The latest pending recap whose target is `session_id` or its subject
/// `subject_id` (AD-84's recap section, FR-5): what `meeting::build` puts
/// in the meeting, which the panel shows under its label. `rows` run
/// newest first.
pub fn latest_recap<'a>(
    rows: &'a [ProposalView],
    session_id: &str,
    subject_id: Option<i64>,
) -> Option<&'a ProposalView> {
    let session = Untrusted::new(session_id.to_owned());
    rows.iter()
        .filter(|row| row.status == ProposalStatus::Pending)
        .find(|row| match &row.content {
            Some(ProposalContent::Recap { target, .. }) => match target {
                TargetView::SessionId(id) => *id == session,
                TargetView::SubjectId(id) => Some(*id) == subject_id,
            },
            _ => false,
        })
}

/// The latest pending answer proposal for `session_id` (AD-182): what
/// `meeting::build` puts beside the composer. `rows` run newest first; a
/// newer valid one supersedes the older at ingestion (D-575), so one shows.
pub fn latest_answer<'a>(rows: &'a [ProposalView], session_id: &str) -> Option<&'a ProposalView> {
    let session = Untrusted::new(session_id.to_owned());
    rows.iter()
        .filter(|row| row.status == ProposalStatus::Pending)
        .find(|row| {
            matches!(&row.content, Some(ProposalContent::Answer { session_id, .. })
                if *session_id == session)
        })
}

/// The pending answer proposals that expire now (AD-182): of `pending`,
/// each `(id, session id)`, those whose session is not in `answerable`, the
/// sessions that take a proposed answer on the latest board. A session
/// that left Your turn, started waiting, ended or left the board, or whose
/// delivery is no longer `ResumeLive`, takes none.
///
/// ```
/// use std::collections::BTreeSet;
/// use officina_core::orders::proposal::expired_answers;
/// let pending = vec![("p1".to_owned(), "s-1".to_owned()), ("p2".to_owned(), "s-2".to_owned())];
/// let answerable = BTreeSet::from(["s-2".to_owned()]);
/// assert_eq!(expired_answers(&pending, &answerable), ["p1"]);
/// ```
pub fn expired_answers(pending: &[(String, String)], answerable: &BTreeSet<String>) -> Vec<String> {
    pending
        .iter()
        .filter(|(_, session)| !answerable.contains(session))
        .map(|(id, _)| id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const NOW: i64 = 1_761_700_000_000;

    fn recap() -> RecapProposal {
        RecapProposal {
            target: ProposalTarget::SubjectId(4),
            done: "done".into(),
            blocking: "nothing".into(),
            decision_expected: "merge?".into(),
        }
    }

    #[test]
    fn a_file_is_one_flat_object_with_its_kind() {
        let file = ProposalFile::recap(recap(), Some("s-1".into()), NOW);
        let value: serde_json::Value = serde_json::from_slice(&file.to_bytes()).unwrap();
        assert_eq!(
            value,
            json!({"kind": "recap", "schema": PROPOSAL_FILE_SCHEMA, "target": {"subject_id": 4},
                "done": "done", "blocking": "nothing", "decision_expected": "merge?",
                "orchestrator_session_id": "s-1", "written_at_ms": NOW})
        );
        let back: ProposalFile = serde_json::from_value(value).unwrap();
        assert_eq!(back, file);
    }

    #[test]
    fn unknown_fields_and_kinds_do_not_parse() {
        let mut value = serde_json::to_value(ProposalFile::recap(recap(), None, NOW)).unwrap();
        value["launch"] = json!("now");
        assert!(serde_json::from_value::<ProposalFile>(value).is_err());
        let order = ProposalFile::work_order(
            WorkOrderProposal {
                template: "bump".into(),
                project: "api".into(),
                values: BTreeMap::new(),
                subject: ProposedSubject::New("deps".into()),
                rationale: "why".into(),
            },
            None,
            NOW,
        );
        let mut value = serde_json::to_value(&order).unwrap();
        assert_eq!(value["subject"], json!({"new": "deps"}));
        value["kind"] = json!("answer");
        assert!(serde_json::from_value::<ProposalFile>(value).is_err());
    }

    #[test]
    fn ids_and_file_names_round_trip() {
        let id = proposal_id(NOW, u64::MAX);
        assert_eq!(written_at(&id), Some(NOW));
        assert_eq!(id_of_file(&file_name(&id)), Some(id.as_str()));
        assert_eq!(id_of_file(&id), None);
        assert_eq!(written_at(&format!("{id}0")), None);
        assert_eq!(written_at(&format!("+{}", &id[1..])), None);
    }

    #[test]
    fn a_pending_view_holds_every_text_untrusted() {
        let view = ProposalFile::recap(recap(), Some("s-1".into()), NOW)
            .pending_view(&proposal_id(NOW, 1));
        let marked = crate::untrusted::wrapped(|| serde_json::to_value(&view)).unwrap();
        assert_eq!(marked["status"], "pending");
        assert_eq!(marked["received_at_ms"], serde_json::Value::Null);
        assert_eq!(
            marked["content"]["recap"]["done"],
            json!({"untrusted": "done"})
        );
        assert_eq!(
            marked["orchestrator_session_id"],
            json!({"untrusted": "s-1"})
        );
    }

    #[test]
    fn the_hourly_limit_counts_the_last_hour_only() {
        let old = vec![NOW - PROPOSAL_HOUR_MS; PROPOSALS_PER_HOUR];
        assert_eq!(rate_refusal(NOW, &old, 0), None);
        assert_eq!(
            rate_refusal(NOW, &[], PROPOSALS_PENDING_MAX),
            Some(RateRefusal::PendingLimit)
        );
        assert_eq!(rate_refusal(NOW, &[], PROPOSALS_PENDING_MAX - 1), None);
        let recent = vec![NOW; PROPOSALS_PER_HOUR];
        assert_eq!(
            rate_refusal(NOW, &recent, PROPOSALS_PENDING_MAX),
            Some(RateRefusal::HourlyLimit),
            "the hourly limit first"
        );
        let ahead = vec![NOW + 1; PROPOSALS_PER_HOUR];
        assert_eq!(
            rate_refusal(NOW, &ahead, 0),
            None,
            "D-964: ids dated ahead of the clock are not this hour's"
        );
    }

    fn view(id: &str, status: ProposalStatus) -> ProposalView {
        ProposalView {
            status,
            ..ProposalView::unreadable_file(id)
        }
    }

    #[test]
    fn listed_keeps_seven_days_newest_first_at_most_fifty() {
        let mut all: Vec<ProposalView> = (0..PROPOSALS_LISTED as i64 + 5)
            .map(|i| view(&proposal_id(NOW - i, 1), ProposalStatus::Pending))
            .collect();
        all.push(view(
            &proposal_id(NOW - PROPOSALS_LISTED_FOR_MS, 2),
            ProposalStatus::Pending,
        ));
        all.reverse();
        let shown = listed(all.clone(), None, NOW);
        assert_eq!(shown.len(), PROPOSALS_LISTED);
        assert_eq!(shown[0].written_at_ms, NOW);
        assert!(
            shown
                .windows(2)
                .all(|w| w[0].written_at_ms >= w[1].written_at_ms)
        );
        assert!(listed(all, Some(ProposalStatus::Invalid), NOW).is_empty());
    }

    #[test]
    fn a_row_wins_over_its_file() {
        let id = proposal_id(NOW, 3);
        let merged = merged(
            vec![view(&id, ProposalStatus::Invalid)],
            vec![
                view(&id, ProposalStatus::Pending),
                view(&proposal_id(NOW, 4), ProposalStatus::Pending),
            ],
        );
        assert_eq!(merged.len(), 2);
        assert_eq!(
            merged.iter().find(|p| p.id == id).unwrap().status,
            ProposalStatus::Invalid
        );
    }

    use crate::registry::types::{Source, Sourced};
    use crate::relay::template::sample;

    fn subject(id: i64, project: &str, status: SubjectStatus) -> SubjectRecord {
        SubjectRecord {
            id,
            project: project.to_owned(),
            name: format!("subject {id}"),
            status: Sourced {
                value: status,
                src: Source::Author,
                at_ms: NOW,
            },
            created_at_ms: NOW,
            routine: false,
            merged_into: None,
        }
    }

    fn order(values: &[(&str, &str)], subject: ProposedSubject) -> ProposalFile {
        ProposalFile::work_order(
            WorkOrderProposal {
                template: sample::TEMPLATE.into(),
                project: "backend".into(),
                values: values
                    .iter()
                    .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
                    .collect(),
                subject,
                rationale: "Tests fail on the old release.".into(),
            },
            Some("s-1".into()),
            NOW,
        )
    }

    const VALID: &[(&str, &str)] = &[("package", "serde_json"), ("level", "minor")];

    /// Records `file` at `now` against the sample catalogue, subjects 1
    /// and 4 (open, `backend`), 2 (archived) and 3 (`lab`), and session
    /// `s-9`.
    fn record_at(file: &ProposalFile, now: i64) -> ProposalRecord {
        let approved = sample::approved();
        let subjects = [
            subject(1, "backend", SubjectStatus::Active),
            subject(2, "backend", SubjectStatus::Archived),
            subject(3, "lab", SubjectStatus::Active),
            subject(4, "backend", SubjectStatus::ToResume),
        ];
        let sessions = BTreeSet::from(["s-9".to_owned()]);
        let answerable = BTreeSet::from([ANSWERED.to_owned()]);
        let refs = RegistryRefs {
            subjects: &subjects,
            sessions: &sessions,
            answerable: &answerable,
        };
        record_of(
            &proposal_id(now, 1),
            FileRead::Bytes(&file.to_bytes()),
            Some(&approved.canonical),
            &refs,
            now,
        )
    }

    /// AD-91, D-145: a valid work order proposal is pending until 24 h
    /// after its receipt, then expired.
    #[test]
    fn a_work_order_proposal_expires_after_24_h() {
        let row = record_at(&order(VALID, ProposedSubject::Id(1)), NOW);
        assert_eq!(row.status, ProposalStatus::Pending);
        assert_eq!(row.expires_at_ms, Some(NOW + WORK_ORDER_PROPOSAL_TTL_MS));
        assert!(!is_expired(
            row.expires_at_ms,
            NOW + WORK_ORDER_PROPOSAL_TTL_MS - 1
        ));
        assert!(is_expired(
            row.expires_at_ms,
            NOW + WORK_ORDER_PROPOSAL_TTL_MS
        ));
    }

    /// D-145: recaps never expire, however old.
    #[test]
    fn recaps_do_not_expire() {
        let row = record_at(&ProposalFile::recap(recap(), None, NOW), NOW);
        assert_eq!(row.kind, Some(ProposalKind::Recap));
        assert_eq!(row.expires_at_ms, None);
        assert!(!is_expired(row.expires_at_ms, i64::MAX));
        assert_eq!(expires_at_ms(ProposalKind::Recap, NOW), None);
    }

    /// D-145: a newer valid recap on the same target supersedes the
    /// pending one, whatever order the files came in; another target's
    /// stays.
    #[test]
    fn a_newer_recap_supersedes_the_pending_one() {
        let older = proposal_id(NOW, 1);
        let newer = proposal_id(NOW + 1, 1);
        let other = proposal_id(NOW - 5, 2);
        let subject = ProposalTarget::SubjectId(4);
        let session = ProposalTarget::SessionId("s-9".into());
        for pending in [
            vec![
                (older.clone(), subject.clone()),
                (newer.clone(), subject.clone()),
            ],
            vec![
                (newer.clone(), subject.clone()),
                (older.clone(), subject.clone()),
            ],
        ] {
            let mut all = pending;
            all.push((other.clone(), session.clone()));
            assert_eq!(superseded_pending(&all), std::slice::from_ref(&older));
        }
        // A recap only supersedes once valid: an invalid one names none.
        let mut invalid = recap();
        invalid.target = ProposalTarget::SessionId("s-unknown".into());
        let row = record_at(&ProposalFile::recap(invalid, None, NOW), NOW);
        assert_eq!(row.superseding(), None);
        let valid = record_at(&ProposalFile::recap(recap(), None, NOW), NOW);
        assert_eq!(
            valid.superseding(),
            Some((ProposalKind::Recap, &ProposalTarget::SubjectId(4)))
        );
    }

    /// AD-91: the stored names and values are the template's strings, the
    /// texts as written; an invalid proposal keeps its texts and reason.
    #[test]
    fn a_valid_order_stores_the_templates_strings() {
        let row = record_at(&order(VALID, ProposedSubject::New("deps".into())), NOW);
        assert_eq!(row.status, ProposalStatus::Pending);
        assert_eq!(row.template.as_deref(), Some(sample::TEMPLATE));
        assert_eq!(row.values.as_ref().unwrap()["package"], "serde_json");
        assert_eq!(row.subject, Some(ProposedSubject::New("deps".into())));
        assert_eq!(row.orchestrator_session_id.as_deref(), Some("s-1"));
        let refused = record_at(
            &order(
                &[("package", "serde_json --settings x"), ("level", "minor")],
                ProposedSubject::Id(1),
            ),
            NOW,
        );
        assert_eq!(refused.status, ProposalStatus::Invalid);
        assert_eq!(
            refused.invalid_reason,
            Some(ProposalInvalidReason::ValueNotInList)
        );
        assert_eq!(
            refused.texts.as_ref().unwrap()["rationale"],
            "Tests fail on the old release."
        );
        assert_eq!(refused.expires_at_ms, None);
    }

    /// AD-91: each check's refusal, in `validate`'s order.
    #[test]
    fn each_check_has_its_reason() {
        let reason = |file: &ProposalFile| record_at(file, NOW).invalid_reason;
        let with_recap = |edit: fn(&mut RecapProposal)| {
            let mut proposal = recap();
            edit(&mut proposal);
            ProposalFile::recap(proposal, None, NOW)
        };
        assert_eq!(
            reason(&with_recap(
                |r| r.done = "a".repeat(PROPOSAL_TEXT_MAX_CHARS + 1)
            )),
            Some(ProposalInvalidReason::TextTooLong)
        );
        assert_eq!(
            reason(&with_recap(|r| r.blocking = "a\u{202e}b".into())),
            Some(ProposalInvalidReason::ForbiddenCharacters)
        );
        assert_eq!(
            reason(&with_recap(|r| r.done = "line one\nline two\ttab".into())),
            None
        );
        assert_eq!(
            reason(&with_recap(|r| r.target = ProposalTarget::SubjectId(9))),
            Some(ProposalInvalidReason::UnknownSubject)
        );
        assert_eq!(
            reason(&with_recap(
                |r| r.target = ProposalTarget::SessionId("s-9".into())
            )),
            None
        );
        for (file, expected) in [
            (
                order(VALID, ProposedSubject::Id(2)),
                ProposalInvalidReason::SubjectClosed,
            ),
            (
                order(VALID, ProposedSubject::Id(3)),
                ProposalInvalidReason::SubjectOtherProject,
            ),
            (
                order(VALID, ProposedSubject::Id(9)),
                ProposalInvalidReason::UnknownSubject,
            ),
            (
                order(VALID, ProposedSubject::New("  ".into())),
                ProposalInvalidReason::SubjectNameInvalid,
            ),
            (
                order(&[("package", "serde_json")], ProposedSubject::Id(1)),
                ProposalInvalidReason::MissingParameter,
            ),
            (
                order(
                    &[("package", "serde_json"), ("level", "minor"), ("x", "y")],
                    ProposedSubject::Id(1),
                ),
                ProposalInvalidReason::UnknownParameter,
            ),
        ] {
            assert_eq!(reason(&file), Some(expected), "{expected:?}");
        }
        // A taken name files into the subject that holds it (D-158).
        assert_eq!(
            reason(&order(VALID, ProposedSubject::New("Subject 1".into()))),
            None
        );
        let ProposalFile::WorkOrder(mut file) = order(VALID, ProposedSubject::Id(1)) else {
            unreachable!()
        };
        file.project = "lab".into();
        assert_eq!(
            reason(&ProposalFile::WorkOrder(file.clone())),
            Some(ProposalInvalidReason::ProjectNotAllowed)
        );
        file.template = "release-notes".into();
        assert_eq!(
            reason(&ProposalFile::WorkOrder(file)),
            Some(ProposalInvalidReason::UnknownTemplate)
        );
        // No approved snapshot, no template.
        let refs = RegistryRefs {
            subjects: &[],
            sessions: &BTreeSet::new(),
            answerable: &BTreeSet::new(),
        };
        let row = record_of(
            "0000000000001-0000000000000001",
            FileRead::Bytes(&order(VALID, ProposedSubject::Id(1)).to_bytes()),
            None,
            &refs,
            NOW,
        );
        assert_eq!(
            row.invalid_reason,
            Some(ProposalInvalidReason::UnknownTemplate)
        );
    }

    /// D-133, D-155: a file that holds no proposal is invalid with no kind
    /// and no texts: oversized, not JSON, another schema, an unknown field
    /// or an answer holding a recap's fields.
    #[test]
    fn unread_files_have_no_kind() {
        let refs = RegistryRefs {
            subjects: &[],
            sessions: &BTreeSet::new(),
            answerable: &BTreeSet::new(),
        };
        let unread =
            |read: FileRead| record_of("0000000000001-0000000000000001", read, None, &refs, NOW);
        let oversized = unread(FileRead::Oversized);
        assert_eq!(
            (oversized.kind, oversized.invalid_reason, oversized.texts),
            (None, Some(ProposalInvalidReason::Oversized), None)
        );
        let mut schema = serde_json::to_value(ProposalFile::recap(recap(), None, NOW)).unwrap();
        schema["schema"] = json!(PROPOSAL_FILE_SCHEMA + 1);
        let mut answer = serde_json::to_value(ProposalFile::recap(recap(), None, NOW)).unwrap();
        answer["kind"] = json!("answer");
        for bytes in [
            b"not json".to_vec(),
            serde_json::to_vec(&schema).unwrap(),
            serde_json::to_vec(&answer).unwrap(),
            br#"{"kind":"recap","launch":"now"}"#.to_vec(),
        ] {
            let row = unread(FileRead::Bytes(&bytes));
            assert_eq!(
                (row.kind, row.status, row.invalid_reason, row.texts),
                (
                    None,
                    ProposalStatus::Invalid,
                    Some(ProposalInvalidReason::Unparsable),
                    None
                ),
                "{}",
                String::from_utf8_lossy(&bytes)
            );
        }
    }

    /// AD-90, D-155: at ingestion the limits count the proposals known
    /// before each file, as the tool counted them when it wrote it.
    #[test]
    fn ingestion_holds_the_limits_on_earlier_proposals() {
        let known: Vec<KnownProposal> = (0..PROPOSALS_PER_HOUR as i64)
            .map(|at| KnownProposal {
                id: proposal_id(NOW + at, 1),
                pending: false,
            })
            .collect();
        let next = proposal_id(NOW + PROPOSALS_PER_HOUR as i64, 1);
        assert_eq!(
            ingestion_refusal(&next, &known),
            Some(RateRefusal::HourlyLimit)
        );
        // The last one written within the limit was counted without itself.
        assert_eq!(
            ingestion_refusal(&known[PROPOSALS_PER_HOUR - 1].id, &known),
            None
        );
        let hour_later = proposal_id(NOW + PROPOSAL_HOUR_MS + 1, 1);
        assert_eq!(ingestion_refusal(&hour_later, &known), None);
        let pending: Vec<KnownProposal> = (0..PROPOSALS_PENDING_MAX as i64)
            .map(|at| KnownProposal {
                id: proposal_id(NOW - 3 * PROPOSAL_HOUR_MS - at, 1),
                pending: true,
            })
            .collect();
        assert_eq!(
            ingestion_refusal(&proposal_id(NOW, 2), &pending),
            Some(RateRefusal::PendingLimit)
        );
    }

    /// AD-92, D-155: every pending proposal, the 50 latest invalid and the
    /// 50 latest decided, newest first.
    #[test]
    fn shown_keeps_pending_and_the_latest_invalid_and_decided() {
        let mut rows = Vec::new();
        for (offset, status) in [
            (0, ProposalStatus::Pending),
            (1_000, ProposalStatus::Invalid),
            (2_000, ProposalStatus::Dismissed),
        ] {
            for at in 0..60 {
                rows.push(view(&proposal_id(NOW - offset - at, 1), status));
            }
        }
        rows.sort_by(|a, b| b.id.cmp(&a.id));
        let listed = shown(&rows);
        let count = |status| listed.iter().filter(|p| p.status == status).count();
        assert_eq!(count(ProposalStatus::Pending), 60);
        assert_eq!(count(ProposalStatus::Invalid), PROPOSALS_SHOWN_INVALID);
        assert_eq!(count(ProposalStatus::Dismissed), PROPOSALS_SHOWN_DECIDED);
        assert!(listed.windows(2).all(|w| w[0].id > w[1].id));
    }

    /// AD-84 (FR-5): the latest pending recap of a session or its subject.
    #[test]
    fn the_latest_pending_recap_of_a_session_or_its_subject() {
        let of = |id: &str, target: ProposalTarget, status| {
            let mut proposal = recap();
            proposal.target = target;
            ProposalView {
                status,
                ..ProposalFile::recap(proposal, None, NOW).pending_view(id)
            }
        };
        let rows = vec![
            of(
                "0000000000004-0000000000000001",
                ProposalTarget::SessionId("s-1".into()),
                ProposalStatus::Superseded,
            ),
            of(
                "0000000000003-0000000000000001",
                ProposalTarget::SubjectId(4),
                ProposalStatus::Pending,
            ),
            of(
                "0000000000002-0000000000000001",
                ProposalTarget::SessionId("s-1".into()),
                ProposalStatus::Pending,
            ),
        ];
        assert_eq!(
            latest_recap(&rows, "s-1", Some(4)).map(|r| r.id.as_str()),
            Some("0000000000003-0000000000000001")
        );
        assert_eq!(
            latest_recap(&rows, "s-1", None).map(|r| r.id.as_str()),
            Some("0000000000002-0000000000000001")
        );
        assert!(latest_recap(&rows, "s-2", Some(5)).is_none());
    }

    /// The session [`record_at`] holds as taking a proposed answer.
    const ANSWERED: &str = "0f1e2d3c-4b5a-4978-8796-a5b4c3d2e1f0";

    fn answer(text: &str, rationale: &str, session: &str, at: i64) -> ProposalFile {
        ProposalFile::answer(
            AnswerProposal {
                session_id: session.to_owned(),
                text: text.to_owned(),
                rationale: rationale.to_owned(),
            },
            Some("s-1".into()),
            at,
        )
    }

    /// A text the composer refuses with `refusal`, built from its rule.
    fn breaking(refusal: AnswerRefusal) -> String {
        match refusal {
            AnswerRefusal::Empty => " \n\t".to_owned(),
            AnswerRefusal::TooLong => "a".repeat(PROPOSAL_ANSWER_MAX_CHARS + 1),
            AnswerRefusal::ControlCharacter => "keep\u{7}it".to_owned(),
            AnswerRefusal::FormatCharacter => "keep\u{202e}it".to_owned(),
            AnswerRefusal::LeadingSlash => "  /clear".to_owned(),
            AnswerRefusal::LeadingBang => "\n!rm -rf .".to_owned(),
        }
    }

    use crate::meeting::AnswerRefusal;

    /// Story 5.21 AC 2 (AD-182, D-168): an answer's text meets the
    /// composer's own function: every refusal of [`Answer::parse`] reached
    /// through a proposal stores it `answer_refused`, its texts kept as
    /// written; the rationale holds the texts' bounds, 1 character at
    /// least; a valid text for a session that takes no answer now is
    /// `session_not_answerable`; a valid one is pending, with no expiry.
    #[test]
    fn an_answer_proposal_uses_the_composers_rules() {
        for refusal in AnswerRefusal::ALL {
            let text = breaking(*refusal);
            assert_eq!(Answer::parse(&text).unwrap_err(), *refusal);
            let row = record_at(&answer(&text, "Why.", ANSWERED, NOW), NOW);
            assert_eq!(
                (row.kind, row.status, row.invalid_reason),
                (
                    Some(ProposalKind::Answer),
                    ProposalStatus::Invalid,
                    Some(ProposalInvalidReason::AnswerRefused)
                ),
                "{refusal:?}"
            );
            assert_eq!(row.texts.as_ref().unwrap()["text"], text);
        }
        let reason = |file: &ProposalFile| record_at(file, NOW).invalid_reason;
        let text = "Keep the 1.x line, then -x run the tests.\nThanks";
        for (rationale, expected) in [
            (String::new(), ProposalInvalidReason::RationaleMissing),
            (
                "a".repeat(PROPOSAL_TEXT_MAX_CHARS + 1),
                ProposalInvalidReason::TextTooLong,
            ),
            (
                "why\u{2066}".to_owned(),
                ProposalInvalidReason::ForbiddenCharacters,
            ),
        ] {
            assert_eq!(
                reason(&answer(text, &rationale, ANSWERED, NOW)),
                Some(expected)
            );
        }
        assert_eq!(
            reason(&answer(text, "Why.", "s-9", NOW)),
            Some(ProposalInvalidReason::SessionNotAnswerable),
            "a session the registry holds but no board shows idle"
        );
        let row = record_at(
            &answer(text, &"é".repeat(PROPOSAL_TEXT_MAX_CHARS), ANSWERED, NOW),
            NOW,
        );
        assert_eq!(row.status, ProposalStatus::Pending);
        assert_eq!(row.expires_at_ms, None);
        assert_eq!(row.target, Some(ProposalTarget::SessionId(ANSWERED.into())));
        assert_eq!(row.texts.as_ref().unwrap()["text"], text);
        // A forged field, `source` among them, makes the file unreadable.
        let mut forged = serde_json::to_value(answer(text, "Why.", ANSWERED, NOW)).unwrap();
        forged["source"] = json!("author");
        let refs = RegistryRefs {
            subjects: &[],
            sessions: &BTreeSet::new(),
            answerable: &BTreeSet::from([ANSWERED.to_owned()]),
        };
        let row = record_of(
            &proposal_id(NOW, 1),
            FileRead::Bytes(&serde_json::to_vec(&forged).unwrap()),
            None,
            &refs,
            NOW,
        );
        assert_eq!(row.invalid_reason, Some(ProposalInvalidReason::Unparsable));
    }

    /// The board of one background session listed with `status`, `state`,
    /// `cwd` (when given) and a pid, the session [`ANSWERED`].
    fn board_of(
        status: &str,
        state: &str,
        cwd: Option<&str>,
    ) -> (crate::board::Board, Vec<crate::agents::AgentRecord>) {
        let mut entry = json!({"id": "bg01", "sessionId": ANSWERED, "kind": "background",
            "status": status, "state": state, "pid": 7, "startedAt": NOW - 1_000});
        if let Some(cwd) = cwd {
            entry["cwd"] = json!(cwd);
        }
        let agents = crate::agents::parse_agents(&json!([entry]).to_string()).unwrap();
        let input = crate::board::BoardInput {
            agents: Some(agents.clone()),
            ..Default::default()
        };
        (crate::board::build_board(&input, NOW), agents)
    }

    const NEW_CLAUDE: Option<&str> = Some("2.1.312 (Claude Code)");

    /// Story 5.21 AC 3 (AD-182): a pending answer expires on the first
    /// board that shows its session out of its turn: working, waiting on a
    /// question, ended, or gone from the board.
    #[test]
    fn an_answer_expires_when_the_session_leaves_your_turn() {
        let pending = vec![(proposal_id(NOW, 1), ANSWERED.to_owned())];
        let expired = |status: &str, state: &str| {
            let (board, agents) = board_of(status, state, Some("C:/path/to/repo"));
            expired_answers(
                &pending,
                &crate::meeting::answerable_sessions(&board, &agents, NEW_CLAUDE),
            )
        };
        assert!(expired("idle", "done").is_empty(), "idle in its turn");
        for (status, state) in [
            ("busy", "working"),
            ("waiting", "blocked"),
            ("idle", "stopped"),
            ("idle", "failed"),
        ] {
            assert_eq!(
                expired(status, state),
                [pending[0].0.clone()],
                "{status} {state}"
            );
        }
        let empty = crate::board::build_board(&crate::board::BoardInput::default(), NOW);
        assert_eq!(
            expired_answers(
                &pending,
                &crate::meeting::answerable_sessions(&empty, &[], NEW_CLAUDE)
            ),
            [pending[0].0.clone()],
            "gone from the board"
        );
    }

    /// Story 5.21 AC 3 (AD-182, D-139, D-168): idle in its turn, a pending
    /// answer still expires once its delivery stops being `ResumeLive`: a
    /// `claude` too old or unread, or no folder `claude agents` reported.
    #[test]
    fn an_answer_expires_when_its_delivery_changes() {
        let pending = vec![(proposal_id(NOW, 1), ANSWERED.to_owned())];
        let answerable = |cwd: Option<&str>, version: Option<&str>| {
            let (board, agents) = board_of("idle", "done", cwd);
            crate::meeting::answerable_sessions(&board, &agents, version)
        };
        let folder = Some("C:/path/to/repo");
        assert!(expired_answers(&pending, &answerable(folder, NEW_CLAUDE)).is_empty());
        for (cwd, version) in [
            (folder, Some("2.1.284 (Claude Code)")),
            (folder, None),
            (None, NEW_CLAUDE),
        ] {
            assert_eq!(
                expired_answers(&pending, &answerable(cwd, version)),
                [pending[0].0.clone()],
                "{cwd:?} {version:?}"
            );
        }
    }

    /// Story 5.21 AC 3 (D-575): a newer valid answer for the same session
    /// supersedes the pending one, whatever order the files came in; an
    /// invalid one supersedes none; answers and recaps on one session never
    /// supersede each other, each kind compared apart.
    #[test]
    fn a_newer_answer_supersedes_the_pending_one() {
        let older = record_at(&answer("First.", "Why.", ANSWERED, NOW), NOW);
        assert_eq!(
            older.superseding(),
            Some((
                ProposalKind::Answer,
                &ProposalTarget::SessionId(ANSWERED.into())
            ))
        );
        let invalid = record_at(&answer("/clear", "Why.", ANSWERED, NOW), NOW);
        assert_eq!(invalid.superseding(), None);
        let target = ProposalTarget::SessionId(ANSWERED.into());
        let first = proposal_id(NOW, 1);
        let second = proposal_id(NOW + 1, 1);
        for pending in [
            vec![
                (first.clone(), target.clone()),
                (second.clone(), target.clone()),
            ],
            vec![
                (second.clone(), target.clone()),
                (first.clone(), target.clone()),
            ],
        ] {
            assert_eq!(superseded_pending(&pending), std::slice::from_ref(&first));
        }
        let recap_on_session = record_at(
            &ProposalFile::recap(
                RecapProposal {
                    target: ProposalTarget::SessionId("s-9".into()),
                    ..recap()
                },
                None,
                NOW,
            ),
            NOW,
        );
        let (recap_kind, _) = recap_on_session.superseding().unwrap();
        assert_ne!(recap_kind, ProposalKind::Answer);
    }
}
