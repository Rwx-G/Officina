//! `officina run` end to end (stories 2.5 and 2.6, AD-29 to AD-33, D-20 to
//! D-23), with `officina-fake-tool` as every action's program. Windows only:
//! a `native` program is a Windows absolute path (AD-22) and the process
//! tree is a job object (AD-31); Linux is v1.

#![cfg(windows)]

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use common::fake_output::{stderr_byte, stdout_byte, stream};
use common::relay::{
    Relay, assert_refused, assert_refused_after, slashed, stderr_lines, stderr_text,
};
use common::{git, git_dir_or_skip, heartbeat_len, heartbeat_stopped, stop_heartbeat};
use officina_core::registry::types::AuthorReason;
use officina_core::relay::argv::{
    ArgvRefusal, Launch, WSL_KILL_AFTER_S, WSL_TIMEOUT_MARGIN_S, WSL_TIMEOUT_PROGRAM,
};
use officina_core::relay::classes::CHECK_VALID_MS;
use officina_core::relay::journal::{
    JOURNAL_RETENTION_DAYS, JournalEntry, RefusalReason, RunFinished, RunOutcome, RunStarted,
    StreamSummary, TreeFingerprint, is_expired_journal_file,
};
use officina_core::relay::sha256_hex;
use officina_host::relay::journal::JournalLock;
use officina_host::relay::tree::{ENV_TEST_GIT, TREE_LIMITS};

/// The relay floor of most tests: a guarded project and a free one, both
/// real folders, every action's program the fake tool. `{project}`,
/// `{notes}` and `{tool}` are filled in by [`Relay::with_floor`].
const FLOOR: &str = r#"schema = 2

[[project]]
name = "supervision"
path = "{project}"
actions = ["probe", "tool", "short", "capped", "verify", "apply", "approve-me", "apply-sensitive", "apply-auto"]

[[project]]
name = "notes"
path = "{notes}"
autonomy = "free"
actions = ["probe"]

[[action]]
name = "probe"
description = "Reads one host"
class = "free"
program = "{tool}"
env = { PROBE_MODE = "read-only" }
args = ["--host", "{host}"]

[action.params.host]
values = ["web01", "db01"]

[[action]]
name = "tool"
description = "Whatever FAKE_TOOL_MODE says"
class = "free"
program = "{tool}"

[[action]]
name = "short"
class = "free"
program = "{tool}"
timeout_s = 1

[[action]]
name = "capped"
class = "free"
program = "{tool}"
max_output_kib = 1

