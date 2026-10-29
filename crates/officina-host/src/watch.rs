//! The collector thread of the window (AD-3 to AD-5, AD-35, AD-43, AD-44,
//! AD-92): stat polling, refresh plan, diff before emit, for the board, the
//! relay view, the registry view and the Orchestrator view; the registry's
//! only writer (AD-38), which
//! ingests the event files before anything reads or deletes them, and runs
//! the author's actions the window's commands send it (D-92).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};

use officina_core::board::Board;
use officina_core::dates::event_file_name;
use officina_core::ipc::{IpcError, IpcErrorCode};
use officina_core::orders::card::{CardAnswer, CardChange, CardRequest};
use officina_core::orders::view::OrchestratorView;
use officina_core::registry::confirm::{AuthorAction, ConfirmItem, ItemAnswer};
use officina_core::registry::model::{ModelEnv, unavailable};
use officina_core::registry::retention::PASS_INTERVAL_MS;
use officina_core::registry::view::{DiscardedSession, RegistryView, SubjectView};
use officina_core::relay::journal::spilled_day_file;
use officina_core::relay::view::RelayView;

use crate::collector::Collector;
use crate::files::{event_file_for, now_ms};
use crate::meeting::MeetingGrant;
use crate::orders::orchestrator::{OrchestratorKeeper, StartGrant, StartReport};
use crate::orders::{CardStore, LaunchGrant, LaunchReport};
use crate::paths::{Paths, orchestrator_proposals_dir};
use crate::registry::proposals::ProposalPass;

use crate::registry::ingest::IngestContext;
use crate::registry::review::AutoState;
use crate::registry::store::onedrive_roots_from_env;
use crate::registry::triage::{CallLimits, NextCall, TriageJob, TriageWorker, binary_stamp};
use crate::registry::window::WindowRegistry;
use crate::relay::home::RelayHome;
use crate::relay::prompts::{self, MarkerFile};
use crate::relay::review::{ReviewEnv, ReviewWorker, Reviews, link_runs};
use crate::relay::view::{AutoRead, credential_presence, read_view_with};
use officina_core::registry::types::Source;
use officina_core::relay::auto::{AutoMode, STATS_WINDOW_MS};
use officina_core::relay::journal::{AutoApprovalSet, JournalEntry, PromptRouted};
use officina_core::relay::prompts::WindowMarker;

/// How often the watched paths are stat'ed. A handful of `metadata` calls:
/// cheap enough to run four times a second.
pub const TICK: Duration = Duration::from_millis(250);
/// Safety poll while the window is visible.
pub const POLL_VISIBLE: Duration = Duration::from_secs(15);
/// Safety poll while the window is minimised or hidden.
pub const POLL_HIDDEN: Duration = Duration::from_secs(60);
/// At most one `claude agents` spawn per this interval. One call costs about
/// 200 ms of CPU (E10), so the collector spawns on change, never on a timer
/// this short.
pub const MIN_SPAWN_INTERVAL: Duration = Duration::from_millis(500);
/// Second refresh after a hook event: the hook can fire before `claude agents`
/// reports the new status.
pub const FOLLOW_UP: Duration = Duration::from_secs(1);
/// The hourly pass's interval, the one the registry's retention bounds
/// count with.
pub const RETENTION_EVERY: Duration = Duration::from_millis(PASS_INTERVAL_MS as u64);
/// The relay view is rebuilt at least this often, so a run turns
/// "abandoned" and old gate errors leave the 24 h count without a file
/// change.
pub const RELAY_POLL: Duration = POLL_VISIBLE;
/// The approved credentials' presence is checked when the relay view is
/// rebuilt for a floor or relay change, and at most this often otherwise:
/// never on every tick (D-41).
pub const CREDENTIAL_POLL: Duration = Duration::from_secs(60);
/// The proposal files are ingested at least this often, so a work order
/// proposal turns `expired` without a file change (D-145).
pub const PROPOSALS_POLL: Duration = POLL_VISIBLE;
/// How long a window command waits for the collector's answer to an author
/// request (AD-38): a running `claude agents` takes about 210 ms (E10) and
/// is cut at [`crate::spawn::AGENTS_TIMEOUT`]; twice that is a choice, so
/// a request queued behind a call that runs to its deadline is still
/// answered.
pub const AUTHOR_WAIT: Duration = Duration::from_secs(10);
/// A request this close to its deadline is dropped unrun (D-92): the
/// action, the views it emits and the reply must all land before the window
/// stops waiting, so the window never reports a timeout for an action that
/// happened.
pub const AUTHOR_HEADROOM: Duration = Duration::from_secs(1);

/// What an author request asks of the registry (AD-54).
#[derive(Debug, Clone, PartialEq)]
pub enum AuthorCommand {
    /// The archived subjects, read on the collector's connection, so the
    /// window holds one connection.
    ListArchived,
    /// The sessions the author discarded from the board (story 4.15).
    ListDiscarded,
    /// An action on subjects and sessions, written in one transaction.
    Act(AuthorAction),
    /// Triage rows confirmed in one transaction, answered per item (AD-49).
    Confirm(Vec<ConfirmItem>),
    /// A dry run of the triage model, answered once queued (AD-52, D-92).
    Evaluate,
    /// Auto-archive turned on, under its gate, or off (FR-12).
    SetAutoArchive(bool),
    /// The triage model turned on, under its gate and while it can be
    /// called, or off (FR-10 as amended).
    SetTriageModel(bool),
    /// Auto approval turned on, under its gate, or off (AD-88).
    SetAutoApproval(bool),
    /// Auto approval's mode: `rule` under the rule offer, or `review`
    /// (D-162).
    SetAutoMode(AutoMode),
    /// An auto-approved run marked "should not have run", by run id
    /// (AD-88, FR-9).
    MarkShouldNotHaveRun(String),
    /// A work order card for these values (AD-78).
    IssueCard(CardRequest),
    /// A new card in place of `card_id`, with one change (AD-78).
    EditCard { card_id: String, change: CardChange },
    /// A card for the pending work order proposal `proposal_id`, through
    /// the same validation as the author's (AD-91, D-152).
    IssueCardFromProposal { proposal_id: String },
    /// A pending or invalid proposal set aside by the author (FR-12).
    DismissProposal { proposal_id: String },
    /// A click on Launch: AD-78's checks, then the card consumed and its
    /// row recorded `launching`, answered with the card's vector (AD-81
    /// step 1).
    LaunchCard {
        card_id: String,
        prompt_sha256: String,
    },
    /// How a granted launch ended (AD-81 step 3). It carries no deadline:
    /// the collector records it however late it comes, and never drops it
    /// (D-135, review F14).
    LaunchOutcome(Box<LaunchReport>),
    /// The author read the briefing: left the Orchestrator view, or closed
    /// the window (AD-83).
    MarkBriefingRead,
    /// A click on the orchestrator's Start, with the window's executable
    /// directory (AD-89, D-167).
    StartOrchestrator { exe_dir: PathBuf },
    /// How a granted orchestrator spawn ended. Like a launch's outcome it
    /// carries no deadline: the session runs, so its row and line are
    /// recorded however late (D-144).
    OrchestratorStarted(Box<StartReport>),
    /// Receive on a card, a `sid:` session or a `subject:` to resume
    /// (D-138): what the collector holds for its meeting, read nowhere
    /// else.
    OpenMeeting(String),
    /// Send on the meeting's composer (AD-85), or Send the proposed answer
    /// (AD-182): the delivery the panel showed and the author's text, or
    /// the proposal's id, whose text the collector reads (D-575).
    SendAnswer {
        key: String,
        expected: officina_core::meeting::Delivery,
        from: crate::meeting::AnswerFrom,
    },
    /// How a granted answer's start went. Like a launch's outcome it
    /// carries no deadline and is never dropped; for a resume, it is
    /// answered once the session is confirmed or its window passed
    /// (D-135, D-139, review F14).
    AnswerOutcome(Box<crate::meeting::AnswerReport>),
}

impl AuthorAnswer {
    /// The internal error of a command answered for another request: a
    /// fault of the window, never of the author.
    fn unexpected(&self) -> IpcError {
        IpcError::internal(format!("the collector answered another request: {self:?}"))
    }

    /// The subject an action created, for an action's answer.
    pub fn into_done(self) -> Result<Option<i64>, IpcError> {
        match self {
            AuthorAnswer::Done { created } => Ok(created),
            other => Err(other.unexpected()),
        }
    }

    /// The discarded sessions, for `ListDiscarded`'s answer.
    pub fn into_discarded(self) -> Result<Vec<DiscardedSession>, IpcError> {
        match self {
            AuthorAnswer::Discarded(listed) => Ok(listed),
            other => Err(other.unexpected()),
        }
    }

    /// The archived subjects, for `ListArchived`'s answer.
    pub fn into_archived(self) -> Result<Vec<SubjectView>, IpcError> {
        match self {
            AuthorAnswer::Archived(listed) => Ok(listed),
            other => Err(other.unexpected()),
        }
    }

