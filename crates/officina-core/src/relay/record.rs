//! The window's record of its own decisions (D-173): which requests it
//! decided, which catalogue it approved, whether it holds the relay stopped,
//! and which anomalies the author acknowledged. A decision file, a run or a
//! stop flag's absence the record cannot account for is an anomaly: the
//! Relay view lists it and doctor fails until the author acknowledges it.
//!
//! Every file of the relay is writable by any process running as the
//! author, the record included: it surfaces the cheap forgeries, the file
//! tools and plain writes, not a session that rewrites the record too
//! (AD-25). Pure: the host reads and writes `relay/decisions.json`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::approvals::ApprovalFiles;
use super::journal::{JournalEntry, JournalLine};

/// Version of `relay/decisions.json`.
pub const DECISION_RECORD_SCHEMA: u32 = 1;
/// The file under the relay directory.
pub const DECISION_RECORD_FILE: &str = "decisions.json";
/// How long the record keeps a decided id or an acknowledgement: past the
/// approvals' retention and the two days of journal the view reads.
pub const RECORD_RETENTION_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// Content of `relay/decisions.json`, written by the window only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionRecord {
    pub schema: u32,
    /// When the window started this record: nothing older is judged.
    pub since_ms: i64,
    /// The requests the window approved or rejected, with when.
    #[serde(default)]
    pub decided: BTreeMap<String, i64>,
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
        let fresh = |at: &mut i64| now_ms - *at <= RECORD_RETENTION_MS;
        self.decided.retain(|_, at| fresh(at));
        self.acknowledged.retain(|_, at| fresh(at));
    }
}

/// Parses `relay/decisions.json`, refusing another schema.
pub fn parse_record(content: &[u8]) -> Option<DecisionRecord> {
    serde_json::from_slice::<DecisionRecord>(content)
        .ok()
        .filter(|record| record.schema == DECISION_RECORD_SCHEMA)
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// A decision the window did not make (D-173).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum AnomalyKind {
        /// An approval file, used or not, for a request the window did not
        /// decide.
        ForeignApproval,
        /// A run journaled with an approval the window did not give.
        RunWithoutDecision,
        /// An approved catalogue other than the one the window approved.
        ForeignCatalogue,
        /// The stop flag gone while the window holds the relay stopped.
        StopFlagRemoved,
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
        };
        format!("{prefix}:{subject}")
    }
}

/// What the anomalies are judged on, as the host read it.
#[derive(Debug, Clone, Copy)]
pub struct AnomalyInputs<'a> {
    pub record: Option<&'a DecisionRecord>,
    pub files: &'a ApprovalFiles,
    /// The journal lines read, two days of them.
    pub lines: &'a [JournalLine],
    /// The hash in `relay/approved.json`, when it reads.
    pub approved_hash: Option<&'a str>,
    /// Whether `relay/stopped` is there.
    pub stopped: bool,
}

/// The anomalies (D-173), oldest first; none without a record, which the
/// window starts when it first runs. Decisions and runs older than the
/// record are not judged.
pub fn anomalies(inputs: &AnomalyInputs<'_>) -> Vec<Anomaly> {
    let Some(record) = inputs.record else {
        return Vec::new();
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
    let decided = |id: &str| record.decided.contains_key(id);
    let approvals = inputs
        .files
        .approved
        .iter()
        .chain(&inputs.files.consumed)
        .filter(|(id, decision)| !decided(id) && decision.decided_at_ms >= record.since_ms);
    let mut seen = std::collections::BTreeSet::new();
    for (id, decision) in approvals {
        if seen.insert(id.clone()) {
            push(
                AnomalyKind::ForeignApproval,
                id,
                format!("approval {id} was not given in the Officina window"),
                Some(decision.decided_at_ms),
            );
        }
    }
    for line in inputs
        .lines
        .iter()
        .filter(|line| line.ts >= record.since_ms)
    {
        if let JournalEntry::RunStarted(started) = &line.entry
            && let Some(id) = &started.approval_id
            && !decided(id)
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

    #[test]
    fn what_the_window_decided_is_no_anomaly() {
        let mut record = DecisionRecord::start(NOW - 60 * MIN, Some("h1".to_owned()), None);
        record.decided.insert("id-1".to_owned(), NOW - MIN);
        let mut files = ApprovalFiles::default();
        files
            .consumed
            .insert("id-1".to_owned(), decision(NOW - MIN));
        let lines = [run(NOW, "r1", Some("id-1")), run(NOW, "r2", None)];
        let found = anomalies(&AnomalyInputs {
            record: Some(&record),
            files: &files,
            lines: &lines,
            approved_hash: Some("h1"),
            stopped: false,
        });
        assert!(found.is_empty(), "{found:?}");
    }

    /// Every kind, each from what the window did not do; each one
    /// acknowledged by its key.
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
        fn judged(
            record: &DecisionRecord,
            files: &ApprovalFiles,
            lines: &[JournalLine],
        ) -> Vec<Anomaly> {
            anomalies(&AnomalyInputs {
                record: Some(record),
                files,
                lines,
                approved_hash: Some("h2"),
                stopped: false,
            })
        }
        let found = judged(&record, &files, &lines);
        let mut found_kinds = kinds(&found);
        found_kinds.sort_by_key(|k| AnomalyKind::ALL.iter().position(|a| a == k));
        assert_eq!(found_kinds, AnomalyKind::ALL, "{found:#?}");
        assert!(found.iter().all(|a| !a.acknowledged));
        for anomaly in &found {
            record.acknowledged.insert(anomaly.key.clone(), NOW);
        }
        assert!(
            judged(&record, &files, &lines)
                .iter()
                .all(|a| a.acknowledged)
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
        let inputs = |record| AnomalyInputs {
            record,
            files: &files,
            lines: &lines,
            approved_hash: None,
            stopped: true,
        };
        assert!(anomalies(&inputs(Some(&record))).is_empty());
        assert!(anomalies(&inputs(None)).is_empty());
    }

    #[test]
    fn the_record_round_trips_and_prunes() {
        let mut record = DecisionRecord::start(NOW - 10 * 24 * 60 * MIN, None, None);
        record
            .decided
            .insert("old".to_owned(), NOW - RECORD_RETENTION_MS - 1);
        record.decided.insert("new".to_owned(), NOW - MIN);
        record
            .acknowledged
            .insert("run:r".to_owned(), NOW - RECORD_RETENTION_MS - 1);
        let bytes = serde_json::to_vec(&record).unwrap();
        assert_eq!(parse_record(&bytes), Some(record.clone()));
        record.prune(NOW);
        assert_eq!(record.decided.keys().collect::<Vec<_>>(), ["new"]);
        assert!(record.acknowledged.is_empty());
        let other = br#"{"schema":2,"since_ms":1}"#;
        assert_eq!(parse_record(other), None);
    }
}
