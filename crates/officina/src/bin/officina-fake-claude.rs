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
//!   triage modes below; `interactive` (story 5.2, D-129) records in its
//!   log line whether its stdin and stdout are a console, then exits 0, for
//!   the kinds that start in a console of their own; `bg` (story 5.5,
//!   D-129) answers a call holding `--bg` as `claude --bg` does (E20): it
//!   prints `backgrounded · <id> · <name>`, then appends to the fixture a
//!   background entry with that id, the `--name` value, its working
//!   directory as `cwd` and the current time as `startedAt`, so a later
//!   `agents --json` lists it; any other call prints the fixture.
//!   `bg-hold` (story 5.7, D-128 review F10) does the same, then leaves a
//!   copy of itself holding the output pipes, as the supervisor `claude
//!   --bg` may start does: the heartbeat copy when `heartbeat` is set,
//!   else one that hangs `ORPHAN_HOLD_MS`; with `hang_ms` set it then
//!   sleeps that long before exiting 0, for a deadline.
//!   `bg-untrusted` (story 5.7) answers a `--bg` call as `claude` does in
//!   a folder whose trust dialog was never accepted (E19): the message on
//!   stderr, exit 1. `orchestrator` (story 5.16) answers a call holding
//!   `--name` without `--bg` as an interactive session does: it records in
//!   its log line whether its stdin and stdout are a console, appends to
//!   the fixture an interactive entry with the `--name` value, its working
//!   directory as `cwd`, its pid, `idle` and the current time as
//!   `startedAt` (as E24 found the orchestrator listed), unless `unlisted`
//!   is `true`, then sleeps `hang_ms` (none by default) so its pid stays
//!   alive; any other call prints the fixture.
//!   `logs` (story 5.10, D-129) answers `logs <id>` with a
//!   rendered screen holding CSI, OSC and other escape sequences and an
//!   error line built from the first of the meeting's `LOG_ERROR_MARKERS`
//!   (never a typed string); any other call prints the fixture.
//!   `logs-hang` sleeps `hang_ms` on `logs <id>` and prints the fixture
//!   for any other call, for a meeting whose logs take their deadline.
//! - `FAKE_CLAUDE_HANG_MS` (`hang_ms`): how long `hang` sleeps, 60 s by
//!   default.
//! - `FAKE_CLAUDE_VERSION` (`version`): answer to `--version`.
//! - `FAKE_CLAUDE_LOG` (`log`): each call appends one line of JSON (D-129),
//!   `{"argv": [...], "cwd": "...", "env_names": [...], "config_dir": ...}`,
//!   the names sorted and never a value but `CLAUDE_CONFIG_DIR`'s, which
//!   Officina sets itself, so tests compare argument vectors element by
//!   element and assert what reached the child by name; empty logs nothing.
//! - `FAKE_CLAUDE_HEARTBEAT` (`heartbeat`): the file `tree`'s copy beats in.
//! - `FAKE_CLAUDE_TRIAGE_LOG` (`triage_log`): each triage call appends one
//!   JSON line with its argv, its environment, its config dir, its working
//!   directory and its stdin, so tests can assert what reached it and how.
//!
//! As `claude` does (S7, E19), with `CLAUDE_CODE_DISABLE_AGENT_VIEW=1` a
//! call holding `--bg`, or whose first argument is `agents` or `logs`,
//! exits 1 with Claude Code's message, after its log line.
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
//!
//! In a mode starting with `review` (story 5.13, D-582), a `-p` call is a
//! review of an `auto` request: it logs as a triage call does, then
//! `review` answers the decision a `FAKE-REVIEW:<decision>` token in stdin
//! names (`escalate` without one) with the reason `REVIEW_REASON`;
//! `review-garbage` prints prose; `review-extra-field` adds an argument
//! vector to the answer; `review-exit1` fails; `review-hang` sleeps
//! `hang_ms`; `review-slow` sleeps `hang_ms`, then answers as `review`
//! (story 5.14). A triage call in those modes (its schema names verdicts) is
//! answered in the triage mode `triage_mode` names. Any other call
//! answers as `ok`.

use std::io::{IsTerminal, Read, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long the process `orphan` leaves behind holds the output: well past
/// the caller's grace period, short enough not to linger after the test.
const ORPHAN_HOLD_MS: &str = "5000";

/// The sidecar's file name, next to the executable.
const SIDECAR: &str = "fake-claude.json";

/// The token a test puts in an excerpt to choose the fake model's verdict.
const VERDICT_TOKEN: &str = "FAKE-VERDICT:";

/// The token a test puts in the author's prompt to choose the fake review's
/// decision.
const REVIEW_TOKEN: &str = "FAKE-REVIEW:";

/// The reason every answer of the fake review gives, which tests search
/// for where it must never appear.
const REVIEW_REASON: &str = "FAKE-REVIEW-REASON-7f3a";

/// Set to `1`, `claude` refuses `--bg`, `agents` and `logs` (S7).
const AGENT_VIEW_OFF: &str = "CLAUDE_CODE_DISABLE_AGENT_VIEW";

/// What `claude` refuses with agent view off, and the name its message
/// gives it (E19).
fn refused_without_agent_view(args: &[String]) -> Option<&'static str> {
    if args.iter().any(|a| a == "--bg") {
        return Some("'--bg'");
    }
    match args.first().map(String::as_str) {
        Some("agents") => Some("'claude agents --json'"),
        Some("logs") => Some("'claude logs'"),
        _ => None,
    }
}

