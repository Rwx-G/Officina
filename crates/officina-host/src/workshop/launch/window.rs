//! The window's half of a workshop card (AD-107): the facts gathered on
//! thread `workshop`, the launch's checks there, then its steps, each
//! failure removing what was created. Compiled with the window's authority
//! only: only the window launches.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use officina_core::orders::card::{
    CardAnswer, CardRequest, IssuedCard, LaunchAnswer, LaunchRefusal,
};
use officina_core::orders::workshop_card::{
    BuiltImage, GatheredFacts, ImportedBase, MountFact, WorkshopChecks, check_workshop_launch,
    launch_checks,
};
use officina_core::registry::types::LaunchOutcome;
use officina_core::relay::catalogue::CanonicalCatalogue;
use officina_core::relay::journal::{JournalEntry, StepLine, WorkshopRemoved, WorkshopStarted};
use officina_core::workshop::bundle::{
    MAX_BASE_BUNDLE_BYTES, MAX_HEADER_BYTES, holds_only, parse_header,
};
use officina_core::workshop::engine::EngineCall;
use officina_core::workshop::floor::{CredentialKind, UpstreamProxy};
use officina_core::workshop::launch::{
    BaseChoice, IN_DIR, WorkshopLaunch, gateway, networks, plan_sha256, run_plan, worker,
};
use officina_core::workshop::names::ObjectName;
use officina_core::workshop::run::{LaunchStep, RemovalCause, StepOutcome};
use officina_core::workshop::vector::{ContainerRole, credential_name};

use super::{
    CredentialStateSource, FollowedRuns, RunNeedles, credential_present, credential_store_name,
    git_identity, mount_source, mount_state, run_dir, uncommitted,
};
use crate::registry::workshop::InventoryRecord;
use crate::relay::credential::CredentialStore;
use crate::watch::{AUTHOR_WAIT, AuthorCommand, AuthorRequest, CollectorMsg, ask};
use crate::workshop::battery::{
    BatteryEngine, BatteryJob, BatteryPlan, engine_identity, validity_key,
};
use crate::workshop::engine::{ResolvedDocker, create_with_credential, execute};
use crate::workshop::inventory::{InventoryEngine, InventoryProject, WorkshopGit, for_launch};
use crate::workshop::proxy::{ProxyError, SystemProxyReader, resolve};
use crate::workshop::threads::JobThread;

/// What the collector holds for a workshop card (AD-107): its registry
/// rows and the approved catalogue, read on the collector thread.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredFacts {
    pub live_runs: u32,
    pub max_workers: u32,
    pub reviews_pending: u32,
    /// The template's runs imported in the last 7 days: each run's branch
    /// and id.
    pub imported: Vec<(String, String)>,
    /// The inventory recorded for the project on its image.
    pub inventory: Option<InventoryRecord>,
    /// The images built and their batteries, or none when Run escape
    /// battery's grant refuses (no gateway image, say).
    pub battery: Option<BatteryPlan>,
}

/// What thread `workshop` reads a card's facts, or launches it, from.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkshopTicket {
    /// The new card's id at issue; the clicked card's at launch.
    pub card_id: String,
    pub seq: u32,
    pub request: CardRequest,
    /// The base asked for; the first listed when none.
    pub base: Option<BaseChoice>,
    pub project: String,
    pub project_dir: PathBuf,
    pub mounts: Vec<String>,
    pub allow: Vec<String>,
    pub upstream_proxy: String,
    pub floor_key: String,
    pub kind: CredentialKind,
    pub listed: Vec<String>,
    pub image_tag: String,
    pub canonical: CanonicalCatalogue,
    pub home: PathBuf,
    pub base_env: Vec<(String, String)>,
    pub stored: StoredFacts,
    /// At a launch: the card clicked and the plan hash the click sent.
    pub card: Option<Box<IssuedCard>>,
    pub clicked_plan: Option<String>,
}

