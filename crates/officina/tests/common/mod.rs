//! Isolated environment for CLI tests (AD-17): every location Officina or
//! Claude Code could touch points into one temp directory, and `claude` is the
//! fake binary of this crate.

#![allow(dead_code)] // each test file uses a different subset of the helpers

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

pub const OFFICINA: &str = env!("CARGO_BIN_EXE_officina");
pub const FAKE_CLAUDE: &str = env!("CARGO_BIN_EXE_officina-fake-claude");

pub struct TestEnv {
    pub root: tempfile::TempDir,
}

impl TestEnv {
    pub fn new() -> TestEnv {
        let env = TestEnv {
            root: tempfile::tempdir().unwrap(),
        };
        std::fs::create_dir_all(env.user_home()).unwrap();
        std::fs::create_dir_all(env.claude_config().join("sessions")).unwrap();
        env
    }

    pub fn user_home(&self) -> PathBuf {
        self.root.path().join("home")
    }

    pub fn officina_home(&self) -> PathBuf {
        self.root.path().join("officina-home")
    }

    /// The Claude config dir every command sees, through `CLAUDE_CONFIG_DIR`.
    pub fn claude_config(&self) -> PathBuf {
        self.user_home().join(".claude")
    }

    pub fn fixture(&self) -> PathBuf {
        self.root.path().join("agents.json")
    }

    pub fn fake_log(&self) -> PathBuf {
        self.root.path().join("fake-claude.log")
    }

    pub fn write_agents(&self, json: &str) {
        std::fs::write(self.fixture(), json).unwrap();
    }

    pub fn events_dir(&self) -> PathBuf {
        self.officina_home().join("events")
    }

    /// `officina <args>` with the isolated environment.
    pub fn officina(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(OFFICINA);
        cmd.args(args)
            .env_clear()
            .env(
                "SYSTEMROOT",
                std::env::var_os("SYSTEMROOT").unwrap_or_default(),
            )
            .env("PATH", "")
            .env("USERPROFILE", self.user_home())
            .env("HOME", self.user_home())
            .env("OFFICINA_HOME", self.officina_home())
            .env("OFFICINA_CLAUDE_BIN", FAKE_CLAUDE)
            .env("FAKE_CLAUDE_FIXTURE", self.fixture())
            .env("FAKE_CLAUDE_LOG", self.fake_log())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd
    }

    pub fn run(&self, args: &[&str], stdin: &[u8]) -> Output {
        run_with(self.officina(args), stdin)
    }
}

pub fn run_with(mut cmd: Command, stdin: &[u8]) -> Output {
    let mut child = cmd.spawn().unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
}

/// Every file under `dir`: relative path and full content, sorted. Content
/// comparison needs no hash at test scale (AD-17).
pub fn fingerprint(dir: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let rel = path
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if path.is_dir() {
                out.push((format!("{rel}/"), Vec::new()));
                walk(base, &path, out);
            } else {
                out.push((rel, std::fs::read(&path).unwrap_or_default()));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

pub fn read_events(env: &TestEnv) -> Vec<serde_json::Value> {
    let Ok(entries) = std::fs::read_dir(env.events_dir()) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    for entry in entries.flatten() {
        let content = std::fs::read_to_string(entry.path()).unwrap();
        for line in content.lines().filter(|l| !l.trim().is_empty()) {
            lines.push(serde_json::from_str(line).unwrap());
        }
    }
    lines
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
