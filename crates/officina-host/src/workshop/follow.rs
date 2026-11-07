//! Following a run and finding it again (AD-109, D-216, D-230): one
//! follower per live container, its lines folded by core's
//! [`RunOutput`] in the window's memory, the worker stopped on a sign the
//! stream gives, on the output cap or past its deadline, and every run
//! reconciled with the engine at window start and when the engine comes
//! back. The collector gets what changes a row or the journal; texts leave
//! memory only with the credential's forms replaced, or wait for
//! collection's scan when the window holds no needles.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use officina_core::relay::journal::{
    GatewayRefused, JournalEntry, WorkshopFinished, WorkshopRemoved,
};
use officina_core::untrusted::Untrusted;
use officina_core::workshop::engine::{EngineCall, EngineInfo, parse_inspect, parse_ps};
use officina_core::workshop::floor::CredentialKind;
use officina_core::workshop::lifecycle::{
    ContainerSeen, DEADLINE_GRACE_MS, FinishInput, ReconcileAction, RowSeen, RunEvent, StopReason,
    finish_cause, reconcile,
};
use officina_core::workshop::names::{FLOOR_LABEL, ROLE_LABEL, RUN_LABEL, RunId, RunNames};
use officina_core::workshop::output::{
    LineRead, MAX_HELD_REFUSALS, OutputEvent, Pipe, Refusal, RunOutput, clean_host,
    without_timestamp,
};
use officina_core::workshop::run::{
    FinishCause, Recorded, RemovalCause, RunFacts, RunRefusal, RunState, RunTexts, WorkshopOutcome,
};
use officina_core::workshop::sensitive::PackageGroup;
use officina_core::workshop::stream::{HookCounts, MAX_LINE_BYTES};
use officina_core::workshop::vector::ContainerRole;
use officina_core::workshop::view::OrphanView;

use crate::collector::Collector;
use crate::process::{BoundedLine, FollowedPipe, Following};
use crate::registry::window::WindowRegistry;
use crate::registry::workshop::RunRecord;
use crate::watch::{AUTHOR_WAIT, AuthorAnswer, AuthorCommand, AuthorRequest, CollectorMsg, ask};
use crate::workshop::engine::{ResolvedDocker, execute, follow};
use crate::workshop::launch::{RunNeedles, WorkshopRuntime, credential_store_name, run_dir};

/// How often a watcher looks at its run's deadline.
const DEADLINE_TICK: Duration = Duration::from_secs(1);

/// How often thread `workshop` asks the engine again while it does not
/// answer and a run is not terminal (D-216).
pub const ENGINE_RETRY: Duration = Duration::from_secs(30);

/// What the window keeps of one run it follows (D-230): its needles, its
/// output, its stop, and the refusals waiting for collection's scan.
pub struct Followed {
    needles: Option<RunNeedles>,
    pub output: RunOutput,
    pub stop: Option<StopReason>,
    /// The highest refusal rank journaled, or held.
    pub journaled: u32,
    /// Refusals read while no needles exist, bounded (D-216): journaled
    /// only after collection's scan (story 6.14).
    pub held: Vec<Refusal>,
    /// Refusals as journaled, their hosts replaced, bounded.
    refusals: Vec<RunRefusal>,
    /// Refusal hosts the needles replaced while following (D-230): each
    /// a "gateway line" location for collection's scan (AD-112).
    credential_hits: u32,
    gateway_exit: Option<i64>,
    watching: bool,
}

impl Followed {
    /// A run followed with `needles`, none for a `login` run or a
    /// credential the window could not read (D-230, D-609).
    pub fn new(needles: Option<RunNeedles>) -> Followed {
        Followed {
            needles,
            output: RunOutput::default(),
            stop: None,
            journaled: 0,
            held: Vec::new(),
            refusals: Vec::new(),
            credential_hits: 0,
            gateway_exit: None,
            watching: false,
        }
    }

    pub fn needles(&self) -> Option<&RunNeedles> {
        self.needles.as_ref()
    }

    /// The refusals journaled while following, their hosts replaced.
    pub fn refusals(&self) -> &[RunRefusal] {
        &self.refusals
    }

    /// How many refusal hosts the needles replaced while following.
    pub fn credential_hits(&self) -> u32 {
        self.credential_hits
    }

    /// The run's texts as they may leave memory (D-230): replaced, or
    /// `None` while the window holds no needles.
    pub fn texts(&self) -> Option<officina_core::workshop::output::OutputTexts> {
        self.needles
            .as_ref()
            .map(|needles| self.output.texts(needles.needles()))
    }

    /// The texts the registry keeps of the run (AD-119), replaced; `None`
    /// while the window holds no needles (D-230).
    pub fn run_texts(&self) -> Option<RunTexts> {
        let texts = self.texts()?;
        Some(RunTexts {
            report_tail: texts.assistant_tail,
            refusals: self.refusals.clone(),
            allowed_hosts: texts.allowed_hosts,
            ..RunTexts::default()
        })
    }

