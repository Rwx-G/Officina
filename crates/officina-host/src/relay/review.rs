//! The review of `auto` requests on the host (AD-86 to AD-88, D-142,
//! D-143): the `review` thread that runs one contained call at a time, and
//! what the collector does each tick: each new `auto` request prechecked,
//! escalated without a call or queued, the queue paced and checked again
//! right before each call, each outcome recorded as one `auto_review` row
//! and its journal lines, and the author's decision copied onto the row.
//! While auto approval is off, the review is blind shadow: the request is
//! the author's at once, and what the review answered never shows before
//! he decides. While it is on, in mode `review` a request is `reviewing`
//! until its review answers, and an `approve` is recorded by the window's
//! authority; in mode `rule` no call runs and the rule baseline decides
//! (AD-88, D-141). Compiled only with the `window-authority` feature: only
//! the window spends model calls and records approvals.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::Duration;

use officina_core::board::Board;
use officina_core::registry::triage_call::{CALL_SPACING_MS, CallUsage, PromptError};
use officina_core::registry::types::{AuthorDecision, ReviewMode, ReviewOutcome};
use officina_core::relay::approvals::{
    ApprovalBy, ApprovalFiles, DecideRefusal, Decision, PENDING_MS, is_live,
};
use officina_core::relay::auto::{
    Escalation, EscalationCause, MARK_WINDOW_MS, Precheck, PrecheckFacts, REVIEW_TIMEOUT_MS,
    ReviewAnswer, ReviewContext, ReviewDecision, ReviewJob, check_status, decide, mode,
    output_error_cause, past_outcomes, precheck, precheck_rule, prompt, read_output, recheck,
    rule_baseline, rule_decision, schema,
};
use officina_core::relay::journal::{
    AutoEscalated, AutoReviewed, JournalEntry, JournalLine, TreeFingerprint,
};
use officina_core::relay::template::ModelAlias;
use officina_core::spawn::OfficinaDir;

use super::approvals::{self, ContextSources};
use super::authority::{self, AuthorityError};
use super::home::RelayHome;
use super::journal;
use super::tree;
use crate::registry::review::{AutoState, NewReview, ReviewRow};
use crate::registry::triage::{NONCE_DRAWS, draw_nonce};
use crate::registry::window::WindowRegistry;
use crate::spawn::{BaseEnv, ResolvedClaude, SpawnError, SpawnOutcome, execute};
use officina_core::dates::DAY_MS;

/// `<officina home>/review/empty`, the folder every review runs in (AD-79).
pub fn workdir(officina_home: &Path) -> PathBuf {
    OfficinaDir::ReviewWorkdir
        .segments()
        .iter()
        .fold(officina_home.to_path_buf(), |path, segment| {
            path.join(segment)
        })
}

/// `<officina home>/review`, the review folder's parent, where a memory
/// file would sit if Claude Code read one there (D-164, E25).
pub fn review_dir(officina_home: &Path) -> PathBuf {
    officina_home.join(officina_core::relay::auto::REVIEW_DIR)
}

/// True when the review's folder holds nothing (or is missing): a planted
/// file never reaches a call (D-140).
pub fn workdir_empty(officina_home: &Path) -> bool {
    std::fs::read_dir(workdir(officina_home)).map_or(true, |mut entries| entries.next().is_none())
}

/// One call to run: its prompt, built by the collector, and what the
/// runner needs from the window, so the `review` thread holds no state.
#[derive(Debug, Clone)]
pub struct ReviewCall {
    pub approval_id: String,
    pub model: ModelAlias,
    /// The stdin: the instruction and the data line between their markers.
    pub prompt: String,
    pub claude: PathBuf,
    pub officina_home: PathBuf,
    /// The floor's config dir, when it names one (AD-6).
    pub config_dir: Option<PathBuf>,
    pub base_env: BaseEnv,
    pub timeout: Duration,
}

/// What one call came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallResult {
    pub approval_id: String,
    pub started_at_ms: i64,
    pub duration_ms: i64,
    pub outcome: ReviewOutcome,
    /// The answer of an `answered` call.
    pub answer: Option<ReviewAnswer>,
    /// Why a call that started gave no answer.
    pub cause: Option<EscalationCause>,
    pub usage: CallUsage,
}

