//! `officina board --json` end to end (story 1.5, FR-1, FR-14).

mod common;

use std::process::Stdio;
use std::time::{Duration, Instant};

use common::{TestEnv, now_ms};
use officina_host::claude::{AGENTS_TIMEOUT, OUTPUT_GRACE};

fn agents_fixture(now: i64) -> String {
    serde_json::json!([
        {"pid": 4100, "cwd": "C:/work/repo-a", "kind": "interactive", "startedAt": now - 60_000,
         "sessionId": "00000000-0000-0000-0000-000000000001", "name": "repo-a", "status": "waiting",
         "waitingFor": "permission prompt"},
        {"pid": 4200, "cwd": "C:/work/repo-b", "kind": "interactive", "startedAt": now - 50_000,
         "sessionId": "00000000-0000-0000-0000-000000000002", "name": "repo-b", "status": "idle"},
        {"pid": 4300, "id": "bg01", "cwd": "C:/work/repo-c", "kind": "background", "startedAt": now - 40_000,
         "sessionId": "00000000-0000-0000-0000-000000000003", "name": "nightly", "status": "busy", "state": "working"},
        {"id": "bg02", "cwd": "C:/work/repo-c", "kind": "background", "startedAt": now - 30_000,
         "sessionId": "00000000-0000-0000-0000-000000000004", "name": "report", "state": "done"}
    ])
    .to_string()
}

fn board(env: &TestEnv) -> (Option<i32>, serde_json::Value) {
    let out = env.run(&["board", "--json"], b"");
    let json = serde_json::from_slice(&out.stdout).unwrap_or(serde_json::Value::Null);
    (out.status.code(), json)
}

fn keys(board: &serde_json::Value, column: &str) -> Vec<String> {
    board["columns"][column]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["key"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn board_classifies_the_fake_sessions_and_joins_hook_events() {
    let env = TestEnv::new();
    let now = now_ms();
    env.write_agents(&agents_fixture(now));
    let prompt = serde_json::json!({
        "hook_event_name": "UserPromptSubmit",
        "session_id": "00000000-0000-0000-0000-000000000002",
        "prompt": "which branch?",
    });
    env.run(&["hook"], prompt.to_string().as_bytes());

    let (code, board) = board(&env);
    assert_eq!(code, Some(0));
    assert_eq!(board["schema"], 1);
    assert_eq!(board["waiting_count"], 1);
    assert_eq!(
        keys(&board, "needs_you"),
        [format!("pid:4100:{}", now - 60_000)]
    );
    assert_eq!(
        keys(&board, "your_turn"),
        [format!("pid:4200:{}", now - 50_000)]
    );
    assert_eq!(keys(&board, "working"), ["bg:bg01"]);
    assert_eq!(keys(&board, "done"), ["bg:bg02"]);
    let your_turn = &board["columns"]["your_turn"][0];
    assert_eq!(your_turn["last_prompt"], "which branch?");
    assert_eq!(your_turn["project"]["name"], "repo-b");
    assert_eq!(board["columns"]["working"][0]["attach_available"], true);
    assert!(board["stale"].is_null());
}

#[test]
fn failing_claude_marks_the_board_stale_with_exit_3() {
    for mode in ["exit1", "garbage"] {
        let env = TestEnv::new();
        let mut cmd = env.officina(&["board", "--json"]);
        cmd.env("FAKE_CLAUDE_MODE", mode);
        let out = common::run_with(cmd, b"");
        assert_eq!(out.status.code(), Some(3), "{mode}");
        let board: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(
            board["stale"]["error"].as_str().unwrap().len() > 3,
            "{mode}"
        );
    }
}

#[test]
fn hanging_claude_times_out_instead_of_blocking() {
    let env = TestEnv::new();
    let mut cmd = env.officina(&["board", "--json"]);
    cmd.env("FAKE_CLAUDE_MODE", "hang");
    let started = Instant::now();
    let out = common::run_with(cmd, b"");
    let elapsed = started.elapsed();
    assert_eq!(out.status.code(), Some(3));
    assert!(
        elapsed < AGENTS_TIMEOUT + Duration::from_secs(1),
        "{elapsed:?}"
    );
    let board: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(board["stale"]["error"].as_str().unwrap().contains("within"));
}

#[test]
fn a_process_claude_leaves_running_cannot_hold_the_board() {
    let env = TestEnv::new();
    env.write_agents("[]");
    // The process left behind inherits every inheritable handle down the
    // chain, this test's pipes included: reading officina's stdout through a
    // pipe would wait for it. A file does not.
    let board_file = env.root.path().join("board.json");
    let mut cmd = env.officina(&["board", "--json"]);
    cmd.env("FAKE_CLAUDE_MODE", "orphan")
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&board_file).unwrap())
        .stderr(Stdio::null());
    let started = Instant::now();
    let status = cmd.status().unwrap();
    let elapsed = started.elapsed();
    // The fake's orphan holds the output for 5 s, so waiting on it would
    // take that long; the bound is the grace period plus process start-up.
    assert!(
        elapsed < OUTPUT_GRACE + Duration::from_secs(2),
        "{elapsed:?}"
    );
    assert_eq!(status.code(), Some(3));
    let board: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&board_file).unwrap()).unwrap();
    assert_eq!(
        board["stale"]["error"],
        officina_host::claude::ClaudeError::OutputHeld.to_string()
    );
}

