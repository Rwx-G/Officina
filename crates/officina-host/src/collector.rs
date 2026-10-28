//! Gathers every source into a [`Board`] (AD-2 to AD-5). The CLI calls it once;
//! the window drives it from its collector thread (see [`crate::watch`]).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use officina_core::agents::{AgentRecord, parse_agents};
use officina_core::board::{Board, BoardInput, ContractView, ProjectRule, Staleness, build_board};
use officina_core::contract::check_agents;
use officina_core::floor::{FloorFile, parse_floor, resolve_project_path};
use officina_core::quota::{QuotaFile, parse_quota_file};
use officina_core::registry::types::{RegistryReport, SubjectRef};
use serde::{Deserialize, Serialize};

use crate::claude;
use crate::files::{atomic_write, read_recent_events};
use crate::paths::{ENV_FLOOR, Paths, env_path, excerpts_disabled_by_env};
use crate::relay::approvals::board_approvals;
use crate::relay::credential::{CredentialStore, OwnedStore};
use crate::relay::home::RelayHome;
use crate::relay::known_projects::KnownProjectsKeeper;
use crate::relay::tree::{self, TreeResult};

/// How long the window trusts a project's tree fingerprint before it runs
/// `git` again for an approval card's dry run (D-170): boards are rebuilt on
/// every hook event, and three `git` spawns each time would be wasted.
pub const TREE_RECHECK: Duration = Duration::from_secs(10);

/// Why the collector cannot start at all.
#[derive(Debug, thiserror::Error)]
pub enum CollectorError {
    #[error("no home directory: set OFFICINA_HOME or USERPROFILE")]
    NoHome,
}

/// A floor file as loaded, with the problem that prevented using it.
#[derive(Debug, Clone, Default)]
pub struct LoadedFloor {
    pub path: Option<PathBuf>,
    pub file: Option<FloorFile>,
    pub projects: Vec<ProjectRule>,
    pub error: Option<String>,
}

/// Loads and resolves the floor file, if any.
pub fn load_floor(path: Option<&Path>) -> LoadedFloor {
    let Some(path) = path else {
        return LoadedFloor::default();
    };
    let loaded = |error: String| LoadedFloor {
        path: Some(path.to_path_buf()),
        error: Some(error),
        ..Default::default()
    };
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => return loaded(format!("{}: {e}", path.display())),
    };
    let file = match parse_floor(&content) {
        Ok(f) => f,
        Err(e) => return loaded(format!("{}: {e}", path.display())),
    };
    let floor_dir = path
        .parent()
        .map(|d| d.to_string_lossy().into_owned())
        .unwrap_or_default();
    let projects = file
        .projects
        .iter()
        .map(|p| ProjectRule {
            name: p.name.clone(),
            path: resolve_project_path(&floor_dir, &p.path),
            color: p.color,
        })
        .collect();
    LoadedFloor {
        path: Some(path.to_path_buf()),
        file: Some(file),
        projects,
        error: None,
    }
}

/// Contract results per Claude Code version, `state/contract.json` (AD-8).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractStore {
    pub schema: u32,
    pub versions: std::collections::BTreeMap<String, ContractRecord>,
}

/// One version's contract result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractRecord {
    pub checked_at: i64,
    pub held: bool,
    pub missing: Vec<String>,
    pub not_observed: Vec<String>,
}

/// Tree fingerprints of the projects with a pending dry run, by folder,
/// with when they were taken (D-170), each taken again once
/// [`TREE_RECHECK`] has passed. Interior: a board is built from `&self`.
#[derive(Debug, Default)]
struct TreeCache(RefCell<BTreeMap<PathBuf, (Instant, TreeResult)>>);

impl TreeCache {
    fn fingerprint(&self, dir: &Path) -> TreeResult {
        let mut trees = self.0.borrow_mut();
        if let Some((at, found)) = trees.get(dir)
            && at.elapsed() < TREE_RECHECK
        {
            return found.clone();
        }
        let found = tree::fingerprint(dir);
        trees.insert(dir.to_path_buf(), (Instant::now(), found.clone()));
        found
    }
}

/// The collector's state between two boards: every field private, so what
/// it reads and writes goes through its methods.
#[derive(Debug)]
pub struct Collector {
    /// The paths in use: the environment's, with the floor's `config_dir`.
    paths: Paths,
    /// The environment's paths, before the floor: a reloaded floor's
    /// `config_dir` applies to these.
    base_paths: Paths,
    floor: LoadedFloor,
    claude: Option<PathBuf>,
    relay: RelayHome,
    credentials: OwnedStore,
    agents: Option<Vec<AgentRecord>>,
    agents_raw: Option<Vec<serde_json::Value>>,
    stale: Option<Staleness>,
    contract: ContractView,
    trees: TreeCache,
    known_projects: KnownProjectsKeeper,
    /// The registry's state as its holder last reported it: the window's
    /// collector thread, or `board --json`'s read-only reader.
    registry: Option<RegistryReport>,
    /// The subject of each filed session, from the same holder (AD-54).
    subjects: BTreeMap<String, SubjectRef>,
}

