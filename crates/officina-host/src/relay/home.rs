//! The relay's files under the Officina home (AD-25, D-15), the approved
//! catalogue's states, and which floor the relay reads (AD-24).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use officina_core::floor::FloorError;
use officina_core::relay::approvals::OUTPUTS_DIR;
use officina_core::relay::catalogue::{
    self, ApprovedCatalogue, KnownProjects, RelayFloor, parse_known_projects,
};
use officina_core::relay::record::{DECISION_RECORD_FILE, DecisionRecord, parse_record};
use officina_core::relay::{JOURNAL_DIR, RELAY_DIR};

use crate::paths::{ENV_FLOOR, env_path, officina_home_from_env};

/// The Officina home as the relay sees it, with the `OFFICINA_FLOOR`
/// override the relay floor resolution starts from. Built from the
/// environment by [`RelayHome::from_env`], or from explicit values by tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayHome {
    officina_home: PathBuf,
    floor_override: Option<PathBuf>,
}

/// What `relay/approved.json` holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovedState {
    /// Nothing was ever approved.
    Missing,
    /// The file exists but cannot be read or parsed. Reported, never a
    /// crash, and never read as "nothing approved".
    Unreadable(String),
    Valid(ApprovedCatalogue),
}

impl ApprovedState {
    /// The approved catalogue, when the file is valid.
    pub fn approved(&self) -> Option<&ApprovedCatalogue> {
        match self {
            ApprovedState::Valid(approved) => Some(approved),
            ApprovedState::Missing | ApprovedState::Unreadable(_) => None,
        }
    }
}

/// Why the relay floor could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RelayFloorError {
    #[error("cannot read the relay floor {path}: {error}")]
    Unreadable { path: String, error: String },
    #[error("{path}: {error}")]
    Invalid { path: String, error: FloorError },
}

impl RelayHome {
    /// A relay home at `officina_home`, with `floor_override` standing for
    /// `OFFICINA_FLOOR`.
    pub fn new(officina_home: impl Into<PathBuf>, floor_override: Option<PathBuf>) -> RelayHome {
        RelayHome {
            officina_home: officina_home.into(),
            floor_override,
        }
    }

    /// The relay home of this process: `OFFICINA_HOME` or `~/.officina`, and
    /// `OFFICINA_FLOOR`. `None` when no home can be determined.
    pub fn from_env() -> Option<RelayHome> {
        Some(RelayHome::new(
            officina_home_from_env()?,
            env_path(ENV_FLOOR),
        ))
    }

    /// The Officina home.
    pub fn officina_home(&self) -> &Path {
        &self.officina_home
    }

    /// `OFFICINA_FLOOR` as given, the first step of the relay floor
    /// resolution.
    pub fn floor_override(&self) -> Option<&Path> {
        self.floor_override.as_deref()
    }

    /// `relay/`, where the window and `officina run` keep their files.
    pub fn relay_dir(&self) -> PathBuf {
        self.officina_home.join(RELAY_DIR)
    }

    /// `relay/approved.json`: the approved catalogue, written by the window.
    pub fn approved_file(&self) -> PathBuf {
        self.relay_dir().join("approved.json")
    }

    /// `relay/stopped`: present means the relay is stopped (AD-34).
    pub fn stopped_file(&self) -> PathBuf {
        self.relay_dir().join("stopped")
    }

    /// `relay/approvals/`: requests and decisions of `human` actions (AD-33).
    pub fn approvals_dir(&self) -> PathBuf {
        self.relay_dir().join("approvals")
    }

    /// `relay/outputs/`: the kept output of check runs, one file per run id
    /// (D-170).
    pub fn outputs_dir(&self) -> PathBuf {
        self.relay_dir().join(OUTPUTS_DIR)
    }

    /// `relay/known-projects.json`: the last readable state (D-15).
    pub fn known_projects_file(&self) -> PathBuf {
        self.relay_dir().join("known-projects.json")
    }

    /// `relay/final-roots.json`: the final path of each guarded project
    /// root, which the gate resolves once per set of roots (D-173).
    pub fn final_roots_file(&self) -> PathBuf {
        self.relay_dir().join("final-roots.json")
    }

    /// `relay/decisions.json`: the window's record of its own decisions
    /// (D-173).
    pub fn decisions_file(&self) -> PathBuf {
        self.relay_dir().join(DECISION_RECORD_FILE)
    }

