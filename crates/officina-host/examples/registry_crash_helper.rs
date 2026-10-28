//! Plays the window's registry writer in a process a test kills (stories
//! 3.1 AC 8 and 3.2 AC 11). Never part of a shipped binary.
//!
//! - `registry_crash_helper hold <officina home> <now_ms>`: opens the
//!   writer, records one value under every setting key, prints `ready` and
//!   holds the connection open until it is killed, so the test can read a
//!   registry whose writer died with its WAL in place.
//! - `registry_crash_helper ingest <officina home> <now_ms> [<line>]`: opens
//!   the writer, prints `started`, and ingests the home's event files; with
//!   `<line>`, it prints `stalled` and waits to be killed once it has read
//!   that many lines, inside the transaction of the chunk holding the last
//!   one (D-89). Prints `done` when the pass ends.
//! - `registry_crash_helper confirm <officina home> <now_ms> [<item>]`
//!   (story 3.7 AC 3): opens the writer, prints `started`, ingests the
//!   home's event files, then confirms the triage rows in batches of
//!   [`BATCH`], each unfiled row filed under a subject of its own, until
//!   none is left; after each commit it prints `ack <key>` for every item
//!   confirmed, and `done` at the end. With `<item>`, it prints `stalled`
//!   and waits to be killed once that item (counted from 0 across the
//!   batches of this run) is applied, inside its batch's transaction.

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use officina_core::registry::confirm::{ConfirmItem, FileUnder, ItemOutcome};
use officina_core::registry::types::{SettingKey, Source, Verdict};
use officina_host::registry::ingest::IngestContext;
use officina_host::registry::store::{Writer, open_writer};

/// Rows confirmed per transaction in mode `confirm`.
const BATCH: usize = 3;

fn say(word: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{word}");
    let _ = out.flush();
}

fn wait_to_be_killed() -> ! {
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

fn writer(home: &Path, now_ms: i64) -> Option<Writer> {
    let writer = open_writer(home, &[], now_ms).writer();
    if writer.is_none() {
        eprintln!("registry_crash_helper: the registry did not open writable");
    }
    writer
}

fn hold(home: &Path, now_ms: i64) -> ExitCode {
    let Some(mut writer) = writer(home, now_ms) else {
        return ExitCode::FAILURE;
    };
    for key in SettingKey::ALL {
        let value = format!("\"held {}\"", key.code());
        if let Err(e) = writer.set_setting(*key, &value, Source::Rules, now_ms) {
            eprintln!("registry_crash_helper: {e}");
            return ExitCode::FAILURE;
        }
    }
    say("ready");
    wait_to_be_killed()
}

fn ingest(home: &Path, now_ms: i64, stall_at: Option<u64>) -> ExitCode {
    let Some(mut writer) = writer(home, now_ms) else {
        return ExitCode::FAILURE;
    };
    say("started");
    let events = home.join("events");
    let ctx = IngestContext {
        events_dir: &events,
        officina_home: home,
        projects: &[],
        no_excerpts: false,
    };
    let ingested = writer.ingest_observed(&ctx, now_ms, &mut |read| {
        if Some(read) == stall_at {
            say("stalled");
            wait_to_be_killed();
        }
    });
    match ingested {
        Ok(_) => {
            say("done");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("registry_crash_helper: {e}");
            ExitCode::FAILURE
        }
    }
}

fn confirm(home: &Path, now_ms: i64, stall_at: Option<usize>) -> ExitCode {
    let Some(mut writer) = writer(home, now_ms) else {
        return ExitCode::FAILURE;
    };
    say("started");
    let events = home.join("events");
    let ctx = IngestContext {
        events_dir: &events,
        officina_home: home,
        projects: &[],
        no_excerpts: false,
    };
    if let Err(e) = writer.ingest(&ctx, now_ms) {
        eprintln!("registry_crash_helper: {e}");
        return ExitCode::FAILURE;
    }
    let mut applied = 0;
    loop {
        let rows = match writer.view(None, &events, now_ms) {
            Ok(view) => view.triage,
            Err(e) => {
                eprintln!("registry_crash_helper: {e}");
                return ExitCode::FAILURE;
            }
        };
        if rows.is_empty() {
            say("done");
            return ExitCode::SUCCESS;
        }
        let items: Vec<ConfirmItem> = rows
            .iter()
            .take(BATCH)
            .enumerate()
            .map(|(i, row)| ConfirmItem {
                key: row.key.clone(),
                shown: row.proposal,
                verdict: if i % 2 == 0 {
                    Verdict::Finished
                } else {
                    Verdict::ToResume
                },
                routine: false,
                file_under: Some(FileUnder::New {
                    name: format!("subject of {}", row.key),
                }),
            })
            .collect();
        let answers = writer.confirm_triage_observed(&items, now_ms, &mut |_| {
            if Some(applied) == stall_at {
                say("stalled");
                wait_to_be_killed();
            }
            applied += 1;
        });
        match answers {
            Ok(answers) => {
                for answer in answers {
                    if matches!(answer.outcome, ItemOutcome::Confirmed { .. }) {
                        say(&format!("ack {}", answer.key));
                    }
                }
            }
            Err(e) => {
                eprintln!("registry_crash_helper: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mode, home, now_ms, rest) = match args.as_slice() {
        [mode, home, now_ms, rest @ ..] => (mode.as_str(), Path::new(home), now_ms.parse(), rest),
        _ => {
            eprintln!(
                "usage: registry_crash_helper hold|ingest|confirm <officina home> <now_ms> [<line or item>]"
            );
            return ExitCode::from(64);
        }
    };
    let Ok(now_ms) = now_ms else {
        return ExitCode::from(64);
    };
    match (mode, rest) {
        ("hold", []) => hold(home, now_ms),
        ("ingest", []) => ingest(home, now_ms, None),
        ("ingest", [line]) => match line.parse() {
            Ok(line) => ingest(home, now_ms, Some(line)),
            Err(_) => ExitCode::from(64),
        },
        ("confirm", []) => confirm(home, now_ms, None),
        ("confirm", [item]) => match item.parse() {
            Ok(item) => confirm(home, now_ms, Some(item)),
            Err(_) => ExitCode::from(64),
        },
        _ => ExitCode::from(64),
    }
}
