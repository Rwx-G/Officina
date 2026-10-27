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

/// One session inside `<root>/work/Api` and one outside every project.
fn sessions_in_and_out_of_a_project(env: &TestEnv, now: i64) {
    let api = env.root.path().join("work").join("Api");
    std::fs::create_dir_all(&api).unwrap();
    env.write_agents(
        &serde_json::json!([
            {"pid": 4100, "cwd": api.to_string_lossy().replace('\\', "/"), "kind": "interactive",
             "startedAt": now - 60_000, "sessionId": "00000000-0000-0000-0000-000000000001",
             "name": "api", "status": "idle"},
            {"pid": 4200, "cwd": "C:/work/elsewhere", "kind": "interactive", "startedAt": now - 50_000,
             "sessionId": "00000000-0000-0000-0000-000000000002", "name": "elsewhere", "status": "idle"}
        ])
        .to_string(),
    );
}

fn board_with_floor(env: &TestEnv, name: &str, content: &str) -> serde_json::Value {
    let floor = env.root.path().join("work").join(name);
    std::fs::write(&floor, content).unwrap();
    let out = env.run(
        &["board", "--json", "--floor", floor.to_str().unwrap()],
        b"",
    );
    assert_eq!(out.status.code(), Some(0), "{name}");
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn schema_2_floor_maps_projects_like_schema_1() {
    let env = TestEnv::new();
    sessions_in_and_out_of_a_project(&env, now_ms());
    let config = env.root.path().join("other-config");
    std::fs::create_dir_all(&config).unwrap();
    let config = config.to_string_lossy().replace('\\', "/");
    let v1 = format!(
        "schema = 1\nname = \"work\"\nconfig_dir = \"{config}\"\n\n[[project]]\nname = \"api\"\npath = \"Api\"\ncolor = \"blue\"\n"
    );
    let v2 = format!(
        "schema = 2\nname = \"work\"\nconfig_dir = \"{config}\"\nguarded_programs = [\"ssh\"]\n\n[[project]]\nname = \"api\"\npath = \"Api\"\ncolor = \"blue\"\nautonomy = \"guarded\"\nactions = [\"probe\"]\n\n[[action]]\nname = \"probe\"\nclass = \"free\"\nprogram = \"C:/path/to/tool.exe\"\nargs = [\"{{host}}\"]\n\n[action.params.host]\nvalues = [\"web01\"]\n"
    );

    let mut boards = Vec::new();
    for (name, content) in [("v1.toml", &v1), ("v2.toml", &v2)] {
        std::fs::write(env.fake_log(), "").unwrap();
        let board = board_with_floor(&env, name, content);
        let log = std::fs::read_to_string(env.fake_log()).unwrap();
        assert!(
            log.contains(&format!("CLAUDE_CONFIG_DIR={config}")),
            "{name}: {log}"
        );
        assert!(board["diagnostics"]["floor_error"].is_null(), "{name}");
        boards.push(board);
    }
    assert_eq!(boards[0]["floor"], "work");
    assert_eq!(boards[0]["floor"], boards[1]["floor"]);
    assert_eq!(boards[0]["columns"], boards[1]["columns"]);
    let api = &boards[1]["columns"]["your_turn"][0];
    assert_eq!(api["project"]["name"], "api");
    assert_eq!(api["project"]["color"], "blue");
}

#[test]
fn invalid_action_leaves_the_board_floor_valid() {
    let env = TestEnv::new();
    sessions_in_and_out_of_a_project(&env, now_ms());
    let floor = "schema = 2\n\n[[project]]\nname = \"api\"\npath = \"Api\"\ncolor = \"green\"\nactions = [\"broken\", \"also-broken\"]\n\n[[action]]\nname = \"broken\"\nclass = \"free\"\nprogram = \"relative/tool.exe\"\n\n[[action]]\nname = \"also-broken\"\nclass = \"sometimes\"\nunknown_key = 1\n";
    let board = board_with_floor(&env, "officina.toml", floor);
    assert!(
        board["diagnostics"]["floor_error"].is_null(),
        "{}",
        board["diagnostics"]
    );
    let api = &board["columns"]["your_turn"][0];
    assert_eq!(api["project"]["name"], "api");
    assert_eq!(api["project"]["color"], "green");
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