/// Runs one review call: core's `ReviewCall` vector through
/// `spawn::execute` (its own kill-on-close job), the prompt on its stdin,
/// and the answer read from `structured_output` alone.
pub fn run_call(call: &ReviewCall, now_ms: &dyn Fn() -> i64) -> CallResult {
    let started_at_ms = now_ms();
    let _ = std::fs::create_dir_all(workdir(&call.officina_home));
    let config_dir = call
        .config_dir
        .as_deref()
        .map(|dir| dir.to_string_lossy().into_owned());
    let deadline_ms = u64::try_from(call.timeout.as_millis()).unwrap_or(u64::MAX);
    let vector = officina_core::spawn::review_call(
        call.model,
        &schema(),
        config_dir.as_deref(),
        &call.officina_home.to_string_lossy(),
    )
    .tightened(deadline_ms, usize::MAX)
    .with_stdin(call.prompt.clone().into_bytes());
    let claude = ResolvedClaude::new(call.claude.clone());
    let (outcome, answer, cause, usage) = match execute(&claude, &vector, &call.base_env) {
        Ok(SpawnOutcome::Captured(ran)) if ran.status.success() && !ran.stdout_over => {
            match read_output(&ran.stdout) {
                (Ok(answer), usage) => (ReviewOutcome::Answered, Some(answer), None, usage),
                (Err(error), usage) => {
                    let (outcome, cause) = output_error_cause(error);
                    (outcome, None, Some(cause), usage)
                }
            }
        }
        Ok(SpawnOutcome::Captured(ran)) if ran.stdout_over => (
            ReviewOutcome::Invalid,
            None,
            Some(EscalationCause::InvalidAnswer),
            CallUsage::default(),
        ),
        Ok(_) => (
            ReviewOutcome::Failed,
            None,
            Some(EscalationCause::CallFailed),
            CallUsage::default(),
        ),
        Err(error) if matches!(error.cause(), SpawnError::Timeout(_)) => (
            ReviewOutcome::Timeout,
            None,
            Some(EscalationCause::CallTimeout),
            CallUsage::default(),
        ),
        Err(_) => (
            ReviewOutcome::Failed,
            None,
            Some(EscalationCause::CallFailed),
            CallUsage::default(),
        ),
    };
    CallResult {
        approval_id: call.approval_id.clone(),
        started_at_ms,
        duration_ms: now_ms() - started_at_ms,
        outcome,
        answer,
        cause,
        usage,
    }
}

/// The `review` thread (AD-86, AD-3 as amended): separate from `triage`, so
/// a dry run of the triage model never delays a review; one call at a time,
/// each result on a channel the collector drains with `try_recv`.
#[derive(Debug)]
pub struct ReviewWorker {
    calls: Sender<ReviewCall>,
    results: Receiver<CallResult>,
    busy: bool,
    gone: bool,
}

impl ReviewWorker {
    /// Starts the thread, which ends once the worker is dropped and its
    /// call, if any, has returned.
    pub fn spawn() -> std::io::Result<ReviewWorker> {
        let (calls, inbox) = mpsc::channel::<ReviewCall>();
        let (outbox, results) = mpsc::channel();
        std::thread::Builder::new()
            .name("review".into())
            .spawn(move || {
                while let Ok(call) = inbox.recv() {
                    if outbox.send(run_call(&call, &crate::files::now_ms)).is_err() {
                        return;
                    }
                }
            })?;
        Ok(ReviewWorker {
            calls,
            results,
            busy: false,
            gone: false,
        })
    }

    /// True when no call runs and the thread is there.
    pub fn idle(&self) -> bool {
        !self.busy && !self.gone
    }

    /// True while the thread is there, busy or not.
    pub fn alive(&self) -> bool {
        !self.gone
    }

    fn submit(&mut self, call: ReviewCall) -> bool {
        if !self.idle() {
            return false;
        }
        if self.calls.send(call).is_err() {
            self.gone = true;
            return false;
        }
        self.busy = true;
        true
    }