/// What the window's workshop commands run with: thread `workshop` and
/// the sources it reads. Tests give their own engine, store and readers
/// (D-86's pattern); nothing in the environment replaces them.
#[derive(Clone)]
pub struct WorkshopRuntime {
    pub thread: JobThread,
    pub docker: Option<ResolvedDocker>,
    /// The workshop's `git`, resolved (AD-111).
    pub git: Option<PathBuf>,
    /// The executable's folder, where `linux/` sits (AD-100).
    pub exe_dir: PathBuf,
    pub linux_override: Option<OsString>,
    pub store: Arc<dyn CredentialStore + Send + Sync>,
    pub states: Arc<dyn CredentialStateSource + Send + Sync>,
    pub reader: Arc<dyn SystemProxyReader>,
    pub followed: FollowedRuns,
    /// What host git showed of each package (D-239).
    pub reviews: crate::workshop::review::ReviewCache,
    pub now_ms: fn() -> i64,
    /// How long thread `workshop` waits before asking an engine that did
    /// not answer again (D-216): [`crate::workshop::follow::ENGINE_RETRY`].
    pub engine_retry: std::time::Duration,
}

impl std::fmt::Debug for WorkshopRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkshopRuntime")
            .field("docker", &self.docker)
            .field("git", &self.git)
            .finish_non_exhaustive()
    }
}

/// What a gathering read besides the card's facts: the engine's clock
/// skew and the battery the image check relied on.
#[derive(Debug, Clone, Default)]
pub struct GatherExtras {
    pub clock_skew_ms: Option<i64>,
    pub battery_id: Option<String>,
}

impl WorkshopRuntime {
    fn workshop_git(&self, ticket: &WorkshopTicket) -> Option<WorkshopGit> {
        let git = self.git.clone()?;
        let hooks_dir = crate::workshop::git::ensure_empty_hooks(&ticket.home).ok()?;
        Some(WorkshopGit {
            git,
            hooks_dir,
            base_env: ticket.base_env.clone(),
        })
    }

    fn env(&self, ticket: &WorkshopTicket) -> Vec<(String, String)> {
        let mut env = ticket.base_env.clone();
        if let Some(dir) = &self.linux_override {
            env.push((
                crate::workshop::ENV_LINUX_BIN_DIR.to_owned(),
                dir.to_string_lossy().into_owned(),
            ));
        }
        env
    }
}

/// The base `ticket` asks for, or the first listed one.
fn chosen_base(ticket: &WorkshopTicket) -> Option<BaseChoice> {
    ticket.base.clone().or_else(|| {
        ticket.listed.first().map(|branch| BaseChoice::Listed {
            branch: branch.clone(),
        })
    })
}

