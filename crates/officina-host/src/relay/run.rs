//! `officina run` (AD-29 to AD-33, D-170): reads what the checks need,
//! refuses or starts the approved argument vector with a timeout and an
//! output budget, and journals the run. Runs of an action naming a check and
//! of a check record the tree they start on, and a check keeps its output
//! for the approval card; an `after-check` action runs only after its check;
//! a `human` action waits for the author's approval, made after its dry run
//! when it names one. It is the relay's authority over what runs and fails
//! closed, unlike the gate (AD-28): every doubt is a refusal.
//!
//! Exit codes a shell already reads: the program's own, full Windows value
//! included; 126 refused; 124 timed out; 127 not started; 64 usage. A
//! program can exit 126 or 124 itself: the stderr line and the journal tell
//! them apart. Officina itself never exits 2.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use officina_core::relay::StreamHash;
use officina_core::relay::approvals::{
    ApprovalRequest, DEFAULT_WAIT_S, FileKind, HttpRequestView, KEPT_OUTPUT_BYTES, KeyInputs,
    NextStep, WAIT_TICK_MS, kept_output, next_step, request_id, request_key,
};
use officina_core::relay::argv::Launch;
use officina_core::relay::catalogue::ActionClass;
use officina_core::relay::classes::WAIT_RANGE_S;
use officina_core::relay::http::HttpPlan;
use officina_core::relay::journal::{
    ApprovalEvent, JournalEntry, Refused, RunFinished, RunOutcome, RunStarted, StreamSummary,
    TreeFingerprint,
};
pub use officina_core::relay::run::{EXIT_NOT_STARTED, EXIT_REFUSED, EXIT_TIMEOUT};
use officina_core::relay::run::{
    FloorState, Refusal, RunInputs, RunPlan, SnapshotState, check_step, list_text, plan,
};

use super::credential::{CredentialStore, Secret, SystemCredentialStore};
use super::home::{ApprovedState, RelayHome, load_relay_floor, system_dir};
use super::http::{self, HttpOutcome};
use super::journal::JournalLock;
use super::{approvals, journal, tree};
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
        /// `--wait <s>`: how long a `human` action waits for the author's
        /// decision (AD-33).
        wait_s: u32,
    },
}

/// Parses the arguments after `run` by hand (AD-29): `--list` alone, or an
/// action then `name=value` tokens split at the first `=`, and `--wait <s>`
/// once, in [`WAIT_RANGE_S`] ([`DEFAULT_WAIT_S`] when absent). A token
/// without `=`, a duplicate name, a missing action, an unknown option or a
/// `--wait` out of range is a usage error, which the caller reports with
/// exit 64.
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
    let mut wait_s = None;
    let mut tokens = tokens.iter();
    while let Some(token) = tokens.next() {
        if token == "--wait" {
            let seconds = tokens
                .next()
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|seconds| WAIT_RANGE_S.contains(seconds))
                .ok_or_else(|| {
                    format!(
                        "--wait takes seconds from {} to {}",
                        WAIT_RANGE_S.start(),
                        WAIT_RANGE_S.end()
                    )
                })?;
            if wait_s.replace(seconds).is_some() {
                return Err("--wait is given twice".to_owned());
            }
            continue;
        }
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
        wait_s: wait_s.unwrap_or(DEFAULT_WAIT_S),
    })
}

