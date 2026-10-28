//! `officina hook` end to end (story 1.3, FR-3).

mod common;

use common::{TestEnv, read_events};

use officina_host::doctor::BUFFERED_EVENTS as EVENTS;

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

/// Runs one `UserPromptSubmit` through `cmd` and returns the line it wrote.
fn prompt_line(env: &TestEnv, cmd: std::process::Command) -> serde_json::Value {
    let out = common::run_with(cmd, payload("UserPromptSubmit").as_bytes());
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty() && out.stderr.is_empty());
    let mut lines = read_events(env);
    assert_eq!(lines.len(), 1);
    lines.remove(0)
}

#[test]
fn floor_no_excerpts_drops_author_text_wherever_the_hook_finds_the_floor() {
    let at_default = TestEnv::new();
    std::fs::create_dir_all(at_default.officina_home()).unwrap();
    std::fs::write(
        at_default.officina_home().join("floor.toml"),
        "schema = 1\nno_excerpts = true\n",
    )
    .unwrap();
    let line = prompt_line(&at_default, at_default.officina(&["hook"]));
    assert!(line.get("prompt").is_none() && line.get("last_message").is_none());
    assert_eq!(line["message"], "Claude needs your permission to use Bash");

    let named = TestEnv::new();
    let floor = named.root.path().join("work.toml");
    std::fs::write(&floor, "schema = 1\nno_excerpts = true\n").unwrap();
    let mut cmd = named.officina(&["hook"]);
    cmd.env("OFFICINA_FLOOR", &floor);
    assert!(prompt_line(&named, cmd).get("prompt").is_none());

    // OFFICINA_FLOOR wins over the default location, as for the window.
    let overridden = TestEnv::new();
    std::fs::create_dir_all(overridden.officina_home()).unwrap();
    std::fs::write(
        overridden.officina_home().join("floor.toml"),
        "schema = 1\nno_excerpts = true\n",
    )
    .unwrap();
    let floor = overridden.root.path().join("work.toml");
    std::fs::write(&floor, "schema = 1\nno_excerpts = false\n").unwrap();
    let mut cmd = overridden.officina(&["hook"]);
    cmd.env("OFFICINA_FLOOR", &floor);
    assert_eq!(prompt_line(&overridden, cmd)["prompt"], "check the build");
}

#[test]
fn a_floor_doctor_refuses_still_withholds_excerpts() {
    let env = TestEnv::new();
    let floor = env.root.path().join("work.toml");
    std::fs::write(&floor, "schema = 1\nno_excerpts = true\nunknown_key = 1\n").unwrap();
    let mut cmd = env.officina(&["hook"]);
    cmd.env("OFFICINA_FLOOR", &floor);
    assert!(prompt_line(&env, cmd).get("prompt").is_none());
}

#[test]
fn an_unusable_floor_never_fails_the_hook() {
    let contents: [&[u8]; 3] = [b"not toml [[[", b"no_excerpts = \"yes\"\n", b"\xff\xfe\x00"];
    for content in contents {
        let env = TestEnv::new();
        let floor = env.root.path().join("work.toml");
        std::fs::write(&floor, content).unwrap();
        let mut cmd = env.officina(&["hook"]);
        cmd.env("OFFICINA_FLOOR", &floor);
        assert_eq!(prompt_line(&env, cmd)["prompt"], "check the build");
    }
    let missing_file_or_directory: [fn(&TestEnv) -> std::path::PathBuf; 2] = [
        |env| env.root.path().join("missing.toml"),
        |env| env.root.path().to_path_buf(),
    ];
    for floor_of in missing_file_or_directory {
        let env = TestEnv::new();
        let floor = floor_of(&env);
        let mut cmd = env.officina(&["hook"]);
        cmd.env("OFFICINA_FLOOR", &floor);
        assert_eq!(
            prompt_line(&env, cmd)["prompt"],
            "check the build",
            "OFFICINA_FLOOR={}",
            floor.display()
        );
    }
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
    for args in [&["doctor", "--floor"][..], &["board", "--json", "--floor"]] {
        let out = env.run(args, b"");
        assert_eq!(out.status.code(), Some(64), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).starts_with("officina: --floor takes a path"),
            "{args:?}"
        );
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
