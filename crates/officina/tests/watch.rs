//! The window's collector thread end to end (stories 1.5, 2.7 and 3.2,
//! AD-3 to AD-5, AD-35, AD-43, AD-44): first board, re-emission on a hook
//! event, no re-emission when nothing changed; the floor reloaded with the
//! paths it names; the relay view rebuilt on the relay's files, the journal
//! and `RelayChanged`, emitted on change only; the known-projects record
//! kept current; the registry ingested before the first board, before each
//! session list and before each board, and reported when it cannot be; the
//! registry view emitted on change, and the author's requests answered on
//! the collector thread (story 3.5); stale launching work orders expired
//! and started ones bound on the collector's tick (story 5.6); cards
//! launched from the collector's store, their outcome recorded and
//! journaled once, the board at its pace while `claude --bg` runs (story
//! 5.7); the Orchestrator view emitted on change and its briefing counted
//! from the author's last reading (story 5.8); a meeting's reads off the
//! collector thread, a subject's meeting on its latest prompted session,
//! and no git where only an event line names the folder (story 5.10);
//! answers sent, refused, copied, attached, confirmed or not, and
//! journaled by their hash (story 5.11).
//!
//! The tests of stories 1.5 and 2.7 give the fake claude no fixture (it
//! then lists no session). Story 3.2's run a copy of the fake configured by
//! its sidecar (D-86), with an agents fixture.

mod common;
#[path = "../../../tests/support/fake_store.rs"]
mod fake_store;
#[path = "../../../tests/support/spawn_cases.rs"]
mod spawn_cases;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use officina_core::board::{Board, Column, Settled};
use officina_core::ipc::IpcErrorCode;
use officina_core::meeting::{AnswerRefused, AnswerResult, Changes, Delivery, Logs};
use officina_core::orders::briefing::Cursor;
use officina_core::orders::card::{LaunchAnswer, LaunchRefusal};
use officina_core::orders::view::ORCHESTRATOR_VIEW_SCHEMA;
use officina_core::orders::work_order::{LAUNCHING_EXPIRY_MS, SubjectChoice, WorkOrder};
use officina_core::registry::confirm::{
    AuthorAction, ConfirmItem, FileUnder, ItemOutcome, SubjectOrigin,
};
use officina_core::registry::proposal::Proposal;
use officina_core::registry::retention::EXCERPT_TTL_MS;
use officina_core::registry::types::{
    AuthorRefusal, Ending, LaunchOutcome, ModelVerdict, ProposalSource, ProposalStatus, ReasonCode,
    RegistryState, SessionRecord, SettingKey, Settlement, Source, Verdict, WorkOrderRecord,
    WorkOrderSource,
};
use officina_core::registry::view::RegistryView;
use officina_core::relay::journal::{AnswerOutcome, MeetingAnswer};
use officina_core::relay::journal::{CatalogueApproved, JournalEntry, WorkOrderLaunched};
use officina_core::relay::view::{RecentEvent, RelayView};
use officina_core::spawn::VectorKind;
use officina_core::untrusted::Untrusted;
use officina_host::collector::{Collector, LoadedFloor, load_floor};
use officina_host::meeting::{AnswerFrom, send_answer};
use officina_host::orders::{LaunchGrant, LaunchReport, LaunchResult};
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

