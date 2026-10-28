//! Isolated environment for CLI tests (AD-17): every location Officina or
//! Claude Code could touch points into one temp directory, and `claude` is the
//! fake binary of this crate.

#![allow(dead_code)] // each test file uses a different subset of the helpers

pub mod fake_output;
#[path = "../../../../tests/support/git.rs"]
pub mod git_support;
pub mod relay;

#[allow(unused_imports)] // a re-export each test file uses or not, as for dead_code above
pub use git_support::git_dir_or_skip;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

pub const OFFICINA: &str = env!("CARGO_BIN_EXE_officina");
pub const FAKE_CLAUDE: &str = env!("CARGO_BIN_EXE_officina-fake-claude");
pub const FAKE_TOOL: &str = env!("CARGO_BIN_EXE_officina-fake-tool");

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

    /// The relay's view of this environment's Officina home.
    pub fn relay_home(&self) -> officina_host::relay::home::RelayHome {
        officina_host::relay::home::RelayHome::new(self.officina_home(), None)
    }

    /// `floor.toml` in the Officina home: the relay floor when nothing else
    /// is configured (AD-24).
    pub fn floor_file(&self) -> PathBuf {
        self.officina_home().join("floor.toml")
    }

    /// Writes the floor, with `{tool}` standing for the fake tool's path.
    pub fn write_floor(&self, text: &str) {
        std::fs::create_dir_all(self.officina_home()).unwrap();
        std::fs::write(self.floor_file(), text.replace("{tool}", &fake_tool_path())).unwrap();
    }

    /// Approves the current relay floor as the window does (story 2.2),
    /// and returns the approved hash.
    pub fn approve(&self) -> String {
        let home = self.relay_home();
        let floor = home.relay_floor(&home.read_approved());
        let hash = officina_host::relay::home::load_relay_floor(&floor)
            .unwrap()
            .hash;
        officina_host::relay::authority::approve_catalogue(&home, &hash)
            .unwrap()
            .hash
    }

    /// Every journal line of the last two days; asserts none is corrupt.
    pub fn journal(&self) -> Vec<officina_core::relay::journal::JournalLine> {
        let (lines, corrupt) = officina_host::relay::journal::read_recent(
            &self.relay_home(),
            officina_host::files::now_ms(),
        );
        assert_eq!(corrupt, 0, "corrupt journal lines");
        lines
    }

    /// The journal without the approvals the test itself made.
    pub fn journal_after_approvals(&self) -> Vec<officina_core::relay::journal::JournalEntry> {
        self.journal()
            .into_iter()
            .map(|line| line.entry)
            .filter(|entry| {
                !matches!(
                    entry,
                    officina_core::relay::journal::JournalEntry::CatalogueApproved(_)
                )
            })
            .collect()
    }

    pub fn tool_log(&self) -> PathBuf {
        self.root.path().join("fake-tool.log")
    }
}

/// A copy of the fake claude in `dir`, configured by `sidecar`, written
/// next to it as `fake-claude.json` (D-86). A test that runs the collector
/// in its own process points it at this copy, since no test may set the
/// process environment.
pub fn fake_claude_in(dir: &Path, sidecar: &serde_json::Value) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let exe = dir.join(Path::new(FAKE_CLAUDE).file_name().unwrap());
    std::fs::copy(FAKE_CLAUDE, &exe).unwrap();
    std::fs::write(dir.join("fake-claude.json"), sidecar.to_string()).unwrap();
    exe
}

/// The fake tool's path as a floor writes it: forward slashes, so it needs
/// no escaping in a TOML string. A `native` program is a Windows absolute
/// path (AD-22; Linux is v1), which the fake tool's own path is not off
/// Windows: there floors name a Windows path that never starts, so their
/// actions stay valid for the tests that only read the catalogue.
pub fn fake_tool_path() -> String {
    if cfg!(windows) {
        FAKE_TOOL.replace('\\', "/")
    } else {
        "C:/path/to/officina-fake-tool.exe".to_owned()
    }
}

/// Runs `git <args>` in `dir` with the `git` of `git_dir` (the folder the
/// tests put on the `PATH` of the commands they start, D-23), and asserts
/// it succeeds.
pub fn git(git_dir: &Path, dir: &Path, args: &[&str]) {
    git_support::git_ok(&git_dir.join(git_support::GIT_EXE), dir, args);
}

/// The hooks block `officina doctor` printed, parsed.
pub fn printed_hooks_block(doctor_stdout: &[u8]) -> serde_json::Value {
    let text = String::from_utf8_lossy(doctor_stdout).to_string();
    let block_start = text.find("{\n  \"hooks\"").unwrap();
    serde_json::Deserializer::from_str(&text[block_start..])
        .into_iter()
        .next()
        .unwrap()
        .unwrap()
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

/// The launches `officina-fake-tool` logged in `log`, one JSON value each;
/// empty when it never ran.
pub fn read_tool_log(log: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// Bytes the fake tool's heartbeat grandchild wrote so far.
pub fn heartbeat_len(file: &Path) -> u64 {
    std::fs::metadata(file).map(|m| m.len()).unwrap_or(0)
}

/// True when the heartbeat in `file` no longer grows: unchanged over half a
/// second, five beats.
pub fn heartbeat_stopped(file: &Path) -> bool {
    let before = heartbeat_len(file);
    std::thread::sleep(std::time::Duration::from_millis(500));
    heartbeat_len(file) == before
}

/// Ends a heartbeat grandchild that nothing else ended (see the fake tool).
pub fn stop_heartbeat(file: &Path) {
    let mut stop = file.as_os_str().to_owned();
    stop.push(".stop");
    let _ = std::fs::write(PathBuf::from(stop), b"");
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// Where a collector thread under test emits (AD-5): each board, relay view
/// and registry view on its channel, every board through `on_board` first,
/// on the collector thread. A receiver dropped by the test just stops
/// receiving.
pub struct Sink {
    on_board: Box<dyn Fn(&officina_core::board::Board) + Send>,
    boards: std::sync::mpsc::Sender<officina_core::board::Board>,
    relays: std::sync::mpsc::Sender<officina_core::relay::view::RelayView>,
    registries: std::sync::mpsc::Sender<officina_core::registry::view::RegistryView>,
}

/// What a [`Sink`] emits, as the test receives it.
pub struct Emitted {
    pub boards: std::sync::mpsc::Receiver<officina_core::board::Board>,
    pub relays: std::sync::mpsc::Receiver<officina_core::relay::view::RelayView>,
    pub registries: std::sync::mpsc::Receiver<officina_core::registry::view::RegistryView>,
}

/// A sink whose boards go through `on_board` before their channel.
pub fn sink(on_board: impl Fn(&officina_core::board::Board) + Send + 'static) -> (Sink, Emitted) {
    use std::sync::mpsc::channel;
    let (boards, board_rx) = channel();
    let (relays, relay_rx) = channel();
    let (registries, registry_rx) = channel();
    (
        Sink {
            on_board: Box::new(on_board),
            boards,
            relays,
            registries,
        },
        Emitted {
            boards: board_rx,
            relays: relay_rx,
            registries: registry_rx,
        },
    )
}

impl officina_host::watch::Emit for Sink {
    fn board(&self, board: &officina_core::board::Board) {
        (self.on_board)(board);
        let _ = self.boards.send(board.clone());
    }

    fn relay(&self, view: &officina_core::relay::view::RelayView) {
        let _ = self.relays.send(view.clone());
    }

    fn registry(&self, view: &officina_core::registry::view::RegistryView) {
        let _ = self.registries.send(view.clone());
    }
}