[[action]]
name = "verify"
class = "free"
program = "{tool}"
args = ["--check", "{group}"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"

[[action]]
name = "apply"
class = "after-check"
check = "verify"
program = "{tool}"
args = ["{group}"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"

[[action]]
name = "approve-me"
class = "human"
program = "{tool}"
args = ["{group}"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"

[[action]]
name = "apply-sensitive"
class = "human"
check = "verify"
program = "{tool}"
args = ["{group}"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"

[[action]]
name = "apply-auto"
class = "auto"
check = "verify"
program = "{tool}"
args = ["{group}"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"
"#;

impl Relay {
    fn with_floor(floor: &str) -> Relay {
        Relay::approved(floor)
    }

    fn new() -> Relay {
        Relay::with_floor(FLOOR)
    }
}

fn sorted_keys(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap().to_owned())
        .collect()
}

// ---- AC 1: usage ----

fn assert_usage(relay: &Relay, args: &[&str]) {
    let out = relay.run(args);
    assert_eq!(out.status.code(), Some(64), "{args:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage: officina"));
    assert!(relay.launches().is_empty() && relay.entries().is_empty());
}

#[test]
fn token_without_equals_is_usage() {
    let relay = Relay::new();
    assert_usage(&relay, &["probe", "host"]);
    assert_usage(&relay, &["probe", "host=web01", "--wait"]);
}

#[test]
fn duplicate_parameter_is_usage() {
    let relay = Relay::new();
    assert_usage(&relay, &["probe", "host=web01", "host=db01"]);
}

#[test]
fn missing_action_is_usage() {
    let relay = Relay::new();
    assert_usage(&relay, &[]);
    assert_usage(&relay, &["host=web01"]);
    assert_usage(&relay, &["--list", "probe"]);
    assert_usage(&relay, &["--force"]);
}

// ---- AC 2, AC 3, AC 4, AC 5: the checks ----

#[test]
fn checks_run_in_the_documented_order() {
    let relay = Relay::new();
    let stop = relay.env.relay_home().stopped_file();
    std::fs::write(&stop, br#"{"since_ms":1}"#).unwrap();
    let floor = FLOOR.replace("\"web01\", \"db01\"", "\"web01\", \"db01\", \"db02\"");
    relay.write(&floor);
    let everything_wrong = ["approve-me", "group=-x", "extra=1"];
    let steps = [
        RefusalReason::Stopped,
        RefusalReason::CatalogueChanged,
        RefusalReason::OutsideProjects,
        RefusalReason::ActionNotAllowed,
        RefusalReason::InvalidValue,
    ];
    for (i, expected) in steps.iter().enumerate() {
        let (dir, args): (PathBuf, Vec<&str>) = match i {
            0 | 1 => (relay.outside(), everything_wrong.to_vec()),
            2 => (relay.outside(), everything_wrong.to_vec()),
            3 => (relay.notes(), everything_wrong.to_vec()),
            _ => (relay.project(), vec!["approve-me", "group=-x"]),
        };
        let out = relay.run_in(&dir, &args);
        assert_refused(&relay, &out, *expected);
        // Lift the failure just reported and go on.
        match expected {
            RefusalReason::Stopped => std::fs::remove_file(&stop).unwrap(),
            RefusalReason::CatalogueChanged => relay.write(FLOOR),
            _ => {}
        }
    }
    assert_eq!(relay.refused().len(), steps.len());
}

#[test]
fn free_project_listing_the_action_is_served() {
    let relay = Relay::new();
    let out = relay.run_in(&relay.notes(), &["probe", "host=db01"]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(relay.started()[0].project, "notes");
    let launched = &relay.launches()[0];
    assert_eq!(
        Path::new(launched["cwd"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        relay.notes().canonicalize().unwrap()
    );
}

/// What a scenario does while its call waits for a decision: the author's
/// part, given the request's id.
type During = Box<dyn FnOnce(&Relay, &str)>;

/// One refusal code's scenario: the relay, where the call runs, with what,
/// the variables it gets, a file held open while it runs, and what happens
/// while it waits.
struct Scenario {
    relay: Relay,
    dir: PathBuf,
    args: Vec<&'static str>,
    env: Vec<(&'static str, String)>,
    held: Option<std::fs::File>,
    during: Option<During>,
}

/// The ids of the requests in the relay's approvals folder.
fn request_ids(relay: &Relay) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(relay.env.relay_home().approvals_dir()) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_string_lossy()
                .strip_suffix(".request.json")
                .map(str::to_owned)
        })
        .collect();
    ids.sort();
    ids
}

/// Runs `cmd`, and once its request appears, `during` with its id.
fn run_waiting(relay: &Relay, cmd: Command, during: During) -> Output {
    let mut cmd = cmd;
    let child = cmd.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let id = loop {
        if let Some(id) = request_ids(relay).pop() {
            break id;
        }
        assert!(Instant::now() < deadline, "no request appeared");
        std::thread::sleep(Duration::from_millis(50));
    };
    during(relay, &id);
    child.wait_with_output().unwrap()
}

/// What one refusal code needs: the floor it starts from, what is changed
/// after approval, where it runs and with what. The `match` is exhaustive,
/// so a new refusal code does not compile without its scenario; `None`, with
/// the reason printed, when this machine cannot play it (D-23).
fn scenario(reason: RefusalReason) -> Option<Scenario> {
    let in_git = |git: GitRelay, args: Vec<&'static str>| Scenario {
        dir: git.relay.project(),
        args,
        env: Vec::new(),
        held: None,
        during: None,
        relay: git.relay,
    };
    let verify = || vec!["verify", "group=lab-web"];
    match reason {
        RefusalReason::CheckNotSatisfied => {
            let git = GitRelay::new("every_refusal_code (check_not_satisfied)")?;
            checked(&git, "lab-web");
            let during: During = Box::new(|relay: &Relay, id: &str| {
                std::fs::write(relay.project().join("playbook.yml"), "- hosts: all\n").unwrap();
                officina_host::relay::authority::decide_approval(
                    &relay.env.relay_home(),
                    id,
                    true,
                    None,
                )
                .unwrap();
            });
            return Some(Scenario {
                during: Some(during),
                ..in_git(
                    git,
                    vec!["apply-sensitive", "group=lab-web", "--wait", "30"],
                )
            });
        }
        RefusalReason::TreeChanged => {
            let git = GitRelay::new("every_refusal_code (tree_changed)")?;
            let during: During = Box::new(|relay: &Relay, id: &str| {
                std::fs::write(relay.project().join("playbook.yml"), "- hosts: all\n").unwrap();
                officina_host::relay::authority::decide_approval(
                    &relay.env.relay_home(),
                    id,
                    true,
                    None,
                )
                .unwrap();
            });
            return Some(Scenario {
                during: Some(during),
                ..in_git(git, vec!["approve-me", "group=lab-web", "--wait", "30"])
            });
        }
        RefusalReason::GitUnavailable => {
            let git = GitRelay::new("every_refusal_code (git_unavailable)")?;
            return Some(Scenario {
                env: vec![(ENV_TEST_GIT, String::new())],
                ..in_git(git, verify())
            });
        }
        RefusalReason::TreeTooLarge => {
            let git = GitRelay::new("every_refusal_code (tree_too_large)")?;
            std::fs::File::create(git.relay.project().join("dump.bin"))
                .unwrap()
                .set_len(TREE_LIMITS.file_bytes + 1)
                .unwrap();
            return Some(in_git(git, verify()));
        }
        RefusalReason::TreeUnreadable => {
            use std::os::windows::fs::OpenOptionsExt;
            let git = GitRelay::new("every_refusal_code (tree_unreadable)")?;
            let held = std::fs::File::options()
                .read(true)
                .write(true)
                .share_mode(0)
                .open(git.relay.project().join("playbook.yml"))
                .unwrap();
            return Some(Scenario {
                held: Some(held),
                ..in_git(git, verify())
            });
        }
        _ => {}
    }
    let relay = match reason {
        RefusalReason::NotApproved => Relay::unapproved(FLOOR),
        _ => Relay::new(),
    };
    let home = relay.env.relay_home();
    let project = relay.project();
    let probe = vec!["probe", "host=web01"];
    let human = |wait: &'static str| vec!["approve-me", "group=lab-web", "--wait", wait];
    let mut during: Option<During> = None;
    let (dir, args) = match reason {
        RefusalReason::Stopped => {
            std::fs::write(home.stopped_file(), b"{}").unwrap();
            (project, probe)
        }
        RefusalReason::NotApproved => (project, probe),
        RefusalReason::ApprovedUnreadable => {
            std::fs::write(home.approved_file(), b"{ not json").unwrap();
            (project, probe)
        }
        RefusalReason::DecisionRecordUnreadable => {
            std::fs::write(home.decisions_file(), b"{ not json").unwrap();
            (project, probe)
        }
        RefusalReason::CatalogueUnconfirmed => {
            forge_catalogue(&relay, &FLOOR.replace("timeout_s = 1", "timeout_s = 2"));
            (project, probe)
        }
        RefusalReason::FloorUnreadable => {
            std::fs::write(relay.env.floor_file(), b"schema = [[[").unwrap();
            (project, probe)
        }
        RefusalReason::CatalogueChanged => {
            relay.write(&FLOOR.replace("timeout_s = 1", "timeout_s = 2"));
            (project, probe)
        }
        RefusalReason::OutsideProjects => (relay.outside(), probe),
        RefusalReason::ActionNotAllowed => (relay.notes(), vec!["tool"]),
        RefusalReason::MissingParameter => (project, vec!["probe"]),
        RefusalReason::UnknownParameter => (project, vec!["probe", "host=web01", "x=1"]),
        RefusalReason::InvalidValue => (project, vec!["probe", "host=web02"]),
        RefusalReason::CheckRequired => (project, vec!["apply", "group=lab-web"]),
        RefusalReason::ApprovalRejected => {
            during = Some(Box::new(|relay: &Relay, id: &str| {
                officina_host::relay::authority::decide_approval(
                    &relay.env.relay_home(),
                    id,
                    false,
                    None,
                )
                .unwrap();
            }));
            (project, human("30"))
        }
        RefusalReason::ApprovalUnavailable => {
            std::fs::create_dir_all(home.relay_dir()).unwrap();
            std::fs::write(home.approvals_dir(), b"not a folder").unwrap();
            (project, human("1"))
        }
        RefusalReason::ApprovalPending => (project, human("1")),
        RefusalReason::ApprovalNotGiven => {
            during = Some(Box::new(|relay: &Relay, id: &str| {
                forge_approval(relay, id);
            }));
            (project, human("30"))
        }
        RefusalReason::CheckNotSatisfied
        | RefusalReason::TreeChanged
        | RefusalReason::GitUnavailable
        | RefusalReason::TreeTooLarge
        | RefusalReason::TreeUnreadable => unreachable!("built above"),
        // These need a credential store, which a CLI test never touches: the
        // real one is the author's. They are proved with a fake store by
        // `crates/officina-host/tests/relay_http.rs::{
        // missing_credential_refuses_before_any_connection,
        // unreadable_credential_refuses_before_any_connection}` (story 2.11
        // AC 7).
        RefusalReason::CredentialMissing | RefusalReason::CredentialUnreadable => {
            println!("{reason:?}: proved at host level, crates/officina-host/tests/relay_http.rs");
            return None;
        }
    };
    Some(Scenario {
        relay,
        dir,
        args,
        env: Vec::new(),
        held: None,
        during,
    })
}

#[test]
fn every_refusal_code_exits_126_with_one_journal_line() {
    for reason in RefusalReason::ALL {
        let Some(scenario) = scenario(*reason) else {
            continue;
        };
        let relay = &scenario.relay;
        let started_before = relay.started().len();
        let launches_before = relay.launches().len();
        let mut cmd = relay.command(&scenario.dir, &scenario.args);
        for (key, value) in &scenario.env {
            cmd.env(key, value);
        }
        let out = match scenario.during {
            Some(during) => run_waiting(relay, cmd, during),
            None => common::run_with(cmd, b""),
        };
        assert_refused_after(relay, &out, *reason, launches_before);
        let refused = relay.refused();
        assert_eq!(refused.len(), 1, "{reason:?}");
        assert_eq!(refused[0].action, scenario.args[0]);
        assert_eq!(relay.started().len(), started_before, "{reason:?}");
        drop(scenario.held);
    }
}

#[test]
fn stop_flag_written_by_hand_refuses() {
    let relay = Relay::new();
    std::fs::write(
        relay.env.relay_home().stopped_file(),
        br#"{"since_ms":1793102400000}"#,
    )
    .unwrap();
    assert_refused(
        &relay,
        &relay.run(&["probe", "host=web01"]),
        RefusalReason::Stopped,
    );
    std::fs::remove_file(relay.env.relay_home().stopped_file()).unwrap();
    assert_eq!(relay.run(&["probe", "host=web01"]).status.code(), Some(0));
}

// ---- story 2.7: stop and resume through the window's authority ----

#[test]
fn stop_through_the_authority_refuses() {
    let relay = Relay::new();
    let home = relay.env.relay_home();
    assert!(officina_host::relay::authority::set_relay_stopped(&home, true).unwrap());
    let out = relay.run(&["probe", "host=web01"]);
    assert_refused(&relay, &out, RefusalReason::Stopped);
    assert!(stderr_lines(&out)[0].contains("the relay is stopped in the Officina window"));
    let listed = list(&relay, &relay.project());
    assert!(listed.contains("The relay is stopped"), "{listed}");
}

#[test]
fn resume_lets_runs_through() {
    let relay = Relay::new();
    let home = relay.env.relay_home();
    officina_host::relay::authority::set_relay_stopped(&home, true).unwrap();
    assert_refused(
        &relay,
        &relay.run(&["probe", "host=web01"]),
        RefusalReason::Stopped,
    );
    assert!(officina_host::relay::authority::set_relay_stopped(&home, false).unwrap());
    let out = relay.run(&["probe", "host=web01"]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(relay.launches().len(), 1);
    let kinds: Vec<&'static str> = relay
        .entries()
        .iter()
        .map(|e| match e {
            JournalEntry::RelayStopped => "stopped",
            JournalEntry::RelayResumed => "resumed",
            JournalEntry::Refused(_) => "refused",
            JournalEntry::RunStarted(_) => "started",
            JournalEntry::RunFinished(_) => "finished",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        ["stopped", "refused", "resumed", "started", "finished"]
    );
}

#[test]
fn free_action_runs() {
    let relay = Relay::new();
    let out = relay.run(&["probe", "host=web01"]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert!(out.stderr.is_empty());
    assert_eq!(relay.launches().len(), 1);
    assert_eq!(
        relay.launches()[0]["argv"],
        serde_json::json!(["--host", "web01"])
    );
}

// ---- AC 6: exit codes ----

fn tool_with_mode(relay: &Relay, action: &str, mode: &str) -> Output {
    let mut cmd = relay.command(&relay.project(), &[action]);
    cmd.env("FAKE_TOOL_MODE", mode);
    common::run_with(cmd, b"")
}

#[test]
fn exit_code_passes_through() {
    let relay = Relay::new();
    for code in [0, 1, 3, 2, 124, 126, 127, 255] {
        let out = tool_with_mode(&relay, "tool", &format!("exit:{code}"));
        assert_eq!(out.status.code(), Some(code), "exit:{code}");
        assert!(
            !String::from_utf8_lossy(&out.stderr).contains("officina:"),
            "the program's own {code} is not Officina's"
        );
    }
    let finished = relay.finished();
    assert!(finished.iter().all(|f| f.outcome == RunOutcome::Exited));
    assert_eq!(finished[2].exit_code, Some(3));
}

#[test]
fn full_windows_exit_code_passes_through() {
    let relay = Relay::new();
    let out = tool_with_mode(&relay, "tool", "exit:3221225477");
    assert_eq!(out.status.code(), Some(0xC000_0005_u32 as i32));
    assert_eq!(
        relay.finished()[0].exit_code,
        Some(i64::from(0xC000_0005_u32 as i32))
    );
}

#[test]
fn missing_program_exits_127() {
    let relay = Relay::with_floor(&FLOOR.replace(
        "name = \"tool\"\ndescription = \"Whatever FAKE_TOOL_MODE says\"\nclass = \"free\"\nprogram = \"{tool}\"",
        "name = \"tool\"\nclass = \"free\"\nprogram = \"C:/path/to/missing-tool.exe\"",
    ));
    let out = relay.run(&["tool"]);
    assert_eq!(out.status.code(), Some(127));
    assert!(String::from_utf8_lossy(&out.stderr).contains("could not start"));
    let finished = relay.finished();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].outcome, RunOutcome::SpawnFailed);
    assert!(finished[0].detail.is_some());
    assert_eq!(relay.started().len(), 1);
}

// ---- AC 7: the process tree ----

#[test]
fn timeout_exits_124_and_the_tree_stops() {
    let relay = Relay::new();
    let heartbeat = relay.env.root.path().join("heartbeat");
    let mut cmd = relay.command(&relay.project(), &["short"]);
    cmd.env("FAKE_TOOL_MODE", "tree")
        .env("FAKE_TOOL_HEARTBEAT", &heartbeat)
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    let started = Instant::now();
    let out = cmd.output().unwrap();
    let took = started.elapsed();
    assert_eq!(out.status.code(), Some(124), "{:?}", stderr_lines(&out));
    assert!(
        took >= Duration::from_secs(1) && took < Duration::from_secs(5),
        "{took:?}"
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("officina: timeout"));
    assert!(heartbeat_len(&heartbeat) > 0, "the grandchild had started");
    let stopped = heartbeat_stopped(&heartbeat);
    stop_heartbeat(&heartbeat);
    assert!(stopped, "the grandchild outlived the timeout");
    let finished = relay.finished();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].outcome, RunOutcome::Timeout);
    assert_eq!(finished[0].exit_code, None);
}

#[test]
fn killing_run_stops_the_grandchild() {
    let relay = Relay::new();
    let heartbeat = relay.env.root.path().join("heartbeat");
    let mut cmd = relay.command(&relay.project(), &["tool"]);
    cmd.env("FAKE_TOOL_MODE", "tree")
        .env("FAKE_TOOL_HEARTBEAT", &heartbeat)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut run = cmd.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while heartbeat_len(&heartbeat) < 3 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(heartbeat_len(&heartbeat) >= 3, "the grandchild never beat");
    run.kill().unwrap();
    let _ = run.wait();
    let stopped = heartbeat_stopped(&heartbeat);
    stop_heartbeat(&heartbeat);
    assert!(stopped, "killing officina run must end its whole tree");
    assert_eq!(relay.started().len(), 1);
    assert!(relay.finished().is_empty(), "a killed run reads abandoned");
}

// ---- AC 8: output ----

#[test]
fn output_is_capped_and_drained() {
    const LEN: u64 = 100_000;
    let relay = Relay::new();
    let started = Instant::now();
    let out = tool_with_mode(&relay, "capped", &format!("bytes:{LEN}"));
    assert_eq!(out.status.code(), Some(0), "drained, never stalled");
    assert!(started.elapsed() < Duration::from_secs(10));
    let marker_dropped = 2 * LEN - 1024;
    let marker = format!("[officina: output cut at 1 KiB, {marker_dropped} bytes dropped]\n");
    let stdout_text = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout_text.ends_with(&marker),
        "{:?}",
        &stdout_text[stdout_text.len().saturating_sub(80)..]
    );
    let passed = out.stdout.len() - marker.len() + out.stderr.len();
    assert!(passed <= 1024 + 1, "{passed} bytes passed a 1 KiB budget");
    let finished = &relay.finished()[0];
    assert_eq!(
        (
            finished.stdout.as_ref().unwrap().bytes,
            finished.stderr.as_ref().unwrap().bytes
        ),
        (LEN, LEN)
    );
    assert_eq!(finished.dropped_bytes, marker_dropped);
}

#[test]
fn hashes_cover_all_bytes() {
    let relay = Relay::new();
    // Under the default budget, both streams pass through whole.
    let whole = tool_with_mode(&relay, "tool", "bytes:30000");
    assert_eq!(whole.stdout, stream(30_000, stdout_byte));
    assert_eq!(whole.stderr, stream(30_000, stderr_byte));
    // Over a budget, the hashes still cover every byte read.
    tool_with_mode(&relay, "capped", "bytes:100000");
    let finished = relay.finished();
    for (f, len) in finished.iter().zip([30_000u64, 100_000]) {
        assert_eq!(
            f.stdout.as_ref().unwrap().sha256,
            sha256_hex(&stream(len, stdout_byte))
        );
        assert_eq!(
            f.stderr.as_ref().unwrap().sha256,
            sha256_hex(&stream(len, stderr_byte))
        );
    }
}

// ---- AC 9, AC 10: the argument vector set ----

const ARGV_VECTORS: &str = include_str!("../../../tests/vectors/argv.json");

struct Case {
    name: String,
    action: serde_json::Value,
    params: BTreeMap<String, String>,
    expect: Option<Launch>,
    refusal: Option<ArgvRefusal>,
}

fn vector_cases() -> Vec<Case> {
    let set: serde_json::Value = serde_json::from_str(ARGV_VECTORS).unwrap();
    set["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| Case {
            name: case["name"].as_str().unwrap().to_owned(),
            action: case["action"].clone(),
            params: serde_json::from_value(case["params"].clone()).unwrap(),
            expect: case
                .get("expect")
                .map(|e| serde_json::from_value(e.clone()).unwrap()),
            refusal: case
                .get("refusal")
                .map(|r| serde_json::from_value(r.clone()).unwrap()),
        })
        .collect()
}

/// A JSON value written as TOML. JSON's string escapes are TOML's, so a
/// string is written as `serde_json` writes it.
fn toml_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => format!(
            "{{ {} }}",
            map.iter()
                .map(|(k, v)| format!("{} = {}", serde_json::to_string(k).unwrap(), toml_value(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        serde_json::Value::Array(items) => format!(
            "[{}]",
            items.iter().map(toml_value).collect::<Vec<_>>().join(", ")
        ),
        other => other.to_string(),
    }
}

/// A floor with one guarded project listing the one action of `action`.
fn floor_with_action(action: &serde_json::Value) -> String {
    let name = action["name"].as_str().unwrap();
    let mut floor = format!(
        "schema = 2\n\n[[project]]\nname = \"supervision\"\npath = \"{{project}}\"\nactions = [\"{name}\"]\n\n[[action]]\n"
    );
    for (key, value) in action.as_object().unwrap() {
        floor.push_str(&format!("{key} = {}\n", toml_value(value)));
    }
    floor
}

fn is_native(action: &serde_json::Value) -> bool {
    action
        .get("runner")
        .and_then(|r| r.as_str())
        .unwrap_or("native")
        == "native"
}

fn tokens(params: &BTreeMap<String, String>) -> Vec<String> {
    params.iter().map(|(k, v)| format!("{k}={v}")).collect()
}

fn reason_for(refusal: &ArgvRefusal) -> RefusalReason {
    match refusal {
        ArgvRefusal::MissingParameter { .. } => RefusalReason::MissingParameter,
        ArgvRefusal::UnknownParameter { .. } => RefusalReason::UnknownParameter,
        ArgvRefusal::Value { .. } => RefusalReason::InvalidValue,
    }
}

#[test]
fn every_argv_injection_is_refused_before_launch() {
    let mut refused_cases = 0;
    for case in vector_cases() {
        let Some(refusal) = &case.refusal else {
            continue;
        };
        let mut action = case.action.clone();
        if is_native(&action) {
            action["program"] = common::fake_tool_path().into();
        }
        let relay = Relay::with_floor(&floor_with_action(&action));
        let args = tokens(&case.params);
        let mut all: Vec<&str> = vec![action["name"].as_str().unwrap()];
        all.extend(args.iter().map(String::as_str));
        if args.iter().any(|a| a.contains('\0')) {
            // Windows cannot carry a NUL in a command line: the value never
            // reaches Officina at all.
            let spawned = relay.command(&relay.project(), &all).spawn();
            assert!(spawned.is_err(), "case {}", case.name);
            continue;
        }
        let out = relay.run(&all);
        assert_refused(&relay, &out, reason_for(refusal));
        refused_cases += 1;
    }
    assert!(refused_cases > 0);
}

#[test]
fn every_accepted_native_case_arrives_intact() {
    let mut launched = 0;
    for case in vector_cases() {
        let Some(expect) = &case.expect else { continue };
        if !is_native(&case.action) {
            continue;
        }
        let mut action = case.action.clone();
        action["program"] = common::fake_tool_path().into();
        let relay = Relay::with_floor(&floor_with_action(&action));
        let args = tokens(&case.params);
        let mut all: Vec<&str> = vec![action["name"].as_str().unwrap()];
        all.extend(args.iter().map(String::as_str));
        let out = relay.run(&all);
        assert_eq!(
            out.status.code(),
            Some(0),
            "case {}: {:?}",
            case.name,
            stderr_lines(&out)
        );
        let logged = &relay.launches()[0];
        assert_eq!(
            logged["argv"],
            serde_json::json!(expect.args),
            "case {}",
            case.name
        );
        let keys = sorted_keys(&logged["env_keys"]);
        for key in expect.env.keys() {
            assert!(keys.contains(key), "case {}: {key}", case.name);
        }
        launched += 1;
    }
    assert!(launched > 0);
}

/// The system directory's `wsl.exe`, as `officina run` must take it.
fn system_wsl() -> PathBuf {
    officina_host::platform::windows::system_directory()
        .unwrap()
        .join("wsl.exe")
}

/// The first installed distro with a POSIX shell, or why there is none.
fn installed_distro() -> Result<String, String> {
    let out = Command::new(system_wsl())
        .args(["-l", "-q"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("wsl.exe does not start: {e}"))?;
    if !out.status.success() {
        return Err(format!("wsl.exe -l -q exited {:?}", out.status.code()));
    }
    let (pairs, _) = out.stdout.as_chunks::<2>();
    let units: Vec<u16> = pairs.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
    String::from_utf16_lossy(&units)
        .lines()
        .map(|l| {
            l.trim_matches(|c: char| c == '\0' || c.is_whitespace())
                .to_owned()
        })
        .find(|name| !name.is_empty() && !name.starts_with("docker-desktop"))
        .ok_or_else(|| "no WSL distro is installed".to_owned())
}

#[test]
fn every_accepted_wsl_case_arrives_intact() {
    let distro = match installed_distro() {
        Ok(distro) => distro,
        Err(why) => {
            println!("skipped: {why}");
            return;
        }
    };
    let mut checked = 0;
    for case in vector_cases() {
        let Some(expect) = &case.expect else { continue };
        if is_native(&case.action) {
            continue;
        }
        let program = case.action["program"].as_str().unwrap().to_owned();
        let own_args = expect
            .args
            .iter()
            .position(|a| *a == program)
            .map(|i| expect.args[i + 1..].to_vec())
            .unwrap();
        let mut action = case.action.clone();
        action["program"] = "/bin/bash".into();
        action["distro"] = distro.clone().into();
        let mut args = vec![
            serde_json::json!("-c"),
            serde_json::json!("printf '%s\\0' \"$@\""),
            serde_json::json!("argv0"),
        ];
        args.extend(action["args"].as_array().cloned().unwrap_or_default());
        action["args"] = serde_json::Value::Array(args);
        let relay = Relay::with_floor(&floor_with_action(&action));
        let tokens = tokens(&case.params);
        let mut all: Vec<&str> = vec![action["name"].as_str().unwrap()];
        all.extend(tokens.iter().map(String::as_str));
        let out = relay.run(&all);
        assert_eq!(
            out.status.code(),
            Some(0),
            "case {}: {:?}",
            case.name,
            stderr_lines(&out)
        );
        let received: Vec<String> = out
            .stdout
            .split(|b| *b == 0)
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect();
        let received = &received[..received.len() - 1];
        assert_eq!(received, own_args.as_slice(), "case {}", case.name);
        checked += 1;
    }
    assert!(checked > 0);
}

// ---- AC 11, AC 12: the launch ----

#[test]
fn native_launch_gets_cwd_env_and_null_stdin() {
    let relay = Relay::new();
    let mut cmd = relay.command(&relay.notes(), &["probe", "host=web01"]);
    // Run from the free project's subfolder: the action still starts in
    // its project's folder.
    let sub = relay.notes().join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    cmd.current_dir(&sub)
        .env("FAKE_TOOL_LOG_VALUES", "PROBE_MODE");
    let out = common::run_with(cmd, b"bytes a session typed on stdin");
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    let logged = &relay.launches()[0];
    assert_eq!(
        Path::new(logged["cwd"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        relay.notes().canonicalize().unwrap()
    );
    assert_eq!(logged["env"]["PROBE_MODE"], "read-only");
    assert_eq!(logged["stdin_bytes"], 0, "stdin is null");
}

/// The `probe` launch run with the caller's `PATH`, `PATHEXT`, `ComSpec`
/// and `WSLENV` pointing at a planted folder, the tool logging their values.
fn launch_from_a_planted_environment(relay: &Relay, planted: &Path) -> serde_json::Value {
    let mut cmd = relay.command(&relay.project(), &["probe", "host=web01"]);
    cmd.env("PATH", planted)
        .env("PATHEXT", ".SHIM")
        .env("ComSpec", planted.join("cmd.exe"))
        .env("WSLENV", "OFFICINA_PLANTED/p")
        .env(
            "FAKE_TOOL_LOG_VALUES",
            "PATH,PATHEXT,ComSpec,WSLENV,PROBE_MODE",
        );
    let out = common::run_with(cmd, b"");
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    relay.launches().pop().unwrap()
}

/// AC 3 of story 2.14 (D-173): a `PATH` the caller planted a shim folder in
/// does not reach the action; its `PATH`, `PATHEXT` and `ComSpec` are the
/// ones Windows builds from the registry, and the catalogue's `env` applies.
#[test]
fn a_caller_path_shim_is_not_found_by_the_action() {
    let relay = Relay::new();
    let planted = relay.outside().join("shim");
    std::fs::create_dir_all(&planted).unwrap();
    let logged = launch_from_a_planted_environment(&relay, &planted);
    let env = &logged["env"];
    let path = env["PATH"].as_str().expect("the action has a PATH");
    let planted_text = planted.to_string_lossy().to_lowercase();
    assert!(
        !path.to_lowercase().contains(&planted_text),
        "the caller's shim folder reached the action: {path}"
    );
    let system = officina_host::relay::home::system_dir().unwrap();
    assert!(
        std::env::split_paths(path).any(|dir| dir
            .to_string_lossy()
            .eq_ignore_ascii_case(&system.to_string_lossy())),
        "the system folder is on the registry's Path: {path}"
    );
    assert_ne!(env["PATHEXT"], ".SHIM");
    let comspec = env["ComSpec"].as_str().unwrap_or_default().to_lowercase();
    assert!(
        !comspec.contains(&planted_text),
        "the caller's ComSpec reached the action: {comspec}"
    );
    assert_eq!(
        env["PROBE_MODE"], "read-only",
        "the catalogue's env applies"
    );
}

/// Story 2.16 AC 6 (D-175): a variable a session sets for one `officina
/// run` never reaches the action, whatever program would read it; the
/// action's environment is the registry's, then the catalogue's `env`.
#[test]
fn a_caller_variable_never_reaches_the_action() {
    let relay = Relay::new();
    let seeded = [
        ("PYTHONPATH", "C:/path/to/shim"),
        ("NODE_OPTIONS", "--require C:/path/to/shim.js"),
        ("KUBECONFIG", "C:/path/to/shim/config"),
        ("SEEDED_BY_THE_CALLER_7F3A9C", "seeded"),
    ];
    let mut cmd = relay.command(&relay.project(), &["probe", "host=web01"]);
    for (name, value) in seeded {
        cmd.env(name, value);
    }
    let names: Vec<&str> = seeded.iter().map(|(name, _)| *name).collect();
    cmd.env(
        "FAKE_TOOL_LOG_VALUES",
        format!("{},PROBE_MODE,SystemRoot", names.join(",")),
    );
    let out = common::run_with(cmd, b"");
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    let logged = relay.launches().pop().unwrap();
    for (name, value) in seeded {
        assert_ne!(
            logged["env"][name], value,
            "the caller's {name} reached the action"
        );
    }
    let keys = sorted_keys(&logged["env_keys"]);
    assert!(
        !keys.iter().any(|key| key == "SEEDED_BY_THE_CALLER_7F3A9C"),
        "a name only the caller set reached the action: {keys:?}"
    );
    assert_eq!(
        logged["env"]["PROBE_MODE"], "read-only",
        "the catalogue's env"
    );
    assert!(
        logged["env"]["SystemRoot"].is_string(),
        "positive control: the registry's environment reached it"
    );
}

/// AC 3 of story 2.14 (D-173): `WSLENV` forwards Windows variables into
/// Linux; the caller's never reaches a program that could start `wsl.exe`.
#[test]
fn wslenv_from_the_caller_does_not_reach_wsl() {
    let relay = Relay::new();
    let planted = relay.outside().join("shim");
    std::fs::create_dir_all(&planted).unwrap();
    let logged = launch_from_a_planted_environment(&relay, &planted);
    assert_ne!(
        logged["env"]["WSLENV"], "OFFICINA_PLANTED/p",
        "the caller's WSLENV reached the action"
    );
}

const WSL_FLOOR: &str = r#"schema = 2

[[project]]
name = "supervision"
path = "{project}"
actions = ["remote"]

[[action]]
name = "remote"
class = "free"
runner = "wsl"
distro = "officina-test-none"
program = "/usr/bin/true"
timeout_s = 7
"#;

/// Runs the `wsl` action with `PATH` and `SYSTEMROOT` pointing at a folder
/// holding a planted `wsl.exe` (the fake tool), and returns the journaled
/// start.
fn wsl_run(relay: &Relay) -> RunStarted {
    let planted = relay.env.root.path().join("planted");
    std::fs::create_dir_all(planted.join("System32")).unwrap();
    for target in [
        planted.join("wsl.exe"),
        planted.join("System32").join("wsl.exe"),
    ] {
        std::fs::copy(common::FAKE_TOOL, target).unwrap();
    }
    let mut cmd = relay.command(&relay.project(), &["remote"]);
    cmd.env("SYSTEMROOT", &planted)
        .env("PATH", &planted)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let status = cmd.status().unwrap();
    assert_ne!(status.code(), Some(126), "the run was refused");
    assert!(relay.launches().is_empty(), "the planted wsl.exe ran");
    relay.started().remove(0)
}

#[test]
fn wsl_program_comes_from_the_system_directory() {
    let relay = Relay::with_floor(WSL_FLOOR);
    let started = wsl_run(&relay);
    assert_eq!(
        started.argv[0].to_lowercase(),
        system_wsl().to_string_lossy().to_lowercase()
    );
    assert!(started.argv.contains(&"--exec".to_owned()));
}

#[test]
fn outer_deadline_precedes_the_inner_timeout() {
    let relay = Relay::with_floor(WSL_FLOOR);
    let started = wsl_run(&relay);
    let at = started
        .argv
        .iter()
        .position(|a| a == WSL_TIMEOUT_PROGRAM)
        .unwrap();
    assert_eq!(started.argv[at + 1], "-k");
    assert_eq!(started.argv[at + 2], WSL_KILL_AFTER_S.to_string());
    let inner: u32 = started.argv[at + 3].parse().unwrap();
    let timeout_s: u32 = 7;
    assert_eq!(inner, timeout_s + WSL_TIMEOUT_MARGIN_S);
    assert!(inner > timeout_s, "the job kill at timeout_s fires first");
}

// ---- AC 13, AC 14: the journal ----

#[test]
fn run_leaves_one_start_and_one_finish_line() {
    let relay = Relay::new();
    assert_eq!(relay.run(&["probe", "host=db01"]).status.code(), Some(0));
    let (started, finished) = (relay.started(), relay.finished());
    assert_eq!((started.len(), finished.len()), (1, 1));
    let start = &started[0];
    assert_eq!(start.run_id, finished[0].run_id);
    assert_eq!(start.project, "supervision");
    assert_eq!(start.action, "probe");
    assert_eq!(start.params["host"], "db01");
    assert_eq!(start.argv[1..], ["--host", "db01"]);
    assert_eq!(
        start.catalogue_hash,
        relay
            .env
            .relay_home()
            .read_approved()
            .approved()
            .unwrap()
            .hash
    );
    assert_eq!(finished[0].outcome, RunOutcome::Exited);
    assert_eq!(finished[0].exit_code, Some(0));
    assert!(relay.refused().is_empty());
}

/// AD-32: only the window prunes the journal. A day file far past the
/// retention survives runs, refusals and listings.
#[test]
fn run_never_deletes_journal_files() {
    let relay = Relay::new();
    let home = relay.env.relay_home();
    let day = 24 * 60 * 60 * 1000;
    let old = common::now_ms() - 2 * JOURNAL_RETENTION_DAYS * day;
    let old_file = home
        .journal_dir()
        .join(officina_core::dates::event_file_name(old));
    std::fs::write(
        &old_file,
        b"{\"v\":1,\"ts\":1,\"kind\":\"relay_stopped\"}\n",
    )
    .unwrap();
    assert!(is_expired_journal_file(
        &old_file.file_name().unwrap().to_string_lossy(),
        common::now_ms()
    ));

    assert_eq!(relay.run(&["probe", "host=web01"]).status.code(), Some(0));
    assert_eq!(relay.run(&["probe", "host=nope"]).status.code(), Some(126));
    assert_eq!(relay.run(&["--list"]).status.code(), Some(0));
    assert!(old_file.exists(), "officina run deleted a journal file");
    assert!(home.journal_lock_file().exists());
}

#[test]
fn journal_lock_timeout_refuses_before_spawn() {
    let relay = Relay::new();
    let held = JournalLock::acquire(&relay.env.relay_home()).unwrap();
    let out = relay.run(&["probe", "host=web01"]);
    drop(held);
    assert_eq!(out.status.code(), Some(126));
    let lines = stderr_lines(&out);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].contains("officina: refused:") && lines[0].contains("journal"),
        "{lines:?}"
    );
    assert!(relay.launches().is_empty());
    assert!(relay.entries().is_empty());
}

/// D-760: a refusal stands whatever the journal does, and its line is not
/// lost to a lock held past the timeout: it goes to a spill file.
#[test]
fn refusal_is_journaled_when_the_lock_stays_held() {
    let relay = Relay::new();
    std::fs::write(
        relay.env.relay_home().stopped_file(),
        br#"{"since_ms":1793102400000}"#,
    )
    .unwrap();
    let held = JournalLock::acquire(&relay.env.relay_home()).unwrap();
    let out = relay.run(&["probe", "host=web01"]);
    drop(held);
    assert_refused(&relay, &out, RefusalReason::Stopped);
    assert_eq!(relay.refused().len(), 1);
}

/// D-760: a run whose end finds the journal locked past the timeout still
/// records `run_finished`, and says nothing on stderr since nothing is lost.
#[test]
fn run_end_is_journaled_when_the_lock_is_taken_meanwhile() {
    let relay = Relay::new();
    let mut cmd = relay.command(&relay.project(), &["probe", "host=web01"]);
    cmd.env("FAKE_TOOL_MODE", "sleep:1");
    let child = cmd.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while relay.started().is_empty() {
        assert!(Instant::now() < deadline, "the run never started");
        std::thread::sleep(Duration::from_millis(20));
    }
    let held = JournalLock::acquire(&relay.env.relay_home()).unwrap();
    let out = child.wait_with_output().unwrap();
    drop(held);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert!(out.stderr.is_empty(), "{:?}", stderr_lines(&out));
    let finished = relay.finished();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].run_id, relay.started()[0].run_id);
}

#[test]
fn session_id_from_env_or_null() {
    let relay = Relay::new();
    let mut cmd = relay.command(&relay.project(), &["probe", "host=web01"]);
    cmd.env(
        "CLAUDE_CODE_SESSION_ID",
        "00000000-0000-0000-0000-000000000001",
    );
    assert_eq!(common::run_with(cmd, b"").status.code(), Some(0));
    relay.run(&["probe", "host=web01"]);
    relay.run(&["probe", "host=nope"]);
    let started = relay.started();
    assert_eq!(
        started[0].session_id.as_deref(),
        Some("00000000-0000-0000-0000-000000000001")
    );
    assert_eq!(started[1].session_id, None);
    assert_eq!(relay.refused()[0].session_id, None);
    let raw = std::fs::read_dir(relay.env.relay_home().journal_dir())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".jsonl"))
        .map(|e| std::fs::read_to_string(e.path()).unwrap())
        .collect::<String>();
    assert!(raw.contains("\"session_id\":null"), "written as null");
}

