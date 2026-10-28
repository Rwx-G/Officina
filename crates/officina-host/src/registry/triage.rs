//! The triage call on the host (AD-51, AD-52, D-64, D-65, D-67, D-86,
//! D-87, D-97): the refusals checked before anything starts, the contained
//! `claude -p` child core describes, the `triage` worker thread that runs
//! one call at a time, and the dry run the collector drives from the
//! registry. Compiled only with the `window-authority` feature: only the
//! window spends model calls.

use std::collections::VecDeque;
use std::hash::{BuildHasher, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use officina_core::registry::dry_run::{
    DryRunCase, DryRunProgress, DryRunState, DryRunStop, Tally, dry_run_cases,
};
use officina_core::registry::ingestion::TRIAGE_DIR;
use officina_core::registry::triage_call::{
    Budget, CALL_TIMEOUT_MS, CallAnswer, CallRecord, CallUsage, ModelOutputError, STDOUT_CAP_BYTES,
    TriageCase, WORKDIR_NAME, argv, budget, prompt, queue_again, read_output, refusal_for_binary,
    triage_env,
};
use officina_core::registry::types::{TriageOutcome, TriagePurpose, TriageRefusal};
use rusqlite::TransactionBehavior;

use super::read;
use super::store::Writer;
use super::write::RegistryError;
use crate::process::{Waited, hide_console, wait_until};

/// How long the output pipes may stay open once the child exited and its
/// job was ended: what it wrote is drained by then.
const OUTPUT_GRACE: Duration = Duration::from_secs(2);

/// How much of stderr a failed call keeps, for its detail.
const STDERR_KEPT: usize = 4 * 1024;

/// How many nonces are drawn before a case is given up (D-87's "host draws
/// again"): one 16-digit nonce in an excerpt is chance; several in a row
/// is a case built to collide.
const NONCE_DRAWS: usize = 8;

/// How long one call may run, and how much stdout it may write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallLimits {
    pub timeout: Duration,
    pub stdout_cap: usize,
}

impl Default for CallLimits {
    /// AD-51's: [`CALL_TIMEOUT_MS`] and [`STDOUT_CAP_BYTES`].
    fn default() -> CallLimits {
        CallLimits {
            timeout: Duration::from_millis(CALL_TIMEOUT_MS),
            stdout_cap: STDOUT_CAP_BYTES,
        }
    }
}

/// One call to run: the case, and everything the runner needs from the
/// window, so the `triage` thread holds no state of its own.
#[derive(Debug, Clone)]
pub struct TriageJob {
    pub purpose: TriagePurpose,
    /// `confirmation:<id>` for a dry run case, a row key for a proposal.
    pub case_key: String,
    pub case: TriageCase,
    pub claude: Option<PathBuf>,
    pub officina_home: PathBuf,
    /// The floor's config dir, when it names one (AD-6).
    pub config_dir: Option<PathBuf>,
    /// `no_excerpts` from the environment or the window's floor.
    pub no_excerpts: bool,
    /// The environment the call's own is built from (D-86): the window's.
    pub base_env: Arc<Vec<(String, String)>>,
    pub limits: CallLimits,
}

/// What one call came to, as the collector records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallResult {
    pub purpose: TriagePurpose,
    pub case_key: String,
    pub input_hash: String,
    pub started_at_ms: i64,
    pub duration_ms: i64,
    pub outcome: TriageOutcome,
    pub refusal: Option<TriageRefusal>,
    /// The answer of an `answer` call: untrusted text (AD-40).
    pub answer: Option<CallAnswer>,
    pub usage: CallUsage,
    /// Why a call that started gave no answer, for the record of a test or
    /// a log; never stored, never shown as the model's.
    pub detail: Option<String>,
}

impl CallResult {
    /// The row the budget and the counts read.
    pub fn record(&self) -> CallRecord {
        CallRecord {
            purpose: self.purpose,
            started_at_ms: self.started_at_ms,
            outcome: self.outcome,
            refusal: self.refusal,
            input_tokens: self.usage.input_tokens,
            output_tokens: self.usage.output_tokens,
            cost_microusd: self.usage.cost_microusd,
        }
    }
}

