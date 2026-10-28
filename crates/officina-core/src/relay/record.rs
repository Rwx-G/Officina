//! The window's record of its own decisions (D-173, D-175): which requests
//! it decided, how and for which request key, which catalogue it approved,
//! whether it holds the relay stopped, and which anomalies the author
//! acknowledged. A decision file, a run or a stop flag's absence the record
//! cannot account for is an anomaly: the Relay view lists it and doctor
//! fails until the author acknowledges it. `officina run` obeys the record:
//! it runs only the catalogue the record names, holds the relay stopped
//! while the record says so, uses only an approval the record holds as
//! approved for the same request key, and refuses everything while the
//! record is missing or unreadable once the relay holds a decision.
//!
//! Every file of the relay is writable by any process running as the
//! author, the record included: it surfaces the cheap forgeries, the file
//! tools and plain writes, and a record deleted or broken makes everything
//! refuse until the author acknowledges it; a session that rewrites the
//! record consistently is not stopped (AD-25). Pure: the host reads and
//! writes `relay/decisions.json`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::approvals::ApprovalFiles;
use super::journal::{JournalEntry, JournalLine};

/// Version of `relay/decisions.json`.
pub const DECISION_RECORD_SCHEMA: u32 = 2;
/// The file under the relay directory.
pub const DECISION_RECORD_FILE: &str = "decisions.json";
/// How long the record keeps a decided id or an acknowledgement: past the
/// approvals' retention and the two days of journal the view reads.
pub const RECORD_RETENTION_MS: i64 = 7 * 24 * 60 * 60 * 1000;

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What the window decided on a request.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum Verdict {
        Approved,
        Rejected,
    }
}

/// One decision the window made on a request (D-175).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedDecision {
    pub verdict: Verdict,
    /// The key of the request it was given for: the run that uses an
    /// approval must still have it.
    pub request_key: String,
    pub at_ms: i64,
}

/// Content of `relay/decisions.json`, written by the window only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionRecord {
    pub schema: u32,
    /// When the window started this record: nothing older is judged.
    pub since_ms: i64,
    /// The requests the window approved or rejected, by id.
    #[serde(default)]
    pub decided: BTreeMap<String, RecordedDecision>,
    /// The hash of the catalogue the window last approved.
    #[serde(default)]
    pub approved_hash: Option<String>,
    /// When the window stopped the relay, while it holds it stopped.
    #[serde(default)]
    pub stopped_since_ms: Option<i64>,
    /// The anomalies the author acknowledged, by key, with when.
    #[serde(default)]
    pub acknowledged: BTreeMap<String, i64>,
}

impl DecisionRecord {
    /// A record started at `now_ms`, taking the state the window finds as
    /// its own: the approved catalogue and the stop flag as they are.
    pub fn start(
        now_ms: i64,
        approved_hash: Option<String>,
        stopped_since_ms: Option<i64>,
    ) -> Self {
        DecisionRecord {
            schema: DECISION_RECORD_SCHEMA,
            since_ms: now_ms,
            decided: BTreeMap::new(),
            approved_hash,
            stopped_since_ms,
            acknowledged: BTreeMap::new(),
        }
    }

    /// Drops the decided ids and acknowledgements older than
    /// [`RECORD_RETENTION_MS`].
    pub fn prune(&mut self, now_ms: i64) {
        let fresh = |at: i64| now_ms - at <= RECORD_RETENTION_MS;
        self.decided.retain(|_, decision| fresh(decision.at_ms));
        self.acknowledged.retain(|_, at| fresh(*at));
    }

    /// True while the window holds the relay stopped.
    pub fn holds_stopped(&self) -> bool {
        self.stopped_since_ms.is_some()
    }
}

/// Parses `relay/decisions.json`, refusing another schema; the error says
/// why.
pub fn parse_record(content: &[u8]) -> Result<DecisionRecord, String> {
    let record =
        serde_json::from_slice::<DecisionRecord>(content).map_err(|e| format!("corrupt: {e}"))?;
    if record.schema == DECISION_RECORD_SCHEMA {
        Ok(record)
    } else {
        Err(format!(
            "schema {}, not {DECISION_RECORD_SCHEMA}",
            record.schema
        ))
    }
}