/// `CLAUDE_CODE_SESSION_ID`, absent outside Claude Code (AD-29).
pub fn session_id_from_env() -> Option<String> {
    std::env::var(ENV_SESSION_ID).ok().filter(|v| !v.is_empty())
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
    // Empty off Windows, where the `wsl` runner has nothing to start.
    let system_dir = system_dir()
        .map(|dir| dir.to_string_lossy().into_owned())
        .unwrap_or_default();
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
///
/// After the checks of [`plan`], a run that binds the tree takes its
/// fingerprint in the project folder (D-22), and an action naming a check
/// reads two days of journal for it (AD-33, D-170). A `human` action then
/// waits up to `wait` for the author's approval ([`approval`]), the last step
/// before anything is journaled as started.
pub fn run(
    home: &RelayHome,
    cwd: &str,
    action: &str,
    params: &BTreeMap<String, String>,
    session_id: Option<String>,
    wait: Duration,
) -> i32 {
    let store = SystemCredentialStore;
    let mut stdout = std::io::stdout();
    run_with(
        home,
        cwd,
        &Call {
            action,
            params,
            session_id,
            wait,
        },
        &store,
        &mut stdout,
    )
}

/// One `officina run` call: the action, its parameters, the session and how
/// long a `human` action waits.
pub struct Call<'a> {
    pub action: &'a str,
    pub params: &'a BTreeMap<String, String>,
    pub session_id: Option<String>,
    pub wait: Duration,
}

/// [`run`] with the credential store and the stream an HTTP action's
/// response is written to given, for tests: a fake store, a captured
/// output. A process action's output goes to this process's stdout.
pub fn run_with(
    home: &RelayHome,
    cwd: &str,
    call: &Call<'_>,
    store: &dyn CredentialStore,
    out: &mut dyn Write,
) -> i32 {
    let (action, params, wait) = (call.action, call.params, call.wait);
    let session_id = call.session_id.clone();
    let plan = match with_inputs(home, cwd, |inputs| plan(inputs, action, params)) {
        Ok(plan) => plan,
        Err(refusal) => return refuse(home, &refusal, action, params, session_id),
    };
    let tree = plan
        .binds_tree
        .then(|| tree::fingerprint(Path::new(&plan.project_dir)))
        .flatten();
    let mut check_run_id = None;
    if plan.check.is_some() {
        let now = now_ms();
        let (lines, _) = journal::read_recent(home, now);
        match check_step(&plan, params, &lines, tree.as_ref(), now) {
            Ok(found) => check_run_id = found,
            Err(refusal) => return refuse(home, &refusal, action, params, session_id),
        }
    }
    // `auto` never passes validation (v0.5); were it to, it would still wait
    // for the author.
    if matches!(plan.class, ActionClass::Human | ActionClass::Auto) {
        let caller = Caller {
            home,
            cwd,
            params,
            session_id: &session_id,
            store,
        };
        let bound = Bound {
            check_run_id: check_run_id.as_deref(),
            tree: tree.as_ref(),
        };
        return match approval(&caller, &plan, bound, wait) {
            Ok(approved) => execute(
                home,
                &approved.plan,
                Started {
                    params,
                    session_id,
                    tree,
                    approval_id: Some(approved.id),
                },
                approved.secret,
                out,
            ),
            Err(code) => code,
        };
    }
    // After every check, just before the request (D-29).
    let secret = match read_secret(&plan, store) {
        Ok(secret) => secret,
        Err(refusal) => return refuse(home, &refusal, action, params, session_id),
    };
    execute(
        home,
        &plan,
        Started {
            params,
            session_id,
            tree,
            approval_id: None,
        },
        secret,
        out,
    )
}

/// The secret of the credential an HTTP action's plan carries, read from
/// `store`; `None` for a plan without one. A missing or unreadable
/// credential is a refusal, before any connection (D-29).
fn read_secret(plan: &RunPlan, store: &dyn CredentialStore) -> Result<Option<Secret>, Refusal> {
    let Some(binding) = plan.http().and_then(|http| http.credential.as_ref()) else {
        return Ok(None);
    };
    match store.read(&binding.name) {
        Ok(Some(secret)) => Ok(Some(secret)),
        Ok(None) => Err(Refusal::credential_missing(&plan.project, &binding.name)),
        Err(e) => Err(Refusal::credential_unreadable(
            &plan.project,
            &binding.name,
            &e.to_string(),
        )),
    }
}

/// Who called and from where, for the steps of a `human` action.
struct Caller<'a> {
    home: &'a RelayHome,
    cwd: &'a str,
    params: &'a BTreeMap<String, String>,
    session_id: &'a Option<String>,
    store: &'a dyn CredentialStore,
}