// ---- AC 15: --list ----

fn list(relay: &Relay, dir: &Path) -> String {
    let out = relay.run_in(dir, &["--list"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(relay.launches().is_empty());
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn list_prints_the_projects_approved_actions() {
    let relay = Relay::new();
    let roles = relay.project().join("roles");
    std::fs::create_dir_all(&roles).unwrap();
    let text = list(&relay, &roles);
    assert!(text.starts_with("Project supervision (guarded)"), "{text}");
    assert!(text.contains("officina run probe host=<...>"), "{text}");
    assert!(text.contains("host: one of db01, web01"), "{text}");
    assert!(
        text.contains("group: the whole value matches lab-[a-z0-9-]{1,30}"),
        "{text}"
    );
    assert!(text.contains("Reads one host"), "{text}");
    let notes = list(&relay, &relay.notes());
    assert!(notes.starts_with("Project notes (free)"), "{notes}");
    assert!(
        !notes.contains("officina run tool"),
        "another project's action: {notes}"
    );
}

#[test]
fn list_explains_human_actions_and_their_dry_run() {
    let relay = Relay::new();
    let text = list(&relay, &relay.project());
    let approvals = text
        .matches("runs once per approval the author gives in the Officina window")
        .count();
    assert_eq!(approvals, 2, "approve-me and apply-sensitive: {text}");
    assert!(
        text.contains("asks for approval only after its dry run `officina run verify` succeeded"),
        "{text}"
    );
}

/// Story 5.12 AC 1 (AD-86): `run --list` gives every action's class.
#[test]
fn run_list_shows_the_class() {
    let relay = Relay::new();
    let text = list(&relay, &relay.project());
    for (call, class) in [
        ("officina run probe host=<...>", "free"),
        ("officina run apply group=<...>", "after-check"),
        ("officina run approve-me group=<...>", "human"),
        ("officina run apply-auto group=<...>", "auto"),
    ] {
        assert!(
            text.contains(&format!("{call}  (class {class})\n")),
            "{call}: {text}"
        );
    }
    assert!(!text.contains("not available"), "{text}");
}

/// The author's decision on request `id`, as the window takes it.
fn author_decides(relay: &Relay, id: &str, approve: bool, reason: Option<AuthorReason>) {
    officina_host::relay::authority::decide_approval(&relay.env.relay_home(), id, approve, reason)
        .unwrap();
}

/// Story 5.12 AC 1 (AD-86): an `auto` action, its check run, writes a
/// request and waits like a `human` one; the author's approval runs it once,
/// journaled with its approval and its class.
#[test]
fn auto_creates_a_request_and_waits_like_human() {
    let Some(git) = GitRelay::new("auto_creates_a_request_and_waits_like_human") else {
        return;
    };
    let before = checked(&git, "lab-web");
    let relay = &git.relay;
    let out = run_waiting(
        relay,
        relay.command(
            &relay.project(),
            &["apply-auto", "group=lab-web", "--wait", "30"],
        ),
        Box::new(|relay: &Relay, id: &str| author_decides(relay, id, true, None)),
    );
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(relay.launches().len(), before + 1);
    let id = request_ids(relay).pop().unwrap();
    let started = relay.started().pop().unwrap();
    assert_eq!(started.action, "apply-auto");
    assert_eq!(
        started.class,
        officina_core::relay::catalogue::ActionClass::Auto
    );
    assert_eq!(started.approval_id.as_deref(), Some(id.as_str()));
    let again = relay.run(&["apply-auto", "group=lab-web", "--wait", "1"]);
    assert_refused_after(relay, &again, RefusalReason::ApprovalPending, before + 1);
}

/// Story 5.12 AC 1 (D-570): every `auto` action names its check, so no
/// request is made before the check passed.
#[test]
fn auto_with_a_check_is_refused_until_the_check_passed() {
    let Some(git) = GitRelay::new("auto_with_a_check_is_refused_until_the_check_passed") else {
        return;
    };
    let out = git.run(&["apply-auto", "group=lab-web"]);
    assert_refused(&git.relay, &out, RefusalReason::CheckRequired);
    assert!(request_ids(&git.relay).is_empty(), "no request");
    let before = checked(&git, "lab-web");
    let out = git.run(&["apply-auto", "group=lab-web", "--wait", "1"]);
    assert_refused_after(&git.relay, &out, RefusalReason::ApprovalPending, before);
    assert_eq!(request_ids(&git.relay).len(), 1);
}

/// Story 5.12 AC 1 (D-151): an approval of an `auto` action is checked
/// again at consumption on the files as they are then: an edit made during
/// the wait refuses `check_not_satisfied`, nothing runs, the refusal is
/// journaled.
#[test]
fn an_auto_approval_is_refused_after_a_tree_change() {
    let Some(git) = GitRelay::new("an_auto_approval_is_refused_after_a_tree_change") else {
        return;
    };
    let before = checked(&git, "lab-web");
    let relay = &git.relay;
    let out = run_waiting(
        relay,
        relay.command(
            &relay.project(),
            &["apply-auto", "group=lab-web", "--wait", "30"],
        ),
        Box::new(|relay: &Relay, id: &str| {
            std::fs::write(relay.project().join("playbook.yml"), "- hosts: all\n").unwrap();
            author_decides(relay, id, true, None);
        }),
    );
    assert_refused_after(relay, &out, RefusalReason::CheckNotSatisfied, before);
}

/// Story 5.12 AC 1 (D-151): the check is evaluated again, not only the
/// files: a check that aged past its half hour during the wait refuses the
/// approval, which stays unconsumed for a call after a fresh check.
#[test]
fn the_refused_approval_stays_unconsumed() {
    use officina_core::relay::approvals::{FileKind, file_name};
    let Some(git) = GitRelay::new("the_refused_approval_stays_unconsumed") else {
        return;
    };
    // A check that turns stale ten seconds from now, during the wait.
    seed_check(&git, "ageing", CHECK_VALID_MS - 10_000);
    let relay = &git.relay;
    let out = run_waiting(
        relay,
        relay.command(
            &relay.project(),
            &["apply-auto", "group=lab-web", "--wait", "30"],
        ),
        Box::new(|relay: &Relay, id: &str| {
            std::thread::sleep(Duration::from_secs(12));
            author_decides(relay, id, true, None);
        }),
    );
    assert_refused(relay, &out, RefusalReason::CheckNotSatisfied);
    assert!(relay.launches().is_empty());
    let id = request_ids(relay).pop().unwrap();
    let dir = relay.env.relay_home().approvals_dir();
    assert!(dir.join(file_name(&id, FileKind::Approved)).is_file());
    assert!(!dir.join(file_name(&id, FileKind::Consumed)).exists());
}

/// Story 5.12 AC 1 and AC 5 (AD-33, FR-9): an `auto` rejection carries its
/// reason and blocks the same request for 15 minutes, as a `human` one.
#[test]
fn an_auto_rejection_blocks_for_15_minutes() {
    use officina_core::relay::approvals::{Decision, FileKind, file_name};
    let Some(git) = GitRelay::new("an_auto_rejection_blocks_for_15_minutes") else {
        return;
    };
    let before = checked(&git, "lab-web");
    let relay = &git.relay;
    let args = ["apply-auto", "group=lab-web", "--wait", "30"];
    let out = run_waiting(
        relay,
        relay.command(&relay.project(), &args),
        Box::new(|relay: &Relay, id: &str| {
            author_decides(relay, id, false, Some(AuthorReason::NotNow));
        }),
    );
    assert_refused_after(relay, &out, RefusalReason::ApprovalRejected, before);
    let id = request_ids(relay).pop().unwrap();
    let rejected: Decision = serde_json::from_slice(
        &std::fs::read(
            relay
                .env
                .relay_home()
                .approvals_dir()
                .join(file_name(&id, FileKind::Rejected)),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(rejected.reason, Some(AuthorReason::NotNow));
    let again = relay.run(&args);
    assert_refused_after(relay, &again, RefusalReason::ApprovalRejected, before);
    assert_eq!(request_ids(relay), [id], "no new request while blocked");
}

/// Story 5.4 AC 7 (AD-76, D-131): an approval in the previous format
/// parses, keeps its hash, and `officina run` refuses every action as a
/// pending change until the author approves once; the approval then holds
/// the new format.
#[test]
fn previous_format_approval_refuses_as_pending() {
    use officina_core::relay::catalogue::{ApprovedCatalogue, CANONICAL_FORMAT};
    use officina_host::relay::home::ApprovedState;

    let relay = Relay::approved_in_the_previous_format();
    let home = relay.env.relay_home();
    let previous: ApprovedCatalogue =
        serde_json::from_str(common::previous_format::PREVIOUS_APPROVAL).unwrap();
    let ApprovedState::Valid(read) = home.read_approved() else {
        panic!("the previous format reads");
    };
    assert_eq!(
        read.hash, previous.hash,
        "its stored hash, never recomputed"
    );
    assert_eq!(read.canonical.format, CANONICAL_FORMAT - 1);

    let dir = relay.golden_project();
    let out = relay.run_in(&dir, &["status", "host=web01", "unit=nginx.service"]);
    assert_refused(&relay, &out, RefusalReason::CatalogueChanged);
    assert!(
        stderr_text(&out).contains("catalogue changed, approval pending"),
        "{}",
        stderr_text(&out)
    );
    let text = list(&relay, &dir);
    assert!(
        text.contains("A catalogue change awaits approval"),
        "{text}"
    );

    let approved = relay.env.approve();
    assert_ne!(approved, previous.hash);
    let ApprovedState::Valid(now) = home.read_approved() else {
        panic!("the new approval reads");
    };
    assert_eq!(now.canonical.format, CANONICAL_FORMAT);
    assert!(
        !now.canonical.templates.is_empty(),
        "the templates are approved"
    );
    let after = list(&relay, &dir);
    assert!(!after.contains("awaits approval"), "{after}");
}

#[test]
fn list_outside_every_project() {
    let relay = Relay::new();
    let text = list(&relay, &relay.outside());
    assert!(text.contains("lies in no project"), "{text}");
    let unapproved = Relay::unapproved(FLOOR);
    assert!(list(&unapproved, &unapproved.project()).contains("No relay catalogue is approved"));
}

#[test]
fn list_reports_stop_and_pending_change() {
    let relay = Relay::new();
    std::fs::write(relay.env.relay_home().stopped_file(), b"{}").unwrap();
    relay.write(&FLOOR.replace("timeout_s = 1", "timeout_s = 2"));
    let text = list(&relay, &relay.project());
    assert!(text.contains("The relay is stopped"), "{text}");
    assert!(
        text.contains("A catalogue change awaits approval"),
        "{text}"
    );
    assert!(
        text.contains("officina run probe"),
        "the approved actions stay listed"
    );
}

// ---- adversarial ----

/// A program that exits at once and leaves a process behind holding its
/// output (a daemon): the run neither waits for that process nor lets it
/// outlive it, and the program's exit code still passes through.
#[test]
fn a_process_left_behind_neither_holds_nor_outlives_the_run() {
    let relay = Relay::new();
    let heartbeat = relay.env.root.path().join("heartbeat");
    let mut cmd = relay.command(&relay.project(), &["tool"]);
    cmd.env("FAKE_TOOL_MODE", "orphan")
        .env("FAKE_TOOL_HEARTBEAT", &heartbeat)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let started = Instant::now();
    let status = cmd.status().unwrap();
    let took = started.elapsed();
    assert_eq!(status.code(), Some(0));
    assert!(took < Duration::from_secs(5), "{took:?}");
    let stopped = heartbeat_stopped(&heartbeat);
    stop_heartbeat(&heartbeat);
    assert!(stopped, "the process left behind outlived the run");
    let finished = relay.finished();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].exit_code, Some(0));
}

/// Concurrent runs share the journal: every start and end line is whole and
/// every run has its own id.
#[test]
fn concurrent_runs_each_journal_one_start_and_one_end() {
    const RUNS: usize = 8;
    let relay = Relay::new();
    std::thread::scope(|scope| {
        for _ in 0..RUNS {
            scope.spawn(|| {
                let out = relay.run(&["probe", "host=web01"]);
                assert_eq!(out.status.code(), Some(0));
            });
        }
    });
    let (started, finished) = (relay.started(), relay.finished());
    assert_eq!((started.len(), finished.len()), (RUNS, RUNS));
    let mut ids: Vec<&str> = started.iter().map(|s| s.run_id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), RUNS, "run ids are unique");
    assert_eq!(relay.launches().len(), RUNS);
}

// ---- story 2.6: after-check and the tree fingerprint ----

/// A relay whose guarded project is a git work tree with one commit, built
/// with the test's own `git` (D-23); `officina run` lists it with the `git`
/// Git for Windows records in the registry (D-173). `None`, with the reason
/// printed, on a machine without either.
struct GitRelay {
    relay: Relay,
}

impl GitRelay {
    fn new(test: &str) -> Option<GitRelay> {
        let git_dir = git_dir_or_skip(test)?;
        if officina_host::relay::tree::resolve_git().is_none() {
            println!("skipped {test}: Git for Windows records no install folder here");
            return None;
        }
        let relay = Relay::new();
        let project = relay.project();
        git(&git_dir, &project, &["init", "-q"]);
        git(&git_dir, &project, &["config", "core.autocrlf", "false"]);
        std::fs::write(project.join("playbook.yml"), "- hosts: lab\n").unwrap();
        git(&git_dir, &project, &["add", "playbook.yml"]);
        git(&git_dir, &project, &["commit", "-q", "-m", "first"]);
        Some(GitRelay { relay })
    }

    fn run(&self, args: &[&str]) -> Output {
        common::run_with(self.relay.command(&self.relay.project(), args), b"")
    }

    /// The fingerprint `officina run` should record now.
    fn tree(&self) -> Option<TreeFingerprint> {
        officina_host::relay::tree::fingerprint(&self.relay.project()).unwrap()
    }
}

/// Runs the check and returns how many times the tool started so far.
fn checked(git: &GitRelay, group: &str) -> usize {
    let out = git.run(&["verify", &format!("group={group}")]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    git.relay.launches().len()
}

/// The `after-check` refusal: exit 126, one stderr line naming the check,
/// no launch beyond the `launches_before` of the check, one `refused` line.
fn assert_check_required(git: &GitRelay, out: &Output, launches_before: usize) {
    assert_eq!(out.status.code(), Some(126), "{:?}", stderr_lines(out));
    let lines = stderr_lines(out);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("officina: refused: run `verify` first"),
        "{lines:?}"
    );
    assert!(lines[0].ends_with("(check_required)"), "{lines:?}");
    assert_eq!(git.relay.launches().len(), launches_before);
    assert_eq!(
        git.relay.refused().last().map(|r| r.reason),
        Some(RefusalReason::CheckRequired)
    );
}

#[test]
fn check_then_apply_runs() {
    let Some(git) = GitRelay::new("check_then_apply_runs") else {
        return;
    };
    checked(&git, "lab-web");
    let out = git.run(&["apply", "group=lab-web"]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(git.relay.launches().len(), 2);
    let started = git.relay.started();
    assert!(started[0].tree.is_some(), "a check records its tree");
    assert_eq!(
        started[0].tree, started[1].tree,
        "the apply ran on the checked tree"
    );
    assert_eq!(started[0].tree, git.tree());
    assert_eq!(started[0].timeout_s, Some(540));
    let probe = git.run(&["probe", "host=web01"]);
    assert_eq!(probe.status.code(), Some(0));
    assert_eq!(
        git.relay.started()[2].tree,
        None,
        "an action no after-check names records no tree"
    );
}

#[test]
fn apply_without_check_is_refused() {
    let Some(git) = GitRelay::new("apply_without_check_is_refused") else {
        return;
    };
    let out = git.run(&["apply", "group=lab-web"]);
    assert_check_required(&git, &out, 0);
}

#[test]
fn apply_with_other_params_is_refused() {
    let Some(git) = GitRelay::new("apply_with_other_params_is_refused") else {
        return;
    };
    let before = checked(&git, "lab-web");
    let out = git.run(&["apply", "group=lab-db"]);
    assert_check_required(&git, &out, before);
}

#[test]
fn apply_after_a_tree_change_is_refused() {
    let Some(git) = GitRelay::new("apply_after_a_tree_change_is_refused") else {
        return;
    };
    let before = checked(&git, "lab-web");
    std::fs::write(git.relay.project().join("playbook.yml"), "- hosts: all\n").unwrap();
    let out = git.run(&["apply", "group=lab-web"]);
    assert_check_required(&git, &out, before);
}

#[test]
fn apply_after_an_untracked_edit_is_refused() {
    let Some(git) = GitRelay::new("apply_after_an_untracked_edit_is_refused") else {
        return;
    };
    let vars = git.relay.project().join("vars.yml");
    std::fs::write(&vars, "group: aaaa\n").unwrap();
    let before = checked(&git, "lab-web");
    std::fs::write(&vars, "group: bbbb\n").unwrap();
    let out = git.run(&["apply", "group=lab-web"]);
    assert_check_required(&git, &out, before);
}

/// AC 1 of story 2.14 (D-173): without the `git` Git for Windows records,
/// an action that binds a git work tree refuses `git_unavailable`; an action
/// that binds nothing, and a check outside git, still run.
#[test]
fn git_unavailable_refuses_actions_that_need_a_fingerprint() {
    let Some(git) = GitRelay::new("git_unavailable_refuses_actions_that_need_a_fingerprint") else {
        return;
    };
    let without_git = |relay: &Relay, dir: &Path, args: &[&str], value: &str| {
        let mut cmd = relay.command(dir, args);
        cmd.env(ENV_TEST_GIT, value);
        common::run_with(cmd, b"")
    };
    let missing = git.relay.outside().join("no-git.exe");
    for value in [String::new(), slashed(&missing)] {
        let out = without_git(
            &git.relay,
            &git.relay.project(),
            &["verify", "group=lab-web"],
            &value,
        );
        assert_refused(&git.relay, &out, RefusalReason::GitUnavailable);
        assert!(
            stderr_lines(&out)[0].contains("git work tree"),
            "{:?}",
            stderr_lines(&out)
        );
    }
    let probe = without_git(
        &git.relay,
        &git.relay.project(),
        &["probe", "host=web01"],
        "",
    );
    assert_eq!(probe.status.code(), Some(0), "{:?}", stderr_lines(&probe));

    let plain = Relay::new();
    let out = without_git(&plain, &plain.project(), &["verify", "group=lab-web"], "");
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
    assert_eq!(
        plain.started()[0].tree,
        None,
        "outside git, nothing to bind"
    );
}

/// Seeds a successful check of `verify group=lab-web` on the current tree,
/// started `ago` milliseconds ago, as an earlier run would have journaled it
/// (D-25).
fn seed_check(git: &GitRelay, run_id: &str, ago: i64) {
    let home = git.relay.env.relay_home();
    let hash = home.read_approved().approved().unwrap().hash.clone();
    let now = common::now_ms();
    let stream = StreamSummary {
        bytes: 0,
        sha256: sha256_hex(b""),
    };
    let start = JournalEntry::RunStarted(RunStarted {
        run_id: run_id.to_owned(),
        project: "supervision".to_owned(),
        session_id: None,
        action: "verify".to_owned(),
        class: officina_core::relay::catalogue::ActionClass::Free,
        params: BTreeMap::from([("group".to_owned(), "lab-web".to_owned())]),
        argv: vec![
            common::fake_tool_path(),
            "--check".to_owned(),
            "lab-web".to_owned(),
        ],
        method: None,
        url: None,
        credential: None,
        catalogue_hash: hash,
        approval_id: None,
        tree: git.tree(),
        timeout_s: Some(540),
    });
    let end = JournalEntry::RunFinished(RunFinished {
        body_broke_off: false,
        run_id: run_id.to_owned(),
        outcome: RunOutcome::Exited,
        exit_code: Some(0),
        duration_ms: 1000,
        stdout: Some(stream.clone()),
        stderr: Some(stream),
        dropped_bytes: 0,
        status: None,
        body_bytes: None,
        body_sha256: None,
        detail: None,
    });
    officina_host::relay::journal::append(&home, &[start], now - ago).unwrap();
    officina_host::relay::journal::append(&home, &[end], now - ago + 1000).unwrap();
}

#[test]
fn stale_check_is_refused() {
    let Some(git) = GitRelay::new("stale_check_is_refused") else {
        return;
    };
    seed_check(&git, "stale", CHECK_VALID_MS + 60_000);
    let out = git.run(&["apply", "group=lab-web"]);
    assert_check_required(&git, &out, 0);
    // The same check, recent: the seeded line is what the apply reads.
    seed_check(&git, "recent", 10 * 60_000);
    let out = git.run(&["apply", "group=lab-web"]);
    assert_eq!(out.status.code(), Some(0), "{:?}", stderr_lines(&out));
}

/// Writes `<id>.approved.json` as a process other than the window would:
/// the request's key, decided now, through no authority call.
fn forge_approval(relay: &Relay, id: &str) {
    use officina_core::relay::approvals::{ApprovalRequest, Decision, FileKind, file_name};
    let dir = relay.env.relay_home().approvals_dir();
    let request: ApprovalRequest =
        serde_json::from_slice(&std::fs::read(dir.join(file_name(id, FileKind::Request))).unwrap())
            .unwrap();
    let decision = Decision::by_author(common::now_ms(), &request.request_key, None);
    std::fs::write(
        dir.join(file_name(id, FileKind::Approved)),
        serde_json::to_vec(&decision).unwrap(),
    )
    .unwrap();
}

/// Adversarial pass of story 2.14, the audits' forged approval: a process
/// writes the approval file the waiting call looks for. The call refuses
/// `approval_not_given`, nothing runs, the file stays, and the Relay view
/// lists it until the author acknowledges it.
#[test]
fn a_forged_approval_file_does_not_run() {
    let relay = Relay::new();
    let cmd = relay.command(
        &relay.project(),
        &["approve-me", "group=lab-web", "--wait", "30"],
    );
    let out = run_waiting(
        &relay,
        cmd,
        Box::new(|relay: &Relay, id: &str| forge_approval(relay, id)),
    );
    assert_refused(&relay, &out, RefusalReason::ApprovalNotGiven);
    let home = relay.env.relay_home();
    let id = request_ids(&relay).pop().unwrap();
    assert!(
        home.approvals_dir()
            .join(officina_core::relay::approvals::file_name(
                &id,
                officina_core::relay::approvals::FileKind::Approved
            ))
            .is_file(),
        "left unused"
    );
    let (lines, _) = officina_host::relay::journal::read_recent(&home, common::now_ms());
    let anomalies = officina_host::relay::view::read_anomalies(
        &home,
        &lines,
        home.read_approved().approved().map(|a| a.hash.as_str()),
        home.relay_stopped(),
    );
    assert_eq!(anomalies.len(), 1, "{anomalies:#?}");
    assert!(anomalies[0].detail.contains(&id));
}

/// What a process writing relay files would do to run another catalogue:
/// write `floor` as the relay floor and approve it in `relay/approved.json`
/// by hand, behind the window's back.
fn forge_catalogue(relay: &Relay, floor: &str) {
    relay.write(floor);
    let home = relay.env.relay_home();
    let floor_path = home.relay_floor(&home.read_approved());
    let current = officina_host::relay::home::load_relay_floor(&floor_path).unwrap();
    let forged = officina_core::relay::catalogue::ApprovedCatalogue::approving(
        &current,
        &floor_path.to_string_lossy(),
        common::now_ms(),
    );
    std::fs::write(home.approved_file(), serde_json::to_vec(&forged).unwrap()).unwrap();
}

/// Story 2.16 AC 6 (D-175), the delta audit's forged catalogue: a process
/// rewrites the floor and `relay/approved.json` together. `officina run`
/// refuses `catalogue_unconfirmed`, nothing starts, and `--list` says why.
#[test]
fn a_catalogue_the_window_did_not_approve_is_not_run() {
    let relay = Relay::new();
    forge_catalogue(&relay, &FLOOR.replace("timeout_s = 1", "timeout_s = 2"));
    let out = relay.run(&["probe", "host=web01"]);
    assert_refused(&relay, &out, RefusalReason::CatalogueUnconfirmed);
    assert!(relay.launches().is_empty());
    let listed = list(&relay, &relay.project());
    assert!(
        listed.contains("not the one the Officina window approved"),
        "{listed}"
    );
}

/// Story 2.16 AC 6 (D-175): the author stops the relay and a process
/// deletes the stop flag; the window's record still holds it stopped.
#[test]
fn a_stop_flag_deleted_behind_the_window_still_stops() {
    let relay = Relay::new();
    let home = relay.env.relay_home();
    assert!(officina_host::relay::authority::set_relay_stopped(&home, true).unwrap());
    std::fs::remove_file(home.stopped_file()).unwrap();
    assert_refused(
        &relay,
        &relay.run(&["probe", "host=web01"]),
        RefusalReason::Stopped,
    );
    assert!(officina_host::relay::authority::set_relay_stopped(&home, false).unwrap());
    assert_eq!(relay.run(&["probe", "host=web01"]).status.code(), Some(0));
}

/// Story 2.16 AC 6 (D-175): with the window's record deleted or broken,
/// every action is refused `decision_record_unreadable`, the Relay view
/// lists the one anomaly, and acknowledging it starts a new record from
/// the state the window finds, after which actions run again.
#[test]
fn a_broken_record_refuses_until_the_author_acknowledges_it() {
    for (name, damage) in [
        ("deleted", None),
        ("corrupt", Some(&b"{ not json"[..])),
        ("another schema", Some(&br#"{"schema":1,"since_ms":1}"#[..])),
    ] {
        let relay = Relay::new();
        let home = relay.env.relay_home();
        match damage {
            Some(bytes) => std::fs::write(home.decisions_file(), bytes).unwrap(),
            None => std::fs::remove_file(home.decisions_file()).unwrap(),
        }
        assert_refused(
            &relay,
            &relay.run(&["probe", "host=web01"]),
            RefusalReason::DecisionRecordUnreadable,
        );
        let (lines, _) = officina_host::relay::journal::read_recent(&home, common::now_ms());
        let anomalies = officina_host::relay::view::read_anomalies(
            &home,
            &lines,
            home.read_approved().approved().map(|a| a.hash.as_str()),
            home.relay_stopped(),
        );
        let keys: Vec<&str> = anomalies.iter().map(|a| a.key.as_str()).collect();
        assert_eq!(
            keys,
            [officina_core::relay::record::record_anomaly_key()],
            "{name}"
        );
        assert!(
            officina_host::relay::authority::ensure_record(&home).is_ok()
                && officina_host::relay::authority::approve_catalogue(
                    &home,
                    &home.read_approved().approved().unwrap().hash
                )
                .is_err(),
            "{name}: the window neither restarts the record nor decides on its own"
        );
        officina_host::relay::authority::acknowledge_anomaly(
            &home,
            &officina_core::relay::record::record_anomaly_key(),
        )
        .unwrap();
        assert_eq!(
            relay.run(&["probe", "host=web01"]).status.code(),
            Some(0),
            "{name}"
        );
    }
}

/// Story 2.16 AC 6 (D-175), the delta audit's reused rejection: the author
/// rejects a request; a process deletes the rejection and writes an
/// approval for the same request. The record holds the request as
/// rejected: the next call refuses `approval_not_given` and runs nothing.
#[test]
fn a_rejected_request_rewritten_as_approved_does_not_run() {
    use officina_core::relay::approvals::{FileKind, file_name};
    let relay = Relay::new();
    let args = ["approve-me", "group=lab-web", "--wait", "30"];
    let out = run_waiting(
        &relay,
        relay.command(&relay.project(), &args),
        Box::new(|relay: &Relay, id: &str| {
            officina_host::relay::authority::decide_approval(
                &relay.env.relay_home(),
                id,
                false,
                None,
            )
            .unwrap();
        }),
    );
    assert_refused(&relay, &out, RefusalReason::ApprovalRejected);
    let id = request_ids(&relay).pop().unwrap();
    let dir = relay.env.relay_home().approvals_dir();
    std::fs::remove_file(dir.join(file_name(&id, FileKind::Rejected))).unwrap();
    forge_approval(&relay, &id);
    let again = relay.run(&args);
    assert_refused(&relay, &again, RefusalReason::ApprovalNotGiven);
    assert!(relay.launches().is_empty());
}

/// Adversarial pass of story 2.14, the audits' first chain: a session puts
/// a `git.exe` first on `PATH` for its relay calls, runs the check, edits
/// the playbook and runs the `after-check` action. The fingerprint never
/// runs the shim (no launch of it is logged) and sees the edit: refused.
#[test]
fn a_path_shim_cannot_hide_an_edit_after_the_check() {
    let Some(git) = GitRelay::new("a_path_shim_cannot_hide_an_edit_after_the_check") else {
        return;
    };
    let shim = git.relay.outside().join("shim");
    std::fs::create_dir_all(&shim).unwrap();
    std::fs::copy(common::FAKE_TOOL, shim.join("git.exe")).unwrap();
    let with_shim = |args: &[&str]| {
        let mut cmd = git.relay.command(&git.relay.project(), args);
        cmd.env("PATH", &shim);
        common::run_with(cmd, b"")
    };
    let check = with_shim(&["verify", "group=lab-web"]);
    assert_eq!(check.status.code(), Some(0), "{:?}", stderr_lines(&check));
    std::fs::write(git.relay.project().join("playbook.yml"), "- hosts: all\n").unwrap();
    let apply = with_shim(&["apply", "group=lab-web"]);
    assert_check_required(&git, &apply, 1);
    assert!(
        git.relay
            .launches()
            .iter()
            .all(|launch| launch["argv"][0] != "--no-pager"),
        "the shim on PATH ran as git: {:?}",
        git.relay.launches()
    );
}

// ---- story 6.2: `officina run` compares the relay-section hash (D-244) ----

/// A CA file of one certificate block, for `[workshop] ca_file`.
const WORKSHOP_CA: &str =
    "-----BEGIN CERTIFICATE-----\nTUlJQmV4YW1wbGU=\n-----END CERTIFICATE-----\n";

/// [`FLOOR`] with a `[workshop]` table naming a real CA file and the
/// guarded project's `[project.workshop]` naming a real Dockerfile,
/// approved with both read, as the window reads them. Returns the relay,
/// the Dockerfile and the CA file.
fn workshop_relay() -> (Relay, PathBuf, PathBuf) {
    let relay = Relay::unapproved(FLOOR);
    let dockerfile = relay.project().join("tools").join("Dockerfile");
    std::fs::create_dir_all(dockerfile.parent().unwrap()).unwrap();
    std::fs::write(&dockerfile, "FROM officina-workshop:local\n").unwrap();
    let ca = relay.env.root.path().join("corporate-ca.pem");
    std::fs::write(&ca, WORKSHOP_CA).unwrap();
    let supervision = "\"apply-sensitive\", \"apply-auto\"]\n";
    assert!(FLOOR.contains(supervision));
    let floor = format!(
        "{}\n[workshop]\nca_file = \"{}\"\n",
        FLOOR.replacen(
            supervision,
            &format!("{supervision}\n[project.workshop]\ndockerfile = \"tools/Dockerfile\"\n"),
            1,
        ),
        slashed(&ca)
    );
    relay.write(&floor);
    let approved = relay.env.approve();
    assert_eq!(global_hash(&relay), approved, "the window read the files");
    (relay, dockerfile, ca)
}

/// The floor's global hash as the window computes it, files read.
fn global_hash(relay: &Relay) -> String {
    officina_host::relay::home::load_relay_floor(&relay.env.floor_file())
        .unwrap()
        .hash
}

#[test]
fn an_edited_project_dockerfile_does_not_block_relay_actions() {
    let (relay, dockerfile, _) = workshop_relay();
    let approved = global_hash(&relay);
    std::fs::write(
        &dockerfile,
        "FROM officina-workshop:local\nRUN echo edited\n",
    )
    .unwrap();
    assert_ne!(
        global_hash(&relay),
        approved,
        "a workshop launch is refused on the global hash"
    );
    assert_eq!(relay.run(&["probe", "host=web01"]).status.code(), Some(0));
}

#[test]
fn an_unreadable_ca_file_does_not_block_relay_actions() {
    let (relay, _, ca) = workshop_relay();
    let approved = global_hash(&relay);
    std::fs::rename(&ca, ca.with_extension("moved")).unwrap();
    assert_ne!(
        global_hash(&relay),
        approved,
        "an unreadable CA file turns the workshop off"
    );
    assert_eq!(relay.run(&["probe", "host=web01"]).status.code(), Some(0));
}

#[test]
fn an_edited_action_still_refuses_until_approved() {
    let (relay, _, _) = workshop_relay();
    let floor = std::fs::read_to_string(relay.env.floor_file()).unwrap();
    let edited = floor.replacen("\"web01\", \"db01\"", "\"web01\", \"db02\"", 1);
    assert_ne!(edited, floor);
    std::fs::write(relay.env.floor_file(), &edited).unwrap();
    assert_refused(
        &relay,
        &relay.run(&["probe", "host=web01"]),
        RefusalReason::CatalogueChanged,
    );
    relay.env.approve();
    assert_eq!(relay.run(&["probe", "host=web01"]).status.code(), Some(0));
}
