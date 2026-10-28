//! `officina relay --json` (story 2.6, AD-35, D-24, D-25): the relay view the
//! window shows, printed from a real home. Journals are seeded line by line,
//! as fixtures seed them, so no test depends on a program running.

mod common;

use std::collections::BTreeMap;
use std::process::{Command, Output, Stdio};

use common::relay::Relay;
use common::{TestEnv, fingerprint, now_ms};
use officina_core::relay::catalogue::ActionClass;
use officina_core::relay::gate::DenyKind;
use officina_core::relay::journal::{
    GateDenied, JournalEntry, RefusalReason, Refused, RunFinished, RunOutcome, RunStarted,
    StreamSummary,
};
use officina_core::relay::sha256_hex;
use officina_core::relay::view::{ABANDONED_GRACE_MS, RecentEvent, RelayView, RunState};
use officina_host::relay::home::load_relay_floor;
use officina_host::relay::journal::append;

const FLOOR: &str = r#"schema = 2

[[project]]
name = "lab"
path = "Lab"
actions = ["probe"]

[[action]]
name = "probe"
description = "Reads one host"
class = "free"
program = "{tool}"
args = ["--host", "{host}"]

[action.params.host]
values = ["web01", "db01"]
"#;

fn approved_env() -> TestEnv {
    Relay::approved(FLOOR).env
}

fn relay_json(env: &TestEnv) -> Output {
    env.run(&["relay", "--json"], b"")
}

fn view(env: &TestEnv) -> RelayView {
    let out = relay_json(env);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn prints_the_relay_view() {
    let env = approved_env();
    let approved = env.relay_home().read_approved().approved().unwrap().clone();
    let shown = view(&env);
    assert_eq!(shown.schema, officina_core::relay::view::RELAY_VIEW_SCHEMA);
    assert_eq!(shown.approved_hash.as_deref(), Some(approved.hash.as_str()));
    assert_eq!(shown.approved_at_ms, Some(approved.approved_at_ms));
    assert_eq!(shown.pending, None);
    assert_eq!(
        shown.floor.as_deref(),
        Some(env.floor_file().to_string_lossy().as_ref())
    );
    assert!(!shown.stopped && shown.stopped_since_ms.is_none());
    assert!(matches!(
        shown.recent[0].event,
        RecentEvent::CatalogueApproved(_)
    ));
    assert_eq!((shown.journal_corrupt_lines, shown.gate_errors), (0, 0));

    std::fs::write(
        env.relay_home().stopped_file(),
        br#"{"since_ms":1793102400000}"#,
    )
    .unwrap();
    let stopped = view(&env);
    assert!(stopped.stopped);
    assert_eq!(stopped.stopped_since_ms, Some(1_793_102_400_000));
    std::fs::write(env.relay_home().stopped_file(), b"").unwrap();
    let unreadable = view(&env);
    assert!(
        unreadable.stopped && unreadable.stopped_since_ms.is_none(),
        "any flag stops the relay (AD-34)"
    );

    let fresh = TestEnv::new();
    let empty = view(&fresh);
    assert_eq!(
        (empty.floor, empty.approved_hash, empty.pending),
        (None, None, None)
    );
    assert!(empty.recent.is_empty());
}

#[test]
fn catalogue_is_the_approved_snapshot() {
    let env = approved_env();
    let approved = env.relay_home().read_approved().approved().unwrap().clone();
    env.write_floor(&FLOOR.replace("\"web01\", \"db01\"", "\"web01\""));
    let shown = view(&env);
    assert_eq!(shown.catalogue, Some(approved.canonical));
    assert_eq!(shown.approved_hash, Some(approved.hash));
}

#[test]
fn floor_edit_shows_a_pending_diff() {
    let env = approved_env();
    env.write_floor(&FLOOR.replace("\"web01\", \"db01\"", "\"web01\""));
    let current = load_relay_floor(&env.floor_file()).unwrap();
    let pending = view(&env).pending.expect("a pending change");
    assert_eq!(pending.hash, current.hash);
    let changed = &pending.diff.actions.changed;
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].name, "probe");
    assert_eq!(changed[0].fields[0].field, "params");
    assert!(pending.diff.projects.changed.is_empty());

    let unapproved = Relay::unapproved(FLOOR).env;
    let first = view(&unapproved).pending.expect("a first catalogue waits");
    assert_eq!(first.diff.actions.added, ["probe"]);
}