    /// Each item's answer, for `Confirm`'s answer.
    pub fn into_confirmed(self) -> Result<Vec<ItemAnswer>, IpcError> {
        match self {
            AuthorAnswer::Confirmed(answers) => Ok(answers),
            other => Err(other.unexpected()),
        }
    }

    /// How many cases the dry run holds, for `Evaluate`'s answer.
    pub fn into_evaluating(self) -> Result<u32, IpcError> {
        match self {
            AuthorAnswer::Evaluating { cases } => Ok(cases),
            other => Err(other.unexpected()),
        }
    }

    /// The card or its refusal, for `IssueCard`'s and `EditCard`'s answer.
    pub fn into_card(self) -> Result<CardAnswer, IpcError> {
        match self {
            AuthorAnswer::Card(answer) => Ok(answer),
            other => Err(other.unexpected()),
        }
    }

    /// The answer's grant, for `SendAnswer`'s answer.
    pub fn into_answer_grant(self) -> Result<crate::meeting::AnswerGrant, IpcError> {
        match self {
            AuthorAnswer::AnswerGrant(grant) => Ok(*grant),
            other => Err(other.unexpected()),
        }
    }

    /// How an answer went, for `AnswerOutcome`'s answer.
    pub fn into_answered(self) -> Result<officina_core::meeting::AnswerResult, IpcError> {
        match self {
            AuthorAnswer::Answered(result) => Ok(result),
            other => Err(other.unexpected()),
        }
    }

    /// What the collector holds for a meeting, for `OpenMeeting`'s
    /// answer.
    pub fn into_meeting(self) -> Result<MeetingGrant, IpcError> {
        match self {
            AuthorAnswer::Meeting(grant) => Ok(*grant),
            other => Err(other.unexpected()),
        }
    }

    /// The launch's ticket or its refusal, for `LaunchCard`'s answer.
    pub fn into_launch(self) -> Result<LaunchGrant, IpcError> {
        match self {
            AuthorAnswer::Launch(grant) => Ok(grant),
            other => Err(other.unexpected()),
        }
    }

    /// The Start's ticket, refusal or "running", for `StartOrchestrator`'s
    /// answer.
    pub fn into_start(self) -> Result<StartGrant, IpcError> {
        match self {
            AuthorAnswer::Start(grant) => Ok(grant),
            other => Err(other.unexpected()),
        }
    }
}

/// What the collector answers to an author request.
#[derive(Debug, Clone, PartialEq)]
pub enum AuthorAnswer {
    /// The action was written; `created` is the subject it created, if any.
    Done {
        created: Option<i64>,
    },
    Archived(Vec<SubjectView>),
    Discarded(Vec<DiscardedSession>),
    /// What became of each confirmed item, in the order sent.
    Confirmed(Vec<ItemAnswer>),
    /// A dry run is queued, with this many cases; its progress shows in the
    /// registry view.
    Evaluating {
        cases: u32,
    },
    Card(CardAnswer),
    Launch(LaunchGrant),
    Start(StartGrant),
    Meeting(Box<MeetingGrant>),
    AnswerGrant(Box<crate::meeting::AnswerGrant>),
    Answered(officina_core::meeting::AnswerResult),
}

/// The collector's answer, or why there is none to give.
pub type AuthorReply = Result<AuthorAnswer, IpcError>;

/// A window command's request to the registry's writer (AD-38, D-92): run
/// on the collector thread, which answers on `reply`, or dropped unrun once
/// `deadline` is too close.
#[derive(Debug)]
pub struct AuthorRequest {
    pub command: AuthorCommand,
    pub deadline: Instant,
    pub reply: Sender<AuthorReply>,
}

impl AuthorRequest {
    /// A request whose deadline is `wait` from now, and the receiver its
    /// answer arrives on.
    pub fn new(command: AuthorCommand, wait: Duration) -> (AuthorRequest, Receiver<AuthorReply>) {
        let (reply, answer) = mpsc::channel();
        let request = AuthorRequest {
            command,
            deadline: Instant::now() + wait,
            reply,
        };
        (request, answer)
    }

    /// True when the collector must drop the request unrun (D-92). A
    /// launch's outcome and an orchestrator spawn's end never expire: the
    /// process started, and its row and line are recorded however late
    /// (D-135, review F14, D-144).
    fn expired(&self, now: Instant) -> bool {
        !matches!(
            self.command,
            AuthorCommand::LaunchOutcome(_)
                | AuthorCommand::OrchestratorStarted(_)
                | AuthorCommand::AnswerOutcome(_)
        ) && now + AUTHOR_HEADROOM >= self.deadline
    }
}

/// Sends `command` to the collector thread and waits at most `wait` for
/// its answer (AD-38, D-92). Past `wait`, or when the collector dropped the
/// request unrun near its deadline, the answer is a timeout: the action
/// did not run and will not, unless it is a launch's outcome, which the
/// collector records whenever it reaches it.
pub fn ask(sender: &Sender<CollectorMsg>, command: AuthorCommand, wait: Duration) -> AuthorReply {
    let (request, answer) = AuthorRequest::new(command, wait);
    sender
        .send(CollectorMsg::Author(request))
        .map_err(|_| IpcError::internal("the collector has stopped"))?;
    answer.recv_timeout(wait).unwrap_or_else(|_| {
        Err(IpcError::new(
            IpcErrorCode::Timeout,
            format!(
                "the registry did not answer within {} ms; the next view shows whether the change was made",
                wait.as_millis()
            ),
        ))
    })
}

/// Messages to the collector thread.
#[derive(Debug)]
pub enum CollectorMsg {
    Refresh,
    Visibility(bool),
    /// An authority command changed the relay's files: rebuild the relay
    /// view and the board now rather than at the next tick (AD-35).
    RelayChanged,
    /// A window command's request to the registry (AD-38).
    Author(AuthorRequest),
    Shutdown,
}

/// Where the collector thread sends what changed (AD-5): each method is
/// called from the collector thread with a value that differs from the last
/// one it was given, and `registry` and `orchestrator` also right after
/// each author action.
pub trait Emit: Send + 'static {
    fn board(&self, board: &Board);
    fn relay(&self, view: &RelayView);
    fn registry(&self, view: &RegistryView);
    fn orchestrator(&self, view: &OrchestratorView);
}

/// Handle on a running collector thread.
pub struct WatchHandle {
    pub sender: Sender<CollectorMsg>,
    pub board: Arc<RwLock<Option<Board>>>,
    /// The last relay view, for the window's first paint (`get_relay`).
    pub relay: Arc<RwLock<Option<RelayView>>>,
    /// The last registry view, for the window's first paint
    /// (`get_registry`), which never goes through the collector (D-92).
    pub registry: Arc<RwLock<Option<RegistryView>>>,
    /// The last Orchestrator view, for the window's first paint
    /// (`get_orchestrator`), which never goes through the collector.
    pub orchestrator: Arc<RwLock<Option<OrchestratorView>>>,
    /// The collector's paths, updated when a reloaded floor moves the config
    /// dir, so the window's focus and attach commands follow it.
    pub paths: Arc<RwLock<Paths>>,
    pub thread: std::thread::JoinHandle<()>,
}

#[derive(Default, PartialEq, Eq, Clone, Copy)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
}

fn stamp(path: &std::path::Path) -> Stamp {
    std::fs::metadata(path).map_or(Stamp::default(), |m| Stamp {
        len: m.len(),
        modified: m.modified().ok(),
    })
}

/// Name, size and modification time of the files of `dir` that `keep`
/// selects, and of the files one folder below when `nested`.
fn dir_entries(
    dir: &Path,
    nested: bool,
    keep: &dyn Fn(&str) -> bool,
    out: &mut Vec<(String, u64, Option<SystemTime>)>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() {
            if nested {
                let mut below = Vec::new();
                dir_entries(&entry.path(), false, keep, &mut below);
                out.extend(
                    below
                        .into_iter()
                        .map(|(n, len, at)| (format!("{name}/{n}"), len, at)),
                );
            }
        } else if keep(&name) {
            out.push((name, meta.len(), meta.modified().ok()));
        }
    }
}

fn fingerprint_of(mut entries: Vec<(String, u64, Option<SystemTime>)>) -> u64 {
    use std::hash::{Hash, Hasher};
    entries.sort();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    entries.hash(&mut hasher);
    hasher.finish()
}

/// Fingerprint of the session files in the Claude sessions dir: name, size and
/// modification time of each `*.json`, never their content. Claude Code
/// rewrites a session's file on every state change (E10) while the directory
/// itself only changes when a session appears or ends, so the directory's own
/// date misses most transitions. Other files (the `.key` files) are not even
/// stat'ed.
fn sessions_fingerprint(dir: &Path) -> u64 {
    let mut entries = Vec::new();
    dir_entries(dir, false, &|name| name.ends_with(".json"), &mut entries);
    fingerprint_of(entries)
}

