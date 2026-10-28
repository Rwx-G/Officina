//! Stand-in for `claude` in tests (AD-17). Never shipped.
//!
//! Each setting comes from its `FAKE_CLAUDE_*` environment variable or,
//! when that is absent, from the key of the same name in `fake-claude.json`
//! next to this executable (D-86): a test that runs the collector in its own
//! process copies the fake into a folder with that file, since no test may
//! set the process environment.
//!
//! - `FAKE_CLAUDE_FIXTURE` (`fixture`): file printed for `agents --json`.
//! - `FAKE_CLAUDE_MODE` (`mode`): `ok` (default), `garbage`, `exit1`, `hang`,
//!   `orphan` (answers as `ok` after starting a copy of itself in `hang` mode
//!   that inherits its output and outlives it, as a process `claude` leaves
//!   running would), or `tree` (starts a copy of itself that appends a byte
//!   to `FAKE_CLAUDE_HEARTBEAT` every 100 ms, for 30 s at most or until a
//!   file named like it plus `.stop` exists, then hangs for 60 s).
//! - `FAKE_CLAUDE_HANG_MS` (`hang_ms`): how long `hang` sleeps, 60 s by
//!   default.
//! - `FAKE_CLAUDE_VERSION` (`version`): answer to `--version`.
//! - `FAKE_CLAUDE_LOG` (`log`): each call appends its argv and
//!   `CLAUDE_CONFIG_DIR`, so tests can assert what reached the child; empty
//!   logs nothing.
//! - `FAKE_CLAUDE_HEARTBEAT` (`heartbeat`): the file `tree`'s copy beats in.

use std::io::Write;
use std::time::Duration;

/// How long the process `orphan` leaves behind holds the output: well past
/// the caller's grace period, short enough not to linger after the test.
const ORPHAN_HOLD_MS: &str = "5000";

/// The sidecar's file name, next to the executable.
const SIDECAR: &str = "fake-claude.json";

/// The setting `name`: `FAKE_CLAUDE_<NAME>` when set, even empty, else the
/// sidecar's `name` key.
fn setting(name: &str) -> Option<String> {
    if let Some(value) = std::env::var_os(format!("FAKE_CLAUDE_{}", name.to_ascii_uppercase())) {
        return Some(value.to_string_lossy().into_owned());
    }
    let sidecar = std::env::current_exe().ok()?.parent()?.join(SIDECAR);
    let config: serde_json::Value = serde_json::from_slice(&std::fs::read(sidecar).ok()?).ok()?;
    match config.get(name)? {
        serde_json::Value::String(text) => Some(text.clone()),
        other => Some(other.to_string()),
    }
}

fn print_fixture() {
    let fixture = setting("fixture")
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_else(|| "[]".into());
    print!("{fixture}");
}

fn heartbeat() {
    let Some(file) = setting("heartbeat") else {
        return;
    };
    let stop = std::path::PathBuf::from(format!("{file}.stop"));
    let started = std::time::Instant::now();
    while started.elapsed() < Duration::from_secs(30) && !stop.exists() {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&file)
        {
            let _ = f.write_all(b".");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn main() {
    let mode = setting("mode").unwrap_or_else(|| "ok".into());
    if mode == "heartbeat" {
        heartbeat();
        return;
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(log) = setting("log").filter(|log| !log.is_empty())
        && let Ok(mut f) = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(log)
    {
        let config = std::env::var("CLAUDE_CONFIG_DIR").unwrap_or_default();
        let _ = writeln!(f, "{}\tCLAUDE_CONFIG_DIR={config}", args.join(" "));
    }
    if args.first().map(String::as_str) == Some("--version") {
        let version = setting("version").unwrap_or_else(|| "9.9.9".into());
        println!("{version} (Claude Code)");
        return;
    }
    match mode.as_str() {
        "hang" => {
            let ms = setting("hang_ms")
                .and_then(|v| v.parse().ok())
                .unwrap_or(60_000);
            std::thread::sleep(Duration::from_millis(ms));
        }
        "exit1" => {
            eprintln!("fake failure");
            std::process::exit(1);
        }
        "garbage" => println!("this is not json"),
        "tree" => {
            if let Ok(me) = std::env::current_exe() {
                let _ = std::process::Command::new(me)
                    .env("FAKE_CLAUDE_MODE", "heartbeat")
                    .env("FAKE_CLAUDE_LOG", "")
                    .stdin(std::process::Stdio::null())
                    .spawn();
            }
            std::thread::sleep(Duration::from_secs(60));
        }
        "orphan" => {
            if let Ok(me) = std::env::current_exe() {
                let _ = std::process::Command::new(me)
                    .env("FAKE_CLAUDE_MODE", "hang")
                    .env("FAKE_CLAUDE_HANG_MS", ORPHAN_HOLD_MS)
                    .env("FAKE_CLAUDE_LOG", "")
                    .spawn();
            }
            print_fixture();
        }
        _ => print_fixture(),
    }
}
