//! `officina quota write`, `officina doctor`, and the zero-write guarantee on
//! the Claude config dir (stories 1.8, 1.9, 1.11; FR-5, FR-11, FR-13).

mod common;

use common::{TestEnv, fingerprint, now_ms};

const STATUSLINE: &str = r#"{"session_id":"s1","model":{"display_name":"Opus"},
  "rate_limits":{"five_hour":{"used_percentage":62.0,"resets_at":1800000000},
                 "seven_day":{"used_percentage":41.0,"resets_at":1800500000}}}"#;

#[test]
fn quota_write_is_silent_and_tee_copies_stdin_unchanged() {
    let env = TestEnv::new();
    let out = env.run(&["quota", "write"], STATUSLINE.as_bytes());
    assert!(out.status.success() && out.stdout.is_empty() && out.stderr.is_empty());
    let file: serde_json::Value = serde_json::from_slice(
        &std::fs::read(env.officina_home().join("quota").join("default.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(file["five_hour"]["used_percentage"], 62.0);
    assert_eq!(file["seven_day"]["resets_at"], 1_800_500_000);

    let out = env.run(&["quota", "write", "--tee"], STATUSLINE.as_bytes());
    assert_eq!(out.stdout, STATUSLINE.as_bytes());
    let out = env.run(&["quota", "write", "--tee"], b"not json at all");
    assert_eq!(out.stdout, b"not json at all");
    assert!(out.status.success());
}

#[test]
fn quota_reaches_the_board() {
    let env = TestEnv::new();
    env.write_agents("[]");
    env.run(&["quota", "write"], STATUSLINE.as_bytes());
    let out = env.run(&["board", "--json"], b"");
    let board: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(board["quota"]["five_hour"]["used_percentage"], 62.0);
    let written = board["quota"]["written_at_ms"].as_i64().unwrap();
    assert!((now_ms() - written).abs() < 120_000);
    assert_eq!(board["quota"]["stale_after_ms"], 1_800_000);
}

#[test]
fn doctor_fails_without_hooks_and_prints_the_block_to_paste() {
    let env = TestEnv::new();
    env.write_agents("[]");
    let out = env.run(&["doctor"], b"");
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("[FAIL] hooks"));
    assert!(text.contains("\"UserPromptSubmit\""));
    assert!(text.contains("hook\""));
    assert!(text.contains("[warn] quota"));
    assert!(text.contains("quota write"));
}

#[test]
fn doctor_passes_once_the_block_is_merged() {
    let env = TestEnv::new();
    env.write_agents(
        r#"[{"pid":1,"cwd":"C:/x","kind":"interactive","startedAt":1,"sessionId":"s","status":"busy"}]"#,
    );
    let first = env.run(&["doctor"], b"");
    let text = String::from_utf8_lossy(&first.stdout).to_string();
    let block_start = text.find("{\n  \"hooks\"").unwrap();
    let block: serde_json::Value = serde_json::Deserializer::from_str(&text[block_start..])
        .into_iter()
        .next()
        .unwrap()
        .unwrap();
    std::fs::write(env.claude_config().join("settings.json"), block.to_string()).unwrap();
    env.run(
        &["hook"],
        br#"{"hook_event_name":"SessionStart","session_id":"s"}"#,
    );
    env.run(&["quota", "write"], STATUSLINE.as_bytes());

    let out = env.run(&["doctor"], b"");
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(text.contains("[ok  ] hooks"));
    assert!(text.contains("[ok  ] hook activity"));
    assert!(text.contains("[ok  ] quota"));
    assert!(text.contains("9.9.9"));
}

#[test]
fn no_command_writes_under_the_claude_config_dir() {
    let env = TestEnv::new();
    let config = env.claude_config();
    std::fs::write(config.join("settings.json"), r#"{"permissions":{}}"#).unwrap();
    std::fs::write(config.join("sessions").join("1234.json"), "{}").unwrap();
    env.write_agents(
        r#"[{"pid":1,"cwd":"C:/x","kind":"interactive","startedAt":1,"sessionId":"s","status":"busy"}]"#,
    );
    let before = fingerprint(&config);

    env.run(&["hook"], br#"{"hook_event_name":"Stop","session_id":"s"}"#);
    env.run(&["quota", "write", "--tee"], STATUSLINE.as_bytes());
    env.run(&["board", "--json"], b"");
    env.run(&["doctor"], b"");

    assert_eq!(fingerprint(&config), before);
}