/// `relay/decisions.json` as the host found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordState {
    Missing,
    /// It exists and cannot be read, parsed, or is of another schema.
    Unreadable(String),
    Valid(DecisionRecord),
}

impl RecordState {
    /// The record, when it is valid.
    pub fn valid(&self) -> Option<&DecisionRecord> {
        match self {
            RecordState::Valid(record) => Some(record),
            RecordState::Missing | RecordState::Unreadable(_) => None,
        }
    }

    /// Why the record cannot vouch for anything, once the relay holds
    /// something it would vouch for (`holds_decisions`: an approved
    /// catalogue or a decision file): missing or unreadable (D-175).
    /// `None` for a valid record, and for a missing one before the relay
    /// holds anything, which the window starts the first time it opens.
    pub fn broken(&self, holds_decisions: bool) -> Option<String> {
        match self {
            RecordState::Valid(_) => None,
            RecordState::Missing if !holds_decisions => None,
            RecordState::Missing => Some("it is missing".to_owned()),
            RecordState::Unreadable(why) => Some(format!("it cannot be read ({why})")),
        }
    }
}

/// True when the relay holds what the record vouches for: an approved
/// catalogue (`approved_exists`, readable or not) or a decision file.
pub fn holds_decisions(approved_exists: bool, files: &ApprovalFiles) -> bool {
    approved_exists
        || !files.approved.is_empty()
        || !files.rejected.is_empty()
        || !files.consumed.is_empty()
}

/// The key of the anomaly a broken record is (D-175): acknowledging it is
/// what lets the window start a new record from the state it finds.
pub fn record_anomaly_key() -> String {
    AnomalyKind::RecordBroken.key(DECISION_RECORD_FILE)
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// A decision the window did not make (D-173).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum AnomalyKind {
        /// An approval file, used or not, for a request the window did not
        /// decide, rejected, or approved for another request key.
        ForeignApproval,
        /// A run journaled with an approval the window did not give.
        RunWithoutDecision,
        /// An approved catalogue other than the one the window approved.
        ForeignCatalogue,
        /// The stop flag gone while the window holds the relay stopped.
        StopFlagRemoved,
        /// The record itself is missing or unreadable while the relay holds
        /// decisions (D-175): `officina run` refuses everything until the
        /// author acknowledges it, which starts a new record.
        RecordBroken,
    }
}

/// One anomaly, as the Relay view lists it and doctor reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Anomaly {
    /// What the author acknowledges it by: its kind and what it is about.
    pub key: String,
    pub kind: AnomalyKind,
    /// One sentence for the author.
    pub detail: String,
    /// When it happened, when known.
    pub at_ms: Option<i64>,
    pub acknowledged: bool,
}

impl AnomalyKind {
    /// The key of an anomaly of this kind about `subject`: a prefix naming
    /// the kind, then the subject.
    fn key(self, subject: &str) -> String {
        let prefix = match self {
            AnomalyKind::ForeignApproval => "approval",
            AnomalyKind::RunWithoutDecision => "run",
            AnomalyKind::ForeignCatalogue => "catalogue",
            AnomalyKind::StopFlagRemoved => "stop",
            AnomalyKind::RecordBroken => "record",
        };
        format!("{prefix}:{subject}")
    }
}

/// What the anomalies are judged on, as the host read it.
#[derive(Debug, Clone, Copy)]
pub struct AnomalyInputs<'a> {
    pub record: &'a RecordState,
    pub files: &'a ApprovalFiles,
    /// The journal lines read, two days of them.
    pub lines: &'a [JournalLine],
    /// The hash in `relay/approved.json`, when it reads.
    pub approved_hash: Option<&'a str>,
    /// Whether `relay/approved.json` exists, readable or not.
    pub approved_exists: bool,
    /// Whether `relay/stopped` is there.
    pub stopped: bool,
}

