//! The workshop's test doubles (D-223): the fake engine and the logging
//! git, each a link to the built binary in a folder of its own beside it,
//! with its sidecar, state and log.

use std::path::{Path, PathBuf};

use officina_host::workshop::engine::{BinarySource, ResolvedDocker};

pub const FAKE_DOCKER: &str = env!("CARGO_BIN_EXE_officina-fake-docker");
pub const GIT_LOGGER: &str = env!("CARGO_BIN_EXE_officina-git-logger");

/// A folder beside `binary` holding a link to it. Beside the built binary,
/// so the hard link works: a copy written by this process stays open for
/// writing in children other test threads fork, and Linux then refuses to
/// run it ("Text file busy", D-884).
fn linked(binary: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir_in(Path::new(binary).parent().unwrap()).unwrap();
    let exe = dir.path().join(Path::new(binary).file_name().unwrap());
    if std::fs::hard_link(binary, &exe).is_err() {
        std::fs::copy(binary, &exe).unwrap();
    }
    (dir, exe)
}

fn json_lines(path: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
        .collect()
}

/// The fake engine, configured by `sidecar`, its state `state`.
pub struct FakeEngine {
    pub dir: tempfile::TempDir,
    pub exe: PathBuf,
}

impl FakeEngine {
    pub fn new(sidecar: &serde_json::Value, state: &serde_json::Value) -> FakeEngine {
        let (dir, exe) = linked(FAKE_DOCKER);
        std::fs::write(dir.path().join("fake-docker.json"), sidecar.to_string()).unwrap();
        std::fs::write(dir.path().join("fake-docker-state.json"), state.to_string()).unwrap();
        FakeEngine { dir, exe }
    }

    pub fn docker(&self) -> ResolvedDocker {
        ResolvedDocker::at(self.exe.clone(), BinarySource::Override)
    }

    /// Every call so far, `{"argv": [...], "env_names": [...]}` each.
    pub fn log(&self) -> Vec<serde_json::Value> {
        json_lines(&self.dir.path().join("fake-docker.log"))
    }
}

/// The logging git, running `git` after logging each argument vector.
pub struct GitLogger {
    pub dir: tempfile::TempDir,
    pub exe: PathBuf,
}

impl GitLogger {
    pub fn new(git: &Path) -> GitLogger {
        let (dir, exe) = linked(GIT_LOGGER);
        std::fs::write(
            dir.path().join("git-logger.json"),
            serde_json::json!({ "git": git }).to_string(),
        )
        .unwrap();
        GitLogger { dir, exe }
    }

    /// Every call so far, `{"argv": [...]}` each.
    pub fn log(&self) -> Vec<serde_json::Value> {
        json_lines(&self.dir.path().join("git-logger.log"))
    }
}