/// An approval used: its id, the plan checked again just before, and the
/// credential read before the approval was consumed (D-29).
struct Approved {
    id: String,
    plan: RunPlan,
    secret: Option<Secret>,
}

impl Caller<'_> {
    fn refuse(&self, refusal: &Refusal, action: &str) -> i32 {
        refuse(
            self.home,
            refusal,
            action,
            self.params,
            self.session_id.clone(),
        )
    }
}

/// What a `human` action's request is bound to besides its parameters: the
/// dry run it was made after and the tree that ran on (D-170).
#[derive(Clone, Copy)]
struct Bound<'a> {
    check_run_id: Option<&'a str>,
    tree: Option<&'a TreeFingerprint>,
}

/// AD-33 steps 1 to 4 for a `human` action: refused after a recent
/// rejection; an approval for this request consumed and the plan returned
/// with its id; otherwise the pending request reused, or a new one written
/// and journaled, then waited for, a stat every [`WAIT_TICK_MS`] with the
/// stop flag read each tick, until a decision or `wait`. Errors are exit
/// codes, the refusal already printed and journaled.
fn approval(
    caller: &Caller<'_>,
    plan: &RunPlan,
    bound: Bound<'_>,
    wait: Duration,
) -> Result<Approved, i32> {
    let key = request_key(&KeyInputs {
        project: &plan.project,
        action: &plan.action,
        params: caller.params,
        catalogue_hash: &plan.catalogue_hash,
        check_run_id: bound.check_run_id,
        tree: bound.tree,
    });
    let deadline = Instant::now() + wait;
    loop {
        let files = approvals::read_files(caller.home);
        let id = match next_step(&files, &key, now_ms()) {
            NextStep::Rejected { .. } => {
                return Err(caller.refuse(&Refusal::approval_rejected(&plan.project), &plan.action));
            }
            NextStep::Consume { id } => match consume(caller, plan, bound, &id) {
                Consumed::Done(fresh, secret) => {
                    return Ok(Approved {
                        id,
                        plan: *fresh,
                        secret,
                    });
                }
                Consumed::Gone => continue,
                Consumed::Refused(refusal) => return Err(caller.refuse(&refusal, &plan.action)),
            },
            NextStep::Wait { id } => id,
            NextStep::Create => create(caller, plan, bound, &key)?,
        };
        loop {
            if caller.home.relay_stopped() {
                return Err(caller.refuse(&Refusal::stopped(), &plan.action));
            }
            match approvals::decision(caller.home, &id) {
                Some(FileKind::Approved) => break,
                Some(FileKind::Rejected) => {
                    return Err(
                        caller.refuse(&Refusal::approval_rejected(&plan.project), &plan.action)
                    );
                }
                _ => {}
            }
            if Instant::now() >= deadline {
                return Err(caller.refuse(&Refusal::approval_pending(&plan.project), &plan.action));
            }
            std::thread::sleep(Duration::from_millis(WAIT_TICK_MS));
        }
    }
}

/// How using an approval went.
enum Consumed {
    /// Used: the plan checked again just before, and its credential.
    Done(Box<RunPlan>, Option<Secret>),
    /// Another call used it first.
    Gone,
    /// Refused before using it: the approval stays as it was.
    Refused(Refusal),
}

/// Uses approval `id`, last before the spawn (AD-33): the checks of [`plan`]
/// again, the approval made under the same catalogue, and for an action
/// naming a check the tree still the dry run's (D-170), and an HTTP action's
/// credential read (D-29); then the claim. Anything refused leaves the
/// approval unconsumed.
fn consume(caller: &Caller<'_>, planned: &RunPlan, bound: Bound<'_>, id: &str) -> Consumed {
    let fresh = match with_inputs(caller.home, caller.cwd, |inputs| {
        plan(inputs, &planned.action, caller.params)
    }) {
        Ok(fresh) => fresh,
        Err(refusal) => return Consumed::Refused(refusal),
    };
    if fresh.catalogue_hash != planned.catalogue_hash {
        return Consumed::Refused(Refusal::catalogue_changed_since_request(&fresh.project));
    }
    if let Some(check) = &fresh.check
        && tree::fingerprint(Path::new(&fresh.project_dir)).as_ref() != bound.tree
    {
        return Consumed::Refused(Refusal::check_not_satisfied(&fresh.project, check));
    }
    let secret = match read_secret(&fresh, caller.store) {
        Ok(secret) => secret,
        Err(refusal) => return Consumed::Refused(refusal),
    };
    match approvals::consume(caller.home, id) {
        Ok(()) => Consumed::Done(Box::new(fresh), secret),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Consumed::Gone,
        Err(e) => Consumed::Refused(Refusal::approval_unavailable(
            &fresh.project,
            &e.to_string(),
        )),
    }
}

