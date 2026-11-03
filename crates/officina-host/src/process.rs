//! What every child process Officina starts shares: no console window, one
//! way to wait for it against a deadline, and one way to run it contained
//! (its tree in a job, its pipes drained, a deadline and a grace).

use std::cell::RefCell;
use std::io::{Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
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
    wait_unless(child, deadline, None).unwrap_or(Waited::Deadline)
}

/// [`wait_until`], ending early with `None` once `watch` fired.
fn wait_unless(child: &mut Child, deadline: Instant, watch: Option<&Watch>) -> Option<Waited> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(Waited::Exited(status)),
            Ok(None) if watch.is_some_and(Watch::fired) => return None,
            Ok(None) if Instant::now() >= deadline => return Some(Waited::Deadline),
            Ok(None) => std::thread::sleep(POLL_TICK),
            Err(e) => return Some(Waited::Lost(e)),
        }
    }
}

/// What ends the children a job thread is running before their deadline
/// (AD-116): Stop all fires it, and every child a thread watching it had
/// started by then is ended with its tree. A child started afterwards is
/// not, so the job's own clean-up still runs.
#[derive(Debug, Default)]
pub struct Interrupts {
    fired: AtomicU64,
}

impl Interrupts {
    /// Ends every child the threads watching `self` are running now.
    pub fn fire(&self) {
        self.fired.fetch_add(1, Ordering::SeqCst);
    }
}

thread_local! {
    static WATCHED: RefCell<Option<Arc<Interrupts>>> = const { RefCell::new(None) };
}

/// Makes the calling thread's children end when `interrupts` fires: a job
/// thread calls it once, before its first job.
pub fn watch_interrupts(interrupts: Arc<Interrupts>) {
    WATCHED.with(|watched| *watched.borrow_mut() = Some(interrupts));
}

/// The calling thread's interrupts as they stand when a child starts.
struct Watch {
    interrupts: Arc<Interrupts>,
    at: u64,
}

impl Watch {
    fn now() -> Option<Watch> {
        WATCHED.with(|watched| {
            watched.borrow().as_ref().map(|interrupts| Watch {
                at: interrupts.fired.load(Ordering::SeqCst),
                interrupts: Arc::clone(interrupts),
            })
        })
    }

    fn fired(&self) -> bool {
        self.interrupts.fired.load(Ordering::SeqCst) != self.at
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
    /// What a child that exited 0 while a process it started still holds
    /// its output pipes gives: `true` keeps what was read by the end of
    /// the grace (`claude --bg` and the supervisor it may start, D-128),
    /// `false` fails with [`RunError::OutputHeld`].
    pub keep_held_output: bool,
}

/// A child that ran to its exit.
#[derive(Debug)]
pub struct Contained {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    /// True when the child wrote more than [`RunLimits::stdout_cap`].
    pub stdout_over: bool,
    /// At most [`RunLimits::stderr_cap`]; empty when the pipe stayed open
    /// past the grace, unless [`RunLimits::keep_held_output`] keeps what
    /// was read.
    pub stderr: Vec<u8>,
    /// True when a process the child started still held the output past
    /// the grace, and what was read by then is kept.
    pub output_held: bool,
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
    /// Stop all ended it and its tree (AD-116).
    #[error("stopped by Stop all; it and its tree were ended")]
    Interrupted,
}

/// One output pipe drained on a thread of its own: what it kept once the
/// pipe closed, and what it kept so far.
struct Drain {
    done: Receiver<()>,
    kept: Arc<Mutex<(Vec<u8>, bool)>>,
}

impl Drain {
    /// What was kept, and whether more came than the cap.
    fn kept(&self) -> (Vec<u8>, bool) {
        self.kept
            .lock()
            .map(|kept| kept.clone())
            .unwrap_or_default()
    }
}

/// Reads `pipe` to its end on a thread of its own, keeping the first
/// `keep` bytes and discarding the rest, so a child writing more never
/// blocks on a full pipe (D-97); signals when the pipe closed. A pipe that
/// never closes keeps its thread blocked; the caller stops waiting, reads
/// what was kept so far or nothing, and abandons it.
fn drain(pipe: Option<impl Read + Send + 'static>, keep: usize) -> Drain {
    let (sender, done) = mpsc::channel();
    let kept = Arc::new(Mutex::new((Vec::new(), false)));
    let shared = Arc::clone(&kept);
    std::thread::spawn(move || {
        if let Some(mut pipe) = pipe {
            let mut chunk = [0u8; 8192];
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let Ok(mut kept) = shared.lock() else {
                            break;
                        };
                        let room = keep.saturating_sub(kept.0.len());
                        kept.0.extend_from_slice(&chunk[..n.min(room)]);
                        kept.1 |= n > room;
                    }
                }
            }
        }
        let _ = sender.send(());
    });
    Drain { done, kept }
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

