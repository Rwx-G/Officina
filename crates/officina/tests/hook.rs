//! `officina hook` end to end (story 1.3, FR-3).

mod common;

use common::{TestEnv, read_events};

use officina_host::doctor::HOOK_EVENTS as EVENTS;

fn payload(event: &str) -> String {
    serde_json::json!({
        "hook_event_name": event,
        "session_id": "00000000-0000-0000-0000-000000000001",
        "cwd": "C:/work/repo-a",
        "transcript_path": "/path/to/transcript.jsonl",
        "prompt": "check the build",
        "last_assistant_message": "Done.",
        "source": "startup",
        "reason": "prompt_input_exit",
        "notification_type": "permission_prompt",
        "message": "Claude needs your permission to use Bash",
    })
    .to_string()
}

#[test]
fn appends_one_line_per_event_silently() {
    let env = TestEnv::new();
    for event in EVENTS {
        let out = env.run(&["hook"], payload(event).as_bytes());
        assert!(out.status.success(), "{event}");
        assert!(
            out.stdout.is_empty() && out.stderr.is_empty(),
            "{event} printed something"
        );
    }
    let lines = read_events(&env);
    assert_eq!(
        lines
            .iter()
            .map(|l| l["event"].as_str().unwrap())
            .collect::<Vec<_>>(),
        EVENTS
    );
    assert!(
        lines
            .iter()
            .all(|l| l["v"] == 1 && l.get("transcript_path").is_none())
    );
    assert_eq!(lines[2]["prompt"], "check the build");
    assert_eq!(lines[3]["last_message"], "Done.");
}

#[test]
fn bad_input_and_unwritable_home_exit_0_silently() {
    let env = TestEnv::new();
    for input in [&b""[..], b"not json", b"[1,2]", b"\"text\"", b"{}"] {
        let out = env.run(&["hook"], input);
        assert!(out.status.success());
        assert!(out.stdout.is_empty() && out.stderr.is_empty());
    }
    assert!(read_events(&env).is_empty());

    std::fs::write(env.officina_home(), b"a file, not a directory").unwrap();
    let out = env.run(&["hook"], payload("Stop").as_bytes());
    assert!(out.status.success() && out.stderr.is_empty());
}

#[test]
fn no_excerpts_env_drops_author_text() {
    let env = TestEnv::new();
    let mut cmd = env.officina(&["hook"]);
    cmd.env("OFFICINA_NO_EXCERPTS", "1");
    common::run_with(cmd, payload("UserPromptSubmit").as_bytes());
    let lines = read_events(&env);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].get("prompt").is_none() && lines[0].get("last_message").is_none());
    assert_eq!(
        lines[0]["message"],
        "Claude needs your permission to use Bash"
    );
}

#[test]
fn hook_spawns_no_claude_and_never_deletes() {
    let env = TestEnv::new();
    let old = env.events_dir().join("2020-01-01.jsonl");
    std::fs::create_dir_all(env.events_dir()).unwrap();
    std::fs::write(&old, "{}\n").unwrap();
    env.run(&["hook"], payload("Stop").as_bytes());
    assert!(!env.fake_log().exists(), "the hook must not spawn claude");
    assert!(old.exists(), "the hook must never delete");
}

#[test]
fn unknown_subcommand_exits_64_never_2() {
    let env = TestEnv::new();
    for args in [
        &["frobnicate"][..],
        &[],
        &["board"],
        &["quota"],
        &["doctor", "--floor"],
    ] {
        let out = env.run(args, b"");
        assert_eq!(out.status.code(), Some(64), "{args:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("usage: officina"));
    }
    // Extra arguments after `hook` are ignored: whatever Claude Code passes,
    // the hook exits 0 and never 2 (which would block the session).
    let out = env.run(
        &["hook", "--unexpected", "x"],
        br#"{"hook_event_name":"Stop"}"#,
    );
    assert_eq!(out.status.code(), Some(0));
    let out = env.run(&["version"], b"");
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("officina 0."));
}
