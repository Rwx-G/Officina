//! Stand-in for `claude` in tests (AD-17). Never shipped.
//!
//! - `FAKE_CLAUDE_FIXTURE`: file printed for `agents --json`.
//! - `FAKE_CLAUDE_MODE`: `ok` (default), `garbage`, `exit1`, `hang`.
//! - `FAKE_CLAUDE_VERSION`: answer to `--version`.
//! - `FAKE_CLAUDE_LOG`: each call appends its argv and `CLAUDE_CONFIG_DIR`,
//!   so tests can assert what reached the child.

use std::io::Write;

fn main() {
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
        "hang" => std::thread::sleep(std::time::Duration::from_secs(60)),
        "exit1" => {
            eprintln!("fake failure");
            std::process::exit(1);
        }
        "garbage" => println!("this is not json"),
        _ => {
            let fixture = std::env::var_os("FAKE_CLAUDE_FIXTURE")
                .and_then(|p| std::fs::read_to_string(p).ok())
                .unwrap_or_else(|| "[]".into());
            print!("{fixture}");
        }
    }
}