/// D-760: a line spilled beside the day file, while another writer holds
/// the lock, rebuilds the view as an append does.
#[test]
fn journal_spill_recomputes_the_relay_view() {
    let env = common::TestEnv::new();
    let (handle, _boards, views) = watch(&env, None);
    let first = until(&views, |_| true);
    assert!(first.recent.is_empty());
    let held = officina_host::relay::journal::JournalLock::acquire(&env.relay_home()).unwrap();
    officina_host::relay::journal::record(
        &env.relay_home(),
        &[approved_line("01")],
        officina_host::files::now_ms(),
    )
    .unwrap();
    let next = until(&views, |v| !v.recent.is_empty());
    drop(held);
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

/// An untrusted text as the window receives it, through serialization.
fn shown(text: &Untrusted<String>) -> String {
    serde_json::to_value(text)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

/// The work order rows of `home`, read as a command line reads them.
fn launched_row(home: &common::orders::OrdersHome, card: &str) -> WorkOrderRecord {
    work_order_row(&home.env, card).unwrap_or_else(|| panic!("no row for {card}"))
}

/// The `work_order_launched` lines of `home`'s journal.
fn launch_lines(home: &common::orders::OrdersHome) -> Vec<WorkOrderLaunched> {
    home.env
        .journal()
        .into_iter()
        .filter_map(|line| match line.entry {
            JournalEntry::WorkOrderLaunched(launched) => Some(launched),
            _ => None,
        })
        .collect()
}

/// Story 5.7 AC 4 (AD-81 steps 1 to 3): a click launches the card's
/// vector once; its row reads `started` with what `claude --bg` printed,
/// and one `work_order_launched` line records what ran.
#[test]
fn a_launch_records_started_and_one_journal_line() {
    let home = common::orders::OrdersHome::start(serde_json::json!({"mode": "bg"}));
    let card = home.bump("serde_json", "minor");
    assert_eq!(
        home.launch(&card.id, &card.prompt_sha256),
        LaunchAnswer::Launched(LaunchOutcome::Started)
    );
    let row = launched_row(&home, &card.id);
    assert_eq!(row.outcome, LaunchOutcome::Started);
    assert_eq!(row.name, card.name);
    assert_eq!(row.exit_code, Some(0));
    let printed = shown(&row.printed.expect("what --bg printed"));
    assert!(printed.starts_with("backgrounded"), "{printed}");
    let lines = launch_lines(&home);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let line = &lines[0];
    assert_eq!(line.outcome, LaunchOutcome::Started);
    assert_eq!(line.exit_code, Some(0));
    assert_eq!(line.order_id, card.id);
    assert_eq!(line.argv, card.argv);
    assert_eq!(line.template, "bump");
    assert_eq!(line.project, "backend");
    assert_eq!(line.prompt_sha256, card.prompt_sha256);
    assert_eq!(line.catalogue_hash, card.catalogue_hash);
    assert_eq!(
        line.values,
        BTreeMap::from([
            ("level".to_owned(), "minor".to_owned()),
            ("package".to_owned(), "serde_json".to_owned())
        ])
    );
}

/// Story 5.7 AC 4, review F2: what runs is the card's argument vector, byte
/// for byte, element by element, in the project's folder.
#[test]
fn the_launched_vector_equals_the_card_vector() {
    let home = common::orders::OrdersHome::start(serde_json::json!({"mode": "bg"}));
    let card = home.bump("tokio", "patch");
    home.launch(&card.id, &card.prompt_sha256);
    let calls = home.bg_calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    let argv = common::logged_argv(&calls[0]);
    assert_eq!(argv.len(), card.argv.len());
    for (ran, shown) in argv.iter().zip(&card.argv) {
        assert_eq!(ran.as_bytes(), shown.as_bytes());
    }
    assert_eq!(
        std::fs::canonicalize(calls[0]["cwd"].as_str().unwrap()).unwrap(),
        std::fs::canonicalize(home.project()).unwrap()
    );
    let stored = launched_row(&home, &card.id).argv;
    assert_eq!(stored, card.argv, "the row stores what ran");
}

/// AD-81 step 3, S4: `claude` refusing an untrusted folder is its own
/// outcome, with what it said.
#[test]
fn workspace_not_trusted_is_its_own_outcome() {
    let home = common::orders::OrdersHome::start(serde_json::json!({"mode": "bg-untrusted"}));
    let card = home.bump("serde_json", "patch");
    assert_eq!(
        home.launch(&card.id, &card.prompt_sha256),
        LaunchAnswer::Launched(LaunchOutcome::UntrustedWorkspace)
    );
    let row = launched_row(&home, &card.id);
    assert_eq!(row.outcome, LaunchOutcome::UntrustedWorkspace);
    assert_eq!(row.exit_code, Some(1));
    let error = shown(&row.error.expect("its stderr"));
    assert!(
        error.contains(officina_core::orders::card::WORKSPACE_NOT_TRUSTED),
        "{error}"
    );
    assert_eq!(
        launch_lines(&home)[0].outcome,
        LaunchOutcome::UntrustedWorkspace
    );
}

/// FR-4, D-135: a failed spawn burns its card; nothing retries it, and
/// the same click again is refused before anything starts.
#[test]
fn a_failed_spawn_burns_the_card_and_is_not_retried() {
    let home = common::orders::OrdersHome::start(serde_json::json!({"mode": "exit1"}));
    let card = home.bump("serde_json", "minor");
    assert_eq!(
        home.launch(&card.id, &card.prompt_sha256),
        LaunchAnswer::Launched(LaunchOutcome::Failed)
    );
    let row = launched_row(&home, &card.id);
    assert_eq!(row.outcome, LaunchOutcome::Failed);
    assert_eq!(row.exit_code, Some(1));
    assert_eq!(row.error, Some(Untrusted::new("fake failure".to_owned())));
    assert_eq!(
        home.launch(&card.id, &card.prompt_sha256),
        LaunchAnswer::Refused(LaunchRefusal::Consumed)
    );
    std::thread::sleep(SETTLE);
    assert_eq!(home.bg_calls().len(), 1, "one spawn, never retried");
    assert_eq!(launch_lines(&home).len(), 1);
}

/// FR-4: two clicks on one card at once launch it once; the other is
/// refused as consumed.
#[test]
fn a_double_click_launches_once() {
    let home = common::orders::OrdersHome::start(serde_json::json!({"mode": "bg"}));
    let card = home.bump("serde_json", "minor");
    let sender = home.handle().sender.clone();
    let clicks: Vec<_> = (0..2)
        .map(|_| {
            let (sender, id, sha) = (sender.clone(), card.id.clone(), card.prompt_sha256.clone());
            std::thread::spawn(move || {
                officina_host::orders::launch_card(&sender, &id, &sha).unwrap()
            })
        })
        .collect();
    let answers: Vec<LaunchAnswer> = clicks.into_iter().map(|c| c.join().unwrap()).collect();
    assert!(
        answers.contains(&LaunchAnswer::Launched(LaunchOutcome::Started)),
        "{answers:?}"
    );
    assert!(
        answers.contains(&LaunchAnswer::Refused(LaunchRefusal::Consumed)),
        "{answers:?}"
    );
    assert_eq!(home.bg_calls().len(), 1);
    assert_eq!(launch_lines(&home).len(), 1);
}

/// D-135, review F14: a launch's outcome carries no deadline; one sent
/// past its own is still recorded, its row and its line.
#[test]
fn a_late_launch_outcome_is_still_recorded() {
    let home = common::orders::OrdersHome::start(serde_json::json!({"mode": "bg"}));
    let card = home.bump("tokio", "minor");
    let granted = officina_host::watch::ask(
        &home.handle().sender,
        AuthorCommand::LaunchCard {
            card_id: card.id.clone(),
            prompt_sha256: card.prompt_sha256.clone(),
        },
        AUTHOR_WAIT,
    )
    .unwrap()
    .into_launch()
    .unwrap();
    let LaunchGrant::Granted(ticket) = granted else {
        panic!("{granted:?}");
    };
    assert_eq!(ticket.vector.args(), card.argv);
    assert_eq!(
        launched_row(&home, &card.id).outcome,
        LaunchOutcome::Launching
    );
    let report = LaunchReport {
        order_id: ticket.order_id.clone(),
        line: ticket.line.clone(),
        result: LaunchResult {
            outcome: LaunchOutcome::Started,
            exit_code: Some(0),
            error: None,
            printed: Some("backgrounded".to_owned()),
        },
    };
    // Its deadline is already past when it is sent.
    let (request, answer) = AuthorRequest::new(
        AuthorCommand::LaunchOutcome(Box::new(report)),
        Duration::ZERO,
    );
    home.handle()
        .sender
        .send(CollectorMsg::Author(request))
        .unwrap();
    assert_eq!(
        answer.recv_timeout(AUTHOR_WAIT).unwrap(),
        Ok(AuthorAnswer::Done { created: None })
    );
    assert_eq!(
        launched_row(&home, &card.id).outcome,
        LaunchOutcome::Started
    );
    assert_eq!(launch_lines(&home).len(), 1);
    assert!(home.bg_calls().is_empty(), "the test ran nothing");
}

/// AD-3 as amended, AD-81 step 2: the launch runs on the command's thread;
/// while `claude --bg` hangs there, hook events still reach the board at
/// its usual pace.
#[test]
fn the_board_keeps_its_pace_while_a_launch_spawns() {
    const HANG: Duration = Duration::from_secs(12);
    let home = common::orders::OrdersHome::start(
        serde_json::json!({"mode": "bg-hold", "hang_ms": HANG.as_millis() as u64}),
    );
    let card = home.bump("serde_json", "minor");
    let sender = home.handle().sender.clone();
    let (id, sha) = (card.id.clone(), card.prompt_sha256.clone());
    let launched =
        std::thread::spawn(move || officina_host::orders::launch_card(&sender, &id, &sha).unwrap());
    {
        let deadline = Instant::now() + Duration::from_secs(10);
        while home.bg_calls().is_empty() {
            assert!(Instant::now() < deadline, "the launch never started");
            std::thread::sleep(Duration::from_millis(20));
        }
        for i in 0..3 {
            let session = format!("s-during-launch-{i}");
            let at = officina_host::files::now_ms();
            hook_line(
                &home.env,
                serde_json::json!({"hook_event_name": "UserPromptSubmit", "session_id": session, "cwd": "C:/work/api", "prompt": "x"}),
                at - 2_000,
            );
            let written = Instant::now();
            hook_line(
                &home.env,
                serde_json::json!({"hook_event_name": "SessionEnd", "session_id": session, "cwd": "C:/work/api", "reason": "prompt_input_exit"}),
                at - 1_000,
            );
            until(&home.emitted.boards, |b| shows(b, &session));
            assert!(
                written.elapsed() < HANG / 3,
                "a board waited on the launch: {:?}",
                written.elapsed()
            );
        }
        assert!(!launched.is_finished(), "claude still runs");
    }
    assert_eq!(
        launched.join().unwrap(),
        LaunchAnswer::Launched(LaunchOutcome::Started)
    );
}

// ---- story 5.13: the review worker (AD-86, AD-3 as amended, D-142) ----

/// The prompt of the board's session, once a board shows `text` as it.
#[cfg(windows)]
fn shows_prompt(board: &Board, text: &str) -> bool {
    board.columns.all().any(|card| {
        card.session_id.as_deref() == Some(common::auto::SESSION)
            && card
                .last_prompt
                .as_ref()
                .is_some_and(|p| serde_json::to_value(p).unwrap() == text)
    })
}

/// Story 5.13 AC 4: a review call runs on the `review` thread; while it
/// hangs, the author's prompts still reach the board at its pace.
#[cfg(windows)]
#[test]
fn the_board_keeps_its_pace_while_a_review_hangs() {
    use common::auto::{Auto, Options, SESSION, stop};
    const HANG: Duration = Duration::from_secs(15);
    let options = Options {
        mode: "review-hang",
        hang_ms: u64::try_from(HANG.as_millis()).unwrap(),
        ..Options::default()
    };
    let auto = Auto::new(&options);
    auto.request("lab-web", Some(SESSION));
    let fake = auto.fake(&options, "bin");
    let (handle, emitted) = auto.window_with(auto.collector(Some(fake), 2 * HANG));
    let deadline = Instant::now() + Duration::from_secs(10);
    while auto.calls().is_empty() {
        assert!(Instant::now() < deadline, "the review never started");
        std::thread::sleep(Duration::from_millis(20));
    }
    for i in 0..3 {
        let text = format!("next step {i}");
        let written = Instant::now();
        auto.prompt(&text, officina_host::files::now_ms());
        until(&emitted.boards, |b| shows_prompt(b, &text));
        assert!(
            written.elapsed() < HANG / 3,
            "a board waited on the review: {:?}",
            written.elapsed()
        );
    }
    assert!(auto.rows().is_empty(), "the review is still out");
    stop(handle);
}

/// Story 5.13 AC 4 (AD-52 as amended): the triage model's dry run runs on
/// its own thread; while its call hangs, a review still runs and answers.
#[cfg(windows)]
#[test]
fn a_triage_dry_run_does_not_delay_a_review() {
    use common::auto::{Auto, Options, SESSION, stop};
    let options = Options {
        triage_mode: Some("triage-hang"),
        hang_ms: 20_000,
        prompt: "Apply it. FAKE-REVIEW:reject".to_owned(),
        ..Options::default()
    };
    let auto = Auto::new(&options);
    seed_a_dry_run_case(&auto.relay.env);
    let id = auto.request("lab-web", Some(SESSION));
    let fake = auto.fake(&options, "bin");
    let (handle, _emitted) = auto.window_with(auto.collector(Some(fake), Duration::from_secs(10)));
    match ask(&handle, AuthorCommand::Evaluate, AUTHOR_WAIT)
        .recv_timeout(AUTHOR_WAIT)
        .unwrap()
    {
        Ok(AuthorAnswer::Evaluating { cases }) => assert_eq!(cases, 1),
        other => panic!("{other:?}"),
    }
    let started = Instant::now();
    let row = auto.row_of(&id);
    assert_eq!(
        row.decision,
        Some(officina_core::relay::auto::ReviewDecision::Reject)
    );
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "the review waited on the triage call: {:?}",
        started.elapsed()
    );
    let calls = auto.calls();
    assert!(
        calls
            .iter()
            .any(|c| c["argv"].to_string().contains("verdict")),
        "the triage call ran too"
    );
    stop(handle);
}

/// Story 5.13 AC 4 (D-142): two requests a second apart are both reviewed,
/// their calls at least `CALL_SPACING_MS` apart: pacing, not escalation.
#[cfg(windows)]
#[test]
fn queued_reviews_are_paced_5_s_apart() {
    use common::auto::{Auto, Options, SESSION, stop};
    use officina_core::registry::triage_call::CALL_SPACING_MS;
    let options = Options::default();
    let auto = Auto::new(&options);
    let first = auto.request("lab-web", Some(SESSION));
    let second = auto.request("lab-db", Some(SESSION));
    let (handle, _emitted) = auto.window(&options);
    let a = auto.row_of(&first);
    let b = auto.row_of(&second);
    stop(handle);
    for row in [&a, &b] {
        assert_eq!(
            row.outcome,
            officina_core::registry::types::ReviewOutcome::Answered
        );
    }
    let gap = (b.call_started_at_ms.unwrap() - a.call_started_at_ms.unwrap()).abs();
    assert!(gap >= CALL_SPACING_MS, "{gap} ms apart");
    assert_eq!(auto.calls().len(), 2);
}

/// Story 5.13 AC 4 (D-142): both requests pass the precheck before any
/// call; the first review's approve reaches the cap of one, and the cap,
/// checked again right before the second call, escalates it with no call.
#[cfg(windows)]
#[test]
fn the_cap_is_checked_again_right_before_each_call() {
    use common::auto::{Auto, Options, SESSION, stop};
    use officina_core::relay::auto::EscalationCause;
    let options = Options {
        cap: 1,
        prompt: "Apply it. FAKE-REVIEW:approve".to_owned(),
        ..Options::default()
    };
    let auto = Auto::new(&options);
    let first = auto.request("lab-web", Some(SESSION));
    let second = auto.request("lab-db", Some(SESSION));
    let (handle, _emitted) = auto.window(&options);
    let a = auto.row_of(&first);
    let b = auto.row_of(&second);
    stop(handle);
    assert_eq!(
        a.decision,
        Some(officina_core::relay::auto::ReviewDecision::Approve)
    );
    assert_eq!(b.cause, Some(EscalationCause::HourlyCap));
    assert_eq!(b.call_started_at_ms, None);
    assert_eq!(auto.calls().len(), 1);
}

/// AD-92, AD-5: the Orchestrator view is built after each recompute,
/// cached for `get_orchestrator`, and emitted only when its bytes change,
/// its generation times aside; an author's reading is such a change.
#[test]
fn the_orchestrator_view_is_emitted_only_on_change() {
    let env = common::TestEnv::new();
    let now = officina_host::files::now_ms();
    env.write_agents(&listed_session("s-live", now));
    let (handle, emitted, _) = watch_all(&env, |_| {});
    let first = until(&emitted.orchestrators, |v| {
        v.briefing.subjects.count == 0 && v.briefing.cursor == Cursor::None
    });
    assert_eq!(first.schema, ORCHESTRATOR_VIEW_SCHEMA);
    // The startup's spawns and their follow-up settle first.
    std::thread::sleep(SETTLE);
    while emitted.orchestrators.try_recv().is_ok() {}
    let cached = handle
        .orchestrator
        .read()
        .unwrap()
        .as_ref()
        .map(|v| v.briefing.cursor);
    assert_eq!(cached, Some(Cursor::None), "cached for the first paint");
    assert!(
        emitted
            .orchestrators
            .recv_timeout(Duration::from_millis(1600))
            .is_err(),
        "an unchanged view must not be emitted again"
    );
    let reply = ask(&handle, AuthorCommand::MarkBriefingRead, AUTHOR_WAIT);
    let read = next(&emitted.orchestrators);
    assert!(
        matches!(read.briefing.cursor, Cursor::ReadAt { .. }),
        "the next view is the reading, not a repeat"
    );
    assert!(reply.recv_timeout(AUTHOR_WAIT).unwrap().is_ok());
    assert!(
        emitted
            .orchestrators
            .recv_timeout(Duration::from_millis(1600))
            .is_err(),
        "nothing else changed"
    );
    stop(handle);
}

/// AD-83: the window's briefing counts from the author's last reading,
/// stored by the writer, and a window started again reads it back; before
/// the first reading it has no cursor.
#[test]
fn the_window_briefing_starts_at_the_last_reading() {
    let env = common::TestEnv::new();
    env.write_agents("[]");
    let (handle, emitted, _) = watch_all(&env, |_| {});
    let first = until(&emitted.orchestrators, |_| true);
    assert_eq!(first.briefing.cursor, Cursor::None, "no reading yet");
    let before = officina_host::files::now_ms();
    let reply = ask(&handle, AuthorCommand::MarkBriefingRead, AUTHOR_WAIT);
    assert!(reply.recv_timeout(AUTHOR_WAIT).unwrap().is_ok());
    let read = until(&emitted.orchestrators, |v| {
        v.briefing.cursor != Cursor::None
    });
    let Cursor::ReadAt { at_ms } = read.briefing.cursor else {
        panic!(
            "the window's cursor is its reading: {:?}",
            read.briefing.cursor
        );
    };
    assert!(at_ms >= before && at_ms <= officina_host::files::now_ms());
    stop(handle);
    let stored = open_reader(&env.officina_home(), &[])
        .unwrap()
        .briefing_read_at()
        .unwrap();
    assert_eq!(stored, Some(at_ms), "setting.briefing_read_at_ms");

    let (again, emitted, _) = watch_all(&env, |_| {});
    let restarted = until(&emitted.orchestrators, |_| true);
    assert_eq!(restarted.briefing.cursor, Cursor::ReadAt { at_ms });
    stop(again);
}

// ---- story 5.16: the orchestrator's lifecycle (AD-89, D-144, D-167) ----

/// A window on its own home whose `claude` is a copy of the fake in mode
/// `orchestrator`, with an executable directory of its own (D-167): the
/// functions the window's Start command calls, without Tauri.
struct OrchestratorWindow {
    env: common::TestEnv,
    /// The window's executable directory, holding an `officina.exe` unless
    /// the test says not.
    exe_dir: PathBuf,
    claude: PathBuf,
    handle: Option<WatchHandle>,
    emitted: Option<common::Emitted>,
}

impl OrchestratorWindow {
    /// The fake's sidecar holds `sidecar` beside its fixture and log; the
    /// copy is named `claude_name` (`claude.cmd` for a shim).
    fn new(sidecar: serde_json::Value, claude_name: &str, with_officina_exe: bool) -> Self {
        let env = common::TestEnv::new();
        env.write_agents("[]");
        let mut sidecar = sidecar;
        sidecar["mode"] = "orchestrator".into();
        sidecar["fixture"] = env.fixture().to_string_lossy().into();
        sidecar["log"] = env.fake_log().to_string_lossy().into();
        let bin = env.root.path().join("bin");
        let fake = common::fake_claude_in(&bin, &sidecar);
        let claude = if claude_name == "claude" {
            fake
        } else {
            let renamed = bin.join(claude_name);
            std::fs::copy(&fake, &renamed).unwrap();
            renamed
        };
        let exe_dir = env.root.path().join("app");
        std::fs::create_dir_all(&exe_dir).unwrap();
        if with_officina_exe {
            let name = officina_host::orders::orchestrator::officina_exe_name();
            std::fs::write(exe_dir.join(name), b"").unwrap();
        }
        let mut window = OrchestratorWindow {
            env,
            exe_dir,
            claude,
            handle: None,
            emitted: None,
        };
        window.open();
        window
    }

    /// Starts the window's collector thread, as a window start does.
    fn open(&mut self) {
        let collector = Collector::from_parts(
            paths_of(&self.env, None),
            LoadedFloor::default(),
            Some(self.claude.clone()),
        )
        .with_base_env(officina_host::spawn::BaseEnv::new(vec![(
            "SYSTEMROOT".to_owned(),
            std::env::var("SYSTEMROOT").unwrap_or_default(),
        )]))
        .with_credential_store(OwnedStore(Box::new(FakeStore::empty())));
        let (sink, emitted) = common::sink(|_| {});
        let handle = spawn(collector, sink).unwrap();
        emitted
            .boards
            .recv_timeout(Duration::from_secs(10))
            .expect("first board");
        self.handle = Some(handle);
        self.emitted = Some(emitted);
    }

    /// Stops the collector thread, as closing the window does.
    fn close(&mut self) {
        if let Some(handle) = self.handle.take() {
            stop(handle);
        }
        self.emitted = None;
    }

    fn handle(&self) -> &WatchHandle {
        self.handle.as_ref().unwrap()
    }

    /// What `start_orchestrator` answers.
    fn start(&self) -> officina_core::orders::orchestrator::StartAnswer {
        officina_host::orders::orchestrator::start_orchestrator(
            &self.handle().sender,
            &self.exe_dir,
        )
        .expect("the collector answers")
    }

    fn workdir(&self) -> PathBuf {
        officina_host::orders::orchestrator::workdir(&self.env.officina_home())
    }

    /// The `orchestrator_start` rows, newest first, as another process
    /// reads them.
    fn rows(&self) -> Vec<officina_host::registry::read::OrchestratorStartRow> {
        open_reader(&self.env.officina_home(), &[])
            .unwrap()
            .orchestrator_starts()
            .unwrap()
    }

    /// The `orchestrator_started` lines.
    fn lines(&self) -> Vec<officina_core::relay::journal::OrchestratorStarted> {
        self.env
            .journal()
            .into_iter()
            .filter_map(|line| match line.entry {
                JournalEntry::OrchestratorStarted(started) => Some(started),
                _ => None,
            })
            .collect()
    }

    /// The fake's calls holding `--name`: the orchestrator's spawns,
    /// waiting up to 10 s for `count` of them, since a console is not
    /// waited on.
    fn spawns(&self, count: usize) -> Vec<serde_json::Value> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let calls: Vec<serde_json::Value> = common::fake_claude_calls(&self.env.fake_log())
                .into_iter()
                .filter(|c| {
                    c["argv"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|x| x == "--name"))
                })
                .collect();
            if calls.len() >= count || Instant::now() > deadline {
                return calls;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// The first board within 10 s that `wanted` accepts, the session list
    /// refreshed meanwhile. Only the Windows tests start a session.
    #[cfg(windows)]
    fn board_until(&self, wanted: impl Fn(&Board) -> bool) -> Board {
        let deadline = Instant::now() + Duration::from_secs(10);
        let boards = &self.emitted.as_ref().unwrap().boards;
        loop {
            let _ = self.handle().sender.send(CollectorMsg::Refresh);
            if let Ok(board) = boards.recv_timeout(Duration::from_millis(700))
                && wanted(&board)
            {
                return board;
            }
            assert!(Instant::now() < deadline, "no board matched");
        }
    }
}

impl Drop for OrchestratorWindow {
    fn drop(&mut self) {
        self.close();
    }
}

/// Story 5.16 AC 3: a refusal records its row with its cause and its
/// line, and starts nothing.
fn assert_refused(
    window: &OrchestratorWindow,
    cause: officina_core::orders::orchestrator::StartRefusal,
) {
    use officina_core::orders::orchestrator::StartAnswer;
    use officina_core::registry::types::OrchestratorStartOutcome;
    assert_eq!(window.start(), StartAnswer::Refused(cause));
    let rows = window.rows();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].outcome, OrchestratorStartOutcome::Refused);
    assert_eq!(rows[0].error.as_deref(), Some(cause.code()));
    assert_eq!((rows[0].pid, rows[0].pid_created_at_ms), (None, None));
    let lines = window.lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0].outcome, OrchestratorStartOutcome::Refused);
    assert!(lines[0].argv.is_empty());
    std::thread::sleep(Duration::from_millis(300));
    assert!(window.spawns(0).is_empty(), "the fake started");
}