/// Reads a card's facts on thread `workshop` (AD-107): the bases' tips,
/// the imported branches that still exist, the uncommitted count, each
/// mount, the commit identity, the engine and its clock, the image check,
/// the battery, the credential's presence and state, the routes.
pub fn gather(ticket: &WorkshopTicket, runtime: &WorkshopRuntime) -> (GatheredFacts, GatherExtras) {
    let git = runtime.workshop_git(ticket);
    let dir = &ticket.project_dir;
    let tip = |branch: &str| git.as_ref().and_then(|g| g.branch_commit(dir, branch));
    let listed_tips: BTreeMap<String, Option<String>> = ticket
        .listed
        .iter()
        .map(|branch| (branch.clone(), tip(branch)))
        .collect();
    let imported: Vec<ImportedBase> = ticket
        .stored
        .imported
        .iter()
        .filter_map(|(branch, parent)| {
            tip(branch).map(|found| ImportedBase {
                branch: branch.clone(),
                parent_run_id: parent.clone(),
                tip: Some(found),
            })
        })
        .collect();
    let base_commit = chosen_base(ticket).and_then(|base| match &base {
        BaseChoice::Listed { branch } => listed_tips.get(branch).cloned().flatten(),
        BaseChoice::Imported { branch, .. } => imported
            .iter()
            .find(|b| &b.branch == branch)
            .and_then(|b| b.tip.clone()),
    });
    let mounts: Vec<MountFact> = ticket
        .mounts
        .iter()
        .map(|mount| MountFact {
            path: mount.clone(),
            state: mount_state(dir, mount, git.as_ref(), base_commit.as_deref()),
        })
        .collect();
    let mut extras = GatherExtras::default();
    let engine = runtime.docker.clone().map(|docker| BatteryEngine {
        docker,
        base_env: ticket.base_env.clone(),
        home: ticket.home.clone(),
    });
    let identity = engine
        .as_ref()
        .and_then(|engine| engine_identity(engine, &runtime.now_ms).ok());
    extras.clock_skew_ms = identity.as_ref().and_then(|i| i.clock_skew_ms);
    let (image, gateway_image_id) = match &identity {
        Some(_) => image_ids(ticket, runtime),
        None => (None, None),
    };
    let battery_valid = match (&identity, &image, &ticket.stored.battery) {
        (Some(identity), Some(image), Some(plan)) => {
            let found = valid_battery(
                plan,
                &ticket.image_tag,
                &image.image_id,
                &identity.key,
                (runtime.now_ms)(),
            );
            extras.battery_id.clone_from(&found);
            found.is_some()
        }
        _ => false,
    };
    let routes = match UpstreamProxy::parse(&ticket.upstream_proxy) {
        Ok(setting) => resolve(&setting, &ticket.allow, Arc::clone(&runtime.reader))
            .map(|routes| routes.routes)
            .map_err(|why| match why {
                ProxyError::Refused(refusal) => refusal.cause,
                ProxyError::Unreadable(_) => {
                    officina_core::workshop::proxy::ProxyRefusal::ProxyUnresolved
                }
            }),
        Err(_) => Err(officina_core::workshop::proxy::ProxyRefusal::ProxyUnsupported),
    };
    let checks = WorkshopChecks {
        workshop_on: true,
        stopped: crate::workshop::workshop_stopped(&ticket.home),
        engine_ready: identity.is_some(),
        image_matches: image.is_some() && gateway_image_id.is_some(),
        battery_valid,
        mounts: mounts.iter().map(|m| m.state).collect(),
        live_runs: ticket.stored.live_runs,
        max_workers: ticket.stored.max_workers,
        reviews_pending: ticket.stored.reviews_pending,
        credential_kind: ticket.kind,
        credential_present: credential_present(
            runtime.store.as_ref(),
            ticket.kind,
            &ticket.floor_key,
        ),
        credential_state: runtime.states.state(&ticket.floor_key, ticket.kind),
        proxy: routes.as_ref().err().copied(),
        project_exists: dir.is_dir(),
        base_matches: base_commit.as_ref().map(|_| true),
        plan_matches: true,
        hooks_found: None,
    };
    let gathered = GatheredFacts {
        listed_tips,
        imported,
        uncommitted: git.as_ref().and_then(|g| uncommitted(g, dir)).unwrap_or(0),
        mounts,
        mount_sources: ticket.mounts.iter().map(|m| mount_source(dir, m)).collect(),
        image_tag: ticket.image_tag.clone(),
        image,
        gateway_image_id,
        floor_key: ticket.floor_key.clone(),
        officina_version: crate::workshop::build::VERSION.to_owned(),
        git: git
            .as_ref()
            .map(|g| git_identity(g, dir))
            .unwrap_or_else(|| officina_core::workshop::vector::GitIdentity {
                name: super::FALLBACK_GIT_NAME.to_owned(),
                email: super::FALLBACK_GIT_EMAIL.to_owned(),
            }),
        routes,
        checks,
    };
    (gathered, extras)
}

/// The image check of story 6.8 for the run's image and the gateway's
/// (AD-99, D-207, D-233): the ids the `create`s name, or none for the one
/// that fails.
fn image_ids(
    ticket: &WorkshopTicket,
    runtime: &WorkshopRuntime,
) -> (Option<BuiltImage>, Option<String>) {
    use officina_core::workshop::image::{GATEWAY_IMAGE, check};
    let context = crate::workshop::doctor::ImageContext {
        exe_dir: runtime.exe_dir.clone(),
        canonical: ticket.canonical.clone(),
        floor_key: ticket.floor_key.clone(),
        project_dirs: BTreeMap::new(),
        claude_version: None,
    };
    let Some(report) = crate::workshop::doctor::image_report(
        &context,
        &runtime.env(ticket),
        &ticket.home,
        runtime.docker.as_ref(),
    ) else {
        return (None, None);
    };
    let id_of = |tag: &str| {
        report
            .images
            .iter()
            .find(|input| input.tag == tag)
            .and_then(|input| {
                check(
                    &input.expected,
                    input.inspected.as_ref(),
                    input.recorded.as_ref(),
                )
                .ok()
            })
    };
    let built_at = crate::registry::store::open_reader(&ticket.home, &[])
        .ok()
        .and_then(|reader| reader.workshop_image(&ticket.image_tag).ok().flatten())
        .map(|row| row.built_at_ms);
    let image = id_of(&ticket.image_tag).map(|image_id| BuiltImage {
        image_id,
        built_at_ms: built_at,
    });
    (image, id_of(GATEWAY_IMAGE))
}

