//! The hook-program inventory on the host (AD-118, D-211): the settings
//! files and `.mcp.json` read (mounted ones from the project folder,
//! tracked ones at the base commit through the workshop's `git`), the
//! tracked scripts' presence, mode and first line at the base commit, and,
//! for the window, the `Inventory` container run attached with the list on
//! stdin, its results recorded per image and project.

use std::io::Read;
use std::path::{Path, PathBuf};

use officina_core::workshop::inventory::{
    InventoryCommand, InventoryError, MAX_INPUT_BYTES, SettingsFile, SettingsSource, extract,
    first_line, under_mount,
};

/// The settings files of a project's `.claude`, by source (D-211).
pub const SETTINGS_FILES: &[(&str, SettingsSource, SettingsSource)] = &[
    (
        ".claude/settings.json",
        SettingsSource::MountedSettings,
        SettingsSource::TrackedSettings,
    ),
    (
        ".claude/settings.local.json",
        SettingsSource::MountedLocal,
        SettingsSource::TrackedLocal,
    ),
];
/// The project's MCP servers file (AD-118).
pub const MCP_FILE: &str = ".mcp.json";
/// The mode git records for an executable file.
const EXECUTABLE_MODE: &str = "100755";

/// One workshop project as the inventory needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryProject {
    pub name: String,
    /// The project folder on the host.
    pub path: PathBuf,
    /// The mounts, relative to the project (AD-101).
    pub mounts: Vec<String>,
}

/// The workshop's `git` and what each call runs with (AD-111).
#[derive(Debug, Clone)]
pub struct WorkshopGit {
    pub git: PathBuf,
    pub hooks_dir: PathBuf,
    pub base_env: Vec<(String, String)>,
}

impl WorkshopGit {
    fn run(&self, dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
        let ran =
            crate::workshop::git::run(&self.git, dir, args, &self.base_env, &self.hooks_dir, None)
                .map_err(|e| e.to_string())?;
        if !ran.status.success() || ran.stdout_over {
            return Err(format!(
                "git {}: {}",
                args.first().copied().unwrap_or_default(),
                String::from_utf8_lossy(&ran.stderr).trim()
            ));
        }
        Ok(ran.stdout)
    }

    /// The commit `refs/heads/<branch>` names in `dir`, if any.
    pub fn branch_commit(&self, dir: &Path, branch: &str) -> Option<String> {
        let spec = format!("refs/heads/{branch}^{{commit}}");
        let out = self
            .run(dir, &["rev-parse", "--verify", "--quiet", &spec])
            .ok()?;
        let id = String::from_utf8_lossy(&out).trim().to_owned();
        (!id.is_empty() && id.bytes().all(|b| b.is_ascii_hexdigit())).then_some(id)
    }

    /// The mode and blob id of `path` at `commit`, when it is a blob there.
    fn blob(
        &self,
        dir: &Path,
        commit: &str,
        path: &str,
    ) -> Result<Option<(String, String)>, String> {
        let out = self.run(dir, &["ls-tree", "-z", "--full-tree", commit, "--", path])?;
        let text = String::from_utf8_lossy(&out);
        let Some(entry) = text.split('\0').find(|e| !e.is_empty()) else {
            return Ok(None);
        };
        let (meta, name) = entry.split_once('\t').unwrap_or((entry, ""));
        let fields: Vec<&str> = meta.split(' ').collect();
        match fields.as_slice() {
            [mode, "blob", id] if name == path && id.bytes().all(|b| b.is_ascii_hexdigit()) => {
                Ok(Some(((*mode).to_owned(), (*id).to_owned())))
            }
            _ => Ok(None),
        }
    }

    /// A blob's bytes, fed by id only (D-219's rule).
    fn cat(&self, dir: &Path, blob: &str) -> Result<Vec<u8>, String> {
        self.run(dir, &["cat-file", "blob", blob])
    }
}

/// Why the inventory's input could not be gathered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GatherError {
    #[error(transparent)]
    Inventory(#[from] InventoryError),
    #[error("{0}")]
    Git(String),
}

/// A mounted file, read to one byte past the bound so `extract` refuses an
/// oversized one rather than reading it cut; `None` when absent.
fn read_bounded(path: &Path) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    let limit = u64::try_from(MAX_INPUT_BYTES).unwrap_or(u64::MAX) + 1;
    file.take(limit).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// The command list of `project` at `base_commit` (AD-118, D-211): the
