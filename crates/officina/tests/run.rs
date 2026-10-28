//! `officina run` end to end (story 2.5, AD-29 to AD-32, D-20, D-21), with
//! `officina-fake-tool` as every action's program. Windows only: a `native`
//! program is a Windows absolute path (AD-22) and the process tree is a job
//! object (AD-31); Linux is v1.

#![cfg(windows)]

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use common::fake_output::{stderr_byte, stdout_byte, stream};
use common::{TestEnv, heartbeat_len, heartbeat_stopped, read_tool_log, stop_heartbeat};
use officina_core::relay::argv::{
    ArgvRefusal, Launch, WSL_KILL_AFTER_S, WSL_TIMEOUT_MARGIN_S, WSL_TIMEOUT_PROGRAM,
};
use officina_core::relay::journal::{
    JournalEntry, RefusalReason, Refused, RunFinished, RunOutcome, RunStarted,
};
use officina_core::relay::sha256_hex;
use officina_host::relay::journal::JournalLock;

/// The relay floor of most tests: a guarded project and a free one, both
/// real folders, every action's program the fake tool. `{project}`,
/// `{notes}` and `{tool}` are filled in by [`Relay::with_floor`].
const FLOOR: &str = r#"schema = 2

[[project]]
name = "supervision"
path = "{project}"
actions = ["probe", "tool", "short", "capped", "verify", "apply", "approve-me"]

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
"#;

struct Relay {
    env: TestEnv,
}

fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

impl Relay {
    /// A relay whose floor is `floor`, approved through the window's
    /// authority (story 2.2), never written by hand.
    fn with_floor(floor: &str) -> Relay {
        let relay = Relay {
            env: TestEnv::new(),
        };
        for dir in [relay.project(), relay.notes(), relay.outside()] {
            std::fs::create_dir_all(dir).unwrap();
        }
        relay.write(floor);
        relay.env.approve();
        relay
    }

    fn new() -> Relay {
        Relay::with_floor(FLOOR)
    }

    /// Writes a floor, filling in the folders and the tool, unapproved.
    fn write(&self, floor: &str) {
        self.env.write_floor(
            &floor
                .replace("{project}", &slashed(&self.project()))
                .replace("{notes}", &slashed(&self.notes())),
        );
    }

    fn project(&self) -> PathBuf {
        self.env.root.path().join("work").join("Supervision")
    }

    fn notes(&self) -> PathBuf {
        self.env.root.path().join("work").join("Notes")
    }

    fn outside(&self) -> PathBuf {
        self.env.root.path().join("elsewhere")
    }

    /// `officina run <args>` from `dir`, the tool logging its launches.
    fn command(&self, dir: &Path, args: &[&str]) -> Command {
        let mut full = vec!["run"];
        full.extend_from_slice(args);
        let mut cmd = self.env.officina(&full);
        cmd.current_dir(dir)
            .env("FAKE_TOOL_LOG", self.env.tool_log());
        cmd
    }

    fn run_in(&self, dir: &Path, args: &[&str]) -> Output {
        common::run_with(self.command(dir, args), b"")
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_in(&self.project(), args)
    }

    fn launches(&self) -> Vec<serde_json::Value> {
        read_tool_log(&self.env.tool_log())
    }

    fn entries(&self) -> Vec<JournalEntry> {
        self.env.journal_after_approvals()
    }

    fn refused(&self) -> Vec<Refused> {
        self.entries()
            .into_iter()
            .filter_map(|e| match e {
                JournalEntry::Refused(r) => Some(r),
                _ => None,
            })
            .collect()
    }

    fn started(&self) -> Vec<RunStarted> {
        self.entries()
            .into_iter()
            .filter_map(|e| match e {
                JournalEntry::RunStarted(s) => Some(s),
                _ => None,
            })
            .collect()
    }

    fn finished(&self) -> Vec<RunFinished> {
        self.entries()
            .into_iter()
            .filter_map(|e| match e {
                JournalEntry::RunFinished(f) => Some(f),
                _ => None,
            })
            .collect()
    }
}

fn stderr_lines(out: &Output) -> Vec<String> {
    String::from_utf8_lossy(&out.stderr)
        .lines()
        .map(str::to_owned)
        .collect()
}

