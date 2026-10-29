//! The window's collector thread end to end (stories 1.5, 2.7 and 3.2,
//! AD-3 to AD-5, AD-35, AD-43, AD-44): first board, re-emission on a hook
//! event, no re-emission when nothing changed; the floor reloaded with the
//! paths it names; the relay view rebuilt on the relay's files, the journal
//! and `RelayChanged`, emitted on change only; the known-projects record
//! kept current; the registry ingested before the first board, before each
//! session list and before each board, and reported when it cannot be; the
//! registry view emitted on change, and the author's requests answered on
//! the collector thread (story 3.5); stale launching work orders expired
//! and started ones bound on the collector's tick (story 5.6).
//!
//! The tests of stories 1.5 and 2.7 give the fake claude no fixture (it
//! then lists no session). Story 3.2's run a copy of the fake configured by
//! its sidecar (D-86), with an agents fixture.

mod common;
#[path = "../../../tests/support/fake_store.rs"]
mod fake_store;
#[path = "../../../tests/support/spawn_cases.rs"]
mod spawn_cases;

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use officina_core::board::{Board, Column, Settled};
use officina_core::ipc::IpcErrorCode;
use officina_core::orders::work_order::{LAUNCHING_EXPIRY_MS, SubjectChoice, WorkOrder};
use officina_core::registry::confirm::{
    AuthorAction, ConfirmItem, FileUnder, ItemOutcome, SubjectOrigin,
};
use officina_core::registry::proposal::Proposal;
use officina_core::registry::retention::EXCERPT_TTL_MS;
use officina_core::registry::types::{
    AuthorRefusal, Ending, LaunchOutcome, ModelVerdict, ProposalSource, ReasonCode, RegistryState,
    SessionRecord, SettingKey, Settlement, Source, Verdict, WorkOrderRecord, WorkOrderSource,
};
use officina_core::registry::view::RegistryView;
use officina_core::relay::journal::{CatalogueApproved, JournalEntry};
use officina_core::relay::view::{RecentEvent, RelayView};
use officina_core::spawn::VectorKind;
use officina_core::untrusted::Untrusted;
use officina_host::collector::{Collector, LoadedFloor, load_floor};
use officina_host::paths::Paths;
use officina_host::registry::ingest::IngestContext;
use officina_host::registry::orders::LaunchRecord;
use officina_host::registry::store::{open_reader, open_writer};
use officina_host::relay::credential::OwnedStore;
use officina_host::relay::journal::append;
use officina_host::watch::{
    AUTHOR_HEADROOM, AUTHOR_WAIT, AuthorAnswer, AuthorCommand, AuthorReply, AuthorRequest,
    CollectorMsg, FOLLOW_UP, MIN_SPAWN_INTERVAL, RELAY_POLL, WatchHandle, spawn,
};
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
    let (sink, emitted) = common::sink(|_| {});
    let handle = spawn(collector, sink).unwrap();
    (handle, emitted.boards, emitted.relays)
}

fn stop(handle: WatchHandle) {
    handle.sender.send(CollectorMsg::Shutdown).unwrap();
    handle.thread.join().unwrap();
}

/// How long the startup's spawns and their follow-up take to pass, from
/// the plan's own constants: after it, a trigger's spawn is not rate
/// limited away.
const SETTLE: Duration = FOLLOW_UP.saturating_add(MIN_SPAWN_INTERVAL);