/// settings files of a mounted `.claude` from the project folder, the
/// tracked ones from the base commit when they are not under a mount, the
/// base commit's `.mcp.json`, and each tracked script's presence, mode,
/// blob and first line there.
pub fn gather(
    project: &InventoryProject,
    base_commit: &str,
    git: &WorkshopGit,
) -> Result<Vec<InventoryCommand>, GatherError> {
    let dir = &project.path;
    let mut settings = Vec::new();
    for (path, mounted, tracked) in SETTINGS_FILES {
        if under_mount(path, &project.mounts) {
            if let Some(bytes) = read_bounded(&dir.join(path)) {
                settings.push(SettingsFile {
                    source: *mounted,
                    bytes,
                });
            }
        } else if let Some((_, blob)) =
            git.blob(dir, base_commit, path).map_err(GatherError::Git)?
        {
            settings.push(SettingsFile {
                source: *tracked,
                bytes: git.cat(dir, &blob).map_err(GatherError::Git)?,
            });
        }
    }
    let mcp = match git
        .blob(dir, base_commit, MCP_FILE)
        .map_err(GatherError::Git)?
    {
        Some((_, blob)) => Some(git.cat(dir, &blob).map_err(GatherError::Git)?),
        None => None,
    };
    let mut commands = extract(&settings, mcp.as_deref(), &project.mounts)?;
    for tracked in commands.iter_mut().filter_map(|c| c.tracked.as_mut()) {
        if let Some((mode, blob)) = git
            .blob(dir, base_commit, &tracked.path)
            .map_err(GatherError::Git)?
        {
            let bytes = git.cat(dir, &blob).map_err(GatherError::Git)?;
            tracked.present = true;
            tracked.executable = mode == EXECUTABLE_MODE;
            tracked.first_line = Some(first_line(&bytes));
            tracked.blob = Some(blob);
        }
    }
    Ok(commands)
}

#[cfg(feature = "window-authority")]
pub use window::*;

#[cfg(feature = "window-authority")]
mod window {
    use std::path::{Path, PathBuf};

    use officina_core::workshop::engine::EngineCall;
    use officina_core::workshop::inventory::{
        INVENTORY_INPUT_SCHEMA, InventoryCommand, InventoryInput, Resolved, StoredInventory,
        inventory_key, parse_results,
    };
    use officina_core::workshop::names::{ObjectName, ROLE_LABEL, base_labels};
    use officina_core::workshop::vector::{self, ContainerRole, WORK_DIR};

    use super::{InventoryProject, WorkshopGit, gather};
    use crate::registry::workshop::InventoryRecord;
    use crate::workshop::engine::{ResolvedDocker, execute};

    /// The engine and the floor an inventory runs with.
    #[derive(Debug, Clone)]
    pub struct InventoryEngine {
        pub docker: ResolvedDocker,
        pub base_env: Vec<(String, String)>,
        pub home: PathBuf,
        pub floor_key: String,
    }

    /// Why an inventory was not taken; nothing is recorded, so the next
    /// launch runs it again.
    #[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
    pub enum InventoryFailure {
        #[error("the workshop is stopped")]
        Stopped,
        #[error("the inventory's input: {0}")]
        Gather(String),
        #[error("the inventory container: {0}")]
        Engine(String),
        #[error("the inventory's output: {0}")]
        Results(String),
    }

    /// The inventory container's name for `project` on this floor.
    pub fn container_name(floor_key: &str, project: &str) -> Option<ObjectName> {
        ObjectName::parse(&format!("officina-inventory-{floor_key}-{project}"))
    }

    /// Runs the `Inventory` vector (AD-118, D-205, D-211): no network, the
    /// mounts read-only, attached with the list on stdin, 60 s; the
    /// container removed before (a crash's leftover) and after. Refused
    /// while the workshop's stop flag is set.
    pub fn run(
        engine: &InventoryEngine,
        image_id: &str,
        project: &InventoryProject,
        commands: &[InventoryCommand],
    ) -> Result<Vec<Resolved>, InventoryFailure> {
        if crate::workshop::workshop_stopped(&engine.home) {
            return Err(InventoryFailure::Stopped);
        }
        let name = container_name(&engine.floor_key, &project.name)
            .ok_or_else(|| InventoryFailure::Engine("the container's name".to_owned()))?;
        let mut labels = base_labels(&engine.floor_key, crate::workshop::build::VERSION);
        labels.insert(
            ROLE_LABEL.to_owned(),
            ContainerRole::Inventory.code().to_owned(),
        );
        let sources: Vec<String> = project
            .mounts
            .iter()
            .map(|m| project.path.join(m).to_string_lossy().into_owned())
            .collect();
        let container = vector::inventory(&name, &labels, image_id, &project.name, &sources);
        let input = InventoryInput {
            schema: INVENTORY_INPUT_SCHEMA,
            project_dir: format!("{WORK_DIR}/{}", project.name),
            mounts: project.mounts.clone(),
            commands: commands.to_vec(),
        };
        let stdin =
            serde_json::to_vec(&input).map_err(|e| InventoryFailure::Engine(e.to_string()))?;
        let call = |call: &EngineCall, stdin: Option<Vec<u8>>| {
            execute(&engine.docker, call, &engine.base_env, &engine.home, stdin)
                .map_err(|e| InventoryFailure::Engine(e.to_string()))
        };
        let remove = || {
            let _ = call(&EngineCall::rm(std::slice::from_ref(&name)), None);
        };
        remove();
        let created = call(&EngineCall::create(&container), None)?;
        if !created.succeeded() {
            remove();
            return Err(InventoryFailure::Engine(format!(
                "create: {}",
                created.stderr.trim()
            )));
        }
        let started = call(
            &EngineCall::start_attached(&name, ContainerRole::Inventory, 0, true),
            Some(stdin),
        );
        remove();
        let started = started?;
        if !started.succeeded() || started.stdout_over {
            return Err(InventoryFailure::Engine(format!(
                "start: exit {:?}: {}",
                started.code,
                started.stderr.trim()
            )));
        }
        parse_results(&String::from_utf8_lossy(&started.stdout), commands.len())
            .map_err(|e| InventoryFailure::Results(e.to_string()))
    }