    fn try_result(&mut self) -> Option<CallResult> {
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

/// What the collector knows of the window when it reviews (AD-51, D-86).
#[derive(Debug, Clone, Copy)]
pub struct ReviewEnv<'a> {
    pub relay: &'a RelayHome,
    pub officina_home: &'a Path,
    pub claude: Option<&'a Path>,
    pub config_dir: Option<&'a Path>,
    pub base_env: &'a BaseEnv,
    pub no_excerpts: bool,
    /// The latest known 5-hour use, stale or not.
    pub five_hour_used: Option<f64>,
    /// The board last built: the session's subject and the author's last
    /// prompt come from its cards (D-569).
    pub board: Option<&'a Board>,
    /// How long a call may run: [`REVIEW_TIMEOUT_MS`] unless a test lowers
    /// it.
    pub timeout: Duration,
}

/// A request queued for its call, with the mode it was queued in:
/// [`ReviewMode::Shadow`], or [`ReviewMode::Live`] with its `reviewing`
/// mark standing.
#[derive(Debug, Clone)]
struct Queued {
    job: ReviewJob,
    mode: ReviewMode,
}

/// The call in flight, with what its row needs once it answers.
#[derive(Debug, Clone)]
struct InFlight {
    job: ReviewJob,
    mode: ReviewMode,
    rule_baseline: bool,
}

/// The reviews of the window: the worker, the queue and what the board
/// shows of each `auto` request.
#[derive(Debug)]
pub struct Reviews {
    worker: Option<ReviewWorker>,
    queue: VecDeque<Queued>,
    in_flight: Option<InFlight>,
    /// Requests already prechecked this run, so each is reviewed once.
    seen: BTreeSet<String>,
    /// Why each `auto` request is the author's, and in which mode, for the
    /// board (D-143).
    escalations: BTreeMap<String, Escalation>,
    /// When the last call started, as the `review` thread stamped it and
    /// its row records it: the next waits [`CALL_SPACING_MS`] from there.
    /// Not the submitting tick's stamp, which is taken before the tick's
    /// reads and prechecks and so, under load, seconds before the call;
    /// no call is submitted while one is in flight, so the start is known
    /// before the next is due.
    last_call_ms: Option<i64>,
    /// True once this run cleared the marks an earlier window left
    /// (D-157): a mark only holds while the window that wrote it reviews.
    marks_cleared: bool,
}

impl Reviews {
    /// Starts the `review` thread; without one, no request is reviewed and
    /// every `auto` request is the author's, as a `human` one.
    pub fn new(worker: Option<ReviewWorker>) -> Reviews {
        Reviews {
            worker,
            queue: VecDeque::new(),
            in_flight: None,
            seen: BTreeSet::new(),
            escalations: BTreeMap::new(),
            last_call_ms: None,
            marks_cleared: false,
        }
    }

    /// Why each pending `auto` request is the author's, by approval id.
    pub fn escalations(&self) -> &BTreeMap<String, Escalation> {
        &self.escalations
    }

    /// One tick (AD-86): the results back, the new requests prechecked, the
    /// author's decisions copied, the runs linked, the next call started.
    /// Never waits. Returns true when the board's escalations, the rows or
    /// the approval files changed.
    pub fn tick(
        &mut self,
        env: &ReviewEnv<'_>,
        registry: &mut WindowRegistry,
        now_ms: i64,
    ) -> bool {
        if registry.writer().is_none() {
            return false;
        }
        let mut changed = false;
        if !self.marks_cleared {
            changed |= !approvals::read_files(env.relay).reviewing.is_empty();
            approvals::clear_reviewing(env.relay);
            self.marks_cleared = true;
        }
        let state = registry.auto_state();
        changed |= self.take_results(env, registry, state, now_ms);
        let files = approvals::read_files(env.relay);
        changed |= self.discover(env, registry, &files, state, now_ms);
        copy_author_decisions(registry, &files, now_ms);
        link_runs(env.relay, registry, now_ms);
        changed |= self.start_next(env, registry, &files, now_ms);
        self.forget_settled(&files, now_ms);
        changed
    }