/// Appends this call's line to the log (D-129): its argv, its working
/// directory, its variables' names sorted, its config dir, and in
/// `interactive` mode whether stdin and stdout are a console.
fn log_call(log: &str, args: &[String], interactive: bool) {
    let Ok(mut f) = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(log)
    else {
        return;
    };
    let mut names: Vec<String> = std::env::vars_os()
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut entry = serde_json::json!({
        "argv": args,
        "cwd": std::env::current_dir().ok(),
        "env_names": names,
        "config_dir": std::env::var("CLAUDE_CONFIG_DIR").ok(),
    });
    if interactive {
        entry["console"] = serde_json::json!({
            "stdin": std::io::stdin().is_terminal(),
            "stdout": std::io::stdout().is_terminal(),
        });
    }
    let _ = writeln!(f, "{entry}");
}

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

/// A `--bg` call in mode `bg`: a new background session in the fixture,
/// and the lines `claude --bg` prints for it (E20).
fn background(args: &[String]) {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let started_at = u64::try_from(since_epoch.as_millis()).unwrap_or(u64::MAX);
    let id = format!("{:08x}", since_epoch.subsec_nanos() ^ std::process::id());
    let name = args
        .iter()
        .position(|a| a == "--name")
        .and_then(|at| args.get(at + 1))
        .cloned()
        .unwrap_or_default();
    let entry = serde_json::json!({
        "id": id,
        "sessionId": format!("{id}-0000-4000-8000-{started_at:012x}"),
        "name": name,
        "cwd": std::env::current_dir().ok(),
        "kind": "background",
        "pid": std::process::id(),
        "startedAt": started_at,
        "status": "busy",
        "state": "working",
    });
    if let Some(fixture) = setting("fixture") {
        let mut entries: Vec<serde_json::Value> = std::fs::read(&fixture)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        entries.push(entry);
        let _ = std::fs::write(&fixture, serde_json::Value::from(entries).to_string());
    }
    println!("backgrounded \u{b7} {id} \u{b7} {name}");
    for command in ["agents", "attach", "logs", "stop"] {
        println!("  claude {command} {id}");
    }
}

/// Leaves a copy of this program holding the output pipes it inherits:
/// the heartbeat copy when a heartbeat file is set, else one that hangs.
fn hold_the_pipes() {
    if setting("heartbeat").is_some() {
        start_heartbeat();
        return;
    }
    if let Ok(me) = std::env::current_exe() {
        let _ = std::process::Command::new(me)
            .env("FAKE_CLAUDE_MODE", "hang")
            .env("FAKE_CLAUDE_HANG_MS", ORPHAN_HOLD_MS)
            .env("FAKE_CLAUDE_LOG", "")
            .stdin(std::process::Stdio::null())
            .spawn();
    }
}

/// A session started in a console of its own in mode `orchestrator`: an
/// interactive entry in the fixture, as `claude agents` lists one (E24),
/// unless the sidecar says it is unlisted, then the session's lifetime.
fn interactive_session(args: &[String]) {
    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let name = args
        .iter()
        .position(|a| a == "--name")
        .and_then(|at| args.get(at + 1))
        .cloned()
        .unwrap_or_default();
    let listed = setting("unlisted").is_none_or(|unlisted| unlisted != "true");
    if listed && let Some(fixture) = setting("fixture") {
        let entry = serde_json::json!({
            "sessionId": format!("{:08x}-0000-4000-8000-{started_at:012x}", std::process::id()),
            "name": name,
            "cwd": std::env::current_dir().ok(),
            "kind": "interactive",
            "pid": std::process::id(),
            "startedAt": u64::try_from(started_at).unwrap_or(u64::MAX),
            "status": "idle",
        });
        let mut entries: Vec<serde_json::Value> = std::fs::read(&fixture)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        entries.push(entry);
        let _ = std::fs::write(&fixture, serde_json::Value::from(entries).to_string());
    }
    if let Some(ms) = setting("hang_ms").and_then(|v| v.parse().ok()) {
        std::thread::sleep(Duration::from_millis(ms));
    }
}