impl Collector {
    /// Resolves paths, the floor and the `claude` binary; the relay reads
    /// the floor `OFFICINA_FLOOR` names, if any.
    pub fn new(floor_arg: Option<&Path>) -> Result<Collector, CollectorError> {
        let paths = Paths::from_env(floor_arg).ok_or(CollectorError::NoHome)?;
        let floor = load_floor(paths.floor_file.as_deref());
        let relay = RelayHome::new(paths.officina_home.clone(), env_path(ENV_FLOOR));
        Ok(Collector::assemble(
            paths,
            floor,
            claude::resolve_claude(),
            relay,
        ))
    }

    /// Builds a collector from already resolved parts: `paths` as the
    /// environment gives them, before the floor, whose `config_dir` is
    /// applied here; the relay home is the Officina home with no
    /// `OFFICINA_FLOOR`. Used by tests, which must not depend on the process
    /// environment.
    pub fn from_parts(paths: Paths, floor: LoadedFloor, claude: Option<PathBuf>) -> Collector {
        let relay = RelayHome::new(paths.officina_home.clone(), None);
        Collector::assemble(paths, floor, claude, relay)
    }

    fn assemble(
        paths: Paths,
        floor: LoadedFloor,
        claude: Option<PathBuf>,
        relay: RelayHome,
    ) -> Collector {
        let config_dir = floor.file.as_ref().and_then(|f| f.config_dir.clone());
        let applied = paths.clone().with_floor_config_dir(config_dir.as_deref());
        Collector {
            relay,
            credentials: OwnedStore::system(),
            paths: applied,
            base_paths: paths,
            floor,
            claude,
            agents: None,
            agents_raw: None,
            stale: None,
            contract: ContractView::default(),
            trees: TreeCache::default(),
            known_projects: KnownProjectsKeeper::default(),
            registry: None,
            subjects: BTreeMap::new(),
        }
    }

    /// Records the registry's state, which the next board reports
    /// (`Diagnostics.registry`, D-47).
    pub fn set_registry(&mut self, report: RegistryReport) {
        self.registry = Some(report);
    }

    /// Records the subject of each filed session, which the next board's
    /// cards show (`Card.subject`, AD-54); empty when the registry could
    /// not be read.
    pub fn set_subjects(&mut self, subjects: BTreeMap<String, SubjectRef>) {
        self.subjects = subjects;
    }

    /// True when no prompt or answer text may be kept: the environment or
    /// the window's floor says so (AD-8, AD-43).
    pub fn no_excerpts(&self) -> bool {
        excerpts_disabled_by_env() || self.floor.file.as_ref().is_some_and(|f| f.no_excerpts)
    }

    /// The collector checking credentials in `store` (D-41): the
    /// Credential Manager by default, a fake in tests.
    pub fn with_credential_store(mut self, store: OwnedStore) -> Collector {
        self.credentials = store;
        self
    }

    /// The paths in use: the environment's, with the floor's `config_dir`.
    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    /// The floor file as last loaded.
    pub fn floor(&self) -> &LoadedFloor {
        &self.floor
    }

    /// The `claude` binary, when one was found.
    pub fn claude(&self) -> Option<&Path> {
        self.claude.as_deref()
    }

    /// Where the relay's files are, and which floor it reads (AD-24, D-8).
    pub fn relay(&self) -> &RelayHome {
        &self.relay
    }

    /// Where the window checks the presence of the approved credentials
    /// (D-41).
    pub fn credential_store(&self) -> &dyn CredentialStore {
        self.credentials.0.as_ref()
    }

    /// Keeps `relay/known-projects.json` current (D-15); a write that fails
    /// is journaled once as a gate error, and returned while it stands.
    pub fn keep_known_projects(&mut self, now_ms: i64) -> Option<&str> {
        self.known_projects.keep(&self.relay, now_ms)
    }

    /// The floor file the window watches: the one configured, else
    /// `floor.toml` in the Officina home, which may not exist yet.
    pub fn watched_floor(&self) -> PathBuf {
        self.base_paths
            .floor_file
            .clone()
            .unwrap_or_else(|| self.base_paths.officina_home.join("floor.toml"))
    }

    /// Reads the floor again after it changed (AD-4, AD-35): its projects,
    /// name and colours reach the next board, and its `config_dir` moves the
    /// sessions dir and the quota file. The default floor (`floor.toml` in
    /// the home) counts when it exists, as at startup: created later it is
    /// picked up, deleted it leaves no floor; a floor named by `--floor` or
    /// `OFFICINA_FLOOR` that disappears is an error on the board.
    pub fn reload_floor(&mut self) {
        let default = self.base_paths.officina_home.join("floor.toml");
        let path = match &self.base_paths.floor_file {
            Some(named) if *named != default => Some(named.clone()),
            _ => Some(default).filter(|p| p.is_file()),
        };
        self.floor = load_floor(path.as_deref());
        let config_dir = self.floor.file.as_ref().and_then(|f| f.config_dir.clone());
        self.paths = self
            .base_paths
            .clone()
            .with_floor_config_dir(config_dir.as_deref());
    }