#[test]
fn missing_claude_is_reported_not_crashed() {
    let env = TestEnv::new();
    let mut cmd = env.officina(&["board", "--json"]);
    cmd.env("OFFICINA_CLAUDE_BIN", env.root.path().join("nope.exe"));
    let out = common::run_with(cmd, b"");
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn floor_config_dir_reaches_the_claude_child() {
    let env = TestEnv::new();
    env.write_agents("[]");
    let other = env.root.path().join("other-config");
    std::fs::create_dir_all(&other).unwrap();
    let floor = env.root.path().join("officina.toml");
    std::fs::write(
        &floor,
        format!(
            "schema = 1\nname = \"work\"\nconfig_dir = \"{}\"\n[[project]]\nname = \"api\"\npath = \"api\"\ncolor = \"blue\"\n",
            other.to_string_lossy().replace('\\', "/")
        ),
    )
    .unwrap();
    let out = env.run(
        &["board", "--json", "--floor", floor.to_str().unwrap()],
        b"",
    );
    assert_eq!(out.status.code(), Some(0));
    let board: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(board["floor"], "work");
    let log = std::fs::read_to_string(env.fake_log()).unwrap();
    assert!(log.contains("agents --json --all"));
    assert!(log.contains(&format!(
        "CLAUDE_CONFIG_DIR={}",
        other.to_string_lossy().replace('\\', "/")
    )));
}

#[test]
fn invalid_floor_is_reported_on_the_board() {
    let env = TestEnv::new();
    env.write_agents("[]");
    let floor = env.root.path().join("officina.toml");
    std::fs::write(&floor, "schema = 1\nunknown_key = true\n").unwrap();
    let out = env.run(
        &["board", "--json", "--floor", floor.to_str().unwrap()],
        b"",
    );
    let board: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        board["diagnostics"]["floor_error"]
            .as_str()
            .unwrap()
            .contains("unknown_key")
    );
}

#[test]
fn contract_is_unknown_on_an_empty_sample_and_held_on_a_real_one() {
    let env = TestEnv::new();
    env.write_agents("[]");
    let (_, empty) = board(&env);
    assert!(
        empty["contract"]["held"].is_null(),
        "an empty list proves nothing"
    );

    env.write_agents(&agents_fixture(now_ms()));
    let (_, full) = board(&env);
    assert_eq!(full["contract"]["held"], true);

    env.write_agents(
        r#"[{"cwd":"C:/x","kind":"interactive","startedAt":1,"pid":1,"status":"busy"}]"#,
    );
    let (_, broken) = board(&env);
    assert_eq!(broken["contract"]["held"], false);
    assert_eq!(broken["contract"]["missing"][0], "sessionId");
}
