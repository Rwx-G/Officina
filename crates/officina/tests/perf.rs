//! Hook and gate latency, the `PermissionRequest` path without a window
//! (story 5.22), concurrent appends to the event buffer and the
//! journal (stories 1.3 and 2.4, AD-10, AD-17, AD-28, AD-32, D-19). Ignored
//! by default: run with `cargo test --release -p officina -- --ignored`.
//!
//! Every test takes [`SERIAL`] first: a stress test starting hundreds of
//! processes next to a timing test would measure the stress, not Officina.

mod common;

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use common::{TestEnv, read_events};
use officina_core::relay::journal::{JournalEntry, JournalKind, spilled_day_file};
use officina_host::relay::gate::{GateEnv, answer};
use officina_host::relay::journal::{JournalLock, LOCK_TIMEOUT};

fn big_payload() -> Vec<u8> {
    serde_json::json!({
        "hook_event_name": "Stop",
        "session_id": "00000000-0000-0000-0000-000000000001",
        "cwd": "C:/work/repo-a",
        "last_assistant_message": "x".repeat(200_000),
    })
    .to_string()
    .into_bytes()
}

fn p95(mut samples: Vec<Duration>) -> Duration {
    samples.sort();
    samples[samples.len() * 95 / 100 - 1]
}

static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Times `subject` and `baseline` alternately, so a change of machine load
/// during the test weighs on both samples alike.
fn timed_pairs(
    runs: usize,
    mut subject: impl FnMut(),
    mut baseline: impl FnMut(),
) -> (Vec<Duration>, Vec<Duration>) {
    (0..runs)
        .map(|_| {
            let started = Instant::now();
            subject();
            let subject_took = started.elapsed();
            let started = Instant::now();
            baseline();
            (subject_took, started.elapsed())
        })
        .unzip()
}

fn timed(runs: usize, mut f: impl FnMut()) -> Vec<Duration> {
    (0..runs)
        .map(|_| {
            let started = Instant::now();
            f();
            started.elapsed()
        })
        .collect()
}

