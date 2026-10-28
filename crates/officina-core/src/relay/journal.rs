//! The relay's journal (AD-32, FR-13): one JSON line per run start and end,
//! refusal, gate deny or error, and authority decision, in
//! `journal/YYYY-MM-DD.jsonl` (UTC). The host appends under `journal/.lock`
//! and reads two days back; this module holds the line types only.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::catalogue::ActionClass;
use super::gate::DenyKind;
use super::http::HttpMethod;

/// Version of the journal line format. Readers skip lines of another
/// version without counting them corrupt.
pub const JOURNAL_LINE_VERSION: u32 = 1;

/// Longest command text a gate line keeps, in characters (AD-32), as the
/// event buffer keeps prompts.
pub const COMMAND_EXCERPT_CHARS: usize = crate::events::EXCERPT_CHARS;

/// How many days of journal files the window keeps (AD-32, PRD amendment
/// 10). `officina run` and the gate never delete.
pub const JOURNAL_RETENTION_DAYS: i64 = 90;

/// True when a journal file name is a day older than
/// [`JOURNAL_RETENTION_DAYS`] and may be deleted by the window's hourly
/// pass. The lock file and any name that is not a day file never are.
///
/// ```
/// use officina_core::relay::journal::is_expired_journal_file;
/// let now = 1_793_102_400_000; // 2026-10-27
/// assert!(is_expired_journal_file("2026-07-28.jsonl", now));
/// assert!(!is_expired_journal_file("2026-07-29.jsonl", now));
/// assert!(!is_expired_journal_file(".lock", now));
/// ```
pub fn is_expired_journal_file(name: &str, now_ms: i64) -> bool {
    crate::dates::is_dated_file_older_than(name, now_ms, JOURNAL_RETENTION_DAYS)
}

/// One journal line: the version, the time it was written and its entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalLine {
    pub v: u32,
    /// Unix milliseconds.
    pub ts: i64,
    #[serde(flatten)]
    pub entry: JournalEntry,
}

impl JournalLine {
    /// A line of the current version.
    pub fn new(ts: i64, entry: JournalEntry) -> JournalLine {
        JournalLine {
            v: JOURNAL_LINE_VERSION,
            ts,
            entry,
        }
    }

    /// The line serialized with its trailing newline.
    pub fn to_line(&self) -> String {
        // Plain structs, string-keyed maps and unit-only enums always
        // serialize.
        let mut text = serde_json::to_string(self).unwrap_or_default();
        text.push('\n');
        text
    }
}

/// What a line records, tagged by `kind` (AD-32).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JournalEntry {
    RunStarted(RunStarted),
    RunFinished(RunFinished),
    Refused(Refused),
    GateDenied(GateDenied),
    GateError(GateError),
    CatalogueApproved(CatalogueApproved),
    ApprovalRequested(ApprovalEvent),
    ApprovalGiven(ApprovalEvent),
    ApprovalRejected(ApprovalEvent),
    RelayStopped,
    RelayResumed,
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Every `kind` this version writes, so a reader can tell a line of a
    /// later version (unknown kind, skipped) from a broken one (counted).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub enum JournalKind {
        RunStarted,
        RunFinished,
        Refused,
        GateDenied,
        GateError,
        CatalogueApproved,
        ApprovalRequested,
        ApprovalGiven,
        ApprovalRejected,
        RelayStopped,
        RelayResumed,
    }
}

impl JournalEntry {
    /// The entry's `kind`.
    pub fn kind(&self) -> JournalKind {
        match self {
            JournalEntry::RunStarted(_) => JournalKind::RunStarted,
            JournalEntry::RunFinished(_) => JournalKind::RunFinished,
            JournalEntry::Refused(_) => JournalKind::Refused,
            JournalEntry::GateDenied(_) => JournalKind::GateDenied,
            JournalEntry::GateError(_) => JournalKind::GateError,
            JournalEntry::CatalogueApproved(_) => JournalKind::CatalogueApproved,
            JournalEntry::ApprovalRequested(_) => JournalKind::ApprovalRequested,
            JournalEntry::ApprovalGiven(_) => JournalKind::ApprovalGiven,
            JournalEntry::ApprovalRejected(_) => JournalKind::ApprovalRejected,
            JournalEntry::RelayStopped => JournalKind::RelayStopped,
            JournalEntry::RelayResumed => JournalKind::RelayResumed,
        }
    }
}

