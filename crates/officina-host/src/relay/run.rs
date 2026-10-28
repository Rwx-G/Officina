//! `officina run` (AD-29 to AD-32): reads what the checks need, refuses or
//! starts the approved argument vector with a timeout and an output budget,
//! and journals the run. It is the relay's authority over what runs and
//! fails closed, unlike the gate (AD-28): every doubt is a refusal.
//!
//! Exit codes a shell already reads: the program's own, full Windows value
//! included; 126 refused; 124 timed out; 127 not started; 64 usage. A
//! program can exit 126 or 124 itself: the stderr line and the journal tell
//! them apart. Officina itself never exits 2.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use officina_core::relay::StreamHash;
use officina_core::relay::argv::Launch;
use officina_core::relay::journal::{
    JournalEntry, Refused, RunFinished, RunOutcome, RunStarted, StreamSummary,
};
pub use officina_core::relay::run::{EXIT_NOT_STARTED, EXIT_REFUSED, EXIT_TIMEOUT};
use officina_core::relay::run::{
    FloorState, Refusal, RunInputs, RunPlan, SnapshotState, list_text, plan,
};

use super::home::{ApprovedState, RelayHome, load_relay_floor};
use super::journal;
use crate::files::now_ms;

/// The session that called, as Claude Code's Bash tool sets it (E13).
pub const ENV_SESSION_ID: &str = "CLAUDE_CODE_SESSION_ID";

/// How often a running action is polled against its deadline (AD-3).
pub const WAIT_TICK: Duration = Duration::from_millis(50);

/// How long the output pipes may stay open once the program has exited: a
/// process it left running holds them, and is ended with the job when
/// `officina run` exits.
pub const OUTPUT_GRACE: Duration = Duration::from_millis(500);

const CHUNK: usize = 8192;

/// What `officina run` was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunArgs {
    List,
    Action {
        action: String,
        params: BTreeMap<String, String>,
    },
}

/// Parses the arguments after `run` by hand (AD-29): `--list` alone, or an
/// action then `name=value` tokens split at the first `=`. A token without
/// `=`, a duplicate name, a missing action or an unknown option is a usage
/// error, which the caller reports with exit 64.
pub fn parse_args(args: &[String]) -> Result<RunArgs, String> {
    let Some((action, tokens)) = args.split_first() else {
        return Err("missing action".to_owned());
    };
    if action == "--list" {
        return if tokens.is_empty() {
            Ok(RunArgs::List)
        } else {
            Err("--list takes no other argument".to_owned())
        };
    }
    if action.starts_with('-') {
        return Err(format!("unknown option {action}"));
    }
    if action.contains('=') {
        return Err("missing action: the first argument is a parameter".to_owned());
    }
    let mut params = BTreeMap::new();
    for token in tokens {
        let Some((name, value)) = token.split_once('=') else {
            return Err(format!("{token:?} is not name=value"));
        };
        if params.insert(name.to_owned(), value.to_owned()).is_some() {
            return Err(format!("parameter {name} is given twice"));
        }
    }
    Ok(RunArgs::Action {
        action: action.clone(),
        params,
    })
}

/// `CLAUDE_CODE_SESSION_ID`, absent outside Claude Code (AD-29).
pub fn session_id_from_env() -> Option<String> {
    std::env::var(ENV_SESSION_ID).ok().filter(|v| !v.is_empty())
}

