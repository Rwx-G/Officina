//! The window's collector thread end to end (stories 1.5, 2.7 and 3.2,
//! AD-3 to AD-5, AD-35, AD-43, AD-44): first board, re-emission on a hook
//! event, no re-emission when nothing changed; the floor reloaded with the
//! paths it names; the relay view rebuilt on the relay's files, the journal
//! and `RelayChanged`, emitted on change only; the known-projects record
//! kept current; the registry ingested before the first board, before each
//! session list and before each board, and reported when it cannot be.
//!
//! The tests of stories 1.5 and 2.7 give the fake claude no fixture (it
//! then lists no session). Story 3.2's run a copy of the fake configured by
//! its sidecar (D-86), with an agents fixture.

mod common;
#[path = "../../../tests/support/fake_store.rs"]
mod fake_store;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use officina_core::board::Board;
use officina_core::registry::retention::EXCERPT_TTL_MS;
use officina_core::registry::types::{RegistryState, SettingKey, Source};
use officina_core::relay::journal::{CatalogueApproved, JournalEntry};
use officina_core::relay::view::{RecentEvent, RelayView};
use officina_host::collector::{Collector, LoadedFloor, load_floor};
use officina_host::paths::Paths;
use officina_host::registry::ingest::IngestContext;
use officina_host::registry::store::{open_reader, open_writer};
use officina_host::relay::credential::OwnedStore;
use officina_host::relay::journal::append;
use officina_host::watch::{CollectorMsg, WatchHandle, spawn};
use officina_host::writers::{record_hook, record_quota};

use fake_store::{FakeStore, MARKER_SECRET};

const DAY_MS: i64 = 86_400_000;

fn paths_of(env: &common::TestEnv, floor: Option<PathBuf>) -> Paths {
    Paths {
        officina_home: env.officina_home(),
        claude_config_dir: env.claude_config(),
        claude_config_override: None,
        floor_file: floor,
    }
}

/// A collector thread on `env`, with the floor at `floor` when given; every
/// board and relay view it emits arrives on the returned receivers.
fn watch(
    env: &common::TestEnv,
    floor: Option<PathBuf>,
) -> (WatchHandle, Receiver<Board>, Receiver<RelayView>) {
    watch_with(env, floor, FakeStore::empty())
}

/// [`watch`] with `store` as the collector's credential store.
fn watch_with(
    env: &common::TestEnv,
    floor: Option<PathBuf>,
    store: FakeStore,
) -> (WatchHandle, Receiver<Board>, Receiver<RelayView>) {
    let paths = paths_of(env, floor.clone());
    let loaded = match &floor {
        Some(path) => load_floor(Some(path)),
        None => LoadedFloor::default(),
    };
    let collector = Collector::from_parts(paths, loaded, Some(PathBuf::from(common::FAKE_CLAUDE)))
        .with_credential_store(OwnedStore(Box::new(store)));
    let (boards, board_rx) = mpsc::channel::<Board>();
    let (views, view_rx) = mpsc::channel::<RelayView>();
    let handle = spawn(
        collector,
        move |b| {
            let _ = boards.send(b.clone());
        },
        move |v| {
            let _ = views.send(v.clone());
        },
    )
    .unwrap();
    (handle, board_rx, view_rx)
}

fn stop(handle: WatchHandle) {
    handle.sender.send(CollectorMsg::Shutdown).unwrap();
    handle.thread.join().unwrap();
}

