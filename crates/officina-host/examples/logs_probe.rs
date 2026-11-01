//! The meeting's logs on a real background session (story 5.10 AC 4,
//! D-126, E23, E94): runs the exact `Logs` vector against the `claude`
//! Officina resolves, plays it on the meeting's screen (D-1020), and prints
//! the counts, then the text the panel would show, for the owner to judge
//! readable. Run by hand, never by a test, never part of a shipped binary:
//!
//! `cargo run -p officina-host --example logs_probe -- <background id>`
//!
//! It reads the session's output and starts nothing else; the session is
//! left as it was.

use std::process::ExitCode;
use std::time::Instant;

use officina_core::meeting::{Logs, LogsRead, logs_of};
use officina_core::spawn::BackgroundId;
use officina_host::spawn::{BaseEnv, logs, resolve_claude};

fn main() -> ExitCode {
    let Some(id) = std::env::args()
        .nth(1)
        .as_deref()
        .and_then(BackgroundId::parse)
    else {
        eprintln!("usage: logs_probe <background id> (from `claude agents --json`, field `id`)");
        return ExitCode::from(64);
    };
    let Some(claude) = resolve_claude() else {
        eprintln!("claude not found");
        return ExitCode::FAILURE;
    };
    let started = Instant::now();
    let read = match logs(&claude, &id, None, &BaseEnv::from_process()) {
        Ok(stdout) => {
            println!(
                "claude logs: exit 0 in {} ms, {} bytes, {} ESC bytes",
                started.elapsed().as_millis(),
                stdout.len(),
                stdout.iter().filter(|b| **b == 0x1b).count()
            );
            LogsRead::Output(stdout)
        }
        Err(error) => {
            println!("claude logs failed: {error}");
            LogsRead::Failed(error.to_string())
        }
    };
    match logs_of(Some(&read)) {
        Logs::Shown(shown) => {
            let text = serde_json::to_value(&shown.text).unwrap_or_default();
            let text = text.as_str().unwrap_or_default();
            println!(
                "rendered: {} sequences read, final screen {} bytes kept, {} dropped from the start, first error line {}",
                shown.escapes_stripped,
                text.len(),
                shown.dropped_bytes,
                shown
                    .first_error_line
                    .map_or_else(|| "none".to_owned(), |line| line.to_string())
            );
            println!("----- what the meeting shows -----");
            print!("{text}");
            println!("----- end -----");
            ExitCode::SUCCESS
        }
        Logs::Unavailable { reason } => {
            println!("no logs section: {reason}");
            ExitCode::FAILURE
        }
    }
}