/// Story 5.16 AC 3 (AD-89, D-562): a `claude` that is no `.exe` is
/// refused before anything starts: the fake's log stays absent.
#[test]
fn start_is_refused_with_a_shim() {
    use officina_core::orders::orchestrator::StartRefusal;
    let window = OrchestratorWindow::new(serde_json::json!({}), "claude.cmd", true);
    assert_refused(&window, StartRefusal::ClaudeNotExe);
    assert!(!window.env.fake_log().exists(), "nothing ran");
}

/// Story 5.16 AC 3 (AD-89): a workdir holding anything is refused, and
/// what it holds is kept.
#[test]
fn start_is_refused_with_a_non_empty_workdir() {
    use officina_core::orders::orchestrator::StartRefusal;
    let window = OrchestratorWindow::new(serde_json::json!({}), "claude", true);
    write(&window.workdir().join("left.txt"), "left behind");
    assert_refused(&window, StartRefusal::WorkdirNotEmpty);
    assert!(window.workdir().join("left.txt").is_file(), "never deleted");
}

/// Story 5.16 AC 3 (D-144, D-167): no `officina.exe` in the window's
/// executable directory is refused, since the MCP config would name it.
#[test]
fn start_is_refused_without_officina_exe() {
    use officina_core::orders::orchestrator::StartRefusal;
    let window = OrchestratorWindow::new(serde_json::json!({}), "claude", false);
    assert_refused(&window, StartRefusal::OfficinaExeMissing);
}