    /// The session list of the last successful `claude agents` call.
    pub fn agents(&self) -> Option<&[AgentRecord]> {
        self.agents.as_deref()
    }

    /// Runs `claude agents --json --all`; true when it answered. On failure
    /// the previous list is kept and the board is marked stale (FR-1): a
    /// failed call never empties the board.
    pub fn refresh_agents(&mut self, now_ms: i64) -> bool {
        let result = match &self.claude {
            None => Err(claude::ClaudeError::NotFound.to_string()),
            Some(bin) => claude::agents_json(bin, self.paths.claude_config_override.as_deref())
                .map_err(|e| e.to_string())
                .and_then(|out| {
                    let raw: Vec<serde_json::Value> = serde_json::from_str(out.trim())
                        .map_err(|e| format!("claude agents output is not a JSON array: {e}"))?;
                    let records = parse_agents(out.trim()).map_err(|e| e.to_string())?;
                    Ok((raw, records))
                }),
        };
        match result {
            Ok((raw, records)) => {
                self.agents = Some(records);
                self.agents_raw = Some(raw);
                self.stale = None;
                self.check_contract(now_ms);
                true
            }
            Err(error) => {
                let since_ms = self.stale.as_ref().map_or(now_ms, |s| s.since_ms);
                self.stale = Some(Staleness { since_ms, error });
                false
            }
        }
    }

    /// Reads the `claude` version, once per binary (a spawn), then re-runs the
    /// contract check against the last sample (FR-12).
    pub fn refresh_contract(&mut self, now_ms: i64) {
        let Some(bin) = &self.claude else {
            return;
        };
        self.contract.claude_version =
            claude::version(bin, self.paths.claude_config_override.as_deref()).ok();
        self.check_contract(now_ms);
    }

    /// Checks the last agents sample against the contract. An empty sample
    /// proves nothing, so it leaves the result unknown rather than "held".
    /// The per-version record is written only when the result changes.
    fn check_contract(&mut self, now_ms: i64) {
        let Some(raw) = self.agents_raw.as_ref().filter(|r| !r.is_empty()) else {
            return;
        };
        let check = check_agents(raw);
        self.contract.held = Some(check.held());
        self.contract.missing.clone_from(&check.missing);
        let Some(version) = self.contract.claude_version.clone() else {
            return;
        };
        let path = self.paths.state_dir().join("contract.json");
        let mut store: ContractStore = fs::read_to_string(&path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or(ContractStore {
                schema: 1,
                ..Default::default()
            });
        let record = ContractRecord {
            checked_at: now_ms,
            held: check.held(),
            missing: check.missing,
            not_observed: check.not_observed,
        };
        let unchanged = store.versions.get(&version).is_some_and(|r| {
            r.held == record.held
                && r.missing == record.missing
                && r.not_observed == record.not_observed
        });
        if unchanged {
            return;
        }
        store.versions.insert(version, record);
        if let Ok(bytes) = serde_json::to_vec_pretty(&store) {
            let _ = atomic_write(&path, &bytes);
        }
    }

    /// The quota file of this floor's account, if readable.
    pub fn read_quota(&self) -> Option<QuotaFile> {
        parse_quota_file(&fs::read_to_string(self.paths.quota_file()).ok()?)
    }

    /// Computes the board from the current state and the files on disk.
    pub fn board(&self, now_ms: i64) -> Board {
        let (events, corrupt_event_lines) = read_recent_events(&self.paths.events_dir(), now_ms);
        let floor_file = self.floor.file.as_ref();
        let input = BoardInput {
            agents: self.agents.clone(),
            stale: self.stale.clone(),
            events,
            corrupt_event_lines,
            quota: self.read_quota(),
            floor_name: floor_file.and_then(|f| f.name.clone()),
            projects: self.floor.projects.clone(),
            contract: self.contract.clone(),
            no_excerpts: self.no_excerpts(),
            approvals: board_approvals(&self.relay, now_ms, &mut |dir| self.trees.fingerprint(dir)),
            registry: self.registry.clone(),
            subjects: self.subjects.clone(),
        };
        let mut board = build_board(&input, now_ms);
        board.diagnostics.floor_error.clone_from(&self.floor.error);
        board
    }

    /// Looks up a card of the last board by key, for the focus action.
    pub fn agent_by_key(&self, key: &str) -> Option<&AgentRecord> {
        self.agents
            .iter()
            .flatten()
            .find(|r| officina_core::board::card_key(r).as_deref() == Some(key))
    }
}
