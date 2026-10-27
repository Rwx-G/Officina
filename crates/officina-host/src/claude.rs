//! Resolving and running the `claude` binary (AD-6).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use crate::paths::{ENV_CLAUDE_BIN, ENV_CLAUDE_CONFIG_DIR, user_home};

/// Deadline of one `claude agents` call.
pub const AGENTS_TIMEOUT: Duration = Duration::from_secs(5);

/// How long the output pipes may stay open once `claude` has exited. What it
/// wrote is drained within microseconds; only a process it started and left
/// running, which inherited the pipes, keeps them open longer.
pub const OUTPUT_GRACE: Duration = Duration::from_millis(500);

/// Cap on captured stdout; `claude agents --all` stays far below it.
const STDOUT_CAP: u64 = 8 * 1024 * 1024;

/// Why a `claude` call produced no usable output.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClaudeError {
    #[error("claude not found (searched OFFICINA_CLAUDE_BIN, PATH, ~/.local/bin)")]
    NotFound,
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

/// Builds a `Command` for `claude`, routing `.cmd` shims through `cmd.exe`
/// and never opening a console window from the GUI.
fn command(binary: &Path, config_dir: Option<&Path>) -> Command {
    let is_cmd = binary
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"));
    let mut cmd = if is_cmd {
        let mut c = Command::new("cmd.exe");
        c.arg("/d").arg("/c").arg(binary);
        c
    } else {
        Command::new(binary)
    };
    if let Some(dir) = config_dir {
        cmd.env(ENV_CLAUDE_CONFIG_DIR, dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Reads `pipe` to its end, at most `cap` bytes, on a thread of its own, and
/// sends what it read once the pipe closes. A pipe that never closes keeps
/// its thread blocked; the caller stops waiting and abandons it.
fn read_on_thread(pipe: Option<impl Read + Send + 'static>, cap: u64) -> Receiver<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(pipe) = pipe {
            let _ = pipe.take(cap).read_to_end(&mut buf);
        }
        let _ = sender.send(buf);
    });
    receiver
}

/// Runs `claude <args>` with a deadline and returns its stdout. Returns
/// within about `timeout` plus [`OUTPUT_GRACE`] whatever `claude` leaves running:
/// the kill on timeout reaches only `claude` itself, and a process it started
/// can hold the output pipes open after it exits.
pub fn run(
    binary: &Path,
    args: &[&str],
    config_dir: Option<&Path>,
    timeout: Duration,
) -> Result<String, ClaudeError> {
    let mut child = command(binary, config_dir)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ClaudeError::Spawn(e.to_string()))?;

    // Pipes are drained on their own threads: a child that fills a pipe
    // buffer while we only poll `try_wait` would otherwise never exit.
    let stdout = read_on_thread(child.stdout.take(), STDOUT_CAP);
    let stderr = read_on_thread(child.stderr.take(), 64 * 1024);

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ClaudeError::Timeout(timeout.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(ClaudeError::Spawn(e.to_string())),
        }
    };
    let drained_by = Instant::now() + OUTPUT_GRACE;
    let remaining = || drained_by.saturating_duration_since(Instant::now());
    if !status.success() {
        let err = stderr.recv_timeout(remaining()).unwrap_or_default();
        let stderr: String = String::from_utf8_lossy(&err).chars().take(500).collect();
        return Err(ClaudeError::Failed {
            code: status
                .code()
                .map_or_else(|| "a signal".to_owned(), |c| c.to_string()),
            stderr: stderr.trim().to_owned(),
        });
    }
    let out = stdout
        .recv_timeout(remaining())
        .map_err(|_| ClaudeError::OutputHeld)?;
    Ok(String::from_utf8_lossy(&out).into_owned())
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
