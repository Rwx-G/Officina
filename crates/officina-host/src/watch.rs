//! The collector thread of the window (AD-3 to AD-5, AD-35, AD-43, AD-44):
//! stat polling, refresh plan, diff before emit, for the board, the relay
//! view and the registry view; the registry's only writer (AD-38), which
//! ingests the event files before anything reads or deletes them, and runs
//! the author's actions the window's commands send it (D-92).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};

use officina_core::board::Board;
use officina_core::dates::event_file_name;
use officina_core::ipc::IpcError;
use officina_core::registry::confirm::{AuthorAction, ConfirmItem, ItemAnswer};
use officina_core::registry::retention::PASS_INTERVAL_MS;
use officina_core::registry::view::{RegistryView, SubjectView};
use officina_core::relay::view::RelayView;

use crate::collector::Collector;
use crate::files::{event_file_for, now_ms};
use crate::paths::Paths;
use crate::registry::ingest::IngestContext;
use crate::registry::store::onedrive_roots_from_env;
use crate::registry::window::WindowRegistry;
use crate::relay::home::RelayHome;
use crate::relay::view::{credential_presence, read_view_with};

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
/// How long a window command waits for the collector's answer to an author
/// request (AD-38): a running `claude agents` takes about 210 ms, 5 s at
/// worst (E10).
pub const AUTHOR_WAIT: Duration = Duration::from_secs(10);
/// A request this close to its deadline is dropped unrun (D-92): the
/// action, the views it emits and the reply must all land before the window
/// stops waiting, so the window never reports a timeout for an action that
/// happened.
pub const AUTHOR_HEADROOM: Duration = Duration::from_secs(1);

/// What an author request asks of the registry (AD-54).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorCommand {
    /// The archived subjects, read on the collector's connection, so the
    /// window holds one connection.
    ListArchived,
    /// An action on subjects and sessions, written in one transaction.
    Act(AuthorAction),
    /// Triage rows confirmed in one transaction, answered per item (AD-49).
    Confirm(Vec<ConfirmItem>),
}