/// Story 5.16 AC 3 (D-144): Start creates the workdir and the proposals
/// folder, refused or not, and never deletes them.
#[test]
fn start_creates_the_two_folders() {
    let window = OrchestratorWindow::new(serde_json::json!({}), "claude", false);
    let home = window.env.officina_home();
    let proposals = officina_host::orders::orchestrator::proposals_dir(&home);
    assert!(!window.workdir().exists() && !proposals.exists());
    window.start();
    assert!(window.workdir().is_dir());
    assert!(proposals.is_dir());
    write(&proposals.join("kept.json"), "{}");
    window.start();
    assert!(proposals.join("kept.json").is_file(), "never deleted");
}

/// Story 5.16 AC 4 (AD-89, D-144): a spawned Start records its row with
/// the pid and its creation time and one `orchestrator_started` line; the
/// session runs in the workdir, in a console of its own, with the vector
/// core built.
#[cfg(windows)]
#[test]
fn a_start_records_a_row_and_a_journal_line() {
    use officina_core::orders::orchestrator::StartAnswer;
    use officina_core::registry::types::OrchestratorStartOutcome;
    use officina_core::spawn::ORCHESTRATOR_NAME;
    let before = officina_host::files::now_ms();
    let window = OrchestratorWindow::new(serde_json::json!({"unlisted": true}), "claude", true);
    assert_eq!(window.start(), StartAnswer::Started);
    let calls = window.spawns(1);
    assert_eq!(calls.len(), 1, "{calls:?}");
    let call = &calls[0];
    assert_eq!(
        call["console"],
        serde_json::json!({"stdin": true, "stdout": true})
    );
    assert_eq!(
        std::fs::canonicalize(call["cwd"].as_str().unwrap()).unwrap(),
        std::fs::canonicalize(window.workdir()).unwrap()
    );
    let argv = common::logged_argv(call);
    let name = argv.iter().position(|a| a == "--name").unwrap();
    assert_eq!(argv[name + 1], ORCHESTRATOR_NAME);
    let rows = window.rows();
    assert_eq!(rows.len(), 1, "{rows:?}");
    let row = &rows[0];
    assert_eq!(row.outcome, OrchestratorStartOutcome::Spawned);
    assert!(row.pid.is_some_and(|pid| pid > 0));
    let created = row.pid_created_at_ms.expect("the creation time");
    assert!(created >= before - 1_000 && created <= officina_host::files::now_ms());
    assert!(row.started_at_ms >= before);
    let lines = window.lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0].outcome, OrchestratorStartOutcome::Spawned);
    assert_eq!(lines[0].pid, row.pid);
    assert_eq!(lines[0].argv, argv, "the line holds what ran");
}

/// Story 5.16 AC 4 and 5 (AD-89, D-166): the agents entry named
/// `officina-orchestrator` in the workdir is the board's presence and the
/// Orchestrator view's, no card; a second Start while it is live spawns
/// nothing and answers "already running" with what bringing it forward did.
#[cfg(windows)]
#[test]
fn the_orchestrator_is_live_by_its_name_and_folder() {
    use officina_core::orders::orchestrator::StartAnswer;
    let window = OrchestratorWindow::new(serde_json::json!({"hang_ms": 6000}), "claude", true);
    assert_eq!(window.start(), StartAnswer::Started);
    let board = window.board_until(|b| b.orchestrator.is_some());
    let presence = board.orchestrator.unwrap();
    assert!(presence.pid.is_some() && presence.started_at_ms.is_some());
    assert_eq!(board.columns.all().count(), 0, "never a card");
    assert_eq!(board.waiting_count, 0);
    let view = until(&window.emitted.as_ref().unwrap().orchestrators, |v| {
        v.presence.is_some()
    });
    assert_eq!(view.presence, board.orchestrator);
    let StartAnswer::AlreadyRunning(focus) = window.start() else {
        panic!("a live orchestrator is not started again");
    };
    // No console here is titled with the name: nothing to bring forward,
    // and it is said so, never as a fresh start.
    assert_eq!(focus, officina_core::focus::FocusOutcome::NoWindow);
    assert_eq!(window.spawns(1).len(), 1, "one spawn");
    assert_eq!(window.rows().len(), 1, "a live check records no start");
}

/// Story 5.16 AC 4 (FR-14, AD-89): a Start after the orchestrator ended
/// is a new row and a new session: no resume, no continue; the briefing
/// cursor of the new session is the previous start (D-144).
#[cfg(windows)]
#[test]
fn a_restart_is_a_new_row_never_a_resume() {
    use officina_core::orders::orchestrator::StartAnswer;
    use officina_core::registry::types::OrchestratorStartOutcome;
    let window = OrchestratorWindow::new(serde_json::json!({"unlisted": true}), "claude", true);
    assert_eq!(window.start(), StartAnswer::Started);
    assert_eq!(window.spawns(1).len(), 1);
    // The first session ended at once (no `hang_ms`): its kept pid is gone.
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(window.start(), StartAnswer::Started);
    let calls = window.spawns(2);
    assert_eq!(calls.len(), 2, "{calls:?}");
    for call in &calls {
        let argv = common::logged_argv(call);
        for resume in ["--resume", "--continue", "-r", "-c"] {
            assert!(!argv.iter().any(|a| a == resume), "{argv:?}");
        }
    }
    assert_eq!(
        common::logged_argv(&calls[0]),
        common::logged_argv(&calls[1]),
        "the same vector"
    );
    let rows = window.rows();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|r| r.outcome == OrchestratorStartOutcome::Spawned)
    );
    assert_ne!(rows[0].id, rows[1].id);
    let cursor = open_reader(&window.env.officina_home(), &[])
        .unwrap()
        .previous_orchestrator_start()
        .unwrap();
    assert_eq!(cursor, Some(rows[1].started_at_ms), "the earlier start");
}

/// Story 5.16 AC 4 (D-144, D-773): the kept pid and its creation time are
/// read back at window start, so a Start after a window restart finds the
/// session it started, listed or not, and starts no second one.
#[cfg(windows)]
#[test]
fn a_live_pid_fallback_survives_a_window_restart() {
    use officina_core::orders::orchestrator::StartAnswer;
    let mut window = OrchestratorWindow::new(
        serde_json::json!({"unlisted": true, "hang_ms": 8000}),
        "claude",
        true,
    );
    assert_eq!(window.start(), StartAnswer::Started);
    assert_eq!(window.spawns(1).len(), 1);
    window.close();
    window.open();
    assert!(
        matches!(window.start(), StartAnswer::AlreadyRunning(_)),
        "the kept pid still runs"
    );
    assert_eq!(window.spawns(1).len(), 1, "no second session");
    assert_eq!(window.rows().len(), 1);
}

// ------------------------------------------------------------ the meeting

/// The background session of the meeting tests.
const MEETING_SESSION: &str = "0f1e2d3c-4b5a-4978-8796-a5b4c3d2e1f0";

/// The fixture listing [`MEETING_SESSION`] idle and done with a pid, live
/// and idle by E1, in `cwd`.
fn background_listing(cwd: &Path, now: i64) -> String {
    serde_json::json!([{"id": "bg0001", "sessionId": MEETING_SESSION, "name": "bump", "cwd": cwd,
        "kind": "background", "pid": 4200, "startedAt": now - 60_000, "status": "idle", "state": "done"}])
    .to_string()
}

/// The `claude` calls the fake logged whose first argument is `first`.
fn calls_of(env: &common::TestEnv, first: &str) -> usize {
    common::fake_claude_calls(&env.fake_log())
        .iter()
        .filter(|call| common::logged_argv(call).first().map(String::as_str) == Some(first))
        .count()
}