/// `<officina home>/triage/empty`, the folder every call runs in: under
/// the folder ingestion skips (AD-43), and empty, or the call is refused.
pub fn workdir(officina_home: &Path) -> PathBuf {
    officina_home.join(TRIAGE_DIR).join(WORKDIR_NAME)
}

/// Why `job` must not start anything (AD-51, D-65): no `claude`, a `.cmd`
/// or `.bat` shim, `no_excerpts`, or a work folder that is not empty. The
/// folder is created when it is missing.
pub fn refusal(job: &TriageJob) -> Option<TriageRefusal> {
    let Some(claude) = &job.claude else {
        return Some(TriageRefusal::ClaudeNotFound);
    };
    if let Some(shim) = refusal_for_binary(&claude.to_string_lossy()) {
        return Some(shim);
    }
    if job.no_excerpts {
        return Some(TriageRefusal::NoExcerpts);
    }
    let dir = workdir(&job.officina_home);
    let _ = std::fs::create_dir_all(&dir);
    let occupied = std::fs::read_dir(&dir).is_ok_and(|mut entries| entries.next().is_some());
    occupied.then_some(TriageRefusal::WorkdirNotEmpty)
}

/// Sixteen hex digits from std's randomly keyed hasher (AD-51): no
/// dependency, and unguessable by what an excerpt was written with.
fn draw_nonce() -> String {
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    format!("{:016x}", hasher.finish())
}

/// The prompt of `case` with a nonce it does not hold, drawn again on a
/// collision; `None` when every draw collided.
fn prompt_for(case: &TriageCase) -> Option<String> {
    (0..NONCE_DRAWS).find_map(|_| prompt(case, &draw_nonce()).ok())
}

/// Reads `pipe` to its end on a thread of its own, keeping the first
/// `keep` bytes and discarding the rest, so a child writing more never
/// blocks on a full pipe (D-97); sends what it kept and whether more came.
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

/// The call's process tree (D-64): its own kill-on-close job, assigned
/// right after the spawn; the window never joins a job. Off Windows the
/// child alone is killed (Linux is v1).
struct CallJob {
    #[cfg(windows)]
    job: Option<crate::platform::windows::Job>,
}

impl CallJob {
    fn around(child: &Child) -> CallJob {
        #[cfg(windows)]
        {
            let job = crate::platform::windows::Job::new(true)
                .ok()
                .filter(|job| job.assign(child).is_ok());
            CallJob { job }
        }
        #[cfg(not(windows))]
        {
            let _ = child;
            CallJob {}
        }
    }