/// What the collector answers to an author request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorAnswer {
    /// The action was written; `created` is the subject it created, if any.
    Done {
        created: Option<i64>,
    },
    Archived(Vec<SubjectView>),
    /// What became of each confirmed item, in the order sent.
    Confirmed(Vec<ItemAnswer>),
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

    /// True when the collector must drop the request unrun (D-92).
    fn expired(&self, now: Instant) -> bool {
        now + AUTHOR_HEADROOM >= self.deadline
    }
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
/// one it was given, and `registry` also right after each author action.
pub trait Emit: Send + 'static {
    fn board(&self, board: &Board);
    fn relay(&self, view: &RelayView);
    fn registry(&self, view: &RegistryView);
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
    last_spawn: Option<Instant>,
    follow_up_at: Option<Instant>,
    last_retention: Instant,
    last_relay: Instant,
    last_credentials: Instant,
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
            last_spawn: None,
            follow_up_at: None,
            last_retention: now,
            last_relay: now,
            last_credentials: now,
        }
    }

    /// `CollectorMsg::Refresh`.
    fn refresh_requested(&mut self) {
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

    /// The window's floor changed and was reloaded: its projects reach a new
    /// board, its config dir may name other sessions, and the relay view may
    /// read it too.
    fn floor_changed(&mut self) {
        self.recompute_wanted = true;
        self.agents_wanted = true;
        self.relay_wanted = true;
        self.credentials_wanted = true;
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
        if now.duration_since(self.last_retention) >= RETENTION_EVERY {
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
    journal_file: Stamp,
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
        WatchedPaths {
            event_file: stamp(&event_file_for(&events_dir, now_ms())),
            sessions: sessions_fingerprint(&sessions_dir),
            quota: stamp(&quota_file),
            claude_binary: claude.as_deref().map(stamp),
            floor: stamp(&floor_file),
            relay_tree: relay_fingerprint(&relay.relay_dir()),
            relay_floor: stamp(&relay_floor_file),
            journal_file: stamp(&today_journal(&relay)),
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
        let current = stamp(&today_journal(&self.relay));
        if current != self.journal_file {
            self.journal_file = current;
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

/// Today's journal file (AD-32), the one appends go to.
fn today_journal(relay: &RelayHome) -> PathBuf {
    relay.journal_dir().join(event_file_name(now_ms()))
}

/// The shared slots of one collector thread, where it emits, and the bytes
/// it last emitted.
struct Outputs<E> {
    board: Arc<RwLock<Option<Board>>>,
    relay: Arc<RwLock<Option<RelayView>>>,
    registry: Arc<RwLock<Option<RegistryView>>>,
    paths: Arc<RwLock<Paths>>,
    emit: E,
    last_board: Vec<u8>,
    last_relay: Vec<u8>,
    last_registry: Vec<u8>,
}

/// Starts the collector thread. `emit` receives each board, relay view and
/// registry view that differs from the previous one, from the collector
/// thread (AD-5).
pub fn spawn(mut collector: Collector, emit: impl Emit) -> std::io::Result<WatchHandle> {
    let (sender, receiver) = mpsc::channel();
    let board = Arc::new(RwLock::new(None));
    let relay = Arc::new(RwLock::new(None));
    let registry = Arc::new(RwLock::new(None));
    let paths = Arc::new(RwLock::new(collector.paths().clone()));
    let mut outputs = Outputs {
        board: Arc::clone(&board),
        relay: Arc::clone(&relay),
        registry: Arc::clone(&registry),
        paths: Arc::clone(&paths),
        emit,
        last_board: Vec::new(),
        last_relay: Vec::new(),
        last_registry: Vec::new(),
    };
    let thread = std::thread::Builder::new()
        .name("collector".into())
        .spawn(move || run(&mut collector, &receiver, &mut outputs))?;
    Ok(WatchHandle {
        sender,
        board,
        relay,
        registry,
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
fn report_registry(collector: &mut Collector, registry: &WindowRegistry) {
    collector.set_registry(registry.report());
    collector.set_subjects(registry.card_subjects());
}

/// Folds the session list the collector just received into the window's
/// registry (AD-46), after the tick's ingestion (D-78).
fn apply_agents(collector: &mut Collector, registry: &mut WindowRegistry, now_ms: i64) {
    let records = collector.agents().unwrap_or_default();
    registry.apply_agents(records, &collector.floor().projects, now_ms);
    report_registry(collector, registry);
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
    report_registry(collector, registry);
}

fn run<E: Emit>(
    collector: &mut Collector,
    receiver: &Receiver<CollectorMsg>,
    outputs: &mut Outputs<E>,
) {
    // At start, every incomplete file is ingested and the privacy bounds
    // applied before the first board is built (D-85).
    let mut registry = WindowRegistry::open(
        &collector.paths().officina_home,
        &onedrive_roots_from_env(),
        now_ms(),
    );
    hourly_pass(collector, &mut registry);
    let mut watched = WatchedPaths::new(collector);
    let mut plan = RefreshPlan::new(Instant::now);
    let mut presence: BTreeMap<String, bool> = BTreeMap::new();

    loop {
        match receiver.recv_timeout(TICK) {
            Ok(CollectorMsg::Shutdown) | Err(RecvTimeoutError::Disconnected) => return,
            Ok(CollectorMsg::Refresh) => plan.refresh_requested(),
            Ok(CollectorMsg::Visibility(visible)) => plan.visibility_changed(visible),
            Ok(CollectorMsg::RelayChanged) => plan.relay_files_changed(),
            Ok(CollectorMsg::Author(request)) => {
                answer_author(collector, &mut registry, request, outputs);
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        if watched.poll(&mut plan) {
            collector.reload_floor();
            if let Ok(mut paths) = outputs.paths.write() {
                paths.clone_from(collector.paths());
            }
            watched = WatchedPaths::new(collector);
            plan.floor_changed();
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
            if due.contract {
                collector.refresh_contract(now_ms());
            }
        }
        if due.retention {
            hourly_pass(collector, &mut registry);
        }
        // The hourly pass can clear excerpts the registry view shows.
        if due.recompute || due.retention {
            publish_both(collector, &mut registry, outputs, false);
        }
        if due.relay {
            collector.keep_known_projects(now_ms());
            if due.credentials {
                presence = credential_presence(collector.relay(), collector.credential_store());
            }
            // A home that cannot be read keeps the last view.
            if let Ok(view) = read_view_with(collector.relay(), now_ms(), Some(&presence)) {
                publish_relay(view, outputs);
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
    let board = collector.board(now_ms());
    let view = registry.view(Some(&board), &collector.paths().events_dir(), now_ms());
    publish(board, outputs);
    publish_registry(view, outputs, force);
}

/// Runs one author request on the registry's writer (AD-38), unless its
/// deadline is too close (D-92): an action is written, the cards' subjects
/// refreshed, and the board and the registry view emitted before the reply.
fn answer_author<E: Emit>(
    collector: &mut Collector,
    registry: &mut WindowRegistry,
    request: AuthorRequest,
    outputs: &mut Outputs<E>,
) {
    if request.expired(Instant::now()) {
        return;
    }
    let reply = match request.command {
        AuthorCommand::ListArchived => {
            let board = outputs.board.read().ok().and_then(|slot| slot.clone());
            registry
                .archived(board.as_ref())
                .map(AuthorAnswer::Archived)
        }
        AuthorCommand::Act(action) => {
            let done = registry.act(&action, now_ms());
            report_registry(collector, registry);
            publish_both(collector, registry, outputs, true);
            done.map(|created| AuthorAnswer::Done { created })
        }
        AuthorCommand::Confirm(items) => {
            let answered = registry.confirm(&items, now_ms());
            report_registry(collector, registry);
            publish_both(collector, registry, outputs, true);
            answered.map(AuthorAnswer::Confirmed)
        }
    };
    let _ = request.reply.send(reply);
}

/// Shares `board` with the window's commands, and emits it when it differs
/// from the last one emitted, generation time aside (AD-5).
fn publish<E: Emit>(board: Board, outputs: &mut Outputs<E>) {
    let mut comparable = board.clone();
    comparable.generated_at_ms = 0;
    let bytes = serde_json::to_vec(&comparable).unwrap_or_default();
    if let Ok(mut slot) = outputs.board.write() {
        *slot = Some(board.clone());
    }
    if bytes != outputs.last_board {
        outputs.last_board = bytes;
        outputs.emit.board(&board);
    }
}

/// Shares `view` with the window's commands, and emits it when its bytes
/// differ from the last one emitted (AD-5, AD-35).
fn publish_relay<E: Emit>(view: RelayView, outputs: &mut Outputs<E>) {
    let bytes = serde_json::to_vec(&view).unwrap_or_default();
    if let Ok(mut slot) = outputs.relay.write() {
        *slot = Some(view.clone());
    }
    if bytes != outputs.last_relay {
        outputs.last_relay = bytes;
        outputs.emit.relay(&view);
    }
}

/// Shares `view` with `get_registry`, and emits it when it differs from the
/// last one emitted, generation time aside, or when `force` (AD-54).
fn publish_registry<E: Emit>(view: RegistryView, outputs: &mut Outputs<E>, force: bool) {
    let mut comparable = view.clone();
    comparable.generated_at_ms = 0;
    let bytes = serde_json::to_vec(&comparable).unwrap_or_default();
    if let Ok(mut slot) = outputs.registry.write() {
        *slot = Some(view.clone());
    }
    if force || bytes != outputs.last_registry {
        outputs.last_registry = bytes;
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
                ..agents_and_board()
            }
        );
        advance(&now, TICK);
        assert_eq!(plan.due(), Due::default());
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
