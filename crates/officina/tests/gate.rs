//! The gate through `officina hook` (story 2.4, AD-26, D-14 to D-17): the
//! verified `PreToolUse` payloads of E15, redacted, go in on stdin; the
//! deny JSON or nothing comes out; the journal records each deny and fault.

mod common;

use std::process::Output;

use common::relay::Relay;
use common::{TestEnv, read_events};
use officina_core::relay::catalogue::{DEFAULT_GUARDED_PROGRAMS, IssueKind};
use officina_core::relay::gate::{DenyKind, LIST_COMMAND};
use officina_core::relay::journal::{
    COMMAND_EXCERPT_CHARS, GateDenied, GateError, GateFault, JournalEntry,
};
use officina_host::relay::journal::JournalLock;

const GUARDED_ROOT: &str = "C:/path/to/Supervision";
const FREE_ROOT: &str = "C:/path/to/Notes";

/// Two projects, one guarded and one free, each with actions; every action
/// points at the fake tool, whose launch log proves the gate starts nothing.
const FLOOR: &str = r#"schema = 2

[[project]]
name = "supervision"
path = "C:/path/to/Supervision"
actions = ["check", "apply"]

[[project]]
name = "notes"
path = "C:/path/to/Notes"
autonomy = "free"
actions = ["status"]