    fn take_results(
        &mut self,
        env: &ReviewEnv<'_>,
        registry: &mut WindowRegistry,
        state: AutoState,
        now_ms: i64,
    ) -> bool {
        let Some(worker) = &mut self.worker else {
            return false;
        };
        let results: Vec<CallResult> = std::iter::from_fn(|| worker.try_result()).collect();
        let mut changed = false;
        for result in results {
            self.last_call_ms = Some(result.started_at_ms);
            let Some(flight) = self.in_flight.take() else {
                continue;
            };
            // Live only while approval is still on in mode `review`: a
            // switch turned off meanwhile makes the answer a shadow one.
            let live = flight.mode == ReviewMode::Live
                && mode(state.approval.on, state.mode) == ReviewMode::Live;
            let row_mode = if live {
                ReviewMode::Live
            } else {
                ReviewMode::Shadow
            };
            let decision = result.answer.as_ref().map(|answer| answer.decision);
            let cause = match decision {
                Some(decision) => decide(decision, live),
                None => result.cause,
            };
            let review = NewReview {
                rule_baseline: flight.rule_baseline,
                call_started_at_ms: Some(result.started_at_ms),
                duration_ms: Some(result.duration_ms),
                input_tokens: result.usage.input_tokens,
                output_tokens: result.usage.output_tokens,
                cost_microusd: result.usage.cost_microusd,
                reason: result.answer.map(|answer| answer.reason),
                ..row(&flight.job, row_mode, result.outcome, cause, decision)
            };
            let id = record(env.relay, registry, &review, now_ms);
            let cause = match (cause, id) {
                (None, Some(id)) => approve(
                    env.relay,
                    registry,
                    flight.job.approval_id(),
                    false,
                    id,
                    now_ms,
                ),
                (None, None) => Some(EscalationCause::ApprovalRefused),
                (cause, _) => cause,
            };
            if let Some(cause) = cause {
                self.escalate(flight.job.approval_id(), cause, row_mode);
            }
            if flight.mode == ReviewMode::Live {
                let _ = approvals::set_reviewing(env.relay, flight.job.approval_id(), false);
            }
            changed = true;
        }
        changed
    }

    fn escalate(&mut self, approval_id: &str, cause: EscalationCause, mode: ReviewMode) {
        self.escalations
            .insert(approval_id.to_owned(), Escalation { cause, mode });
    }

    /// True while a review call can be made: the `review` thread is there.
    fn can_call(&self) -> bool {
        self.worker.as_ref().is_some_and(ReviewWorker::alive)
    }

    /// Prechecks every pending request not seen yet (AD-87), in the mode
    /// auto approval is in (AD-88). Shadow: an `auto` request escalates at
    /// once, with its row and its line, or is queued, its card saying
    /// `auto_approval_off`. Live: it escalates at once or is queued under
    /// its `reviewing` mark. Rule: the rule's precheck, then the baseline
    /// approves or escalates it, without a call.
    fn discover(
        &mut self,
        env: &ReviewEnv<'_>,
        registry: &mut WindowRegistry,
        files: &ApprovalFiles,
        state: AutoState,
        now_ms: i64,
    ) -> bool {
        // Oldest first, so the queue reviews them in the order they came.
        let mut fresh: Vec<(i64, &String)> = files
            .requests
            .iter()
            .filter(|(id, request)| {
                !self.seen.contains(*id)
                    && !files.decided(id)
                    && is_live(request.created_at_ms, now_ms)
            })
            .map(|(id, request)| (request.created_at_ms, id))
            .collect();
        fresh.sort();
        let fresh: Vec<&String> = fresh.into_iter().map(|(_, id)| id).collect();
        if fresh.is_empty() {
            return false;
        }
        let reviewed = registry
            .writer()
            .and_then(|w| w.reviewed_requests(now_ms - PENDING_MS).ok())
            .unwrap_or_default();
        let records = review_records(registry, now_ms);
        let sources = ContextSources::read(env.relay);
        let ctx = sources.context();
        let review_model = ctx
            .approved
            .and_then(|approved| approved.canonical.auto_review.model);
        let claude = env.claude.map(|path| path.to_string_lossy().into_owned());
        let facts = PrecheckFacts {
            claude: claude.as_deref(),
            workdir_empty: workdir_empty(env.officina_home),
            no_excerpts: env.no_excerpts,
            five_hour_used: env.five_hour_used,
            rows: &records,
            review_model,
            now_ms,
        };
        let current = mode(state.approval.on, state.mode);
        let mut changed = false;
        for id in fresh {
            self.seen.insert(id.clone());
            if reviewed.contains(id) {
                continue;
            }
            let request = &files.requests[id];
            let found = if current == ReviewMode::Rule {
                let records = review_records(registry, now_ms);
                precheck_rule(&ctx, id, request, &records, now_ms)
            } else {
                precheck(&ctx, id, request, &facts)
            };
            match found {
                Precheck::NotAuto => {}
                Precheck::Escalate(cause) => {
                    let review = NewReview {
                        approval_id: id.clone(),
                        request_key: request.request_key.clone(),
                        project: request.project.clone(),
                        action: request.action.clone(),
                        requested_at_ms: request.created_at_ms,
                        mode: current,
                        outcome: ReviewOutcome::NoCall,
                        cause: Some(cause),
                        decision: None,
                        reason: None,
                        rule_baseline: false,
                        call_started_at_ms: None,
                        duration_ms: None,
                        input_tokens: None,
                        output_tokens: None,
                        cost_microusd: None,
                    };
                    record(env.relay, registry, &review, now_ms);
                    self.escalate(id, cause, current);
                    changed = true;
                }
                Precheck::Review(job) => match current {
                    ReviewMode::Shadow => {
                        self.escalate(id, EscalationCause::AutoApprovalOff, current);
                        self.queue.push_back(Queued {
                            job: *job,
                            mode: current,
                        });
                        changed = true;
                    }
                    ReviewMode::Live if !self.can_call() => {
                        // No `review` thread: nothing would ever answer, so
                        // the request is the author's now, never left
                        // reviewing.
                        let review = row(
                            &job,
                            current,
                            ReviewOutcome::NoCall,
                            Some(EscalationCause::CallFailed),
                            None,
                        );
                        record(env.relay, registry, &review, now_ms);
                        self.escalate(id, EscalationCause::CallFailed, current);
                        changed = true;
                    }
                    ReviewMode::Live => {
                        let _ = approvals::set_reviewing(env.relay, id, true);
                        self.queue.push_back(Queued {
                            job: *job,
                            mode: current,
                        });
                        changed = true;
                    }
                    ReviewMode::Rule => {
                        self.decide_by_rule(env, registry, &job, now_ms);
                        changed = true;
                    }
                },
            }
        }
        changed
    }