    /// The gateway's exit code, once its container exited.
    pub fn gateway_exit(&self) -> Option<i64> {
        self.gateway_exit
    }

    /// The facts a `workshop_finished` line carries (AD-120).
    pub fn summary(&self) -> RunSummary {
        let result = self.output.stream.result.as_ref();
        RunSummary {
            hooks: self.output.stream.hooks.clone(),
            refused: self.output.gateway.refused,
            stream_bytes: self.output.worker_bytes,
            cost_usd_micros: result
                .and_then(|r| r.cost_usd)
                .filter(|cost| cost.is_finite() && *cost >= 0.0)
                .map(|cost| (cost * 1_000_000.0).round() as u64),
            turns: result.and_then(|r| r.turns),
            result_subtype: self.texts().and_then(|t| t.result_subtype).map(|s| {
                serde_json::to_value(&s)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default()
            }),
        }
    }
}

impl std::fmt::Debug for Followed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Followed")
            .field("stop", &self.stop)
            .field("journaled", &self.journaled)
            .finish_non_exhaustive()
    }
}

/// Every run the window follows, by run id.
pub type FollowedRuns = Arc<Mutex<BTreeMap<String, Followed>>>;

/// What a `workshop_finished` line takes from a run's output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunSummary {
    pub hooks: BTreeMap<String, HookCounts>,
    pub refused: u32,
    pub stream_bytes: u64,
    pub cost_usd_micros: Option<u64>,
    pub turns: Option<u64>,
    pub result_subtype: Option<String>,
}

/// What a follower tells the collector (AD-109). Each is a request the
/// collector never drops.
#[derive(Debug, Clone, PartialEq)]
pub enum FollowEvent {
    /// Journal this refusal (AD-120), its host already replaced, and
    /// count it journaled.
    Refused { line: GatewayRefused, ordinal: u32 },
    /// The window stopped the worker for `reason`: `running` to
    /// `stopping`, the reason recorded.
    StopAsked { run_id: String, reason: StopReason },
    /// The worker exited: `collecting`, how it ended, its texts when they
    /// may be stored.
    Exited {
        run_id: String,
        cause: FinishCause,
        exit_code: Option<i64>,
        texts: Option<RunTexts>,
    },
    /// What reconciliation found that changes rows: launches that never
    /// came back (their objects removed), runs the engine lost, and the
    /// orphans it lists.
    Reconciled {
        failed: Vec<String>,
        lost: Vec<(String, RunSummary)>,
        orphans: Vec<OrphanView>,
    },
    /// What collection found (AD-110): the row's outcome, its package
    /// fields and the journal lines it owes.
    Collected(Box<crate::workshop::collect::Collected>),
}

/// Where a run's watcher works from.
#[derive(Clone)]
pub struct FollowContext {
    pub runtime: WorkshopRuntime,
    pub collector: Sender<CollectorMsg>,
    pub home: PathBuf,
    pub base_env: Vec<(String, String)>,
}

impl FollowContext {
    fn docker(&self) -> Option<&ResolvedDocker> {
        self.runtime.docker.as_ref()
    }

    fn tell(&self, event: FollowEvent) {
        let (request, _answer) =
            AuthorRequest::new(AuthorCommand::WorkshopFollow(Box::new(event)), AUTHOR_WAIT);
        let _ = self.collector.send(CollectorMsg::Author(request));
    }

    /// The container's state; `Err` when the engine gave no answer,
    /// `Ok(None)` when it answered without that container.
    fn inspect(
        &self,
        name: &officina_core::workshop::names::ObjectName,
    ) -> Result<Option<officina_core::workshop::engine::ContainerState>, ()> {
        let docker = self.docker().ok_or(())?;
        let outcome = execute(
            docker,
            &EngineCall::inspect(name),
            &self.base_env,
            &self.home,
            None,
        )
        .map_err(|_| ())?;
        if outcome.succeeded() {
            return Ok(parse_inspect(&String::from_utf8_lossy(&outcome.stdout)));
        }
        if outcome.stderr.to_ascii_lowercase().contains("no such") {
            return Ok(None);
        }
        Err(())
    }

    /// True when the engine answers `version` as one that runs the
    /// workshop.
    fn engine_answers(&self) -> bool {
        let Some(docker) = self.docker() else {
            return false;
        };
        execute(
            docker,
            &EngineCall::version(),
            &self.base_env,
            &self.home,
            None,
        )
        .ok()
        .is_some_and(|outcome| EngineInfo::parse(&String::from_utf8_lossy(&outcome.stdout)).is_ok())
    }
}

/// One run to follow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunToFollow {
    pub run_id: String,
    /// When the window stops the worker: its deadline plus the grace.
    pub stop_at_ms: i64,
    /// The refusals already journaled, which a replay does not journal
    /// again.
    pub journaled: u32,
}

impl RunToFollow {
    /// The run of `row`, on the window's clock (D-242).
    pub fn of(row: &RunRecord) -> RunToFollow {
        RunToFollow {
            run_id: row.id.clone(),
            stop_at_ms: row
                .started_at_ms
                .saturating_add(row.deadline_ms)
                .saturating_add(DEADLINE_GRACE_MS),
            journaled: row.facts.refusals_journaled,
        }
    }
}