/// The directory `wsl.exe` is taken from (AD-30). Empty off Windows, where
/// the `wsl` runner has nothing to start.
fn system_dir() -> String {
    #[cfg(windows)]
    {
        crate::platform::windows::system_directory()
            .map(|dir| dir.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        String::new()
    }
}

/// Reads what the checks need and hands it to `f`.
fn with_inputs<R>(home: &RelayHome, cwd: &str, f: impl FnOnce(&RunInputs<'_>) -> R) -> R {
    let stopped = home.relay_stopped();
    let approved = home.read_approved();
    let floor_path = home.relay_floor(&approved);
    let floor_text = floor_path.to_string_lossy();
    let loaded = load_relay_floor(&floor_path);
    let floor_error = loaded.as_ref().err().map(ToString::to_string);
    let snapshot = match &approved {
        ApprovedState::Missing => SnapshotState::Missing,
        ApprovedState::Unreadable(why) => SnapshotState::Unreadable(why),
        ApprovedState::Valid(approved) => SnapshotState::Valid(approved),
    };
    let floor = match &loaded {
        Ok(relay) => FloorState::Valid {
            relay,
            path: &floor_text,
        },
        Err(_) => FloorState::Unreadable(floor_error.as_deref().unwrap_or_default()),
    };
    let system_dir = system_dir();
    f(&RunInputs {
        stopped,
        snapshot,
        floor,
        cwd,
        system_dir: &system_dir,
    })
}

/// `officina run --list` (D-21).
pub fn list(home: &RelayHome, cwd: &str) -> String {
    with_inputs(home, cwd, list_text)
}

/// `officina run <action> name=value ...`: checks, then runs. Returns the
/// exit code.
pub fn run(
    home: &RelayHome,
    cwd: &str,
    action: &str,
    params: &BTreeMap<String, String>,
    session_id: Option<String>,
) -> i32 {
    match with_inputs(home, cwd, |inputs| plan(inputs, action, params)) {
        Ok(plan) => execute(home, &plan, params, session_id),
        Err(refusal) => refuse(home, &refusal, action, params, session_id),
    }
}

/// One stderr line and one `refused` line (PRD amendment 11). A journal
/// that cannot be written does not add a second line: the refusal stands.
fn refuse(
    home: &RelayHome,
    refusal: &Refusal,
    action: &str,
    params: &BTreeMap<String, String>,
    session_id: Option<String>,
) -> i32 {
    let code = serde_json::to_value(refusal.reason)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    eprintln!("officina: refused: {} ({code})", refusal.detail);
    let line = JournalEntry::Refused(Refused {
        project: refusal.project.clone(),
        session_id,
        action: action.to_owned(),
        params: Some(params.clone()),
        reason: refusal.reason,
        detail: refusal.detail.clone(),
    });
    let _ = journal::append(home, &[line], now_ms());
    EXIT_REFUSED
}

/// The command for a launch (AD-30): the program itself, its arguments as a
/// vector, the project folder as working directory, the declared `env`
/// added and `env_remove` removed, stdin null, no console window.
pub fn command_for(launch: &Launch) -> Command {
    let mut command = Command::new(&launch.program);
    command
        .args(&launch.args)
        .current_dir(&launch.cwd)
        .envs(&launch.env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for key in &launch.env_remove {
        command.env_remove(key);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// The process tree of a run (AD-31). On Windows `officina run` puts itself
/// in a kill-on-close job before it starts anything, so every process of the
/// action is in the job from its start and dies with `officina run`, however
/// it ends. If that assignment fails, the child is assigned right after its
/// start instead. Elsewhere the child alone is killed (Linux is v1).
struct ProcessTree {
    #[cfg(windows)]
    job: Option<crate::platform::windows::Job>,
    #[cfg(windows)]
    contains_self: bool,
}

impl ProcessTree {
    fn new() -> ProcessTree {
        #[cfg(windows)]
        {
            let job = crate::platform::windows::Job::new(true).ok();
            let contains_self = job
                .as_ref()
                .is_some_and(|job| job.assign_current_process().is_ok());
            ProcessTree { job, contains_self }
        }
        #[cfg(not(windows))]
        {
            ProcessTree {}
        }
    }

    fn started(&self, child: &Child) {
        #[cfg(windows)]
        if !self.contains_self
            && let Some(job) = &self.job
        {
            let _ = job.assign(child);
        }
        #[cfg(not(windows))]
        let _ = child;
    }

    /// Ends the whole tree with `code`, `officina run` included when it is
    /// in the job; returns only if it is not.
    fn end(&self, child: &mut Child, code: i32) {
        self.exit_with(code);
        let _ = child.kill();
        let _ = child.wait();
    }

    /// Ends whatever the action left running, and `officina run` itself
    /// when it is in the job, with `code`. A plain exit would let the
    /// kill-on-close job end the exiting process a second time and replace
    /// the program's exit code with 0 (found by `exit_code_passes_through`).
    /// The bits of a negative code are the full Windows value.
    fn exit_with(&self, code: i32) {
        #[cfg(windows)]
        if let Some(job) = &self.job {
            let _ = job.terminate(code as u32);
        }
        #[cfg(not(windows))]
        let _ = code;
    }
}

/// What one output stream carried so far.
#[derive(Default)]
struct Stream {
    bytes: u64,
    hash: StreamHash,
}

impl Stream {
    fn summary(&self) -> StreamSummary {
        StreamSummary {
            bytes: self.bytes,
            sha256: self.hash.hex(),
        }
    }
}

/// The two output streams of a run, copied through as bytes arrive under
/// one shared budget; past it they keep draining, so a full pipe never
/// stalls the program, and every byte is counted and hashed (AD-31).
struct Output {
    stdout: Arc<Mutex<Stream>>,
    stderr: Arc<Mutex<Stream>>,
    dropped: Arc<AtomicU64>,
    stdout_at_line_start: Arc<AtomicBool>,
    done: Receiver<()>,
    budget_kib: u32,
}

impl Output {
    fn start(child: &mut Child, budget_kib: u32) -> Output {
        let budget = Arc::new(AtomicU64::new(u64::from(budget_kib) * 1024));
        let dropped = Arc::new(AtomicU64::new(0));
        let stdout_at_line_start = Arc::new(AtomicBool::new(true));
        let (done_tx, done) = mpsc::channel();
        let stdout = Arc::new(Mutex::new(Stream::default()));
        let stderr = Arc::new(Mutex::new(Stream::default()));
        if let Some(pipe) = child.stdout.take() {
            let at_line_start = Arc::clone(&stdout_at_line_start);
            pump(
                pipe,
                Arc::clone(&stdout),
                Arc::clone(&budget),
                Arc::clone(&dropped),
                done_tx.clone(),
                move |bytes| {
                    let mut out = std::io::stdout().lock();
                    if out.write_all(bytes).and_then(|()| out.flush()).is_ok() {
                        at_line_start.store(bytes.ends_with(b"\n"), Ordering::SeqCst);
                    }
                },
            );
        }
        if let Some(pipe) = child.stderr.take() {
            pump(
                pipe,
                Arc::clone(&stderr),
                budget,
                Arc::clone(&dropped),
                done_tx,
                |bytes| {
                    let mut err = std::io::stderr().lock();
                    let _ = err.write_all(bytes).and_then(|()| err.flush());
                },
            );
        }
        Output {
            stdout,
            stderr,
            dropped,
            stdout_at_line_start,
            done,
            budget_kib,
        }
    }

    /// Waits for both streams to close, at most `grace`.
    fn drain(&self, grace: Duration) {
        let deadline = Instant::now() + grace;
        for _ in 0..2 {
            let left = deadline.saturating_duration_since(Instant::now());
            if self.done.recv_timeout(left).is_err() {
                return;
            }
        }
    }

    fn summaries(&self) -> (StreamSummary, StreamSummary, u64) {
        let summary = |stream: &Arc<Mutex<Stream>>| {
            stream
                .lock()
                .map(|s| s.summary())
                .unwrap_or_else(|poisoned| poisoned.into_inner().summary())
        };
        (
            summary(&self.stdout),
            summary(&self.stderr),
            self.dropped.load(Ordering::SeqCst),
        )
    }

    /// The last line when the budget cut the output (AD-31).
    fn mark_cut(&self, dropped: u64) {
        if dropped == 0 {
            return;
        }
        let lead = if self.stdout_at_line_start.load(Ordering::SeqCst) {
            ""
        } else {
            "\n"
        };
        let mut out = std::io::stdout().lock();
        let _ = writeln!(
            out,
            "{lead}[officina: output cut at {} KiB, {dropped} bytes dropped]",
            self.budget_kib
        );
        let _ = out.flush();
    }
}

/// Copies `pipe` through `sink` within the shared `budget`, on a thread of
/// its own, counting and hashing every byte; signals `done` at its end.
fn pump(
    mut pipe: impl Read + Send + 'static,
    stream: Arc<Mutex<Stream>>,
    budget: Arc<AtomicU64>,
    dropped: Arc<AtomicU64>,
    done: Sender<()>,
    sink: impl Fn(&[u8]) + Send + 'static,
) {
    std::thread::spawn(move || {
        let mut buf = vec![0u8; CHUNK];
        loop {
            let n = match pipe.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let chunk = &buf[..n];
            if let Ok(mut stream) = stream.lock() {
                stream.bytes += n as u64;
                stream.hash.update(chunk);
            }
            let wanted = n as u64;
            let left = budget
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |b| {
                    Some(b.saturating_sub(wanted))
                })
                .unwrap_or(0);
            let allowed = usize::try_from(left.min(wanted)).unwrap_or(0);
            if allowed > 0 {
                sink(&chunk[..allowed]);
            }
            dropped.fetch_add(wanted - allowed as u64, Ordering::SeqCst);
        }
        let _ = done.send(());
    });
}

fn finished(
    run_id: &str,
    outcome: RunOutcome,
    exit_code: Option<i64>,
    started: Instant,
    output: Option<&Output>,
    detail: Option<String>,
) -> JournalEntry {
    let empty = || Stream::default().summary();
    let (stdout, stderr, dropped) = output.map_or_else(|| (empty(), empty(), 0), Output::summaries);
    JournalEntry::RunFinished(RunFinished {
        run_id: run_id.to_owned(),
        outcome,
        exit_code,
        duration_ms: i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX),
        stdout,
        stderr,
        dropped_bytes: dropped,
        detail,
    })
}

/// Journals `run_finished`; a journal that stays locked loses the line and
/// says so, the run having happened.
fn journal_end(home: &RelayHome, line: JournalEntry) {
    if journal::append(home, &[line], now_ms()).is_err() {
        eprintln!("officina: the journal stayed locked: this run's end is not recorded");
    }
}

/// Journals `run_started`, starts the launch in its process tree, copies
/// its output, waits for it against its deadline and journals the end.
fn execute(
    home: &RelayHome,
    plan: &RunPlan,
    params: &BTreeMap<String, String>,
    session_id: Option<String>,
) -> i32 {
    let run_id = format!("{:x}-{:x}", now_ms(), std::process::id());
    let start = JournalEntry::RunStarted(RunStarted {
        run_id: run_id.clone(),
        project: plan.project.clone(),
        session_id,
        action: plan.action.clone(),
        class: plan.class,
        params: params.clone(),
        argv: plan.argv(),
        catalogue_hash: plan.catalogue_hash.clone(),
        approval_id: None,
        tree: None,
    });
    if let Err(e) = journal::append(home, &[start], now_ms()) {
        eprintln!("officina: refused: {e}, so nothing ran (journal_unavailable)");
        return EXIT_REFUSED;
    }
    let tree = ProcessTree::new();
    let started = Instant::now();
    let mut child = match command_for(&plan.launch).spawn() {
        Ok(child) => child,
        Err(e) => {
            eprintln!("officina: could not start {}: {e}", plan.launch.program);
            journal_end(
                home,
                finished(
                    &run_id,
                    RunOutcome::SpawnFailed,
                    None,
                    started,
                    None,
                    Some(e.to_string()),
                ),
            );
            tree.exit_with(EXIT_NOT_STARTED);
            return EXIT_NOT_STARTED;
        }
    };
    tree.started(&child);
    let output = Output::start(&mut child, plan.max_output_kib);
    let deadline = started + Duration::from_secs(u64::from(plan.timeout_s));
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                output.drain(OUTPUT_GRACE);
                let (_, _, dropped) = output.summaries();
                output.mark_cut(dropped);
                // Windows always reports a code; elsewhere a signal has none.
                let code = status.code().unwrap_or(1);
                journal_end(
                    home,
                    finished(
                        &run_id,
                        RunOutcome::Exited,
                        Some(i64::from(code)),
                        started,
                        Some(&output),
                        None,
                    ),
                );
                tree.exit_with(code);
                return code;
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(WAIT_TICK),
            Ok(None) => {
                journal_end(
                    home,
                    finished(
                        &run_id,
                        RunOutcome::Timeout,
                        None,
                        started,
                        Some(&output),
                        None,
                    ),
                );
                let (_, _, dropped) = output.summaries();
                output.mark_cut(dropped);
                eprintln!(
                    "officina: timeout: {} ran past its {} s; it and every process it started were stopped",
                    plan.action, plan.timeout_s
                );
                tree.end(&mut child, EXIT_TIMEOUT);
                return EXIT_TIMEOUT;
            }
            Err(e) => {
                journal_end(
                    home,
                    finished(
                        &run_id,
                        RunOutcome::Exited,
                        None,
                        started,
                        Some(&output),
                        Some(e.to_string()),
                    ),
                );
                eprintln!("officina: lost track of {}: {e}", plan.action);
                tree.end(&mut child, 1);
                return 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| (*a).to_owned()).collect()
    }

    #[test]
    fn arguments_split_at_the_first_equals() {
        assert_eq!(parse_args(&args(&["--list"])), Ok(RunArgs::List));
        assert_eq!(
            parse_args(&args(&["check", "group=lab-web", "q=a=b", "empty="])),
            Ok(RunArgs::Action {
                action: "check".to_owned(),
                params: BTreeMap::from([
                    ("group".to_owned(), "lab-web".to_owned()),
                    ("q".to_owned(), "a=b".to_owned()),
                    ("empty".to_owned(), String::new()),
                ]),
            })
        );
        for usage in [
            &[][..],
            &["check", "group"],
            &["check", "group=a", "group=b"],
            &["group=lab-web"],
            &["--list", "extra"],
            &["--wait", "10"],
        ] {
            assert!(parse_args(&args(usage)).is_err(), "{usage:?}");
        }
    }

    #[test]
    fn wsl_command_removes_wslenv() {
        let launch = Launch {
            program: r"C:\Windows\System32\wsl.exe".to_owned(),
            args: vec!["--exec".to_owned(), "/usr/bin/env".to_owned()],
            cwd: "C:/path/to/Lab".to_owned(),
            env: BTreeMap::from([("PROBE_MODE".to_owned(), "read-only".to_owned())]),
            env_remove: officina_core::relay::argv::WSL_REMOVED_ENV
                .iter()
                .map(|k| (*k).to_owned())
                .collect(),
        };
        let command = command_for(&launch);
        let envs: Vec<(String, Option<String>)> = command
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        for removed in officina_core::relay::argv::WSL_REMOVED_ENV {
            assert!(envs.contains(&((*removed).to_owned(), None)), "{envs:?}");
        }
        assert!(envs.contains(&("PROBE_MODE".to_owned(), Some("read-only".to_owned()))));
        assert_eq!(
            command.get_current_dir(),
            Some(std::path::Path::new("C:/path/to/Lab"))
        );
        assert_eq!(command.get_args().count(), 2);
    }
}
