//! `officina board --json` end to end (story 1.5, FR-1, FR-14; story 3.4
//! AC 9: cards carry their subject).

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
    assert_eq!(board["schema"], officina_core::board::BOARD_SCHEMA);
    assert_eq!(board["approvals"], serde_json::json!([]));
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

/// A timed-out `claude` loses its whole tree, not only itself (E13): the
/// process it started stops with it.
#[cfg(windows)]
#[test]
fn a_timed_out_claude_stops_with_every_process_it_started() {
    let env = TestEnv::new();
    let heartbeat = env.root.path().join("heartbeat");
    // Its output goes to a file: a grandchild inherits every inheritable
    // handle, this test's pipes included.
    let board_file = env.root.path().join("board.json");
    let mut cmd = env.officina(&["board", "--json"]);
    cmd.env("FAKE_CLAUDE_MODE", "tree")
        .env("FAKE_CLAUDE_HEARTBEAT", &heartbeat)
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&board_file).unwrap())
        .stderr(Stdio::null());
    let status = cmd.status().unwrap();
    assert_eq!(status.code(), Some(3), "stale: claude timed out");
    assert!(
        common::heartbeat_len(&heartbeat) > 0,
        "the grandchild had started"
    );
    let stopped = common::heartbeat_stopped(&heartbeat);
    common::stop_heartbeat(&heartbeat);
    assert!(stopped, "the process claude started outlived the timeout");
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

/// FR-2, AD-44, AD-53: no command line deletes an event file, however old.
#[test]
fn board_json_deletes_no_event_file() {
    let env = TestEnv::new();
    env.write_agents("[]");
    std::fs::create_dir_all(env.events_dir()).unwrap();
    let now = now_ms();
    let names: Vec<String> = [2, 9, 40]
        .map(|days| officina_core::dates::event_file_name(now - days * 86_400_000))
        .to_vec();
    for name in &names {
        std::fs::write(
            env.events_dir().join(name),
            "{\"v\":1,\"ts\":1,\"event\":\"Stop\"}\n",
        )
        .unwrap();
    }
    let (code, _) = board(&env);
    assert_eq!(code, Some(0));
    for name in &names {
        assert!(env.events_dir().join(name).exists(), "{name} was deleted");
    }
}

/// AD-53, D-47, D-82: `board --json` reads the registry read-only to
/// report its state, with the window closed too, and never creates it.
#[test]
fn board_json_reports_the_registry_state_with_the_window_closed() {
    use officina_core::registry::types::RegistryState;
    let env = TestEnv::new();
    env.write_agents("[]");
    let registry = officina_host::registry::RegistryPaths::new(&env.officina_home());
    let state = |board: &serde_json::Value| board["diagnostics"]["registry"]["state"].clone();

    let (_, absent) = board(&env);
    assert_eq!(state(&absent), RegistryState::Absent.code());
    assert!(
        !registry.dir().exists(),
        "board --json created the registry"
    );

    // The window wrote it, then closed.
    drop(
        officina_host::registry::store::open_writer(&env.officina_home(), &[], now_ms())
            .writer()
            .unwrap(),
    );
    let before = std::fs::read(registry.database()).unwrap();
    let (code, ready) = board(&env);
    assert_eq!(code, Some(0));
    assert_eq!(state(&ready), RegistryState::Ready.code());
    assert!(ready["diagnostics"]["registry"]["detail"].is_null());
    assert_eq!(std::fs::read(registry.database()).unwrap(), before);

    std::fs::write(registry.database(), "not a database, ".repeat(300)).unwrap();
    let (code, unavailable) = board(&env);
    assert_eq!(code, Some(0), "the board keeps working");
    assert_eq!(state(&unavailable), RegistryState::Unavailable.code());
    assert!(
        unavailable["diagnostics"]["registry"]["detail"]
            .as_str()
            .is_some()
    );
}