enum Heard {
    Line(ContainerRole, Pipe, LineRead),
    Ended(ContainerRole),
}

fn heard_line(read: BoundedLine) -> LineRead {
    match read {
        BoundedLine::Line(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            LineRead::Line(without_timestamp(&text).to_owned())
        }
        BoundedLine::Oversized => LineRead::Oversized,
    }
}

/// Starts `role`'s follower of `names`, from the beginning of its logs.
fn start_follower(
    context: &FollowContext,
    names: &RunNames,
    role: ContainerRole,
    heard: &Sender<Heard>,
) -> Option<Following> {
    let name = match role {
        ContainerRole::Gateway => &names.gateway,
        _ => &names.worker,
    };
    let lines = heard.clone();
    let ended = heard.clone();
    follow(
        context.docker()?,
        &EngineCall::logs(name),
        &context.base_env,
        &context.home,
        MAX_LINE_BYTES + 1,
        Arc::new(move |pipe, read| {
            let pipe = match pipe {
                FollowedPipe::Stdout => Pipe::Stdout,
                FollowedPipe::Stderr => Pipe::Stderr,
            };
            let _ = lines.send(Heard::Line(role, pipe, heard_line(read)));
        }),
        Box::new(move || {
            let _ = ended.send(Heard::Ended(role));
        }),
    )
    .ok()
}

/// Starts following `run` on a thread of its own, unless the window
/// already does; `needles` are kept for it when it was not followed yet.
pub fn start(
    context: &FollowContext,
    run: RunToFollow,
    needles: impl FnOnce() -> Option<RunNeedles>,
) {
    {
        let Ok(mut followed) = context.runtime.followed.lock() else {
            return;
        };
        let entry = followed
            .entry(run.run_id.clone())
            .or_insert_with(|| Followed {
                journaled: run.journaled,
                ..Followed::new(needles())
            });
        if entry.watching {
            return;
        }
        entry.watching = true;
    }
    let context = context.clone();
    let spawned = std::thread::Builder::new()
        .name(format!("follow-{}", &run.run_id[..8.min(run.run_id.len())]))
        .spawn({
            let context = context.clone();
            let run = run.clone();
            move || watch(&context, &run)
        });
    if spawned.is_err()
        && let Ok(mut followed) = context.runtime.followed.lock()
        && let Some(entry) = followed.get_mut(&run.run_id)
    {
        entry.watching = false;
    }
}

/// Asks thread `workshop` to stop the worker of `run_id` for `reason`,
/// once (AD-109): no flag is set.
fn stop_worker(context: &FollowContext, run_id: &str, names: &RunNames, reason: StopReason) {
    {
        let Ok(mut followed) = context.runtime.followed.lock() else {
            return;
        };
        let Some(entry) = followed.get_mut(run_id) else {
            return;
        };
        if entry.stop.is_some() {
            return;
        }
        entry.stop = Some(reason);
    }
    context.tell(FollowEvent::StopAsked {
        run_id: run_id.to_owned(),
        reason,
    });
    let job = context.clone();
    let worker = names.worker.clone();
    context.runtime.thread.submit(move || {
        if let Some(docker) = job.docker() {
            let _ = execute(
                docker,
                &EngineCall::stop(std::slice::from_ref(&worker)),
                &job.base_env,
                &job.home,
                None,
            );
        }
    });
}

/// Folds one line of `run_id` and acts on what it asks (AD-109, D-230).
fn heard(
    context: &FollowContext,
    run_id: &str,
    names: &RunNames,
    role: ContainerRole,
    pipe: Pipe,
    read: &LineRead,
) {
    let (journal, stops) = {
        let Ok(mut followed) = context.runtime.followed.lock() else {
            return;
        };
        let Some(entry) = followed.get_mut(run_id) else {
            return;
        };
        entry.absorb(run_id, role, pipe, read)
    };
    for event in journal {
        context.tell(event);
    }
    if let Some(reason) = stops.first() {
        stop_worker(context, run_id, names, *reason);
    }
}