/// The first value `rx` brings within 10 s that satisfies `wanted`.
fn until<T>(rx: &Receiver<T>, wanted: impl Fn(&T) -> bool) -> T {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let value = rx.recv_timeout(left).expect("nothing matching arrived");
        if wanted(&value) {
            return value;
        }
    }
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn emits_first_board_then_only_on_change() {
    let env = common::TestEnv::new();
    let paths = paths_of(&env, None);
    let (handle, rx, _views) = watch(&env, None);

    let first = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("first board");
    assert!(first.stale.is_none());
    assert!(handle.board.read().unwrap().is_some());

    assert!(
        rx.recv_timeout(Duration::from_millis(1600)).is_err(),
        "an unchanged board must not be emitted again"
    );

    let now = officina_host::files::now_ms();
    record_hook(
        &paths,
        br#"{"hook_event_name":"UserPromptSubmit","session_id":"s-gone","cwd":"C:/work/api","prompt":"x"}"#,
        now - 120_000,
        true,
        None,
    );
    record_hook(
        &paths,
        br#"{"hook_event_name":"SessionEnd","session_id":"s-gone","reason":"prompt_input_exit"}"#,
        now - 60_000,
        true,
        None,
    );
    let next = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("board after hook event");
    assert_eq!(next.columns.to_resume.len(), 1);
    assert_eq!(next.columns.to_resume[0].last_prompt.as_deref(), Some("x"));

    stop(handle);
}

#[test]
fn floor_edit_reloads_board_projects() {
    let env = common::TestEnv::new();
    let floor = env.root.path().join("floor").join("officina.toml");
    let declare = |name: &str| {
        write(
            &floor,
            &format!(
                "schema = 1\nname = \"{name}\"\n[[project]]\nname = \"{name}\"\npath = \"C:/work/api\"\n"
            ),
        );
    };
    declare("billing");
    let paths = paths_of(&env, Some(floor.clone()));
    let now = officina_host::files::now_ms();
    record_hook(
        &paths,
        br#"{"hook_event_name":"UserPromptSubmit","session_id":"s-gone","cwd":"C:/work/api/src","prompt":"x"}"#,
        now - 120_000,
        true,
        None,
    );
    record_hook(
        &paths,
        br#"{"hook_event_name":"SessionEnd","session_id":"s-gone","reason":"prompt_input_exit"}"#,
        now - 60_000,
        true,
        None,
    );
    let (handle, rx, _views) = watch(&env, Some(floor.clone()));
    let first = until(&rx, |b| !b.columns.to_resume.is_empty());
    assert_eq!(first.floor.as_deref(), Some("billing"));
    assert_eq!(first.columns.to_resume[0].project.name, "billing");

    declare("invoices");
    let next = until(&rx, |b| b.floor.as_deref() == Some("invoices"));
    assert_eq!(next.columns.to_resume[0].project.name, "invoices");

    write(&floor, "schema = [[[");
    let broken = until(&rx, |b| b.diagnostics.floor_error.is_some());
    assert_eq!(broken.floor, None);
    stop(handle);
}

#[test]
fn floor_edit_rebuilds_watched_paths() {
    let env = common::TestEnv::new();
    let floor = env.root.path().join("floor").join("officina.toml");
    let config = |name: &str| env.root.path().join(name);
    let point_at = |name: &str| {
        let dir = config(name).to_string_lossy().replace('\\', "/");
        write(
            &floor,
            &format!("schema = 1\nname = \"{name}\"\nconfig_dir = \"{dir}\"\n"),
        );
    };
    point_at("account-a");
    let (handle, rx, _views) = watch(&env, Some(floor.clone()));
    until(&rx, |b| b.floor.as_deref() == Some("account-a"));

    point_at("account-b");
    let moved = until(&rx, |b| b.floor.as_deref() == Some("account-b"));
    assert!(moved.quota.five_hour.is_none());
    // The window's focus and attach commands read these paths: they follow
    // the new config dir too (adversarial pass).
    let shared = handle.paths.read().unwrap().clone();
    assert_eq!(shared.claude_config_override, Some(config("account-b")));
    assert_eq!(
        shared.claude_sessions_dir(),
        config("account-b").join("sessions")
    );

    // The quota file of the new config dir is watched: writing it brings a
    // board without any other change.
    let account_b = Paths {
        claude_config_override: Some(config("account-b")),
        claude_config_dir: config("account-b"),
        ..paths_of(&env, None)
    };
    record_quota(
        &account_b,
        br#"{"rate_limits":{"five_hour":{"used_percentage":37.0,"resets_at":1800000000}}}"#,
        officina_host::files::now_ms() / 1000,
    );
    let quota = until(&rx, |b| b.quota.five_hour.is_some());
    assert_eq!(quota.quota.five_hour.unwrap().used_percentage, 37.0);
    stop(handle);
}