/// The passing battery that keeps `image_id` valid on the engine `key`
/// (AD-117, D-232), by its id.
fn valid_battery(
    plan: &BatteryPlan,
    tag: &str,
    image_id: &str,
    key: &str,
    now_ms: i64,
) -> Option<String> {
    let candidate = plan
        .candidates
        .iter()
        .find(|c| c.image == tag && c.image_id == image_id)?;
    let job = BatteryJob {
        id: officina_core::workshop::names::RunId::parse(&"0".repeat(32))?,
        image: candidate.image.clone(),
        spec: officina_core::workshop::battery::BatterySpec {
            kind: plan.kind,
            cpus: plan.cpus,
            memory_mib: plan.memory_mib,
            floor_key: plan.floor_key.clone(),
            version: crate::workshop::build::VERSION.to_owned(),
            image_id: candidate.image_id.clone(),
            image_env: candidate.image_env.clone(),
        },
        programs_sha256: candidate.programs_sha256.clone(),
        gateway_image_id: plan.gateway_image_id.clone(),
    };
    crate::workshop::battery::valid_battery(&candidate.batteries, &validity_key(&job, key), now_ms)
        .map(|record| record.id.clone())
}

/// A workshop card's issue on thread `workshop` (AD-107, D-239): the
/// facts are read there and handed to the collector as a request it never
/// drops, which issues the card; its answer comes back here.
pub fn issue_on_thread(
    sender: &std::sync::mpsc::Sender<CollectorMsg>,
    runtime: &WorkshopRuntime,
    ticket: WorkshopTicket,
) -> Result<CardAnswer, officina_core::ipc::IpcError> {
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let collector = sender.clone();
    let job = runtime.clone();
    let submitted = runtime.thread.submit(move || {
        let (gathered, _) = gather(&ticket, &job);
        let (request, answer) = AuthorRequest::new(
            AuthorCommand::WorkshopCardFacts(Box::new((ticket, gathered))),
            AUTHOR_WAIT,
        );
        let reply = collector
            .send(CollectorMsg::Author(request))
            .map_err(|_| officina_core::ipc::IpcError::internal("the collector has stopped"))
            .and_then(|()| {
                answer.recv().map_err(|_| {
                    officina_core::ipc::IpcError::internal("the collector has stopped")
                })
            });
        let _ = done_tx.send(
            reply
                .and_then(|r| r)
                .and_then(crate::watch::AuthorAnswer::into_card),
        );
    });
    if !submitted {
        return Err(officina_core::ipc::IpcError::internal(
            "thread workshop has stopped",
        ));
    }
    done_rx
        .recv()
        .map_err(|_| officina_core::ipc::IpcError::internal("thread workshop has stopped"))?
}

/// What a granted launch records beside its `work_order` row (AD-107
/// step 1), owned so it can travel to the collector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRow {
    pub image: String,
    pub image_id: String,
    pub gateway_image_id: String,
    pub base_branch: String,
    pub base_commit: String,
    pub parent_run_id: Option<String>,
    pub branch: String,
    pub uncommitted: u32,
    pub plan_json: String,
    pub plan_sha256: String,
    pub credential_kind: CredentialKind,
    pub deadline_ms: i64,
    pub battery_id: String,
    pub argv: Vec<String>,
}

/// How a launch's steps ended (AD-107 step 4), for the collector to
/// record: the run's state and the `work_order` outcome, the facts, and
/// the journal lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkshopLaunchReport {
    pub run_id: String,
    pub outcome: LaunchOutcome,
    pub clock_skew_ms: Option<i64>,
    pub line: WorkshopStarted,
    /// Set when a failure removed what was created.
    pub removed: bool,
}