    /// Takes `project`'s inventory on `image` at `base_commit` and returns
    /// its record for the collector to store (AD-38).
    pub fn take(
        engine: &InventoryEngine,
        git: &WorkshopGit,
        image: (&str, &str),
        project: &InventoryProject,
        base_commit: &str,
        now_ms: i64,
    ) -> Result<InventoryRecord, InventoryFailure> {
        let commands = gather(project, base_commit, git)
            .map_err(|e| InventoryFailure::Gather(e.to_string()))?;
        record_of(engine, image, project, commands, now_ms)
    }

    fn record_of(
        engine: &InventoryEngine,
        (image, image_id): (&str, &str),
        project: &InventoryProject,
        commands: Vec<InventoryCommand>,
        now_ms: i64,
    ) -> Result<InventoryRecord, InventoryFailure> {
        let key = inventory_key(&commands, image_id);
        let results = run(engine, image_id, project, &commands)?;
        let stored = StoredInventory { commands, results };
        Ok(InventoryRecord {
            image: image.to_owned(),
            project: project.name.clone(),
            image_id: image_id.to_owned(),
            inventory_key: key,
            inventory_json: serde_json::to_string(&stored)
                .map_err(|e| InventoryFailure::Results(e.to_string()))?,
            inventory_at_ms: now_ms,
        })
    }

    /// What a launch rests on (AD-118, D-247): the stored inventory when
    /// its key equals the one computed now from the files at the base
    /// commit and the image id, else a fresh run, whose record the caller
    /// hands to the collector.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct LaunchInventory {
        pub inventory: StoredInventory,
        /// Set when the inventory ran again.
        pub fresh: Option<InventoryRecord>,
    }

    /// The inventory a launch judges (AD-118): reused while its key holds,
    /// run again on thread `workshop` when a hook, script, `.mcp.json` or
    /// the image changed.
    pub fn for_launch(
        engine: &InventoryEngine,
        git: &WorkshopGit,
        image: (&str, &str),
        project: &InventoryProject,
        base_commit: &str,
        stored: Option<&InventoryRecord>,
        now_ms: i64,
    ) -> Result<LaunchInventory, InventoryFailure> {
        let commands = gather(project, base_commit, git)
            .map_err(|e| InventoryFailure::Gather(e.to_string()))?;
        let key = inventory_key(&commands, image.1);
        if let Some(record) = stored.filter(|r| r.inventory_key == key && r.image_id == image.1)
            && let Ok(inventory) = serde_json::from_str::<StoredInventory>(&record.inventory_json)
        {
            return Ok(LaunchInventory {
                inventory,
                fresh: None,
            });
        }
        let record = record_of(engine, image, project, commands, now_ms)?;
        let inventory = serde_json::from_str(&record.inventory_json)
            .map_err(|e| InventoryFailure::Results(e.to_string()))?;
        Ok(LaunchInventory {
            inventory,
            fresh: Some(record),
        })
    }

    /// The workshop's `git` as an inventory runs it, when it resolves and
    /// its empty hooks folder can be made (AD-111).
    pub fn workshop_git(home: &Path, base_env: &[(String, String)]) -> Option<WorkshopGit> {
        let (git, _) = crate::workshop::git::resolve().ok()?;
        let hooks_dir = crate::workshop::git::ensure_empty_hooks(home).ok()?;
        Some(WorkshopGit {
            git,
            hooks_dir,
            base_env: base_env.to_vec(),
        })
    }
}