    /// Rule mode (D-141, D-162): one row of mode `rule`, outcome `no_call`,
    /// with the baseline's decision; a true baseline approves `by = "rule"`
    /// through the window's authority, a false one escalates
    /// `rule_baseline_false`.
    fn decide_by_rule(
        &mut self,
        env: &ReviewEnv<'_>,
        registry: &mut WindowRegistry,
        job: &ReviewJob,
        now_ms: i64,
    ) {
        let (lines, tree) = lines_and_tree(env.relay, job, now_ms);
        let baseline = rule_baseline(job, &lines, tree.as_ref(), now_ms);
        let (decision, cause) = rule_decision(baseline);
        let mut review = row(
            job,
            ReviewMode::Rule,
            ReviewOutcome::NoCall,
            cause,
            Some(decision),
        );
        review.rule_baseline = baseline;
        let id = record(env.relay, registry, &review, now_ms);
        let cause = match (cause, id) {
            (None, Some(id)) => approve(env.relay, registry, job.approval_id(), true, id, now_ms),
            (None, None) => Some(EscalationCause::ApprovalRefused),
            (cause, _) => cause,
        };
        if let Some(cause) = cause {
            self.escalate(job.approval_id(), cause, ReviewMode::Rule);
        }
    }

    /// Starts the oldest queued call when the worker is idle and the last
    /// call started [`CALL_SPACING_MS`] ago (D-142's pacing); right before
    /// it, `quota_high`, `hourly_cap` and the budget are checked again, and
    /// a request one of them now stops escalates without a call.
    fn start_next(
        &mut self,
        env: &ReviewEnv<'_>,
        registry: &mut WindowRegistry,
        files: &ApprovalFiles,
        now_ms: i64,
    ) -> bool {
        if !self.can_call() {
            return self.fail_live_queue(env, registry, now_ms);
        }
        let Some(worker) = &self.worker else {
            return false;
        };
        if !worker.idle() || self.in_flight.is_some() {
            return false;
        }
        if self
            .last_call_ms
            .is_some_and(|last| now_ms - last < CALL_SPACING_MS)
        {
            return false;
        }
        let mut changed = false;
        while let Some(Queued { job, mode }) = self.queue.pop_front() {
            let live = mode == ReviewMode::Live;
            let id = job.approval_id().to_owned();
            if live && !still_pending(files, &id, now_ms) {
                // Decided or expired before its turn: nothing to review.
                let _ = approvals::set_reviewing(env.relay, &id, false);
                changed = true;
                continue;
            }
            let records = review_records(registry, now_ms);
            let facts = PrecheckFacts {
                claude: None,
                workdir_empty: true,
                no_excerpts: env.no_excerpts,
                five_hour_used: env.five_hour_used,
                rows: &records,
                review_model: Some(job.model()),
                now_ms,
            };
            let cause =
                recheck(job.project(), job.action(), job.max_per_hour(), &facts).or_else(|| {
                    (live && env.claude.is_none()).then_some(EscalationCause::ClaudeNotFound)
                });
            if let Some(cause) = cause {
                record(
                    env.relay,
                    registry,
                    &row(&job, mode, ReviewOutcome::NoCall, Some(cause), None),
                    now_ms,
                );
                self.escalate(&id, cause, mode);
                if live {
                    let _ = approvals::set_reviewing(env.relay, &id, false);
                }
                changed = true;
                continue;
            }
            let Some(claude) = env.claude else {
                // Shadow: the request is the author's already; its call
                // waits for a `claude` binary.
                self.queue.push_front(Queued { job, mode });
                break;
            };
            let (lines, tree) = lines_and_tree(env.relay, &job, now_ms);
            let context = ReviewContext {
                subject: card_of(env.board, job.session_id())
                    .and_then(|card| card.subject.as_ref().map(|s| s.name.clone())),
                last_prompt: card_of(env.board, job.session_id())
                    .and_then(|card| card.last_prompt.clone()),
                outcomes: past_outcomes(&lines, job.project(), job.action()),
                check: check_status(&job, &lines, tree.as_ref(), now_ms),
            };
            let baseline = rule_baseline(&job, &lines, tree.as_ref(), now_ms);
            let Some(text) = (0..NONCE_DRAWS)
                .map(|_| prompt(&job, &context, &draw_nonce()))
                .find(|built| !matches!(built, Err(PromptError::NonceInInput)))
                .and_then(Result::ok)
            else {
                let mut review = row(
                    &job,
                    mode,
                    ReviewOutcome::Failed,
                    Some(EscalationCause::CallFailed),
                    None,
                );
                review.rule_baseline = baseline;
                record(env.relay, registry, &review, now_ms);
                self.escalate(&id, EscalationCause::CallFailed, mode);
                if live {
                    let _ = approvals::set_reviewing(env.relay, &id, false);
                }
                changed = true;
                continue;
            };
            let call = ReviewCall {
                approval_id: id,
                model: job.model(),
                prompt: text,
                claude: claude.to_path_buf(),
                officina_home: env.officina_home.to_path_buf(),
                config_dir: env.config_dir.map(Path::to_path_buf),
                base_env: env.base_env.clone(),
                timeout: env.timeout,
            };
            let Some(worker) = &mut self.worker else {
                break;
            };
            if worker.submit(call) {
                self.in_flight = Some(InFlight {
                    job,
                    mode,
                    rule_baseline: baseline,
                });
            } else if live {
                // The thread is gone: nothing will answer this request.
                let cause = EscalationCause::CallFailed;
                record(
                    env.relay,
                    registry,
                    &row(&job, mode, ReviewOutcome::NoCall, Some(cause), None),
                    now_ms,
                );
                self.escalate(job.approval_id(), cause, mode);
                let _ = approvals::set_reviewing(env.relay, job.approval_id(), false);
                changed = true;
                continue;
            } else {
                self.queue.push_front(Queued { job, mode });
            }
            break;
        }
        changed
    }