impl WorkshopLaunchReport {
    /// The journal lines of the launch (AD-120): `workshop_started`, and
    /// `workshop_removed` when a failed launch removed what it created.
    pub fn journal_lines(&self) -> Vec<JournalEntry> {
        let mut lines = vec![JournalEntry::WorkshopStarted(Box::new(self.line.clone()))];
        if self.removed {
            lines.push(JournalEntry::WorkshopRemoved(WorkshopRemoved {
                run_id: self.run_id.clone(),
                cause: RemovalCause::LaunchFailed,
            }));
        }
        lines
    }
}

/// The launch of a granted workshop card (AD-107): on thread `workshop`,
/// the facts read again and the checks run in AD-107's order (the
/// inventory last), the card consumed and the rows written by the
/// collector, then the steps; the report goes to the collector, which
/// records it whenever it arrives.
pub fn launch_on_thread(
    sender: &std::sync::mpsc::Sender<CollectorMsg>,
    runtime: &WorkshopRuntime,
    ticket: WorkshopTicket,
) -> Result<LaunchAnswer, officina_core::ipc::IpcError> {
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let collector = sender.clone();
    let job = runtime.clone();
    let submitted = runtime.thread.submit(move || {
        let _ = done_tx.send(launch(&collector, &job, &ticket));
    });
    if !submitted {
        return Err(officina_core::ipc::IpcError::internal(
            "thread workshop has stopped",
        ));
    }
    done_rx
        .recv()
        .map_err(|_| officina_core::ipc::IpcError::internal("thread workshop has stopped"))?
}

fn launch(
    collector: &std::sync::mpsc::Sender<CollectorMsg>,
    runtime: &WorkshopRuntime,
    ticket: &WorkshopTicket,
) -> Result<LaunchAnswer, officina_core::ipc::IpcError> {
    let Some(card) = ticket.card.as_deref() else {
        return Ok(LaunchAnswer::Refused(LaunchRefusal::UnknownCard));
    };
    let Some(issue) = card.workshop_issue() else {
        return Ok(LaunchAnswer::Refused(LaunchRefusal::UnknownCard));
    };
    let (fresh, extras) = gather(ticket, runtime);
    let checks = launch_checks(
        issue,
        card.order(),
        &fresh,
        card.id(),
        card.seq(),
        ticket.clicked_plan.as_deref(),
    );
    let launch = issue.launch.clone();
    let checked = check_workshop_launch(&checks, || match &launch {
        Some(launch) => inventory_found(collector, runtime, ticket, launch),
        None => false,
    });
    if let Err(refusal) = checked {
        return Ok(LaunchAnswer::Refused(refusal));
    }
    let Some(launch) = launch else {
        return Ok(LaunchAnswer::Refused(LaunchRefusal::PlanChanged));
    };
    let plan = run_plan(&launch);
    let row = RunRow {
        image: launch.facts().image.clone(),
        image_id: launch.facts().image_id.clone(),
        gateway_image_id: launch.facts().gateway_image_id.clone(),
        base_branch: launch.base().branch().to_owned(),
        base_commit: launch.facts().base_commit.clone(),
        parent_run_id: launch.base().parent_run_id().map(str::to_owned),
        branch: launch.branch(),
        uncommitted: fresh.uncommitted,
        plan_json: serde_json::to_string(&plan).unwrap_or_default(),
        plan_sha256: plan_sha256(&launch),
        credential_kind: launch.credential_kind(),
        deadline_ms: i64::try_from(launch.deadline_s()).unwrap_or(i64::MAX) * 1000,
        battery_id: extras.battery_id.clone().unwrap_or_default(),
        argv: plan.claude_argv.clone(),
    };
    let consumed = ask(
        collector,
        AuthorCommand::ConsumeWorkshopCard {
            card_id: card.id().to_owned(),
            run: Box::new(row.clone()),
        },
        AUTHOR_WAIT,
    )?
    .into_launch_refusal()?;
    if let Some(refusal) = consumed {
        return Ok(LaunchAnswer::Refused(refusal));
    }
    let mut steps = vec![StepLine {
        step: LaunchStep::Record,
        outcome: StepOutcome::Ok,
        detail: None,
    }];
    let runner = Steps {
        runtime,
        ticket,
        launch: &launch,
    };
    let ended = runner.run(&mut steps);
    let removed = ended.is_err();
    if removed {
        runner.remove();
        let _ = std::fs::remove_dir_all(run_dir(&ticket.home, card.id()));
    }
    let outcome = if removed {
        LaunchOutcome::Failed
    } else {
        LaunchOutcome::Started
    };
    let shown = card.card();
    let report = WorkshopLaunchReport {
        run_id: card.id().to_owned(),
        outcome,
        clock_skew_ms: extras.clock_skew_ms,
        removed,
        line: WorkshopStarted {
            run_id: card.id().to_owned(),
            name: shown.name.clone(),
            template: shown.template.clone(),
            project: shown.project.clone(),
            values: officina_core::orders::card::card_values(shown),
            image: row.image.clone(),
            image_id: row.image_id.clone(),
            base_branch: row.base_branch.clone(),
            base_commit: row.base_commit.clone(),
            parent_run_id: row.parent_run_id.clone(),
            hosts: launch.allow_list(),
            mounts: ticket.mounts.clone(),
            deadline_ms: row.deadline_ms,
            max_usd_cents: shown.workshop.as_ref().and_then(|w| w.max_usd_cents),
            credential_kind: row.credential_kind,
            argv: row.argv.clone(),
            plan_sha256: row.plan_sha256.clone(),
            catalogue_hash: shown.catalogue_hash.clone(),
            prompt_sha256: shown.prompt_sha256.clone(),
            battery_id: row.battery_id.clone(),
            steps,
            outcome,
        },
    };
    let (request, _answer) = AuthorRequest::new(
        AuthorCommand::WorkshopLaunched(Box::new(report)),
        AUTHOR_WAIT,
    );
    let _ = collector.send(CollectorMsg::Author(request));
    if let Ok(needles) = ended {
        let context = crate::workshop::follow::FollowContext {
            runtime: runtime.clone(),
            collector: collector.clone(),
            home: ticket.home.clone(),
            base_env: ticket.base_env.clone(),
        };
        crate::workshop::follow::after_launch(&context, card.id(), row.deadline_ms, needles);
    }
    Ok(LaunchAnswer::Launched(outcome))
}