    /// Ends whatever the call left running, and the child itself.
    fn end(&self, child: &mut Child) {
        #[cfg(windows)]
        if let Some(job) = &self.job {
            let _ = job.terminate(1);
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// What the child came to.
struct Ran {
    outcome: TriageOutcome,
    answer: Option<CallAnswer>,
    usage: CallUsage,
    detail: Option<String>,
}

fn failed(detail: impl Into<String>) -> Ran {
    Ran {
        outcome: TriageOutcome::Failed,
        answer: None,
        usage: CallUsage::default(),
        detail: Some(detail.into()),
    }
}

/// The command of one call (AD-51): the program, core's argument vector,
/// the work folder as working directory, an environment cleared then set
/// to core's [`triage_env`] of `job.base_env`, the three pipes, no console
/// window.
pub fn command_for(job: &TriageJob, claude: &Path) -> Command {
    let config_dir = job
        .config_dir
        .as_deref()
        .map(|dir| dir.to_string_lossy().into_owned());
    let mut command = Command::new(claude);
    command
        .args(argv())
        .current_dir(workdir(&job.officina_home))
        .env_clear()
        .envs(triage_env(&job.base_env, config_dir.as_deref()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_console(&mut command);
    command
}

/// Starts the child, writes the prompt on its stdin, drains its output and
/// waits for it against the job's deadline.
fn spawn_and_wait(job: &TriageJob, claude: &Path, prompt: String) -> Ran {
    let started = Instant::now();
    let mut child = match command_for(job, claude).spawn() {
        Ok(child) => child,
        Err(e) => return failed(format!("claude could not start: {e}")),
    };
    let tree = CallJob::around(&child);
    // On a thread: a child that never reads its stdin must not hold the
    // runner on a full pipe.
    if let Some(mut stdin) = child.stdin.take() {
        std::thread::spawn(move || {
            let _ = stdin.write_all(prompt.as_bytes());
        });
    }
    let stdout = drain(child.stdout.take(), job.limits.stdout_cap);
    let stderr = drain(child.stderr.take(), STDERR_KEPT);
    let status = match wait_until(&mut child, started + job.limits.timeout) {
        Waited::Exited(status) => status,
        Waited::Deadline => {
            tree.end(&mut child);
            return Ran {
                outcome: TriageOutcome::Timeout,
                answer: None,
                usage: CallUsage::default(),
                detail: Some(format!(
                    "no answer within {} s; the call and its tree were ended",
                    job.limits.timeout.as_secs()
                )),
            };
        }
        Waited::Lost(e) => {
            tree.end(&mut child);
            return failed(format!("lost track of claude: {e}"));
        }
    };
    // The call starts no child (E16); one it left anyway would hold the
    // pipes open, so the job ends it before the output is read.
    tree.end(&mut child);
    let Ok((out, over)) = stdout.recv_timeout(OUTPUT_GRACE) else {
        return failed("claude exited but its output stayed open");
    };
    let (err, _) = stderr.recv_timeout(OUTPUT_GRACE).unwrap_or_default();
    let (answer, usage) = if over {
        (Err(ModelOutputError::TooLarge), CallUsage::default())
    } else {
        read_output(&out)
    };
    if !status.success() {
        let stderr: String = String::from_utf8_lossy(&err).chars().take(500).collect();
        return Ran {
            outcome: TriageOutcome::Failed,
            answer: None,
            usage,
            detail: Some(format!(
                "claude exited with {}: {}",
                status
                    .code()
                    .map_or_else(|| "a signal".to_owned(), |c| c.to_string()),
                stderr.trim()
            )),
        };
    }
    match answer {
        Ok(answer) => Ran {
            outcome: TriageOutcome::Answer,
            answer: Some(answer),
            usage,
            detail: None,
        },
        Err(error) => Ran {
            outcome: error.outcome(),
            answer: None,
            usage,
            detail: Some(error.code().to_owned()),
        },
    }
}

/// Runs one call (AD-51): refused before anything starts when [`refusal`]
/// says so, else the contained child, its answer read by core. `now_ms`
/// stamps the start and measures the duration.
pub fn run_call(job: &TriageJob, now_ms: &dyn Fn() -> i64) -> CallResult {
    let started_at_ms = now_ms();
    let result = |ran: Ran, refusal: Option<TriageRefusal>| CallResult {
        purpose: job.purpose,
        case_key: job.case_key.clone(),
        input_hash: job.case.input_hash(),
        started_at_ms,
        duration_ms: now_ms() - started_at_ms,
        outcome: ran.outcome,
        refusal,
        answer: ran.answer,
        usage: ran.usage,
        detail: ran.detail,
    };
    if let Some(refused) = refusal(job) {
        let ran = Ran {
            outcome: TriageOutcome::Refused,
            answer: None,
            usage: CallUsage::default(),
            detail: Some(refused.code().to_owned()),
        };
        return result(ran, Some(refused));
    }
    let Some(claude) = job.claude.as_deref() else {
        return result(failed("claude not found"), None);
    };
    let ran = match prompt_for(&job.case) {
        Some(text) => spawn_and_wait(job, claude, text),
        None => failed("every nonce drawn was in the case"),
    };
    result(ran, None)
}

/// The `triage` thread (AD-52, spine AD-3 as amended): it holds no
/// registry connection, takes one job at a time and returns each result on
/// a channel the collector drains with `try_recv` each tick, so the board
/// never waits on a call.
#[derive(Debug)]
pub struct TriageWorker {
    jobs: Sender<TriageJob>,
    results: Receiver<CallResult>,
    busy: bool,
}

impl TriageWorker {
    /// Starts the thread. It ends once the worker is dropped and its call,
    /// if any, has returned.
    pub fn spawn() -> std::io::Result<TriageWorker> {
        let (jobs, inbox) = mpsc::channel::<TriageJob>();
        let (outbox, results) = mpsc::channel();
        std::thread::Builder::new()
            .name("triage".into())
            .spawn(move || {
                while let Ok(job) = inbox.recv() {
                    let done = run_call(&job, &crate::files::now_ms);
                    if outbox.send(done).is_err() {
                        return;
                    }
                }
            })?;
        Ok(TriageWorker {
            jobs,
            results,
            busy: false,
        })
    }

    /// True when no call runs.
    pub fn idle(&self) -> bool {
        !self.busy
    }

    /// Hands `job` to the thread; false, and nothing sent, while a call
    /// runs or when the thread is gone.
    pub fn submit(&mut self, job: TriageJob) -> bool {
        if self.busy || self.jobs.send(job).is_err() {
            return false;
        }
        self.busy = true;
        true
    }

    /// The result of the call that ran, once it is back; never waits.
    pub fn try_result(&mut self) -> Option<CallResult> {
        match self.results.try_recv() {
            Ok(result) => {
                self.busy = false;
                Some(result)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.busy = false;
                None
            }
        }
    }
}

/// Why a dry run was not started (D-67).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DryRunRefusal {
    #[error("a dry run is already running")]
    Running,
    #[error(
        "no confirmation to evaluate on: the dry run uses the confirmations of a default proposal whose excerpts are still kept"
    )]
    NoCases,
}

/// A dry run in progress: the cases left, the one out, what ran.
#[derive(Debug)]
struct DryRun {
    left: VecDeque<DryRunCase>,
    out: Option<DryRunCase>,
    total: u32,
    started_at_ms: i64,
    tally: Tally,
}

/// The window's triage state (AD-52): the dry run running, if any, and
/// how the last one ended. The collector asks it for the next job, hands
/// it each result, and the registry records both.
#[derive(Debug, Default)]
pub struct Triage {
    dry_run: Option<DryRun>,
    last_stop: Option<DryRunStop>,
}

/// One case to send: the purpose, its key and what is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextCall {
    pub purpose: TriagePurpose,
    pub case_key: String,
    pub case: TriageCase,
}

impl Triage {
    /// What the view shows of dry runs.
    pub fn state(&self) -> DryRunState {
        DryRunState {
            running: self.dry_run.as_ref().map(|run| DryRunProgress {
                done: run.tally.cases,
                total: run.total,
                started_at_ms: run.started_at_ms,
            }),
            last_stop: self.last_stop,
        }
    }