/// What the tracked and unignored files of a work tree held when a check, an
/// `after-check` action or a `human` action's request ran (AD-33, D-173):
/// SHA-256 over each file's path and the hash of its bytes, which the host
/// reads itself. Two runs on the same files carry equal fingerprints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TreeFingerprint {
    /// Lowercase hex.
    pub sha256: String,
    /// How many files it binds. Absent from lines written before it existed.
    #[serde(default)]
    pub files: u32,
    /// The `git` that listed them, resolved from Git for Windows' install
    /// location, never `PATH` (D-173). Absent from lines written before it
    /// existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,
}

/// `officina run` is about to start a program (AD-29 step 7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RunStarted {
    pub run_id: String,
    pub project: String,
    /// `CLAUDE_CODE_SESSION_ID`, `null` outside Claude Code.
    #[serde(default)]
    pub session_id: Option<String>,
    pub action: String,
    pub class: ActionClass,
    pub params: BTreeMap<String, String>,
    /// Exactly what is started: the program, then its arguments. Absent
    /// for an HTTP action, which records its request instead.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub argv: Vec<String>,
    /// An HTTP action's method (AD-36, D-36).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<HttpMethod>,
    /// An HTTP action's URL, its placeholders filled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The credential an HTTP request carries, by name: never its secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
    pub catalogue_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    /// The files it started on, for an action that binds its work tree in a
    /// git work tree; absent otherwise (D-22, D-173).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree: Option<TreeFingerprint>,
    /// The action's timeout, so a reader can tell a run still going from an
    /// abandoned one (AD-32). Absent from lines written before it existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_s: Option<u32>,
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// How a run ended (AD-32).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum RunOutcome {
        /// The program exited by itself; `exit_code` holds its code.
        Exited,
        /// Officina ended the process tree at the action's `timeout_s`.
        Timeout,
        /// The program could not start.
        SpawnFailed,
        /// An HTTP action got a response; `status` holds its code (D-36).
        Http,
        /// An HTTP action got no response: name resolution, connection or
        /// TLS failure (D-35, D-36).
        ConnectFailed,
    }
}

/// What one output stream carried. Every byte is counted and hashed, the
/// bytes dropped past the output budget included (AD-31).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct StreamSummary {
    pub bytes: u64,
    /// SHA-256 of every byte, lowercase hex.
    pub sha256: String,
}

/// A run ended. A `run_started` without one past its timeout plus 60 s
/// reads "abandoned" (AD-32).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RunFinished {
    pub run_id: String,
    pub outcome: RunOutcome,
    /// The program's own code, full Windows value; absent unless `exited`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
    pub duration_ms: i64,
    /// A process's streams; absent for an HTTP action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout: Option<StreamSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr: Option<StreamSummary>,
    /// Bytes read but not passed on, past `max_output_kib` (AD-31).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub dropped_bytes: u64,
    /// An HTTP response's status code (D-36).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    /// The body bytes read and passed on, at most `max_output_kib`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_bytes: Option<u64>,
    /// SHA-256 of those bytes, lowercase hex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_sha256: Option<String>,
    /// Why the program could not start, why no response came, or that the
    /// body was cut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// An HTTP body broke off before its end for another reason than the
    /// cap: the run failed whatever its status (D-35). Absent from older
    /// lines.
    #[serde(default, skip_serializing_if = "is_false")]
    pub body_broke_off: bool,
}