/// The anomalies (D-173, D-175), oldest first. A broken record is the one
/// anomaly ([`RecordState::broken`]), never acknowledged, since the record
/// that would hold the acknowledgement is what is broken; a record missing
/// before the relay holds anything is none. Decisions and runs older than
/// the record are not judged, except an approval file the record holds
/// with another verdict or request key.
pub fn anomalies(inputs: &AnomalyInputs<'_>) -> Vec<Anomaly> {
    let record = match inputs.record {
        RecordState::Valid(record) => record,
        other => {
            let holds = holds_decisions(inputs.approved_exists, inputs.files);
            return other
                .broken(holds)
                .map(|why| Anomaly {
                    key: record_anomaly_key(),
                    kind: AnomalyKind::RecordBroken,
                    detail: format!(
                        "the Officina window's record of its decisions cannot vouch for anything: {why}; `officina run` refuses every action until you acknowledge this, which starts a new record from the approved catalogue and the stop flag as they are"
                    ),
                    at_ms: None,
                    acknowledged: false,
                })
                .into_iter()
                .collect();
        }
    };
    let mut found = Vec::new();
    let mut push = |kind: AnomalyKind, subject: &str, detail: String, at_ms: Option<i64>| {
        let key = kind.key(subject);
        let acknowledged = record.acknowledged.contains_key(&key);
        found.push(Anomaly {
            key,
            kind,
            detail,
            at_ms,
            acknowledged,
        });
    };
    let mut seen = std::collections::BTreeSet::new();
    for (id, decision) in inputs.files.approved.iter().chain(&inputs.files.consumed) {
        let why = match record.decided.get(id) {
            None if decision.decided_at_ms >= record.since_ms => {
                format!("approval {id} was not given in the Officina window")
            }
            None => continue,
            Some(recorded) if recorded.verdict != Verdict::Approved => {
                format!(
                    "approval {id} was rejected in the Officina window, and a file says it was approved"
                )
            }
            Some(recorded) if recorded.request_key != decision.request_key => format!(
                "approval {id} names another request than the one approved in the Officina window"
            ),
            Some(_) => continue,
        };
        if seen.insert(id.clone()) {
            push(
                AnomalyKind::ForeignApproval,
                id,
                why,
                Some(decision.decided_at_ms),
            );
        }
    }
    let approved_here = |id: &str| {
        record
            .decided
            .get(id)
            .is_some_and(|recorded| recorded.verdict == Verdict::Approved)
    };
    for line in inputs
        .lines
        .iter()
        .filter(|line| line.ts >= record.since_ms)
    {
        if let JournalEntry::RunStarted(started) = &line.entry
            && let Some(id) = &started.approval_id
            && !approved_here(id)
        {
            push(
                AnomalyKind::RunWithoutDecision,
                &started.run_id,
                format!(
                    "{} ran in {} with approval {id}, which the Officina window did not give",
                    started.action, started.project
                ),
                Some(line.ts),
            );
        }
    }
    if let Some(hash) = inputs.approved_hash
        && record.approved_hash.as_deref() != Some(hash)
    {
        push(
            AnomalyKind::ForeignCatalogue,
            hash,
            format!(
                "the approved catalogue {} was not approved in the Officina window",
                hash.get(..12).unwrap_or(hash)
            ),
            None,
        );
    }
    if let Some(since) = record.stopped_since_ms
        && !inputs.stopped
    {
        push(
            AnomalyKind::StopFlagRemoved,
            &since.to_string(),
            "the relay was stopped in the Officina window, and its stop flag is gone".to_owned(),
            Some(since),
        );
    }
    found.sort_by_key(|anomaly| anomaly.at_ms.unwrap_or(i64::MAX));
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::approvals::Decision;
    use crate::relay::catalogue::ActionClass;
    use crate::relay::journal::RunStarted;

    const NOW: i64 = 1_793_102_400_000;
    const MIN: i64 = 60_000;

    fn decision(at: i64) -> Decision {
        Decision {
            decided_at_ms: at,
            request_key: "ab".to_owned(),
        }
    }

    fn run(ts: i64, run_id: &str, approval_id: Option<&str>) -> JournalLine {
        JournalLine::new(
            ts,
            JournalEntry::RunStarted(RunStarted {
                run_id: run_id.to_owned(),
                project: "lab".to_owned(),
                session_id: None,
                action: "restart".to_owned(),
                class: ActionClass::Human,
                params: BTreeMap::new(),
                argv: vec!["C:/path/to/tool.exe".to_owned()],
                method: None,
                url: None,
                credential: None,
                catalogue_hash: "00".to_owned(),
                approval_id: approval_id.map(str::to_owned),
                tree: None,
                timeout_s: Some(60),
            }),
        )
    }

    fn kinds(found: &[Anomaly]) -> Vec<AnomalyKind> {
        found.iter().map(|a| a.kind).collect()
    }

    fn recorded(verdict: Verdict, request_key: &str, at_ms: i64) -> RecordedDecision {
        RecordedDecision {
            verdict,
            request_key: request_key.to_owned(),
            at_ms,
        }
    }

    fn judged(
        record: &RecordState,
        files: &ApprovalFiles,
        lines: &[JournalLine],
        approved_hash: Option<&str>,
    ) -> Vec<Anomaly> {
        anomalies(&AnomalyInputs {
            record,
            files,
            lines,
            approved_hash,
            approved_exists: approved_hash.is_some(),
            stopped: false,
        })
    }

    #[test]
    fn what_the_window_decided_is_no_anomaly() {
        let mut record = DecisionRecord::start(NOW - 60 * MIN, Some("h1".to_owned()), None);
        record.decided.insert(
            "id-1".to_owned(),
            recorded(Verdict::Approved, "ab", NOW - MIN),
        );
        let mut files = ApprovalFiles::default();
        files
            .consumed
            .insert("id-1".to_owned(), decision(NOW - MIN));
        let lines = [run(NOW, "r1", Some("id-1")), run(NOW, "r2", None)];
        let found = judged(&RecordState::Valid(record), &files, &lines, Some("h1"));
        assert!(found.is_empty(), "{found:?}");
    }

    /// Every kind, each from what the window did not do, the kinds of a
    /// valid record acknowledged by their key; a broken record is the one
    /// kind a valid record cannot show, and is never acknowledged.
    #[test]
    fn every_kind_is_found_and_acknowledged_by_key() {
        let mut record =
            DecisionRecord::start(NOW - 60 * MIN, Some("h1".to_owned()), Some(NOW - 30 * MIN));
        let mut files = ApprovalFiles::default();
        files
            .approved
            .insert("id-2".to_owned(), decision(NOW - 2 * MIN));
        files
            .consumed
            .insert("id-2".to_owned(), decision(NOW - 2 * MIN));
        let lines = [run(NOW - MIN, "r3", Some("id-2"))];
        let found = judged(
            &RecordState::Valid(record.clone()),
            &files,
            &lines,
            Some("h2"),
        );
        assert!(found.iter().all(|a| !a.acknowledged));
        let mut found_kinds = kinds(&found);
        let broken = judged(
            &RecordState::Unreadable("corrupt".into()),
            &files,
            &lines,
            Some("h2"),
        );
        assert_eq!(kinds(&broken), [AnomalyKind::RecordBroken]);
        assert_eq!(broken[0].key, record_anomaly_key());
        assert!(!broken[0].acknowledged);
        found_kinds.extend(kinds(&broken));
        found_kinds.sort_by_key(|k| AnomalyKind::ALL.iter().position(|a| a == k));
        assert_eq!(found_kinds, AnomalyKind::ALL, "{found:#?}");
        for anomaly in &found {
            record.acknowledged.insert(anomaly.key.clone(), NOW);
        }
        assert!(
            judged(&RecordState::Valid(record), &files, &lines, Some("h2"))
                .iter()
                .all(|a| a.acknowledged)
        );
    }

    /// D-175: an approval file the record holds as rejected, or as approved
    /// for another request key, is an anomaly, however old; so is a run
    /// with a rejected approval.
    #[test]
    fn an_approval_against_its_recorded_decision_is_an_anomaly() {
        let mut record = DecisionRecord::start(NOW - 60 * MIN, Some("h1".to_owned()), None);
        record.decided.insert(
            "id-rejected".to_owned(),
            recorded(Verdict::Rejected, "ab", NOW - 3 * MIN),
        );
        record.decided.insert(
            "id-retargeted".to_owned(),
            recorded(Verdict::Approved, "cd", NOW - 3 * MIN),
        );
        let mut files = ApprovalFiles::default();
        files
            .approved
            .insert("id-rejected".to_owned(), decision(NOW - 120 * MIN));
        files
            .approved
            .insert("id-retargeted".to_owned(), decision(NOW - 2 * MIN));
        let lines = [run(NOW - MIN, "r4", Some("id-rejected"))];
        let found = judged(&RecordState::Valid(record), &files, &lines, Some("h1"));
        let keys: Vec<&str> = found.iter().map(|a| a.key.as_str()).collect();
        assert_eq!(
            keys,
            ["approval:id-rejected", "approval:id-retargeted", "run:r4"],
            "{found:#?}"
        );
        assert!(found[0].detail.contains("rejected"), "{}", found[0].detail);
        assert!(
            found[1].detail.contains("another request"),
            "{}",
            found[1].detail
        );
    }

    #[test]
    fn nothing_before_the_record_or_without_one_is_judged() {
        let record = DecisionRecord::start(NOW, None, None);
        let mut files = ApprovalFiles::default();
        files
            .approved
            .insert("id-old".to_owned(), decision(NOW - MIN));
        let lines = [run(NOW - MIN, "r-old", Some("id-old"))];
        assert!(judged(&RecordState::Valid(record), &files, &lines, None).is_empty());
        assert!(
            judged(
                &RecordState::Missing,
                &ApprovalFiles::default(),
                &lines,
                None
            )
            .is_empty(),
            "a record the window has not started yet, with nothing to vouch for"
        );
    }

    /// D-175: a missing record is broken once the relay holds an approved
    /// catalogue or a decision file; an unreadable one always is.
    #[test]
    fn a_missing_record_is_broken_once_the_relay_holds_decisions() {
        let empty = ApprovalFiles::default();
        assert!(!holds_decisions(false, &empty));
        assert!(holds_decisions(true, &empty));
        let mut files = ApprovalFiles::default();
        files
            .rejected
            .insert("id-r".to_owned(), decision(NOW - MIN));
        assert!(holds_decisions(false, &files));
        assert_eq!(RecordState::Missing.broken(false), None);
        assert!(RecordState::Missing.broken(true).is_some());
        let unreadable = RecordState::Unreadable("corrupt".into());
        assert!(unreadable.broken(false).is_some());
        let valid = RecordState::Valid(DecisionRecord::start(NOW, None, None));
        assert_eq!(valid.broken(true), None);
    }

    #[test]
    fn the_record_round_trips_and_prunes() {
        let mut record = DecisionRecord::start(NOW - 10 * 24 * 60 * MIN, None, None);
        record.decided.insert(
            "old".to_owned(),
            recorded(Verdict::Approved, "ab", NOW - RECORD_RETENTION_MS - 1),
        );
        record.decided.insert(
            "new".to_owned(),
            recorded(Verdict::Rejected, "cd", NOW - MIN),
        );
        record
            .acknowledged
            .insert("run:r".to_owned(), NOW - RECORD_RETENTION_MS - 1);
        let bytes = serde_json::to_vec(&record).unwrap();
        assert_eq!(parse_record(&bytes), Ok(record.clone()));
        record.prune(NOW);
        assert_eq!(record.decided.keys().collect::<Vec<_>>(), ["new"]);
        assert!(record.acknowledged.is_empty());
        let other = format!(
            r#"{{"schema":{},"since_ms":1}}"#,
            DECISION_RECORD_SCHEMA + 1
        );
        assert!(
            parse_record(other.as_bytes())
                .unwrap_err()
                .contains("schema")
        );
        assert!(parse_record(b"{ not json").unwrap_err().contains("corrupt"));
    }
}
