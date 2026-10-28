//! The window's collector thread end to end (stories 1.5 and 2.7, AD-3 to
//! AD-5, AD-35): first board, re-emission on a hook event, no re-emission
//! when nothing changed; the floor reloaded with the paths it names; the
//! relay view rebuilt on the relay's files, the journal and `RelayChanged`,
//! emitted on change only; the known-projects record kept current.
//!
//! The fake claude is given no fixture here (it then lists no session):
//! classification of real fixtures is covered by the `board` CLI tests, and
//! passing a fixture would need a process-wide environment variable.

mod common;
#[path = "../../../tests/support/fake_store.rs"]
mod fake_store;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use officina_core::board::Board;
use officina_core::relay::journal::{CatalogueApproved, JournalEntry};
use officina_core::relay::view::{RecentEvent, RelayView};
use officina_host::collector::{Collector, LoadedFloor, load_floor};
use officina_host::paths::Paths;
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
    let mut collector =
        Collector::from_parts(paths, loaded, Some(PathBuf::from(common::FAKE_CLAUDE)));
    collector.credentials = OwnedStore(Box::new(store));
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