impl Followed {
    /// Folds one line of run `run_id` (AD-109, D-230): the refusals to
    /// journal now, their hosts replaced, each once whatever replays; the
    /// ones read without needles held; and the stops the line asks for.
    pub fn absorb(
        &mut self,
        run_id: &str,
        role: ContainerRole,
        pipe: Pipe,
        read: &LineRead,
    ) -> (Vec<FollowEvent>, Vec<StopReason>) {
        let mut journal = Vec::new();
        let mut stops = Vec::new();
        for event in self.output.observe(role, pipe, read) {
            match event {
                OutputEvent::Stop(reason) => stops.push(reason),
                OutputEvent::Refused(refusal) if refusal.ordinal > self.journaled => {
                    self.journaled = refusal.ordinal;
                    match self.needles.as_ref() {
                        Some(needles) => {
                            let host = clean_host(&refusal.host, needles.needles());
                            if !officina_core::workshop::scan::find(
                                refusal.host.as_bytes(),
                                needles.needles(),
                            )
                            .is_empty()
                            {
                                self.credential_hits = self.credential_hits.saturating_add(1);
                            }
                            if self.refusals.len() < MAX_HELD_REFUSALS {
                                self.refusals.push(RunRefusal {
                                    host: Untrusted::new(host.clone()),
                                    port: refusal.port,
                                    cause: refusal.cause,
                                });
                            }
                            journal.push(FollowEvent::Refused {
                                line: GatewayRefused {
                                    run_id: Some(run_id.to_owned()),
                                    battery_id: None,
                                    host,
                                    port: refusal.port,
                                    cause: refusal.cause,
                                },
                                ordinal: refusal.ordinal,
                            });
                        }
                        None if self.held.len() < MAX_HELD_REFUSALS => self.held.push(refusal),
                        None => {}
                    }
                }
                OutputEvent::Refused(_) => {}
            }
        }
        (journal, stops)
    }
}

/// The worker exited: its cause decided by core from the engine's facts,
/// the window's stop reason, the gateway's exit and the result (AD-109).
fn worker_exited(
    context: &FollowContext,
    run_id: &str,
    state: &officina_core::workshop::engine::ContainerState,
) {
    let event = {
        let Ok(followed) = context.runtime.followed.lock() else {
            return;
        };
        let Some(entry) = followed.get(run_id) else {
            return;
        };
        let subtype = entry
            .output
            .stream
            .result
            .as_ref()
            .and_then(|r| r.subtype.as_ref())
            .and_then(|s| serde_json::to_value(s).ok())
            .and_then(|v| v.as_str().map(str::to_owned));
        let cause = finish_cause(&FinishInput {
            lost: false,
            stop: entry.stop,
            oom_killed: state.oom_killed,
            exit_code: state.exit_code,
            gateway_exit: entry.gateway_exit,
            result_subtype: subtype,
        });
        FollowEvent::Exited {
            run_id: run_id.to_owned(),
            cause,
            exit_code: state.exit_code,
            texts: entry.run_texts(),
        }
    };
    context.tell(event);
}

