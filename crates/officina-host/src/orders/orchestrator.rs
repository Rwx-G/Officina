//! The orchestrator's lifecycle in the window (AD-89, FR-10, FR-14, D-144,
//! D-164, D-166, D-167). A Start is decided on the collector thread: the
//! window creates `<home>/orchestrator/workdir` and `proposals/` (never
//! deleting them), says whether one runs already, checks the refusals, and
//! hands the vector to the window's command, which spawns it on its own
//! blocking thread and sends back the pid and its creation time. The
//! collector then records the `orchestrator_start` row and the
//! `orchestrator_started` line, and keeps the pid, as it does a refusal or
//! a failure. A Start is always a new session, never a resume.

use std::path::Path;
use std::sync::mpsc::Sender;

use officina_core::board::Board;
use officina_core::focus::FocusOutcome;
use officina_core::ipc::IpcError;
use officina_core::orders::orchestrator::{StartAnswer, StartRefusal};
use officina_core::registry::types::OrchestratorStartOutcome;
use officina_core::relay::journal::{JournalEntry, OrchestratorStarted};
use officina_core::spawn::{ClaudeVector, McpServerInputs};

use crate::collector::Collector;
pub use crate::files::holds_anything;
pub use crate::paths::{
    officina_exe_name, orchestrator_proposals_dir as proposals_dir, orchestrator_workdir as workdir,
};
use crate::registry::orders::OrchestratorStartRecord;
use crate::registry::window::WindowRegistry;
use crate::relay::home::load_relay_floor;
use crate::spawn::{self, BaseEnv, ResolvedClaude, SpawnError, SpawnOutcome};
use crate::watch::{AUTHOR_WAIT, AuthorCommand, CollectorMsg, ask};

/// The pid a spawned Start kept, with its creation time in Unix
/// milliseconds, which tells it from a later process given the same pid
/// (D-144).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeptPid {
    pub pid: u32,
    pub created_at_ms: i64,
}

impl KeptPid {
    /// True while that very process runs (AD-89).
    pub fn alive(self) -> bool {
        #[cfg(windows)]
        {
            crate::platform::windows::process_alive(self.pid, self.created_at_ms)
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
}

/// What the collector keeps of the orchestrator between two Starts: the
/// last kept pid, read back from the registry at window start (D-144), and
/// whether a spawn is out, so two quick clicks start one session.
#[derive(Debug, Default)]
pub struct OrchestratorKeeper {
    kept: Option<KeptPid>,
    starting: bool,
}

impl OrchestratorKeeper {
    /// The keeper of a window that starts with `kept`, the pid the newest
    /// spawned row holds.
    pub fn new(kept: Option<(u32, i64)>) -> OrchestratorKeeper {
        OrchestratorKeeper {
            kept: kept.map(|(pid, created_at_ms)| KeptPid { pid, created_at_ms }),
            starting: false,
        }
    }

    /// True when a Start must not spawn (D-773): an agents entry shows the
    /// orchestrator on `board`, a spawn is out, or the kept pid still runs,
    /// which covers a session not listed yet, its trust dialog open.
    fn running(&self, board: Option<&Board>) -> bool {
        board.is_some_and(|b| b.orchestrator.is_some())
            || self.starting
            || self.kept.is_some_and(KeptPid::alive)
    }
}

/// What a Start hands the window's command: the vector, the `claude` and
/// base environment the collector holds, and the start's time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartTicket {
    pub vector: ClaudeVector,
    pub claude: ResolvedClaude,
    pub base_env: BaseEnv,
    pub started_at_ms: i64,
}

/// The collector's answer to a Start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartGrant {
    Spawn(Box<StartTicket>),
    /// One runs already: the command brings its console forward.
    Running,
    Refused(StartRefusal),
}

/// How a granted spawn ended, as the command sends it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartReport {
    pub started_at_ms: i64,
    pub argv: Vec<String>,
    pub ended: StartEnded,
}

/// A spawn's end: the pid and its creation time, or why it failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartEnded {
    Spawned {
        pid: u32,
        created_at_ms: Option<i64>,
    },
    Failed(String),
}

impl StartReport {
    /// The report of `ticket`'s spawn, which `ran` answered.
    pub fn of(ticket: &StartTicket, ran: Result<SpawnOutcome, SpawnError>) -> StartReport {
        let ended = match ran {
            Ok(SpawnOutcome::Started { pid, created }) => StartEnded::Spawned {
                pid,
                created_at_ms: created.map(officina_core::dates::filetime_ms),
            },
            Ok(SpawnOutcome::Captured(_)) => {
                StartEnded::Failed("claude was captured, not started".to_owned())
            }
            Err(error) => StartEnded::Failed(error.cause().to_string()),
        };
        StartReport {
            started_at_ms: ticket.started_at_ms,
            argv: ticket.vector.args().to_vec(),
            ended,
        }
    }

