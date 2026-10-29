//! What every child process Officina starts shares: no console window, one
//! way to wait for it against a deadline, and one way to run it contained
//! (its tree in a job, its pipes drained, a deadline and a grace).

use std::io::{Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

/// How often a running child is polled against its deadline.
pub const POLL_TICK: Duration = Duration::from_millis(50);

/// Starts `command` without a console window: Officina runs from the window
/// and from hooks, where a console flashing up for each child would take the
/// focus. Nothing to do off Windows.
pub fn hide_console(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

/// How a wait for a child ended.
#[derive(Debug)]
pub enum Waited {
    Exited(ExitStatus),
    /// Still running at the deadline: ending it is the caller's.
    Deadline,
    /// The child could not be waited for.
    Lost(std::io::Error),
}

/// Waits for `child` until `deadline`, a look every [`POLL_TICK`]. Never
/// kills: what a deadline ends (the child, its job) is the caller's choice.
pub fn wait_until(child: &mut Child, deadline: Instant) -> Waited {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Waited::Exited(status),
            Ok(None) if Instant::now() >= deadline => return Waited::Deadline,
            Ok(None) => std::thread::sleep(POLL_TICK),
            Err(e) => return Waited::Lost(e),
        }
    }
}

/// How many characters of the kept stderr a failure message quotes.
const STDERR_QUOTED_CHARS: usize = 500;

/// What a child's process tree is to Officina (AD-73, D-128 review F10), as
/// [`run_contained`] sets it per call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobRule {
    /// In a job ended with its tree on a timeout only: what the child
    /// leaves running after it answered is not Officina's (a supervisor
    /// `claude agents` starts), and a job that cannot be set up leaves the
    /// child alone.
    TreeOnTimeout,
    /// In a kill-on-close job assigned before anything is written to the
    /// child, or stopped (D-64, D-436); the tree is ended at the exit.
    KillOnClose,
    /// No job: a deadline ends the child alone. What it starts is not
    /// Officina's: a console kind's is the author's, and `claude --bg` may
    /// start the supervisor of every background session (D-128).
    NoJob,
}

/// What [`run_contained`] allows one child.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunLimits {
    /// How long the child may run before it is ended, with its tree as
    /// [`RunLimits::job`] says.
    pub timeout: Duration,
    /// The most stdout kept; more is drained and dropped, and
    /// [`Contained::stdout_over`] says so.
    pub stdout_cap: usize,
    /// The most stderr kept, for a failure's message; more is drained and
    /// dropped.
    pub stderr_cap: usize,
    /// How long the output pipes may stay open once the child exited:
    /// what it wrote is drained within microseconds; only a process it
    /// started and left running, which inherited the pipes, keeps them
    /// open longer.
    pub grace: Duration,
    /// The child's tree: in a job, and which.
    pub job: JobRule,
}

/// A child that ran to its exit.
#[derive(Debug)]
pub struct Contained {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    /// True when the child wrote more than [`RunLimits::stdout_cap`].
    pub stdout_over: bool,
    /// At most [`RunLimits::stderr_cap`]; empty when the pipe stayed open
    /// past the grace.
    pub stderr: Vec<u8>,
}

impl Contained {
    /// The start of stderr a failure message quotes, trimmed.
    pub fn quoted_stderr(&self) -> String {
        let stderr: String = String::from_utf8_lossy(&self.stderr)
            .chars()
            .take(STDERR_QUOTED_CHARS)
            .collect();
        stderr.trim().to_owned()
    }

    /// `exited with <code>: <start of stderr>`, for a child that failed.
    pub fn failure(&self) -> String {
        format!(
            "exited with {}: {}",
            exit_code_text(self.status),
            self.quoted_stderr()
        )
    }
}

/// The exit code of `status`, or "a signal" when it has none.
pub fn exit_code_text(status: ExitStatus) -> String {
    status
        .code()
        .map_or_else(|| "a signal".to_owned(), |c| c.to_string())
}

/// Why [`run_contained`] gave no exit.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("could not start: {0}")]
    Spawn(std::io::Error),
    #[error(
        "could not be put in a job of its own, so it was stopped before anything was sent: {0}"
    )]
    NotContained(std::io::Error),
    #[error("no exit within {} s; it and its tree were ended", .0.as_secs())]
    Timeout(Duration),
    #[error("lost track of it: {0}")]
    Lost(std::io::Error),
    #[error("it exited but a process it started still holds its output")]
    OutputHeld,
}

/// Reads `pipe` to its end on a thread of its own, keeping the first
/// `keep` bytes and discarding the rest, so a child writing more never
/// blocks on a full pipe (D-97); sends what it kept and whether more came.
/// A pipe that never closes keeps its thread blocked; the caller stops
/// waiting and abandons it.
fn drain(pipe: Option<impl Read + Send + 'static>, keep: usize) -> Receiver<(Vec<u8>, bool)> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut over = false;
        if let Some(mut pipe) = pipe {
            let mut chunk = [0u8; 8192];
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let room = keep.saturating_sub(kept.len());
                        kept.extend_from_slice(&chunk[..n.min(room)]);
                        over |= n > room;
                    }
                }
            }
        }
        let _ = sender.send((kept, over));
    });
    receiver
}