    /// Reads `relay/decisions.json`, if present and of this schema.
    pub fn read_record(&self) -> Option<DecisionRecord> {
        parse_record(&fs::read(self.decisions_file()).ok()?)
    }

    /// `journal/`: one file per UTC day (AD-32).
    pub fn journal_dir(&self) -> PathBuf {
        self.officina_home.join(JOURNAL_DIR)
    }

    /// `journal/.lock`, taken around every journal append (AD-32).
    pub fn journal_lock_file(&self) -> PathBuf {
        self.journal_dir().join(".lock")
    }

    /// `floor.toml` in the Officina home, the last resort of the relay floor
    /// resolution.
    pub fn default_floor(&self) -> PathBuf {
        self.officina_home.join("floor.toml")
    }

    /// Reads `relay/approved.json` into one of its three states.
    pub fn read_approved(&self) -> ApprovedState {
        let bytes = match fs::read(self.approved_file()) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return ApprovedState::Missing,
            Err(e) => return ApprovedState::Unreadable(e.to_string()),
        };
        match serde_json::from_slice::<ApprovedCatalogue>(&bytes) {
            Ok(approved) => ApprovedState::Valid(approved),
            Err(e) => ApprovedState::Unreadable(format!("corrupt: {e}")),
        }
    }

    /// The floor the relay reads (AD-24): `OFFICINA_FLOOR`, else the floor
    /// path recorded in the approved catalogue, as the approval read it,
    /// else `floor.toml` in the home. The gate, `officina run` and the
    /// window's approval all use it (D-8); a different floor fails the hash
    /// check.
    pub fn relay_floor(&self, approved: &ApprovedState) -> PathBuf {
        if let Some(path) = &self.floor_override {
            return path.clone();
        }
        match approved.approved() {
            Some(approved) => PathBuf::from(approved.floor_file()),
            None => self.default_floor(),
        }
    }

    /// Reads `relay/known-projects.json`, if present and of this schema.
    pub fn read_known_projects(&self) -> Option<KnownProjects> {
        parse_known_projects(&fs::read_to_string(self.known_projects_file()).ok()?)
    }

    /// True when the relay is stopped (AD-34): `relay/stopped` exists, or
    /// cannot be looked at.
    pub fn relay_stopped(&self) -> bool {
        is_stopped(
            fs::metadata(self.stopped_file())
                .map(|_| ())
                .map_err(|e| e.kind()),
        )
    }
}

/// The stop flag's decision (AD-34) on what looking at `relay/stopped`
/// gave: present means stopped, and so does any failure but "not found",
/// so a flag the process cannot read never lets an action run.
pub fn is_stopped(lookup: Result<(), io::ErrorKind>) -> bool {
    !matches!(lookup, Err(io::ErrorKind::NotFound))
}

/// The Windows system directory, where `wsl.exe` is taken from (AD-30):
/// `GetSystemDirectoryW`, never `PATH` or `%SystemRoot%`. `None` off Windows
/// or when it cannot be read.
pub fn system_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        crate::platform::windows::system_directory().ok()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Reads a floor for the relay: both phases, the canonical form and its
/// hash. The path is recorded as given, normalised by the canonical form.
pub fn load_relay_floor(path: &Path) -> Result<RelayFloor, RelayFloorError> {
    let shown = path.display().to_string();
    let content = fs::read_to_string(path).map_err(|e| RelayFloorError::Unreadable {
        path: shown.clone(),
        error: e.to_string(),
    })?;
    catalogue::load(&content, &path.to_string_lossy())
        .map_err(|error| RelayFloorError::Invalid { path: shown, error })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_error_other_than_not_found_is_stopped() {
        assert!(is_stopped(Ok(())));
        assert!(!is_stopped(Err(io::ErrorKind::NotFound)));
        for kind in [
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::InvalidInput,
            io::ErrorKind::Other,
        ] {
            assert!(is_stopped(Err(kind)), "{kind:?}");
        }
        let dir = tempfile::tempdir().unwrap();
        let home = RelayHome::new(dir.path(), None);
        assert!(!home.relay_stopped());
        fs::create_dir_all(home.relay_dir()).unwrap();
        fs::write(home.stopped_file(), b"{\"since_ms\":1}").unwrap();
        assert!(home.relay_stopped());
    }
}
