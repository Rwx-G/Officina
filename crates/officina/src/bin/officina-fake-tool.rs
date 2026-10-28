//! Stand-in for a relay action's program in tests (AD-37). Never shipped:
//! `test = false`, and the release packages an explicit file list.
//!
//! - `FAKE_TOOL_LOG`: each launch appends one JSON line with its arguments
//!   (`argv`, the program excluded), working directory (`cwd`), sorted
//!   environment variable names (`env_keys`) and the number of bytes it read
//!   on stdin (`stdin_bytes`); the line count is the launch counter.
//! - `FAKE_TOOL_LOG_VALUES`: comma-separated variable names whose values the
//!   line also carries (`env`), for the tests that check a declared `env`.
//! - `FAKE_TOOL_MODE`: `exit:<n>` (exits with `n`, a full Windows code
//!   included), `bytes:<n>` (writes `n` bytes on stdout and `n` on stderr,
//!   see `tests/common/fake_output.rs`), `sleep:<s>` (seconds, then exit 0),
//!   `tree` (starts a copy of itself that appends a byte to
//!   `FAKE_TOOL_HEARTBEAT` every 100 ms, then sleeps), `orphan` (starts the
//!   same heartbeat with its own stdout and stderr, then exits 0 at once, as
//!   a program that leaves a daemon behind), `echo` (writes its arguments,
//!   joined by spaces, as one line on stdout); unset, exit 0. The heartbeat
//!   ends by itself after a while, or as soon as a file named like the
//!   heartbeat plus `.stop` exists, which tests create to clean up a
//!   grandchild nothing else ends.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

#[path = "../../tests/common/fake_output.rs"]
#[allow(dead_code)] // shared with the tests, which also use what the tool does not
mod fake_output;

/// How long the `tree` child and its heartbeat grandchild live when nothing
/// stops them: long past any test's wait, short enough not to linger.
const TREE_LIFETIME: Duration = Duration::from_secs(30);
const HEARTBEAT_EVERY: Duration = Duration::from_millis(100);
/// The grandchild's mode, set by `tree` only.
const HEARTBEAT_MODE: &str = "heartbeat";

fn log_launch() {
    let Some(log) = std::env::var_os("FAKE_TOOL_LOG") else {
        return;
    };
    let mut stdin = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut stdin);
    let mut env_keys: Vec<String> = std::env::vars_os()
        .map(|(k, _)| k.to_string_lossy().into_owned())
        .collect();
    env_keys.sort();
    let values: BTreeMap<String, String> = std::env::var("FAKE_TOOL_LOG_VALUES")
        .unwrap_or_default()
        .split(',')
        .filter(|name| !name.is_empty())
        .filter_map(|name| Some((name.to_owned(), std::env::var(name).ok()?)))
        .collect();
    let line = serde_json::json!({
        "argv": std::env::args_os().skip(1).map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>(),
        "cwd": std::env::current_dir().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default(),
        "env_keys": env_keys,
        "env": values,
        "stdin_bytes": stdin.len(),
    });
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(log)
    {
        let _ = file.write_all(format!("{line}\n").as_bytes());
    }
}

fn write_stream(out: &mut impl Write, len: u64, byte: fn(u64) -> u8) {
    let mut written = 0;
    while written < len {
        let chunk = (len - written).min(8192);
        let bytes: Vec<u8> = (written..written + chunk).map(byte).collect();
        if out.write_all(&bytes).is_err() {
            return;
        }
        written += chunk;
    }
    let _ = out.flush();
}

fn heartbeat() {
    let Some(file) = std::env::var_os("FAKE_TOOL_HEARTBEAT") else {
        return;
    };
    let mut stop = file.clone();
    stop.push(".stop");
    let stop = std::path::PathBuf::from(stop);
    let started = Instant::now();
    while started.elapsed() < TREE_LIFETIME && !stop.exists() {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&file)
        {
            let _ = f.write_all(b".");
        }
        std::thread::sleep(HEARTBEAT_EVERY);
    }
}

fn main() {
    let mode = std::env::var("FAKE_TOOL_MODE").unwrap_or_default();
    if mode == HEARTBEAT_MODE {
        heartbeat();
        return;
    }
    log_launch();
    let (name, value) = mode.split_once(':').unwrap_or((mode.as_str(), ""));
    match name {
        "exit" => {
            let code: i64 = value.parse().unwrap_or(1);
            // A full Windows code such as 0xC0000005 is past i32::MAX: its
            // bits are what the process exit status carries.
            std::process::exit(code as u32 as i32);
        }
        "bytes" => {
            let len: u64 = value.parse().unwrap_or(0);
            write_stream(&mut std::io::stdout().lock(), len, fake_output::stdout_byte);
            write_stream(&mut std::io::stderr().lock(), len, fake_output::stderr_byte);
        }
        "echo" => {
            let args: Vec<String> = std::env::args().skip(1).collect();
            println!("{}", args.join(" "));
        }
        "sleep" => {
            let seconds: f64 = value.parse().unwrap_or(0.0);
            std::thread::sleep(Duration::from_secs_f64(seconds.max(0.0)));
        }
        "tree" | "orphan" => {
            // The grandchild inherits this process's stdout and stderr.
            if let Ok(me) = std::env::current_exe() {
                let _ = std::process::Command::new(me)
                    .env("FAKE_TOOL_MODE", HEARTBEAT_MODE)
                    .env_remove("FAKE_TOOL_LOG")
                    .stdin(std::process::Stdio::null())
                    .spawn();
            }
            if name == "tree" {
                std::thread::sleep(TREE_LIFETIME);
            }
        }
        _ => {}
    }
}