    /// Without a `review` thread, every queued live request is the
    /// author's at once (`call_failed`), never left reviewing; a shadow one
    /// is his already and stays queued.
    fn fail_live_queue(
        &mut self,
        env: &ReviewEnv<'_>,
        registry: &mut WindowRegistry,
        now_ms: i64,
    ) -> bool {
        let (live, shadow): (Vec<Queued>, Vec<Queued>) = self
            .queue
            .drain(..)
            .partition(|queued| queued.mode == ReviewMode::Live);
        self.queue.extend(shadow);
        let cause = EscalationCause::CallFailed;
        for Queued { job, mode } in &live {
            record(
                env.relay,
                registry,
                &row(job, *mode, ReviewOutcome::NoCall, Some(cause), None),
                now_ms,
            );
            self.escalate(job.approval_id(), cause, *mode);
            let _ = approvals::set_reviewing(env.relay, job.approval_id(), false);
        }
        !live.is_empty()
    }

    /// Drops what this run kept of requests no longer pending.
    fn forget_settled(&mut self, files: &ApprovalFiles, now_ms: i64) {
        let pending = |id: &String| {
            files
                .requests
                .get(id)
                .is_some_and(|r| !files.decided(id) && is_live(r.created_at_ms, now_ms))
        };
        self.escalations.retain(|id, _| pending(id));
        self.seen.retain(|id| {
            files
                .requests
                .get(id)
                .is_some_and(|r| is_live(r.created_at_ms, now_ms))
        });
    }
}

/// The card of `session_id` on `board`, when it shows one.
fn card_of<'b>(
    board: Option<&'b Board>,
    session_id: &str,
) -> Option<&'b officina_core::board::Card> {
    board?
        .columns
        .all()
        .find(|card| card.session_id.as_deref() == Some(session_id))
}