#[test]
fn relay_tree_change_recomputes_the_relay_view() {
    let env = common::TestEnv::new();
    let (handle, _boards, views) = watch(&env, None);
    let first = until(&views, |_| true);
    assert!(!first.stopped);
    assert!(handle.relay.read().unwrap().is_some(), "get_relay's cache");
    write(
        &env.relay_home().stopped_file(),
        r#"{"since_ms":1793102400000}"#,
    );
    let stopped = until(&views, |v| v.stopped);
    assert_eq!(stopped.stopped_since_ms, Some(1_793_102_400_000));
    stop(handle);
}

fn approved_line(hash: &str) -> JournalEntry {
    JournalEntry::CatalogueApproved(CatalogueApproved {
        hash: hash.to_owned(),
        previous_hash: None,
    })
}

#[test]
fn journal_append_recomputes_the_relay_view() {
    let env = common::TestEnv::new();
    let (handle, _boards, views) = watch(&env, None);
    let first = until(&views, |_| true);
    assert!(first.recent.is_empty());
    append(
        &env.relay_home(),
        &[approved_line("01")],
        officina_host::files::now_ms(),
    )
    .unwrap();
    let next = until(&views, |v| !v.recent.is_empty());
    assert!(matches!(
        next.recent[0].event,
        RecentEvent::CatalogueApproved(_)
    ));
    stop(handle);
}

#[test]
fn relay_view_is_emitted_only_on_change() {
    let env = common::TestEnv::new();
    let (handle, _boards, views) = watch(&env, None);
    until(&views, |_| true);
    // A file in the relay tree that changes nothing the view shows: the
    // view is rebuilt, and not emitted.
    write(&env.relay_home().approvals_dir().join("unrelated.txt"), "x");
    assert!(
        views.recv_timeout(Duration::from_millis(1600)).is_err(),
        "an unchanged relay view must not be emitted again"
    );
    write(&env.relay_home().stopped_file(), "{}");
    until(&views, |v| v.stopped);
    stop(handle);
}

#[test]
fn relay_changed_message_recomputes_now() {
    let env = common::TestEnv::new();
    let (handle, _boards, views) = watch(&env, None);
    until(&views, |_| true);
    // Yesterday's journal file is not watched: only the message tells.
    let yesterday = officina_host::files::now_ms() - DAY_MS;
    append(&env.relay_home(), &[approved_line("02")], yesterday).unwrap();
    assert!(
        views.recv_timeout(Duration::from_millis(1000)).is_err(),
        "nothing the collector stats changed"
    );
    let sent = Instant::now();
    handle.sender.send(CollectorMsg::RelayChanged).unwrap();
    let next = until(&views, |v| !v.recent.is_empty());
    assert!(
        sent.elapsed() < Duration::from_secs(2),
        "{:?}",
        sent.elapsed()
    );
    assert_eq!(next.recent[0].ts, yesterday);
    stop(handle);
}

