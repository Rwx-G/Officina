//! Where Officina reads and writes (AD-7). Every location has an environment
//! override, so tests never touch the real home.

use std::path::{Path, PathBuf};

/// Environment variable overriding the Officina home.
pub const ENV_HOME: &str = "OFFICINA_HOME";
/// Environment variable naming the floor file.
pub const ENV_FLOOR: &str = "OFFICINA_FLOOR";
/// Environment variable naming the `claude` binary.
pub const ENV_CLAUDE_BIN: &str = "OFFICINA_CLAUDE_BIN";
/// Environment variable dropping prompt and assistant excerpts.
pub const ENV_NO_EXCERPTS: &str = "OFFICINA_NO_EXCERPTS";
/// Claude Code's own config dir override (research notes E7).
pub const ENV_CLAUDE_CONFIG_DIR: &str = "CLAUDE_CONFIG_DIR";

/// The user's home directory: `USERPROFILE` on Windows, `HOME` elsewhere.
pub fn user_home() -> Option<PathBuf> {
    let primary = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(primary)
        .or_else(|| std::env::var_os("HOME"))
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Resolved locations for one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// `~/.officina` or `OFFICINA_HOME`.
    pub officina_home: PathBuf,
    /// The Claude config dir sessions of this floor use.
    pub claude_config_dir: PathBuf,
    /// Set when the config dir comes from `CLAUDE_CONFIG_DIR` or the floor:
    /// it must then be passed to every `claude` child, and it selects the
    /// quota account key.
    pub claude_config_override: Option<PathBuf>,
    /// The floor file to load, if one is configured or present.
    pub floor_file: Option<PathBuf>,
}

impl Paths {
    /// Resolves paths from the environment. `floor_arg` is `--floor <path>`.
    /// Returns `None` when no home directory can be determined and no
    /// override is set: there is nowhere sensible to write.
    pub fn from_env(floor_arg: Option<&Path>) -> Option<Paths> {
        let home = user_home();
        let officina_home =
            env_path(ENV_HOME).or_else(|| home.as_ref().map(|h| h.join(".officina")))?;
        let default_config = home.as_ref().map(|h| h.join(".claude"));
        // A CLAUDE_CONFIG_DIR that names the default dir is no override: the
        // statusline of a session that sets it and the window started without
        // it must agree on the quota account key.
        let claude_config_override = env_path(ENV_CLAUDE_CONFIG_DIR).filter(|dir| {
            default_config.as_ref().is_none_or(|d| {
                officina_core::quota::normalise_path(&dir.to_string_lossy())
                    != officina_core::quota::normalise_path(&d.to_string_lossy())
            })
        });
        let claude_config_dir = claude_config_override.clone().or(default_config)?;
        let default_floor = officina_home.join("floor.toml");
        let floor_file = floor_arg
            .map(Path::to_path_buf)
            .or_else(|| env_path(ENV_FLOOR))
            .or_else(|| default_floor.is_file().then_some(default_floor));
        Some(Paths {
            officina_home,
            claude_config_dir,
            claude_config_override,
            floor_file,
        })
    }

    /// Applies a floor's `config_dir`, which wins over the environment.
    pub fn with_floor_config_dir(mut self, config_dir: Option<&str>) -> Paths {
        if let Some(dir) = config_dir {
            self.claude_config_dir = PathBuf::from(dir);
            self.claude_config_override = Some(PathBuf::from(dir));
        }
        self
    }

    /// Directory of the event buffer files.
    pub fn events_dir(&self) -> PathBuf {
        self.officina_home.join("events")
    }

    /// Directory of the quota files.
    pub fn quota_dir(&self) -> PathBuf {
        self.officina_home.join("quota")
    }

    /// Directory of the state files (contract results, focus outcomes).
    pub fn state_dir(&self) -> PathBuf {
        self.officina_home.join("state")
    }

    /// The Claude sessions directory, watched by modification time only.
    pub fn claude_sessions_dir(&self) -> PathBuf {
        self.claude_config_dir.join("sessions")
    }

    /// The account key of this floor's quota file.
    pub fn account_key(&self) -> String {
        officina_core::quota::account_key(
            self.claude_config_override
                .as_deref()
                .map(|p| p.to_string_lossy())
                .as_deref(),
        )
    }

    /// The quota file of this floor's account.
    pub fn quota_file(&self) -> PathBuf {
        self.quota_dir()
            .join(format!("{}.json", self.account_key()))
    }
}

/// True when prompt and assistant excerpts are disabled by the environment.
pub fn excerpts_disabled_by_env() -> bool {
    std::env::var_os(ENV_NO_EXCERPTS).is_some_and(|v| v == "1")
}
