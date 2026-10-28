//! Resolving and running the `claude` binary (AD-6).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use officina_core::agents::is_cmd_shim;

use crate::paths::{ENV_CLAUDE_BIN, ENV_CLAUDE_CONFIG_DIR, user_home};
use crate::process::{RunError, RunLimits, run_contained};

/// Deadline of one `claude agents` call.
pub const AGENTS_TIMEOUT: Duration = Duration::from_secs(5);

/// How long the output pipes may stay open once `claude` has exited. Short,
/// because the collector thread, which builds the board, waits on it; only
/// a process `claude` started and left running keeps them open longer.
pub const OUTPUT_GRACE: Duration = Duration::from_millis(500);

/// Cap on captured stdout; `claude agents --all` stays far below it.
const STDOUT_CAP: usize = 8 * 1024 * 1024;

/// Why a `claude` call produced no usable output.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClaudeError {
    #[error("claude not found (searched OFFICINA_CLAUDE_BIN, PATH, ~/.local/bin)")]
    NotFound,
    #[error(
        "claude resolves to {0}, a shim only cmd.exe runs, and Officina runs none: install the native claude.exe or set OFFICINA_CLAUDE_BIN to it"
    )]
    CmdShim(String),
    #[error("claude could not start: {0}")]
    Spawn(String),
    #[error("claude did not answer within {0} s")]
    Timeout(u64),
    #[error("claude exited but a process it started still holds its output")]
    OutputHeld,
    #[error("claude exited with {code}: {stderr}")]
    Failed { code: String, stderr: String },
}

/// Locates `claude`: `OFFICINA_CLAUDE_BIN`, then `PATH` (`claude.exe`, then
/// `claude.cmd` on Windows), then `~/.local/bin`, where the native installer
/// puts it (E8) and where a GUI started with a stale `PATH` still finds it.
pub fn resolve_claude() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os(ENV_CLAUDE_BIN).filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(explicit));
    }
    let names: &[&str] = if cfg!(windows) {
        &["claude.exe", "claude.cmd"]
    } else {
        &["claude"]
    };
    let path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    for name in names {
        // Relative PATH entries resolve against whatever directory Officina
        // was started from; they are skipped so a planted `claude.exe` there
        // is never run.
        if let Some(found) = path_dirs
            .iter()
            .filter(|d| d.is_absolute())
            .map(|d| d.join(name))
            .find(|p| p.is_file())
        {
            return Some(found);
        }
    }
    let local = user_home()?.join(".local").join("bin").join(names[0]);
    local.is_file().then_some(local)
}

/// Runs `claude <args>` with a deadline and returns its stdout; a `.cmd`
/// or `.bat` shim is refused before anything starts ([`is_cmd_shim`]).
/// Returns within about `timeout` plus [`OUTPUT_GRACE`] whatever `claude`
/// leaves running: a timeout ends `claude` and the processes it started,
/// while a process it left running after it answered is not Officina's to
/// end and only costs the grace.
pub fn run(
    binary: &Path,
    args: &[&str],
    config_dir: Option<&Path>,
    timeout: Duration,
) -> Result<String, ClaudeError> {
    if is_cmd_shim(&binary.to_string_lossy()) {
        return Err(ClaudeError::CmdShim(binary.display().to_string()));
    }
    let mut command = Command::new(binary);
    command.args(args);
    if let Some(dir) = config_dir {
        command.env(ENV_CLAUDE_CONFIG_DIR, dir);
    }
    let limits = RunLimits {
        timeout,
        stdout_cap: STDOUT_CAP,
        grace: OUTPUT_GRACE,
        owns_tree: false,
    };
    let ran = run_contained(command, None, &limits).map_err(|error| match error {
        RunError::Timeout(_) => ClaudeError::Timeout(timeout.as_secs()),
        RunError::OutputHeld => ClaudeError::OutputHeld,
        RunError::Spawn(e) | RunError::Lost(e) | RunError::NotContained(e) => {
            ClaudeError::Spawn(e.to_string())
        }
    })?;
    if !ran.status.success() {
        return Err(ClaudeError::Failed {
            code: crate::process::exit_code_text(ran.status),
            stderr: ran.quoted_stderr(),
        });
    }
    Ok(String::from_utf8_lossy(&ran.stdout).into_owned())
}

/// `claude agents --json --all`.
pub fn agents_json(binary: &Path, config_dir: Option<&Path>) -> Result<String, ClaudeError> {
    run(
        binary,
        &["agents", "--json", "--all"],
        config_dir,
        AGENTS_TIMEOUT,
    )
}

/// `claude --version`, first line trimmed.
pub fn version(binary: &Path, config_dir: Option<&Path>) -> Result<String, ClaudeError> {
    run(binary, &["--version"], config_dir, AGENTS_TIMEOUT)
        .map(|v| v.lines().next().unwrap_or_default().trim().to_owned())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// Penetration audit M1: `claude agents` and `claude --version` refuse a
    /// shim as the triage call does, before anything starts.
    #[test]
    fn every_call_refuses_a_cmd_shim_before_it_starts() {
        let dir = tempfile::tempdir().unwrap();
        let ran = dir.path().join("ran.txt");
        for name in ["claude.cmd", "claude.BAT", "claude.cmd."] {
            let shim = dir.path().join(name.trim_end_matches('.'));
            std::fs::write(&shim, format!("@echo ran> \"{}\"\r\n", ran.display())).unwrap();
            let named = dir.path().join(name);
            assert!(
                matches!(agents_json(&named, None), Err(ClaudeError::CmdShim(_))),
                "{name}"
            );
            assert!(
                matches!(version(&named, None), Err(ClaudeError::CmdShim(_))),
                "{name}"
            );
        }
        assert!(!ran.exists(), "a shim ran");
        // Positive control: the same script runs when cmd.exe is asked.
        let shim = dir.path().join("claude.cmd");
        let status = std::process::Command::new("cmd.exe")
            .args(["/d", "/c"])
            .arg(&shim)
            .status()
            .unwrap();
        assert!(status.success() && ran.exists());
    }
}
