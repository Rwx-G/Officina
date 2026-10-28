//! The triage call on the host (AD-51, AD-52, D-64 to D-68, D-81, D-86,
//! D-87, D-97): the refusals checked before anything starts, the contained
//! `claude -p` child core describes, the `triage` worker thread that runs
//! one call at a time, the dry run and the proposal queue the collector
//! drives from the registry, and the pause. Compiled only with the
//! `window-authority` feature: only the window spends model calls.

use std::collections::VecDeque;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::Duration;

use officina_core::registry::dry_run::{
    DryRunCase, DryRunProgress, DryRunState, DryRunStop, Tally, dry_run_cases,
};
use officina_core::registry::ingestion::TRIAGE_DIR;
use officina_core::registry::model::{
    BinaryStamp, ModelPause, answer_still_wanted, case_key, may_send, pause_due, queue,
};
use officina_core::registry::triage_call::{
    Budget, CALL_SPACING_MS, CALL_TIMEOUT_MS, CallAnswer, CallRecord, CallUsage, ModelOutputError,
    STDOUT_CAP_BYTES, TriageCase, WORKDIR_NAME, argv, budget, prompt, read_output,
    refusal_for_binary, triage_env,
};
use officina_core::registry::types::{SettingKey, Source};
use officina_core::registry::types::{TriageOutcome, TriagePurpose, TriageRefusal};
use rusqlite::TransactionBehavior;

use super::read::{self, VerdictSnapshot};
use super::store::Writer;
use super::write::{RegistryError, setting_json, write_setting};
use crate::process::{RunError, RunLimits, hide_console, run_contained};

/// How long the output pipes may stay open once the child exited and its
/// job was ended. Longer than `claude agents`' grace: the call runs on the
/// `triage` thread, which nothing waits on, and its tree is already ended,
/// so the pipes close as soon as the reader threads are scheduled.
const OUTPUT_GRACE: Duration = Duration::from_secs(2);

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
/// or `.bat` shim, `no_excerpts`, or a work folder that is not empty. Reads
/// only; [`run_call`] creates the folder first when it is missing.
pub fn refusal(job: &TriageJob) -> Option<TriageRefusal> {
    let Some(claude) = &job.claude else {
        return Some(TriageRefusal::ClaudeNotFound);
    };
    if let Some(shim) = refusal_for_binary(&claude.to_string_lossy()) {
        return Some(shim);
    }
    // Nothing to start: refused, so it spends no budget (D-65).
    if !claude.is_file() {
        return Some(TriageRefusal::ClaudeNotFound);
    }
    if job.no_excerpts {
        return Some(TriageRefusal::NoExcerpts);
    }
    let occupied = std::fs::read_dir(workdir(&job.officina_home))
        .is_ok_and(|mut entries| entries.next().is_some());
    occupied.then_some(TriageRefusal::WorkdirNotEmpty)
}

/// The `claude` binary's stamp (D-66): its path, size and modification
/// time; `None` when it cannot be read.
pub fn binary_stamp(claude: &Path) -> Option<BinaryStamp> {
    let meta = std::fs::metadata(claude).ok()?;
    let modified_ms = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())?;
    Some(BinaryStamp {
        path: claude.to_string_lossy().into_owned(),
        size: meta.len(),
        modified_ms,
    })
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

