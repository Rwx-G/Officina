//! A logging `git` for tests (D-223, review F38). Never shipped: `test =
//! false`. Configured by `git-logger.json` next to the executable
//! (`{"git": "<the real git>"}`), it appends its argument vector as one
//! JSON line to `git-logger.log` beside it, then runs the real git with the
//! same arguments, stdio inherited, and exits with its code. Tests point
//! `OFFICINA_GIT_BIN` at it to show that a worker-made value never reaches
//! git as an argument.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

fn beside(name: &str) -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|me| me.parent().map(Path::to_path_buf))
        .unwrap_or_default()
        .join(name)
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(beside("git-logger.log"))
    {
        let _ = writeln!(file, "{}", serde_json::json!({ "argv": argv }));
    }
    let config: serde_json::Value = std::fs::read(beside("git-logger.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let Some(git) = config["git"].as_str() else {
        eprintln!("officina-git-logger: git-logger.json names no git");
        std::process::exit(2);
    };
    let status = Command::new(git).args(&argv).status();
    std::process::exit(status.ok().and_then(|s| s.code()).unwrap_or(1));
}