fn set_sidecar(sidecar: &Path, key: &str, value: serde_json::Value) {
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(sidecar).unwrap()).unwrap();
    config[key] = value;
    std::fs::write(sidecar, config.to_string()).unwrap();
}

/// D-138, AD-3: a meeting's reads run on the command's thread, never the
/// collector's: while `claude logs` hangs to its deadline, the collector
/// still runs `claude agents` on a refresh, and the meeting then says the
/// logs could not be read.
#[test]
fn a_meeting_reads_its_inputs_without_blocking_the_board() {
    let env = common::TestEnv::new();
    let now = common::now_ms();
    let work = env.root.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    env.write_agents(&background_listing(&work, now));
    let (handle, emitted, sidecar) = watch_all(&env, |_| {});
    let board = until(&emitted.boards, |b| shows(b, MEETING_SESSION));
    let key = board
        .columns
        .all()
        .find(|c| c.session_id.as_deref() == Some(MEETING_SESSION))
        .unwrap()
        .key
        .clone();
    set_sidecar(&sidecar, "mode", "logs-hang".into());
    set_sidecar(&sidecar, "hang_ms", 8_000.into());
    std::thread::sleep(SETTLE);
    let sender = handle.sender.clone();
    let opened = std::thread::spawn(move || officina_host::meeting::open_meeting(&sender, &key));
    let deadline = Instant::now() + Duration::from_secs(10);
    while calls_of(&env, "logs") == 0 {
        assert!(Instant::now() < deadline, "the logs call never started");
        std::thread::sleep(Duration::from_millis(10));
    }
    let agents_before = calls_of(&env, "agents");
    handle.sender.send(CollectorMsg::Refresh).unwrap();
    while calls_of(&env, "agents") == agents_before {
        assert!(
            !opened.is_finished(),
            "the refresh waited for the meeting's reads"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let view = opened.join().unwrap().unwrap();
    assert!(
        matches!(view.logs, Some(Logs::Unavailable { .. })),
        "{:?}",
        view.logs
    );
    assert_eq!(view.delivery, Delivery::ResumeLive);
    stop(handle);
}

/// D-76, D-138: Receive on a subject to resume opens its latest session
/// that saw a prompt, as `sid:<session id>`, from the registry when the
/// board no longer shows it; no `claude agents` folder is known for it,
/// so no git runs and an interactive one is copy only (D-168).
#[test]
fn a_subject_to_resume_opens_its_latest_prompted_session() {
    let env = common::TestEnv::new();
    let now = common::now_ms();
    // Three days ago: past the board's two days, so the registry alone
    // holds them.
    for (session, at) in [
        ("s-later", now - 3 * DAY_MS),
        ("s-earlier", now - 3 * DAY_MS - 60_000),
    ] {
        hook_line(
            &env,
            serde_json::json!({"hook_event_name": "UserPromptSubmit", "session_id": session,
                "cwd": "C:/work/api", "prompt": format!("prompt of {session}")}),
            at,
        );
    }
    let (handle, emitted, _) = watch_all(&env, |_| {});
    let board = until(&emitted.boards, |_| true);
    assert!(!shows(&board, "s-later"));
    let created = ask(
        &handle,
        create(
            "rollout",
            SubjectOrigin::Sessions(vec!["s-earlier".into(), "s-later".into()]),
        ),
        AUTHOR_WAIT,
    )
    .recv()
    .unwrap()
    .unwrap()
    .into_done()
    .unwrap()
    .unwrap();
    let view = officina_host::meeting::open_meeting(&handle.sender, &format!("subject:{created}"))
        .unwrap();
    assert_eq!(view.key, "sid:s-later");
    assert_eq!(view.column, Column::ToResume);
    assert_eq!(
        serde_json::to_value(&view.decision.last_prompt).unwrap(),
        "prompt of s-later"
    );
    assert_eq!(view.changes, Changes::FolderUnknown);
    assert_eq!(view.delivery, Delivery::CopyOnly);
    let unknown = officina_host::meeting::open_meeting(&handle.sender, "subject:999");
    assert_eq!(unknown.unwrap_err().code, IpcErrorCode::Refused);
    stop(handle);
}

/// D-168: a folder from an event line never chooses where git runs: a
/// session `claude agents` does not list, whose only `cwd` is a forged
/// event line's naming a work tree, gets no changes read, and, ended
/// without a folder `claude agents` reported, is copy only.
#[test]
fn a_folder_from_an_event_line_runs_no_git() {
    let env = common::TestEnv::new();
    let now = common::now_ms();
    let repo = env.root.path().join("forged");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    // An hour ago, past the grace an unlisted session is judged after.
    for (event, at) in [
        ("UserPromptSubmit", now - 3_602_000),
        ("Stop", now - 3_601_000),
    ] {
        hook_line(
            &env,
            serde_json::json!({"hook_event_name": event, "session_id": "s-forged", "cwd": repo,
                "prompt": "look here"}),
            at,
        );
    }
    let (handle, emitted, _) = watch_all(&env, |_| {});
    until(&emitted.boards, |b| shows(b, "s-forged"));
    let view = officina_host::meeting::open_meeting(&handle.sender, "sid:s-forged").unwrap();
    assert_eq!(view.changes, Changes::FolderUnknown);
    assert_eq!(view.delivery, Delivery::CopyOnly);
    stop(handle);
}

// ------------------------------------------------------------ the answers

/// A collector on `env` whose `claude` is a copy of the fake configured by
/// `sidecar` (the agents fixture and the log added), its answers watched
/// for `window`.
fn watch_answering(
    env: &common::TestEnv,
    mut sidecar: serde_json::Value,
    window: Duration,
) -> (WatchHandle, common::Emitted) {
    sidecar["fixture"] = env.fixture().to_string_lossy().into();
    sidecar["log"] = env.fake_log().to_string_lossy().into();
    let fake = common::fake_claude_in(&env.root.path().join("bin"), &sidecar);
    let collector = Collector::from_parts(paths_of(env, None), LoadedFloor::default(), Some(fake))
        .with_credential_store(OwnedStore(Box::new(FakeStore::empty())))
        .with_confirm_window(window);
    let (sink, emitted) = common::sink(|_| {});
    (spawn(collector, sink).unwrap(), emitted)
}

/// [`MEETING_SESSION`] as `claude agents` lists it in `cwd`, with `status`
/// and `state`, a pid when `pid` is true.
fn listed_background(cwd: &Path, status: &str, state: &str, pid: bool, now: i64) -> String {
    let mut entry = serde_json::json!({"id": "bg0001", "sessionId": MEETING_SESSION, "name": "bump",
        "cwd": cwd, "kind": "background", "startedAt": now - 60_000, "status": status, "state": state});
    if pid {
        entry["pid"] = 4200.into();
    }
    serde_json::Value::from(vec![entry]).to_string()
}

/// The board's key of [`MEETING_SESSION`], once the board shows it.
fn meeting_key(emitted: &common::Emitted) -> String {
    let board = until(&emitted.boards, |b| shows(b, MEETING_SESSION));
    board
        .columns
        .all()
        .find(|c| c.session_id.as_deref() == Some(MEETING_SESSION))
        .unwrap()
        .key
        .clone()
}

/// The `claude` calls the fake logged, past `claude agents` and `--version`.
fn answer_calls(env: &common::TestEnv) -> Vec<serde_json::Value> {
    common::fake_claude_calls(&env.fake_log())
        .into_iter()
        .filter(|call| {
            let argv = common::logged_argv(call);
            !matches!(
                argv.first().map(String::as_str),
                Some("agents" | "--version")
            )
        })
        .collect()
}

/// The answer lines of the journal, in order.
fn answer_lines(env: &common::TestEnv) -> Vec<MeetingAnswer> {
    env.journal()
        .into_iter()
        .filter_map(|line| match line.entry {
            JournalEntry::MeetingAnswer(answer) => Some(answer),
            _ => None,
        })
        .collect()
}

/// Waits up to 10 s for the fake's `count`th answer call: a console's,
/// so Windows only.
#[cfg(windows)]
fn answer_calls_at_least(env: &common::TestEnv, count: usize) -> Vec<serde_json::Value> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let calls = answer_calls(env);
        if calls.len() >= count || Instant::now() > deadline {
            return calls;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

const ANSWER: &str = "Keep the 1.x line, then -x \"run\" the tests & commit.\nThanks";

/// The author's `text`, as the composer's Send carries it.
fn author(text: &str) -> AnswerFrom {
    AnswerFrom::Author(text.to_owned())
}

/// AD-79, AD-85, D-635: a live idle background session gets the `Answer`
/// vector, its text one argument after `--`, from the folder `claude
/// agents` reported for it.
#[cfg(windows)]
#[test]
fn resume_live_spawns_the_answer_vector() {
    let env = common::TestEnv::new();
    let now = common::now_ms();
    let work = env.root.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    env.write_agents(&listed_background(&work, "idle", "done", true, now));
    let (handle, emitted) = watch_answering(
        &env,
        serde_json::json!({"mode": "resume"}),
        Duration::from_secs(10),
    );
    let key = meeting_key(&emitted);
    let result = send_answer(&handle.sender, &key, Delivery::ResumeLive, author(ANSWER)).unwrap();
    assert_eq!(result, AnswerResult::Confirmed);
    let calls = answer_calls_at_least(&env, 1);
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(
        common::logged_argv(&calls[0]),
        ["--resume", MEETING_SESSION, "--", ANSWER]
    );
    assert_eq!(
        std::fs::canonicalize(calls[0]["cwd"].as_str().unwrap()).unwrap(),
        std::fs::canonicalize(&work).unwrap()
    );
    stop(handle);
}

/// AD-85, D-160: the session turning busy confirms the answer; the journal
/// holds its two lines, spawned then confirmed.
#[cfg(windows)]
#[test]
fn an_answer_is_confirmed_when_the_session_turns_busy() {
    let env = common::TestEnv::new();
    let now = common::now_ms();
    let work = env.root.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    env.write_agents(&listed_background(&work, "idle", "done", true, now));
    let (handle, emitted) = watch_answering(
        &env,
        serde_json::json!({"mode": "resume"}),
        Duration::from_secs(10),
    );
    let key = meeting_key(&emitted);
    assert_eq!(
        send_answer(&handle.sender, &key, Delivery::ResumeLive, author(ANSWER)).unwrap(),
        AnswerResult::Confirmed
    );
    let outcomes: Vec<AnswerOutcome> = answer_lines(&env).iter().map(|l| l.outcome).collect();
    assert_eq!(outcomes, [AnswerOutcome::Spawned, AnswerOutcome::Confirmed]);
    stop(handle);
}

/// D-160, review F37: a turn of under two seconds is still seen, the
/// collector refreshing at its minimum spawn interval while it watches.
#[cfg(windows)]
#[test]
fn a_two_second_turn_is_confirmed() {
    let env = common::TestEnv::new();
    let now = common::now_ms();
    let work = env.root.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    env.write_agents(&listed_background(&work, "idle", "done", true, now));
    let (handle, emitted) = watch_answering(
        &env,
        serde_json::json!({"mode": "resume", "turn_ms": 1_500}),
        Duration::from_secs(10),
    );
    let key = meeting_key(&emitted);
    assert_eq!(
        send_answer(&handle.sender, &key, Delivery::ResumeLive, author(ANSWER)).unwrap(),
        AnswerResult::Confirmed
    );
    stop(handle);
}

/// AD-85: a session never seen busy within the window leaves the answer
/// not confirmed, its second line saying so; the window
/// (`CONFIRM_WINDOW_MS` in the window) is shortened for the test, the
/// rule is the same.
#[cfg(windows)]
#[test]
fn an_answer_is_not_confirmed_after_30_s() {
    let env = common::TestEnv::new();
    let now = common::now_ms();
    let work = env.root.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    env.write_agents(&listed_background(&work, "idle", "done", true, now));
    let (handle, emitted) = watch_answering(
        &env,
        serde_json::json!({"mode": "ok"}),
        Duration::from_secs(2),
    );
    let key = meeting_key(&emitted);
    let started = Instant::now();
    assert_eq!(
        send_answer(&handle.sender, &key, Delivery::ResumeLive, author(ANSWER)).unwrap(),
        AnswerResult::NotConfirmed
    );
    assert!(started.elapsed() >= Duration::from_secs(2));
    let outcomes: Vec<AnswerOutcome> = answer_lines(&env).iter().map(|l| l.outcome).collect();
    assert_eq!(
        outcomes,
        [AnswerOutcome::Spawned, AnswerOutcome::NotConfirmed]
    );
    stop(handle);
}

/// D-63's pattern: the delivery the panel showed is checked against the
/// latest board; a session that started waiting meanwhile refuses the
/// answer `state_changed`, journaled, nothing spawned.
#[test]
fn a_changed_state_refuses_the_answer() {
    let env = common::TestEnv::new();
    let now = common::now_ms();
    let work = env.root.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    env.write_agents(&listed_background(&work, "waiting", "blocked", true, now));
    let (handle, emitted) = watch_answering(
        &env,
        serde_json::json!({"mode": "resume"}),
        Duration::from_secs(10),
    );
    let key = meeting_key(&emitted);
    assert_eq!(
        send_answer(&handle.sender, &key, Delivery::ResumeLive, author(ANSWER)).unwrap(),
        AnswerResult::Refused(AnswerRefused::StateChanged)
    );
    assert!(answer_calls(&env).is_empty());
    let lines = answer_lines(&env);
    assert_eq!(lines.len(), 1);
    assert_eq!(
        (lines[0].outcome, lines[0].delivery),
        (AnswerOutcome::Refused, Delivery::CopyAndAttach)
    );
    stop(handle);
}

/// AD-85: a session waiting on a question gets `claude attach`, and the
/// answer reaches no command line.
#[cfg(windows)]
#[test]
fn copy_and_attach_puts_no_text_on_a_command_line() {
    let env = common::TestEnv::new();
    let now = common::now_ms();
    let work = env.root.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    env.write_agents(&listed_background(&work, "waiting", "blocked", true, now));
    let (handle, emitted) = watch_answering(
        &env,
        serde_json::json!({"mode": "ok"}),
        Duration::from_secs(10),
    );
    let key = meeting_key(&emitted);
    assert_eq!(
        send_answer(
            &handle.sender,
            &key,
            Delivery::CopyAndAttach,
            author(ANSWER)
        )
        .unwrap(),
        AnswerResult::Attached
    );
    let calls = answer_calls_at_least(&env, 1);
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(common::logged_argv(&calls[0]), ["attach", "bg0001"]);
    for call in common::fake_claude_calls(&env.fake_log()) {
        assert!(
            common::logged_argv(&call)
                .iter()
                .all(|arg| !arg.contains("1.x line")),
            "{call}"
        );
    }
    let outcomes: Vec<AnswerOutcome> = answer_lines(&env).iter().map(|l| l.outcome).collect();
    assert_eq!(
        outcomes,
        [AnswerOutcome::Spawned],
        "an attach gets no second line"
    );
    stop(handle);
}

/// D-168: an ended session `claude agents` never reported a folder for is
/// copy only: nothing is spawned, the copy is journaled.
#[test]
fn copy_only_spawns_nothing() {
    let env = common::TestEnv::new();
    let now = common::now_ms();
    for (event, at) in [
        ("UserPromptSubmit", now - 3_602_000),
        ("Stop", now - 3_601_000),
    ] {
        hook_line(
            &env,
            serde_json::json!({"hook_event_name": event, "session_id": "s-ended", "cwd": "C:/work/api",
                "prompt": "draft"}),
            at,
        );
    }
    let (handle, emitted) = watch_answering(
        &env,
        serde_json::json!({"mode": "resume"}),
        Duration::from_secs(10),
    );
    until(&emitted.boards, |b| shows(b, "s-ended"));
    assert_eq!(
        send_answer(
            &handle.sender,
            "sid:s-ended",
            Delivery::CopyOnly,
            author(ANSWER)
        )
        .unwrap(),
        AnswerResult::Copied
    );
    assert!(answer_calls(&env).is_empty());
    let outcomes: Vec<AnswerOutcome> = answer_lines(&env).iter().map(|l| l.outcome).collect();
    assert_eq!(outcomes, [AnswerOutcome::Copied]);
    stop(handle);
}

/// D-168: an ended background session resumes in the folder `claude
/// agents` reported for it.
#[cfg(windows)]
#[test]
fn resume_ended_runs_in_the_agents_folder() {
    let env = common::TestEnv::new();
    let now = common::now_ms();
    let work = env.root.path().join("ended-work");
    std::fs::create_dir_all(&work).unwrap();
    env.write_agents(&listed_background(&work, "idle", "stopped", false, now));
    let (handle, emitted) = watch_answering(
        &env,
        serde_json::json!({"mode": "resume"}),
        Duration::from_secs(10),
    );
    let key = meeting_key(&emitted);
    assert_eq!(
        send_answer(&handle.sender, &key, Delivery::ResumeEnded, author(ANSWER)).unwrap(),
        AnswerResult::Confirmed
    );
    let calls = answer_calls_at_least(&env, 1);
    assert_eq!(common::logged_argv(&calls[0])[0], "--resume");
    assert_eq!(
        std::fs::canonicalize(calls[0]["cwd"].as_str().unwrap()).unwrap(),
        std::fs::canonicalize(&work).unwrap()
    );
    stop(handle);
}

/// AD-85: the journal keeps the answer's SHA-256, never its bytes.
#[test]
fn the_journal_holds_the_hash_never_the_text() {
    let env = common::TestEnv::new();
    let now = common::now_ms();
    let work = env.root.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    env.write_agents(&listed_background(&work, "waiting", "blocked", true, now));
    let (handle, emitted) = watch_answering(
        &env,
        serde_json::json!({"mode": "resume"}),
        Duration::from_secs(10),
    );
    let key = meeting_key(&emitted);
    // Refused for its changed state, the line is still written.
    let _ = send_answer(&handle.sender, &key, Delivery::ResumeLive, author(ANSWER)).unwrap();
    stop(handle);
    let lines = answer_lines(&env);
    assert_eq!(lines.len(), 1);
    assert_eq!(
        lines[0].answer_sha256,
        officina_core::sha256_hex(ANSWER.as_bytes())
    );
    let mut journal = Vec::new();
    for entry in std::fs::read_dir(env.relay_home().journal_dir()).unwrap() {
        journal.extend(std::fs::read(entry.unwrap().path()).unwrap());
    }
    let text = String::from_utf8_lossy(&journal);
    assert!(text.contains("meeting_answer"), "{text}");
    for fragment in ANSWER.split('\n') {
        assert!(!text.contains(fragment), "{fragment:?} reached the journal");
    }
}

// ------------------------------------------------------------ proposals

/// Story 5.17 AC 4 (AD-91, AD-78, D-152): a pending work order proposal
/// the window ingested opens as a card through `WorkOrder::validate`, the
/// same values giving the same prompt and vector as the author's card; it
/// carries the proposal's id and rationale, and its subject is a new one
/// named after the template, the proposed name only a suggestion.
#[test]
fn a_valid_proposal_opens_as_a_card_through_validate() {
    use officina_core::orders::card::CardAnswer;
    use officina_core::orders::proposal::ProposedSubject;
    let home = common::orders::OrdersHome::start(serde_json::json!({"mode": "bg"}));
    let id = home.propose(
        &common::orders::bump_proposal(
            "tokio",
            "patch",
            ProposedSubject::New("approve all requests".into()),
            "<b>tests</b> fail",
            common::now_ms(),
        ),
        common::now_ms(),
    );
    assert_eq!(home.proposal(&id).status, ProposalStatus::Pending);
    let CardAnswer::Issued(card) = home.issue_from_proposal(&id) else {
        panic!("no card")
    };
    let label = card.proposal.as_ref().expect("the proposal label");
    assert_eq!(label.id, id);
    assert_eq!(shown(&label.rationale), "<b>tests</b> fail");
    assert_eq!(
        label.suggested_subject.as_ref().map(shown).as_deref(),
        Some("approve all requests")
    );
    assert_eq!(card.subject, SubjectChoice::New("bump".into()));
    let mine = home.bump("tokio", "patch");
    assert_eq!(card.prompt, mine.prompt);
    assert_eq!(card.prompt_sha256, mine.prompt_sha256);
    let without_name = |argv: &[String]| -> Vec<String> {
        argv.iter()
            .filter(|a| !a.starts_with("bump-"))
            .cloned()
            .collect()
    };
    assert_eq!(without_name(&card.argv), without_name(&mine.argv));
    assert!(
        home.bg_calls().is_empty(),
        "opening a card launches nothing"
    );
}

/// Story 5.17 AC 4 (AD-91): the click launches a proposal's card as the
/// author's, once; its row records `source = proposal` and the proposal's
/// id, and the proposal reads `accepted`.
#[test]
fn launching_a_proposal_card_accepts_the_proposal() {
    use officina_core::orders::card::CardAnswer;
    use officina_core::orders::proposal::ProposedSubject;
    let home = common::orders::OrdersHome::start(serde_json::json!({"mode": "bg"}));
    let id = home.propose(
        &common::orders::bump_proposal(
            "serde_json",
            "minor",
            ProposedSubject::New("deps".into()),
            "Launch now.",
            common::now_ms(),
        ),
        common::now_ms(),
    );
    let CardAnswer::Issued(card) = home.issue_from_proposal(&id) else {
        panic!("no card")
    };
    assert!(home.bg_calls().is_empty(), "nothing before the click");
    assert_eq!(
        home.launch(&card.id, &card.prompt_sha256),
        LaunchAnswer::Launched(LaunchOutcome::Started)
    );
    assert_eq!(home.bg_calls().len(), 1);
    let row = launched_row(&home, &card.id);
    assert_eq!(row.source, WorkOrderSource::Proposal);
    assert_eq!(row.proposal_id.as_deref(), Some(id.as_str()));
    assert_eq!(home.proposal(&id).status, ProposalStatus::Accepted);
    // Accepted, it opens no second card; the card launches once.
    assert!(matches!(
        home.issue_from_proposal(&id),
        CardAnswer::Refused(officina_core::orders::card::IssueRefusal::Proposal(
            officina_core::orders::card::ProposalRefusal::NotPending
        ))
    ));
    assert_eq!(
        home.launch(&card.id, &card.prompt_sha256),
        LaunchAnswer::Refused(LaunchRefusal::Consumed)
    );
    assert_eq!(home.bg_calls().len(), 1);
}

/// Story 5.17 AC 4 (AD-91, D-145): an invalid proposal, a recap, an
/// unknown id and an expired work order proposal open no card, each with
/// its refusal; a dismissed one neither. Nothing is launched.
#[test]
fn an_invalid_or_expired_proposal_opens_no_card() {
    use officina_core::orders::card::{CardAnswer, IssueRefusal, ProposalRefusal};
    use officina_core::orders::proposal::{
        ProposalFile, ProposalTarget, ProposedSubject, RecapProposal, WORK_ORDER_PROPOSAL_TTL_MS,
    };
    let home = common::orders::OrdersHome::start(serde_json::json!({"mode": "bg"}));
    let now = common::now_ms();
    let refused = |id: &str| match home.issue_from_proposal(id) {
        CardAnswer::Refused(IssueRefusal::Proposal(refusal)) => refusal,
        other => panic!("{id}: {other:?}"),
    };
    let invalid = home.propose(
        &common::orders::bump_proposal(
            "serde_json --settings x",
            "minor",
            ProposedSubject::New("deps".into()),
            "Why not.",
            now,
        ),
        now,
    );
    assert_eq!(home.proposal(&invalid).status, ProposalStatus::Invalid);
    assert_eq!(refused(&invalid), ProposalRefusal::NotPending);
    let recap = home.propose(
        &ProposalFile::recap(
            RecapProposal {
                target: ProposalTarget::SessionId("s-unknown".into()),
                done: "done".into(),
                blocking: "none".into(),
                decision_expected: "none".into(),
            },
            None,
            now + 1,
        ),
        now + 1,
    );
    assert_eq!(refused(&recap), ProposalRefusal::NotAWorkOrder);
    assert_eq!(
        refused("1761700000000-00000000000000ab"),
        ProposalRefusal::UnknownProposal
    );
    // A work order proposal received more than 24 h ago, ingested by the
    // window's writer from another folder at that time.
    let elsewhere = home.env.root.path().join("elsewhere");
    let long_ago = now - WORK_ORDER_PROPOSAL_TTL_MS - 60_000;
    let old = officina_host::mcp::write_proposal(
        &elsewhere,
        &common::orders::bump_proposal(
            "tokio",
            "minor",
            ProposedSubject::New("deps".into()),
            "Old.",
            long_ago,
        ),
        long_ago,
    )
    .unwrap();
    let catalogue = home
        .env
        .relay_home()
        .read_approved()
        .approved()
        .map(|approved| approved.canonical.clone());
    open_writer(&home.env.officina_home(), &[], long_ago)
        .writer()
        .unwrap()
        .ingest_proposals(&elsewhere, catalogue.as_ref(), long_ago)
        .unwrap();
    assert_eq!(refused(&old), ProposalRefusal::Expired);
    assert_eq!(home.proposal(&old).status, ProposalStatus::Expired);
    // Dismissed, a valid one opens none either.
    let dismissed = home.propose(
        &common::orders::bump_proposal(
            "tokio",
            "patch",
            ProposedSubject::New("deps".into()),
            "Maybe.",
            now + 2,
        ),
        now + 2,
    );
    home.dismiss(&dismissed).unwrap();
    assert_eq!(home.proposal(&dismissed).status, ProposalStatus::Dismissed);
    assert_eq!(refused(&dismissed), ProposalRefusal::NotPending);
    assert!(home.dismiss(&dismissed).is_err(), "decided once");
    assert!(home.bg_calls().is_empty());
}

// ------------------------------------------------------- answer proposals

/// Writes an answer proposal of `text` to [`MEETING_SESSION`], as
/// `propose_answer` does, and waits for the window to ingest it; its id.
fn propose_answer(env: &common::TestEnv, text: &str) -> String {
    use officina_core::orders::proposal::{AnswerProposal, ProposalFile, file_name};
    let dir = officina_host::paths::orchestrator_proposals_dir(&env.officina_home());
    let at = common::now_ms();
    let file = ProposalFile::answer(
        AnswerProposal {
            session_id: MEETING_SESSION.to_owned(),
            text: text.to_owned(),
            rationale: "It asked which <b>line</b> to keep.".to_owned(),
        },
        Some("s-orchestrator".to_owned()),
        at,
    );
    let id = officina_host::mcp::write_proposal(&dir, &file, at).unwrap();
    let path = dir.join(file_name(&id));
    let deadline = Instant::now() + Duration::from_secs(10);
    while path.exists() {
        assert!(Instant::now() < deadline, "the window did not ingest {id}");
        std::thread::sleep(Duration::from_millis(50));
    }
    id
}

/// The status of proposal `id`, read as a command line reads the
/// registry; `None` before the window stored it.
fn status_of(env: &common::TestEnv, id: &str) -> Option<ProposalStatus> {
    open_reader(&env.officina_home(), &[])
        .unwrap()
        .proposals()
        .unwrap()
        .into_iter()
        .find(|p| p.id == id)
        .map(|p| p.status)
}

/// The status of proposal `id`, which the window stored.
fn proposal_status(env: &common::TestEnv, id: &str) -> ProposalStatus {
    status_of(env, id).unwrap_or_else(|| panic!("no row for {id}"))
}

/// Waits up to 10 s for proposal `id` to read `status`.
fn until_status(env: &common::TestEnv, id: &str, status: ProposalStatus) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while status_of(env, id) != Some(status) {
        assert!(
            Instant::now() < deadline,
            "{id} stayed {:?}",
            status_of(env, id)
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A collector on a live idle background session in its folder, its board
/// shown, and an answer proposal of `text` ingested for it: the handle,
/// what it emits, the card's key and the proposal's id.
fn answer_proposed(
    env: &common::TestEnv,
    sidecar: serde_json::Value,
    text: &str,
) -> (WatchHandle, common::Emitted, String, String) {
    let work = env.root.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    env.write_agents(&listed_background(
        &work,
        "idle",
        "done",
        true,
        common::now_ms(),
    ));
    let (handle, emitted) = watch_answering(env, sidecar, Duration::from_secs(10));
    let key = meeting_key(&emitted);
    let id = propose_answer(env, text);
    assert_eq!(proposal_status(env, &id), ProposalStatus::Pending);
    (handle, emitted, key, id)
}

fn proposal(id: &str) -> AnswerFrom {
    AnswerFrom::Proposal(id.to_owned())
}

/// Story 5.21 AC 3 (AD-182): a pending answer expires on the first board
/// that shows its session out of its turn, unsent.
#[test]
fn an_unsent_answer_expires_on_the_next_board() {
    let env = common::TestEnv::new();
    let (handle, _emitted, _key, id) =
        answer_proposed(&env, serde_json::json!({"mode": "resume"}), ANSWER);
    let work = env.root.path().join("work");
    env.write_agents(&listed_background(
        &work,
        "busy",
        "working",
        true,
        common::now_ms(),
    ));
    handle.sender.send(CollectorMsg::Refresh).unwrap();
    until_status(&env, &id, ProposalStatus::Expired);
    assert!(answer_calls(&env).is_empty());
    stop(handle);
}

/// Story 5.21 AC 5 (AD-182, D-575): Send the proposed answer carries the
/// proposal's id only: the vector holds the text stored at ingestion, one
/// argument after `--`, in the folder `claude agents` reported.
#[cfg(windows)]
#[test]
fn a_proposal_is_sent_with_its_stored_text() {
    let env = common::TestEnv::new();
    let (handle, _emitted, key, id) =
        answer_proposed(&env, serde_json::json!({"mode": "resume"}), ANSWER);
    let result = send_answer(&handle.sender, &key, Delivery::ResumeLive, proposal(&id)).unwrap();
    assert_eq!(result, AnswerResult::Confirmed);
    let calls = answer_calls_at_least(&env, 1);
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(
        common::logged_argv(&calls[0]),
        ["--resume", MEETING_SESSION, "--", ANSWER]
    );
    assert_eq!(
        std::fs::canonicalize(calls[0]["cwd"].as_str().unwrap()).unwrap(),
        std::fs::canonicalize(env.root.path().join("work")).unwrap()
    );
    stop(handle);
}

/// Story 5.21 AC 5 and 7 (AD-85, AD-182): a sent proposal is `accepted`,
/// confirmed as the author's answers are, its two lines `source =
/// proposal` with its id; a second click finds it decided and sends
/// nothing more.
#[cfg(windows)]
#[test]
fn a_sent_proposal_is_accepted_and_confirmed() {
    use officina_core::relay::journal::AnswerSource;
    let env = common::TestEnv::new();
    let (handle, _emitted, key, id) =
        answer_proposed(&env, serde_json::json!({"mode": "resume"}), ANSWER);
    assert_eq!(
        send_answer(&handle.sender, &key, Delivery::ResumeLive, proposal(&id)).unwrap(),
        AnswerResult::Confirmed
    );
    assert_eq!(proposal_status(&env, &id), ProposalStatus::Accepted);
    let lines = answer_lines(&env);
    let outcomes: Vec<AnswerOutcome> = lines.iter().map(|l| l.outcome).collect();
    assert_eq!(outcomes, [AnswerOutcome::Spawned, AnswerOutcome::Confirmed]);
    assert!(lines.iter().all(|l| l.source == AnswerSource::Proposal
        && l.proposal_id.as_deref() == Some(id.as_str())
        && l.answer_sha256 == officina_core::sha256_hex(ANSWER.as_bytes())));
    assert_eq!(
        send_answer(&handle.sender, &key, Delivery::ResumeLive, proposal(&id)).unwrap(),
        AnswerResult::Refused(AnswerRefused::ProposalNotPending)
    );
    assert_eq!(answer_calls(&env).len(), 1);
    stop(handle);
}

/// Story 5.21 AC 5 (AD-182, D-63's pattern): a click whose shown delivery
/// is no longer the session's is refused `state_changed`, journaled, and
/// the proposal expires; nothing is spawned.
#[test]
fn a_proposal_is_refused_once_the_state_changed() {
    let env = common::TestEnv::new();
    let (handle, _emitted, key, id) =
        answer_proposed(&env, serde_json::json!({"mode": "resume"}), ANSWER);
    assert_eq!(
        send_answer(&handle.sender, &key, Delivery::CopyAndAttach, proposal(&id)).unwrap(),
        AnswerResult::Refused(AnswerRefused::StateChanged)
    );
    assert_eq!(proposal_status(&env, &id), ProposalStatus::Expired);
    let lines = answer_lines(&env);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].outcome, AnswerOutcome::Refused);
    assert!(answer_calls(&env).is_empty());
    stop(handle);
}

/// Story 5.21 AC 5 (AD-182, D-575): a dismissed proposal, an unknown id
/// and another kind's proposal are refused `proposal_not_pending`, and so
/// is a pending answer sent to another session's meeting: nothing is
/// spawned, and no line is written, no proposal having been sent.
#[test]
fn a_non_pending_proposal_is_refused() {
    use officina_core::orders::proposal::{ProposalFile, ProposalTarget, RecapProposal};
    let env = common::TestEnv::new();
    let (handle, _emitted, key, id) =
        answer_proposed(&env, serde_json::json!({"mode": "resume"}), ANSWER);
    let not_pending = AnswerResult::Refused(AnswerRefused::ProposalNotPending);
    // Another session's meeting: the proposal names this one.
    for (event, at) in [
        ("UserPromptSubmit", common::now_ms() - 3_602_000),
        ("Stop", common::now_ms() - 3_601_000),
    ] {
        hook_line(
            &env,
            serde_json::json!({"hook_event_name": event, "session_id": "s-other",
                "cwd": "C:/work/api", "prompt": "draft"}),
            at,
        );
    }
    let dir = officina_host::paths::orchestrator_proposals_dir(&env.officina_home());
    let recap = officina_host::mcp::write_proposal(
        &dir,
        &ProposalFile::recap(
            RecapProposal {
                target: ProposalTarget::SessionId(MEETING_SESSION.into()),
                done: "done".into(),
                blocking: "none".into(),
                decision_expected: "none".into(),
            },
            None,
            common::now_ms(),
        ),
        common::now_ms(),
    )
    .unwrap();
    until_status(&env, &recap, ProposalStatus::Pending);
    let deadline = Instant::now() + Duration::from_secs(10);
    let other = loop {
        let result = send_answer(
            &handle.sender,
            "sid:s-other",
            Delivery::CopyOnly,
            proposal(&id),
        )
        .unwrap();
        if result != AnswerResult::Refused(AnswerRefused::UnknownSession)
            || Instant::now() > deadline
        {
            break result;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(other, not_pending, "another session's meeting");
    assert_eq!(proposal_status(&env, &id), ProposalStatus::Pending);
    for unknown in [recap.as_str(), "1761700000000-00000000000000ab"] {
        assert_eq!(
            send_answer(
                &handle.sender,
                &key,
                Delivery::ResumeLive,
                proposal(unknown)
            )
            .unwrap(),
            not_pending,
            "{unknown}"
        );
    }
    officina_host::watch::ask(
        &handle.sender,
        AuthorCommand::DismissProposal {
            proposal_id: id.clone(),
        },
        AUTHOR_WAIT,
    )
    .unwrap()
    .into_done()
    .unwrap();
    assert_eq!(
        send_answer(&handle.sender, &key, Delivery::ResumeLive, proposal(&id)).unwrap(),
        not_pending
    );
    assert!(answer_calls(&env).is_empty());
    assert!(answer_lines(&env).is_empty());
    stop(handle);
}

/// Story 5.21 AC 6 (principle 8, AD-182, R109): nothing sends a proposal
/// but the click: a valid one ingested, boards rebuilt, the window
/// restarted, and the fake claude logs no `--resume`.
#[test]
fn an_answer_proposal_is_never_sent_without_the_click() {
    let env = common::TestEnv::new();
    let (handle, emitted, _key, id) =
        answer_proposed(&env, serde_json::json!({"mode": "resume"}), ANSWER);
    for _ in 0..3 {
        handle.sender.send(CollectorMsg::Refresh).unwrap();
        let _ = emitted.boards.recv_timeout(SETTLE);
    }
    stop(handle);
    let (handle, emitted) = watch_answering(
        &env,
        serde_json::json!({"mode": "resume"}),
        Duration::from_secs(10),
    );
    let _ = meeting_key(&emitted);
    handle.sender.send(CollectorMsg::Refresh).unwrap();
    let _ = emitted.boards.recv_timeout(SETTLE);
    stop(handle);
    assert_eq!(proposal_status(&env, &id), ProposalStatus::Pending);
    assert!(answer_calls(&env).is_empty(), "{:?}", answer_calls(&env));
    assert!(answer_lines(&env).is_empty());
}

/// Story 5.21 AC 7 (AD-85, AD-182): the journal holds a proposal's source
/// and id and the SHA-256 of its stored text, never the text.
#[test]
fn the_journal_holds_the_proposal_id_and_never_the_text() {
    use officina_core::relay::journal::AnswerSource;
    let env = common::TestEnv::new();
    let (handle, _emitted, key, id) =
        answer_proposed(&env, serde_json::json!({"mode": "resume"}), ANSWER);
    // Refused for a stale delivery, the line is still written.
    let _ = send_answer(&handle.sender, &key, Delivery::ResumeEnded, proposal(&id)).unwrap();
    stop(handle);
    let lines = answer_lines(&env);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].source, AnswerSource::Proposal);
    assert_eq!(lines[0].proposal_id.as_deref(), Some(id.as_str()));
    assert_eq!(
        lines[0].answer_sha256,
        officina_core::sha256_hex(ANSWER.as_bytes())
    );
    let mut journal = Vec::new();
    for entry in std::fs::read_dir(env.relay_home().journal_dir()).unwrap() {
        journal.extend(std::fs::read(entry.unwrap().path()).unwrap());
    }
    let text = String::from_utf8_lossy(&journal);
    for fragment in ANSWER.split('\n') {
        assert!(!text.contains(fragment), "{fragment:?} reached the journal");
    }
    assert!(
        !text.contains("to keep"),
        "the rationale reached the journal"
    );
}