/// One line [`read_line_bounded`] read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundedLine {
    /// The line's bytes, without its newline.
    Line(Vec<u8>),
    /// A line past the bound, discarded to its newline (D-216).
    Oversized,
}

/// Reads one line of `reader`, keeping at most `max` bytes: a longer line
/// is discarded to its next newline and reported [`BoundedLine::Oversized`],
/// so no line costs more than `max + 1` bytes of memory (D-216). `None` at
/// the end of the stream; a last line without a newline is still a line.
pub fn read_line_bounded(
    reader: &mut impl std::io::BufRead,
    max: usize,
) -> std::io::Result<Option<BoundedLine>> {
    let mut kept = Vec::new();
    let mut over = false;
    let mut any = false;
    loop {
        let buf = reader.fill_buf()?;
        if buf.is_empty() {
            return Ok(any.then(|| finished(kept, over)));
        }
        any = true;
        let (part, done) = match buf.iter().position(|b| *b == b'\n') {
            Some(at) => (&buf[..at], Some(at + 1)),
            None => (buf, None),
        };
        if !over {
            if kept.len() + part.len() > max {
                over = true;
                kept = Vec::new();
            } else {
                kept.extend_from_slice(part);
            }
        }
        let used = done.unwrap_or(buf.len());
        reader.consume(used);
        if done.is_some() {
            return Ok(Some(finished(kept, over)));
        }
    }
}

fn finished(mut kept: Vec<u8>, over: bool) -> BoundedLine {
    if over {
        return BoundedLine::Oversized;
    }
    if kept.last() == Some(&b'\r') {
        kept.pop();
    }
    BoundedLine::Line(kept)
}

/// Which pipe of a followed child a line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowedPipe {
    Stdout,
    Stderr,
}

/// A child whose lines are being read (AD-109): in a kill-on-close job,
/// ended when this is dropped.
pub struct Following {
    child: Child,
    tree: Tree,
}

impl Following {
    /// Ends the child and its tree.
    pub fn end(mut self) {
        self.tree.end(&mut self.child);
    }
}

impl Drop for Following {
    fn drop(&mut self) {
        self.tree.end(&mut self.child);
    }
}