impl RunFinished {
    /// True when the run did what it was asked: a program that exited 0, or
    /// a response that [`super::http::http_succeeded`]. The one rule the
    /// exit code of `officina run` and the window's colour share.
    pub fn succeeded(&self) -> bool {
        match self.outcome {
            RunOutcome::Exited => self.exit_code == Some(0),
            RunOutcome::Http => self
                .status
                .is_some_and(|status| super::http::http_succeeded(status, self.body_broke_off)),
            RunOutcome::Timeout | RunOutcome::SpawnFailed | RunOutcome::ConnectFailed => false,
        }
    }
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

fn is_false(value: &bool) -> bool {
    !*value
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why `officina run` refused (AD-29), in the order its checks run. The
    /// stderr line and the journal's `refused` line carry the code.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum RefusalReason {
        /// The author stopped the relay in the window (AD-34).
        Stopped,
        /// No catalogue was ever approved.
        NotApproved,
        /// `relay/approved.json` cannot be read: fail closed.
        ApprovedUnreadable,
        /// The window's record of its decisions is missing or cannot be
        /// read while a catalogue is approved: nothing approved can be
        /// confirmed until the author acknowledges it in the Relay view
        /// (D-175).
        DecisionRecordUnreadable,
        /// The approved catalogue is not the one the window's record says
        /// it approved: written by another process, it is not run (D-175).
        CatalogueUnconfirmed,
        /// The relay floor cannot be read or fails phase 1.
        FloorUnreadable,
        /// The floor's catalogue differs from the approved one.
        CatalogueChanged,
        /// The working directory lies in no approved project.
        OutsideProjects,
        /// The project does not list the action.
        ActionNotAllowed,
        /// A declared parameter is missing.
        MissingParameter,
        /// A parameter the action does not declare.
        UnknownParameter,
        /// A value breaks a rule of AD-30 or the parameter's own.
        InvalidValue,
        /// The project is a git work tree, and no `git` is installed where
        /// Git for Windows records it: its files cannot be bound (D-173).
        GitUnavailable,
        /// The work tree holds a file, or more files or bytes, than the
        /// fingerprint reads (D-173).
        TreeTooLarge,
        /// The work tree cannot be read by `git` though a `.git` lies above
        /// it, or a file of it cannot be read, so its files cannot be bound
        /// (D-173, D-175).
        TreeUnreadable,
        /// An `after-check` action, or a `human` action naming its dry run,
        /// without a successful run of its check, with the same parameters
        /// on the same code, in the last 30 minutes (AD-33, D-170).
        CheckRequired,
        /// The author rejected this request in the window in the last 15
        /// minutes (AD-33).
        ApprovalRejected,
        /// The approval request could not be written.
        ApprovalUnavailable,
        /// No decision came within `--wait` (AD-33).
        ApprovalPending,
        /// An approval file the window's record does not name: written by
        /// another process, it is left unused (D-173).
        ApprovalNotGiven,
        /// The code changed since the dry run the approval was given for: the
        /// approval stays unconsumed (D-170).
        CheckNotSatisfied,
        /// The tracked and unignored files of a `human` action naming no
        /// check changed since its request: the approval stays unconsumed
        /// (D-173).
        TreeChanged,
        /// The credential an HTTP action names is not in the Credential
        /// Manager: nothing was sent, an approval stays unconsumed (D-29).
        CredentialMissing,
        /// The credential cannot be read or decoded: nothing was sent, an
        /// approval stays unconsumed (D-29).
        CredentialUnreadable,
    }
}

/// `officina run` refused before starting anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Refused {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<BTreeMap<String, String>>,
    pub reason: RefusalReason,
    pub detail: String,
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why a `gate_error` is journaled (AD-28, D-15, D-452): an input the
    /// gate could not read, which made it stricter, an internal fault, which
    /// refused the call unless it lay outside every guarded project, or a
    /// gate input the window could not write. Also the journal's `cause`
    /// vocabulary.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum GateFault {
        /// `relay/approved.json` cannot be read or parsed.
        ApprovedUnreadable,
        /// `relay/approved.json` is gone while the known-projects record
        /// names an approved catalogue.
        ApprovedMissing,
        /// The relay floor exists but cannot be read.
        FloorUnreadable,
        /// The relay floor fails phase 1.
        FloorInvalid,
        /// The relay floor is gone while the relay is in use.
        FloorMissing,
        /// The floor and the snapshot both failed and the known-projects
        /// record cannot be read either: nothing is guarded.
        KnownProjectsUnreadable,
        /// The window could not write the known-projects record: what a
        /// broken floor falls back to (D-15) stays as it was.
        KnownProjectsUnwritable,
        /// A panic inside the gate (D-452): a deny when the call lies in a
        /// guarded project or cannot be located, no decision when it lies
        /// outside every guarded project.
        Panic,
    }
}