    /// True while a dry run runs.
    pub fn running(&self) -> bool {
        self.dry_run.is_some()
    }

    /// Queues a dry run on `writer`'s confirmations at `now_ms` (AD-52,
    /// D-67, D-92): refused while one runs or with no case; it runs while
    /// the model is disabled or paused. Returns how many cases it holds.
    pub fn start_dry_run(
        &mut self,
        writer: &Writer,
        now_ms: i64,
    ) -> Result<Result<u32, DryRunRefusal>, RegistryError> {
        if self.dry_run.is_some() {
            return Ok(Err(DryRunRefusal::Running));
        }
        let cases = dry_run_cases(&read::evaluable_confirmations(&writer.conn)?, now_ms);
        if cases.is_empty() {
            return Ok(Err(DryRunRefusal::NoCases));
        }
        let total = u32::try_from(cases.len()).unwrap_or(u32::MAX);
        self.dry_run = Some(DryRun {
            left: cases.into(),
            out: None,
            total,
            started_at_ms: now_ms,
            tally: Tally::default(),
        });
        Ok(Ok(total))
    }

    /// The next call to make at `now_ms`, if one may start (AD-52): none
    /// while a call is out, while the last call is too recent, or with
    /// nothing queued; a spent budget ends the dry run and records what
    /// ran (D-67).
    pub fn next_call(
        &mut self,
        writer: &mut Writer,
        now_ms: i64,
    ) -> Result<Option<NextCall>, RegistryError> {
        let Some(run) = &mut self.dry_run else {
            return Ok(None);
        };
        if run.out.is_some() {
            return Ok(None);
        }
        let Some(case) = run.left.front().cloned() else {
            return Ok(None);
        };
        match writer.budget(now_ms)? {
            Budget::Open => {}
            Budget::Spacing { .. } => return Ok(None),
            Budget::Spent { .. } => {
                self.finish(writer, DryRunStop::BudgetSpent, now_ms)?;
                return Ok(None);
            }
        }
        run.left.pop_front();
        run.out = Some(case.clone());
        Ok(Some(NextCall {
            purpose: TriagePurpose::DryRun,
            case_key: case.key(),
            case: case.case,
        }))
    }

