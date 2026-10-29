//! The orchestrator session as the window knows it (AD-89, D-144, D-166):
//! which agents entry is it, what the board shows of it, and how a Start
//! is answered. The vector itself is `spawn::orchestrator`'s.

use serde::{Deserialize, Serialize};

use crate::agents::{AgentRecord, LiveStatus};
use crate::focus::FocusOutcome;
use crate::quota::normalise_path;
use crate::registry::ingestion::in_officina_dir;
use crate::spawn::{ORCHESTRATOR_NAME, OfficinaDir};

/// True when `cwd` is exactly `<officina_home>/orchestrator/workdir`, both
/// compared as [`normalise_path`] does (AD-43 as amended, D-166): a
/// session elsewhere under `orchestrator/` is not the orchestrator's.
///
/// ```
/// use officina_core::orders::orchestrator::in_workdir;
/// let home = r"C:\Users\user01\.officina";
/// assert!(in_workdir(r"c:/users/USER01/.officina/orchestrator/workdir/", home));
/// assert!(!in_workdir(r"C:\Users\user01\.officina\orchestrator\proposals", home));
/// assert!(!in_workdir(r"C:\Users\user01\.officina\orchestrator\workdir\sub", home));
/// ```
pub fn in_workdir(cwd: &str, officina_home: &str) -> bool {
    in_officina_dir(cwd, officina_home, OfficinaDir::OrchestratorWorkdir)
}

/// True when `cwd` lies under `<officina_home>/orchestrator/`, its folder
/// included, compared as [`normalise_path`] does: where doctor looks for
/// sessions that are not the orchestrator (D-166).
///
/// ```
/// use officina_core::orders::orchestrator::under_orchestrator_dir;
/// assert!(under_orchestrator_dir("C:/home/orchestrator/notes", "C:/home"));
/// assert!(under_orchestrator_dir("C:/home/Orchestrator", "C:/home"));
/// assert!(!under_orchestrator_dir("C:/home/orchestrator-notes", "C:/home"));
/// ```
pub fn under_orchestrator_dir(cwd: &str, officina_home: &str) -> bool {
    let dir = format!(
        "{}/{}",
        normalise_path(officina_home),
        crate::orders::ORCHESTRATOR_DIR
    );
    let cwd = normalise_path(cwd);
    cwd == dir
        || cwd
            .strip_prefix(&dir)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// True when `record` is the orchestrator (D-166): named
/// [`ORCHESTRATOR_NAME`] and working in [`in_workdir`]'s folder. Either one
/// alone is an ordinary session.
///
/// ```
/// use officina_core::agents::parse_agents;
/// use officina_core::orders::orchestrator::is_orchestrator;
/// let listed = parse_agents(r#"[
///   {"name":"officina-orchestrator","cwd":"C:/home/orchestrator/workdir"},
///   {"name":"officina-orchestrator","cwd":"C:/work/api"},
///   {"name":"notes","cwd":"C:/home/orchestrator/workdir"}
/// ]"#).unwrap();
/// let found: Vec<bool> = listed.iter().map(|r| is_orchestrator(r, "C:/home")).collect();
/// assert_eq!(found, [true, false, false]);
/// ```
pub fn is_orchestrator(record: &AgentRecord, officina_home: &str) -> bool {
    record.name.as_deref() == Some(ORCHESTRATOR_NAME)
        && record
            .cwd
            .as_deref()
            .is_some_and(|cwd| in_workdir(cwd, officina_home))
}

/// The orchestrator as the board and the Orchestrator view show it (AD-89,
/// P-26): its agents entry's pid, live status and start. Never a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OrchestratorPresence {
    pub pid: Option<u32>,
    /// Unknown when the entry carries none (E1).
    pub status: Option<LiveStatus>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub started_at_ms: Option<i64>,
}

impl OrchestratorPresence {
    /// The presence `record` stands for.
    pub fn of(record: &AgentRecord) -> OrchestratorPresence {
        OrchestratorPresence {
            pid: record.pid,
            status: record.status,
            started_at_ms: record.started_at_ms,
        }
    }
}

/// The orchestrator among `agents`, the first entry [`is_orchestrator`]
/// accepts; `None` while none is listed.
pub fn presence(agents: &[AgentRecord], officina_home: &str) -> Option<OrchestratorPresence> {
    agents
        .iter()
        .find(|record| is_orchestrator(record, officina_home))
        .map(OrchestratorPresence::of)
}

crate::closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why a Start is refused before anything starts (AD-89, D-164,
    /// D-167). Its code is the `orchestrator_start` row's error.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum StartRefusal {
        /// No `claude` was found.
        ClaudeNotFound,
        /// `claude` is no native executable (a shim, D-562).
        ClaudeNotExe,
        /// `<home>/orchestrator/workdir` holds something.
        WorkdirNotEmpty,
        /// No `officina.exe` beside the window's executable (D-144).
        OfficinaExeMissing,
        /// The registry cannot record the start, so its briefing cursor
        /// would be lost (D-144).
        RegistryUnavailable,
        /// A console of its own opens on Windows only.
        Unsupported,
    }
}

/// How a Start was answered (AD-89, AD-92).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "outcome", content = "detail", rename_all = "snake_case")]
pub enum StartAnswer {
    /// A new session started in its console; it is never a resume.
    Started,
    /// One runs already: what bringing its console forward did. Several
    /// windows titled with its name are candidates; none is "already
    /// running" without a window.
    AlreadyRunning(FocusOutcome),
    Refused(StartRefusal),
    /// `claude` did not start: why.
    Failed(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::parse_agents;

    #[test]
    fn presence_takes_the_name_and_the_exact_folder() {
        let home = r"C:\Users\user01\.officina";
        let listed = parse_agents(
            r#"[
              {"name":"notes","cwd":"C:/Users/user01/.officina/orchestrator/notes","pid":3},
              {"name":"officina-orchestrator","cwd":"c:\\users\\user01\\.officina\\orchestrator\\workdir","pid":7,"status":"waiting","startedAt":5}
            ]"#,
        )
        .unwrap();
        assert_eq!(
            presence(&listed, home),
            Some(OrchestratorPresence {
                pid: Some(7),
                status: Some(LiveStatus::Waiting),
                started_at_ms: Some(5),
            })
        );
        assert_eq!(presence(&listed[..1], home), None);
        assert!(under_orchestrator_dir(
            listed[0].cwd.as_deref().unwrap(),
            home
        ));
    }

    #[test]
    fn every_refusal_has_its_code() {
        for refusal in StartRefusal::ALL {
            let answer = serde_json::to_value(StartAnswer::Refused(*refusal)).unwrap();
            assert_eq!(answer["outcome"], "refused");
            assert_eq!(answer["detail"], refusal.code());
        }
    }
}
