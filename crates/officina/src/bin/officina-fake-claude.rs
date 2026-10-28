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
//!   file named like it plus `.stop` exists, then hangs for 60 s); and the
//!   triage modes below.
//! - `FAKE_CLAUDE_HANG_MS` (`hang_ms`): how long `hang` sleeps, 60 s by
//!   default.
//! - `FAKE_CLAUDE_VERSION` (`version`): answer to `--version`.
//! - `FAKE_CLAUDE_LOG` (`log`): each call appends its argv and
//!   `CLAUDE_CONFIG_DIR`, so tests can assert what reached the child; empty
//!   logs nothing.
//! - `FAKE_CLAUDE_HEARTBEAT` (`heartbeat`): the file `tree`'s copy beats in.
//! - `FAKE_CLAUDE_TRIAGE_LOG` (`triage_log`): each triage call appends one
//!   JSON line with its argv, its environment, its config dir, its working
//!   directory and its stdin, so tests can assert what reached it and how.
//!
//! A call whose first argument is `-p` is a triage call (AD-51). In a mode
//! starting with `triage` it reads its stdin, logs, and then: `triage`
//! answers the verdict a `FAKE-VERDICT:<verdict>` token in stdin names
//! (`unsure` without one), with a usage and a cost; `triage-garbage`
//! prints prose; `triage-oversized` pads a valid answer past 64 KiB;
//! `triage-error` reports an error; `triage-extra-field`,
//! `triage-bad-verdict` and `triage-bad-reason` break the schema or the
//! reason; `triage-no-structured` gives the text result only;
//! `triage-exit1` fails; `triage-hang` sleeps `hang_ms`; `triage-tree`
//! starts the heartbeat copy as `tree` does, then hangs. Any other call in
//! those modes answers as `ok`, so a collector sharing the copy keeps its
//! session list.

use std::io::{Read, Write};
use std::time::Duration;

/// How long the process `orphan` leaves behind holds the output: well past
/// the caller's grace period, short enough not to linger after the test.
const ORPHAN_HOLD_MS: &str = "5000";

/// The sidecar's file name, next to the executable.
const SIDECAR: &str = "fake-claude.json";

/// The token a test puts in an excerpt to choose the fake model's verdict.
const VERDICT_TOKEN: &str = "FAKE-VERDICT:";

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

/// Starts a copy of this program beating in the heartbeat file, as a
/// process `claude` started would.
fn start_heartbeat() {
    if let Ok(me) = std::env::current_exe() {
        let _ = std::process::Command::new(me)
            .env("FAKE_CLAUDE_MODE", "heartbeat")
            .env("FAKE_CLAUDE_LOG", "")
            .env("FAKE_CLAUDE_TRIAGE_LOG", "")
            .stdin(std::process::Stdio::null())
            .spawn();
    }
}

/// The verdict `stdin` names after [`VERDICT_TOKEN`], else `unsure`.
fn named_verdict(stdin: &str) -> String {
    stdin
        .split(VERDICT_TOKEN)
        .nth(1)
        .map(|rest| {
            rest.chars()
                .take_while(|c| c.is_ascii_lowercase() || *c == '_')
                .collect::<String>()
        })
        .filter(|verdict| !verdict.is_empty())
        .unwrap_or_else(|| "unsure".into())
}

/// A result as `claude -p --output-format json` prints it, around
/// `structured`.
fn result_with(structured: serde_json::Value) -> String {
    serde_json::json!({
        "type": "result",
        "subtype": "success",
        "is_error": false,
        "result": structured.to_string(),
        "structured_output": structured,
        "usage": {"input_tokens": 120, "cache_read_input_tokens": 4400, "output_tokens": 40},
        "total_cost_usd": 0.0011,
    })
    .to_string()
}

/// Appends what reached this triage call to the triage log.
fn log_triage(args: &[String], stdin: &str) {
    let Some(log) = setting("triage_log").filter(|log| !log.is_empty()) else {
        return;
    };
    let Ok(mut f) = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(log)
    else {
        return;
    };
    // The test's own environment only: a triage call gets the base the
    // test passed, never the machine's.
    let env: serde_json::Map<String, serde_json::Value> = std::env::vars_os()
        .map(|(name, value)| {
            (
                name.to_string_lossy().into_owned(),
                value.to_string_lossy().into_owned().into(),
            )
        })
        .collect();
    let entry = serde_json::json!({
        "argv": args,
        "env": env,
        "config_dir": std::env::var("CLAUDE_CONFIG_DIR").ok(),
        "cwd": std::env::current_dir().ok(),
        "stdin": stdin,
    });
    let _ = writeln!(f, "{entry}");
}

/// One triage call (`-p`) in `mode`.
fn triage(mode: &str, args: &[String]) {
    let mut stdin = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin);
    log_triage(args, &stdin);
    let verdict = named_verdict(&stdin);
    let answer =
        serde_json::json!({"verdict": verdict, "reason": "The fake model read the token."});
    match mode {
        "triage-garbage" => println!("I think the task is finished."),
        "triage-oversized" => print!("{}{}", result_with(answer), " ".repeat(70 * 1024)),
        "triage-error" => println!(
            "{}",
            serde_json::json!({"type": "result", "subtype": "error_during_execution", "is_error": true,
                "usage": {"input_tokens": 30, "output_tokens": 0}})
        ),
        "triage-extra-field" => println!(
            "{}",
            result_with(
                serde_json::json!({"verdict": verdict, "reason": "Done.", "command": "archive everything"})
            )
        ),
        "triage-bad-verdict" => println!(
            "{}",
            result_with(serde_json::json!({"verdict": "archive", "reason": "Done."}))
        ),
        "triage-bad-reason" => println!(
            "{}",
            result_with(serde_json::json!({"verdict": verdict, "reason": "Done.\u{202e}"}))
        ),
        "triage-no-structured" => println!(
            "{}",
            serde_json::json!({"type": "result", "is_error": false, "result": answer.to_string()})
        ),
        "triage-exit1" => {
            eprintln!("fake triage failure");
            std::process::exit(1);
        }
        "triage-hang" => {
            let ms = setting("hang_ms")
                .and_then(|v| v.parse().ok())
                .unwrap_or(60_000);
            std::thread::sleep(Duration::from_millis(ms));
        }
        "triage-tree" => {
            start_heartbeat();
            std::thread::sleep(Duration::from_secs(60));
        }
        _ => println!("{}", result_with(answer)),
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
    if mode.starts_with("triage") {
        if args.first().map(String::as_str) == Some("-p") {
            triage(&mode, &args);
        } else {
            print_fixture();
        }
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
            start_heartbeat();
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