    /// What the author is told.
    pub fn answer(&self) -> StartAnswer {
        match &self.ended {
            StartEnded::Spawned { .. } => StartAnswer::Started,
            StartEnded::Failed(error) => StartAnswer::Failed(error.clone()),
        }
    }
}

/// Records one start: its row and its line (AD-89, D-144). A registry or
/// journal that cannot be written is not the author's to handle here: the
/// registry's own report on the board says it, and the start stands.
fn record(
    collector: &Collector,
    registry: &mut WindowRegistry,
    row: &OrchestratorStartRecord,
    argv: Vec<String>,
    now_ms: i64,
) {
    let _ = registry.record_orchestrator_start(row);
    let line = OrchestratorStarted {
        outcome: row.outcome,
        pid: row.pid,
        argv,
        error: row.error.map(str::to_owned),
    };
    let _ = crate::relay::journal::record(
        collector.relay(),
        &[JournalEntry::OrchestratorStarted(line)],
        now_ms,
    );
}

/// The refusal of a Start in this window, in AD-89's order, or `None`.
fn refusal(
    collector: &Collector,
    registry: &WindowRegistry,
    exe_dir: &Path,
) -> Option<StartRefusal> {
    let home = &collector.paths().officina_home;
    let refused = match collector.resolved_claude() {
        None => StartRefusal::ClaudeNotFound,
        Some(claude) if !claude.is_native() => StartRefusal::ClaudeNotExe,
        Some(_) if holds_anything(&workdir(home)) => StartRefusal::WorkdirNotEmpty,
        Some(_) if !exe_dir.join(officina_exe_name()).is_file() => StartRefusal::OfficinaExeMissing,
        Some(_) if !cfg!(windows) => StartRefusal::Unsupported,
        Some(_) if registry.writer().is_none() => StartRefusal::RegistryUnavailable,
        Some(_) => return None,
    };
    Some(refused)
}

/// A Start on the collector thread (AD-89): the two folders created, then
/// running, refused (its row and line recorded), or the vector to spawn.
/// `exe_dir` is the window's executable directory (D-167).
pub(crate) fn grant(
    keeper: &mut OrchestratorKeeper,
    collector: &Collector,
    registry: &mut WindowRegistry,
    board: Option<&Board>,
    exe_dir: &Path,
    now_ms: i64,
) -> Result<StartGrant, IpcError> {
    let home = collector.paths().officina_home.clone();
    for dir in [workdir(&home), proposals_dir(&home)] {
        std::fs::create_dir_all(&dir).map_err(|e| {
            IpcError::internal(format!(
                "the orchestrator's folder {} could not be created: {e}",
                dir.display()
            ))
        })?;
    }
    if keeper.running(board) {
        return Ok(StartGrant::Running);
    }
    if let Some(refused) = refusal(collector, registry, exe_dir) {
        let row = OrchestratorStartRecord {
            started_at_ms: now_ms,
            outcome: OrchestratorStartOutcome::Refused,
            pid: None,
            pid_created_at_ms: None,
            error: Some(refused.code()),
        };
        record(collector, registry, &row, Vec::new(), now_ms);
        return Ok(StartGrant::Refused(refused));
    }
    let Some(claude) = collector.resolved_claude().cloned() else {
        return Ok(StartGrant::Refused(StartRefusal::ClaudeNotFound));
    };
    let relay = collector.relay();
    let model = load_relay_floor(&relay.relay_floor(&relay.read_approved()))
        .ok()
        .and_then(|floor| floor.catalogue.orchestrator_model);
    let home_text = home.to_string_lossy();
    let exe = exe_dir.join(officina_exe_name());
    let exe_text = exe.to_string_lossy();
    let floor = collector
        .floor()
        .path
        .as_deref()
        .map(|path| path.to_string_lossy().into_owned());
    let claude_text = claude.path().to_string_lossy();
    let config_dir = collector
        .paths()
        .claude_config_override
        .as_deref()
        .map(|dir| dir.to_string_lossy().into_owned());
    let server = McpServerInputs {
        officina_exe: &exe_text,
        officina_home: &home_text,
        floor: floor.as_deref(),
        no_excerpts: collector.no_excerpts(),
        claude_bin: &claude_text,
    };
    let vector = officina_core::spawn::orchestrator(&server, model, config_dir.as_deref());
    keeper.starting = true;
    Ok(StartGrant::Spawn(Box::new(StartTicket {
        vector,
        claude: claude.clone(),
        base_env: collector.base_env().clone(),
        started_at_ms: now_ms,
    })))
}