/// Fingerprint of the `relay/` tree (AD-35): name, size and modification
/// time of every file in it and in its folders (`approvals/`).
fn relay_fingerprint(dir: &Path) -> u64 {
    let mut entries = Vec::new();
    dir_entries(dir, true, &|_| true, &mut entries);
    fingerprint_of(entries)
}

/// Fingerprint of `orchestrator/proposals/` (AD-91), as of `relay/`: name,
/// size and modification time of every file in it.
fn proposals_fingerprint(dir: &Path) -> u64 {
    let mut entries = Vec::new();
    dir_entries(dir, false, &|_| true, &mut entries);
    fingerprint_of(entries)
}

/// What the refresh plan asks the collector thread to do now.

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Due {
    /// Ingest the event files into the registry, before the session list
    /// and the board read them (D-78): today's file grew, or a session list
    /// is due.
    ingest: bool,
    /// Run `claude agents`.
    agents: bool,
    /// Then re-read the `claude` version and re-check the contract.
    contract: bool,
    /// Run the retention pass.
    retention: bool,
    /// Rebuild the board, and emit it if it changed.
    recompute: bool,
    /// Rebuild the relay view, and emit it if it changed.
    relay: bool,
    /// Check the credentials' presence before rebuilding it (D-41).
    credentials: bool,
    /// Ingest the orchestrator's proposal files (AD-91): one changed, or
    /// their poll, which also expires the old ones.
    proposals: bool,
}

/// The refresh plan of AD-4. Each trigger has its method, which records what
/// the trigger asks for; [`RefreshPlan::due`], called once per tick, decides
/// what runs now under the spawn rate limit and the timers. Triggers that
/// arrive within one tick, or while a spawn is held back by the rate limit,
/// coalesce into one spawn. No IO, and time comes only from `clock`, so the
/// plan is tested without waiting.
struct RefreshPlan<C> {
    clock: C,
    visible: bool,
    ingest_wanted: bool,
    agents_wanted: bool,
    contract_wanted: bool,
    recompute_wanted: bool,
    relay_wanted: bool,
    credentials_wanted: bool,
    retention_wanted: bool,
    proposals_wanted: bool,
    last_spawn: Option<Instant>,
    follow_up_at: Option<Instant>,
    last_retention: Instant,
    last_relay: Instant,
    last_credentials: Instant,
    last_proposals: Instant,
}

impl<C: Fn() -> Instant> RefreshPlan<C> {
    /// At startup the session list, the contract check, a first board and a
    /// first relay view are due; the hourly pass has just run.
    fn new(clock: C) -> Self {
        let now = clock();
        RefreshPlan {
            clock,
            visible: true,
            ingest_wanted: false,
            agents_wanted: true,
            contract_wanted: true,
            recompute_wanted: true,
            relay_wanted: true,
            credentials_wanted: true,
            retention_wanted: false,
            proposals_wanted: true,
            last_spawn: None,
            follow_up_at: None,
            last_retention: now,
            last_relay: now,
            last_credentials: now,
            last_proposals: now,
        }
    }

    /// `CollectorMsg::Refresh`.
    fn refresh_requested(&mut self) {
        self.agents_wanted = true;
    }

    /// An answer awaits its confirmation: the session list is wanted at
    /// every tick, so it runs at the minimum spawn interval, not the safety
    /// poll, and a short turn falls on a listing (D-160).
    fn answer_watched(&mut self) {
        self.agents_wanted = true;
    }

    /// `CollectorMsg::Visibility`: picks the safety poll interval.
    fn visibility_changed(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Today's event file grew: a hook fired. Its line is ingested and shows
    /// at once; the session list is refreshed now and once more after
    /// [`FOLLOW_UP`].
    fn event_file_changed(&mut self) {
        self.ingest_wanted = true;
        self.recompute_wanted = true;
        self.agents_wanted = true;
        self.follow_up_at = Some((self.clock)() + FOLLOW_UP);
    }

    /// A session file appeared, changed or went away (E10).
    fn session_files_changed(&mut self) {
        self.agents_wanted = true;
    }

    /// The quota file changed: a new board, no spawn.
    fn quota_changed(&mut self) {
        self.recompute_wanted = true;
    }

    /// The `claude` binary changed: the contract is checked with the next
    /// session list, which this does not bring forward.
    fn claude_binary_changed(&mut self) {
        self.contract_wanted = true;
    }

    /// The version could not be read: it is asked again with the next
    /// session list, which this does not bring forward (D-880).
    fn version_unread(&mut self) {
        self.contract_wanted = true;
    }

    /// Today's journal changed: a new relay view, no spawn; the
    /// credentials wait for their poll (D-41).
    fn relay_changed(&mut self) {
        self.relay_wanted = true;
    }

    /// The relay floor changed: a new relay view, its credentials checked
    /// again (D-41).
    fn relay_floor_changed(&mut self) {
        self.relay_wanted = true;
        self.credentials_wanted = true;
    }

    /// The relay's own files changed, an approval request among them, or an
    /// authority command says so: a new relay view and a new board, whose
    /// Needs you lists the requests (AD-35); no spawn.
    fn relay_files_changed(&mut self) {
        self.relay_wanted = true;
        self.recompute_wanted = true;
        self.credentials_wanted = true;
    }

    /// A file of `orchestrator/proposals/` appeared, changed or went away
    /// (AD-91): ingested at the next tick, no spawn.
    fn proposals_changed(&mut self) {
        self.proposals_wanted = true;
    }

    /// The window's floor changed and was reloaded: its projects reach a new
    /// board, its config dir may name other sessions, and the relay view may
    /// read it too.
    fn floor_changed(&mut self) {
        self.recompute_wanted = true;
        self.agents_wanted = true;
        self.relay_wanted = true;
        self.credentials_wanted = true;
    }

    /// `no_excerpts` was just turned on: the pass that clears the stored
    /// excerpts runs at the next tick, not up to an hour later (D-85); the
    /// view shows none meanwhile.
    fn excerpts_turned_off(&mut self) {
        self.retention_wanted = true;
    }

    /// What runs now.
    fn due(&mut self) -> Due {
        let now = (self.clock)();
        if self.follow_up_at.is_some_and(|at| now >= at) {
            self.follow_up_at = None;
            self.agents_wanted = true;
        }
        let poll = if self.visible {
            POLL_VISIBLE
        } else {
            POLL_HIDDEN
        };
        let since_spawn = |interval| {
            self.last_spawn
                .is_none_or(|t| now.duration_since(t) >= interval)
        };
        if since_spawn(poll) {
            self.agents_wanted = true;
        }
        let mut due = Due::default();
        if self.agents_wanted && since_spawn(MIN_SPAWN_INTERVAL) {
            self.agents_wanted = false;
            self.last_spawn = Some(now);
            self.recompute_wanted = true;
            due.agents = true;
            due.contract = std::mem::take(&mut self.contract_wanted);
        }
        if std::mem::take(&mut self.retention_wanted)
            || now.duration_since(self.last_retention) >= RETENTION_EVERY
        {
            self.last_retention = now;
            due.retention = true;
        }
        due.ingest = std::mem::take(&mut self.ingest_wanted) || due.agents;
        due.recompute = std::mem::take(&mut self.recompute_wanted);
        if std::mem::take(&mut self.relay_wanted)
            || now.duration_since(self.last_relay) >= RELAY_POLL
        {
            self.last_relay = now;
            due.relay = true;
            if std::mem::take(&mut self.credentials_wanted)
                || now.duration_since(self.last_credentials) >= CREDENTIAL_POLL
            {
                self.last_credentials = now;
                due.credentials = true;
            }
        }
        if std::mem::take(&mut self.proposals_wanted)
            || now.duration_since(self.last_proposals) >= PROPOSALS_POLL
        {
            self.last_proposals = now;
            due.proposals = true;
        }
        due
    }
}

/// The paths the collector stats each tick (AD-4, AD-35) and what they
/// looked like last time. Built from the collector's paths, so a reloaded
/// floor's config dir is followed by building it again.
struct WatchedPaths {
    events_dir: PathBuf,
    sessions_dir: PathBuf,
    quota_file: PathBuf,
    claude: Option<PathBuf>,
    floor_file: PathBuf,
    relay: RelayHome,
    relay_floor_file: PathBuf,
    event_file: Stamp,
    sessions: u64,
    quota: Stamp,
    claude_binary: Option<Stamp>,
    floor: Stamp,
    relay_tree: u64,
    relay_floor: Stamp,
    journal: (Stamp, u64),
    proposals_dir: PathBuf,
    proposals: u64,
}

impl WatchedPaths {
    fn new(collector: &Collector) -> WatchedPaths {
        let events_dir = collector.paths().events_dir();
        let sessions_dir = collector.paths().claude_sessions_dir();
        let quota_file = collector.paths().quota_file();
        let claude = collector.claude().map(Path::to_path_buf);
        let floor_file = collector.watched_floor();
        let relay = collector.relay().clone();
        let relay_floor_file = relay.relay_floor(&relay.read_approved());
        let proposals_dir = orchestrator_proposals_dir(&collector.paths().officina_home);
        WatchedPaths {
            proposals: proposals_fingerprint(&proposals_dir),
            proposals_dir,
            event_file: stamp(&event_file_for(&events_dir, now_ms())),
            sessions: sessions_fingerprint(&sessions_dir),
            quota: stamp(&quota_file),
            claude_binary: claude.as_deref().map(stamp),
            floor: stamp(&floor_file),
            relay_tree: relay_fingerprint(&relay.relay_dir()),
            relay_floor: stamp(&relay_floor_file),
            journal: journal_stamps(&relay),
            events_dir,
            sessions_dir,
            quota_file,
            claude,
            floor_file,
            relay,
            relay_floor_file,
        }
    }