#[test]
fn issues_carry_their_lines() {
    let env = Relay::unapproved(&format!(
        "{FLOOR}\n[[action]]\nname = \"broken\"\nclass = \"free\"\nprogram = \"relative.exe\"\n"
    ))
    .env;
    let shown = view(&env);
    let broken_line = FLOOR.lines().count() + 2;
    assert_eq!(shown.issues.len(), 1, "{:?}", shown.issues);
    assert_eq!(shown.issues[0].name, "broken");
    assert_eq!(
        shown.issues[0].line,
        Some(u32::try_from(broken_line).unwrap())
    );
    assert!(!shown.issues[0].message.is_empty());

    env.write_floor("schema = [[[");
    let invalid = view(&env);
    assert!(invalid.floor_error.is_some());
    assert!(invalid.issues.is_empty() && invalid.pending.is_none());
}

/// Adversarial pass: with `approved.json` gone after an approval, the gate
/// guards every project (D-15), so the view must offer a new approval even
/// for a floor that declares no action and no guarded project.
#[test]
fn a_snapshot_gone_after_an_approval_can_be_approved_again() {
    let env = Relay::unapproved(
        "schema = 2\n[[project]]\nname = \"notes\"\npath = \"Notes\"\nautonomy = \"free\"\n",
    )
    .env;
    assert_eq!(view(&env).pending, None, "nothing to approve yet");
    env.approve();
    std::fs::remove_file(env.relay_home().approved_file()).unwrap();
    let shown = view(&env);
    assert!(
        shown.snapshot_error.as_deref().unwrap().contains("is gone"),
        "{:?}",
        shown.snapshot_error
    );
    let pending = shown.pending.expect("a new approval is offered");
    assert_eq!(
        pending.hash,
        load_relay_floor(&env.floor_file()).unwrap().hash
    );
}

fn stream() -> StreamSummary {
    StreamSummary {
        bytes: 3,
        sha256: sha256_hex(b"abc"),
    }
}

fn started(run_id: &str, timeout_s: u32) -> JournalEntry {
    JournalEntry::RunStarted(RunStarted {
        run_id: run_id.to_owned(),
        project: "lab".to_owned(),
        session_id: Some("00000000-0000-0000-0000-000000000001".to_owned()),
        action: "probe".to_owned(),
        class: ActionClass::Free,
        params: BTreeMap::from([("host".to_owned(), "web01".to_owned())]),
        argv: vec![
            common::fake_tool_path(),
            "--host".to_owned(),
            "web01".to_owned(),
        ],
        method: None,
        url: None,
        credential: None,
        catalogue_hash: "00".to_owned(),
        approval_id: None,
        tree: None,
        timeout_s: Some(timeout_s),
    })
}

#[test]
fn recent_pairs_runs_and_marks_abandoned() {
    let env = approved_env();
    let home = env.relay_home();
    let now = now_ms();
    let finished = JournalEntry::RunFinished(RunFinished {
        body_broke_off: false,
        run_id: "done".to_owned(),
        outcome: RunOutcome::Exited,
        exit_code: Some(3),
        duration_ms: 1200,
        stdout: Some(stream()),
        stderr: Some(stream()),
        dropped_bytes: 0,
        status: None,
        body_bytes: None,
        body_sha256: None,
        detail: None,
    });
    let refused = JournalEntry::Refused(Refused {
        project: Some("lab".to_owned()),
        session_id: None,
        action: "probe".to_owned(),
        params: None,
        reason: RefusalReason::InvalidValue,
        detail: "parameter host: not one of the allowed values".to_owned(),
    });
    let denied = JournalEntry::GateDenied(GateDenied {
        project: "lab".to_owned(),
        session_id: None,
        deny: DenyKind::GuardedProgram,
        matched: Some("ssh".to_owned()),
        command_excerpt: Some("ssh host01".to_owned()),
    });
    for (ago, entry) in [
        (10 * 60_000, started("gone", 1)),
        (9 * 60_000, started("done", 540)),
        (8 * 60_000, finished),
        (7 * 60_000, refused),
        (6 * 60_000, denied),
        (1000, started("going", 540)),
    ] {
        append(&home, &[entry], now - ago).unwrap();
    }
    const { assert!(10 * 60_000 > 1000 + ABANDONED_GRACE_MS) };

    let shown = view(&env);
    let kinds: Vec<String> = shown
        .recent
        .iter()
        .map(|entry| match &entry.event {
            RecentEvent::Run(run) => format!("run {} {:?}", run.started.run_id, run.state),
            other => serde_json::to_value(other).unwrap()["kind"]
                .as_str()
                .unwrap()
                .to_owned(),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "catalogue_approved",
            "run going Running",
            "gate_denied",
            "refused",
            "run done Finished",
            "run gone Abandoned",
        ],
        "newest first (the approval is stamped now), a run once"
    );
    let RecentEvent::Run(done) = &shown.recent[4].event else {
        panic!();
    };
    assert_eq!(done.state, RunState::Finished);
    assert_eq!(done.finished.as_ref().unwrap().exit_code, Some(3));
    assert_eq!(done.finished_at_ms, Some(shown.recent[4].ts + 60_000));
}