/// The row of `job` with its request fields, `mode`, `outcome`, `cause` and
/// `decision`; no call, cost or reason.
fn row(
    job: &ReviewJob,
    mode: ReviewMode,
    outcome: ReviewOutcome,
    cause: Option<EscalationCause>,
    decision: Option<ReviewDecision>,
) -> NewReview {
    NewReview {
        approval_id: job.approval_id().to_owned(),
        request_key: job.request_key().to_owned(),
        project: job.project().to_owned(),
        action: job.action().to_owned(),
        requested_at_ms: job.requested_at_ms(),
        mode,
        outcome,
        cause,
        decision,
        reason: None,
        rule_baseline: false,
        call_started_at_ms: None,
        duration_ms: None,
        input_tokens: None,
        output_tokens: None,
        cost_microusd: None,
    }
}

/// The rows the cap and the budget count: the last day's (D-142).
fn review_records(
    registry: &mut WindowRegistry,
    now_ms: i64,
) -> Vec<officina_core::relay::auto::ReviewRecord> {
    review_records_but(registry, now_ms, None)
}

/// [`review_records`] without row `except`.
fn review_records_but(
    registry: &mut WindowRegistry,
    now_ms: i64,
    except: Option<i64>,
) -> Vec<officina_core::relay::auto::ReviewRecord> {
    registry
        .writer()
        .and_then(|writer| writer.review_rows(now_ms - DAY_MS).ok())
        .unwrap_or_default()
        .iter()
        .filter(|row| Some(row.id) != except)
        .map(ReviewRow::record)
        .collect()
}

/// True while request `id` is pending: there, undecided, not expired.
fn still_pending(files: &ApprovalFiles, id: &str, now_ms: i64) -> bool {
    files.requests.get(id).is_some_and(|request| {
        !files.decided(id) && !files.markers.contains(id) && is_live(request.created_at_ms, now_ms)
    })
}

/// The journal's last two days and the tree of `job`'s project now: what
/// the review's context and the rule baseline read.
fn lines_and_tree(
    relay: &RelayHome,
    job: &ReviewJob,
    now_ms: i64,
) -> (Vec<JournalLine>, Option<TreeFingerprint>) {
    let (lines, _) = journal::read_recent(relay, now_ms);
    let folder = ContextSources::read(relay)
        .project_dirs
        .get(job.project())
        .cloned();
    let tree = folder.and_then(|dir| tree::fingerprint(Path::new(&dir)).ok().flatten());
    (lines, tree)
}