/// Records how a granted spawn ended (AD-89, D-144): its row with the pid
/// and its creation time, its line, and the pid kept for the next Start.
pub(crate) fn record_start(
    keeper: &mut OrchestratorKeeper,
    collector: &Collector,
    registry: &mut WindowRegistry,
    report: &StartReport,
    now_ms: i64,
) {
    keeper.starting = false;
    let row = match &report.ended {
        StartEnded::Spawned { pid, created_at_ms } => {
            keeper.kept = created_at_ms.map(|created_at_ms| KeptPid {
                pid: *pid,
                created_at_ms,
            });
            OrchestratorStartRecord {
                started_at_ms: report.started_at_ms,
                outcome: OrchestratorStartOutcome::Spawned,
                pid: Some(*pid),
                pid_created_at_ms: *created_at_ms,
                error: None,
            }
        }
        StartEnded::Failed(error) => OrchestratorStartRecord {
            started_at_ms: report.started_at_ms,
            outcome: OrchestratorStartOutcome::Failed,
            pid: None,
            pid_created_at_ms: None,
            error: Some(error.as_str()),
        },
    };
    record(collector, registry, &row, report.argv.clone(), now_ms);
}

/// Brings the orchestrator's console to the front by its title (AD-89).
fn bring_forward() -> FocusOutcome {
    #[cfg(windows)]
    {
        crate::platform::windows::focus_window_by_title(officina_core::spawn::ORCHESTRATOR_NAME)
    }
    #[cfg(not(windows))]
    {
        FocusOutcome::Unsupported
    }
}

/// A click on Start (AD-89, AD-92): the collector decides and records, this
/// thread spawns or brings the console forward, and the spawn's end goes
/// back to the collector, which records it whenever it arrives. The
/// window's command calls it in `spawn_blocking` with its executable
/// directory (D-167); the collector never waits on `claude`.
pub fn start_orchestrator(
    collector: &Sender<CollectorMsg>,
    exe_dir: &Path,
) -> Result<StartAnswer, IpcError> {
    let command = AuthorCommand::StartOrchestrator {
        exe_dir: exe_dir.to_path_buf(),
    };
    let ticket = match ask(collector, command, AUTHOR_WAIT)?.into_start()? {
        StartGrant::Refused(refused) => return Ok(StartAnswer::Refused(refused)),
        StartGrant::Running => return Ok(StartAnswer::AlreadyRunning(bring_forward())),
        StartGrant::Spawn(ticket) => ticket,
    };
    let ran = spawn::execute(&ticket.claude, &ticket.vector, &ticket.base_env);
    let report = StartReport::of(&ticket, ran);
    let answer = report.answer();
    // Recorded whenever the collector reaches it, like a launch's outcome.
    let _ = ask(
        collector,
        AuthorCommand::OrchestratorStarted(Box::new(report)),
        AUTHOR_WAIT,
    );
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn a_spawn_ends_as_its_outcome_says() {
        let ticket = StartTicket {
            vector: officina_core::spawn::orchestrator(
                &McpServerInputs {
                    officina_exe: "C:/path/to/officina.exe",
                    officina_home: "C:/path/to/home",
                    floor: None,
                    no_excerpts: false,
                    claude_bin: "C:/path/to/claude.exe",
                },
                None,
                None,
            ),
            claude: ResolvedClaude::new(PathBuf::from("C:/path/to/claude.exe")),
            base_env: BaseEnv::default(),
            started_at_ms: 5,
        };
        let spawned = StartReport::of(
            &ticket,
            Ok(SpawnOutcome::Started {
                pid: 7,
                created: Some(116_444_736_000_010_000),
            }),
        );
        assert_eq!(
            spawned.ended,
            StartEnded::Spawned {
                pid: 7,
                created_at_ms: Some(1)
            }
        );
        assert_eq!(spawned.answer(), StartAnswer::Started);
        assert_eq!(spawned.argv, ticket.vector.args());
        let failed = StartReport::of(&ticket, Err(SpawnError::Spawn("denied".into())));
        assert!(matches!(failed.answer(), StartAnswer::Failed(e) if e.contains("denied")));
    }

    #[test]
    fn a_folder_that_cannot_be_listed_holds_something() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!holds_anything(dir.path()));
        std::fs::write(dir.path().join("x"), b"").unwrap();
        assert!(holds_anything(dir.path()));
        assert!(holds_anything(&dir.path().join("missing")));
    }
}