/// The next value `rx` brings within 10 s: after a wait in which nothing
/// was to come, it must be the change the test made next, so a repeat
/// sent meanwhile fails the test instead of being skipped (quality audit
/// M8).
fn next<T>(rx: &Receiver<T>) -> T {
    rx.recv_timeout(Duration::from_secs(10))
        .expect("the change made no emission")
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
    assert_eq!(
        next.columns.to_resume[0].last_prompt,
        Some(Untrusted::new("x".to_owned()))
    );

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
    assert!(
        next(&views).stopped,
        "the next view is the change, not a repeat"
    );
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
    let changed = next(&views);
    // Well before the relay's own poll, which would also have found it.
    assert!(sent.elapsed() < RELAY_POLL / 2, "{:?}", sent.elapsed());
    assert_eq!(changed.recent[0].ts, yesterday);
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
        let known = home.read_known_projects().valid().cloned().unwrap();
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
        home.read_known_projects()
            .valid()
            .cloned()
            .unwrap()
            .approved_hash
            .as_deref(),
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
/// its receiver, beside the relay and registry views. Returns the sidecar's
/// path too, which a test may rewrite.
fn watch_all(
    env: &common::TestEnv,
    on_board: impl Fn(&Board) + Send + 'static,
) -> (WatchHandle, common::Emitted, PathBuf) {
    let bin = env.root.path().join("bin");
    let sidecar = serde_json::json!({"fixture": env.fixture(), "log": env.fake_log()});
    let fake = common::fake_claude_in(&bin, &sidecar);
    let collector = Collector::from_parts(paths_of(env, None), LoadedFloor::default(), Some(fake))
        .with_credential_store(OwnedStore(Box::new(FakeStore::empty())));
    let (sink, emitted) = common::sink(on_board);
    let handle = spawn(collector, sink).unwrap();
    (handle, emitted, bin.join("fake-claude.json"))
}

/// [`watch_all`], its boards only.
fn watch_listing(
    env: &common::TestEnv,
    on_board: impl Fn(&Board) + Send + 'static,
) -> (WatchHandle, Receiver<Board>, PathBuf) {
    let (handle, emitted, sidecar) = watch_all(env, on_board);
    (handle, emitted.boards, sidecar)
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
    std::thread::sleep(SETTLE);
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

/// The registry's records, read as another process would, once `wanted`
/// holds for them; panics after 10 s.
fn records_once(
    env: &common::TestEnv,
    wanted: impl Fn(&[SessionRecord]) -> bool,
) -> Vec<SessionRecord> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let records = open_reader(&env.officina_home(), &[])
            .ok()
            .and_then(|r| r.session_records().ok())
            .unwrap_or_default();
        if wanted(&records) {
            return records;
        }
        assert!(
            Instant::now() < deadline,
            "the registry never held what was wanted: {records:#?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn record<'a>(records: &'a [SessionRecord], id: &str) -> Option<&'a SessionRecord> {
    records.iter().find(|r| r.session_id == id)
}

/// AD-46: what `claude agents` reports is stored with source `agents`.
#[test]
fn agents_facts_carry_source_agents() {
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    env.write_agents(
        &serde_json::json!([{"pid": 4300, "id": "bg01", "cwd": "C:/work/web", "kind": "background",
            "startedAt": now - 60_000, "sessionId": "s-bg", "name": "nightly", "status": "waiting",
            "state": "blocked", "waitingFor": "permission prompt"}])
        .to_string(),
    );
    let (handle, rx, _) = watch_listing(&env, |_| {});
    until(&rx, |b| shows(b, "s-bg"));
    let records = records_once(&env, |r| {
        record(r, "s-bg").is_some_and(|s| s.state.is_some())
    });
    let bg = record(&records, "s-bg").unwrap();
    let sources = [
        bg.kind.as_ref().map(|f| f.src),
        bg.name.as_ref().map(|f| f.src),
        bg.cwd.as_ref().map(|f| f.src),
        bg.claude_pid.as_ref().map(|f| f.src),
        bg.background_id.as_ref().map(|f| f.src),
        bg.status.as_ref().map(|f| f.src),
        bg.state.as_ref().map(|f| f.src),
        bg.listed.as_ref().map(|f| f.src),
    ];
    assert!(
        sources.iter().all(|src| *src == Some(Source::Agents)),
        "{sources:?}"
    );
    assert_eq!(bg.name.as_ref().unwrap().value, "nightly");
    assert_eq!(bg.project, "web");
    stop(handle);
}

/// D-79: a session first seen in `claude agents` is created by the one
/// creation function, like one first seen in an event; its later events
/// join the same record.
#[test]
fn a_session_first_seen_in_agents_gets_one_record() {
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    env.write_agents(&listed_session("s-agents", now));
    let (handle, rx, _) = watch_listing(&env, |_| {});
    until(&rx, |b| shows(b, "s-agents"));
    let records = records_once(&env, |r| record(r, "s-agents").is_some());
    assert_eq!(record(&records, "s-agents").unwrap().project, "api");
    assert!(
        record(&records, "s-agents")
            .unwrap()
            .last_prompt_at
            .is_none()
    );
    hook_line(
        &env,
        serde_json::json!({"hook_event_name": "UserPromptSubmit", "session_id": "s-agents", "cwd": "C:/work/api", "prompt": "x"}),
        officina_host::files::now_ms(),
    );
    let records = records_once(&env, |r| {
        record(r, "s-agents").is_some_and(|s| s.last_prompt_at.is_some())
    });
    assert_eq!(
        records
            .iter()
            .filter(|r| r.session_id == "s-agents")
            .count(),
        1
    );
    let session = record(&records, "s-agents").unwrap();
    assert_eq!(session.project, "api");
    assert_eq!(session.listed.as_ref().unwrap().src, Source::Agents);
    stop(handle);
}

/// AD-48, D-79: a session first seen in `claude agents` joins the single
/// active subject of its project at creation, like one first seen in an
/// event, and its card shows it.
#[test]
fn a_session_first_seen_in_agents_is_filed() {
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    hook_line(
        &env,
        serde_json::json!({"hook_event_name": "SessionStart", "session_id": "s-known", "cwd": "C:/work/api"}),
        now - 60_000,
    );
    let subject = {
        let events = env.events_dir();
        let home = env.officina_home();
        let mut writer = open_writer(&home, &[], now).writer().unwrap();
        writer
            .ingest(
                &IngestContext {
                    events_dir: &events,
                    officina_home: &home,
                    projects: &[],
                    no_excerpts: false,
                },
                now,
            )
            .unwrap();
        writer
            .create_subject("rollout", SubjectOrigin::Project("api".into()), now)
            .unwrap()
    };
    env.write_agents(&listed_session("s-agents", now));
    let (handle, rx, _) = watch_listing(&env, |_| {});
    let board = until(&rx, |b| {
        b.columns
            .all()
            .any(|c| c.session_id.as_deref() == Some("s-agents") && c.subject.is_some())
    });
    let card = board
        .columns
        .all()
        .find(|c| c.session_id.as_deref() == Some("s-agents"))
        .unwrap();
    assert_eq!(card.subject.as_ref().map(|s| s.id), Some(subject));
    let records = records_once(&env, |r| record(r, "s-agents").is_some());
    let filing = record(&records, "s-agents")
        .unwrap()
        .subject
        .clone()
        .unwrap();
    assert_eq!((filing.value, filing.src), (Some(subject), Source::Rules));
    stop(handle);
}

/// D-51: the snapshot is seeded from the registry at start, so a session
/// that died while the window was closed is `interrupted` at the first
/// refresh.
#[test]
fn session_gone_while_the_window_was_closed_is_interrupted() {
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    env.write_agents(&listed_session("s-closed", now));
    let (handle, rx, _) = watch_listing(&env, |_| {});
    until(&rx, |b| shows(b, "s-closed"));
    records_once(&env, |r| {
        record(r, "s-closed").is_some_and(|s| s.status.is_some())
    });
    stop(handle);

    env.write_agents("[]");
    let (handle, rx, _) = watch_listing(&env, |_| {});
    until(&rx, |b| !shows(b, "s-closed"));
    let records = records_once(&env, |r| {
        record(r, "s-closed").is_some_and(|s| s.interrupted.is_some())
    });
    let session = record(&records, "s-closed").unwrap();
    assert_eq!(session.interrupted.as_ref().unwrap().src, Source::Rules);
    assert_eq!(session.ending().map(|e| e.value), Some(Ending::Interrupted));
    stop(handle);
}

/// Writes a `SessionEnd` for `session` with `reason`, then empties the
/// session list, and waits for a board that no longer lists `session`.
/// Past the startup's spawns and follow-ups, the line's tick both ingests
/// and lists the sessions, so the order within one tick decides (D-78).
fn end_then_vanish(env: &common::TestEnv, rx: &Receiver<Board>, session: &str, reason: &str) {
    std::thread::sleep(SETTLE);
    hook_line(
        env,
        serde_json::json!({"hook_event_name": "SessionEnd", "session_id": session, "cwd": "C:/work/api", "reason": reason}),
        officina_host::files::now_ms(),
    );
    env.write_agents("[]");
    until(rx, |b| !shows(b, session) && b.stale.is_none());
}

/// D-78: a `SessionEnd` and the vanishing of its session seen in one tick:
/// the tick ingests first, so the real end is kept and no `interrupted` is
/// inferred.
#[test]
fn session_end_and_vanishing_in_one_tick_keep_the_real_end() {
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    env.write_agents(&listed_session("s-end", now));
    let (handle, rx, _) = watch_listing(&env, |_| {});
    until(&rx, |b| shows(b, "s-end"));
    records_once(&env, |r| record(r, "s-end").is_some());
    end_then_vanish(&env, &rx, "s-end", "prompt_input_exit");
    let records = records_once(&env, |r| {
        record(r, "s-end").is_some_and(|s| s.ended.is_some())
    });
    let session = record(&records, "s-end").unwrap();
    assert!(session.interrupted.is_none(), "{session:#?}");
    assert_eq!(
        session.ending().map(|e| e.value),
        Some(Ending::SessionEnd {
            reason: Some("prompt_input_exit".to_owned())
        })
    );
    stop(handle);
}

/// FR-3 as amended, AD-47: a session seen in `claude agents` that ends with
/// `other` is not non-interactive; a run seen only in events is.
#[test]
fn session_seen_live_is_never_non_interactive() {
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    env.write_agents(&listed_session("s-live", now));
    let (handle, rx, _) = watch_listing(&env, |_| {});
    until(&rx, |b| shows(b, "s-live"));
    records_once(&env, |r| record(r, "s-live").is_some());
    for (event, extra) in [
        ("SessionStart", ("source", "startup")),
        ("SessionEnd", ("reason", "other")),
    ] {
        let mut payload = serde_json::json!({"hook_event_name": event, "session_id": "p-run", "cwd": "C:/work/api"});
        payload[extra.0] = extra.1.into();
        hook_line(&env, payload, officina_host::files::now_ms());
    }
    end_then_vanish(&env, &rx, "s-live", "other");
    let records = records_once(&env, |r| {
        record(r, "s-live").is_some_and(|s| s.ended.is_some())
            && record(r, "p-run").is_some_and(|s| s.ended.is_some())
    });
    let live = record(&records, "s-live").unwrap();
    assert!(live.is_ended());
    assert!(!live.is_non_interactive(), "{live:#?}");
    assert!(record(&records, "p-run").unwrap().is_non_interactive());
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
    // An author action is answered with why nothing can be written, and
    // the Subjects view says why it is empty.
    let refused = ask(
        &handle,
        create("rollout", SubjectOrigin::Project("api".into())),
        AUTHOR_WAIT,
    )
    .recv_timeout(AUTHOR_WAIT)
    .unwrap()
    .unwrap_err();
    assert_eq!(
        refused.code,
        IpcErrorCode::RegistryUnavailable,
        "{refused:?}"
    );
    assert!(refused.message.contains("unavailable"), "{refused:?}");
    // So is a triage confirmation (story 3.7).
    let confirm = AuthorCommand::Confirm(vec![ConfirmItem {
        key: "subject:1".into(),
        shown: Proposal {
            verdict: ModelVerdict::ToResume,
            source: ProposalSource::Rules,
            reason: ReasonCode::DefaultToResume,
            call_id: None,
        },
        verdict: Verdict::Finished,
        routine: false,
        file_under: None,
    }]);
    let refused = ask(&handle, confirm, AUTHOR_WAIT)
        .recv_timeout(AUTHOR_WAIT)
        .unwrap()
        .unwrap_err();
    assert_eq!(
        refused.code,
        IpcErrorCode::RegistryUnavailable,
        "{refused:?}"
    );
    let view = handle.registry.read().unwrap().clone().unwrap();
    assert_eq!(view.registry.state, RegistryState::Unavailable);
    assert!(view.registry.detail.is_some() && view.subjects.is_empty());
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
    std::thread::sleep(SETTLE);
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

/// Sends `command` to the collector as a window command does, with `wait`
/// as its deadline, and returns the receiver of its answer.
fn ask(handle: &WatchHandle, command: AuthorCommand, wait: Duration) -> Receiver<AuthorReply> {
    let (request, answer) = AuthorRequest::new(command, wait);
    handle.sender.send(CollectorMsg::Author(request)).unwrap();
    answer
}

fn create(name: &str, origin: SubjectOrigin) -> AuthorCommand {
    AuthorCommand::Act(AuthorAction::Create {
        name: name.to_owned(),
        origin,
    })
}

/// The subjects' names the registry holds, read as another process would.
fn subject_names(env: &common::TestEnv) -> Vec<String> {
    open_reader(&env.officina_home(), &[])
        .map(|r| r.subjects().unwrap().into_iter().map(|s| s.name).collect())
        .unwrap_or_default()
}

/// The number of `claude` calls the fake has logged.
fn claude_calls(env: &common::TestEnv) -> usize {
    std::fs::read_to_string(env.fake_log())
        .unwrap_or_default()
        .lines()
        .count()
}

/// Makes the fake hang `hang_ms` in its next calls, asks for a refresh and
/// returns once that call has started, with the instant it was seen.
fn hold_in_a_refresh(
    env: &common::TestEnv,
    handle: &WatchHandle,
    sidecar: &Path,
    hang_ms: u64,
) -> Instant {
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(sidecar).unwrap()).unwrap();
    config["mode"] = "hang".into();
    config["hang_ms"] = hang_ms.into();
    std::fs::write(sidecar, config.to_string()).unwrap();
    // Past the startup's spawns, so the refresh is not rate limited away.
    std::thread::sleep(SETTLE);
    let before = claude_calls(env);
    handle.sender.send(CollectorMsg::Refresh).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while claude_calls(env) == before {
        assert!(Instant::now() < deadline, "the refresh never started");
        std::thread::sleep(Duration::from_millis(10));
    }
    Instant::now()
}

fn release(sidecar: &Path) {
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(sidecar).unwrap()).unwrap();
    config["mode"] = "ok".into();
    std::fs::write(sidecar, config.to_string()).unwrap();
}

/// AD-38, D-92: a window command's author request is run by the collector
/// thread on its one connection and answered there: an action with what it
/// created, a refusal with its rule, the archived list.
#[test]
fn author_request_is_answered_by_the_collector_thread() {
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    env.write_agents(&listed_session("s-live", now));
    let (handle, emitted, _) = watch_all(&env, |_| {});
    until(&emitted.boards, |b| shows(b, "s-live"));
    records_once(&env, |r| record(r, "s-live").is_some());

    let answer = ask(
        &handle,
        create("rollout", SubjectOrigin::Sessions(vec!["s-live".into()])),
        AUTHOR_WAIT,
    );
    let created = match answer.recv_timeout(AUTHOR_WAIT).unwrap() {
        Ok(AuthorAnswer::Done {
            created: Some(created),
        }) => created,
        other => panic!("{other:?}"),
    };
    assert_eq!(subject_names(&env), ["rollout"]);
    let filed = records_once(&env, |r| {
        record(r, "s-live").is_some_and(|s| s.filed_subject().is_some())
    });
    assert_eq!(
        record(&filed, "s-live").unwrap().filed_subject(),
        Some(created)
    );

    let refused = ask(
        &handle,
        create("Rollout", SubjectOrigin::Project("api".into())),
        AUTHOR_WAIT,
    )
    .recv_timeout(AUTHOR_WAIT)
    .unwrap()
    .unwrap_err();
    assert_eq!(
        (refused.code, refused.refusal),
        (IpcErrorCode::AuthorRefused, Some(AuthorRefusal::NameTaken))
    );
    assert_eq!(subject_names(&env), ["rollout"], "nothing written");

    let archive = AuthorCommand::Act(AuthorAction::Archive {
        subject: created,
        routine: true,
    });
    ask(&handle, archive, AUTHOR_WAIT)
        .recv_timeout(AUTHOR_WAIT)
        .unwrap()
        .unwrap();
    match ask(&handle, AuthorCommand::ListArchived, AUTHOR_WAIT)
        .recv_timeout(AUTHOR_WAIT)
        .unwrap()
    {
        Ok(AuthorAnswer::Archived(listed)) => {
            assert_eq!(listed.len(), 1);
            assert!(listed[0].routine);
            assert_eq!(listed[0].sessions[0].session_id, "s-live");
        }
        other => panic!("{other:?}"),
    }
    stop(handle);
}

/// Where the board shows `session`: its column and why it is settled.
fn placed(board: &Board, session: &str) -> Option<(Column, Option<Settled>)> {
    board
        .columns
        .all()
        .find(|card| card.session_id.as_deref() == Some(session))
        .map(|card| (card.column, card.settled))
}

/// Story 4.15, the owner's item 7 then item 3, end to end through the
/// collector thread: a session that ended with a prompt sits in To resume;
/// archiving its subject moves it to Done, reopening brings it back;
/// discarding it takes it off the board and lists it, restoring brings it
/// back; marking it done moves it to Done. Each step reaches the next
/// board the action publishes.
#[test]
fn author_verdicts_move_an_ended_session_on_the_board() {
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    env.write_agents(&listed_session("s-old", now));
    let (handle, emitted, _) = watch_all(&env, |_| {});
    until(&emitted.boards, |b| shows(b, "s-old"));
    records_once(&env, |r| record(r, "s-old").is_some());
    std::thread::sleep(SETTLE);
    let prompt = serde_json::json!({"hook_event_name": "UserPromptSubmit", "session_id": "s-old",
        "cwd": "C:/work/api", "prompt": "try the hook"});
    hook_line(&env, prompt, officina_host::files::now_ms());
    hook_line(
        &env,
        serde_json::json!({"hook_event_name": "SessionEnd", "session_id": "s-old", "cwd": "C:/work/api",
            "reason": "prompt_input_exit"}),
        officina_host::files::now_ms(),
    );
    env.write_agents("[]");
    until(&emitted.boards, |b| {
        placed(b, "s-old") == Some((Column::ToResume, None))
    });
    records_once(&env, |r| {
        record(r, "s-old").is_some_and(SessionRecord::is_ended)
    });

    let run = |command: AuthorCommand| {
        ask(&handle, command, AUTHOR_WAIT)
            .recv_timeout(AUTHOR_WAIT)
            .unwrap()
            .unwrap()
    };
    let subject = match run(create(
        "hook test",
        SubjectOrigin::Sessions(vec!["s-old".into()]),
    )) {
        AuthorAnswer::Done { created: Some(id) } => id,
        other => panic!("{other:?}"),
    };
    run(AuthorCommand::Act(AuthorAction::Archive {
        subject,
        routine: false,
    }));
    until(&emitted.boards, |b| {
        placed(b, "s-old") == Some((Column::Done, Some(Settled::SubjectArchived)))
    });
    run(AuthorCommand::Act(AuthorAction::Reopen { subject }));
    until(&emitted.boards, |b| {
        placed(b, "s-old") == Some((Column::ToResume, None))
    });

    let settle = |settlement| {
        AuthorCommand::Act(AuthorAction::Settle {
            session: "s-old".into(),
            settlement,
        })
    };
    run(settle(Some(Settlement::Discarded)));
    until(&emitted.boards, |b| placed(b, "s-old").is_none());
    match run(AuthorCommand::ListDiscarded) {
        AuthorAnswer::Discarded(listed) => {
            assert_eq!(listed.len(), 1);
            // Its name from `claude agents`; no prompt, since archiving the
            // subject cleared its excerpts (FR-4).
            assert_eq!(
                (
                    listed[0].session_id.as_str(),
                    listed[0].name.clone(),
                    listed[0].prompt.clone()
                ),
                ("s-old", Some(Untrusted::new("api".to_owned())), None)
            );
        }
        other => panic!("{other:?}"),
    }
    run(settle(None));
    until(&emitted.boards, |b| {
        placed(b, "s-old") == Some((Column::ToResume, None))
    });
    run(settle(Some(Settlement::Done)));
    until(&emitted.boards, |b| {
        placed(b, "s-old") == Some((Column::Done, Some(Settled::MarkedDone)))
    });
    match run(AuthorCommand::ListDiscarded) {
        AuthorAnswer::Discarded(listed) => assert!(listed.is_empty()),
        other => panic!("{other:?}"),
    }
    let unknown = ask(
        &handle,
        AuthorCommand::Act(AuthorAction::Settle {
            session: "never-seen".into(),
            settlement: Some(Settlement::Done),
        }),
        AUTHOR_WAIT,
    )
    .recv_timeout(AUTHOR_WAIT)
    .unwrap()
    .unwrap_err();
    assert_eq!(unknown.refusal, Some(AuthorRefusal::UnknownSession));
    stop(handle);
}

/// AD-38's trade-off: a request that arrives while `claude agents` runs is
/// answered once the call has returned, on the same thread, and is not
/// lost.
#[test]
fn author_request_waits_behind_a_running_refresh() {
    const HANG_MS: u64 = 1500;
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    env.write_agents(&listed_session("s-live", now));
    let (handle, emitted, sidecar) = watch_all(&env, |_| {});
    until(&emitted.boards, |b| shows(b, "s-live"));
    let started = hold_in_a_refresh(&env, &handle, &sidecar, HANG_MS);
    let answer = ask(
        &handle,
        create("behind", SubjectOrigin::Project("api".into())),
        AUTHOR_WAIT,
    );
    assert!(
        answer.recv_timeout(Duration::from_millis(500)).is_err(),
        "answered while claude agents ran"
    );
    let reply = answer.recv_timeout(AUTHOR_WAIT).unwrap();
    assert!(
        started.elapsed() >= Duration::from_millis(HANG_MS - 200),
        "{:?}",
        started.elapsed()
    );
    assert!(matches!(reply, Ok(AuthorAnswer::Done { created: Some(_) })));
    assert_eq!(subject_names(&env), ["behind"]);
    release(&sidecar);
    stop(handle);
}

/// D-92: a request whose deadline passed while the collector was busy is
/// dropped unrun, so the window, which has reported the timeout, never sees
/// the action happen later.
#[test]
fn expired_author_request_is_dropped_unexecuted() {
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    env.write_agents(&listed_session("s-live", now));
    let (handle, emitted, sidecar) = watch_all(&env, |_| {});
    until(&emitted.boards, |b| shows(b, "s-live"));
    hold_in_a_refresh(&env, &handle, &sidecar, 2000);
    let answer = ask(
        &handle,
        create("late", SubjectOrigin::Project("api".into())),
        AUTHOR_HEADROOM + Duration::from_millis(200),
    );
    assert!(
        matches!(
            answer.recv_timeout(Duration::from_secs(10)),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
        ),
        "dropped without an answer"
    );
    release(&sidecar);
    assert!(subject_names(&env).is_empty(), "never run");
    // Positive control: the same request in time is run.
    let reply = ask(
        &handle,
        create("late", SubjectOrigin::Project("api".into())),
        AUTHOR_WAIT,
    )
    .recv_timeout(AUTHOR_WAIT)
    .unwrap();
    assert!(reply.is_ok(), "{reply:?}");
    assert_eq!(subject_names(&env), ["late"]);
    stop(handle);
}

fn unfiled_ids(view: &RegistryView) -> Vec<&str> {
    view.unfiled
        .iter()
        .flat_map(|p| &p.sessions)
        .map(|s| s.session_id.as_str())
        .collect()
}

/// AD-5, AD-54: the registry view is emitted when its bytes change, never
/// again when nothing did; the cache `get_registry` reads holds the last.
#[test]
fn registry_view_is_emitted_only_on_change() {
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    env.write_agents(&listed_session("s-live", now));
    let (handle, emitted, _) = watch_all(&env, |_| {});
    let first = until(&emitted.registries, |v| unfiled_ids(v).contains(&"s-live"));
    assert_eq!(first.registry.state, RegistryState::Ready);
    assert!(
        first.unfiled[0].sessions[0].card.is_some(),
        "joined to its board card"
    );
    let cached: Option<Vec<String>> = handle
        .registry
        .read()
        .unwrap()
        .as_ref()
        .map(|v| unfiled_ids(v).into_iter().map(str::to_owned).collect());
    assert_eq!(cached, Some(vec!["s-live".to_owned()]));
    assert!(
        emitted
            .registries
            .recv_timeout(Duration::from_millis(1600))
            .is_err(),
        "an unchanged registry view must not be emitted again"
    );
    hook_line(
        &env,
        serde_json::json!({"hook_event_name": "UserPromptSubmit", "session_id": "s-new", "cwd": "C:/work/api", "prompt": "x"}),
        officina_host::files::now_ms(),
    );
    assert!(
        unfiled_ids(&next(&emitted.registries)).contains(&"s-new"),
        "the next view is the change, not a repeat"
    );
    stop(handle);
}

/// AD-54: every author action emits a registry view, and the board's cards
/// name the subject it filed them under.
#[test]
fn author_action_emits_a_new_view() {
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    env.write_agents(&listed_session("s-live", now));
    let (handle, emitted, _) = watch_all(&env, |_| {});
    until(&emitted.registries, |v| unfiled_ids(v).contains(&"s-live"));
    let reply = ask(
        &handle,
        create("rollout", SubjectOrigin::Sessions(vec!["s-live".into()])),
        AUTHOR_WAIT,
    );
    let view = emitted
        .registries
        .recv_timeout(AUTHOR_WAIT)
        .expect("a view after the action");
    assert_eq!(view.subjects[0].name, "rollout");
    assert_eq!(view.subjects[0].sessions[0].session_id, "s-live");
    assert!(reply.recv_timeout(AUTHOR_WAIT).unwrap().is_ok());
    let board = until(&emitted.boards, |b| {
        b.columns
            .all()
            .any(|c| c.subject.as_ref().is_some_and(|s| s.name == "rollout"))
    });
    assert!(shows(&board, "s-live"));

    // A refused action changes nothing; its view is emitted all the same.
    let refused = ask(
        &handle,
        create("rollout", SubjectOrigin::Project("api".into())),
        AUTHOR_WAIT,
    );
    let again = emitted
        .registries
        .recv_timeout(AUTHOR_WAIT)
        .expect("a view after the refused action");
    assert_eq!(again.subjects.len(), 1);
    assert!(refused.recv_timeout(AUTHOR_WAIT).unwrap().is_err());
    stop(handle);
}

/// The hook lines of one session that saw a prompt, an answer and an end,
/// from `at` on.
fn ended_session(env: &common::TestEnv, session: &str, at: i64) {
    let cwd = "C:/work/api";
    for (offset, payload) in [
        serde_json::json!({"hook_event_name": "SessionStart", "session_id": session, "cwd": cwd, "source": "startup"}),
        serde_json::json!({"hook_event_name": "UserPromptSubmit", "session_id": session, "cwd": cwd, "prompt": format!("work of {session}")}),
        serde_json::json!({"hook_event_name": "Stop", "session_id": session, "cwd": cwd, "last_assistant_message": "Done."}),
        serde_json::json!({"hook_event_name": "SessionEnd", "session_id": session, "cwd": cwd, "reason": "prompt_input_exit"}),
    ]
    .into_iter()
    .enumerate()
    {
        hook_line(env, payload, at + offset as i64 * 60_000);
    }
}

/// Journey 5, PRD section 7 Memory (story 3.7 AC 4): a subject and a
/// confirmation are made, the window is stopped, and three days of event
/// files in which sessions end are written while it is closed; at the next
/// start every one of those sessions is in the registry, ended, and the
/// subject and the confirmation made before the stop are still there.
#[test]
fn a_closed_weekend_loses_no_session_subject_or_verdict() {
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    env.write_agents("[]");
    ended_session(&env, "s-friday", now - 4 * DAY_MS);
    let (handle, emitted, _) = watch_all(&env, |_| {});
    let view = until(&emitted.registries, |v| {
        v.triage.iter().any(|r| r.key == "session:s-friday")
    });
    let row = view
        .triage
        .iter()
        .find(|r| r.key == "session:s-friday")
        .unwrap();
    let confirm = AuthorCommand::Confirm(vec![ConfirmItem {
        key: row.key.clone(),
        shown: row.proposal,
        verdict: Verdict::ToResume,
        routine: false,
        file_under: Some(FileUnder::New {
            name: "friday work".into(),
        }),
    }]);
    match ask(&handle, confirm, AUTHOR_WAIT)
        .recv_timeout(AUTHOR_WAIT)
        .unwrap()
    {
        Ok(AuthorAnswer::Confirmed(answers)) => assert!(
            matches!(answers[0].outcome, ItemOutcome::Confirmed { .. }),
            "{answers:?}"
        ),
        other => panic!("{other:?}"),
    }
    ask(
        &handle,
        create("monday plan", SubjectOrigin::Project("api".into())),
        AUTHOR_WAIT,
    )
    .recv_timeout(AUTHOR_WAIT)
    .unwrap()
    .unwrap();
    stop(handle);

    let weekend: Vec<String> = (1..=3).map(|d| format!("s-day-{d}")).collect();
    for (d, session) in weekend.iter().enumerate() {
        ended_session(&env, session, now - (d as i64 + 1) * DAY_MS);
    }
    let (handle, emitted, _) = watch_all(&env, |_| {});
    let view = until(&emitted.registries, |v| {
        v.registry.state == RegistryState::Ready
    });
    let records = open_reader(&env.officina_home(), &[])
        .unwrap()
        .session_records()
        .unwrap();
    for session in &weekend {
        let found = record(&records, session).unwrap_or_else(|| panic!("{session} lost"));
        assert!(found.is_ended(), "{session} not ended");
    }
    assert_eq!(subject_names(&env), ["friday work", "monday plan"]);
    let reader = open_reader(&env.officina_home(), &[]).unwrap();
    let confirmations = reader.confirmations().unwrap();
    assert_eq!(confirmations.len(), 1, "the verdict made before the stop");
    assert_eq!(confirmations[0].session_id.as_deref(), Some("s-friday"));
    assert!(
        view.triage.iter().all(|r| r.key != "session:s-friday"),
        "its row stays answered"
    );
    stop(handle);
}

/// Story 3.8 AC 3 and 4 through the window (AD-56, D-71): once the running
/// version has its record, a hook line whose payload lacked a field reaches
/// the record at ingestion, and the next board names the field.
#[test]
fn a_missing_hook_field_reaches_the_board() {
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    env.write_agents(&listed_session("s-live", now));
    let (handle, emitted, _) = watch_all(&env, |_| {});
    until(&emitted.boards, |b| b.contract.held == Some(true));
    let (event, fields) = officina_core::contract::HOOK_FIELDS[2];
    let mut payload = serde_json::json!({"hook_event_name": event});
    for field in fields.iter().skip(1) {
        payload[*field] = serde_json::json!("redacted");
    }
    hook_line(&env, payload, officina_host::files::now_ms());
    let expected = vec![format!("{event}.{}", fields[0])];
    let board = until(&emitted.boards, |b| !b.contract.hook_missing.is_empty());
    assert_eq!(board.contract.hook_missing, expected);
    let stored: serde_json::Value = serde_json::from_slice(
        &std::fs::read(env.officina_home().join("state").join("contract.json")).unwrap(),
    )
    .unwrap();
    let version = board.contract.claude_version.clone().unwrap();
    assert_eq!(
        stored["versions"][&version]["hook_missing"],
        serde_json::json!(expected)
    );
    stop(handle);
}

/// A confirmation the dry run can take: a session of `env` that saw a
/// prompt, an answer and an end a day ago, confirmed `to_resume` from its
/// default proposal through the window's writer, before the window starts.
fn seed_a_dry_run_case(env: &common::TestEnv) {
    let now = officina_host::files::now_ms();
    ended_session(env, "s-done", now - DAY_MS);
    let events = env.events_dir();
    let home = env.officina_home();
    let ctx = IngestContext {
        events_dir: &events,
        officina_home: &home,
        projects: &[],
        no_excerpts: false,
    };
    let mut writer = open_writer(&home, &[], now).writer().unwrap();
    writer.ingest(&ctx, now).unwrap();
    let view = writer.view(None, &events, now).unwrap();
    let row = view
        .triage
        .iter()
        .find(|r| r.key == "session:s-done")
        .unwrap();
    writer
        .confirm_triage(
            &[ConfirmItem {
                key: row.key.clone(),
                shown: row.proposal,
                verdict: Verdict::ToResume,
                routine: false,
                file_under: Some(FileUnder::New {
                    name: "done work".into(),
                }),
            }],
            now,
        )
        .unwrap();
}

/// Story 3.9 AC 6 (AD-52, spine AD-3 as amended): a dry run's call runs on
/// the `triage` thread; while it hangs, hook events still reach the board
/// at its usual pace, and `evaluate_triage_model` answered once queued.
#[test]
fn board_keeps_its_pace_while_a_triage_call_hangs() {
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    env.write_agents(&listed_session("s-live", now));
    seed_a_dry_run_case(&env);
    let bin = env.root.path().join("bin");
    let triage_log = env.root.path().join("triage.log");
    const HANG: Duration = Duration::from_secs(15);
    let sidecar = serde_json::json!({"fixture": env.fixture(), "log": env.fake_log(),
        "mode": "triage-hang", "hang_ms": HANG.as_millis() as u64, "triage_log": triage_log});
    let fake = common::fake_claude_in(&bin, &sidecar);
    let base = vec![(
        "SYSTEMROOT".to_owned(),
        std::env::var("SYSTEMROOT").unwrap_or_default(),
    )];
    let collector = Collector::from_parts(paths_of(&env, None), LoadedFloor::default(), Some(fake))
        .with_credential_store(OwnedStore(Box::new(FakeStore::empty())))
        .with_base_env(officina_host::spawn::BaseEnv::new(base));
    let (sink, emitted) = common::sink(|_| {});
    let handle = spawn(collector, sink).unwrap();
    until(&emitted.boards, |b| shows(b, "s-live"));

    let asked = Instant::now();
    match ask(&handle, AuthorCommand::Evaluate, AUTHOR_WAIT)
        .recv_timeout(AUTHOR_WAIT)
        .unwrap()
    {
        Ok(AuthorAnswer::Evaluating { cases }) => assert_eq!(cases, 1),
        other => panic!("{other:?}"),
    }
    assert!(
        asked.elapsed() < HANG / 3,
        "answered once queued, not after the call"
    );
    let running = until(&emitted.registries, |v| v.model.dry_run.is_some());
    assert_eq!(
        running.model.dry_run.map(|p| (p.done, p.total)),
        Some((0, 1))
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while !triage_log.exists() {
        assert!(Instant::now() < deadline, "the call never started");
        std::thread::sleep(Duration::from_millis(20));
    }

    // The call hangs; each hook event still shows within the board's pace.
    for i in 0..3 {
        let session = format!("s-new-{i}");
        let at = officina_host::files::now_ms();
        hook_line(
            &env,
            serde_json::json!({"hook_event_name": "UserPromptSubmit", "session_id": session, "cwd": "C:/work/api", "prompt": "x"}),
            at - 2_000,
        );
        let written = Instant::now();
        hook_line(
            &env,
            serde_json::json!({"hook_event_name": "SessionEnd", "session_id": session, "cwd": "C:/work/api", "reason": "prompt_input_exit"}),
            at - 1_000,
        );
        until(&emitted.boards, |b| shows(b, &session));
        assert!(
            written.elapsed() < HANG / 3,
            "a board waited on the call: {:?}",
            written.elapsed()
        );
    }
    let still = handle.registry.read().unwrap().clone().unwrap();
    assert!(still.model.dry_run.is_some(), "the call is still out");
    stop(handle);
}

/// A home whose registry holds 50 `state` `finished` proposals the author
/// confirmed (the auto-archive gate holds), a subject `live job` filed
/// with the background session `bg-live`, which saw a prompt, and
/// auto-archive on; the agents fixture lists `bg-live` in `state`.
/// Written through the window's writer before the window starts.
fn auto_archive_home(env: &common::TestEnv, state: &str) -> i64 {
    use officina_core::registry::fold::FactWrite;
    use officina_core::registry::types::{FactValue, SessionFact};
    let now = officina_host::files::now_ms();
    let ops = "C:/work/ops";
    let start = now - DAY_MS;
    for i in 0..50 {
        hook_line(
            env,
            serde_json::json!({"hook_event_name": "UserPromptSubmit", "session_id": format!("bg-{i:03}"), "cwd": ops, "prompt": "nightly"}),
            start + i,
        );
    }
    hook_line(
        env,
        serde_json::json!({"hook_event_name": "UserPromptSubmit", "session_id": "bg-live", "cwd": ops, "prompt": "nightly report"}),
        now - 60_000,
    );
    let events = env.events_dir();
    let home = env.officina_home();
    let ctx = IngestContext {
        events_dir: &events,
        officina_home: &home,
        projects: &[],
        no_excerpts: false,
    };
    let mut writer = open_writer(&home, &[], start).writer().unwrap();
    writer.ingest(&ctx, now - 50_000).unwrap();
    for i in 0..50 {
        let id = format!("bg-{i:03}");
        let fact = |fact, value: &str| FactWrite {
            session_id: id.clone(),
            fact,
            value: FactValue::Text(value.to_owned()),
            src: Source::Agents,
            at_ms: start + 60_000,
        };
        writer
            .record_facts(&[
                fact(SessionFact::Kind, "background"),
                fact(SessionFact::State, "done"),
            ])
            .unwrap();
        let at = now - 3_600_000 + i * 1_000;
        let subject = writer
            .create_subject(&format!("job {i}"), SubjectOrigin::Sessions(vec![id]), at)
            .unwrap();
        let key = format!("subject:{subject}");
        let view = writer.view(None, &events, at).unwrap();
        let shown = view.triage.iter().find(|r| r.key == key).unwrap().proposal;
        writer
            .confirm_triage(
                &[ConfirmItem {
                    key,
                    shown,
                    verdict: Verdict::Finished,
                    routine: false,
                    file_under: None,
                }],
                at + 500,
            )
            .unwrap();
    }
    let live = writer
        .create_subject(
            "live job",
            SubjectOrigin::Sessions(vec!["bg-live".into()]),
            now - 40_000,
        )
        .unwrap();
    writer
        .set_auto_archive(true, now - 30_000)
        .unwrap()
        .unwrap();
    write_background_listing(env, state, now);
    live
}

/// The agents fixture listing `bg-live` as a background session in `state`.
fn write_background_listing(env: &common::TestEnv, state: &str, now: i64) {
    env.write_agents(
        &serde_json::json!([{"pid": 4301, "id": "bg0001", "cwd": "C:/work/ops", "kind": "background",
            "startedAt": now - 120_000, "sessionId": "bg-live", "name": "nightly", "status": "idle", "state": state}])
        .to_string(),
    );
}

/// Story 3.10 AC 3 (FR-12, D-70): with auto-archive on and its gate
/// holding, the collector confirms the `state` `finished` row of a filed
/// subject right after the view that shows it, and the next view shows the
/// subject archived and listed among the recent auto-archives.
#[test]
fn auto_archive_runs_after_the_view_build() {
    let env = common::TestEnv::new();
    let live = auto_archive_home(&env, "done");
    let (handle, emitted, _) = watch_all(&env, |_| {});
    let view = until(&emitted.registries, |v| {
        v.recent_auto_archives
            .iter()
            .any(|a| a.subject.id == live && a.archived)
    });
    assert!(
        view.triage
            .iter()
            .all(|r| r.key != format!("subject:{live}"))
    );
    assert!(view.auto_archive.on);
    let subjects = open_reader(&env.officina_home(), &[])
        .unwrap()
        .subjects()
        .unwrap();
    let archived = subjects.iter().find(|s| s.id == live).unwrap();
    assert_eq!(
        archived.status.value,
        officina_core::registry::types::SubjectStatus::Archived
    );
    assert!(!archived.routine);
    stop(handle);
}

/// Story 3.10 AC 4 (D-77, FR-12 amendment 10): the background session of
/// an auto-archived subject re-instructed within 7 days reopens the
/// subject, the `state` source counts one flip, and the gate it breaks
/// turns auto-archive off with its reason.
#[test]
fn a_re_instructed_auto_archived_session_counts_as_a_flip() {
    let env = common::TestEnv::new();
    let live = auto_archive_home(&env, "done");
    let (handle, emitted, _) = watch_all(&env, |_| {});
    until(&emitted.registries, |v| {
        v.recent_auto_archives
            .iter()
            .any(|a| a.subject.id == live && a.archived)
    });
    write_background_listing(&env, "working", officina_host::files::now_ms());
    handle.sender.send(CollectorMsg::Refresh).unwrap();
    let view = until(&emitted.registries, |v| {
        v.recent_auto_archives
            .iter()
            .any(|a| a.subject.id == live && a.reopened_at_ms.is_some())
    });
    let state = view
        .rates
        .iter()
        .find(|r| r.source == ProposalSource::State)
        .unwrap();
    let finished = state.finished.unwrap();
    assert_eq!((finished.cases, finished.flipped), (50, 1));
    let reopened = view.subjects.iter().find(|s| s.id == live).unwrap();
    assert_eq!(
        reopened.status,
        officina_core::registry::types::SubjectStatus::Active
    );
    let off = until(&emitted.registries, |v| !v.auto_archive.on);
    assert_eq!(
        off.auto_archive.reason,
        Some(officina_core::registry::rates::GateState::TooManyFlips)
    );
    assert_eq!(off.auto_archive.src, Some(Source::Rules));
    stop(handle);
}

/// Story 3.11 AC 2 (AD-52): with the model on, the window's collector
/// sends a row falling to the default to the `triage` thread on its own,
/// records the fake's answer, and emits a view showing the model's
/// proposal; the board never waits on it.
#[test]
fn the_collector_sends_a_default_row_to_the_enabled_model() {
    use officina_core::registry::dry_run::Tally;
    use officina_core::registry::types::{ModelVerdict, ProposalSource};
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    env.write_agents(&listed_session("s-live", now));
    ended_session(&env, "s-pending", now - 2 * 3_600_000);
    let events = env.events_dir();
    let home = env.officina_home();
    let mut writer = open_writer(&home, &[], now).writer().unwrap();
    let ctx = IngestContext {
        events_dir: &events,
        officina_home: &home,
        projects: &[],
        no_excerpts: false,
    };
    writer.ingest(&ctx, now).unwrap();
    writer
        .record_evaluation(
            &Tally {
                cases: 30,
                model_correct: 20,
                default_correct: 10,
                ..Tally::default()
            },
            now - 60_000,
        )
        .unwrap();
    writer.set_triage_model(true, now).unwrap().unwrap();
    drop(writer);
    let bin = env.root.path().join("bin");
    let triage_log = env.root.path().join("triage.log");
    let sidecar = serde_json::json!({"fixture": env.fixture(), "log": env.fake_log(),
        "mode": "triage", "triage_log": triage_log});
    let fake = common::fake_claude_in(&bin, &sidecar);
    let base = vec![(
        "SYSTEMROOT".to_owned(),
        std::env::var("SYSTEMROOT").unwrap_or_default(),
    )];
    let collector = Collector::from_parts(paths_of(&env, None), LoadedFloor::default(), Some(fake))
        .with_credential_store(OwnedStore(Box::new(FakeStore::empty())))
        .with_base_env(officina_host::spawn::BaseEnv::new(base));
    let (sink, emitted) = common::sink(|_| {});
    let handle = spawn(collector, sink).unwrap();
    until(&emitted.boards, |b| shows(b, "s-live"));
    let answered = until(&emitted.registries, |v| {
        v.triage
            .iter()
            .any(|r| r.key == "session:s-pending" && r.proposal.source == ProposalSource::Model)
    });
    let row = answered
        .triage
        .iter()
        .find(|r| r.key == "session:s-pending")
        .unwrap();
    assert_eq!(
        row.proposal.verdict,
        ModelVerdict::Unsure,
        "no token: unsure"
    );
    assert!(row.model_reason.is_some());
    assert!(!row.model_pending);
    assert_eq!(answered.model.calls.today.calls, 1);
    let sent = std::fs::read_to_string(&triage_log).unwrap();
    assert_eq!(sent.lines().count(), 1, "one case, one call");
    assert!(sent.contains("work of s-pending"));
    stop(handle);
}

/// The golden work order of `tests/vectors/spawn.json`, its floor's folder
/// under `env`'s root, and that project folder created.
fn golden_order(env: &common::TestEnv) -> WorkOrder {
    let order = spawn_cases::load()
        .cases
        .into_iter()
        .find(|case| case.kind() == VectorKind::WorkOrder)
        .expect("a golden work order")
        .work_order(Some(&env.root.path().to_string_lossy()));
    std::fs::create_dir_all(order.project_folder()).unwrap();
    order
}

/// Records the launch of `order` as `card` with `seq`, its subject a new
/// one named "Rollout", at `at`, through a writer of its own.
fn record_launch(env: &common::TestEnv, order: &WorkOrder, card: &str, seq: u32, at: i64) {
    let mut writer = open_writer(&env.officina_home(), &[], at).writer().unwrap();
    writer
        .insert_work_order(
            &LaunchRecord {
                order,
                card_id: card,
                seq,
                source: WorkOrderSource::Composer,
                subject: &SubjectChoice::New("Rollout".into()),
                proposal_id: None,
            },
            at,
        )
        .unwrap()
        .unwrap();
}

/// The work order `card` as a command line reads it.
fn work_order_row(env: &common::TestEnv, card: &str) -> Option<WorkOrderRecord> {
    open_reader(&env.officina_home(), &[])
        .ok()?
        .work_orders()
        .ok()?
        .into_iter()
        .find(|order| order.id == card)
}

/// Polls `card`'s row until `wanted` holds; panics after 10 s.
fn work_order_once(
    env: &common::TestEnv,
    card: &str,
    wanted: impl Fn(&WorkOrderRecord) -> bool,
) -> WorkOrderRecord {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(order) = work_order_row(env, card).filter(|o| wanted(o)) {
            return order;
        }
        assert!(
            Instant::now() < deadline,
            "{card} never reached the state wanted: {:?}",
            work_order_row(env, card)
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// D-136, review F14: a `launching` row older than two minutes reads
/// `unknown` before the first board at start, and one written while the
/// window runs turns `unknown` on the collector's own tick, without a
/// restart.
#[test]
fn a_stale_launching_row_becomes_unknown_on_the_tick() {
    let env = common::TestEnv::new();
    env.write_agents("[]");
    let order = golden_order(&env);
    let stale = |now: i64| now - LAUNCHING_EXPIRY_MS - 1_000;
    record_launch(
        &env,
        &order,
        "at-start",
        1,
        stale(officina_host::files::now_ms()),
    );
    let seen = Arc::new(Mutex::new(None));
    let first = Arc::clone(&seen);
    let home = env.officina_home();
    let (handle, rx, _) = watch_listing(&env, move |_| {
        let mut slot = first.lock().unwrap();
        if slot.is_none() {
            let outcome = open_reader(&home, &[])
                .unwrap()
                .work_orders()
                .unwrap()
                .into_iter()
                .find(|o| o.id == "at-start")
                .map(|o| o.outcome);
            *slot = Some(outcome);
        }
    });
    until(&rx, |_| true);
    assert_eq!(
        *seen.lock().unwrap(),
        Some(Some(LaunchOutcome::Unknown)),
        "expired before the first board"
    );
    record_launch(
        &env,
        &order,
        "on-tick",
        2,
        stale(officina_host::files::now_ms()),
    );
    work_order_once(&env, "on-tick", |o| o.outcome == LaunchOutcome::Unknown);
    stop(handle);
}

/// AD-81 steps 4 and 5, D-129: the fake in `bg` mode starts the order's
/// background session and lists it in `claude agents`; the window's next
/// refresh binds the order to it by its name, and the session is filed
/// under the subject created at `started`, source `author`.
#[test]
fn a_started_order_binds_to_its_background_session() {
    use officina_host::spawn::{BaseEnv, ResolvedClaude, SpawnOutcome, execute};

    let env = common::TestEnv::new();
    env.write_agents("[]");
    let order = golden_order(&env);
    let launched_at = officina_host::files::now_ms();
    record_launch(&env, &order, "card", 1, launched_at);
    let background = common::fake_claude_in(
        &env.root.path().join("bin-bg"),
        &serde_json::json!({"fixture": env.fixture(), "log": env.fake_log(), "mode": "bg"}),
    );
    let base = BaseEnv::new(vec![(
        "SYSTEMROOT".to_owned(),
        std::env::var("SYSTEMROOT").unwrap_or_default(),
    )]);
    let outcome = execute(
        &ResolvedClaude::new(background),
        &officina_core::spawn::work_order(&order, 1, None),
        &base,
    );
    let Ok(SpawnOutcome::Captured(ran)) = outcome else {
        panic!("{outcome:?}");
    };
    assert!(ran.status.success());
    let printed = String::from_utf8_lossy(&ran.stdout).into_owned();
    {
        let mut writer = open_writer(&env.officina_home(), &[], launched_at)
            .writer()
            .unwrap();
        writer
            .record_launch_outcome(
                "card",
                LaunchOutcome::Started,
                Some(0),
                None,
                Some(&printed),
                officina_host::files::now_ms(),
            )
            .unwrap();
    }
    let (handle, _rx, _) = watch_listing(&env, |_| {});
    let bound = work_order_once(&env, "card", |o| o.bound_at_ms.is_some());
    let bg_id = bound.bg_id.clone().unwrap();
    assert!(
        printed.contains(&bg_id),
        "the id --bg printed is the bound entry's: {printed}"
    );
    let session = bound.session.expect("the bound session's record");
    assert_eq!(
        Some(session.session_id.as_str()),
        bound.session_id.as_deref()
    );
    let filing = session.subject.expect("filed");
    assert!(bound.subject_id.is_some(), "created at started");
    assert_eq!(filing.value, bound.subject_id);
    assert_eq!(filing.src, Source::Author);
    stop(handle);
}