/// AD-54: `board --json` fills each card's subject from the read-only
/// registry, by the card's session id, with the window closed.
#[test]
fn cards_carry_their_subject() {
    use officina_core::registry::confirm::SubjectOrigin;
    use officina_host::registry::ingest::IngestContext;
    let env = TestEnv::new();
    let now = now_ms();
    env.write_agents(&agents_fixture(now));
    for session in [
        "00000000-0000-0000-0000-000000000001",
        "00000000-0000-0000-0000-000000000002",
    ] {
        let payload = serde_json::json!({"hook_event_name": "UserPromptSubmit",
            "session_id": session, "cwd": "C:/work/repo-a", "prompt": "x"});
        env.run(&["hook"], payload.to_string().as_bytes());
    }
    let mut writer = officina_host::registry::store::open_writer(&env.officina_home(), &[], now)
        .writer()
        .unwrap();
    let (events, home) = (env.events_dir(), env.officina_home());
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
    let id = writer
        .create_subject(
            "permissions",
            SubjectOrigin::Sessions(vec!["00000000-0000-0000-0000-000000000001".into()]),
            now,
        )
        .unwrap();
    drop(writer);

    let (code, board) = board(&env);
    assert_eq!(code, Some(0));
    let card = |key: &str| {
        board["columns"]
            .as_object()
            .unwrap()
            .values()
            .flat_map(|cards| cards.as_array().unwrap())
            .find(|card| card["key"].as_str().unwrap().starts_with(key))
            .unwrap()
            .clone()
    };
    assert_eq!(
        card("pid:4100")["subject"],
        serde_json::json!({"id": id, "name": "permissions"})
    );
    assert!(card("pid:4200")["subject"].is_null(), "unfiled");
    assert!(
        card("bg:bg01")["subject"].is_null(),
        "unknown to the registry"
    );
    assert_eq!(board["schema"], officina_core::board::BOARD_SCHEMA);
}

/// D-47: with a registry that cannot be read, every card's subject is null
/// and `diagnostics.registry` says why.
#[test]
fn unreadable_registry_leaves_subjects_null_with_the_reason() {
    use officina_core::registry::types::RegistryState;
    let env = TestEnv::new();
    env.write_agents(&agents_fixture(now_ms()));
    let registry = officina_host::registry::RegistryPaths::new(&env.officina_home());
    std::fs::create_dir_all(registry.dir()).unwrap();
    std::fs::write(registry.database(), "not a database, ".repeat(300)).unwrap();
    let (code, board) = board(&env);
    assert_eq!(code, Some(0));
    let cards: Vec<&serde_json::Value> = board["columns"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|cards| cards.as_array().unwrap())
        .collect();
    assert!(!cards.is_empty());
    assert!(cards.iter().all(|card| card["subject"].is_null()));
    assert_eq!(
        board["diagnostics"]["registry"]["state"],
        RegistryState::Unavailable.code()
    );
    assert!(
        board["diagnostics"]["registry"]["detail"]
            .as_str()
            .is_some()
    );
}

/// AD-68, D-99: the floor's projects in file order, each with its colour
/// and priority, the threshold in ms; a project the session list never
/// names is listed too.
#[test]
fn floor_projects_are_printed_in_file_order() {
    let env = TestEnv::new();
    sessions_in_and_out_of_a_project(&env, now_ms());
    let floor = "schema = 1\n\n[alert]\nafter_minutes = 25\n\n[[project]]\nname = \"zeta\"\npath = \"Zeta\"\ncolor = \"plum\"\npriority = \"high\"\n\n[[project]]\nname = \"api\"\npath = \"Api\"\ncolor = \"blue\"\n";
    let board = board_with_floor(&env, "officina.toml", floor);
    assert_eq!(
        board["floor_projects"],
        serde_json::json!([
            {"name": "zeta", "color": "plum", "priority": "high"},
            {"name": "api", "color": "blue", "priority": "normal"}
        ])
    );
    assert_eq!(board["alert_after_ms"], 25 * 60_000);
    assert_eq!(
        board["columns"]["your_turn"][0]["project"],
        board["floor_projects"][1]
    );
}