/// Writes a new request and its `approval_requested` line under the journal
/// lock, so a request and its line go together (PRD amendment 11), unless a
/// concurrent call made the same request first. Returns its id.
fn create(caller: &Caller<'_>, plan: &RunPlan, bound: Bound<'_>, key: &str) -> Result<String, i32> {
    let lock = match JournalLock::acquire(caller.home) {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("officina: refused: {e}, so no approval was requested (journal_unavailable)");
            return Err(EXIT_REFUSED);
        }
    };
    let now = now_ms();
    // Every request is written under the lock: a call that waited for it
    // while another made the same request reuses that one.
    if let NextStep::Wait { id } = next_step(&approvals::read_files(caller.home), key, now) {
        return Ok(id);
    }
    let id = request_id(key, now);
    let request = ApprovalRequest {
        request_key: key.to_owned(),
        created_at_ms: now,
        project: plan.project.clone(),
        action: plan.action.clone(),
        params: caller.params.clone(),
        argv: plan.argv(),
        http: plan.http().map(HttpRequestView::from),
        catalogue_hash: plan.catalogue_hash.clone(),
        session_id: caller.session_id.clone(),
        check_run_id: bound.check_run_id.map(str::to_owned),
        tree: bound.tree.cloned(),
    };
    if let Err(e) = approvals::create_request(caller.home, &id, &request) {
        drop(lock);
        return Err(caller.refuse(
            &Refusal::approval_unavailable(&plan.project, &e.to_string()),
            &plan.action,
        ));
    }
    let line = JournalEntry::ApprovalRequested(ApprovalEvent {
        approval_id: id.clone(),
        request_key: key.to_owned(),
    });
    if lock.append(&[line], now).is_err() {
        eprintln!("officina: the journal could not record approval request {id}");
    }
    Ok(id)
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
    /// The first [`KEPT_OUTPUT_BYTES`] bytes, for a check run (D-170).
    kept: Option<Vec<u8>>,
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
    /// `keep`: also keep the first bytes of each stream (D-170).
    fn start(child: &mut Child, budget_kib: u32, keep: bool) -> Output {
        let budget = Arc::new(AtomicU64::new(u64::from(budget_kib) * 1024));
        let dropped = Arc::new(AtomicU64::new(0));
        let stdout_at_line_start = Arc::new(AtomicBool::new(true));
        let (done_tx, done) = mpsc::channel();
        let stream = || {
            Arc::new(Mutex::new(Stream {
                kept: keep.then(Vec::new),
                ..Stream::default()
            }))
        };
        let stdout = stream();
        let stderr = stream();
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

    /// What a check run keeps (D-170): stdout then stderr, capped, with the
    /// cut marker; `None` when the run keeps nothing.
    fn kept(&self) -> Option<Vec<u8>> {
        let out = self.stdout.lock().ok()?;
        let err = self.stderr.lock().ok()?;
        Some(kept_output(
            out.kept.as_deref()?,
            err.kept.as_deref()?,
            out.bytes,
            err.bytes,
        ))
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
                if let Some(kept) = &mut stream.kept {
                    let room = KEPT_OUTPUT_BYTES.saturating_sub(kept.len()).min(n);
                    kept.extend_from_slice(&chunk[..room]);
                }
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
        duration_ms: elapsed_ms(started),
        stdout: Some(stdout),
        stderr: Some(stderr),
        dropped_bytes: dropped,
        status: None,
        body_bytes: None,
        body_sha256: None,
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

/// Keeps a check run's output (D-170); a failure is said, the run stands.
fn keep(home: &RelayHome, run_id: &str, output: &Output) {
    if let Some(bytes) = output.kept()
        && let Err(e) = approvals::keep_output(home, run_id, &bytes)
    {
        eprintln!("officina: this check's output could not be kept: {e}");
    }
}

/// What the start line records besides the plan.
struct Started<'a> {
    params: &'a BTreeMap<String, String>,
    session_id: Option<String>,
    tree: Option<TreeFingerprint>,
    approval_id: Option<String>,
}

/// Journals `run_started`, then starts the process or sends the request.
fn execute(
    home: &RelayHome,
    plan: &RunPlan,
    context: Started<'_>,
    secret: Option<Secret>,
    out: &mut dyn Write,
) -> i32 {
    let run_id = format!("{:x}-{:x}", now_ms(), std::process::id());
    let http = plan.http();
    let start = JournalEntry::RunStarted(RunStarted {
        run_id: run_id.clone(),
        project: plan.project.clone(),
        session_id: context.session_id,
        action: plan.action.clone(),
        class: plan.class,
        params: context.params.clone(),
        argv: plan.argv(),
        method: http.map(|h| h.method),
        url: http.map(|h| h.url.clone()),
        credential: http.and_then(|h| h.credential.as_ref().map(|c| c.name.clone())),
        catalogue_hash: plan.catalogue_hash.clone(),
        approval_id: context.approval_id,
        tree: context.tree,
        timeout_s: Some(http.map_or(plan.timeout_s, |h| h.timeout_s)),
    });
    if let Err(e) = journal::append(home, &[start], now_ms()) {
        eprintln!("officina: refused: {e}, so nothing ran (journal_unavailable)");
        return EXIT_REFUSED;
    }
    match (plan.launch(), http) {
        (Some(launch), _) => spawn(home, plan, launch, &run_id),
        (None, Some(request)) => send_request(home, plan, request, &run_id, secret, out),
        (None, None) => EXIT_NOT_STARTED,
    }
}

fn elapsed_ms(started: Instant) -> i64 {
    i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX)
}

