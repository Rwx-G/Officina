//! The collector thread of the window (AD-3 to AD-5): stat polling, refresh
//! plan, diff before emit.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};

use officina_core::board::Board;

use crate::collector::Collector;
use crate::files::{event_file_for, now_ms, retention_pass};

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
/// Retention pass interval.
pub const RETENTION_EVERY: Duration = Duration::from_secs(3600);

/// Messages to the collector thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectorMsg {
    Refresh,
    Visibility(bool),
    Shutdown,
}

/// Handle on a running collector thread.
pub struct WatchHandle {
    pub sender: Sender<CollectorMsg>,
    pub board: Arc<RwLock<Option<Board>>>,
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

/// Fingerprint of the session files in the Claude sessions dir: name, size and
/// modification time of each `*.json`, never their content. Claude Code
/// rewrites a session's file on every state change (E10) while the directory
/// itself only changes when a session appears or ends, so the directory's own
/// date misses most transitions. Other files (the `.key` files) are not even
/// stat'ed.
fn sessions_fingerprint(dir: &std::path::Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut entries: Vec<(String, u64, Option<SystemTime>)> = std::fs::read_dir(dir)
        .map(|it| {
            it.flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if !name.ends_with(".json") {
                        return None;
                    }
                    let meta = e.metadata().ok()?;
                    Some((name, meta.len(), meta.modified().ok()))
                })
                .collect()
        })
        .unwrap_or_default();
    entries.sort();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    entries.hash(&mut hasher);
    hasher.finish()
}

/// What the refresh plan asks the collector thread to do now.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Due {
    /// Run `claude agents`.
    agents: bool,
    /// Then re-read the `claude` version and re-check the contract.
    contract: bool,
    /// Run the retention pass.
    retention: bool,
    /// Rebuild the board, and emit it if it changed.
    recompute: bool,
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
    agents_wanted: bool,
    contract_wanted: bool,
    recompute_wanted: bool,
    last_spawn: Option<Instant>,
    follow_up_at: Option<Instant>,
    last_retention: Instant,
}

impl<C: Fn() -> Instant> RefreshPlan<C> {
    /// At startup the session list, the contract check and a first board are
    /// due; the retention pass has just run.
    fn new(clock: C) -> Self {
        let now = clock();
        RefreshPlan {
            clock,
            visible: true,
            agents_wanted: true,
            contract_wanted: true,
            recompute_wanted: true,
            last_spawn: None,
            follow_up_at: None,
            last_retention: now,
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

    /// Today's event file grew: a hook fired. Its line shows at once; the
    /// session list is refreshed now and once more after [`FOLLOW_UP`].
    fn event_file_changed(&mut self) {
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
        due.recompute = std::mem::take(&mut self.recompute_wanted);
        due
    }
}

/// The paths the collector stats each tick (AD-4) and what they looked like
/// last time.
struct WatchedPaths {
    events_dir: PathBuf,
    sessions_dir: PathBuf,
    quota_file: PathBuf,
    claude: Option<PathBuf>,
    event_file: Stamp,
    sessions: u64,
    quota: Stamp,
    claude_binary: Option<Stamp>,
}

impl WatchedPaths {
    fn new(collector: &Collector) -> WatchedPaths {
        let events_dir = collector.paths.events_dir();
        let sessions_dir = collector.paths.claude_sessions_dir();
        let quota_file = collector.paths.quota_file();
        let claude = collector.claude.clone();
        WatchedPaths {
            event_file: stamp(&event_file_for(&events_dir, now_ms())),
            sessions: sessions_fingerprint(&sessions_dir),
            quota: stamp(&quota_file),
            claude_binary: claude.as_deref().map(stamp),
            events_dir,
            sessions_dir,
            quota_file,
            claude,
        }
    }

    /// Stats every watched path and tells `plan` what changed.
    fn poll(&mut self, plan: &mut RefreshPlan<impl Fn() -> Instant>) {
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
    }
}

/// Starts the collector thread. `emit` is called with each board that differs
/// from the previous one, from the collector thread.
pub fn spawn(
    mut collector: Collector,
    emit: impl Fn(&Board) + Send + 'static,
) -> std::io::Result<WatchHandle> {
    let (sender, receiver) = mpsc::channel();
    let board = Arc::new(RwLock::new(None));
    let shared = Arc::clone(&board);
    let thread = std::thread::Builder::new()
        .name("collector".into())
        .spawn(move || run(&mut collector, &receiver, &shared, &emit))?;
    Ok(WatchHandle {
        sender,
        board,
        thread,
    })
}

fn run(
    collector: &mut Collector,
    receiver: &Receiver<CollectorMsg>,
    shared: &Arc<RwLock<Option<Board>>>,
    emit: &dyn Fn(&Board),
) {
    let retention = |collector: &Collector| {
        retention_pass(&collector.paths.events_dir(), now_ms());
        crate::focus::prune_focus_log(&collector.paths.state_dir(), now_ms());
    };
    retention(collector);
    let mut watched = WatchedPaths::new(collector);
    let mut plan = RefreshPlan::new(Instant::now);
    let mut last_bytes: Vec<u8> = Vec::new();

    loop {
        match receiver.recv_timeout(TICK) {
            Ok(CollectorMsg::Shutdown) | Err(RecvTimeoutError::Disconnected) => return,
            Ok(CollectorMsg::Refresh) => plan.refresh_requested(),
            Ok(CollectorMsg::Visibility(visible)) => plan.visibility_changed(visible),
            Err(RecvTimeoutError::Timeout) => {}
        }
        watched.poll(&mut plan);
        let due = plan.due();
        if due.agents {
            collector.refresh_agents(now_ms());
            if due.contract {
                collector.refresh_contract(now_ms());
            }
        }
        if due.retention {
            retention(collector);
        }
        if due.recompute {
            publish(collector.board(now_ms()), shared, emit, &mut last_bytes);
        }
    }
}

/// Shares `board` with the window's commands, and emits it when it differs
/// from the last one emitted, generation time aside (AD-5).
fn publish(
    board: Board,
    shared: &Arc<RwLock<Option<Board>>>,
    emit: &dyn Fn(&Board),
    last_bytes: &mut Vec<u8>,
) {
    let mut comparable = board.clone();
    comparable.generated_at_ms = 0;
    let bytes = serde_json::to_vec(&comparable).unwrap_or_default();
    if let Ok(mut slot) = shared.write() {
        *slot = Some(board.clone());
    }
    if bytes != *last_bytes {
        *last_bytes = bytes;
        emit(&board);
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
                ..agents_and_board()
            }
        );
        advance(&now, TICK);
        assert_eq!(plan.due(), Due::default());
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
                recompute: true,
                ..Due::default()
            },
            "the event shows, the spawn waits"
        );
        advance(&now, TICK / 2);
        plan.session_files_changed();
        assert_eq!(plan.due(), Due::default());
        advance(&now, MIN_SPAWN_INTERVAL - TICK);
        assert_eq!(plan.due(), agents_and_board(), "one spawn for the burst");
        advance(&now, TICK);
        assert_eq!(plan.due(), Due::default());
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