/// Story 2.11 AC 12 (D-41): every session can run `officina relay --json`,
/// so it never asks the Credential Manager anything. Had it asked, the
/// credential below, stored nowhere, would read `false`; it reads `null`.
#[test]
fn relay_json_never_reads_credentials() {
    let env = Relay::unapproved(&format!(
        "{FLOOR}\n[[credential]]\nname = \"officina-test-unstored\"\nhosts = [\"monitoring.example.com\"]\nheader = \"Authorization\"\nscheme = \"Bearer\"\n\n[[action]]\nname = \"health\"\nkind = \"http\"\nclass = \"free\"\nmethod = \"GET\"\nurl = \"https://monitoring.example.com/api/{{check}}\"\ncredential = \"officina-test-unstored\"\n[action.params.check]\nvalues = [\"health\"]\n"
    ))
    .env;
    env.approve();
    let view = view(&env);
    assert_eq!(view.credentials.len(), 1, "{:?}", view.credentials);
    let credential = &view.credentials[0];
    assert_eq!(credential.name, "officina-test-unstored");
    assert_eq!(credential.present, None, "relay --json read the store");
    assert_eq!(credential.hosts, ["monitoring.example.com"]);
    let raw: serde_json::Value = serde_json::from_slice(&relay_json(&env).stdout).unwrap();
    assert!(raw["credentials"][0]["present"].is_null());
    let http = &raw["catalogue"]["actions"]["health"]["http"];
    assert_eq!(http["url"], "https://monitoring.example.com/api/{check}");
    assert_eq!(http["method"], "GET");
}

/// Story 2.13 AC 3 (D-172): the notices of the floor, each with its line of
/// explanation, outside the hash.
#[test]
fn relay_json_lists_notices() {
    let env = approved_env();
    let approved_hash = view(&env).approved_hash;
    env.write_floor(&format!(
        "{FLOOR}\n[[action]]\nname = \"status\"\nclass = \"free\"\nprogram = \"C:/Windows/System32/OpenSSH/ssh.exe\"\nargs = [\"web01\", \"systemctl\", \"status\", \"{{unit}}\"]\n[action.params.unit]\nvalues = [\"nginx.service\"]\n"
    ));
    let raw: serde_json::Value = serde_json::from_slice(&relay_json(&env).stdout).unwrap();
    let notices = raw["notices"].as_array().unwrap();
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert_eq!(notices[0]["kind"], "remote_command_value");
    assert_eq!(notices[0]["action"], "status");
    assert_eq!(notices[0]["param"], "unit");
    assert_eq!(notices[0]["item"], 3);
    assert!(
        notices[0]["explanation"]
            .as_str()
            .unwrap()
            .contains("remote shell")
    );
    assert_eq!(view(&env).approved_hash, approved_hash);
}

#[test]
fn usage_error_exits_64() {
    let env = approved_env();
    for args in [
        &["relay"][..],
        &["relay", "--json", "extra"],
        &["relay", "--pretty"],
    ] {
        let out = env.run(args, b"");
        assert_eq!(out.status.code(), Some(64), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("relay --json"));
    }
}

#[test]
fn unreadable_home_exits_1() {
    let env = TestEnv::new();
    let file = env.root.path().join("not-a-folder");
    std::fs::write(&file, b"x").unwrap();
    let mut cmd = env.officina(&["relay", "--json"]);
    cmd.env("OFFICINA_HOME", &file);
    let out = common::run_with(cmd, b"");
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("cannot read the Officina home"));

    let out = Command::new(common::OFFICINA)
        .args(["relay", "--json"])
        .env_clear()
        .env(
            "SYSTEMROOT",
            std::env::var_os("SYSTEMROOT").unwrap_or_default(),
        )
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "no home at all");
    assert!(String::from_utf8_lossy(&out.stderr).contains("no Officina home"));
}

#[test]
fn relay_json_writes_nothing() {
    let env = approved_env();
    let home = env.relay_home();
    append(&home, &[started("going", 540)], now_ms()).unwrap();
    std::fs::write(home.stopped_file(), br#"{"since_ms":1}"#).unwrap();
    env.write_floor(&FLOOR.replace("\"web01\", \"db01\"", "\"web01\""));
    let before = fingerprint(env.root.path());
    for _ in 0..2 {
        assert_eq!(relay_json(&env).status.code(), Some(0));
    }
    let fresh = TestEnv::new();
    let fresh_before = fingerprint(fresh.root.path());
    assert_eq!(relay_json(&fresh).status.code(), Some(0));
    assert_eq!(fingerprint(env.root.path()), before);
    assert_eq!(
        fingerprint(fresh.root.path()),
        fresh_before,
        "not even the home is created"
    );
}
