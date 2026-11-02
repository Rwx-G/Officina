//! `officina-entry`'s subcommands (AD-108, AD-110, AD-117, D-608, D-609): fixed
//! code a workshop container runs. Unix only: symbolic links, file modes
//! and coreutils' `timeout`.

pub mod auth_status;
pub mod battery;
pub mod decoy;
pub mod inventory;
pub mod pack;
pub mod run;
pub mod scan;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use officina_core::workshop::run::EntryLine;

/// The folders a container sees, under one root: `/` in a container, a
/// temporary folder in the tests (`--root`).
#[derive(Debug, Clone)]
pub struct Layout {
    root: PathBuf,
    /// Claude Code; `/opt/officina/bin/claude` in the image, a fake in the
    /// tests (`--claude`).
    pub claude: PathBuf,
}

impl Layout {
    pub fn new(root: Option<&Path>, claude: Option<&Path>) -> Layout {
        let root = root.map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
        let claude = claude.map_or_else(
            || PathBuf::from(officina_core::workshop::vector::CLAUDE_PROGRAM),
            Path::to_path_buf,
        );
        Layout { root, claude }
    }

    /// `path`, a container path (`/work/...`), under the root.
    pub fn at(&self, path: &str) -> PathBuf {
        self.root.join(path.trim_start_matches('/'))
    }

    pub fn in_dir(&self) -> PathBuf {
        self.at(officina_core::workshop::launch::IN_DIR)
    }

    pub fn out_dir(&self) -> PathBuf {
        self.at(OUT_DIR)
    }

    pub fn steps_file(&self) -> PathBuf {
        self.at(STEPS_FILE)
    }
}

/// Where the packer writes the package (AD-110).
pub const OUT_DIR: &str = "/work/.officina/out";
/// Where `run` records its steps for the packer's manifest; worker-writable,
/// so untrusted like the rest of the manifest (AD-110).
pub const STEPS_FILE: &str = "/work/.officina/steps.jsonl";

/// Prints `line` on stderr, cut to its bound by core, and appends it to
/// the steps file when `layout` is given.
pub fn report(line: &EntryLine, layout: Option<&Layout>) {
    let text = line.to_line();
    eprintln!("{text}");
    if let Some(layout) = layout
        && let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(layout.steps_file())
    {
        let _ = writeln!(file, "{text}");
    }
}

/// `git` with the flags every call of the entry point takes: no hook, no
/// filesystem monitor, no system or global configuration (AD-110).
pub fn git(dir: Option<&Path>) -> Command {
    let mut command = Command::new("git");
    if let Some(dir) = dir {
        command.arg("-C").arg(dir);
    }
    command
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
        ])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null");
    command
}

/// The value after `flag` in `args`.
pub fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].as_str())
}