/// The processes of one child. On Windows the child goes into a job right
/// after it starts, unless its rule is [`JobRule::NoJob`], so ending the
/// job ends what it started too (E13). What
/// the child starts in the instant before the assignment escapes it (D-64):
/// the programs Officina runs this way take far longer than that before
/// they start anything.
struct Tree {
    #[cfg(windows)]
    job: Option<crate::platform::windows::Job>,
}

impl Tree {
    fn around(child: &Child, rule: JobRule) -> std::io::Result<Tree> {
        #[cfg(windows)]
        {
            let kill_on_close = match rule {
                JobRule::NoJob => return Ok(Tree { job: None }),
                JobRule::KillOnClose => true,
                JobRule::TreeOnTimeout => false,
            };
            let job = crate::platform::windows::Job::new(kill_on_close)
                .and_then(|job| job.assign(child).map(|()| job));
            match job {
                Ok(job) => Ok(Tree { job: Some(job) }),
                Err(error) if kill_on_close => Err(error),
                Err(_) => Ok(Tree { job: None }),
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (child, rule);
            Ok(Tree {})
        }
    }

    /// Ends whatever the child left running, and the child itself.
    fn end(&self, child: &mut Child) {
        #[cfg(windows)]
        if let Some(job) = &self.job {
            let _ = job.terminate(1);
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Runs `command` within `limits`, with `stdin` written on a thread of its
/// own (a child that never reads it must not hold the caller on a full
/// pipe), or a null stdin. Both output pipes are drained on threads of
/// their own while the child is polled against its deadline: a child that
/// fills a pipe buffer would otherwise never exit. Returns within about
/// the timeout plus the grace whatever the child leaves running.
pub fn run_contained(
    mut command: Command,
    stdin: Option<Vec<u8>>,
    limits: &RunLimits,
) -> Result<Contained, RunError> {
    let started = Instant::now();
    command
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_console(&mut command);
    let mut child = command.spawn().map_err(RunError::Spawn)?;
    let tree = match Tree::around(&child, limits.job) {
        Ok(tree) => tree,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(RunError::NotContained(error));
        }
    };
    if let (Some(bytes), Some(mut pipe)) = (stdin, child.stdin.take()) {
        std::thread::spawn(move || {
            let _ = pipe.write_all(&bytes);
        });
    }
    let stdout = drain(child.stdout.take(), limits.stdout_cap);
    let stderr = drain(child.stderr.take(), limits.stderr_cap);
    let status = match wait_until(&mut child, started + limits.timeout) {
        Waited::Exited(status) => status,
        Waited::Deadline => {
            tree.end(&mut child);
            return Err(RunError::Timeout(limits.timeout));
        }
        Waited::Lost(error) => {
            tree.end(&mut child);
            return Err(RunError::Lost(error));
        }
    };
    if limits.job == JobRule::KillOnClose {
        // A process the child left running would hold the pipes open; it
        // is Officina's, so it ends before the output is read.
        tree.end(&mut child);
    }
    let drained_by = Instant::now() + limits.grace;
    let remaining = || drained_by.saturating_duration_since(Instant::now());
    let stdout = stdout.recv_timeout(remaining());
    let (stderr, _) = stderr.recv_timeout(remaining()).unwrap_or_default();
    // A failed exit is reported as such even when its output stays held:
    // the caller reads the status first.
    let (stdout, stdout_over) = match stdout {
        Ok(read) => read,
        Err(_) if !status.success() => (Vec::new(), false),
        Err(_) => return Err(RunError::OutputHeld),
    };
    Ok(Contained {
        status,
        stdout,
        stdout_over,
        stderr,
    })
}

#[cfg(all(test, windows))]
#[expect(
    clippy::disallowed_methods,
    reason = "the tests wait on cmd.exe children"
)]
mod tests {
    use super::*;

    fn cmd(script: &str) -> Command {
        let mut command = Command::new("cmd.exe");
        command
            .args(["/d", "/c", script])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        hide_console(&mut command);
        command
    }

    #[test]
    fn an_exit_before_the_deadline_is_its_status() {
        let mut child = cmd("exit 3").spawn().unwrap();
        let waited = wait_until(&mut child, Instant::now() + Duration::from_secs(30));
        assert!(
            matches!(&waited, Waited::Exited(status) if status.code() == Some(3)),
            "{waited:?}"
        );
    }

    #[test]
    fn a_child_running_at_the_deadline_is_left_running() {
        let mut child = cmd("ping -n 30 127.0.0.1").spawn().unwrap();
        let waited = wait_until(&mut child, Instant::now() + POLL_TICK);
        assert!(matches!(waited, Waited::Deadline), "{waited:?}");
        assert!(child.try_wait().unwrap().is_none(), "not killed");
        child.kill().unwrap();
        child.wait().unwrap();
    }
}
