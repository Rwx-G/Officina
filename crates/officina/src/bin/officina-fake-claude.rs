//! Stand-in for `claude` in tests (AD-17). Never shipped.
//!
//! - `FAKE_CLAUDE_FIXTURE`: file printed for `agents --json`.
//! - `FAKE_CLAUDE_MODE`: `ok` (default), `garbage`, `exit1`, `hang`,
//!   `orphan` (answers as `ok` after starting a copy of itself in `hang` mode
//!   that inherits its output and outlives it, as a process `claude` leaves
//!   running would), or `tree` (starts a copy of itself that appends a byte
//!   to `FAKE_CLAUDE_HEARTBEAT` every 100 ms, for 30 s at most or until a
//!   file named like it plus `.stop` exists, then hangs for 60 s).
//! - `FAKE_CLAUDE_HANG_MS`: how long `hang` sleeps, 60 s by default.
//! - `FAKE_CLAUDE_VERSION`: answer to `--version`.
//! - `FAKE_CLAUDE_LOG`: each call appends its argv and `CLAUDE_CONFIG_DIR`,
//!   so tests can assert what reached the child.

use std::io::Write;
use std::time::Duration;

/// How long the process `orphan` leaves behind holds the output: well past
/// the caller's grace period, short enough not to linger after the test.
const ORPHAN_HOLD_MS: &str = "5000";

fn print_fixture() {
    let fixture = std::env::var_os("FAKE_CLAUDE_FIXTURE")
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_else(|| "[]".into());
    print!("{fixture}");
}

fn heartbeat() {
    let Some(file) = std::env::var_os("FAKE_CLAUDE_HEARTBEAT") else {
        return;
    };
    let mut stop = file.clone();
    stop.push(".stop");
    let stop = std::path::PathBuf::from(stop);
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
    if std::env::var("FAKE_CLAUDE_MODE").as_deref() == Ok("heartbeat") {
        heartbeat();
        return;
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(log) = std::env::var_os("FAKE_CLAUDE_LOG")
        && let Ok(mut f) = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(log)
    {
        let config = std::env::var("CLAUDE_CONFIG_DIR").unwrap_or_default();
        let _ = writeln!(f, "{}\tCLAUDE_CONFIG_DIR={config}", args.join(" "));
    }
    let mode = std::env::var("FAKE_CLAUDE_MODE").unwrap_or_else(|_| "ok".into());
    if args.first().map(String::as_str) == Some("--version") {
        let version = std::env::var("FAKE_CLAUDE_VERSION").unwrap_or_else(|_| "9.9.9".into());
        println!("{version} (Claude Code)");
        return;
    }
    match mode.as_str() {
        "hang" => {
            let ms = std::env::var("FAKE_CLAUDE_HANG_MS")
                .ok()
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
                    .env_remove("FAKE_CLAUDE_LOG")
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
                    .env_remove("FAKE_CLAUDE_LOG")
                    .spawn();
            }
            print_fixture();
        }
        _ => print_fixture(),
    }
}