/// Passes what an HTTP run prints on, keeping its first bytes for a check
/// run's kept output (D-170).
struct Tee<'a> {
    out: &'a mut dyn Write,
    kept: Option<Vec<u8>>,
    total: u64,
}

impl Write for Tee<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let written = self.out.write(bytes)?;
        self.total += written as u64;
        if let Some(kept) = &mut self.kept {
            let room = KEPT_OUTPUT_BYTES.saturating_sub(kept.len()).min(written);
            kept.extend_from_slice(&bytes[..room]);
        }
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
    }
}

/// The response half of a `run_finished` line: status, body bytes and hash.
struct Response {
    status: u16,
    body_bytes: u64,
    body_sha256: String,
}

fn http_finished(
    run_id: &str,
    outcome: RunOutcome,
    started: Instant,
    response: Option<Response>,
    detail: Option<String>,
) -> JournalEntry {
    JournalEntry::RunFinished(RunFinished {
        run_id: run_id.to_owned(),
        outcome,
        exit_code: None,
        duration_ms: elapsed_ms(started),
        stdout: None,
        stderr: None,
        dropped_bytes: 0,
        status: response.as_ref().map(|r| r.status),
        body_bytes: response.as_ref().map(|r| r.body_bytes),
        body_sha256: response.map(|r| r.body_sha256),
        detail,
    })
}

