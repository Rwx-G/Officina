//! Orchestrator proposals (AD-90, AD-91, FR-11, FR-12): what a proposal
//! tool of `officina mcp` asks for, the file it writes under
//! `<home>/orchestrator/proposals/`, its name, its bounds and the rate
//! limits, and the view the window and the `proposals` tool carry
//! ([`ProposalView`], D-576).
//!
//! A file is a parse record (D-130): its strings stay plain until the
//! window validates them at ingestion (story 5.17), which a forged file
//! meets the same way. Every string a view carries from a file or a row is
//! [`Untrusted`]: the orchestrator wrote it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::registry::confirm::NAME_MAX_CHARS;
use crate::registry::types::{ProposalKind, ProposalStatus};
use crate::relay::params::TEMPLATE_VALUE_MAX_CHARS;
use crate::untrusted::Untrusted;

/// The proposal file's format version (AD-91).
pub const PROPOSAL_FILE_SCHEMA: u32 = 1;

/// The most bytes a proposal file holds (AD-91): the writer refuses a
/// larger one, ingestion reads one byte more and stores it `invalid`.
pub const PROPOSAL_FILE_MAX_BYTES: usize = 16 * 1024;

/// The most characters of a proposal's text: each recap field and a work
/// order's rationale (AD-90).
pub const PROPOSAL_TEXT_MAX_CHARS: usize = 500;

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

/// What a recap is about: a session or a subject (AD-90).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// A proposal file as written and parsed (AD-91), its kind the tag:
/// one flat object, unknown fields refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProposalFile {
    Recap(RecapFile),
    WorkOrder(WorkOrderFile),
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

    /// Its kind.
    pub fn kind(&self) -> ProposalKind {
        match self {
            ProposalFile::Recap(_) => ProposalKind::Recap,
            ProposalFile::WorkOrder(_) => ProposalKind::WorkOrder,
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
/// many wait. The hourly limit is checked first.
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
        .filter(|at| **at > now_ms - PROPOSAL_HOUR_MS)
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
}