/// D-99: a floor ignored for an error lists no project, and the threshold
/// is the default.
#[test]
fn an_ignored_floor_prints_no_floor_projects() {
    let env = TestEnv::new();
    sessions_in_and_out_of_a_project(&env, now_ms());
    let floor = env.root.path().join("work").join("broken.toml");
    std::fs::write(
        &floor,
        "schema = 1\nunknown_key = 1\n[[project]]\nname = \"api\"\npath = \"Api\"\n",
    )
    .unwrap();
    let out = env.run(
        &["board", "--json", "--floor", floor.to_str().unwrap()],
        b"",
    );
    let board: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(!board["diagnostics"]["floor_error"].is_null());
    assert_eq!(board["floor_projects"], serde_json::json!([]));
    assert_eq!(
        board["alert_after_ms"],
        i64::from(officina_core::floor::ALERT_AFTER_MINUTES_DEFAULT) * 60_000
    );
}

/// D-182: a card carries the head and the tail of a long answer, as the
/// hook recorded them.
#[test]
fn cards_print_the_answer_tail() {
    let env = TestEnv::new();
    let now = now_ms();
    env.write_agents(&agents_fixture(now));
    let long = format!("{}{}", "a".repeat(400), "the end");
    let stop = serde_json::json!({
        "hook_event_name": "Stop",
        "session_id": "00000000-0000-0000-0000-000000000002",
        "last_assistant_message": long,
    });
    env.run(&["hook"], stop.to_string().as_bytes());
    let (_, board) = board(&env);
    let card = &board["columns"]["your_turn"][0];
    assert!(card["last_message"].as_str().unwrap().starts_with("aaa"));
    assert!(
        card["last_message_tail"]
            .as_str()
            .unwrap()
            .ends_with("the end")
    );
}

/// Story 4.3 AC 9 (AD-167): the board prints each live card's scene state
/// and the alert count; a permission prompt recorded through the real hook
/// gives the wait its start, and a high-priority project puts it in alert.
/// One board, so no previous state.
#[test]
fn board_prints_scene_states_and_the_alert_count() {
    let env = TestEnv::new();
    let now = now_ms();
    sessions_in_and_out_of_a_project(&env, now);
    let api = env.root.path().join("work").join("Api");
    env.write_agents(
        &serde_json::json!([
            {"pid": 4100, "cwd": api.to_string_lossy().replace('\\', "/"), "kind": "interactive",
             "startedAt": now - 60_000, "sessionId": "00000000-0000-0000-0000-000000000001",
             "name": "api", "status": "waiting", "waitingFor": "permission prompt"},
            {"pid": 4200, "cwd": "C:/work/elsewhere", "kind": "interactive", "startedAt": now - 50_000,
             "sessionId": "00000000-0000-0000-0000-000000000002", "name": "elsewhere", "status": "busy"}
        ])
        .to_string(),
    );
    let prompt = serde_json::json!({
        "hook_event_name": "Notification",
        "session_id": "00000000-0000-0000-0000-000000000001",
        "notification_type": officina_core::board::BLOCKING_NOTIFICATIONS[0],
        "message": "Claude needs your permission to use Bash",
    });
    assert!(
        env.run(&["hook"], prompt.to_string().as_bytes())
            .status
            .success()
    );
    let floor = "schema = 1\n\n[[project]]\nname = \"api\"\npath = \"Api\"\npriority = \"high\"\n";
    let board = board_with_floor(&env, "officina.toml", floor);
    let waiting = &board["columns"]["needs_you"][0]["scene"];
    assert_eq!(waiting["zone"], "alert");
    assert_eq!(waiting["alert"], "priority");
    assert!(
        waiting["waiting_since_ms"]
            .as_i64()
            .is_some_and(|t| t >= now - 1_000)
    );
    assert_eq!(waiting["approval"], false);
    assert_eq!(board["columns"]["working"][0]["scene"]["zone"], "office");
    assert_eq!(board["alert_count"], 1);
    assert!(board["next_escalation_at_ms"].is_null());
}