    /// Stats every watched path and tells `plan` what changed. Returns true
    /// when the window's floor changed: the caller reloads it and builds the
    /// watched paths again.
    fn poll(&mut self, plan: &mut RefreshPlan<impl Fn() -> Instant>) -> bool {
        let proposals = proposals_fingerprint(&self.proposals_dir);
        if proposals != self.proposals {
            self.proposals = proposals;
            plan.proposals_changed();
        }
        let relay_tree = relay_fingerprint(&self.relay.relay_dir());
        if relay_tree != self.relay_tree {
            self.relay_tree = relay_tree;
            // The approved record names the relay floor (AD-24).
            self.relay_floor_file = self.relay.relay_floor(&self.relay.read_approved());
            plan.relay_files_changed();
        }
        let current = stamp(&self.relay_floor_file);
        if current != self.relay_floor {
            self.relay_floor = current;
            plan.relay_floor_changed();
        }
        let current = journal_stamps(&self.relay);
        if current != self.journal {
            self.journal = current;
            plan.relay_changed();
        }
        let current = stamp(&self.floor_file);
        if current != self.floor {
            self.floor = current;
            return true;
        }
        let current = stamp(&event_file_for(&self.events_dir, now_ms()));
        if current != self.event_file {
            self.event_file = current;
            plan.event_file_changed();
        }
        let current = sessions_fingerprint(&self.sessions_dir);
        if current != self.sessions {
            self.sessions = current;
            plan.session_files_changed();
        }
        let current = stamp(&self.quota_file);
        if current != self.quota {
            self.quota = current;
            plan.quota_changed();
        }
        let current = self.claude.as_deref().map(stamp);
        if current != self.claude_binary {
            self.claude_binary = current;
            plan.claude_binary_changed();
        }
        false
    }
}

/// Today's journal file (AD-32), the one appends go to, and the
/// fingerprint of the spill files beside it (D-760).
fn journal_stamps(relay: &RelayHome) -> (Stamp, u64) {
    let dir = relay.journal_dir();
    let mut spills = Vec::new();
    dir_entries(
        &dir,
        false,
        &|name| spilled_day_file(name).is_some(),
        &mut spills,
    );
    (
        stamp(&dir.join(event_file_name(now_ms()))),
        fingerprint_of(spills),
    )
}

/// One payload the collector shares with the window's commands and emits:
/// its slot, and the bytes it last emitted (quality audit M7: one shape
/// for the board, the relay view and the registry view).
struct Published<T> {
    slot: Arc<RwLock<Option<T>>>,
    last: Vec<u8>,
}

impl<T: Clone + serde::Serialize> Published<T> {
    fn new(slot: &Arc<RwLock<Option<T>>>) -> Published<T> {
        Published {
            slot: Arc::clone(slot),
            last: Vec::new(),
        }
    }

    /// Shares `value`, and says whether to emit it: when `comparable`, the
    /// value with what does not count for a change cleared, serialises to
    /// other bytes than the last emitted, or when `force`.
    fn share(&mut self, value: &T, comparable: &T, force: bool) -> bool {
        let bytes = serde_json::to_vec(comparable).unwrap_or_default();
        if let Ok(mut slot) = self.slot.write() {
            *slot = Some(value.clone());
        }
        if force || bytes != self.last {
            self.last = bytes;
            return true;
        }
        false
    }