/// Starts `command` in a kill-on-close job and reads its two pipes on a
/// thread each, line by line through [`read_line_bounded`] at `max_line`
/// (AD-109, D-216), handing each to `on_line`; `on_end` runs once both
/// pipes closed. No deadline: a follower runs as long as its container.
pub fn follow_contained(
    mut command: Command,
    max_line: usize,
    on_line: Arc<dyn Fn(FollowedPipe, BoundedLine) + Send + Sync>,
    on_end: Box<dyn FnOnce() + Send>,
) -> Result<Following, RunError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_console(&mut command);
    let mut child = command.spawn().map_err(RunError::Spawn)?;
    let tree = match Tree::around(&child, JobRule::KillOnClose) {
        Ok(tree) => tree,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(RunError::NotContained(error));
        }
    };
    let (closed, both) = mpsc::channel::<()>();
    let pipes: [(FollowedPipe, Option<Box<dyn Read + Send>>); 2] = [
        (
            FollowedPipe::Stdout,
            child
                .stdout
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
        ),
        (
            FollowedPipe::Stderr,
            child
                .stderr
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
        ),
    ];
    for (pipe, reader) in pipes {
        let on_line = Arc::clone(&on_line);
        let closed = closed.clone();
        std::thread::spawn(move || {
            if let Some(reader) = reader {
                let mut reader = std::io::BufReader::with_capacity(64 * 1024, reader);
                while let Ok(Some(line)) = read_line_bounded(&mut reader, max_line) {
                    on_line(pipe, line);
                }
            }
            let _ = closed.send(());
        });
    }
    drop(closed);
    std::thread::spawn(move || {
        let _ = both.recv();
        let _ = both.recv();
        on_end();
    });
    Ok(Following { child, tree })
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
    let watch = Watch::now();
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
    let status = match wait_unless(&mut child, started + limits.timeout, watch.as_ref()) {
        None => {
            tree.end(&mut child);
            return Err(RunError::Interrupted);
        }
        Some(Waited::Exited(status)) => status,
        Some(Waited::Deadline) => {
            tree.end(&mut child);
            return Err(RunError::Timeout(limits.timeout));
        }
        Some(Waited::Lost(error)) => {
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
    let stdout_closed = stdout.done.recv_timeout(remaining()).is_ok();
    let stderr_closed = stderr.done.recv_timeout(remaining()).is_ok();
    let held = !stdout_closed || !stderr_closed;
    let kept_stderr = if stderr_closed || limits.keep_held_output {
        stderr.kept().0
    } else {
        Vec::new()
    };
    // A failed exit is reported as such even when its output stays held:
    // the caller reads the status first.
    let (stdout, stdout_over) = match (stdout_closed, status.success()) {
        (true, _) => stdout.kept(),
        (false, _) if limits.keep_held_output => stdout.kept(),
        (false, false) => (Vec::new(), false),
        (false, true) => return Err(RunError::OutputHeld),
    };
    Ok(Contained {
        status,
        stdout,
        stdout_over,
        stderr: kept_stderr,
        output_held: held && limits.keep_held_output,
    })
}

/// A child that ran to its exit, with the last bytes of its output.
#[derive(Debug)]
pub struct Tailed {
    pub status: ExitStatus,
    /// The last bytes stdout and stderr wrote, interleaved as they came,
    /// at most the cap given.
    pub tail: Vec<u8>,
}

/// Runs `command` in a kill-on-close job with a null stdin, both output
/// pipes drained into one buffer that keeps only the last `tail_cap`
/// bytes: a build's progress runs to megabytes and its end says why it
/// failed (AD-99's 64 KiB tail). Past `timeout` the child and its tree are
/// ended.
pub fn run_tail(
    mut command: Command,
    timeout: Duration,
    tail_cap: usize,
) -> Result<Tailed, RunError> {
    let started = Instant::now();
    let watch = Watch::now();
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_console(&mut command);
    let mut child = command.spawn().map_err(RunError::Spawn)?;
    let tree = match Tree::around(&child, JobRule::KillOnClose) {
        Ok(tree) => tree,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(RunError::NotContained(error));
        }
    };
    let tail = Arc::new(Mutex::new(std::collections::VecDeque::<u8>::new()));
    let mut done = Vec::new();
    for pipe in [
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    {
        let shared = Arc::clone(&tail);
        let (sender, closed) = mpsc::channel();
        done.push(closed);
        std::thread::spawn(move || {
            let mut pipe = pipe;
            let mut chunk = [0u8; 8192];
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let Ok(mut kept) = shared.lock() else { break };
                        kept.extend(&chunk[..n]);
                        let over = kept.len().saturating_sub(tail_cap);
                        kept.drain(..over);
                    }
                }
            }
            let _ = sender.send(());
        });
    }
    let status = match wait_unless(&mut child, started + timeout, watch.as_ref()) {
        None => {
            tree.end(&mut child);
            return Err(RunError::Interrupted);
        }
        Some(Waited::Exited(status)) => status,
        Some(Waited::Deadline) => {
            tree.end(&mut child);
            return Err(RunError::Timeout(timeout));
        }
        Some(Waited::Lost(error)) => {
            tree.end(&mut child);
            return Err(RunError::Lost(error));
        }
    };
    tree.end(&mut child);
    let drained_by = Instant::now() + Duration::from_millis(500);
    for closed in done {
        let _ = closed.recv_timeout(drained_by.saturating_duration_since(Instant::now()));
    }
    let tail = tail
        .lock()
        .map(|kept| kept.iter().copied().collect())
        .unwrap_or_default();
    Ok(Tailed { status, tail })
}

/// How a streamed child ended (D-218's reader).
#[derive(Debug)]
pub struct Streamed {
    pub status: ExitStatus,
    /// Every stdout byte the sink received.
    pub bytes: u64,
    /// At most the stderr cap.
    pub stderr: Vec<u8>,
}

/// Why [`stream_contained`] stopped.
#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error(transparent)]
    Run(#[from] RunError),
    /// More than the cap came: the child and its tree were ended.
    #[error("more than {0} bytes came; the copy was stopped")]
    TooLarge(u64),
    /// The sink refused what it read: the child and its tree were ended.
    #[error("the stream was refused: {0}")]
    Refused(String),
}