/// Runs the child through [`run_contained`] (its own kill-on-close job,
/// D-64), the prompt on its stdin, and reads its answer.
fn spawn_and_wait(job: &TriageJob, claude: &Path, prompt: String) -> Ran {
    let limits = RunLimits {
        timeout: job.limits.timeout,
        stdout_cap: job.limits.stdout_cap,
        grace: OUTPUT_GRACE,
        owns_tree: true,
    };
    let ran = match run_contained(command_for(job, claude), Some(prompt.into_bytes()), &limits) {
        Ok(ran) => ran,
        Err(RunError::Timeout(_)) => {
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
        Err(error) => return failed(format!("claude {error}")),
    };
    let (answer, usage) = if ran.stdout_over {
        (Err(ModelOutputError::TooLarge), CallUsage::default())
    } else {
        read_output(&ran.stdout)
    };
    if !ran.status.success() {
        return Ran {
            outcome: TriageOutcome::Failed,
            answer: None,
            usage,
            detail: Some(format!("claude {}", ran.failure())),
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
    // A folder that cannot be created is left to the spawn, which fails
    // with the reason in the call's detail.
    let _ = std::fs::create_dir_all(workdir(&job.officina_home));
    let claude = match (refusal(job), job.claude.as_deref()) {
        (None, Some(claude)) => claude,
        (refused, _) => {
            let refused = refused.unwrap_or(TriageRefusal::ClaudeNotFound);
            let ran = Ran {
                outcome: TriageOutcome::Refused,
                answer: None,
                usage: CallUsage::default(),
                detail: Some(refused.code().to_owned()),
            };
            return result(ran, Some(refused));
        }
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
    /// The thread is gone: a job could not be handed to it, or its result
    /// channel closed. Nothing more will run.
    gone: bool,
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
        Ok(TriageWorker::over(jobs, results))
    }

    fn over(jobs: Sender<TriageJob>, results: Receiver<CallResult>) -> TriageWorker {
        TriageWorker {
            jobs,
            results,
            busy: false,
            gone: false,
        }
    }

    /// True when no call runs.
    pub fn idle(&self) -> bool {
        !self.busy
    }

    /// True once the thread is gone (quality audit H1): the window then
    /// ends any dry run and refuses another, rather than wait on a call
    /// that never comes back.
    pub fn gone(&self) -> bool {
        self.gone
    }

    /// Hands `job` to the thread; false, and nothing sent, while a call
    /// runs or when the thread is gone.
    pub fn submit(&mut self, job: TriageJob) -> bool {
        if self.busy || self.gone {
            return false;
        }
        if self.jobs.send(job).is_err() {
            self.gone = true;
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
                self.gone = true;
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
    #[error("the thread that makes the calls is gone: restart the window to run a dry run")]
    NoWorker,
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
    /// The queue is read again no sooner than this: a call may start only
    /// one spacing after the last one anyway (AD-52), and reading the
    /// registry's rows on every tick would cost the collector for nothing.
    proposals_after_ms: i64,
    /// The `triage` thread is gone: nothing is queued or started.
    worker_gone: bool,
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
        if self.worker_gone {
            return Ok(Err(DryRunRefusal::NoWorker));
        }
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
    /// ran (D-67). A dry run the author asked for goes first; otherwise the
    /// next row queued for the model (story 3.11).
    pub fn next_call(
        &mut self,
        writer: &mut Writer,
        now_ms: i64,
    ) -> Result<Option<NextCall>, RegistryError> {
        if self.worker_gone {
            return Ok(None);
        }
        let Some(run) = &mut self.dry_run else {
            if now_ms < self.proposals_after_ms {
                return Ok(None);
            }
            self.proposals_after_ms = now_ms + CALL_SPACING_MS;
            return writer.next_proposal(now_ms);
        };
        if run.out.is_some() {
            return Ok(None);
        }
        // An input that expired while its case waited is never sent (D-85).
        while run
            .left
            .front()
            .is_some_and(|case| case.expires_at_ms <= now_ms)
        {
            run.left.pop_front();
            run.total -= 1;
        }
        let Some(case) = run.left.front().cloned() else {
            self.finish(writer, DryRunStop::Completed, now_ms)?;
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
        if result.purpose == TriagePurpose::Proposal {
            return writer.record_proposal(result, now_ms);
        }
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

    /// The `triage` thread is gone, at `now_ms` (quality audit H1): a dry
    /// run running ends with what ran recorded, its case out lost, and
    /// nothing is queued or started again until the window restarts.
    pub fn worker_gone(&mut self, writer: &mut Writer, now_ms: i64) -> Result<(), RegistryError> {
        if self.worker_gone {
            return Ok(());
        }
        self.worker_gone = true;
        if self.dry_run.is_some() {
            self.finish(writer, DryRunStop::WorkerGone, now_ms)?;
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
        insert_call(&tx, result)?;
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
        let calls = read::triage_calls_since(&self.conn, now_ms - officina_core::dates::DAY_MS)?;
        Ok(budget(&calls, now_ms))
    }

    /// The `triage_call` rows started since `from_ms`.
    pub fn triage_calls_since(&self, from_ms: i64) -> Result<Vec<CallRecord>, RegistryError> {
        Ok(read::triage_calls_since(&self.conn, from_ms)?)
    }

    /// True when `case` may be queued under `case_key` (D-81, D-87 as the
    /// owner amended it): its last call on the same inputs was not
    /// answered, was not refused since the model was last turned on, and
    /// did not fail since the model was last turned on or lifted.
    pub fn may_queue(&self, case_key: &str, case: &TriageCase) -> Result<bool, RegistryError> {
        let last = read::last_call_for(&self.conn, case_key)?;
        let facts = read::model_facts(&self.conn, None)?;
        Ok(may_send(
            last.as_ref(),
            &case.input_hash(),
            facts.since_ms(),
            facts.enabled_since_ms(),
        ))
    }

    /// Lifts the pause when the `claude` binary changed since it paused
    /// (D-66), source `rules`; true when it did.
    fn lift_pause_on_a_new_binary(&mut self, now_ms: i64) -> Result<bool, RegistryError> {
        let facts = read::model_facts(&self.conn, None)?;
        let Some(pause) = facts.pause.filter(|p| p.value.paused) else {
            return Ok(false);
        };
        let current = self.model_env.claude_stamp.clone();
        if current.is_none() || pause.value.stamp == current {
            return Ok(false);
        }
        let lifted = setting_json(&ModelPause {
            paused: false,
            stamp: pause.value.stamp,
        });
        write_setting(
            &self.conn,
            SettingKey::TriageModelPause,
            &lifted,
            Source::Rules,
            now_ms,
        )?;
        Ok(true)
    }

    /// The next row queued for the model at `now_ms`, when one may be sent
    /// (AD-52, D-81, D-87): the model on, available and not paused (a pause
    /// lifted first when `claude` changed), the budget open, and a row
    /// falling to the default whose case was not answered, refused, or
    /// failed since the last enable on the same inputs. One case per call.
    pub fn next_proposal(&mut self, now_ms: i64) -> Result<Option<NextCall>, RegistryError> {
        self.lift_pause_on_a_new_binary(now_ms)?;
        let facts = read::model_facts(&self.conn, self.model_env.unavailable)?;
        if !facts.queues() || self.budget(now_ms)? != Budget::Open {
            return Ok(None);
        }
        let snapshot = VerdictSnapshot::load(&self.conn, self.model_env.unavailable)?;
        Ok(queue(
            &snapshot.rows(),
            &facts,
            &snapshot.sessions,
            &snapshot.excerpts,
        )
        .into_iter()
        .next()
        .map(|queued| NextCall {
            purpose: TriagePurpose::Proposal,
            case_key: queued.case_key,
            case: queued.case,
        }))
    }

    /// Records a proposal call's result at `now_ms` in one transaction
    /// (AD-52, D-65, D-68, D-81): its `triage_call` row; its answer in
    /// `model_answer` only while a row still offers the default's place for
    /// that session on the very case sent (an answer for a row confirmed,
    /// settled or changed meanwhile is dropped); then the pause, when three
    /// proposal calls in a row failed since the model was last turned on or
    /// lifted, with the binary's stamp.
    pub fn record_proposal(
        &mut self,
        result: &CallResult,
        now_ms: i64,
    ) -> Result<(), RegistryError> {
        let unavailable = self.model_env.unavailable;
        let stamp = self.model_env.claude_stamp.clone();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        insert_call(&tx, result)?;
        let call_id = tx.last_insert_rowid();
        let session_id = result.case_key.strip_prefix("session:");
        if let (Some(answer), Some(session_id)) = (&result.answer, session_id)
            && result.outcome == TriageOutcome::Answer
            && case_key(session_id) == result.case_key
        {
            let snapshot = VerdictSnapshot::load(&tx, unavailable)?;
            if snapshot.facts.serves()
                && answer_still_wanted(
                    &snapshot.rows(),
                    session_id,
                    &result.input_hash,
                    &snapshot.sessions,
                    &snapshot.excerpts,
                )
            {
                tx.prepare_cached(
                    "INSERT INTO model_answer (session_id, verdict, reason, call_id, at_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT (session_id) DO UPDATE SET verdict = excluded.verdict,
                         reason = excluded.reason, call_id = excluded.call_id, at_ms = excluded.at_ms",
                )?
                .execute(rusqlite::params![
                    session_id,
                    answer.verdict.code(),
                    answer.reason,
                    call_id,
                    now_ms
                ])?;
            }
        }
        let facts = read::model_facts(&tx, unavailable)?;
        let since = facts.since_ms();
        if !facts.paused() && pause_due(&read::triage_calls_since(&tx, since)?, since) {
            let paused = setting_json(&ModelPause {
                paused: true,
                stamp,
            });
            write_setting(
                &tx,
                SettingKey::TriageModelPause,
                &paused,
                Source::Rules,
                now_ms,
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

/// Inserts one call's `triage_call` row.
fn insert_call(conn: &rusqlite::Connection, result: &CallResult) -> rusqlite::Result<()> {
    conn.prepare_cached(
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Quality audit H1: a thread that is gone is seen as such, whether a
    /// job cannot reach it or its results stop coming.
    #[test]
    fn a_gone_thread_is_seen_on_submit_and_on_results() {
        let (jobs, inbox) = mpsc::channel::<TriageJob>();
        let (outbox, results) = mpsc::channel::<CallResult>();
        drop(inbox);
        let mut worker = TriageWorker::over(jobs, results);
        let job = TriageJob {
            purpose: TriagePurpose::DryRun,
            case_key: "confirmation:1".into(),
            case: TriageCase::default(),
            claude: None,
            officina_home: PathBuf::from("home"),
            config_dir: None,
            no_excerpts: false,
            base_env: Arc::new(Vec::new()),
            limits: CallLimits::default(),
        };
        assert!(!worker.gone());
        assert!(!worker.submit(job));
        assert!(worker.gone());

        let (jobs, _inbox) = mpsc::channel::<TriageJob>();
        drop(outbox);
        let mut worker = TriageWorker::over(jobs, results_closed());
        assert!(worker.try_result().is_none());
        assert!(worker.gone() && worker.idle());
    }

    fn results_closed() -> Receiver<CallResult> {
        let (sender, receiver) = mpsc::channel();
        drop(sender);
        receiver
    }
}