/// The inventory a launch judges (AD-118, D-1137): the stored one while
/// its key holds, else a fresh run, whose record goes to the collector.
/// One that cannot be taken refuses, as a missing guard would.
fn inventory_found(
    collector: &std::sync::mpsc::Sender<CollectorMsg>,
    runtime: &WorkshopRuntime,
    ticket: &WorkshopTicket,
    launch: &WorkshopLaunch,
) -> bool {
    let (Some(docker), Some(git)) = (runtime.docker.clone(), runtime.workshop_git(ticket)) else {
        return false;
    };
    let engine = InventoryEngine {
        docker,
        base_env: ticket.base_env.clone(),
        home: ticket.home.clone(),
        floor_key: ticket.floor_key.clone(),
    };
    let project = InventoryProject {
        name: ticket.project.clone(),
        path: ticket.project_dir.clone(),
        mounts: ticket.mounts.clone(),
    };
    let taken = for_launch(
        &engine,
        &git,
        (&ticket.image_tag, &launch.facts().image_id),
        &project,
        &launch.facts().base_commit,
        ticket.stored.inventory.as_ref(),
        (runtime.now_ms)(),
    );
    match taken {
        Ok(taken) => {
            if let Some(record) = taken.fresh {
                let (request, _answer) = AuthorRequest::new(
                    AuthorCommand::WorkshopInventory(Box::new(record)),
                    AUTHOR_WAIT,
                );
                let _ = collector.send(CollectorMsg::Author(request));
            }
            taken.inventory.consequences().refused.is_empty()
        }
        Err(_) => false,
    }
}

/// Why a step failed: its own text, never a worker's.
struct Failed(String);

/// The launch's steps 2 and 3 (AD-107).
struct Steps<'a> {
    runtime: &'a WorkshopRuntime,
    ticket: &'a WorkshopTicket,
    launch: &'a WorkshopLaunch,
}

