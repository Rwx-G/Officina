//! Plays the window's registry writer in a process a test kills (story 3.1
//! AC 8). Never part of a shipped binary.
//!
//! `registry_crash_helper hold <officina home> <now_ms>`: opens the writer,
//! records one value under every setting key, prints `ready` and holds the
//! connection open until it is killed, so the test can read a registry
//! whose writer died with its WAL in place.

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use officina_core::registry::types::{SettingKey, Source};
use officina_host::registry::store::open_writer;

/// The value `hold` records under `key`, which the test expects back.
fn held_value(key: SettingKey) -> String {
    format!("\"held {}\"", key.code())
}

fn hold(home: &Path, now_ms: i64) -> ExitCode {
    let opened = open_writer(home, &[], now_ms);
    let Some(mut writer) = opened.writer() else {
        eprintln!("registry_crash_helper: the registry did not open writable");
        return ExitCode::FAILURE;
    };
    for key in SettingKey::ALL {
        if let Err(e) = writer.set_setting(*key, &held_value(*key), Source::Rules, now_ms) {
            eprintln!("registry_crash_helper: {e}");
            return ExitCode::FAILURE;
        }
    }
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "ready");
    let _ = out.flush();
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [mode, home, now_ms] if mode == "hold" => match now_ms.parse() {
            Ok(now_ms) => hold(Path::new(home), now_ms),
            Err(_) => ExitCode::from(64),
        },
        _ => {
            eprintln!("usage: registry_crash_helper hold <officina home> <now_ms>");
            ExitCode::from(64)
        }
    }
}