/// Has the window's authority record the approval of `approval_id` by a
/// review, or the rule when `by_rule`, from row `review_id` (AD-86, D-157):
/// `None` once recorded; otherwise the cause the request escalates with,
/// set on the row and journaled: `hourly_cap` when the cap is reached,
/// `approval_refused` for any other refusal (D-734).
fn approve(
    relay: &RelayHome,
    registry: &mut WindowRegistry,
    approval_id: &str,
    by_rule: bool,
    review_id: i64,
    now_ms: i64,
) -> Option<EscalationCause> {
    let records = review_records_but(registry, now_ms, Some(review_id));
    let cause =
        match authority::record_review_approval(relay, approval_id, by_rule, review_id, &records) {
            Ok(()) => return None,
            Err(AuthorityError::Decide(DecideRefusal::HourlyCap(_))) => EscalationCause::HourlyCap,
            Err(_) => EscalationCause::ApprovalRefused,
        };
    registry.set_review_cause(review_id, cause);
    let line = JournalEntry::AutoEscalated(AutoEscalated {
        approval_id: approval_id.to_owned(),
        cause,
    });
    let _ = journal::record(relay, &[line], now_ms);
    Some(cause)
}

/// Links each run `officina run` started from an approval a review or the
/// rule gave to that approval's row, through `run_started.approval_id`
/// (AD-88), so "should not have run" can name it. Reads the journal only
/// while such an approval is unlinked.
pub fn link_runs(relay: &RelayHome, registry: &mut WindowRegistry, now_ms: i64) {
    let unlinked = registry.unlinked_approvals(now_ms - MARK_WINDOW_MS);
    if unlinked.is_empty() {
        return;
    }
    let (lines, _) = journal::read_recent(relay, now_ms);
    let runs: Vec<(String, String)> = lines
        .iter()
        .filter_map(|line| match &line.entry {
            JournalEntry::RunStarted(started) => started
                .approval_id
                .as_ref()
                .filter(|id| unlinked.contains(*id))
                .map(|id| (id.clone(), started.run_id.clone())),
            _ => None,
        })
        .collect();
    if !runs.is_empty() {
        registry.link_runs(&runs);
    }
}

/// Writes `review`'s row, then its journal lines: `auto_review` for a
/// call, `auto_escalated` with its cause; never the reason (AD-32 as
/// amended). A journal that cannot be written, even by a spill (D-760),
/// loses the line, not the row. Returns the row's id, `None` when the
/// registry could not take it.
fn record(
    relay: &RelayHome,
    registry: &mut WindowRegistry,
    review: &NewReview,
    now_ms: i64,
) -> Option<i64> {
    let id = registry.insert_review(review);
    let mut lines = Vec::new();
    if review.outcome != ReviewOutcome::NoCall {
        lines.push(JournalEntry::AutoReview(AutoReviewed {
            approval_id: review.approval_id.clone(),
            outcome: review.outcome,
            mode: review.mode,
            cause: review.cause,
            decision: review.decision,
        }));
    }
    if let Some(cause) = review.cause {
        lines.push(JournalEntry::AutoEscalated(AutoEscalated {
            approval_id: review.approval_id.clone(),
            cause,
        }));
    }
    let _ = journal::record(relay, &lines, now_ms);
    id
}

/// The author's decision on each reviewed request, copied onto its row
/// (AD-88): an approval or a rejection the author gave, with its reason;
/// a decision by a review or the rule, or an expiry, is none.
fn copy_author_decisions(registry: &mut WindowRegistry, files: &ApprovalFiles, now_ms: i64) {
    let Some(undecided) = registry
        .writer()
        .and_then(|writer| writer.undecided_reviews(now_ms - DAY_MS).ok())
    else {
        return;
    };
    let by_author = |decision: &Decision| decision.by() == ApprovalBy::Author;
    for review in undecided {
        let id = &review.approval_id;
        let approved = files
            .approved
            .get(id)
            .or_else(|| files.consumed.get(id))
            .filter(|d| by_author(d));
        let found = match (approved, files.rejected.get(id).filter(|d| by_author(d))) {
            (Some(decision), _) => Some((AuthorDecision::Approved, None, decision.decided_at_ms)),
            (None, Some(decision)) => Some((
                AuthorDecision::Rejected,
                decision.reason,
                decision.decided_at_ms,
            )),
            (None, None) => None,
        };
        if let Some((decision, reason, at_ms)) = found {
            registry.copy_author_decision(review.id, decision, reason, at_ms);
        }
    }
}

/// The review's timeout as a duration.
pub const fn review_timeout() -> Duration {
    Duration::from_millis(REVIEW_TIMEOUT_MS)
}
