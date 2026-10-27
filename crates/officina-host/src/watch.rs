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

struct Watched {
    events_dir: PathBuf,
    sessions_dir: PathBuf,
    quota_file: PathBuf,
    claude: Option<PathBuf>,
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
    let watched = Watched {
        events_dir: collector.paths.events_dir(),
        sessions_dir: collector.paths.claude_sessions_dir(),
        quota_file: collector.paths.quota_file(),
        claude: collector.claude.clone(),
    };
    let mut visible = true;
    let mut last_spawn: Option<Instant> = None;
    let mut agents_due = Some(Instant::now());
    let mut follow_up_due: Option<Instant> = None;
    let mut recompute = true;
    let mut contract_due = true;
    let mut last_retention = Instant::now();
    let mut last_bytes: Vec<u8> = Vec::new();

    retention_pass(&watched.events_dir, now_ms());
    crate::focus::prune_focus_log(&collector.paths.state_dir(), now_ms());
    let mut event_stamp = stamp(&event_file_for(&watched.events_dir, now_ms()));
    let mut sessions_stamp = sessions_fingerprint(&watched.sessions_dir);
    let mut quota_stamp = stamp(&watched.quota_file);
    let mut claude_stamp = watched.claude.as_deref().map(stamp);

    loop {
        match receiver.recv_timeout(TICK) {
            Ok(CollectorMsg::Shutdown) | Err(RecvTimeoutError::Disconnected) => return,
            Ok(CollectorMsg::Refresh) => agents_due = Some(Instant::now()),
            Ok(CollectorMsg::Visibility(v)) => visible = v,
            Err(RecvTimeoutError::Timeout) => {}
        }
        let now = Instant::now();

        let current = stamp(&event_file_for(&watched.events_dir, now_ms()));
        if current != event_stamp {
            event_stamp = current;
            recompute = true;
            agents_due = Some(now);
            follow_up_due = Some(now + FOLLOW_UP);
        }
        let current = sessions_fingerprint(&watched.sessions_dir);
        if current != sessions_stamp {
            sessions_stamp = current;
            agents_due = Some(now);
        }
        let current = stamp(&watched.quota_file);
        if current != quota_stamp {
            quota_stamp = current;
            recompute = true;
        }
        let current = watched.claude.as_deref().map(stamp);
        if current != claude_stamp {
            claude_stamp = current;
            contract_due = true;
        }
        if follow_up_due.is_some_and(|d| now >= d) {
            follow_up_due = None;
            agents_due = Some(now);
        }
        let poll = if visible { POLL_VISIBLE } else { POLL_HIDDEN };
        if last_spawn.is_none_or(|t| now.duration_since(t) >= poll) && agents_due.is_none() {
            agents_due = Some(now);
        }

        let spawn_allowed = last_spawn.is_none_or(|t| now.duration_since(t) >= MIN_SPAWN_INTERVAL);
        if agents_due.is_some_and(|d| now >= d) && spawn_allowed {
            agents_due = None;
            last_spawn = Some(now);
            collector.refresh_agents(now_ms());
            recompute = true;
            if contract_due {
                contract_due = false;
                collector.refresh_contract(now_ms());
            }
        }
        if now.duration_since(last_retention) >= RETENTION_EVERY {
            last_retention = now;
            retention_pass(&watched.events_dir, now_ms());
            crate::focus::prune_focus_log(&collector.paths.state_dir(), now_ms());
        }

        if recompute {
            recompute = false;
            let board = collector.board(now_ms());
            let mut comparable = board.clone();
            comparable.generated_at_ms = 0;
            let bytes = serde_json::to_vec(&comparable).unwrap_or_default();
            if let Ok(mut slot) = shared.write() {
                *slot = Some(board.clone());
            }
            if bytes != last_bytes {
                last_bytes = bytes;
                emit(&board);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