fn code_of(reason: RefusalReason) -> String {
    serde_json::to_value(reason)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

/// Asserts a refusal: exit 126, one stderr line naming the code, the tool
/// never started. Returns the reason the journal recorded last.
fn assert_refused(relay: &Relay, out: &Output, reason: RefusalReason) {
    assert_eq!(out.status.code(), Some(126), "{:?}", stderr_lines(out));
    let lines = stderr_lines(out);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("officina: refused: "), "{lines:?}");
    assert!(
        lines[0].ends_with(&format!("({})", code_of(reason))),
        "{lines:?}"
    );
    assert!(relay.launches().is_empty(), "the tool must not start");
    assert_eq!(relay.refused().last().map(|r| r.reason), Some(reason));
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
        RefusalReason::ClassNotAvailable,
    ];
    for (i, expected) in steps.iter().enumerate() {
        let (dir, args): (PathBuf, Vec<&str>) = match i {
            0 | 1 => (relay.outside(), everything_wrong.to_vec()),
            2 => (relay.outside(), everything_wrong.to_vec()),
            3 => (relay.notes(), everything_wrong.to_vec()),
            4 => (relay.project(), vec!["approve-me", "group=-x"]),
            _ => (relay.project(), vec!["approve-me", "group=lab-web"]),
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

/// What one refusal code needs: the floor it starts from, what is changed
/// after approval, where it runs and with what. The `match` is exhaustive,
/// so a new refusal code does not compile without its scenario.
fn scenario(reason: RefusalReason) -> (Relay, PathBuf, Vec<&'static str>) {
    let relay = match reason {
        RefusalReason::NotApproved => {
            let relay = Relay {
                env: TestEnv::new(),
            };
            std::fs::create_dir_all(relay.project()).unwrap();
            relay.write(FLOOR);
            relay
        }
        _ => Relay::new(),
    };
    let home = relay.env.relay_home();
    let project = relay.project();
    let probe = vec!["probe", "host=web01"];
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
        RefusalReason::ClassNotAvailable => (project, vec!["apply", "group=lab-web"]),
    };
    (relay, dir, args)
}

#[test]
fn every_refusal_code_exits_126_with_one_journal_line() {
    for reason in RefusalReason::ALL {
        let (relay, dir, args) = scenario(*reason);
        let out = relay.run_in(&dir, &args);
        assert_refused(&relay, &out, *reason);
        let refused = relay.refused();
        assert_eq!(refused.len(), 1, "{reason:?}");
        assert_eq!(refused[0].action, args[0]);
        assert!(relay.started().is_empty(), "{reason:?}");
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

#[test]
fn after_check_and_human_are_not_available_yet() {
    let relay = Relay::new();
    for action in ["apply", "approve-me"] {
        let out = relay.run(&[action, "group=lab-web"]);
        assert_refused(&relay, &out, RefusalReason::ClassNotAvailable);
    }
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
    assert_eq!((finished.stdout.bytes, finished.stderr.bytes), (LEN, LEN));
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
        assert_eq!(f.stdout.sha256, sha256_hex(&stream(len, stdout_byte)));
        assert_eq!(f.stderr.sha256, sha256_hex(&stream(len, stderr_byte)));
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
    let units: Vec<u16> = out
        .stdout
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
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
fn list_marks_unavailable_classes() {
    let relay = Relay::new();
    let text = list(&relay, &relay.project());
    for (action, class) in [("apply", "after-check"), ("approve-me", "human")] {
        let line = text
            .lines()
            .find(|l| l.contains(&format!("officina run {action} ")))
            .unwrap();
        assert!(
            line.ends_with(&format!("(class {class}, not available in this build)")),
            "{line}"
        );
    }
    let probe = text
        .lines()
        .find(|l| l.contains("officina run probe"))
        .unwrap();
    assert!(!probe.contains("not available"), "{probe}");
}

#[test]
fn list_outside_every_project() {
    let relay = Relay::new();
    let text = list(&relay, &relay.outside());
    assert!(text.contains("lies in no project"), "{text}");
    let unapproved = Relay {
        env: TestEnv::new(),
    };
    std::fs::create_dir_all(unapproved.project()).unwrap();
    unapproved.write(FLOOR);
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
