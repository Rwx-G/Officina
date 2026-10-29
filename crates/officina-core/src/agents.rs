//! Parsing of `claude agents --json`, the first-rank source for session state.
//!
//! Every field is optional on purpose: a field Claude Code stops sending must
//! surface as "unknown" on the board, never as a default value that reads like
//! a real state (research notes E1).

use serde::{Deserialize, Serialize};

/// Whether a session runs in a terminal the author drives, or under the
/// background supervisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    Interactive,
    Background,
}

/// Live process activity, present only while the process runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum LiveStatus {
    Busy,
    Waiting,
    Idle,
}

impl LiveStatus {
    /// Every documented value (research notes E1).
    pub const ALL: [LiveStatus; 3] = [Self::Busy, Self::Waiting, Self::Idle];
}

/// Lifecycle of a background session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LifecycleState {
    Working,
    Blocked,
    Done,
    Failed,
    Stopped,
}

impl LifecycleState {
    /// Every documented value (research notes E1).
    pub const ALL: [LifecycleState; 5] = [
        Self::Working,
        Self::Blocked,
        Self::Done,
        Self::Failed,
        Self::Stopped,
    ];
}

/// One entry of `claude agents --json`. Values outside the documented enums
/// are kept as `None` plus the raw text in `unrecognized`, so a new Claude Code
/// value shows up in the contract canary instead of being silently dropped.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentRecord {
    pub id: Option<String>,
    pub session_id: Option<String>,
    pub name: Option<String>,
    pub cwd: Option<String>,
    pub kind: Option<SessionKind>,
    pub pid: Option<u32>,
    pub started_at_ms: Option<i64>,
    pub status: Option<LiveStatus>,
    pub state: Option<LifecycleState>,
    pub waiting_for: Option<String>,
    pub unrecognized: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawAgent {
    id: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    name: Option<String>,
    cwd: Option<String>,
    kind: Option<String>,
    pid: Option<u32>,
    #[serde(rename = "startedAt")]
    started_at: Option<i64>,
    status: Option<String>,
    state: Option<String>,
    #[serde(rename = "waitingFor")]
    waiting_for: Option<String>,
}

/// Error returned when the output is not a JSON array at all. Individual
/// entries that fail to parse are skipped, not fatal.
#[derive(Debug, PartialEq, Eq)]
pub struct AgentsParseError(pub String);

impl std::fmt::Display for AgentsParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "claude agents output is not a JSON array: {}", self.0)
    }
}

impl std::error::Error for AgentsParseError {}

fn parse_enum<T: for<'de> Deserialize<'de>>(
    field: &str,
    raw: Option<String>,
    unrecognized: &mut Vec<String>,
) -> Option<T> {
    let text = raw?;
    match serde_json::from_value(serde_json::Value::String(text.clone())) {
        Ok(value) => Some(value),
        Err(_) => {
            unrecognized.push(format!("{field}={text}"));
            None
        }
    }
}

/// Parses the full stdout of `claude agents --json [--all]`.
///
/// ```
/// let records = officina_core::agents::parse_agents(
///     r#"[{"sessionId":"s1","kind":"interactive","status":"busy","cwd":"C:\\repo"}]"#,
/// )
/// .unwrap();
/// assert_eq!(records[0].session_id.as_deref(), Some("s1"));
/// ```
pub fn parse_agents(stdout: &str) -> Result<Vec<AgentRecord>, AgentsParseError> {
    let entries: Vec<serde_json::Value> =
        serde_json::from_str(stdout.trim()).map_err(|e| AgentsParseError(e.to_string()))?;
    Ok(entries
        .into_iter()
        .filter_map(|value| serde_json::from_value::<RawAgent>(value).ok())
        .map(|raw| {
            let mut unrecognized = Vec::new();
            let kind = parse_enum("kind", raw.kind, &mut unrecognized);
            let status = parse_enum("status", raw.status, &mut unrecognized);
            let state = parse_enum("state", raw.state, &mut unrecognized);
            AgentRecord {
                id: raw.id,
                session_id: raw.session_id,
                name: raw.name,
                cwd: raw.cwd,
                kind,
                pid: raw.pid,
                started_at_ms: raw.started_at,
                status,
                state,
                waiting_for: raw.waiting_for,
                unrecognized,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_verified_background_and_interactive_shapes() {
        let out = r#"[
          {"pid": 1, "cwd": "C:\\a", "kind": "interactive", "startedAt": 10,
           "sessionId": "s-int", "name": "alpha", "status": "busy"},
          {"pid": 2, "id": "abc", "cwd": "C:\\b", "kind": "background", "startedAt": 20,
           "sessionId": "s-bg", "name": "beta", "status": "waiting", "state": "blocked",
           "waitingFor": "permission prompt"},
          {"id": "def", "cwd": "C:\\c", "kind": "background", "startedAt": 30,
           "sessionId": "s-old", "name": "gamma", "state": "done"}
        ]"#;
        let records = parse_agents(out).unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].kind, Some(SessionKind::Interactive));
        assert_eq!(records[0].status, Some(LiveStatus::Busy));
        assert_eq!(records[0].state, None);
        assert_eq!(records[1].state, Some(LifecycleState::Blocked));
        assert_eq!(records[1].waiting_for.as_deref(), Some("permission prompt"));
        assert_eq!(records[2].pid, None);
        assert_eq!(records[2].status, None);
    }

    #[test]
    fn unknown_enum_value_is_reported_not_defaulted() {
        let records =
            parse_agents(r#"[{"sessionId":"s","status":"pondering","kind":"robot"}]"#).unwrap();
        assert_eq!(records[0].status, None);
        assert_eq!(records[0].kind, None);
        assert_eq!(
            records[0].unrecognized,
            vec!["kind=robot", "status=pondering"]
        );
    }

    #[test]
    fn malformed_entry_is_skipped_and_non_array_is_an_error() {
        let records = parse_agents(r#"[{"sessionId": 5}, {"sessionId": "ok"}]"#).unwrap();
        assert_eq!(records.len(), 1);
        assert!(parse_agents("not json").is_err());
        assert!(parse_agents("{}").is_err());
    }

    #[test]
    fn empty_array_is_an_empty_board_not_an_error() {
        assert_eq!(parse_agents("[]\n").unwrap(), Vec::new());
    }
}