    /// Records `result` at `now_ms` (AD-52, D-65): its `triage_call` row,
    /// and for a dry run case its score; a refusal ends the dry run, since
    /// what refused it holds for every case, and the last case ends it
    /// too. A dry run's failures are dry-run rows, which the pause never
    /// reads (D-81).
    pub fn take_result(
        &mut self,
        writer: &mut Writer,
        result: &CallResult,
        now_ms: i64,
    ) -> Result<(), RegistryError> {
        writer.record_call(result)?;
        let Some(run) = &mut self.dry_run else {
            return Ok(());
        };
        let Some(case) = run.out.take_if(|case| case.key() == result.case_key) else {
            return Ok(());
        };
        if result.outcome == TriageOutcome::Refused {
            return self.finish(writer, DryRunStop::Refused, now_ms);
        }
        run.tally.add(
            case.verdict,
            result.answer.as_ref().map(|a| a.verdict),
            &result.usage,
        );
        if run.left.is_empty() {
            self.finish(writer, DryRunStop::Completed, now_ms)?;
        }
        Ok(())
    }

    /// Ends the dry run, recording what ran when anything did.
    fn finish(
        &mut self,
        writer: &mut Writer,
        stop: DryRunStop,
        now_ms: i64,
    ) -> Result<(), RegistryError> {
        if let Some(run) = self.dry_run.take()
            && run.tally.cases > 0
        {
            writer.record_evaluation(&run.tally, now_ms)?;
        }
        self.last_stop = Some(stop);
        Ok(())
    }
}

impl Writer {
    /// Records one call's `triage_call` row (AD-52, D-65, D-87); returns
    /// its id.
    pub fn record_call(&mut self, result: &CallResult) -> Result<i64, RegistryError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.prepare_cached(
            "INSERT INTO triage_call (purpose, case_key, input_hash, started_at_ms, duration_ms,
                 outcome, refusal, input_tokens, output_tokens, cost_microusd)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?
        .execute(rusqlite::params![
            result.purpose.code(),
            result.case_key,
            result.input_hash,
            result.started_at_ms,
            result.duration_ms,
            result.outcome.code(),
            result.refusal.map(TriageRefusal::code),
            result.usage.input_tokens,
            result.usage.output_tokens,
            result.usage.cost_microusd
        ])?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    /// Records a finished dry run's `model_evaluation` row at `at_ms`
    /// (AD-52, D-88): its results go there only, never to a proposal.
    pub fn record_evaluation(&mut self, tally: &Tally, at_ms: i64) -> Result<(), RegistryError> {
        self.conn
            .prepare_cached(
                "INSERT INTO model_evaluation (at_ms, cases, model_correct, default_correct,
                     model_false_finished, unanswered, input_tokens, output_tokens, cost_microusd)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?
            .execute(rusqlite::params![
                at_ms,
                tally.cases,
                tally.model_correct,
                tally.default_correct,
                tally.model_false_finished,
                tally.unanswered,
                tally.input_tokens,
                tally.output_tokens,
                tally.cost_microusd
            ])?;
        Ok(())
    }

    /// The budget at `now_ms`, from the calls of the last day (AD-52): it
    /// survives restarts since it is read from the rows.
    pub fn budget(&self, now_ms: i64) -> Result<Budget, RegistryError> {
        const DAY_MS: i64 = 86_400_000;
        let calls = read::triage_calls_since(&self.conn, now_ms - DAY_MS)?;
        Ok(budget(&calls, now_ms))
    }

    /// The `triage_call` rows started since `from_ms`.
    pub fn triage_calls_since(&self, from_ms: i64) -> Result<Vec<CallRecord>, RegistryError> {
        Ok(read::triage_calls_since(&self.conn, from_ms)?)
    }

    /// True when `case` may be queued under `case_key` (D-87): its last
    /// call was not refused on the same inputs.
    pub fn may_queue(&self, case_key: &str, case: &TriageCase) -> Result<bool, RegistryError> {
        let last = read::last_call_for(&self.conn, case_key)?;
        Ok(queue_again(
            last.as_ref()
                .map(|(outcome, hash)| (*outcome, hash.as_deref())),
            &case.input_hash(),
        ))
    }
}