#[test]
fn known_projects_follow_the_floor() {
    let env = common::TestEnv::new();
    env.write_floor(
        "schema = 2\n[[project]]\nname = \"lab\"\npath = \"Lab\"\nactions = [\"probe\"]\n[[action]]\nname = \"probe\"\nclass = \"free\"\nprogram = \"{tool}\"\n",
    );
    let hash = env.approve();
    let home = env.relay_home();
    let (handle, _boards, views) = watch(&env, None);
    until(&views, |_| true);

    env.write_floor(
        "schema = 2\n[[project]]\nname = \"lab\"\npath = \"Lab\"\nactions = [\"probe\"]\n[[project]]\nname = \"notes\"\npath = \"Notes\"\nautonomy = \"free\"\n[[action]]\nname = \"probe\"\nclass = \"free\"\nprogram = \"{tool}\"\n",
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let known = loop {
        let known = home.read_known_projects().unwrap();
        if known.project_paths.len() == 2 || Instant::now() > deadline {
            break known;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    assert!(
        known.project_paths.iter().any(|p| p.ends_with("/notes")),
        "{known:?}"
    );
    assert_eq!(known.approved_hash.as_deref(), Some(hash.as_str()));

    // The snapshot gone: the recorded hash stays, so the gate keeps reading
    // the snapshot as broken (D-15).
    std::fs::remove_file(home.approved_file()).unwrap();
    handle.sender.send(CollectorMsg::RelayChanged).unwrap();
    until(&views, |v| v.snapshot_error.is_some());
    assert_eq!(
        home.read_known_projects().unwrap().approved_hash.as_deref(),
        Some(hash.as_str())
    );
    stop(handle);

    // A workstation without the relay gets no record.
    let plain = common::TestEnv::new();
    plain.write_floor("schema = 1\n[[project]]\nname = \"api\"\npath = \"C:/work/api\"\n");
    let (handle, _boards, views) = watch(&plain, None);
    until(&views, |_| true);
    assert!(!plain.relay_home().known_projects_file().exists());
    stop(handle);
}

/// A relay floor approved in `env` with two credentials, `monitoring`
/// (used by an action) and `spare`.
fn credential_floor(env: &common::TestEnv) {
    env.write_floor(
        "schema = 2
[[project]]
name = \"lab\"
path = \"C:/path/to/Lab\"
actions = [\"health\"]
[[action]]
name = \"health\"
kind = \"http\"
class = \"free\"
method = \"GET\"
url = \"https://monitoring.example.com/health\"
credential = \"monitoring\"
[[credential]]
name = \"monitoring\"
hosts = [\"monitoring.example.com\"]
header = \"Authorization\"
scheme = \"Bearer\"
[[credential]]
name = \"spare\"
hosts = [\"example.com\"]
header = \"X-Api-Key\"
scheme = \"Token\"
",
    );
    env.approve();
}

fn presence(view: &RelayView) -> Vec<(String, Option<bool>)> {
    view.credentials
        .iter()
        .map(|c| (c.name.clone(), c.present))
        .collect()
}

/// Story 2.11 AC 12: the window's relay view says which approved
/// credentials the store holds, never the secret.
#[test]
fn relay_view_carries_credential_presence() {
    let env = common::TestEnv::new();
    credential_floor(&env);
    let store = FakeStore::with("monitoring", MARKER_SECRET);
    let (handle, _boards, views) = watch_with(&env, None, store);
    let view = until(&views, |v| {
        v.credentials.iter().any(|c| c.present.is_some())
    });
    assert_eq!(
        presence(&view),
        [
            ("monitoring".to_owned(), Some(true)),
            ("spare".to_owned(), Some(false))
        ]
    );
    assert_eq!(view.credentials[0].actions, ["health"]);
    let bytes = serde_json::to_vec(&view).unwrap();
    assert!(
        !bytes
            .windows(MARKER_SECRET.len())
            .any(|w| w == MARKER_SECRET.as_bytes())
    );
    stop(handle);
}

/// D-41: a journal append rebuilds the view without asking the store; a
/// relay change asks it again.
#[test]
fn credential_presence_is_polled_at_most_once_a_minute() {
    let env = common::TestEnv::new();
    credential_floor(&env);
    let store = FakeStore::with("monitoring", MARKER_SECRET);
    let (handle, _boards, views) = watch_with(&env, None, store.clone());
    until(&views, |v| {
        v.credentials.iter().any(|c| c.present.is_some())
    });
    let first = store.reads();
    assert_eq!(first, 2, "one read per credential");
    for i in 0..5 {
        let hash = format!("0{i}");
        append(
            &env.relay_home(),
            &[approved_line(&hash)],
            officina_host::files::now_ms(),
        )
        .unwrap();
        until(&views, |v| {
            v.recent
                .iter()
                .any(|e| matches!(&e.event, RecentEvent::CatalogueApproved(a) if a.hash == hash))
        });
    }
    assert_eq!(store.reads(), first, "journal appends asked the store");

    write(&env.relay_home().relay_dir().join("note.txt"), "x");
    let deadline = Instant::now() + Duration::from_secs(10);
    while store.reads() == first {
        assert!(
            Instant::now() < deadline,
            "a relay change did not ask the store"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    stop(handle);
}

/// One interactive session the agents fixture lists, with its id.
fn listed_session(id: &str, now: i64) -> String {
    serde_json::json!([{"pid": 4100, "cwd": "C:/work/api", "kind": "interactive", "startedAt": now - 60_000,
        "sessionId": id, "name": "api", "status": "idle"}])
    .to_string()
}

/// A collector thread on `env` whose `claude` is a copy of the fake,
/// configured by its sidecar to list `env`'s agents fixture (D-86); every
/// board goes through `on_board` on the collector thread, then arrives on
/// the receiver. Returns the sidecar's path too, which a test may rewrite.
fn watch_listing(
    env: &common::TestEnv,
    on_board: impl Fn(&Board) + Send + 'static,
) -> (WatchHandle, Receiver<Board>, PathBuf) {
    let bin = env.root.path().join("bin");
    let sidecar = serde_json::json!({"fixture": env.fixture(), "log": env.fake_log()});
    let fake = common::fake_claude_in(&bin, &sidecar);
    let collector = Collector::from_parts(paths_of(env, None), LoadedFloor::default(), Some(fake))
        .with_credential_store(OwnedStore(Box::new(FakeStore::empty())));
    let (boards, board_rx) = mpsc::channel::<Board>();
    let handle = spawn(
        collector,
        move |b| {
            on_board(b);
            let _ = boards.send(b.clone());
        },
        |_| {},
    )
    .unwrap();
    (handle, board_rx, bin.join("fake-claude.json"))
}

fn registry_state(board: &Board) -> Option<RegistryState> {
    board.diagnostics.registry.as_ref().map(|r| r.state)
}

/// The sessions the registry holds, read as another process would.
fn registered(env: &common::TestEnv) -> Vec<String> {
    open_reader(&env.officina_home(), &[])
        .map(|r| {
            r.sessions()
                .unwrap()
                .into_iter()
                .map(|s| s.session_id)
                .collect()
        })
        .unwrap_or_default()
}

fn hook_line(env: &common::TestEnv, payload: serde_json::Value, ts: i64) {
    record_hook(
        &paths_of(env, None),
        payload.to_string().as_bytes(),
        ts,
        true,
        None,
    );
}

fn shows(board: &Board, session: &str) -> bool {
    board
        .columns
        .all()
        .any(|card| card.session_id.as_deref() == Some(session))
}

/// AD-43, AD-44, D-85: at start, every event file is ingested and the
/// privacy bounds applied before the first board is built.
#[test]
fn start_ingests_and_purges_before_the_first_board() {
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    env.write_agents(&listed_session("s-live", now));
    // An earlier window stored an excerpt that has expired since.
    let expired = now - EXCERPT_TTL_MS - DAY_MS;
    hook_line(
        &env,
        serde_json::json!({"hook_event_name": "UserPromptSubmit", "session_id": "s-old", "cwd": "C:/work/api", "prompt": "expired prompt"}),
        expired,
    );
    {
        let events = env.events_dir();
        let home = env.officina_home();
        let ctx = IngestContext {
            events_dir: &events,
            officina_home: &home,
            projects: &[],
            no_excerpts: false,
        };
        let mut earlier = open_writer(&home, &[], expired).writer().unwrap();
        earlier.ingest(&ctx, expired + 1_000).unwrap();
        assert_eq!(earlier.excerpts().unwrap().len(), 1, "positive control");
    }
    for days in 0..4 {
        hook_line(
            &env,
            serde_json::json!({"hook_event_name": "Stop", "session_id": format!("day-{days}"), "cwd": "C:/work/api"}),
            now - days * DAY_MS,
        );
    }
    let seen = Arc::new(Mutex::new(None));
    let first = Arc::clone(&seen);
    let home = env.officina_home();
    let (handle, rx, _) = watch_listing(&env, move |_| {
        let mut slot = first.lock().unwrap();
        if slot.is_none() {
            let reader = open_reader(&home, &[]).unwrap();
            let sessions: Vec<String> = reader
                .sessions()
                .unwrap()
                .into_iter()
                .map(|s| s.session_id)
                .collect();
            *slot = Some((sessions, reader.excerpts().unwrap().len()));
        }
    });
    let board = until(&rx, |_| true);
    assert_eq!(registry_state(&board), Some(RegistryState::Ready));
    let (sessions, excerpts) = seen.lock().unwrap().clone().unwrap();
    for days in 0..4 {
        assert!(sessions.contains(&format!("day-{days}")), "{sessions:?}");
    }
    assert_eq!(excerpts, 0, "the expired excerpt was cleared first");
    stop(handle);
}

/// D-78: a tick ingests before it runs `claude agents`: while the fake
/// hangs in the call this event triggered, the registry already holds the
/// event's session.
#[test]
fn each_tick_ingests_before_the_agents_snapshot() {
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    env.write_agents(&listed_session("s-live", now));
    let (handle, rx, sidecar) = watch_listing(&env, |_| {});
    until(&rx, |_| true);
    // Past the startup's spawns and follow-ups.
    std::thread::sleep(Duration::from_millis(1500));
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&sidecar).unwrap()).unwrap();
    config["mode"] = "hang".into();
    config["hang_ms"] = 1500.into();
    std::fs::write(&sidecar, config.to_string()).unwrap();
    let calls = || {
        std::fs::read_to_string(env.fake_log())
            .unwrap_or_default()
            .lines()
            .count()
    };
    let before = calls();
    hook_line(
        &env,
        serde_json::json!({"hook_event_name": "UserPromptSubmit", "session_id": "s-tick", "cwd": "C:/work/api", "prompt": "x"}),
        officina_host::files::now_ms(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while calls() == before {
        assert!(
            Instant::now() < deadline,
            "the event brought no session list"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        registered(&env).contains(&"s-tick".to_owned()),
        "claude agents ran before the event was ingested"
    );
    config["mode"] = "ok".into();
    std::fs::write(&sidecar, config.to_string()).unwrap();
    stop(handle);
}

/// D-78: today's growth is ingested before the board that shows it is
/// built: when that board is emitted, the registry holds its session.
#[test]
fn today_growth_is_ingested_before_the_recompute() {
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    env.write_agents(&listed_session("s-live", now));
    let held_when_shown = Arc::new(Mutex::new(None));
    let seen = Arc::clone(&held_when_shown);
    let home = env.officina_home();
    let (handle, rx, _) = watch_listing(&env, move |board| {
        let mut slot = seen.lock().unwrap();
        if shows(board, "s-growth") && slot.is_none() {
            let reader = open_reader(&home, &[]).unwrap();
            *slot = Some(
                reader
                    .sessions()
                    .unwrap()
                    .iter()
                    .any(|s| s.session_id == "s-growth"),
            );
        }
    });
    until(&rx, |_| true);
    let at = officina_host::files::now_ms();
    hook_line(
        &env,
        serde_json::json!({"hook_event_name": "UserPromptSubmit", "session_id": "s-growth", "cwd": "C:/work/api", "prompt": "x"}),
        at - 120_000,
    );
    hook_line(
        &env,
        serde_json::json!({"hook_event_name": "SessionEnd", "session_id": "s-growth", "reason": "prompt_input_exit"}),
        at - 60_000,
    );
    until(&rx, |b| shows(b, "s-growth"));
    assert_eq!(*held_when_shown.lock().unwrap(), Some(true));
    stop(handle);
}

fn old_event_file(env: &common::TestEnv, days: i64) -> PathBuf {
    let ts = officina_host::files::now_ms() - days * DAY_MS;
    hook_line(
        env,
        serde_json::json!({"hook_event_name": "Stop", "session_id": format!("old-{days}"), "cwd": "C:/work/api"}),
        ts,
    );
    env.events_dir()
        .join(officina_core::dates::event_file_name(ts))
}

/// FR-1, D-47, P-13: a registry that cannot be opened leaves the board
/// working from live sources, deletes nothing, and the board says why.
#[test]
fn unavailable_registry_keeps_the_board_and_deletes_nothing() {
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    env.write_agents(&listed_session("s-live", now));
    std::fs::create_dir_all(env.officina_home()).unwrap();
    std::fs::write(
        env.officina_home().join("registry"),
        b"a file where the folder goes",
    )
    .unwrap();
    let old = old_event_file(&env, 5);
    let (handle, rx, _) = watch_listing(&env, |_| {});
    let board = until(&rx, |b| shows(b, "s-live"));
    assert_eq!(registry_state(&board), Some(RegistryState::Unavailable));
    assert!(
        board
            .diagnostics
            .registry
            .as_ref()
            .unwrap()
            .detail
            .is_some()
    );
    assert!(old.exists(), "nothing is deleted");
    stop(handle);
}

/// AD-42 last sentence, D-91: a registry found damaged while the window
/// runs becomes unavailable until the next start; the board continues,
/// nothing is deleted, no offset moves; the next start sets it aside.
#[test]
fn corruption_while_running_makes_the_registry_unavailable_until_restart() {
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    env.write_agents(&listed_session("s-live", now));
    let old = old_event_file(&env, 5);
    let (handle, rx, _) = watch_listing(&env, |_| {});
    let first = until(&rx, |_| true);
    assert_eq!(registry_state(&first), Some(RegistryState::Ready));
    // The collector's page cache holds the database's pages; a commit by
    // another connection makes it read the file again at its next
    // transaction, as SQLite does whenever its cache no longer covers a
    // page. No transaction of the collector's comes before the event below:
    // its next session list is the safety poll's, 15 s away.
    let mut other = open_writer(&env.officina_home(), &[], now)
        .writer()
        .unwrap();
    other
        .set_setting(SettingKey::AutoArchive, "false", Source::Author, now)
        .unwrap();
    drop(other);
    let db = officina_host::registry::RegistryPaths::new(&env.officina_home()).database();
    {
        use std::io::{Seek, SeekFrom, Write};
        // Windows' sharing mode lets another process write the open file.
        let bytes = std::fs::read(&db).unwrap();
        let page = usize::from(u16::from_be_bytes([bytes[16], bytes[17]]));
        let mut file = std::fs::OpenOptions::new().write(true).open(&db).unwrap();
        file.seek(SeekFrom::Start(page as u64)).unwrap();
        file.write_all(&vec![0xFF; bytes.len() - page]).unwrap();
    }
    let damaged = std::fs::read(&db).unwrap();
    hook_line(
        &env,
        serde_json::json!({"hook_event_name": "UserPromptSubmit", "session_id": "s-after", "cwd": "C:/work/api", "prompt": "x"}),
        officina_host::files::now_ms(),
    );
    let board = until(&rx, |b| {
        registry_state(b) == Some(RegistryState::Unavailable)
    });
    assert!(
        board
            .diagnostics
            .registry
            .as_ref()
            .unwrap()
            .detail
            .is_some()
    );
    assert!(shows(&board, "s-live"), "the board keeps its sessions");
    hook_line(
        &env,
        serde_json::json!({"hook_event_name": "Stop", "session_id": "s-live", "cwd": "C:/work/api", "last_assistant_message": "done"}),
        officina_host::files::now_ms(),
    );
    std::thread::sleep(Duration::from_millis(1500));
    let still = handle.board.read().unwrap().clone().unwrap();
    assert_eq!(
        registry_state(&still),
        Some(RegistryState::Unavailable),
        "until the next start"
    );
    assert_eq!(
        std::fs::read(&db).unwrap(),
        damaged,
        "no write, no offset moved"
    );
    assert!(old.exists(), "nothing is deleted");
    stop(handle);

    let (handle, rx, _) = watch_listing(&env, |_| {});
    let restarted = until(&rx, |_| true);
    assert_eq!(
        registry_state(&restarted),
        Some(RegistryState::Ready),
        "set aside and restored"
    );
    assert!(
        registered(&env).contains(&"s-after".to_owned()),
        "ingested again from the files"
    );
    stop(handle);
}
