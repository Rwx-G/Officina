//! The collector thread and the scene's alert (story 4.3 AC 7; AD-168,
//! D-467, D-494): a wait reaching the floor's threshold is emitted without
//! a `claude` spawn, nothing is rebuilt when no wait can escalate, and a
//! failed snapshot keeps the last known alert. The collector runs a copy of
//! the fake claude configured by its sidecar (D-86), whose log counts the
//! spawns.

mod common;
#[path = "../../../tests/support/fake_store.rs"]
mod fake_store;

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use officina_core::board::Board;
use officina_core::scene::{AlertCause, Zone};
use officina_host::collector::{Collector, load_floor};
use officina_host::paths::Paths;
use officina_host::relay::credential::OwnedStore;
use officina_host::watch::{
    CollectorMsg, FOLLOW_UP, MIN_SPAWN_INTERVAL, POLL_VISIBLE, WatchHandle, spawn,
};

use fake_store::FakeStore;

const SID: &str = "00000000-0000-0000-0000-000000000001";

/// How long the startup's spawns and their follow-up take to pass.
const SETTLE: Duration = FOLLOW_UP.saturating_add(MIN_SPAWN_INTERVAL);

struct Watched {
    handle: WatchHandle,
    boards: Receiver<Board>,
    sidecar: PathBuf,
    log: PathBuf,
}

/// A collector on `env` with the floor `floor_text`, one interactive session
/// waiting on a permission prompt in the floor's project `api`, and the
/// blocking notification at `notified_ms` when given.
fn watch(env: &common::TestEnv, floor_text: &str, notified_ms: Option<i64>) -> Watched {
    let api = env.root.path().join("work").join("api");
    std::fs::create_dir_all(&api).unwrap();
    let floor = env.root.path().join("work").join("officina.toml");
    std::fs::write(&floor, floor_text).unwrap();
    let now = common::now_ms();
    env.write_agents(
        &serde_json::json!([{"pid": 4100, "cwd": api.to_string_lossy().replace('\\', "/"),
            "kind": "interactive", "startedAt": now - 600_000, "sessionId": SID,
            "status": "waiting", "waitingFor": "permission prompt"}])
        .to_string(),
    );
    if let Some(at) = notified_ms {
        let line = serde_json::json!({"v": 1, "ts": at, "event": "Notification", "session_id": SID,
            "notification_type": officina_core::board::BLOCKING_NOTIFICATIONS[0]});
        let events = env.events_dir();
        std::fs::create_dir_all(&events).unwrap();
        std::fs::write(
            events.join(officina_core::dates::event_file_name(at)),
            format!("{line}\n"),
        )
        .unwrap();
    }
    let bin = env.root.path().join("bin");
    let sidecar = serde_json::json!({"fixture": env.fixture(), "log": env.fake_log()});
    let fake = common::fake_claude_in(&bin, &sidecar);
    let paths = Paths {
        officina_home: env.officina_home(),
        claude_config_dir: env.claude_config(),
        claude_config_override: None,
        floor_file: Some(floor.clone()),
    };
    let collector = Collector::from_parts(paths, load_floor(Some(&floor)), Some(fake))
        .with_credential_store(OwnedStore(Box::new(FakeStore::empty())));
    let (sink, emitted) = common::sink(|_| {});
    Watched {
        handle: spawn(collector, sink).unwrap(),
        boards: emitted.boards,
        sidecar: bin.join("fake-claude.json"),
        log: env.fake_log(),
    }
}

fn stop(watched: Watched) {
    watched.handle.sender.send(CollectorMsg::Shutdown).unwrap();
    watched.handle.thread.join().unwrap();
}

/// The `claude` calls the fake logged so far.
fn spawns(log: &Path) -> usize {
    std::fs::read_to_string(log)
        .map(|text| text.lines().filter(|l| l.contains("agents")).count())
        .unwrap_or(0)
}

/// The first board within `within` that satisfies `wanted`.
fn until(rx: &Receiver<Board>, within: Duration, wanted: impl Fn(&Board) -> bool) -> Board {
    let deadline = Instant::now() + within;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let board = rx.recv_timeout(left).expect("nothing matching arrived");
        if wanted(&board) {
            return board;
        }
    }
}