/// Runs `command` in a kill-on-close job with a null stdin and hands its
/// stdout to `sink` chunk by chunk as it comes, never keeping it: past
/// `cap` bytes, past `timeout`, or when the sink refuses a chunk, the child
/// and its tree are ended at once (D-218, D-623). Stderr is drained and
/// its first `stderr_cap` bytes kept. Used for `docker cp <c>:<path> -`,
/// whose output can be far larger than a captured call's cap.
pub fn stream_contained(
    mut command: Command,
    timeout: Duration,
    cap: u64,
    stderr_cap: usize,
    sink: &mut dyn FnMut(&[u8]) -> Result<(), String>,
) -> Result<Streamed, StreamError> {
    let started = Instant::now();
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_console(&mut command);
    let mut child = command.spawn().map_err(RunError::Spawn)?;
    let tree = match Tree::around(&child, JobRule::KillOnClose) {
        Ok(tree) => tree,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(RunError::NotContained(error).into());
        }
    };
    let stderr = drain(child.stderr.take(), stderr_cap);
    let (chunks, inbox) = mpsc::sync_channel::<Vec<u8>>(16);
    if let Some(mut pipe) = child.stdout.take() {
        std::thread::spawn(move || {
            let mut chunk = vec![0u8; 64 * 1024];
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if chunks.send(chunk[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
    }
    let deadline = started + timeout;
    let watch = Watch::now();
    let mut bytes: u64 = 0;
    loop {
        if watch.as_ref().is_some_and(Watch::fired) {
            tree.end(&mut child);
            return Err(RunError::Interrupted.into());
        }
        let left = deadline
            .saturating_duration_since(Instant::now())
            .min(POLL_TICK);
        if left.is_zero() {
            tree.end(&mut child);
            return Err(RunError::Timeout(timeout).into());
        }
        match inbox.recv_timeout(left) {
            Ok(chunk) => {
                bytes += chunk.len() as u64;
                if bytes > cap {
                    tree.end(&mut child);
                    return Err(StreamError::TooLarge(cap));
                }
                if let Err(why) = sink(&chunk) {
                    tree.end(&mut child);
                    return Err(StreamError::Refused(why));
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
    let status = match wait_unless(&mut child, deadline, watch.as_ref()) {
        None => {
            tree.end(&mut child);
            return Err(RunError::Interrupted.into());
        }
        Some(Waited::Exited(status)) => status,
        Some(Waited::Deadline) => {
            tree.end(&mut child);
            return Err(RunError::Timeout(timeout).into());
        }
        Some(Waited::Lost(error)) => {
            tree.end(&mut child);
            return Err(RunError::Lost(error).into());
        }
    };
    tree.end(&mut child);
    let _ = stderr.done.recv_timeout(Duration::from_millis(500));
    Ok(Streamed {
        status,
        bytes,
        stderr: stderr.kept().0,
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

    fn limits() -> RunLimits {
        RunLimits {
            timeout: Duration::from_secs(30),
            stdout_cap: 1024,
            stderr_cap: 1024,
            grace: Duration::from_millis(500),
            job: JobRule::KillOnClose,
            keep_held_output: false,
        }
    }

    /// Story 6.17 AC 2 (AD-116): firing the interrupts a thread watches
    /// ends the child it is running at once, and a child it starts
    /// afterwards runs to its exit.
    #[test]
    fn firing_ends_the_running_child_and_spares_the_next() {
        let interrupts = Arc::new(Interrupts::default());
        let watched = Arc::clone(&interrupts);
        let (ran, done) = mpsc::channel();
        std::thread::spawn(move || {
            watch_interrupts(watched);
            let started = Instant::now();
            let first = run_contained(cmd("ping -n 30 127.0.0.1"), None, &limits());
            let first_took = started.elapsed();
            let second = run_contained(cmd("exit 4"), None, &limits());
            ran.send((first, first_took, second)).unwrap();
        });
        std::thread::sleep(Duration::from_millis(500));
        interrupts.fire();
        let (first, took, second) = done.recv_timeout(Duration::from_secs(20)).unwrap();
        assert!(matches!(first, Err(RunError::Interrupted)), "{first:?}");
        assert!(took < Duration::from_secs(10), "{took:?}");
        assert_eq!(second.unwrap().status.code(), Some(4));
    }

    /// A thread that watches nothing is never interrupted.
    #[test]
    fn an_unwatched_thread_runs_to_its_exit() {
        Arc::new(Interrupts::default()).fire();
        let ran = run_contained(cmd("exit 5"), None, &limits()).unwrap();
        assert_eq!(ran.status.code(), Some(5));
    }
}

#[cfg(test)]
mod line_tests {
    use super::*;

    /// D-216: a line past the bound is discarded to its newline, counted
    /// as one, and the next line reads whole.
    #[test]
    fn a_long_line_is_skipped_to_its_newline() {
        let text = format!("short\r\n{}\nnext\nlast", "x".repeat(40));
        let mut reader = std::io::BufReader::with_capacity(8, text.as_bytes());
        let mut read = Vec::new();
        while let Some(line) = read_line_bounded(&mut reader, 10).unwrap() {
            read.push(line);
        }
        assert_eq!(
            read,
            vec![
                BoundedLine::Line(b"short".to_vec()),
                BoundedLine::Oversized,
                BoundedLine::Line(b"next".to_vec()),
                BoundedLine::Line(b"last".to_vec()),
            ]
        );
    }
}