    /// What was last shared.
    fn current(&self) -> Option<T> {
        self.slot.read().ok().and_then(|slot| slot.clone())
    }
}

/// The payloads of one collector thread, the shared paths, and where it
/// emits.
struct Outputs<E> {
    board: Published<Board>,
    /// The last board's `next_escalation_at_ms`: when the tick reaches it,
    /// the board is rebuilt from the cached inputs, no spawn (AD-168).
    next_escalation_at_ms: Option<i64>,
    relay: Published<RelayView>,
    registry: Published<RegistryView>,
    orchestrator: Published<OrchestratorView>,
    paths: Arc<RwLock<Paths>>,
    emit: E,
}

/// Starts the collector thread. `emit` receives each board, relay view,
/// registry view and Orchestrator view that differs from the previous one,
/// from the collector thread (AD-5).
pub fn spawn(mut collector: Collector, emit: impl Emit) -> std::io::Result<WatchHandle> {
    let (sender, receiver) = mpsc::channel();
    let board = Arc::new(RwLock::new(None));
    let relay = Arc::new(RwLock::new(None));
    let registry = Arc::new(RwLock::new(None));
    let orchestrator = Arc::new(RwLock::new(None));
    let paths = Arc::new(RwLock::new(collector.paths().clone()));
    let mut outputs = Outputs {
        board: Published::new(&board),
        next_escalation_at_ms: None,
        relay: Published::new(&relay),
        registry: Published::new(&registry),
        orchestrator: Published::new(&orchestrator),
        paths: Arc::clone(&paths),
        emit,
    };
    let thread = std::thread::Builder::new()
        .name("collector".into())
        .spawn(move || run(&mut collector, &receiver, &mut outputs))?;
    Ok(WatchHandle {
        sender,
        board,
        relay,
        registry,
        orchestrator,
        paths,
        thread,
    })
}

/// What an ingestion pass needs from the collector: the window's floor and
/// excerpt setting (AD-43, D-53).
fn ingest_context<'a>(collector: &'a Collector, events: &'a Path) -> IngestContext<'a> {
    IngestContext {
        events_dir: events,
        officina_home: &collector.paths().officina_home,
        projects: &collector.floor().projects,
        no_excerpts: collector.no_excerpts(),
    }
}

/// One ingestion pass into the window's registry (AD-43), with the
/// window's floor and excerpt setting; the board then reports its state.
fn ingest(collector: &mut Collector, registry: &mut WindowRegistry) {
    let events = collector.paths().events_dir();
    let ctx = ingest_context(collector, &events);
    let report = registry.ingest(&ctx, now_ms());
    if let Some(report) = report {
        collector.record_hook_sightings(&report.hook_sightings);
    }
    report_registry(collector, registry);
}

/// Hands the registry's state and the subject of each filed session to the
/// collector: the next board reports one and shows the other on its cards
/// (AD-54).
fn report_registry(collector: &mut Collector, registry: &mut WindowRegistry) {
    // The subjects first: a read that fails there shows in the report.
    let subjects = registry.card_subjects();
    let settlements = registry.card_settlements();
    collector.set_registry(registry.report());
    collector.set_subjects(subjects);
    collector.set_settlements(settlements);
}

/// Folds the session list the collector just received into the window's
/// registry (AD-46), after the tick's ingestion (D-78).
fn apply_agents(collector: &mut Collector, registry: &mut WindowRegistry, now_ms: i64) {
    let records = collector.agents().unwrap_or_default();
    registry.apply_agents(records, &collector.floor().projects, now_ms);
    report_registry(collector, registry);
}

/// One ingestion pass of the orchestrator's proposal files (AD-91, D-155),
/// validated against the approved snapshot the window's record confirms,
/// read only when a file waits, and an answer against the sessions the
/// latest `board` shows taking one (AD-182); the pass also expires the old
/// ones. True when it changed what the Orchestrator view shows.
fn ingest_proposals(
    collector: &Collector,
    registry: &mut WindowRegistry,
    board: Option<&Board>,
) -> bool {
    let dir = orchestrator_proposals_dir(&collector.paths().officina_home);
    let waiting = std::fs::read_dir(&dir).is_ok_and(|mut entries| entries.next().is_some());
    let catalogue = waiting
        .then(|| crate::orders::approved_catalogue(collector.relay()))
        .flatten();
    let answerable = board
        .map(|board| crate::meeting::answerable(collector, board))
        .unwrap_or_default();
    registry
        .ingest_proposals(&dir, catalogue.as_ref(), &answerable, now_ms())
        .is_some_and(|pass| pass != ProposalPass::default())
}

/// The hourly pass, also run at start before the first board (AD-44,
/// D-44, D-85): the registry's part (ingest, copy when due, retention, the
/// WAL truncated) only while the registry is writable, then v0.2's focus
/// log, approval and journal cleanups, which do not depend on it.
fn hourly_pass(collector: &mut Collector, registry: &mut WindowRegistry) {
    let events = collector.paths().events_dir();
    let ctx = ingest_context(collector, &events);
    let pass = registry.hourly_pass(&ctx, now_ms());
    if let Some(pass) = pass {
        collector.record_hook_sightings(&pass.ingested.hook_sightings);
    }
    crate::focus::prune_focus_log(&collector.paths().state_dir(), now_ms());
    crate::relay::approvals::prune(collector.relay(), SystemTime::now());
    crate::relay::journal::prune(collector.relay(), now_ms());
    prompts::prune(collector.relay(), SystemTime::now());
    report_registry(collector, registry);
}

/// Whether the window routes `PermissionRequest`s, which its marker says
/// (D-579). False until the routing story ships and the owner turns
/// routing on (D-580): the hook then always gives Claude Code's own prompt.
const ROUTES_PROMPTS: bool = false;

/// Journals `prompt_routed` for each request in `relay/prompts/` this
/// collector has not seen yet (AD-181), never its input, and forgets the
/// ones gone, so `routed` stays as small as the folder.
fn journal_routed_prompts(relay: &RelayHome, routed: &mut BTreeSet<String>) {
    let waiting = prompts::read_requests(relay);
    routed.retain(|id| waiting.iter().any(|request| &request.id == id));
    let lines: Vec<JournalEntry> = waiting
        .into_iter()
        .filter(|request| routed.insert(request.id.clone()))
        .map(|request| {
            JournalEntry::PromptRouted(PromptRouted {
                session_id: request.session_id,
                tool_name: request.tool_name,
                input_sha256: request.input_sha256,
            })
        })
        .collect();
    let _ = crate::relay::journal::record(relay, &lines, now_ms());
}

fn run<E: Emit>(
    collector: &mut Collector,
    receiver: &Receiver<CollectorMsg>,
    outputs: &mut Outputs<E>,
) {
    // D-579: the window's liveness marker, refreshed by this loop and
    // removed when it returns. Without one, hooks give Claude Code's own
    // prompt, the safe side.
    let mut marker = MarkerFile::write(
        collector.relay(),
        &WindowMarker {
            pid: std::process::id(),
            started_at_ms: now_ms(),
            routes_prompts: ROUTES_PROMPTS,
        },
    )
    .ok();
    let mut routed_prompts = BTreeSet::new();
    // At start, every incomplete file is ingested and the privacy bounds
    // applied before the first board is built (D-85).
    let mut registry = WindowRegistry::open(
        &collector.paths().officina_home,
        &onedrive_roots_from_env(),
        now_ms(),
    );
    registry.expire_launching(now_ms());
    hourly_pass(collector, &mut registry);
    // No thread, no call: the board does not depend on it, and no dry run
    // is accepted that would wait on it forever.
    let mut worker = TriageWorker::spawn().ok();
    if worker.is_none() {
        registry.triage_worker_gone(now_ms());
    }
    // No `review` thread: no request is reviewed, and every `auto` request
    // is the author's, as a `human` one.
    let mut reviews = Reviews::new(ReviewWorker::spawn().ok());
    let mut watched = WatchedPaths::new(collector);
    let mut plan = RefreshPlan::new(Instant::now);
    let mut presence: BTreeMap<String, bool> = BTreeMap::new();
    let mut cards = CardStore::default();
    // D-144: the pid the newest spawned start kept, read back, so a Start
    // after a window restart still finds the session it started.
    let mut keeper = OrchestratorKeeper::new(registry.kept_orchestrator());
    let mut confirmations: Vec<crate::meeting::PendingConfirmation> = Vec::new();

    loop {
        registry.set_model_env(model_env(collector));
        match receiver.recv_timeout(TICK) {
            Ok(CollectorMsg::Shutdown) | Err(RecvTimeoutError::Disconnected) => return,
            Ok(CollectorMsg::Refresh) => plan.refresh_requested(),
            Ok(CollectorMsg::Visibility(visible)) => plan.visibility_changed(visible),
            Ok(CollectorMsg::RelayChanged) => plan.relay_files_changed(),
            Ok(CollectorMsg::Author(request)) => {
                let held = Held {
                    cards: &mut cards,
                    keeper: &mut keeper,
                    confirmations: &mut confirmations,
                };
                if answer_author(collector, &mut registry, held, request, outputs) {
                    plan.relay_files_changed();
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        if let Some(marker) = &mut marker {
            marker.refresh_if_due(Instant::now());
        }
        if watched.poll(&mut plan) {
            let had_excerpts = !collector.no_excerpts();
            collector.reload_floor();
            if let Ok(mut paths) = outputs.paths.write() {
                paths.clone_from(collector.paths());
            }
            watched = WatchedPaths::new(collector);
            plan.floor_changed();
            if had_excerpts && collector.no_excerpts() {
                plan.excerpts_turned_off();
            }
        }
        // D-136: a launch whose outcome never came back reads `unknown`
        // within a tick of its expiry, whatever else is due.
        registry.expire_launching(now_ms());
        if !confirmations.is_empty() {
            plan.answer_watched();
        }
        let due = plan.due();
        // Before the session list and the board read anything (D-78).
        if due.ingest {
            ingest(collector, &mut registry);
        }
        if due.agents {
            let now = now_ms();
            if collector.refresh_agents(now) {
                apply_agents(collector, &mut registry, now);
            }
            if due.contract && !collector.refresh_contract(now_ms()) {
                plan.version_unread();
            }
        }
        if !confirmations.is_empty() {
            crate::meeting::settle_confirmations(collector, &mut confirmations, now_ms());
        }
        if due.retention {
            hourly_pass(collector, &mut registry);
        }
        if due.proposals
            && ingest_proposals(collector, &mut registry, outputs.board.current().as_ref())
        {
            publish_orchestrator(&mut registry, outputs, false, now_ms());
        }
        // A wait reaching the alert threshold is a state change the clock
        // causes: the board is rebuilt from what the collector holds,
        // with no `claude` spawn (AD-168, D-467).
        let escalation = outputs
            .next_escalation_at_ms
            .is_some_and(|at| now_ms() >= at);
        // The hourly pass can clear excerpts the registry view shows.
        if due.recompute || due.retention || escalation {
            publish_both(collector, &mut registry, outputs, false);
        }
        if let Some(worker) = &mut worker {
            pump_triage(collector, &mut registry, worker, outputs);
        }
        let reviewed = pump_review(collector, &mut registry, &mut reviews, outputs);
        if due.relay || reviewed {
            journal_routed_prompts(collector.relay(), &mut routed_prompts);
            collector.keep_known_projects(now_ms());
            if due.credentials {
                presence = credential_presence(collector.relay(), collector.credential_store());
            }
            let now = now_ms();
            let auto = auto_read(&mut registry, now);
            // A home that cannot be read keeps the last view. A new one
            // changes the briefing's runs and the composer's templates.
            if let Ok(view) = read_view_with(collector.relay(), now, Some(&presence), &auto)
                && publish_relay(view, outputs)
            {
                publish_orchestrator(&mut registry, outputs, false, now_ms());
            }
        }
    }
}

/// Builds the board and the registry view, whose sessions join the board's
/// live cards, and emits each that changed; the registry view also when
/// `force`, after an author action (AD-54).
fn publish_both<E: Emit>(
    collector: &Collector,
    registry: &mut WindowRegistry,
    outputs: &mut Outputs<E>,
    force: bool,
) {
    let now = now_ms();
    rebuild_and_publish(collector, registry, outputs, force, now);
    // After each view build (D-70): what it archived, or its switch turned
    // off, shows in the next view at once.
    if registry.auto_archive(now) {
        rebuild_and_publish(collector, registry, outputs, false, now_ms());
    }
}

/// Builds the board and the registry view at `now`, one time for both, and
/// publishes each.
fn rebuild_and_publish<E: Emit>(
    collector: &Collector,
    registry: &mut WindowRegistry,
    outputs: &mut Outputs<E>,
    force: bool,
    now: i64,
) {
    let board = collector.board(now);
    // Each board ends the answer proposals whose session no longer takes
    // one (AD-182), before the views show them.
    registry.expire_answers(&crate::meeting::answerable(collector, &board), now);
    let view = registry.view(Some(&board), &collector.paths().events_dir(), now);
    publish(board, outputs);
    publish_registry(view, outputs, force);
    publish_orchestrator(registry, outputs, force, now);
}

/// What the collector thread holds for the author's requests beside the
/// registry: the work order cards and the orchestrator's keeper.
struct Held<'a> {
    cards: &'a mut CardStore,
    keeper: &'a mut OrchestratorKeeper,
    /// The resumed answers watched for `busy` (AD-85, D-160).
    confirmations: &'a mut Vec<crate::meeting::PendingConfirmation>,
}

/// Runs one author request on the registry's writer (AD-38), unless its
/// deadline is too close (D-92): an action is written, the cards' subjects
/// refreshed, and the board and the registry view emitted before the reply.
/// Returns true when the relay view must be rebuilt: an `auto` switch or a
/// mark changed what it shows.
fn answer_author<E: Emit>(
    collector: &mut Collector,
    registry: &mut WindowRegistry,
    held: Held<'_>,
    request: AuthorRequest,
    outputs: &mut Outputs<E>,
) -> bool {
    let Held {
        cards,
        keeper,
        confirmations,
    } = held;
    if request.expired(Instant::now()) {
        return false;
    }
    let relay_shows = matches!(
        request.command,
        AuthorCommand::SetAutoApproval(_)
            | AuthorCommand::SetAutoMode(_)
            | AuthorCommand::MarkShouldNotHaveRun(_)
    );
    // One time for the whole request, so what it writes and the views it
    // publishes agree.
    let now = now_ms();
    let writes = !matches!(
        request.command,
        AuthorCommand::ListArchived
            | AuthorCommand::ListDiscarded
            | AuthorCommand::OpenMeeting(_)
            | AuthorCommand::SendAnswer {
                from: crate::meeting::AnswerFrom::Author(_),
                ..
            }
            | AuthorCommand::AnswerOutcome(_)
            | AuthorCommand::IssueCard(_)
            | AuthorCommand::EditCard { .. }
    );
    let quota = outputs.board.current().map(|board| board.quota);
    let reply = match request.command {
        AuthorCommand::ListArchived => {
            let board = outputs.board.current();
            registry
                .archived(board.as_ref())
                .map(AuthorAnswer::Archived)
        }
        AuthorCommand::ListDiscarded => registry.discarded().map(AuthorAnswer::Discarded),
        AuthorCommand::Act(action) => registry
            .act(&action, now)
            .map(|created| AuthorAnswer::Done { created }),
        AuthorCommand::Confirm(items) => registry.confirm(&items, now).map(AuthorAnswer::Confirmed),
        AuthorCommand::SetAutoArchive(on) => registry
            .set_auto_archive(on, now)
            .map(|()| AuthorAnswer::Done { created: None }),
        AuthorCommand::SetTriageModel(on) => registry
            .set_triage_model(on, now)
            .map(|()| AuthorAnswer::Done { created: None }),
        AuthorCommand::SetAutoApproval(on) => registry.set_auto_approval(on, now).map(|state| {
            journal_auto_state(collector.relay(), state, now);
            AuthorAnswer::Done { created: None }
        }),
        AuthorCommand::SetAutoMode(mode) => registry.set_auto_mode(mode, now).map(|state| {
            journal_auto_state(collector.relay(), state, now);
            AuthorAnswer::Done { created: None }
        }),
        AuthorCommand::MarkShouldNotHaveRun(run_id) => {
            // A run started since the last tick is linked first.
            link_runs(collector.relay(), registry, now);
            registry
                .mark_should_not_have_run(&run_id, now)
                .map(|state| {
                    journal_auto_state(collector.relay(), state, now);
                    AuthorAnswer::Done { created: None }
                })
        }
        AuthorCommand::Evaluate => registry
            .evaluate(now)
            .map(|cases| AuthorAnswer::Evaluating { cases }),
        AuthorCommand::IssueCard(card_request) => crate::orders::issue(
            cards,
            collector,
            registry,
            quota.as_ref(),
            &card_request,
            now,
        )
        .map(AuthorAnswer::Card),
        AuthorCommand::EditCard { card_id, change } => crate::orders::edit(
            cards,
            collector,
            registry,
            quota.as_ref(),
            &card_id,
            &change,
            now,
        )
        .map(AuthorAnswer::Card),
        AuthorCommand::IssueCardFromProposal { proposal_id } => crate::orders::issue_from_proposal(
            cards,
            collector,
            registry,
            quota.as_ref(),
            &proposal_id,
            now,
        )
        .map(AuthorAnswer::Card),
        AuthorCommand::DismissProposal { proposal_id } => registry
            .dismiss_proposal(&proposal_id, now)
            .map(|()| AuthorAnswer::Done { created: None }),
        AuthorCommand::LaunchCard {
            card_id,
            prompt_sha256,
        } => crate::orders::grant(cards, collector, registry, &card_id, &prompt_sha256, now)
            .map(AuthorAnswer::Launch),
        AuthorCommand::LaunchOutcome(report) => {
            crate::orders::record(collector, registry, &report, now)
                .map(|()| AuthorAnswer::Done { created: None })
        }
        AuthorCommand::MarkBriefingRead => registry
            .mark_briefing_read(now)
            .map(|()| AuthorAnswer::Done { created: None }),
        AuthorCommand::StartOrchestrator { exe_dir } => {
            let board = outputs.board.current();
            crate::orders::orchestrator::grant(
                keeper,
                collector,
                registry,
                board.as_ref(),
                &exe_dir,
                now,
            )
            .map(AuthorAnswer::Start)
        }
        AuthorCommand::OrchestratorStarted(report) => {
            crate::orders::orchestrator::record_start(keeper, collector, registry, &report, now);
            Ok(AuthorAnswer::Done { created: None })
        }
        AuthorCommand::OpenMeeting(key) => {
            let board = outputs.board.current();
            crate::meeting::grant(collector, registry, board.as_ref(), &key)
                .map(|grant| AuthorAnswer::Meeting(Box::new(grant)))
        }
        AuthorCommand::SendAnswer {
            key,
            expected,
            from,
        } => {
            let board = outputs.board.current();
            crate::meeting::grant_answer(
                collector,
                registry,
                board.as_ref(),
                &key,
                expected,
                &from,
                now,
            )
            .map(|grant| AuthorAnswer::AnswerGrant(Box::new(grant)))
        }
        // Answered once the answer settles: at once, or, for a resume,
        // once the session was seen busy or its window passed (AD-85).
        AuthorCommand::AnswerOutcome(report) => {
            let window = collector.confirm_window();
            if let Some(pending) =
                crate::meeting::record_answer(collector, &report, request.reply, window, now)
            {
                confirmations.push(pending);
            }
            return false;
        }
    };
    // Every command reports the registry's state, a failure included
    // (quality audit M3); those that write publish the views at once.
    report_registry(collector, registry);
    if writes {
        publish_both(collector, registry, outputs, true);
    }
    let _ = request.reply.send(reply);
    relay_shows
}

/// Whether the triage model can be called, and the `claude` binary's
/// stamp, from what the collector knows now (AD-51, D-66): the floor's
/// `no_excerpts` can change with a floor edit and `claude` with an update.
fn model_env(collector: &Collector) -> ModelEnv {
    let claude = collector.claude();
    let path = claude.map(|path| path.to_string_lossy().into_owned());
    ModelEnv {
        unavailable: unavailable(path.as_deref(), collector.no_excerpts()),
        claude_stamp: claude.and_then(binary_stamp),
        no_excerpts: collector.no_excerpts(),
    }
}

/// The triage call of `next`, with what the collector knows of the window:
/// its `claude`, home, floor config dir, `no_excerpts` and base
/// environment (AD-51, D-86).
fn triage_job(collector: &Collector, next: NextCall) -> TriageJob {
    TriageJob {
        purpose: next.purpose,
        case_key: next.case_key,
        case: next.case,
        claude: collector.claude().map(Path::to_path_buf),
        officina_home: collector.paths().officina_home.clone(),
        config_dir: collector.paths().claude_config_override.clone(),
        no_excerpts: collector.no_excerpts(),
        base_env: collector.base_env().clone(),
        limits: CallLimits::default(),
    }
}

/// One tick of triage (AD-52): the results back from the `triage` thread
/// are recorded, then the next call starts when the thread is idle and the
/// budget allows; never a wait, so the board keeps its pace while a call
/// runs. The registry view is emitted when the dry run moved.
fn pump_triage<E: Emit>(
    collector: &Collector,
    registry: &mut WindowRegistry,
    worker: &mut TriageWorker,
    outputs: &mut Outputs<E>,
) {
    let before = registry.triage_state();
    let mut recorded = false;
    while let Some(result) = worker.try_result() {
        registry.take_call_result(&result, now_ms());
        recorded = true;
    }
    if worker.idle()
        && !worker.gone()
        && let Some(next) = registry.next_call(now_ms())
    {
        worker.submit(triage_job(collector, next));
    }
    if worker.gone() {
        registry.triage_worker_gone(now_ms());
    }
    if recorded || registry.triage_state() != before {
        publish_both(collector, registry, outputs, false);
    }
}

/// The registry's side of the relay view's `auto` section, reasons
/// included: the window shows a review once the author decided (D-143).
fn auto_read(registry: &mut WindowRegistry, now_ms: i64) -> AutoRead {
    AutoRead {
        state: registry.auto_state(),
        rows: registry
            .review_rows(now_ms - STATS_WINDOW_MS)
            .iter()
            .map(|row| row.auto_row(true))
            .collect(),
    }
}

/// Journals a change of auto approval or its mode (AD-88): the state
/// written, source `author`. A journal that cannot be written loses the
/// line, not the change.
fn journal_auto_state(relay: &RelayHome, state: AutoState, now_ms: i64) {
    let line = JournalEntry::AutoApprovalSet(AutoApprovalSet {
        on: state.approval.on,
        mode: state.mode,
        src: state.approval_src.unwrap_or(Source::Author),
        reason: state.approval.reason,
    });
    let _ = crate::relay::journal::record(relay, &[line], now_ms);
}

/// One tick of the `auto` review (AD-86): new requests prechecked, results
/// recorded, the next call started; never a wait. The board is rebuilt
/// when what it shows of the `auto` requests changed; returns true then,
/// so the relay view is rebuilt too.
fn pump_review<E: Emit>(
    collector: &mut Collector,
    registry: &mut WindowRegistry,
    reviews: &mut Reviews,
    outputs: &mut Outputs<E>,
) -> bool {
    let board = outputs.board.current();
    let paths = collector.paths().clone();
    let env = ReviewEnv {
        relay: collector.relay(),
        officina_home: &paths.officina_home,
        claude: collector.claude(),
        config_dir: paths.claude_config_override.as_deref(),
        base_env: collector.base_env(),
        no_excerpts: collector.no_excerpts(),
        five_hour_used: collector.five_hour_used(),
        board: board.as_ref(),
        timeout: collector.review_timeout(),
    };
    if reviews.tick(&env, registry, now_ms()) {
        collector.set_auto_escalations(reviews.escalations().clone());
        publish_both(collector, registry, outputs, false);
        return true;
    }
    false
}

/// Shares `board` with the window's commands, and emits it when it differs
/// from the last one emitted, generation time aside (AD-5).
fn publish<E: Emit>(board: Board, outputs: &mut Outputs<E>) {
    outputs.next_escalation_at_ms = board.next_escalation_at_ms;
    let mut comparable = board.clone();
    comparable.generated_at_ms = 0;
    if outputs.board.share(&board, &comparable, false) {
        outputs.emit.board(&board);
    }
}

/// Shares `view` with the window's commands, and emits it when its bytes
/// differ from the last one emitted (AD-5, AD-35); true when it did.
fn publish_relay<E: Emit>(view: RelayView, outputs: &mut Outputs<E>) -> bool {
    let changed = outputs.relay.share(&view, &view, false);
    if changed {
        outputs.emit.relay(&view);
    }
    changed
}

/// Builds the Orchestrator view at `now` from the payloads just shared and
/// the registry's orders state (AD-92), shares it with `get_orchestrator`,
/// and emits it when it differs from the last one emitted, generation
/// times aside, or when `force` (AD-5).
fn publish_orchestrator<E: Emit>(
    registry: &mut WindowRegistry,
    outputs: &mut Outputs<E>,
    force: bool,
    now: i64,
) {
    let board = outputs.board.current();
    let relay = outputs.relay.current();
    let view = outputs.registry.current();
    let orchestrator =
        registry.orchestrator_view(board.as_ref(), relay.as_ref(), view.as_ref(), now);
    let mut comparable = orchestrator.clone();
    comparable.generated_at_ms = 0;
    comparable.briefing.generated_at_ms = 0;
    if outputs
        .orchestrator
        .share(&orchestrator, &comparable, force)
    {
        outputs.emit.orchestrator(&orchestrator);
    }
}

/// Shares `view` with `get_registry`, and emits it when it differs from the
/// last one emitted, generation time aside, or when `force` (AD-54).
fn publish_registry<E: Emit>(view: RegistryView, outputs: &mut Outputs<E>, force: bool) {
    let mut comparable = view.clone();
    comparable.generated_at_ms = 0;
    if outputs.registry.share(&view, &comparable, force) {
        outputs.emit.registry(&view);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    /// A plan on a clock that moves only when the test says so.
    fn plan_on_fake_clock() -> (RefreshPlan<impl Fn() -> Instant>, Rc<Cell<Instant>>) {
        let now = Rc::new(Cell::new(Instant::now()));
        let clock = Rc::clone(&now);
        (RefreshPlan::new(move || clock.get()), now)
    }

    /// The same, with the startup spawn done at the clock's time zero.
    fn started() -> (RefreshPlan<impl Fn() -> Instant>, Rc<Cell<Instant>>) {
        let (mut plan, now) = plan_on_fake_clock();
        assert!(plan.due().agents);
        (plan, now)
    }

    fn advance(now: &Cell<Instant>, by: Duration) {
        now.set(now.get() + by);
    }

    fn agents_and_board() -> Due {
        Due {
            ingest: true,
            agents: true,
            recompute: true,
            ..Due::default()
        }
    }

    #[test]
    fn startup_lists_sessions_checks_the_contract_and_builds_a_board() {
        let (mut plan, now) = plan_on_fake_clock();
        assert_eq!(
            plan.due(),
            Due {
                contract: true,
                relay: true,
                credentials: true,
                proposals: true,
                ..agents_and_board()
            }
        );
        advance(&now, TICK);
        assert_eq!(plan.due(), Due::default());
    }

    #[test]
    fn proposal_files_are_ingested_at_start_on_change_and_with_their_poll() {
        let (mut plan, now) = plan_on_fake_clock();
        assert!(plan.due().proposals, "at start");
        advance(&now, TICK);
        assert!(!plan.due().proposals);
        plan.proposals_changed();
        assert_eq!(
            plan.due(),
            Due {
                proposals: true,
                ..Due::default()
            },
            "no spawn, no board: the view is published when a row changed"
        );
        advance(&now, PROPOSALS_POLL);
        assert!(plan.due().proposals, "expiry without a file change");
    }

    #[test]
    fn relay_changes_rebuild_the_relay_view_alone_and_time_does_too() {
        let (mut plan, now) = started();
        advance(&now, TICK);
        plan.relay_changed();
        assert_eq!(
            plan.due(),
            Due {
                relay: true,
                ..Due::default()
            },
            "no spawn, no board"
        );
        advance(&now, TICK);
        assert_eq!(plan.due(), Due::default());
        advance(&now, RELAY_POLL);
        assert!(
            plan.due().relay,
            "abandoned runs show without a file change"
        );
    }

    /// D-41: a floor or relay change checks the credentials with the view;
    /// a journal append or the view's own poll does not, but the check comes
    /// back once a minute.
    #[test]
    fn credentials_are_checked_on_relay_changes_and_at_most_once_a_minute() {
        let (mut plan, now) = started();
        advance(&now, TICK);
        plan.relay_changed();
        let due = plan.due();
        assert!(due.relay && !due.credentials, "a journal append");
        advance(&now, RELAY_POLL);
        let due = plan.due();
        assert!(
            due.relay && !due.credentials,
            "the view's poll, within the minute"
        );
        advance(&now, TICK);
        plan.relay_files_changed();
        assert!(plan.due().credentials, "a relay file change");
        advance(&now, TICK);
        plan.relay_floor_changed();
        assert!(plan.due().credentials, "a relay floor change");
        advance(&now, TICK);
        plan.floor_changed();
        assert!(plan.due().credentials, "a window floor change");

        advance(&now, TICK);
        plan.relay_changed();
        assert!(!plan.due().credentials);
        let mut checked_at = Vec::new();
        for _ in 0..(4 * CREDENTIAL_POLL.as_secs() / RELAY_POLL.as_secs()) {
            advance(&now, RELAY_POLL);
            if plan.due().credentials {
                checked_at.push(now.get());
            }
        }
        assert!(!checked_at.is_empty(), "the minute's poll");
        assert!(
            checked_at
                .windows(2)
                .all(|pair| pair[1].duration_since(pair[0]) >= CREDENTIAL_POLL),
            "at most once a minute"
        );
    }

    #[test]
    fn floor_change_rebuilds_everything() {
        let (mut plan, now) = started();
        advance(&now, Duration::from_secs(2));
        plan.floor_changed();
        assert_eq!(
            plan.due(),
            Due {
                relay: true,
                credentials: true,
                ..agents_and_board()
            }
        );
    }

    /// D-92: a request within the headroom of its deadline is dropped, so
    /// its action, views and reply fit before the window stops waiting.
    #[test]
    fn a_request_within_the_headroom_of_its_deadline_is_expired() {
        let (request, _answer) = AuthorRequest::new(AuthorCommand::ListArchived, AUTHOR_WAIT);
        let sent = request.deadline - AUTHOR_WAIT;
        assert!(!request.expired(sent));
        let last_chance = request.deadline - AUTHOR_HEADROOM - Duration::from_millis(1);
        assert!(!request.expired(last_chance));
        assert!(request.expired(request.deadline - AUTHOR_HEADROOM));
        assert!(request.expired(request.deadline));
    }

    #[test]
    fn relay_tree_changes_change_its_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let first = relay_fingerprint(dir.path());
        std::fs::write(dir.path().join("stopped"), "{}").unwrap();
        let stopped = relay_fingerprint(dir.path());
        assert_ne!(first, stopped);
        std::fs::create_dir_all(dir.path().join("approvals")).unwrap();
        std::fs::write(dir.path().join("approvals").join("a.request.json"), "{}").unwrap();
        assert_ne!(stopped, relay_fingerprint(dir.path()), "approvals/ counts");
    }

    #[test]
    fn hook_event_shows_at_once_and_refreshes_now_and_after_the_follow_up() {
        let (mut plan, now) = started();
        advance(&now, Duration::from_secs(5));
        plan.event_file_changed();
        assert_eq!(plan.due(), agents_and_board());
        advance(&now, FOLLOW_UP - TICK);
        assert_eq!(plan.due(), Due::default());
        advance(&now, TICK);
        assert_eq!(plan.due(), agents_and_board(), "the follow-up");
        advance(&now, TICK);
        assert_eq!(plan.due(), Due::default());
    }

    #[test]
    fn a_later_event_postpones_the_follow_up() {
        let (mut plan, now) = started();
        advance(&now, Duration::from_secs(5));
        plan.event_file_changed();
        plan.due();
        advance(&now, FOLLOW_UP / 2);
        plan.event_file_changed();
        assert!(plan.due().agents);
        advance(&now, FOLLOW_UP / 2);
        assert!(!plan.due().agents, "the first follow-up was replaced");
        advance(&now, FOLLOW_UP / 2);
        assert!(plan.due().agents);
    }

    #[test]
    fn spawns_are_rate_limited_and_a_burst_coalesces_into_one() {
        let (mut plan, now) = started();
        advance(&now, TICK / 2);
        plan.session_files_changed();
        plan.refresh_requested();
        plan.event_file_changed();
        assert_eq!(
            plan.due(),
            Due {
                ingest: true,
                recompute: true,
                ..Due::default()
            },
            "the event is ingested and shows, the spawn waits"
        );
        advance(&now, TICK / 2);
        plan.session_files_changed();
        assert_eq!(plan.due(), Due::default());
        advance(&now, MIN_SPAWN_INTERVAL - TICK);
        assert_eq!(plan.due(), agents_and_board(), "one spawn for the burst");
        advance(&now, TICK);
        assert_eq!(plan.due(), Due::default());
    }

    /// D-78: every session list is preceded by an ingestion pass, and a
    /// hook event is ingested before the board shows it, spawn or not.
    #[test]
    fn ingestion_comes_with_every_session_list_and_every_event() {
        let (mut plan, now) = started();
        advance(&now, POLL_VISIBLE);
        let polled = plan.due();
        assert!(polled.agents && polled.ingest, "the safety poll");
        advance(&now, TICK);
        plan.event_file_changed();
        let event = plan.due();
        assert!(
            event.ingest && event.recompute && !event.agents,
            "the spawn waits, the event does not"
        );
        advance(&now, MIN_SPAWN_INTERVAL);
        let delayed = plan.due();
        assert!(
            delayed.agents && delayed.ingest,
            "the delayed spawn ingests again"
        );
        advance(&now, TICK);
        plan.quota_changed();
        assert!(!plan.due().ingest, "a quota change reads no event");
    }

    /// D-160, D-881: while an answer is watched, a session list is due
    /// every `MIN_SPAWN_INTERVAL` of the plan's clock, so a turn of under
    /// two seconds falls on several listings; a machine whose listing
    /// takes longer spaces them by that time instead (`watch.rs`'s
    /// `a_two_second_turn_is_confirmed` rests on their order alone).
    #[test]
    fn a_watched_answer_lists_sessions_every_min_spawn_interval() {
        let (mut plan, now) = started();
        advance(&now, Duration::from_secs(5));
        let turn = Duration::from_secs(2);
        let from = now.get();
        let mut listed = Vec::new();
        while now.get() - from < turn {
            plan.answer_watched();
            if plan.due().agents {
                listed.push(now.get() - from);
            }
            advance(&now, TICK);
        }
        let every_interval: Vec<Duration> = (0..)
            .map(|n| MIN_SPAWN_INTERVAL * n)
            .take_while(|at| *at < turn)
            .collect();
        assert_eq!(listed, every_interval);
    }

    #[test]
    fn manual_refresh_spawns_at_the_next_tick() {
        let (mut plan, now) = started();
        advance(&now, Duration::from_secs(3));
        plan.refresh_requested();
        assert_eq!(plan.due(), agents_and_board());
    }

    #[test]
    fn safety_poll_slows_down_while_the_window_is_hidden() {
        let (mut plan, now) = started();
        advance(&now, POLL_VISIBLE - TICK);
        assert!(!plan.due().agents);
        advance(&now, TICK);
        assert!(plan.due().agents);

        plan.visibility_changed(false);
        advance(&now, POLL_VISIBLE);
        assert!(!plan.due().agents);
        advance(&now, POLL_HIDDEN - POLL_VISIBLE - TICK);
        assert!(!plan.due().agents);
        advance(&now, TICK);
        assert!(plan.due().agents);

        plan.visibility_changed(true);
        advance(&now, POLL_VISIBLE);
        assert!(plan.due().agents);
    }

    #[test]
    fn quota_change_rebuilds_the_board_without_a_spawn() {
        let (mut plan, now) = started();
        advance(&now, TICK);
        plan.quota_changed();
        assert_eq!(
            plan.due(),
            Due {
                recompute: true,
                ..Due::default()
            }
        );
    }

    #[test]
    fn binary_change_checks_the_contract_with_the_next_spawn_only() {
        let (mut plan, now) = started();
        advance(&now, Duration::from_secs(1));
        plan.claude_binary_changed();
        assert_eq!(plan.due(), Due::default(), "no spawn of its own");
        plan.session_files_changed();
        assert_eq!(
            plan.due(),
            Due {
                contract: true,
                ..agents_and_board()
            }
        );
        advance(&now, Duration::from_secs(1));
        plan.session_files_changed();
        assert_eq!(plan.due(), agents_and_board(), "checked once");
    }

    /// Security audit L4: `no_excerpts` turned on brings the pass forward
    /// to the next tick, once, without moving the hourly one.
    #[test]
    fn excerpts_turned_off_runs_the_pass_at_once() {
        let (mut plan, now) = started();
        advance(&now, TICK);
        assert!(!plan.due().retention);
        plan.excerpts_turned_off();
        assert!(plan.due().retention);
        assert!(!plan.due().retention, "once");
        advance(&now, RETENTION_EVERY - TICK);
        assert!(!plan.due().retention, "the hour counts from the pass run");
        advance(&now, TICK);
        assert!(plan.due().retention);
    }

    /// Quality audit M6: each command takes its own answer and reports any
    /// other as an internal fault.
    #[test]
    fn an_answer_for_another_request_is_an_internal_error() {
        use officina_core::ipc::IpcErrorCode;
        assert_eq!(
            AuthorAnswer::Done { created: Some(4) }.into_done(),
            Ok(Some(4))
        );
        assert_eq!(
            AuthorAnswer::Evaluating { cases: 3 }.into_evaluating(),
            Ok(3)
        );
        let wrong = AuthorAnswer::Evaluating { cases: 3 }
            .into_done()
            .unwrap_err();
        assert_eq!(wrong.code, IpcErrorCode::Internal);
        assert!(
            AuthorAnswer::Done { created: None }
                .into_archived()
                .is_err()
        );
        assert!(
            AuthorAnswer::Done { created: None }
                .into_confirmed()
                .is_err()
        );
    }

    #[test]
    fn retention_runs_hourly() {
        let (mut plan, now) = started();
        advance(&now, RETENTION_EVERY - TICK);
        assert!(!plan.due().retention);
        advance(&now, TICK);
        assert!(plan.due().retention);
        advance(&now, TICK);
        assert!(!plan.due().retention);
        advance(&now, RETENTION_EVERY);
        assert!(plan.due().retention);
    }

    #[test]
    fn session_file_rewrites_change_the_fingerprint_and_key_files_do_not() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("100.json"), "{}").unwrap();
        let first = sessions_fingerprint(dir.path());
        assert_eq!(first, sessions_fingerprint(dir.path()));

        std::fs::write(dir.path().join("100.abc.key"), "secret").unwrap();
        assert_eq!(
            first,
            sessions_fingerprint(dir.path()),
            ".key files are ignored"
        );

        std::fs::write(dir.path().join("100.json"), "{\"status\":\"idle\"}").unwrap();
        let rewritten = sessions_fingerprint(dir.path());
        assert_ne!(first, rewritten, "a state change rewrites the session file");

        std::fs::write(dir.path().join("200.json"), "{}").unwrap();
        assert_ne!(rewritten, sessions_fingerprint(dir.path()));
        assert_eq!(
            sessions_fingerprint(&dir.path().join("missing")),
            sessions_fingerprint(&dir.path().join("missing2"))
        );
    }
}