/// What `claude logs <id>` prints in mode `logs`: a rendered screen as E23
/// saw it, each line wrapped in sequences of one family (CSI colours and
/// cursor moves, OSC titles and links, a character set designation, a
/// string control), its line ends `\r\n`, and an error line built from the
/// first of the meeting's error markers.
fn logs_screen() -> String {
    let marker = officina_host::meeting::LOG_ERROR_MARKERS[0];
    [
        "\u{1b}]0;claude\u{7}\u{1b}[?25l\u{1b}[2J\u{1b}[H".to_owned(),
        "\u{1b}[1m\u{1b}[38;5;208m> \u{1b}[0mBump the dependency\r\n".to_owned(),
        "\u{1b}(B\u{1b}[2mRunning the tests\u{1b}[0m\r\n".to_owned(),
        format!("\u{1b}[31m  {marker} the build failed\u{1b}[39m\r\n"),
        "\u{1b}]8;;https://example.com/run\u{1b}\\the run\u{1b}]8;;\u{1b}\\\r\n".to_owned(),
        "\u{1b}P1$r0m\u{1b}\\\u{1b}[7mbypass permissions on\u{1b}[27m\r\n".to_owned(),
    ]
    .concat()
}

/// What `claude --bg` prints in a folder never trusted (E19).
const NOT_TRUSTED: &str = "Workspace not trusted. Run `claude` in this folder once and accept the trust prompt, then retry.";

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

/// The decision a `FAKE-REVIEW:<decision>` token in `stdin` names, as
/// letters, digits and underscores; `escalate` without one.
fn named_decision(stdin: &str) -> String {
    stdin
        .split(REVIEW_TOKEN)
        .nth(1)
        .map(|rest| {
            rest.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect::<String>()
        })
        .filter(|decision| !decision.is_empty())
        .unwrap_or_else(|| "escalate".into())
}

/// One review call (`-p`, story 5.13, D-582) in `mode`.
fn review(mode: &str, args: &[String]) {
    let mut stdin = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin);
    log_triage(args, &stdin);
    let decision = named_decision(&stdin);
    match mode {
        "review-garbage" => println!("Approve it."),
        "review-extra-field" => println!(
            "{}",
            result_with(
                serde_json::json!({"decision": decision, "reason": "Fine.", "argv": ["C:/Windows/System32/cmd.exe"]})
            )
        ),
        "review-exit1" => {
            eprintln!("fake review failure");
            std::process::exit(1);
        }
        "review-hang" => {
            let ms = setting("hang_ms")
                .and_then(|v| v.parse().ok())
                .unwrap_or(60_000);
            std::thread::sleep(Duration::from_millis(ms));
        }
        "review-slow" => {
            let ms = setting("hang_ms")
                .and_then(|v| v.parse().ok())
                .unwrap_or(60_000);
            std::thread::sleep(Duration::from_millis(ms));
            println!(
                "{}",
                result_with(serde_json::json!({"decision": decision, "reason": REVIEW_REASON}))
            );
        }
        _ => println!(
            "{}",
            result_with(serde_json::json!({"decision": decision, "reason": REVIEW_REASON}))
        ),
    }
}

fn main() {
    let mode = setting("mode").unwrap_or_else(|| "ok".into());
    if mode == "heartbeat" {
        heartbeat();
        return;
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(log) = setting("log").filter(|log| !log.is_empty()) {
        log_call(&log, &args, mode == "interactive" || mode == "orchestrator");
    }
    if std::env::var(AGENT_VIEW_OFF).is_ok_and(|v| v == "1")
        && let Some(refused) = refused_without_agent_view(&args)
    {
        eprintln!("{refused} is disabled by {AGENT_VIEW_OFF}.");
        std::process::exit(1);
    }
    if args.first().map(String::as_str) == Some("--version") {
        let version = setting("version").unwrap_or_else(|| "9.9.9".into());
        println!("{version} (Claude Code)");
        return;
    }
    if mode.starts_with("triage") || mode.starts_with("review") {
        // A triage call's schema names verdicts; in a review mode it is
        // answered in `triage_mode`, so one copy serves both threads.
        let triage_call = args.iter().any(|a| a.contains("\"verdict\""));
        match args.first().map(String::as_str) {
            Some("-p") if mode.starts_with("triage") => triage(&mode, &args),
            Some("-p") if triage_call => {
                triage(
                    &setting("triage_mode").unwrap_or_else(|| "triage".into()),
                    &args,
                );
            }
            Some("-p") => review(&mode, &args),
            _ => print_fixture(),
        }
        return;
    }
    match mode.as_str() {
        "interactive" => {}
        "orchestrator"
            if args.iter().any(|a| a == "--name") && !args.iter().any(|a| a == "--bg") =>
        {
            interactive_session(&args);
        }
        "bg" if args.iter().any(|a| a == "--bg") => background(&args),
        "bg-hold" if args.iter().any(|a| a == "--bg") => {
            background(&args);
            hold_the_pipes();
            if let Some(ms) = setting("hang_ms").and_then(|v| v.parse().ok()) {
                std::thread::sleep(Duration::from_millis(ms));
            }
        }
        "logs" if args.first().map(String::as_str) == Some("logs") => print!("{}", logs_screen()),
        "logs-hang" if args.first().map(String::as_str) == Some("logs") => {
            let ms = setting("hang_ms")
                .and_then(|v| v.parse().ok())
                .unwrap_or(60_000);
            std::thread::sleep(Duration::from_millis(ms));
        }
        "bg-untrusted" if args.iter().any(|a| a == "--bg") => {
            eprintln!("{NOT_TRUSTED}");
            std::process::exit(1);
        }
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