/// The gate denied a tool call: a shell command, or a file write under the
/// Officina home (AD-26, D-171, D-173).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GateDenied {
    pub project: String,
    #[serde(default)]
    pub session_id: Option<String>,
    pub deny: DenyKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched: Option<String>,
    /// The command's first [`COMMAND_EXCERPT_CHARS`] characters; absent
    /// when excerpts are off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_excerpt: Option<String>,
}

/// The gate met a fault: an input it could not read (D-15) or an internal
/// error (AD-28). The decision it took stands beside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GateError {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    pub cause: GateFault,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_excerpt: Option<String>,
}

/// The author approved a catalogue in the window (AD-24).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CatalogueApproved {
    pub hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_hash: Option<String>,
}

/// A `human` approval request and its decisions (AD-33).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ApprovalEvent {
    pub approval_id: String,
    pub request_key: String,
    /// The request's project and action (D-172), so approval metrics are
    /// computed from the journal alone. Absent from lines written before
    /// they existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// On `approval_requested` only: true when the action names a check, its
    /// dry run (D-170, D-172).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
}

/// How a reader classifies one raw line.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // returned by value and consumed at once, never stored
pub enum ParsedLine {
    Line(JournalLine),
    /// Another format version, or a `kind` this version does not know:
    /// written by a later Officina, skipped and not counted.
    Foreign,
    Corrupt,
}