impl Steps<'_> {
    fn docker(&self) -> Result<&ResolvedDocker, Failed> {
        self.runtime
            .docker
            .as_ref()
            .ok_or_else(|| Failed("no docker".to_owned()))
    }

    fn call(&self, call: &EngineCall) -> Result<(), Failed> {
        let outcome = execute(
            self.docker()?,
            call,
            &self.ticket.base_env,
            &self.ticket.home,
            None,
        )
        .map_err(|e| Failed(e.to_string()))?;
        if outcome.succeeded() {
            Ok(())
        } else {
            Err(Failed(format!(
                "exit {:?}: {}",
                outcome.code,
                officina_core::orders::work_order::cut_error(outcome.stderr.trim())
            )))
        }
    }

    /// True once Stop all latched thread `workshop` or wrote the workshop's
    /// flag (AD-116, D-241).
    fn stopped(&self) -> bool {
        self.runtime.thread.latched() || crate::workshop::workshop_stopped(&self.ticket.home)
    }

    fn not_stopped(&self) -> Result<(), Failed> {
        if self.stopped() {
            Err(Failed(LaunchRefusal::WorkshopStopped.code().to_owned()))
        } else {
            Ok(())
        }
    }

    /// One `start`, the latch read right before it under the start gate
    /// Stop all's acknowledgement waits on (D-241).
    fn start(&self, container: &ObjectName) -> Result<(), Failed> {
        let _gate = self.runtime.thread.start_gate();
        self.not_stopped()?;
        self.call(&EngineCall::start(container))
    }

    /// Runs one step unless Stop all came first, recording it; a step
    /// stopped, or failed once Stop all came (an engine call it ended), is
    /// `stopped` with `workshop_stopped` (story 6.17 AC 2).
    fn step(
        &self,
        steps: &mut Vec<StepLine>,
        step: LaunchStep,
        run: impl FnOnce() -> Result<(), Failed>,
    ) -> Result<(), ()> {
        let ran = self.not_stopped().and_then(|()| run());
        let (outcome, detail) = match ran {
            Ok(()) => (StepOutcome::Ok, None),
            Err(Failed(_)) if self.stopped() => (
                StepOutcome::Stopped,
                Some(LaunchRefusal::WorkshopStopped.code().to_owned()),
            ),
            Err(Failed(why)) => (StepOutcome::Failed, Some(why)),
        };
        steps.push(StepLine {
            step,
            outcome,
            detail,
        });
        if outcome == StepOutcome::Ok {
            Ok(())
        } else {
            Err(())
        }
    }

    /// Steps 2 and 3: the run's folder, then the engine's objects; the
    /// needles of the credential the worker's `create` carried.
    fn run(&self, steps: &mut Vec<StepLine>) -> Result<Option<RunNeedles>, ()> {
        let in_dir = run_dir(&self.ticket.home, self.launch.facts().run_id.as_str()).join("in");
        self.step(steps, LaunchStep::Bundle, || self.bundle(&in_dir))?;
        self.step(steps, LaunchStep::Files, || {
            std::fs::write(in_dir.join("prompt.txt"), self.launch.order().prompt())
                .and_then(|()| {
                    std::fs::write(
                        in_dir.join("plan.json"),
                        serde_json::to_string(&run_plan(self.launch)).unwrap_or_default(),
                    )
                })
                .map_err(|e| Failed(format!("the run's folder: {e}")))?;
            self.not_stopped()
        })?;
        let names = self.launch.names();
        let [egress, internal] = networks(self.launch);
        self.step(steps, LaunchStep::Networks, || {
            self.call(&EngineCall::network_create(
                egress,
                false,
                &self.launch.labels(ContainerRole::Gateway),
            ))?;
            self.call(&EngineCall::network_create(
                internal,
                true,
                &self.launch.labels(ContainerRole::Worker),
            ))
        })?;
        self.step(steps, LaunchStep::Volume, || {
            self.call(&EngineCall::volume_create(
                &names.volume,
                &self.launch.labels(ContainerRole::Worker),
            ))
        })?;
        self.step(steps, LaunchStep::Gateway, || {
            self.call(&EngineCall::create(&gateway(self.launch)))?;
            self.call(&EngineCall::network_connect(internal, &names.gateway))
        })?;
        let mut needles = None;
        self.step(steps, LaunchStep::Worker, || {
            needles = self.create_worker()?;
            Ok(())
        })?;
        self.step(steps, LaunchStep::Copy, || {
            let source = format!("{}/.", in_dir.to_string_lossy().replace('\\', "/"));
            self.call(&EngineCall::cp_in(&source, &names.worker, IN_DIR))
        })?;
        self.step(steps, LaunchStep::Start, || {
            self.start(&names.gateway)?;
            self.start(&names.worker)
        })?;
        Ok(needles)
    }

    /// Step 2's bundle (AD-107, AD-111): `git bundle create` of the base
    /// under the workshop rule, its header holding the card's base commit
    /// alone, at most [`MAX_BASE_BUNDLE_BYTES`].
    fn bundle(&self, in_dir: &Path) -> Result<(), Failed> {
        std::fs::create_dir_all(in_dir).map_err(|e| Failed(format!("the run's folder: {e}")))?;
        let git = self
            .runtime
            .workshop_git(self.ticket)
            .ok_or_else(|| Failed("no workshop git".to_owned()))?;
        let bundle = in_dir.join("base.bundle");
        let branch = self.launch.base().branch();
        git.output(
            &self.ticket.project_dir,
            &[
                "bundle",
                "create",
                &bundle.to_string_lossy(),
                &format!("refs/heads/{branch}"),
            ],
        )
        .map_err(Failed)?;
        let size = std::fs::metadata(&bundle)
            .map_err(|e| Failed(format!("the bundle: {e}")))?
            .len();
        if size > MAX_BASE_BUNDLE_BYTES {
            return Err(Failed("base_too_large".to_owned()));
        }
        let mut head = Vec::new();
        std::fs::File::open(&bundle)
            .and_then(|file| file.take(MAX_HEADER_BYTES as u64).read_to_end(&mut head))
            .map_err(|e| Failed(format!("the bundle: {e}")))?;
        let header =
            parse_header(&head).map_err(|e| Failed(format!("the bundle: {}", e.code())))?;
        if holds_only(&header, branch, &self.launch.facts().base_commit) {
            Ok(())
        } else {
            Err(Failed(LaunchRefusal::BaseMoved.code().to_owned()))
        }
    }

    /// The worker's `create` (AD-105, D-206): for a token kind the value
    /// read now, set in that one child's environment, never in an
    /// argument; dropped once the needles are built. A `login` floor is
    /// refused before (D-621).
    fn create_worker(&self) -> Result<Option<RunNeedles>, Failed> {
        let create = EngineCall::create(&worker(self.launch));
        let kind = self.launch.credential_kind();
        let Some(name) = credential_name(kind) else {
            return Err(Failed("the login kind launches from story 6.26".to_owned()));
        };
        let secret = self
            .runtime
            .store
            .read(&credential_store_name(&self.ticket.floor_key))
            .map_err(|e| Failed(e.to_string()))?
            .ok_or_else(|| Failed(LaunchRefusal::CredentialMissing.code().to_owned()))?;
        let needles = RunNeedles::of(kind, &secret);
        let outcome = create_with_credential(
            self.docker()?,
            &create,
            &self.ticket.base_env,
            &self.ticket.home,
            (name, secret.expose()),
        )
        .map_err(|e| Failed(e.to_string()))?;
        drop(secret);
        if outcome.succeeded() {
            Ok(needles)
        } else {
            Err(Failed(format!("exit {:?}", outcome.code)))
        }
    }

    /// Removes every object the launch may have created (AD-107).
    fn remove(&self) {
        let names = self.launch.names();
        let containers: Vec<ObjectName> = vec![names.worker.clone(), names.gateway.clone()];
        let _ = self.call(&EngineCall::rm(&containers));
        let nets: Vec<ObjectName> = networks(self.launch).into_iter().cloned().collect();
        let _ = self.call(&EngineCall::network_rm(&nets));
        let _ = self.call(&EngineCall::volume_rm(std::slice::from_ref(&names.volume)));
    }
}