/// The watcher of one run (AD-109): both followers, the deadline, and a
/// follower that ends while its container runs started again from the
/// beginning, that role's facts rebuilt from nothing.
fn watch(context: &FollowContext, run: &RunToFollow) {
    let done = |context: &FollowContext| {
        if let Ok(mut followed) = context.runtime.followed.lock()
            && let Some(entry) = followed.get_mut(&run.run_id)
        {
            entry.watching = false;
        }
    };
    let Some(id) = RunId::parse(&run.run_id) else {
        return done(context);
    };
    let names = RunNames::of(&id);
    let (sender, inbox) = mpsc::channel::<Heard>();
    let mut followers: BTreeMap<&'static str, Following> = BTreeMap::new();
    let key = |role: ContainerRole| role.code();
    for role in [ContainerRole::Worker, ContainerRole::Gateway] {
        if let Some(following) = start_follower(context, &names, role, &sender) {
            followers.insert(key(role), following);
        } else {
            return done(context);
        }
    }
    let mut worker_done = false;
    let mut gateway_done = false;
    while !(worker_done && gateway_done) {
        match inbox.recv_timeout(DEADLINE_TICK) {
            Ok(Heard::Line(role, pipe, read)) => {
                heard(context, &run.run_id, &names, role, pipe, &read);
            }
            Ok(Heard::Ended(role)) => {
                followers.remove(key(role));
                let name = if role == ContainerRole::Gateway {
                    &names.gateway
                } else {
                    &names.worker
                };
                match context.inspect(name) {
                    Ok(Some(state)) if state.running => {
                        if let Ok(mut followed) = context.runtime.followed.lock()
                            && let Some(entry) = followed.get_mut(&run.run_id)
                        {
                            entry.output.restart(role);
                        }
                        match start_follower(context, &names, role, &sender) {
                            Some(following) => {
                                followers.insert(key(role), following);
                            }
                            None => break,
                        }
                    }
                    Ok(Some(state)) => {
                        if role == ContainerRole::Gateway {
                            gateway_done = true;
                            if let Ok(mut followed) = context.runtime.followed.lock()
                                && let Some(entry) = followed.get_mut(&run.run_id)
                            {
                                entry.gateway_exit = state.exit_code;
                            }
                        } else {
                            worker_done = true;
                            worker_exited(context, &run.run_id, &state);
                            // AD-110: collection starts once the gateway
                            // is stopped and its follower drained.
                            stop_gateway(context, &names);
                        }
                    }
                    // The engine answered without the container, or did
                    // not answer: reconciliation decides, no row changes
                    // here (AD-109).
                    Ok(None) | Err(()) => {
                        drop(followers);
                        done(context);
                        reconcile_on_thread(context.clone());
                        return;
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if !worker_done && (context.runtime.now_ms)() > run.stop_at_ms {
            stop_worker(context, &run.run_id, &names, StopReason::Deadline);
        }
    }
    done(context);
    if worker_done && gateway_done {
        crate::workshop::collect::collect_on_thread(context, &run.run_id);
    }
}

/// Stops the run's gateway once its worker exited (AD-110), on the
/// watcher's thread: its follower then ends and is drained.
fn stop_gateway(context: &FollowContext, names: &RunNames) {
    if let Some(docker) = context.docker() {
        let _ = execute(
            docker,
            &EngineCall::stop(std::slice::from_ref(&names.gateway)),
            &context.base_env,
            &context.home,
            None,
        );
    }
}

/// The containers of this floor the engine lists, as reconciliation reads
/// them; `None` when the engine gave no answer.
fn listed(context: &FollowContext, floor_key: Option<&str>) -> Option<Vec<ContainerSeen>> {
    let docker = context.docker()?;
    let outcome = execute(
        docker,
        &EngineCall::ps(),
        &context.base_env,
        &context.home,
        None,
    )
    .ok()?;
    if !outcome.succeeded() {
        return None;
    }
    Some(
        parse_ps(&String::from_utf8_lossy(&outcome.stdout))
            .into_iter()
            .filter(|row| {
                floor_key
                    .is_some_and(|key| row.labels.get(FLOOR_LABEL).map(String::as_str) == Some(key))
            })
            .map(|row| ContainerSeen {
                run_id: row.labels.get(RUN_LABEL).cloned(),
                role: row.labels.get(ROLE_LABEL).cloned().unwrap_or_default(),
                running: row.state == "running",
                name: row.name,
            })
            .collect(),
    )
}

/// The needles of `row` from a fresh read of the credential (D-230); none
/// for a `login` run or a credential the window cannot read.
pub(crate) fn fresh_needles(
    context: &FollowContext,
    row: &RunRecord,
    floor_key: &str,
) -> Option<RunNeedles> {
    if row.credential_kind == CredentialKind::Login {
        return None;
    }
    let secret = context
        .runtime
        .store
        .read(&credential_store_name(floor_key))
        .ok()
        .flatten()?;
    RunNeedles::of(row.credential_kind, &secret)
}

/// Removes the objects of a launch that never came back (AD-107).
fn remove_objects(context: &FollowContext, run_id: &str) {
    let (Some(docker), Some(id)) = (context.docker(), RunId::parse(run_id)) else {
        return;
    };
    let names = RunNames::of(&id);
    for call in [
        EngineCall::rm(&[names.worker.clone(), names.gateway.clone()]),
        EngineCall::network_rm(&[names.egress.clone(), names.internal.clone()]),
        EngineCall::volume_rm(std::slice::from_ref(&names.volume)),
    ] {
        let _ = execute(docker, &call, &context.base_env, &context.home, None);
    }
}

/// Reconciliation (AD-109, D-216) as a job of thread `workshop`: the
/// rows from the collector, the engine's containers, core's actions. An
/// engine that does not answer changes no row; while a run is not
/// terminal, the engine is asked again every [`WorkshopRuntime`]'s retry.
pub fn reconcile_on_thread(context: FollowContext) {
    let job = context.clone();
    context.runtime.thread.submit(move || {
        reconcile_now(&job);
    });
}

/// One reconciliation, on the calling thread.
/// The actions core decided, for a test to read; none when the engine or
/// the collector gave no answer.
pub fn reconcile_now(context: &FollowContext) -> Vec<ReconcileAction> {
    let Ok(AuthorAnswer::WorkshopRows(rows)) = ask(
        &context.collector,
        AuthorCommand::WorkshopRowsToReconcile,
        AUTHOR_WAIT,
    ) else {
        return Vec::new();
    };
    let (rows, floor_key) = *rows;
    if rows.is_empty() && floor_key.is_none() {
        return Vec::new();
    }
    let containers = if context.engine_answers() {
        listed(context, floor_key.as_deref())
    } else {
        None
    };
    let Some(containers) = containers else {
        if !rows.is_empty() {
            retry_later(context.clone());
        }
        return Vec::new();
    };
    let seen: Vec<RowSeen> = rows
        .iter()
        .map(|row| RowSeen {
            id: row.id.clone(),
            state: row.state,
            started_at_ms: row.started_at_ms,
            deadline_ms: row.deadline_ms,
            has_package: run_dir(&context.home, &row.id).join("out").is_dir(),
        })
        .collect();
    let mut failed = Vec::new();
    let mut lost = Vec::new();
    let mut orphans = Vec::new();
    let actions = reconcile(&seen, &containers, (context.runtime.now_ms)());
    for action in actions.clone() {
        let row_of = |id: &str| rows.iter().find(|row| row.id == id);
        match action {
            ReconcileAction::Follow { run_id } => {
                if let Some(row) = row_of(&run_id) {
                    follow_row(context, row, floor_key.as_deref());
                }
            }
            // A worker still listed is followed again, so its facts are
            // rebuilt before the watcher collects; without one, what the
            // engine still holds is collected at once (AD-110).
            ReconcileAction::Collect { run_id } => {
                let worker_listed = containers.iter().any(|c| {
                    c.run_id.as_deref() == Some(run_id.as_str())
                        && c.role == ContainerRole::Worker.code()
                });
                match row_of(&run_id) {
                    Some(row) if worker_listed => follow_row(context, row, floor_key.as_deref()),
                    Some(row) if row.state == RunState::Collecting => {
                        crate::workshop::collect::collect_on_thread(context, &run_id);
                    }
                    _ => {}
                }
            }
            ReconcileAction::StopPastDeadline { run_id } => {
                if let Some(row) = row_of(&run_id) {
                    follow_row(context, row, floor_key.as_deref());
                    if let Some(id) = RunId::parse(&run_id) {
                        stop_worker(context, &run_id, &RunNames::of(&id), StopReason::Deadline);
                    }
                }
            }
            ReconcileAction::FailLaunch { run_id } => {
                remove_objects(context, &run_id);
                failed.push(run_id);
            }
            ReconcileAction::Lost { run_id } => {
                let summary = context
                    .runtime
                    .followed
                    .lock()
                    .ok()
                    .and_then(|followed| followed.get(&run_id).map(Followed::summary))
                    .unwrap_or_default();
                lost.push((run_id, summary));
            }
            ReconcileAction::Orphan { name, run_id } => orphans.push(OrphanView { name, run_id }),
        }
    }
    context.tell(FollowEvent::Reconciled {
        failed,
        lost,
        orphans,
    });
    actions
}

fn follow_row(context: &FollowContext, row: &RunRecord, floor_key: Option<&str>) {
    let needles = || floor_key.and_then(|key| fresh_needles(context, row, key));
    start(context, RunToFollow::of(row), needles);
}

/// Asks the engine again after the retry, from a sleeper thread that
/// queues the next reconciliation on thread `workshop` (D-216).
fn retry_later(context: FollowContext) {
    let wait = context.runtime.engine_retry;
    let _ = std::thread::Builder::new()
        .name("workshop-engine-retry".to_owned())
        .spawn(move || {
            std::thread::sleep(wait);
            reconcile_on_thread(context);
        });
}

/// The collector's half: what reconciliation reads (AD-109), the runs not
/// terminal and the floor's key.
pub(crate) fn rows_to_reconcile(
    collector: &Collector,
    registry: &mut WindowRegistry,
) -> Result<AuthorAnswer, officina_core::ipc::IpcError> {
    let rows = registry.workshop_runs_to_reconcile()?;
    let relay = collector.relay();
    let floor_key =
        crate::relay::home::load_relay_floor(&relay.relay_floor(&relay.read_approved()))
            .ok()
            .map(|floor| officina_core::workshop::names::floor_key(&floor.floor_path));
    Ok(AuthorAnswer::WorkshopRows(Box::new((rows, floor_key))))
}

pub(crate) fn finished_line(
    row: &RunRecord,
    cause: FinishCause,
    outcome: WorkshopOutcome,
    summary: &RunSummary,
    now_ms: i64,
) -> JournalEntry {
    JournalEntry::WorkshopFinished(Box::new(WorkshopFinished {
        run_id: row.id.clone(),
        template: row.template.clone(),
        project: row.project.clone(),
        base_branch: row.base_branch.clone(),
        base_commit: row.base_commit.clone(),
        cause,
        outcome,
        commits_past_base: None,
        tip: None,
        exit_code: row.exit_code,
        duration_ms: now_ms.saturating_sub(row.started_at_ms).max(0),
        cost_usd_micros: summary.cost_usd_micros,
        turns: summary.turns,
        result_subtype: summary.result_subtype.clone(),
        hooks: summary.hooks.clone(),
        refused: summary.refused,
        stream_bytes: summary.stream_bytes,
    }))
}

/// The collector's half of a [`FollowEvent`] (AD-109, AD-120): rows moved
/// by core's edges, journal lines written.
pub(crate) fn record(
    collector: &mut Collector,
    registry: &mut WindowRegistry,
    event: &FollowEvent,
    now_ms: i64,
) -> Result<(), officina_core::ipc::IpcError> {
    let mut lines = Vec::new();
    match event {
        FollowEvent::Refused { line, ordinal } => {
            if let Some(run_id) = &line.run_id {
                registry.amend_workshop_facts(run_id, |facts: &mut RunFacts| {
                    facts.refusals_journaled = facts.refusals_journaled.max(*ordinal);
                })?;
            }
            lines.push(JournalEntry::GatewayRefused(line.clone()));
        }
        FollowEvent::StopAsked { run_id, reason } => {
            registry.move_workshop_run(run_id, RunEvent::StopAsked, now_ms, |row| {
                row.facts.stop_reason = Some(*reason);
            })?;
        }
        FollowEvent::Exited {
            run_id,
            cause,
            exit_code,
            texts,
        } => {
            registry.move_workshop_run(run_id, RunEvent::Exited, now_ms, |row| {
                row.cause = Some(Recorded::Known(*cause));
                row.exit_code = *exit_code;
                if let Some(texts) = texts {
                    row.texts = Some(texts.clone());
                }
            })?;
        }
        FollowEvent::Reconciled {
            failed,
            lost,
            orphans,
        } => {
            for run_id in failed {
                let moved =
                    registry.move_workshop_run(run_id, RunEvent::LaunchFailed, now_ms, |row| {
                        row.cause = Some(Recorded::Known(FinishCause::Failed));
                        row.outcome = Some(WorkshopOutcome::Failed);
                    })?;
                if let Some(row) = moved {
                    lines.push(JournalEntry::WorkshopRemoved(WorkshopRemoved {
                        run_id: run_id.clone(),
                        cause: RemovalCause::LaunchFailed,
                    }));
                    lines.push(finished_line(
                        &row,
                        FinishCause::Failed,
                        WorkshopOutcome::Failed,
                        &RunSummary::default(),
                        now_ms,
                    ));
                }
            }
            for (run_id, summary) in lost {
                let moved = registry.move_workshop_run(run_id, RunEvent::Lost, now_ms, |row| {
                    row.cause = Some(Recorded::Known(FinishCause::EngineLost));
                    row.outcome = Some(WorkshopOutcome::Unknown);
                })?;
                if let Some(row) = moved {
                    lines.push(finished_line(
                        &row,
                        FinishCause::EngineLost,
                        WorkshopOutcome::Unknown,
                        summary,
                        now_ms,
                    ));
                }
            }
            collector.set_workshop_orphans(orphans.clone());
        }
        FollowEvent::Collected(collected) => {
            let groups = collected_groups(collector, registry, collected)?;
            record_collected(registry, collected, &groups, now_ms, &mut lines)?;
        }
    }
    if lines.is_empty() {
        return Ok(());
    }
    crate::relay::journal::record(collector.relay(), &lines, now_ms).map_err(|e| {
        officina_core::ipc::IpcError::new(
            officina_core::ipc::IpcErrorCode::JournalUnavailable,
            e.to_string(),
        )
    })
}

/// The groups of a collected package that passed every check (AD-113),
/// under its project's rules as the approved catalogue holds them; none
/// for any other outcome.
fn collected_groups(
    collector: &Collector,
    registry: &mut WindowRegistry,
    collected: &crate::workshop::collect::Collected,
) -> Result<Option<Vec<PackageGroup>>, officina_core::ipc::IpcError> {
    use officina_core::workshop::sensitive::{SensitiveRules, groups};
    let Some(changes) = &collected.changes else {
        return Ok(None);
    };
    let Some(row) = registry.workshop_run(&collected.run_id)? else {
        return Ok(None);
    };
    let rules = match collector.relay().read_approved().approved() {
        Some(approved) => SensitiveRules::of_approved(&approved.canonical, &row.project),
        None => SensitiveRules::default(),
    };
    Ok(Some(groups(changes, &rules)))
}

/// The collector's half of a collection (AD-110, AD-120): the row moved
/// to what the run produced, its facts, texts, package fields and groups,
/// and the lines it owes: `gateway_summary`, `workshop_removed`
/// (`collected`), `review_package` for a package, and its one
/// `workshop_finished`.
fn record_collected(
    registry: &mut WindowRegistry,
    collected: &crate::workshop::collect::Collected,
    groups: &Option<Vec<PackageGroup>>,
    now_ms: i64,
    lines: &mut Vec<JournalEntry>,
) -> Result<(), officina_core::ipc::IpcError> {
    use officina_core::relay::journal::{GatewaySummary, ReviewPackage};
    use officina_core::workshop::package::{PackageAssessment, PackageState};
    let (tip, commits, cause) = match &collected.assessment {
        Some(PackageAssessment::Package {
            tip,
            commits_past_base,
            cause,
        }) => (tip.clone(), Some(*commits_past_base), *cause),
        Some(PackageAssessment::NoCommit { tip }) => (Some(tip.clone()), Some(0), None),
        None => (None, None, None),
    };
    let event = match collected.outcome {
        WorkshopOutcome::Package if cause.is_some() => RunEvent::Quarantined,
        WorkshopOutcome::Package => RunEvent::Packaged,
        WorkshopOutcome::NoChange => RunEvent::NothingChanged,
        _ => RunEvent::CollectionFailed,
    };
    let summary = &collected.summary;
    let groups = groups.as_deref().unwrap_or_default();
    let groups_json = (cause.is_none() && collected.changes.is_some())
        .then(|| serde_json::to_string(groups).unwrap_or_default());
    let mut families: Vec<_> = groups.iter().map(|g| g.group).collect();
    families.sort();
    families.dedup();
    let package = crate::registry::workshop::PackageFields {
        package_sha256: collected.bundle_sha256.as_deref(),
        tip_commit: tip.as_deref(),
        commits_past_base: commits,
        files_changed: collected.files_changed,
        groups_json: groups_json.as_deref(),
        quarantine_cause: cause.map(|c| c.code()),
    };
    let moved = registry.record_workshop_collection(
        &collected.run_id,
        event,
        collected.outcome,
        &package,
        now_ms,
        |row| {
            let facts = &mut row.facts;
            facts.cost_usd_micros = summary.cost_usd_micros;
            facts.turns = summary.turns;
            facts.hooks = summary.hooks.clone();
            facts.refused = summary.refused;
            facts.stream_bytes = summary.stream_bytes;
            facts.gateway_exit = collected.gateway_exit;
            facts.upstream = collected.upstream;
            facts.insertions = collected.insertions;
            facts.deletions = collected.deletions;
            facts.collect_failed = collected.failure;
            facts.credential_locations = collected.credential_locations;
            facts.login_without_needles = collected.login_without_needles;
            if let Some(texts) = &collected.texts {
                row.texts = Some(texts.clone());
            }
        },
    )?;
    let Some(row) = moved else {
        return Ok(());
    };
    // The refusals held without needles, journaled once the scan replaced
    // their hosts (D-230).
    for refusal in &collected.released {
        lines.push(JournalEntry::GatewayRefused(GatewayRefused {
            run_id: Some(row.id.clone()),
            battery_id: None,
            host: refusal.host.clone(),
            port: refusal.port,
            cause: refusal.cause,
        }));
    }
    lines.push(JournalEntry::GatewaySummary(Box::new(GatewaySummary {
        run_id: row.id.clone(),
        allowed: collected.allowed.clone(),
        refused: summary.refused,
        exit_code: collected.gateway_exit,
        upstream: collected.upstream,
    })));
    if collected.removed {
        lines.push(JournalEntry::WorkshopRemoved(WorkshopRemoved {
            run_id: row.id.clone(),
            cause: RemovalCause::Collected,
        }));
    }
    if collected.outcome == WorkshopOutcome::Package {
        lines.push(JournalEntry::ReviewPackage(ReviewPackage {
            run_id: row.id.clone(),
            bundle_sha256: collected.bundle_sha256.clone(),
            commits: commits.unwrap_or(0),
            files: collected.files_changed.unwrap_or(0),
            insertions: collected.insertions.unwrap_or(0),
            deletions: collected.deletions.unwrap_or(0),
            groups: u32::try_from(groups.len()).unwrap_or(u32::MAX),
            sensitive: families,
            outcome: if cause.is_some() {
                PackageState::Quarantined
            } else {
                PackageState::Review
            },
            cause,
            credential_locations: collected.credential_locations,
        }));
    }
    let mut finished = finished_line(&row, collected.cause, collected.outcome, summary, now_ms);
    if let JournalEntry::WorkshopFinished(line) = &mut finished {
        line.commits_past_base = commits;
        line.tip = tip;
    }
    lines.push(finished);
    Ok(())
}

/// Starts following a run the window just launched (AD-107 step 4): its
/// needles were built from the credential its `create` carried.
pub fn after_launch(
    context: &FollowContext,
    run_id: &str,
    deadline_ms: i64,
    needles: Option<RunNeedles>,
) {
    let run = RunToFollow {
        run_id: run_id.to_owned(),
        stop_at_ms: (context.runtime.now_ms)()
            .saturating_add(deadline_ms)
            .saturating_add(DEADLINE_GRACE_MS),
        journaled: 0,
    };
    start(context, run, || needles);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::credential::Secret;
    use officina_core::workshop::gateway::{GatewayLine, GatewayRefusal};

    const SEEDED: &str = "a-seeded-credential-value-0123456789";

    fn refused(host: &str) -> LineRead {
        LineRead::Line(GatewayLine::refused(host, 443, GatewayRefusal::NotAllowed, None).to_line())
    }

    /// D-230, D-609: a `login` run has no needles on the host, so its
    /// refusals are held and its texts stay in memory until collection.
    #[test]
    fn a_login_run_holds_its_texts_until_collection() {
        let secret = Secret::new(SEEDED.to_owned());
        let mut followed = Followed::new(RunNeedles::of(CredentialKind::Login, &secret));
        let (journal, _) = followed.absorb(
            "r",
            ContainerRole::Gateway,
            Pipe::Stdout,
            &refused(&format!("{SEEDED}.example.com")),
        );
        assert!(journal.is_empty());
        assert_eq!(followed.held.len(), 1);
        assert!(followed.texts().is_none());
        assert!(followed.run_texts().is_none());
    }

    /// D-230: with needles, the refused host is replaced before it is
    /// journaled, and a replay journals nothing twice (AD-109).
    #[test]
    fn a_refusal_is_replaced_and_journaled_once() {
        let secret = Secret::new(SEEDED.to_owned());
        let mut followed = Followed::new(RunNeedles::of(CredentialKind::OauthToken, &secret));
        let line = refused(&format!("{SEEDED}.example.com"));
        let (journal, _) = followed.absorb("r", ContainerRole::Gateway, Pipe::Stdout, &line);
        assert_eq!(journal.len(), 1);
        let text = format!("{journal:?}");
        assert!(!text.contains(SEEDED), "{text}");
        followed.output.restart(ContainerRole::Gateway);
        let (again, _) = followed.absorb("r", ContainerRole::Gateway, Pipe::Stdout, &line);
        assert!(again.is_empty());
    }
}