/// Parses one line of a journal file (no newline).
///
/// ```
/// use officina_core::relay::journal::{ParsedLine, parse_line};
/// assert!(matches!(parse_line(r#"{"v":1,"ts":5,"kind":"relay_stopped"}"#), ParsedLine::Line(_)));
/// assert_eq!(parse_line(r#"{"v":1,"ts":5,"kind":"from_the_future"}"#), ParsedLine::Foreign);
/// assert_eq!(parse_line(r#"{"v":1,"ts":5,"kind":"refused"}"#), ParsedLine::Corrupt);
/// ```
pub fn parse_line(line: &str) -> ParsedLine {
    if let Ok(parsed) = serde_json::from_str::<JournalLine>(line)
        && parsed.v == JOURNAL_LINE_VERSION
    {
        return ParsedLine::Line(parsed);
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return ParsedLine::Corrupt;
    };
    let version = value.get("v").and_then(serde_json::Value::as_u64);
    if version.is_some_and(|v| v != u64::from(JOURNAL_LINE_VERSION)) {
        return ParsedLine::Foreign;
    }
    let known_kind = value
        .get("kind")
        .and_then(|kind| serde_json::from_value::<JournalKind>(kind.clone()).ok())
        .is_some();
    let unknown_kind = value.get("kind").is_some_and(serde_json::Value::is_string) && !known_kind;
    if version.is_some() && unknown_kind {
        ParsedLine::Foreign
    } else {
        ParsedLine::Corrupt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(kind: JournalKind) -> JournalEntry {
        let stream = StreamSummary {
            bytes: 3,
            sha256: crate::relay::sha256_hex(b"abc"),
        };
        match kind {
            JournalKind::RunStarted => JournalEntry::RunStarted(RunStarted {
                run_id: "r1".into(),
                project: "lab".into(),
                session_id: None,
                action: "check".into(),
                class: ActionClass::Free,
                params: BTreeMap::from([("group".into(), "lab-web".into())]),
                argv: vec!["C:/path/to/tool.exe".into(), "lab-web".into()],
                method: None,
                url: None,
                credential: None,
                catalogue_hash: "00".into(),
                approval_id: None,
                tree: Some(TreeFingerprint {
                    sha256: crate::relay::sha256_hex(b"tree"),
                    files: 3,
                    git: Some(r"C:\Program Files\Git\cmd\git.exe".into()),
                }),
                timeout_s: Some(540),
            }),
            JournalKind::RunFinished => JournalEntry::RunFinished(RunFinished {
                body_broke_off: false,
                run_id: "r1".into(),
                outcome: RunOutcome::Exited,
                exit_code: Some(3_221_225_477),
                duration_ms: 12,
                stdout: Some(stream.clone()),
                stderr: Some(stream),
                dropped_bytes: 0,
                status: None,
                body_bytes: None,
                body_sha256: None,
                detail: None,
            }),
            JournalKind::Refused => JournalEntry::Refused(Refused {
                project: None,
                session_id: Some("s".into()),
                action: "check".into(),
                params: None,
                reason: RefusalReason::Stopped,
                detail: "stopped".into(),
            }),
            JournalKind::GateDenied => JournalEntry::GateDenied(GateDenied {
                project: "lab".into(),
                session_id: None,
                deny: DenyKind::GuardedProgram,
                matched: Some("ssh".into()),
                command_excerpt: Some("ssh host01".into()),
            }),
            JournalKind::GateError => JournalEntry::GateError(GateError {
                project: None,
                session_id: None,
                cause: GateFault::Panic,
                detail: "boom".into(),
                command_excerpt: None,
            }),
            JournalKind::CatalogueApproved => JournalEntry::CatalogueApproved(CatalogueApproved {
                hash: "01".into(),
                previous_hash: None,
            }),
            JournalKind::ApprovalRequested => JournalEntry::ApprovalRequested(approval()),
            JournalKind::ApprovalGiven => JournalEntry::ApprovalGiven(approval()),
            JournalKind::ApprovalRejected => JournalEntry::ApprovalRejected(approval()),
            JournalKind::RelayStopped => JournalEntry::RelayStopped,
            JournalKind::RelayResumed => JournalEntry::RelayResumed,
        }
    }

    fn approval() -> ApprovalEvent {
        ApprovalEvent {
            approval_id: "0123456789abcdef-1".into(),
            request_key: "ab".into(),
            project: Some("lab".into()),
            action: Some("apply".into()),
            dry_run: Some(true),
        }
    }

    /// D-172: a request line names its project, its action and whether the
    /// action has a dry run; a decision line its project and action.
    #[test]
    fn approval_requested_names_its_action() {
        let line = JournalLine::new(3, JournalEntry::ApprovalRequested(approval())).to_line();
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            (&value["project"], &value["action"], &value["dry_run"]),
            (
                &serde_json::json!("lab"),
                &serde_json::json!("apply"),
                &serde_json::json!(true)
            )
        );
        let given = ApprovalEvent {
            dry_run: None,
            ..approval()
        };
        let line = JournalLine::new(3, JournalEntry::ApprovalGiven(given)).to_line();
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["action"], "apply");
        assert!(value.get("dry_run").is_none(), "{line}");
    }

    /// Lines written before D-172 parse, with the new fields absent, and the
    /// line version does not move.
    #[test]
    fn older_approval_lines_still_parse() {
        for kind in ["approval_requested", "approval_given", "approval_rejected"] {
            let old = format!(
                r#"{{"v":1,"ts":5,"kind":"{kind}","approval_id":"0123456789abcdef-1","request_key":"ab"}}"#
            );
            let ParsedLine::Line(line) = parse_line(&old) else {
                panic!("{old}");
            };
            let event = match line.entry {
                JournalEntry::ApprovalRequested(e)
                | JournalEntry::ApprovalGiven(e)
                | JournalEntry::ApprovalRejected(e) => e,
                other => panic!("{other:?}"),
            };
            assert_eq!(
                (event.project, event.action, event.dry_run),
                (None, None, None)
            );
        }
        assert_eq!(JOURNAL_LINE_VERSION, 1);
    }

    #[test]
    fn every_kind_round_trips_with_its_tag() {
        for kind in JournalKind::ALL {
            let entry = sample(*kind);
            assert_eq!(entry.kind(), *kind);
            let line = JournalLine::new(7, entry).to_line();
            assert!(line.ends_with('\n') && !line.trim_end().contains('\n'));
            let value: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(value["v"], JOURNAL_LINE_VERSION);
            assert_eq!(value["kind"], serde_json::to_value(kind).unwrap());
            assert_eq!(
                parse_line(line.trim_end()),
                ParsedLine::Line(JournalLine::new(7, sample(*kind)))
            );
        }
    }

    /// AC 9 of story 2.11 (D-36): an HTTP run records its request instead of
    /// an argument vector, and its response instead of streams.
    #[test]
    fn http_lines_round_trip() {
        let started = JournalEntry::RunStarted(RunStarted {
            run_id: "r2".into(),
            project: "lab".into(),
            session_id: None,
            action: "health".into(),
            class: ActionClass::Free,
            params: BTreeMap::from([("check".into(), "a b".into())]),
            argv: Vec::new(),
            method: Some(HttpMethod::Get),
            url: Some("https://monitoring.example.com/a%20b".into()),
            credential: Some("monitoring".into()),
            catalogue_hash: "00".into(),
            approval_id: None,
            tree: None,
            timeout_s: Some(60),
        });
        let response = |outcome: RunOutcome,
                        status: Option<u16>,
                        body: Option<&[u8]>,
                        detail: Option<&str>| {
            JournalEntry::RunFinished(RunFinished {
                body_broke_off: false,
                run_id: "r2".into(),
                outcome,
                exit_code: None,
                duration_ms: 40,
                stdout: None,
                stderr: None,
                dropped_bytes: 0,
                status,
                body_bytes: body.map(|b| b.len() as u64),
                body_sha256: body.map(crate::relay::sha256_hex),
                detail: detail.map(str::to_owned),
            })
        };
        let finished = [
            response(RunOutcome::Http, Some(503), Some(b"down"), None),
            response(RunOutcome::Timeout, None, None, None),
            response(RunOutcome::ConnectFailed, None, None, Some("refused")),
        ];
        for entry in std::iter::once(started).chain(finished) {
            let line = JournalLine::new(9, entry.clone()).to_line();
            let value: serde_json::Value = serde_json::from_str(&line).unwrap();
            let object = value.as_object().unwrap();
            match &entry {
                JournalEntry::RunStarted(_) => {
                    assert!(!object.contains_key("argv"), "{line}");
                    assert_eq!(value["method"], "GET");
                    assert_eq!(value["credential"], "monitoring");
                }
                _ => assert!(
                    !object.contains_key("stdout") && !object.contains_key("stderr"),
                    "{line}"
                ),
            }
            assert_eq!(
                parse_line(line.trim_end()),
                ParsedLine::Line(JournalLine::new(9, entry))
            );
        }
        let http: serde_json::Value = serde_json::from_str(
            &JournalLine::new(9, response(RunOutcome::Http, Some(200), Some(b"ok"), None))
                .to_line(),
        )
        .unwrap();
        assert_eq!(
            (&http["outcome"], &http["status"], &http["body_bytes"]),
            (
                &serde_json::json!("http"),
                &serde_json::json!(200),
                &serde_json::json!(2)
            )
        );
    }

    #[test]
    fn absent_session_is_written_as_null() {
        let line = JournalLine::new(1, sample(JournalKind::RunStarted)).to_line();
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert!(value["session_id"].is_null());
        assert!(value.as_object().unwrap().contains_key("session_id"));
    }

    #[test]
    fn foreign_and_corrupt_lines_are_told_apart() {
        for foreign in [
            r#"{"v":2,"ts":1,"kind":"run_started"}"#,
            r#"{"v":1,"ts":1,"kind":"relay_paused","by":"x"}"#,
        ] {
            assert_eq!(parse_line(foreign), ParsedLine::Foreign, "{foreign}");
        }
        for corrupt in [
            "garbage",
            "{}",
            r#"{"v":1,"ts":1}"#,
            r#"{"v":1,"ts":1,"kind":7}"#,
            r#"{"ts":1,"kind":"relay_paused"}"#,
            r#"{"v":1,"ts":"x","kind":"relay_stopped"}"#,
            r#"{"v":1,"ts":1,"kind":"gate_denied","project":"p"}"#,
        ] {
            assert_eq!(parse_line(corrupt), ParsedLine::Corrupt, "{corrupt}");
        }
    }
}