fn scene_of(board: &Board) -> officina_core::scene::SceneState {
    board.columns.needs_you[0]
        .scene
        .expect("a live card has a scene state")
}

const ONE_MINUTE_FLOOR: &str =
    "schema = 1\n\n[alert]\nafter_minutes = 1\n\n[[project]]\nname = \"api\"\npath = \"api\"\n";

#[test]
fn an_escalation_recomputes_without_spawning() {
    let env = common::TestEnv::new();
    // The wait reaches the one-minute threshold a few seconds after start,
    // after the startup's spawns and well before the next poll.
    let reach_in = SETTLE + Duration::from_secs(3);
    assert!(reach_in + Duration::from_secs(3) < POLL_VISIBLE);
    let notified = common::now_ms() - 60_000 + reach_in.as_millis() as i64;
    let watched = watch(&env, ONE_MINUTE_FLOOR, Some(notified));
    let first = until(&watched.boards, Duration::from_secs(10), |b| {
        !b.columns.needs_you.is_empty()
    });
    assert_eq!(scene_of(&first).zone, Zone::Decision);
    assert_eq!(first.next_escalation_at_ms, Some(notified + 60_000));
    std::thread::sleep(SETTLE);
    let before = spawns(&watched.log);
    let alerted = until(&watched.boards, Duration::from_secs(10), |b| {
        b.alert_count > 0
    });
    assert_eq!(scene_of(&alerted).alert, Some(AlertCause::Waited));
    assert_eq!(scene_of(&alerted).zone, Zone::Alert);
    assert_eq!(alerted.next_escalation_at_ms, None);
    assert!(
        alerted.generated_at_ms >= notified + 60_000 && alerted.generated_at_ms < notified + 61_000,
        "within a second of the threshold: {}",
        alerted.generated_at_ms - notified - 60_000
    );
    assert_eq!(
        spawns(&watched.log),
        before,
        "the escalation spawned claude"
    );
    stop(watched);
}

#[test]
fn no_escalation_no_recompute() {
    let env = common::TestEnv::new();
    // Waiting with no known start: nothing can escalate by time.
    let watched = watch(&env, ONE_MINUTE_FLOOR, None);
    let first = until(&watched.boards, Duration::from_secs(10), |b| {
        !b.columns.needs_you.is_empty()
    });
    assert_eq!(first.next_escalation_at_ms, None);
    assert_eq!(scene_of(&first).waiting_since_ms, None);
    std::thread::sleep(SETTLE);
    while watched.boards.try_recv().is_ok() {}
    assert!(
        watched.boards.recv_timeout(Duration::from_secs(3)).is_err(),
        "a board was emitted with nothing to escalate"
    );
    stop(watched);
}

#[test]
fn a_failed_snapshot_keeps_the_alert() {
    let env = common::TestEnv::new();
    let floor = "schema = 1\n\n[[project]]\nname = \"api\"\npath = \"api\"\npriority = \"high\"\n";
    let watched = watch(&env, floor, Some(common::now_ms() - 30_000));
    let alerted = until(&watched.boards, Duration::from_secs(10), |b| {
        b.alert_count > 0
    });
    assert_eq!(scene_of(&alerted).alert, Some(AlertCause::Priority));
    std::thread::sleep(SETTLE);
    let failing =
        serde_json::json!({"fixture": env.fixture(), "log": env.fake_log(), "mode": "exit1"});
    std::fs::write(&watched.sidecar, failing.to_string()).unwrap();
    watched.handle.sender.send(CollectorMsg::Refresh).unwrap();
    let stale = until(&watched.boards, Duration::from_secs(10), |b| {
        b.stale.is_some()
    });
    let kept = scene_of(&stale);
    assert_eq!(kept.zone, Zone::Unknown);
    assert_eq!(kept.alert, Some(AlertCause::Priority));
    assert_eq!(kept.waiting_since_ms, scene_of(&alerted).waiting_since_ms);
    assert_eq!(stale.alert_count, alerted.alert_count);
    stop(watched);
}
