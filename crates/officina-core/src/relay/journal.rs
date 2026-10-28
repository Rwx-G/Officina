//! The relay's journal (AD-32, FR-13): one JSON line per run start and end,
//! refusal, gate deny or error, and authority decision, in
//! `journal/YYYY-MM-DD.jsonl` (UTC). The host appends under `journal/.lock`
//! and reads two days back; this module holds the line types only.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::catalogue::ActionClass;
use super::gate::DenyKind;

/// Version of the journal line format. Readers skip lines of another
/// version without counting them corrupt.
pub const JOURNAL_LINE_VERSION: u32 = 1;

/// Longest command text a gate line keeps, in characters (AD-32), as the
/// event buffer keeps prompts.
pub const COMMAND_EXCERPT_CHARS: usize = crate::events::EXCERPT_CHARS;

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
    /// Every `kind` this version writes, so a reader can tell a line of a
    /// later version (unknown kind, skipped) from a broken one (counted).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
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

/// `officina run` is about to start a program (AD-29 step 7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunStarted {
    pub run_id: String,
    pub project: String,
    /// `CLAUDE_CODE_SESSION_ID`, `null` outside Claude Code.
    #[serde(default)]
    pub session_id: Option<String>,
    pub action: String,
    pub class: ActionClass,
    pub params: BTreeMap<String, String>,
    /// Exactly what is started: the program, then its arguments.
    pub argv: Vec<String>,
    pub catalogue_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree: Option<String>,
}

closed_enum! {
    /// How a run ended (AD-32).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum RunOutcome {
        /// The program exited by itself; `exit_code` holds its code.
        Exited,
        /// Officina ended the process tree at the action's `timeout_s`.
        Timeout,
        /// The program could not start.
        SpawnFailed,
    }
}

/// What one output stream carried. Every byte is counted and hashed, the
/// bytes dropped past the output budget included (AD-31).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamSummary {
    pub bytes: u64,
    /// SHA-256 of every byte, lowercase hex.
    pub sha256: String,
}

/// A run ended. A `run_started` without one past its timeout plus 60 s
/// reads "abandoned" (AD-32).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunFinished {
    pub run_id: String,
    pub outcome: RunOutcome,
    /// The program's own code, full Windows value; absent unless `exited`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
    pub duration_ms: i64,
    pub stdout: StreamSummary,
    pub stderr: StreamSummary,
    /// Bytes read but not passed on, past `max_output_kib` (AD-31).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub dropped_bytes: u64,
    /// Why the program could not start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

closed_enum! {
    /// Why `officina run` refused (AD-29), in the order its checks run. The
    /// stderr line and the journal's `refused` line carry the code.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum RefusalReason {
        /// The author stopped the relay in the window (AD-34).
        Stopped,
        /// No catalogue was ever approved.
        NotApproved,
        /// `relay/approved.json` cannot be read: fail closed.
        ApprovedUnreadable,
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
        /// The action's class does not run in this build (D-20).
        ClassNotAvailable,
    }
}

/// `officina run` refused before starting anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Why the gate journals a `gate_error` (AD-28, D-15): an input it could
    /// not read, which made it stricter, or an internal fault, which made it
    /// decide nothing. Also the journal's `cause` vocabulary.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
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
        /// An error inside the gate: no decision (AD-28).
        Io,
        /// A panic inside the gate: no decision (AD-28).
        Panic,
    }
}

/// The gate denied a `Bash` command (AD-26).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
pub struct CatalogueApproved {
    pub hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_hash: Option<String>,
}

/// A `human` approval request and its decisions (AD-33).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalEvent {
    pub approval_id: String,
    pub request_key: String,
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
                catalogue_hash: "00".into(),
                approval_id: None,
                tree: None,
            }),
            JournalKind::RunFinished => JournalEntry::RunFinished(RunFinished {
                run_id: "r1".into(),
                outcome: RunOutcome::Exited,
                exit_code: Some(3_221_225_477),
                duration_ms: 12,
                stdout: stream.clone(),
                stderr: stream,
                dropped_bytes: 0,
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
        }
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