/// PRD section 7: p95 < 30 ms as Claude Code pays it, process start included.
/// That absolute figure depends on how fast the machine creates processes, so
/// the test also times a baseline process that does nothing (the fake claude
/// answering `--version`) and gates Officina's own cost, hook minus baseline,
/// at 30 ms everywhere. The absolute budget defaults to 30 ms (the
/// workstation figure) and CI raises it with `OFFICINA_HOOK_P95_BUDGET_MS`,
/// because hosted runners start any process several times slower.
#[test]
#[ignore = "timing test, run in release by CI"]
fn hook_latency_p95() {
    let _serial = serial();
    let env = TestEnv::new();
    let payload = big_payload();
    for _ in 0..5 {
        env.run(&["hook"], &payload);
    }
    let hook = p95(timed(200, || {
        assert!(env.run(&["hook"], &payload).status.success());
    }));
    let baseline = p95(timed(200, || {
        let out = std::process::Command::new(common::FAKE_CLAUDE)
            .arg("--version")
            .output()
            .unwrap();
        assert!(out.status.success());
    }));
    let own_cost = hook.saturating_sub(baseline);
    let budget = Duration::from_millis(
        std::env::var("OFFICINA_HOOK_P95_BUDGET_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30),
    );
    println!(
        "hook p95 {hook:?}, empty process p95 {baseline:?}, own cost {own_cost:?}, budget {budget:?}"
    );
    assert!(
        own_cost < Duration::from_millis(30),
        "own cost {own_cost:?}"
    );
    assert!(hook < budget, "p95 {hook:?} over {budget:?}");
}

/// Story 5.22 AC 3 (D-511, D-579): with no window, a `PermissionRequest`
/// costs the hook what any event does, the marker's one open included,
/// so Claude Code's own prompt appears without delay. The same budgets as
/// [`hook_latency_p95`], measured the same way.
#[test]
#[ignore = "timing test, run in release by CI"]
fn permission_request_without_a_window_is_within_the_hook_budget() {
    let _serial = serial();
    let env = TestEnv::new();
    let payload = serde_json::json!({
        "hook_event_name": "PermissionRequest",
        "session_id": "00000000-0000-0000-0000-000000000001",
        "permission_mode": "default",
        "tool_name": "Bash",
        "tool_input": {"command": "x".repeat(64 * 1024)},
    })
    .to_string()
    .into_bytes();
    for _ in 0..5 {
        env.run(&["hook"], &payload);
    }
    let hook = p95(timed(200, || {
        let out = env.run(&["hook"], &payload);
        assert!(out.status.success() && out.stdout.is_empty());
    }));
    let baseline = p95(timed(200, || {
        let out = std::process::Command::new(common::FAKE_CLAUDE)
            .arg("--version")
            .output()
            .unwrap();
        assert!(out.status.success());
    }));
    let own_cost = hook.saturating_sub(baseline);
    let budget = Duration::from_millis(
        std::env::var("OFFICINA_HOOK_P95_BUDGET_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30),
    );
    println!(
        "permission request p95 {hook:?}, empty process p95 {baseline:?}, own cost {own_cost:?}, budget {budget:?}"
    );
    assert!(
        own_cost < Duration::from_millis(30),
        "own cost {own_cost:?}"
    );
    assert!(hook < budget, "p95 {hook:?} over {budget:?}");
    assert!(
        !env.relay_home().prompts_dir().exists(),
        "no window, no request"
    );
}

const GATE_ACTIONS: usize = 50;
const GATE_COMMAND_BYTES: usize = 64 * 1024;
const GATE_RUNS: usize = 200;

/// The guarded project of [`gate_env`]: a real folder, so the gate's
/// resolution of `cwd` by handle (D-173) pays its real cost.
fn lab_dir(env: &TestEnv) -> std::path::PathBuf {
    env.root.path().join("work").join("Lab")
}

/// AD-28's sizing: a guarded project whose approved catalogue holds
/// [`GATE_ACTIONS`] actions, each with a pattern parameter.
fn gate_env() -> TestEnv {
    let env = TestEnv::new();
    std::fs::create_dir_all(lab_dir(&env).join("roles")).unwrap();
    let mut floor = format!(
        "schema = 2\n[[project]]\nname = \"lab\"\npath = \"{}\"\nactions = [",
        lab_dir(&env).to_string_lossy().replace('\\', "/")
    );
    let names: Vec<String> = (0..GATE_ACTIONS)
        .map(|i| format!("\"action-{i}\""))
        .collect();
    floor.push_str(&names.join(", "));
    floor.push_str("]\n");
    for i in 0..GATE_ACTIONS {
        floor.push_str(&format!(
            "\n[[action]]\nname = \"action-{i}\"\ndescription = \"Probe number {i}\"\nclass = \"free\"\nprogram = \"{{tool}}\"\nargs = [\"--group\", \"{{group}}\", \"--fixed\"]\n\n[action.params.group]\npattern = \"lab-[a-z0-9-]{{1,30}}\"\n"
        ));
    }
    env.write_floor(&floor);
    env.approve();
    env
}

/// A [`GATE_COMMAND_BYTES`] command the gate reads in full and lets through:
/// pipes, quotes and substitutions, no guarded program.
fn big_command() -> String {
    let unit = "ls -la \"src/module dir\" && grep -n 'pattern' file.txt | head -5; echo $(date) ";
    let mut command = String::with_capacity(GATE_COMMAND_BYTES + unit.len());
    while command.len() < GATE_COMMAND_BYTES {
        command.push_str(unit);
    }
    command
}

fn gate_payload(env: &TestEnv, command: &str) -> serde_json::Value {
    serde_json::json!({
        "cwd": lab_dir(env).join("roles").to_string_lossy(),
        "effort": "high",
        "hook_event_name": "PreToolUse",
        "permission_mode": "bypassPermissions",
        "prompt_id": "00000000-0000-0000-0000-00000000000a",
        "session_id": "00000000-0000-0000-0000-000000000001",
        "tool_input": {"command": command, "description": "x"},
        "tool_name": "Bash",
        "tool_use_id": "toolu_00000000000000000000000001",
        "transcript_path": "C:\\path\\to\\transcript.jsonl",
    })
}

/// D-19: the gate's own cost through the real binary, its p95 over
/// [`GATE_RUNS`] runs minus a baseline's, under 5 ms on the workstation.
/// The bound defaults to those 5 ms and CI loosens it with
/// `OFFICINA_GATE_OWN_COST_BUDGET_MS`, as it does the hook latency: a hosted
/// runner runs the gate's reads and lexing about twice slower than the
/// workstation, so its own cost lands near the 5 ms, and the looser bound
/// there still catches an order-of-magnitude regression.
///
/// The baseline is the same binary, `officina hook`, on the same payload
/// with another tool than `Bash`: the gate answers it at its first step,
/// before reading any file (AD-26). Both runs then pay the same process
/// start, image load, stdin read and payload parse, and the difference is
/// the gate's own work: the floor, snapshot and known-projects reads, phase
/// 1, the lexer and the decision. A smaller program as baseline would charge
/// the gate with loading and scanning `officina.exe` itself, which a hosted
/// runner's antivirus makes tens of milliseconds and the gate cannot change.
#[test]
#[ignore = "timing test, run in release by CI"]
fn gate_overhead_over_empty_process() {
    let _serial = serial();
    let env = gate_env();
    let command = big_command();
    let payload = gate_payload(&env, &command).to_string().into_bytes();
    let mut other_tool = gate_payload(&env, &command);
    other_tool["tool_name"] = "Read".into();
    let other_tool = other_tool.to_string().into_bytes();
    assert_eq!(payload.len(), other_tool.len(), "the same bytes to read");
    for input in [&payload, &other_tool] {
        let first = env.run(&["hook"], input);
        assert!(
            first.status.success() && first.stdout.is_empty(),
            "the command is let through"
        );
    }
    let (gate, baseline) = timed_pairs(
        GATE_RUNS,
        || assert!(env.run(&["hook"], &payload).status.success()),
        || assert!(env.run(&["hook"], &other_tool).status.success()),
    );
    assert!(
        env.journal_after_approvals().is_empty(),
        "neither run journals"
    );
    let (gate, baseline) = (p95(gate), p95(baseline));
    let own_cost = gate.saturating_sub(baseline);
    let budget = Duration::from_millis(
        std::env::var("OFFICINA_GATE_OWN_COST_BUDGET_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5),
    );
    println!(
        "gate p95 {gate:?}, same binary without the gate p95 {baseline:?}, own cost {own_cost:?}, budget {budget:?}"
    );
    assert!(own_cost < budget, "own cost {own_cost:?} over {budget:?}");
}

/// AD-28's absolute bound, in process: payload parse, the two reads, phase 1
/// and the lexer under 3 ms p95. Asserted only where it is meant to hold
/// (`OFFICINA_PERF_WORKSTATION=1`), printed elsewhere (D-19).
#[test]
#[ignore = "timing test, run in release by CI"]
fn gate_in_process_p95() {
    let _serial = serial();
    let env = gate_env();
    let home = env.relay_home();
    let raw = gate_payload(&env, &big_command()).to_string();
    let gate_env = GateEnv::default();
    let in_process = p95(timed(GATE_RUNS, || {
        let payload: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(answer(&home, &payload, &gate_env, 0), None);
    }));
    let enforced = std::env::var("OFFICINA_PERF_WORKSTATION").is_ok_and(|v| v == "1");
    println!("gate in-process p95 {in_process:?} (asserted: {enforced})");
    if enforced {
        assert!(in_process < Duration::from_millis(3), "p95 {in_process:?}");
    }
}

/// AD-37: 16 processes append 200 journal lines each, every line over
/// 4096 bytes (past any size a single write is known to keep whole), and
/// not one is corrupt: the lock, not the write size, keeps them apart.
/// Meanwhile the test holds the lock past [`LOCK_TIMEOUT`], as a loaded
/// machine does, and not one line is lost: the writers that waited spill
/// (D-760).
#[test]
#[ignore = "stress test, run in release by CI"]
fn journal_appends_never_interleave() {
    const PROCESSES: usize = 16;
    const LINES: usize = 200;
    let _serial = serial();
    let env = TestEnv::new();
    std::fs::create_dir_all(lab_dir(&env).join("roles")).unwrap();
    env.write_floor(&format!(
        "schema = 2\n[[project]]\nname = \"lab\"\npath = \"{}\"\n",
        lab_dir(&env).to_string_lossy().replace('\\', "/")
    ));
    env.approve();
    let mut payload = gate_payload(&env, "ssh host01");
    payload["session_id"] = "s".repeat(4200).into();
    let payload = payload.to_string().into_bytes();
    std::thread::scope(|scope| {
        for _ in 0..PROCESSES {
            scope.spawn(|| {
                for _ in 0..LINES {
                    let out = env.run(&["hook"], &payload);
                    assert!(out.status.success() && !out.stdout.is_empty());
                }
            });
        }
        std::thread::sleep(Duration::from_millis(200));
        let held = JournalLock::acquire_within(&env.relay_home(), Duration::from_secs(60)).unwrap();
        std::thread::sleep(LOCK_TIMEOUT + Duration::from_millis(500));
        drop(held);
    });
    let denied: Vec<JournalEntry> = env.journal_after_approvals();
    assert_eq!(denied.len(), PROCESSES * LINES);
    let spills = std::fs::read_dir(env.relay_home().journal_dir())
        .unwrap()
        .flatten()
        .filter(|e| spilled_day_file(&e.file_name().to_string_lossy()).is_some())
        .count();
    println!("{spills} writes spilled");
    assert!(spills > 0, "the held lock made writers spill");
    assert!(denied.iter().all(|e| e.kind() == JournalKind::GateDenied));
    let shortest_line = std::fs::read_dir(env.relay_home().journal_dir())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".jsonl"))
        .flat_map(|e| {
            std::fs::read_to_string(e.path())
                .unwrap()
                .lines()
                .filter(|l| l.contains("gate_denied"))
                .map(str::len)
                .collect::<Vec<_>>()
        })
        .min()
        .unwrap();
    assert!(shortest_line > 4096, "{shortest_line}");
}

#[test]
#[ignore = "stress test, run in release by CI"]
fn concurrent_appends_never_interleave() {
    let _serial = serial();
    let env = TestEnv::new();
    let payload = r#"{"hook_event_name":"UserPromptSubmit","session_id":"s","prompt":"a prompt long enough to make the write non-trivial, with some accents: é è à ç"}"#.as_bytes();
    std::thread::scope(|scope| {
        for _ in 0..16 {
            scope.spawn(|| {
                for _ in 0..500 {
                    assert!(env.run(&["hook"], payload).status.success());
                }
            });
        }
    });
    let lines = read_events(&env);
    assert_eq!(lines.len(), 8000);
}