[[action]]
name = "check"
description = "Dry run of a playbook on one lab group"
class = "free"
program = "{tool}"
args = ["--check", "--limit", "{group}", "{playbook}"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"

[action.params.playbook]
values = ["alerting.yml"]

[[action]]
name = "apply"
class = "human"
program = "{tool}"
args = ["{group}"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"

[[action]]
name = "status"
class = "free"
program = "{tool}"
"#;

fn fixture(name: &str) -> serde_json::Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/pretooluse")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The guarded-program fixture with another command and `cwd`.
fn bash(command: &str, cwd: &str) -> serde_json::Value {
    let mut payload = fixture("bash-guarded-program.json");
    payload["tool_input"]["command"] = command.into();
    payload["cwd"] = cwd.into();
    payload
}

fn hook(env: &TestEnv, payload: &serde_json::Value, project_dir: Option<&str>) -> Output {
    let mut cmd = env.officina(&["hook"]);
    if let Some(dir) = project_dir {
        cmd.env("CLAUDE_PROJECT_DIR", dir);
    }
    cmd.env("FAKE_TOOL_LOG", env.tool_log());
    let out = common::run_with(cmd, payload.to_string().as_bytes());
    assert_eq!(out.status.code(), Some(0), "the hook always exits 0");
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

/// The deny reason when the hook denied, `None` when it printed nothing.
fn decision(out: &Output) -> Option<String> {
    if out.stdout.is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let output = &value["hookSpecificOutput"];
    assert_eq!(output["hookEventName"], "PreToolUse");
    assert_eq!(output["permissionDecision"], "deny");
    Some(
        output["permissionDecisionReason"]
            .as_str()
            .unwrap()
            .to_owned(),
    )
}

fn denied(env: &TestEnv, payload: &serde_json::Value, project_dir: Option<&str>) -> String {
    decision(&hook(env, payload, project_dir))
        .unwrap_or_else(|| panic!("expected a deny for {payload}"))
}

fn not_decided(env: &TestEnv, payload: &serde_json::Value, project_dir: Option<&str>) {
    let out = hook(env, payload, project_dir);
    assert!(
        out.stdout.is_empty(),
        "expected no decision for {payload}, got {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

fn guarded_env() -> TestEnv {
    Relay::unapproved(FLOOR).env
}

fn approved_env() -> TestEnv {
    Relay::approved(FLOOR).env
}

fn gate_denied(env: &TestEnv) -> Vec<GateDenied> {
    env.journal_after_approvals()
        .into_iter()
        .filter_map(|entry| match entry {
            JournalEntry::GateDenied(denied) => Some(denied),
            _ => None,
        })
        .collect()
}

fn gate_errors(env: &TestEnv) -> Vec<GateError> {
    env.journal_after_approvals()
        .into_iter()
        .filter_map(|entry| match entry {
            JournalEntry::GateError(error) => Some(error),
            _ => None,
        })
        .collect()
}

#[test]
fn fixtures_carry_the_verified_field_set() {
    // E15: the exact top-level fields of a real `PreToolUse` payload.
    let verified = [
        "cwd",
        "effort",
        "hook_event_name",
        "permission_mode",
        "prompt_id",
        "session_id",
        "tool_input",
        "tool_name",
        "tool_use_id",
        "transcript_path",
    ];
    for name in [
        "bash-guarded-program.json",
        "bash-officina-run.json",
        "powershell-guarded-program.json",
        "read-file.json",
    ] {
        let payload = fixture(name);
        let mut keys: Vec<&str> = payload
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, verified, "{name}");
        assert!(payload["cwd"].as_str().unwrap().contains('\\'), "{name}");
    }
    let bash = fixture("bash-guarded-program.json");
    let mut input: Vec<&str> = bash["tool_input"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    input.sort_unstable();
    assert_eq!(input, ["command", "description"]);
}

#[test]
fn pre_tool_use_is_never_appended() {
    let env = approved_env();
    hook(&env, &fixture("bash-guarded-program.json"), None);
    hook(&env, &fixture("bash-officina-run.json"), None);
    hook(&env, &fixture("read-file.json"), None);
    assert!(read_events(&env).is_empty());
}

#[test]
fn v0_1_events_are_still_appended() {
    let env = approved_env();
    let stop = serde_json::json!({"hook_event_name": "Stop", "session_id": "s", "cwd": r"C:\path\to\Supervision"});
    let out = hook(&env, &stop, Some(GUARDED_ROOT));
    assert!(out.stdout.is_empty());
    let events = read_events(&env);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["event"], "Stop");
    assert!(gate_denied(&env).is_empty());
}

#[test]
fn guarded_raw_command_is_denied_with_the_verified_shape() {
    let env = approved_env();
    let out = hook(
        &env,
        &fixture("bash-guarded-program.json"),
        Some(GUARDED_ROOT),
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let object = value.as_object().unwrap();
    assert_eq!(object.keys().collect::<Vec<_>>(), ["hookSpecificOutput"]);
    let mut keys: Vec<&str> = value["hookSpecificOutput"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "hookEventName",
            "permissionDecision",
            "permissionDecisionReason"
        ]
    );
    let reason = decision(&out).unwrap();
    assert!(reason.contains("`ansible-playbook`"), "{reason}");
    assert!(reason.contains("supervision"), "{reason}");
    assert!(out.stdout.ends_with(b"\n"));
}

#[test]
fn run_call_gets_no_decision() {
    let env = approved_env();
    not_decided(&env, &fixture("bash-officina-run.json"), Some(GUARDED_ROOT));
}

#[test]
fn free_project_gets_no_decision() {
    let env = approved_env();
    not_decided(
        &env,
        &bash("ssh host01", r"C:\path\to\Notes"),
        Some(FREE_ROOT),
    );
}

#[test]
fn undeclared_folder_gets_no_decision() {
    let env = approved_env();
    not_decided(
        &env,
        &bash("ssh host01", r"C:\path\to\Elsewhere"),
        Some("C:/path/to/Elsewhere"),
    );
    let nothing = TestEnv::new();
    not_decided(
        &nothing,
        &fixture("bash-guarded-program.json"),
        Some(GUARDED_ROOT),
    );
    assert!(nothing.journal().is_empty(), "no relay, no line");
}

#[test]
fn non_bash_tool_gets_no_decision() {
    let env = approved_env();
    not_decided(&env, &fixture("read-file.json"), Some(GUARDED_ROOT));
    let mut no_command = fixture("bash-guarded-program.json");
    no_command["tool_input"] = serde_json::json!({"description": "x"});
    not_decided(&env, &no_command, Some(GUARDED_ROOT));
    assert!(gate_denied(&env).is_empty() && gate_errors(&env).is_empty());
}

/// A file tool's payload, from the verified `Read` fixture: `tool` writing
/// `path`, the shell in `cwd`.
fn file_write(tool: &str, path: &std::path::Path, cwd: &str) -> serde_json::Value {
    let mut payload = fixture("read-file.json");
    let field = officina_core::relay::gate::file_tool_field(tool).unwrap();
    payload["tool_name"] = tool.into();
    payload["tool_input"] = serde_json::json!({ field: path.to_string_lossy(), "content": "{}" });
    payload["cwd"] = cwd.into();
    payload
}

/// AC 6 of story 2.14 (D-173): once the relay guards a project, a file tool
/// writing under the Officina home is denied, from a free project too, and
/// the journal names the file; the approval it would have forged is absent.
#[test]
fn write_into_the_relay_dir_is_denied() {
    let env = approved_env();
    let forged = env
        .relay_home()
        .approvals_dir()
        .join("0123456789abcdef-1.approved.json");
    for (cwd, project_dir) in [
        (r"C:\path\to\Supervision", Some(GUARDED_ROOT)),
        (r"C:\path\to\Notes", Some(FREE_ROOT)),
        (r"C:\path\to\Elsewhere", None),
    ] {
        let reason = denied(&env, &file_write("Write", &forged, cwd), project_dir);
        assert!(
            reason.starts_with("Officina refused this file write"),
            "{reason}"
        );
        assert!(reason.contains(LIST_COMMAND), "{reason}");
    }
    assert!(!forged.exists());
    let journaled = gate_denied(&env);
    assert_eq!(journaled.len(), 3);
    assert!(
        journaled
            .iter()
            .all(|d| d.deny == DenyKind::RelayStateWrite)
    );
    assert_eq!(journaled[0].project, "supervision");
    assert_eq!(journaled[1].project, "", "a free project names none");
    assert!(
        journaled[0]
            .command_excerpt
            .as_deref()
            .is_some_and(|e| e.contains("approved.json")),
        "{journaled:?}"
    );
}

/// AC 6 of story 2.14: every file tool is gated; the journal and the stop
/// flag are relay state like the approvals.
#[test]
fn edit_of_the_journal_is_denied() {
    let env = approved_env();
    let home = env.relay_home();
    let journal = home.journal_dir().join("2026-10-28.jsonl");
    let stopped = home.stopped_file();
    for (tool, _) in officina_core::relay::gate::FILE_TOOLS {
        for target in [&journal, &stopped] {
            denied(
                &env,
                &file_write(tool, target, r"C:\path\to\Supervision"),
                Some(GUARDED_ROOT),
            );
        }
    }
}

/// AC 6 of story 2.14: a write elsewhere, and any write while the relay
/// guards nothing, get no decision.
#[test]
fn write_elsewhere_gets_no_decision() {
    let env = approved_env();
    not_decided(
        &env,
        &file_write(
            "Write",
            std::path::Path::new(r"C:\path\to\Supervision\site.yml"),
            r"C:\path\to\Supervision",
        ),
        Some(GUARDED_ROOT),
    );
    let free = Relay::approved(&FLOOR.replace(
        "actions = [\"check\", \"apply\"]\n",
        "autonomy = \"free\"\nactions = [\"check\", \"apply\"]\n",
    ))
    .env;
    let flag = free.relay_home().stopped_file();
    not_decided(
        &free,
        &file_write("Write", &flag, r"C:\path\to\Supervision"),
        Some(GUARDED_ROOT),
    );
    assert!(gate_denied(&env).is_empty() && gate_denied(&free).is_empty());
}

#[test]
fn every_default_guarded_program_is_denied() {
    let env = approved_env();
    for entry in DEFAULT_GUARDED_PROGRAMS {
        // A prefix entry is denied for any name it starts; the written-form
        // entry (`bash.exe`) for that very spelling.
        let program = entry.trim_end_matches('*');
        let command = format!("{program} --version");
        let reason = denied(
            &env,
            &bash(&command, r"C:\path\to\Supervision"),
            Some(GUARDED_ROOT),
        );
        assert!(reason.contains("guarded program"), "{entry}: {reason}");
    }
    let journaled = gate_denied(&env);
    assert_eq!(journaled.len(), DEFAULT_GUARDED_PROGRAMS.len());
    assert!(journaled.iter().all(|d| d.deny == DenyKind::GuardedProgram));
}

#[test]
fn cd_out_of_a_guarded_project_stays_guarded() {
    let env = approved_env();
    denied(&env, &bash("ssh host01", r"C:\path\to"), Some(GUARDED_ROOT));
    denied(&env, &bash("ssh host01", r"C:\"), Some(GUARDED_ROOT));
}

#[test]
fn cwd_inside_a_guarded_project_is_guarded_from_a_free_root() {
    let env = approved_env();
    denied(
        &env,
        &bash("ssh host01", r"C:\path\to\Supervision\roles"),
        Some(FREE_ROOT),
    );
    denied(
        &env,
        &bash("ssh host01", "/c/path/to/supervision"),
        Some(FREE_ROOT),
    );
}

#[test]
fn no_project_dir_falls_back_to_cwd() {
    let env = approved_env();
    denied(&env, &bash("ssh host01", r"C:\path\to\Supervision"), None);
    not_decided(&env, &bash("ssh host01", r"C:\path\to\Notes"), None);
    let mut no_cwd = bash("ssh host01", "");
    no_cwd.as_object_mut().unwrap().remove("cwd");
    not_decided(&env, &no_cwd, None);
    denied(&env, &no_cwd, Some(GUARDED_ROOT));
}

#[test]
fn emptying_guarded_programs_does_not_lift_containment() {
    let env = approved_env();
    env.write_floor(&FLOOR.replace("schema = 2\n", "schema = 2\nguarded_programs = []\n"));
    denied(
        &env,
        &bash("ssh host01", r"C:\path\to\Supervision"),
        Some(GUARDED_ROOT),
    );
    assert!(
        gate_errors(&env).is_empty(),
        "held by the approved list, not by a fallback"
    );
}

#[test]
fn dropping_a_program_from_the_floor_does_not_lift_it() {
    let env = guarded_env();
    env.write_floor(&FLOOR.replace(
        "schema = 2\n",
        "schema = 2\nguarded_programs = [\"ssh\", \"scp\"]\n",
    ));
    env.approve();
    env.write_floor(&FLOOR.replace("schema = 2\n", "schema = 2\nguarded_programs = [\"scp\"]\n"));
    denied(
        &env,
        &bash("ssh host01", r"C:\path\to\Supervision"),
        Some(GUARDED_ROOT),
    );
    not_decided(
        &env,
        &bash("rsync a b", r"C:\path\to\Supervision"),
        Some(GUARDED_ROOT),
    );
}

#[test]
fn setting_free_in_the_floor_does_not_lift_containment() {
    let env = approved_env();
    env.write_floor(&FLOOR.replace(
        "path = \"C:/path/to/Supervision\"\n",
        "path = \"C:/path/to/Supervision\"\nautonomy = \"free\"\n",
    ));
    denied(
        &env,
        &bash("ssh host01", r"C:\path\to\Supervision"),
        Some(GUARDED_ROOT),
    );
    // The other way round, a session can only tighten: guarding the free
    // project in the floor guards it at once.
    env.write_floor(&FLOOR.replace("autonomy = \"free\"\n", ""));
    denied(
        &env,
        &bash("ssh host01", r"C:\path\to\Notes"),
        Some(FREE_ROOT),
    );
    assert!(
        gate_errors(&env).is_empty(),
        "held by the union of sources, not by a fallback"
    );
}

fn causes(env: &TestEnv) -> Vec<GateFault> {
    gate_errors(env).iter().map(|e| e.cause).collect()
}

#[test]
fn corrupt_approved_json_guards_every_floor_project() {
    let env = approved_env();
    std::fs::write(env.relay_home().approved_file(), b"{ not json").unwrap();
    let reason = denied(
        &env,
        &bash("ssh host01", r"C:\path\to\Notes"),
        Some(FREE_ROOT),
    );
    assert!(reason.contains("awaits approval"), "{reason}");
    assert_eq!(causes(&env), [GateFault::ApprovedUnreadable]);
    assert_eq!(gate_errors(&env)[0].project.as_deref(), Some("notes"));
}

#[test]
fn missing_approved_after_an_approval_guards_every_floor_project() {
    let env = approved_env();
    std::fs::remove_file(env.relay_home().approved_file()).unwrap();
    denied(
        &env,
        &bash("ssh host01", r"C:\path\to\Notes"),
        Some(FREE_ROOT),
    );
    assert_eq!(causes(&env), [GateFault::ApprovedMissing]);
}

#[test]
fn broken_floor_and_snapshot_fall_back_to_known_projects() {
    let env = approved_env();
    std::fs::write(env.relay_home().approved_file(), b"[]").unwrap();
    env.write_floor("schema = 2\n[[project]]\nname = \"broken\"\n");
    let reason = denied(&env, &bash("ssh host01", r"C:\path\to\Notes\sub"), None);
    assert!(reason.contains("officina doctor"), "{reason}");
    denied(
        &env,
        &bash("wsl -d Ubuntu", r"C:\path\to\Supervision"),
        Some(FREE_ROOT),
    );
    not_decided(&env, &bash("ssh host01", r"C:\path\to\Elsewhere"), None);
    let all = causes(&env);
    assert_eq!(
        all,
        [
            GateFault::ApprovedUnreadable,
            GateFault::FloorInvalid,
            GateFault::ApprovedUnreadable,
            GateFault::FloorInvalid,
            GateFault::ApprovedUnreadable,
            GateFault::FloorInvalid,
        ],
        "every call journals its causes, denied or not"
    );
}

#[test]
fn nothing_readable_gives_no_decision() {
    let env = approved_env();
    std::fs::write(env.relay_home().approved_file(), b"[]").unwrap();
    env.write_floor("not toml [[");
    std::fs::write(env.relay_home().known_projects_file(), b"{}").unwrap();
    not_decided(
        &env,
        &bash("ssh host01", r"C:\path\to\Supervision"),
        Some(GUARDED_ROOT),
    );
    assert_eq!(
        causes(&env),
        [
            GateFault::ApprovedUnreadable,
            GateFault::FloorInvalid,
            GateFault::KnownProjectsUnreadable
        ]
    );
    // Story 2.16 AC 1: the line says the record exists and does not read,
    // not that it is missing.
    let detail = &gate_errors(&env)[2].detail;
    assert!(detail.contains("cannot be read ("), "{detail}");
    std::fs::remove_file(env.relay_home().known_projects_file()).unwrap();
    not_decided(
        &env,
        &bash("ssh host01", r"C:\path\to\Supervision"),
        Some(GUARDED_ROOT),
    );
    let detail = &gate_errors(&env)[5].detail;
    assert!(detail.contains("is missing"), "{detail}");
}

/// Every action of the floor these tests approve is valid on every OS: an
/// action dropped as invalid leaves the deny reason nothing to offer, which
/// is how `reason_lists_approved_actions` failed on Linux, where the fake
/// tool's own path is no `native` program.
#[test]
fn the_test_floor_is_valid_on_every_os() {
    let env = guarded_env();
    let floor = officina_host::relay::home::load_relay_floor(&env.floor_file()).unwrap();
    assert_eq!(floor.catalogue.issues, []);
    assert_eq!(
        floor.catalogue.actions.len(),
        floor.floor.actions.len(),
        "no action dropped"
    );
}

/// AD-34: the stop flag stops the relay, not the gate. Every payload gets
/// the same answer, byte for byte, with the relay stopped.
#[test]
fn stop_does_not_change_gate_decisions() {
    let env = approved_env();
    let payloads = [
        fixture("bash-guarded-program.json"),
        fixture("bash-officina-run.json"),
        fixture("read-file.json"),
        bash("ssh host01", r"C:\path\to\Notes"),
        bash("ls -la", r"C:\path\to\Supervision"),
        bash("bash -c 'scp a host01:'", r"C:\path\to\Supervision\roles"),
    ];
    let running: Vec<Vec<u8>> = payloads
        .iter()
        .map(|p| hook(&env, p, Some(GUARDED_ROOT)).stdout)
        .collect();
    assert!(running.iter().any(|out| !out.is_empty()), "some deny");
    assert!(running.iter().any(Vec::is_empty), "some pass");
    officina_host::relay::authority::set_relay_stopped(&env.relay_home(), true).unwrap();
    let stopped: Vec<Vec<u8>> = payloads
        .iter()
        .map(|p| hook(&env, p, Some(GUARDED_ROOT)).stdout)
        .collect();
    assert_eq!(running, stopped);
}

#[test]
fn reason_lists_approved_actions() {
    let env = approved_env();
    let reason = denied(
        &env,
        &fixture("bash-guarded-program.json"),
        Some(GUARDED_ROOT),
    );
    assert!(reason.contains(LIST_COMMAND), "{reason}");
    assert!(
        reason.contains("officina run check group=<...> playbook=<...>"),
        "{reason}"
    );
    assert!(
        reason.lines().any(|l| l.contains("officina run apply")),
        "{reason}"
    );
    assert!(
        !reason.contains("status"),
        "another project's action: {reason}"
    );
}

#[test]
fn reason_without_approval_names_floor_actions() {
    let env = guarded_env();
    let reason = denied(
        &env,
        &fixture("bash-guarded-program.json"),
        Some(GUARDED_ROOT),
    );
    assert!(reason.contains("awaits approval"), "{reason}");
    assert!(reason.contains("check, apply"), "{reason}");
    assert!(reason.contains(LIST_COMMAND), "{reason}");
}

#[test]
fn relay_dir_in_a_command_is_denied() {
    let env = approved_env();
    let home = env.officina_home().to_string_lossy().into_owned();
    for command in [
        format!("cat {home}/relay/approved.json"),
        format!("rm -f {}", env.relay_home().journal_dir().display()),
        format!("echo x > \"{}\"", env.relay_home().stopped_file().display()),
    ] {
        let reason = denied(
            &env,
            &bash(&command, r"C:\path\to\Supervision"),
            Some(GUARDED_ROOT),
        );
        assert!(
            reason.contains("relay or journal directory"),
            "{command}: {reason}"
        );
    }
    assert!(
        gate_denied(&env)
            .iter()
            .all(|d| d.deny == DenyKind::ProtectedDir)
    );
}

#[test]
fn deny_is_journaled_once() {
    let env = approved_env();
    let payload = fixture("bash-guarded-program.json");
    denied(&env, &payload, Some(GUARDED_ROOT));
    let journaled = gate_denied(&env);
    assert_eq!(journaled.len(), 1);
    let line = &journaled[0];
    assert_eq!(line.project, "supervision");
    assert_eq!(line.session_id.as_deref(), payload["session_id"].as_str());
    assert_eq!(line.deny, DenyKind::GuardedProgram);
    assert_eq!(line.matched.as_deref(), Some("ansible-playbook"));
    assert_eq!(
        line.command_excerpt.as_deref(),
        payload["tool_input"]["command"].as_str()
    );
    assert!(gate_errors(&env).is_empty());

    let long = format!("ssh host01 {}", "x".repeat(COMMAND_EXCERPT_CHARS * 2));
    denied(
        &env,
        &bash(&long, r"C:\path\to\Supervision"),
        Some(GUARDED_ROOT),
    );
    let excerpt = gate_denied(&env)[1].command_excerpt.clone().unwrap();
    assert_eq!(
        excerpt.chars().count(),
        COMMAND_EXCERPT_CHARS + 1,
        "cut with an ellipsis"
    );
}

#[test]
fn excerpt_dropped_when_excerpts_are_off() {
    let env = approved_env();
    let payload = fixture("bash-guarded-program.json");
    let mut cmd = env.officina(&["hook"]);
    cmd.env("OFFICINA_NO_EXCERPTS", "1");
    let out = common::run_with(cmd, payload.to_string().as_bytes());
    assert!(decision(&out).is_some());

    env.write_floor(&FLOOR.replace("schema = 2\n", "schema = 2\nno_excerpts = true\n"));
    denied(&env, &payload, Some(GUARDED_ROOT));
    let journaled = gate_denied(&env);
    assert_eq!(journaled.len(), 2);
    assert!(journaled.iter().all(|d| d.command_excerpt.is_none()));
    assert!(journaled.iter().all(|d| d.matched.is_some()));
}

#[test]
fn deny_stands_when_the_journal_lock_is_held() {
    let env = approved_env();
    let held = JournalLock::acquire(&env.relay_home()).unwrap();
    let started = std::time::Instant::now();
    denied(
        &env,
        &fixture("bash-guarded-program.json"),
        Some(GUARDED_ROOT),
    );
    assert!(started.elapsed() >= officina_host::relay::journal::LOCK_TIMEOUT);
    drop(held);
    assert!(
        gate_denied(&env).is_empty(),
        "the line is lost, not the deny"
    );
}

/// Story 2.4 AC 13 (AD-28): the gate reads the floor's phase 1 and compiles
/// no pattern. An action whose pattern does not compile is dropped by phase
/// 2, yet the gate still offers it by the name the floor declares while the
/// catalogue awaits approval: a gate that compiled patterns and acted on
/// them would leave it out. Once approved, it changes no decision.
#[test]
fn invalid_pattern_does_not_affect_the_decision() {
    let broken = "\n[[action]]\nname = \"broken\"\nclass = \"free\"\nprogram = \"{tool}\"\nargs = [\"{p}\"]\n\n[action.params.p]\npattern = \"(unclosed\"\n";
    let floor = format!(
        "{}{broken}",
        FLOOR.replacen(
            "actions = [\"check\", \"apply\"]",
            "actions = [\"check\", \"apply\", \"broken\"]",
            1
        )
    );
    let with_invalid = Relay::unapproved(&floor).env;
    let phase_2 = officina_host::relay::home::load_relay_floor(&with_invalid.floor_file()).unwrap();
    assert!(
        !phase_2.catalogue.actions.contains_key("broken")
            && phase_2
                .catalogue
                .issues
                .iter()
                .any(|i| i.kind == IssueKind::PatternInvalid),
        "positive control: phase 2 drops the action for its pattern"
    );
    let reason = denied(
        &with_invalid,
        &fixture("bash-guarded-program.json"),
        Some(GUARDED_ROOT),
    );
    assert!(reason.contains("check, apply, broken"), "{reason}");

    let plain = approved_env();
    with_invalid.approve();
    for payload in [
        fixture("bash-guarded-program.json"),
        fixture("bash-officina-run.json"),
        bash("ssh host01", r"C:\path\to\Notes"),
    ] {
        assert_eq!(
            hook(&plain, &payload, Some(GUARDED_ROOT)).stdout,
            hook(&with_invalid, &payload, Some(GUARDED_ROOT)).stdout,
            "{payload}"
        );
    }
}

#[test]
fn gate_spawns_nothing() {
    let env = approved_env();
    denied(
        &env,
        &fixture("bash-guarded-program.json"),
        Some(GUARDED_ROOT),
    );
    not_decided(&env, &fixture("bash-officina-run.json"), Some(GUARDED_ROOT));
    let unapproved = guarded_env();
    denied(
        &unapproved,
        &fixture("bash-guarded-program.json"),
        Some(GUARDED_ROOT),
    );
    assert!(!env.tool_log().exists() && !unapproved.tool_log().exists());
    assert!(!env.fake_log().exists(), "nor claude");
}

#[test]
fn hostile_payloads_never_break_the_hook() {
    let env = approved_env();
    // Past the lexer's bound: unreadable, so denied in a guarded project.
    let huge = format!("echo {}", "a".repeat(300 * 1024));
    let reason = denied(
        &env,
        &bash(&huge, r"C:\path\to\Supervision"),
        Some(GUARDED_ROOT),
    );
    assert!(reason.contains("cannot be read"), "{reason}");
    assert!(
        gate_denied(&env)[0]
            .command_excerpt
            .as_ref()
            .unwrap()
            .chars()
            .count()
            <= COMMAND_EXCERPT_CHARS + 1
    );
    // Verbatim and device spellings of a guarded folder are the folder.
    for cwd in [
        r"\\?\C:\path\to\Supervision",
        r"c:\PATH\TO\supervision\",
        "C:/path/to/Supervision/./roles",
    ] {
        denied(&env, &bash("ssh host01", cwd), None);
    }
    // Not an object, wrong types, a NUL in the command: never a crash, never
    // a line in the event buffer.
    for payload in [
        serde_json::json!(["PreToolUse"]),
        serde_json::json!({"hook_event_name": "PreToolUse", "tool_name": 7}),
        serde_json::json!({"hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": "ssh"}),
        serde_json::json!({"hook_event_name": "PreToolUse", "tool_name": "Bash", "tool_input": {"command": "ls"}, "cwd": 5}),
    ] {
        let out = hook(&env, &payload, None);
        assert!(out.stdout.is_empty(), "{payload}");
    }
    denied(
        &env,
        &bash("ss\u{0}h host01", r"C:\path\to\Supervision"),
        None,
    );
    assert!(read_events(&env).is_empty());
}

// ---- story 2.12: every shell tool in the gate (D-171) ----

/// The guarded-program fixture sent through another tool.
fn through(tool: &str, command: Option<&str>, cwd: &str) -> serde_json::Value {
    let mut payload = fixture("bash-guarded-program.json");
    payload["tool_name"] = tool.into();
    payload["cwd"] = cwd.into();
    payload["tool_input"] = match command {
        Some(command) => serde_json::json!({"command": command, "description": "x"}),
        None => serde_json::json!({"url": "wss://example.com/events", "description": "x"}),
    };
    payload
}

#[test]
fn monitor_command_is_decided_like_bash() {
    let env = approved_env();
    let guarded = r"C:\path\to\Supervision";
    let reason = denied(
        &env,
        &through(
            "Monitor",
            Some("ssh host01 tail -f /var/log/syslog"),
            guarded,
        ),
        Some(GUARDED_ROOT),
    );
    assert!(reason.contains("`ssh`"), "{reason}");
    not_decided(
        &env,
        &through("Monitor", Some("tail -f build.log"), guarded),
        Some(GUARDED_ROOT),
    );
    let journaled = gate_denied(&env);
    assert_eq!(journaled.len(), 1);
    assert_eq!(journaled[0].deny, DenyKind::GuardedProgram);
}

#[test]
fn monitor_websocket_gets_no_decision() {
    let env = approved_env();
    not_decided(
        &env,
        &through("Monitor", None, r"C:\path\to\Supervision"),
        Some(GUARDED_ROOT),
    );
    assert!(gate_denied(&env).is_empty() && gate_errors(&env).is_empty());
}

#[test]
fn powershell_in_a_guarded_project_is_denied() {
    let env = approved_env();
    let reason = denied(
        &env,
        &fixture("powershell-guarded-program.json"),
        Some(GUARDED_ROOT),
    );
    assert!(reason.contains("supervision"), "{reason}");
    assert!(reason.contains("Bash commands only"), "{reason}");
    assert!(reason.contains("Bash tool"), "{reason}");
    assert!(reason.contains(LIST_COMMAND), "{reason}");
    // Any PowerShell call, even one with nothing guarded in it, from the
    // shell's folder alone (D-16).
    let harmless = through(
        "PowerShell",
        Some("Get-ChildItem"),
        r"C:\path\to\Supervision\roles",
    );
    denied(&env, &harmless, None);
    let free_root = through(
        "PowerShell",
        Some("Get-ChildItem"),
        r"C:\path\to\Supervision",
    );
    denied(&env, &free_root, Some(FREE_ROOT));
}

#[test]
fn powershell_in_a_free_project_gets_no_decision() {
    let env = approved_env();
    let free = through("PowerShell", Some("ssh host01"), r"C:\path\to\Notes");
    not_decided(&env, &free, Some(FREE_ROOT));
    let elsewhere = through("PowerShell", Some("ssh host01"), r"C:\path\to\Elsewhere");
    not_decided(&env, &elsewhere, Some("C:/path/to/Elsewhere"));
    assert!(gate_denied(&env).is_empty());
}

#[test]
fn powershell_under_a_broken_snapshot_is_denied() {
    let env = approved_env();
    std::fs::write(env.relay_home().approved_file(), b"{ not json").unwrap();
    let in_free = through("PowerShell", Some("Get-ChildItem"), r"C:\path\to\Notes");
    denied(&env, &in_free, Some(FREE_ROOT));
    assert_eq!(causes(&env), [GateFault::ApprovedUnreadable]);
    assert_eq!(
        gate_denied(&env)[0].deny,
        DenyKind::PowershellInGuardedProject
    );
}

#[test]
fn powershell_deny_is_journaled_once() {
    let env = approved_env();
    let payload = fixture("powershell-guarded-program.json");
    denied(&env, &payload, Some(GUARDED_ROOT));
    let journaled = gate_denied(&env);
    assert_eq!(journaled.len(), 1);
    let line = &journaled[0];
    assert_eq!(line.project, "supervision");
    assert_eq!(line.deny, DenyKind::PowershellInGuardedProject);
    assert_eq!(
        serde_json::to_value(line.deny).unwrap(),
        "powershell_in_guarded_project"
    );
    assert_eq!(line.matched, None);
    assert_eq!(
        line.command_excerpt.as_deref(),
        payload["tool_input"]["command"].as_str()
    );

    let long = format!("Get-Content {}", "x".repeat(COMMAND_EXCERPT_CHARS * 2));
    denied(
        &env,
        &through("PowerShell", Some(&long), r"C:\path\to\Supervision"),
        Some(GUARDED_ROOT),
    );
    let excerpt = gate_denied(&env)[1].command_excerpt.clone().unwrap();
    assert!(
        excerpt.chars().count() <= COMMAND_EXCERPT_CHARS + 3,
        "{excerpt}"
    );

    env.write_floor(&FLOOR.replace("schema = 2\n", "schema = 2\nno_excerpts = true\n"));
    env.approve();
    denied(&env, &payload, Some(GUARDED_ROOT));
    assert_eq!(gate_denied(&env)[2].command_excerpt, None);
}