/// Sends an HTTP action's request (AD-36), writes the response to `out`,
/// journals the end and returns the exit code of D-35: 0 for a 2xx status,
/// 1 for any other (one stderr line `officina: http <status>`), 124 on
/// timeout, 127 without a response; never 2.
fn send_request(
    home: &RelayHome,
    plan: &RunPlan,
    request: &HttpPlan,
    run_id: &str,
    secret: Option<Secret>,
    out: &mut dyn Write,
) -> i32 {
    let started = Instant::now();
    let mut tee = Tee {
        out,
        kept: plan.keeps_output.then(Vec::new),
        total: 0,
    };
    let outcome = http::send(request, secret.as_ref(), &mut tee);
    drop(secret);
    let (line, code) = match outcome {
        HttpOutcome::Response {
            status,
            body_bytes,
            body_sha256,
            cut,
            read_error,
        } => {
            let detail = match (&read_error, cut) {
                (Some(e), _) => Some(format!("the body broke off: {e}")),
                (None, true) => Some(format!(
                    "body cut at {} KiB, connection dropped",
                    request.max_body_bytes / 1024
                )),
                (None, false) => None,
            };
            let success = (200..300).contains(&status) && read_error.is_none();
            if !success {
                match &read_error {
                    Some(e) => eprintln!("officina: http {status}, the body broke off: {e}"),
                    None => eprintln!("officina: http {status}"),
                }
            }
            let response = Response {
                status,
                body_bytes,
                body_sha256,
            };
            let line = http_finished(run_id, RunOutcome::Http, started, Some(response), detail);
            (line, i32::from(!success))
        }
        HttpOutcome::Timeout => {
            eprintln!(
                "officina: timeout: {} got no complete response within {} s",
                plan.action, request.timeout_s
            );
            let line = http_finished(run_id, RunOutcome::Timeout, started, None, None);
            (line, EXIT_TIMEOUT)
        }
        HttpOutcome::ConnectFailed { detail } => {
            eprintln!("officina: no response from {}: {detail}", request.host);
            let line = http_finished(
                run_id,
                RunOutcome::ConnectFailed,
                started,
                None,
                Some(detail),
            );
            (line, EXIT_NOT_STARTED)
        }
    };
    journal_end(home, line);
    if let Some(kept) = &tee.kept {
        let bytes = kept_output(kept, &[], tee.total, 0);
        if let Err(e) = approvals::keep_output(home, run_id, &bytes) {
            eprintln!("officina: this check's output could not be kept: {e}");
        }
    }
    code
}

/// Starts a process action in its process tree, copies its output, waits
/// for it against its deadline, keeps a check's output and journals the
/// end.
fn spawn(home: &RelayHome, plan: &RunPlan, launch: &Launch, run_id: &str) -> i32 {
    let run_id = run_id.to_owned();
    let tree = ProcessTree::new();
    let started = Instant::now();
    let mut child = match command_for(launch).spawn() {
        Ok(child) => child,
        Err(e) => {
            eprintln!("officina: could not start {}: {e}", launch.program);
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
    let output = Output::start(&mut child, plan.max_output_kib, plan.keeps_output);
    let deadline = started + Duration::from_secs(u64::from(plan.timeout_s));
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                output.drain(OUTPUT_GRACE);
                let (_, _, dropped) = output.summaries();
                output.mark_cut(dropped);
                keep(home, &run_id, &output);
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
                keep(home, &run_id, &output);
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
                wait_s: DEFAULT_WAIT_S,
            })
        );
        let max = WAIT_RANGE_S.end().to_string();
        assert_eq!(
            parse_args(&args(&["apply", "--wait", &max, "group=lab-web"])),
            Ok(RunArgs::Action {
                action: "apply".to_owned(),
                params: BTreeMap::from([("group".to_owned(), "lab-web".to_owned())]),
                wait_s: *WAIT_RANGE_S.end(),
            })
        );
        let past = (WAIT_RANGE_S.end() + 1).to_string();
        let below = (WAIT_RANGE_S.start() - 1).to_string();
        for usage in [
            &[][..],
            &["check", "group"],
            &["check", "group=a", "group=b"],
            &["group=lab-web"],
            &["--list", "extra"],
            &["--wait", "10"],
            &["apply", "--wait"],
            &["apply", "--wait", "x"],
            &["apply", "--wait", &past],
            &["apply", "--wait", &below],
            &["apply", "--wait", "5", "--wait", "6"],
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
