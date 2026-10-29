//! The fake claude's configuration (story 3.2 AC 12, D-86): the
//! environment, or a sidecar next to a copy of it, so a test running the
//! collector in its own process configures the fake without setting the
//! process environment.

mod common;

use std::path::Path;
use std::process::{Command, Output};

/// Runs `fake` with only `vars` in its environment.
fn run(fake: &Path, args: &[&str], vars: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(fake);
    cmd.args(args).env_clear().env(
        "SYSTEMROOT",
        std::env::var_os("SYSTEMROOT").unwrap_or_default(),
    );
    for (name, value) in vars {
        cmd.env(name, value);
    }
    cmd.output().unwrap()
}

#[test]
fn sidecar_configures_a_copied_fake() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = dir.path().join("agents.json");
    let log = dir.path().join("calls.log");
    let agents = r#"[{"pid":7,"sessionId":"s","kind":"interactive","status":"idle","cwd":"C:/work/api","startedAt":1}]"#;
    std::fs::write(&fixture, agents).unwrap();
    let fake = common::fake_claude_in(
        &dir.path().join("bin"),
        &serde_json::json!({"fixture": fixture, "version": "1.2.3", "log": log, "hang_ms": 10}),
    );
    let version = run(&fake, &["--version"], &[]);
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        "1.2.3 (Claude Code)"
    );
    let listed = run(&fake, &["agents", "--json", "--all"], &[]);
    assert_eq!(String::from_utf8_lossy(&listed.stdout), agents);
    let logged = common::fake_claude_calls(&log);
    assert_eq!(logged.len(), 2, "{logged:?}");
    assert_eq!(
        common::logged_argv(&logged[1]),
        ["agents", "--json", "--all"]
    );
}

#[test]
fn environment_wins_over_the_sidecar() {
    let dir = tempfile::tempdir().unwrap();
    let fake = common::fake_claude_in(
        dir.path(),
        &serde_json::json!({"version": "1.2.3", "mode": "ok"}),
    );
    let version = run(&fake, &["--version"], &[("FAKE_CLAUDE_VERSION", "4.5.6")]);
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        "4.5.6 (Claude Code)"
    );
    let failing = run(
        &fake,
        &["agents", "--json"],
        &[("FAKE_CLAUDE_MODE", "exit1")],
    );
    assert_eq!(failing.status.code(), Some(1));
    let sidecar_only = run(&fake, &["agents", "--json"], &[]);
    assert_eq!(sidecar_only.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&sidecar_only.stdout), "[]");
}

/// D-129, story 5.2 AC 11: every call appends one JSON object, its argv,
/// its working directory and its variables' names sorted, never a value
/// (but the config dir Officina sets).
#[test]
fn the_log_is_one_json_object_per_call() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("calls.log");
    let log_text = log.to_string_lossy().into_owned();
    let secret = "value-never-logged-5f2a";
    let vars = [
        ("FAKE_CLAUDE_LOG", log_text.as_str()),
        ("ZZ_SEEDED", secret),
        ("AA_SEEDED", secret),
    ];
    run(Path::new(common::FAKE_CLAUDE), &["--version"], &vars);
    run(
        Path::new(common::FAKE_CLAUDE),
        &["agents", "--json", "--all"],
        &vars,
    );
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(!text.contains(secret), "a value was logged: {text}");
    let calls = common::fake_claude_calls(&log);
    assert_eq!(calls.len(), 2, "{text}");
    assert_eq!(common::logged_argv(&calls[0]), ["--version"]);
    assert_eq!(
        common::logged_argv(&calls[1]),
        ["agents", "--json", "--all"]
    );
    for call in &calls {
        let object = call.as_object().unwrap();
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["argv", "config_dir", "cwd", "env_names"], "{call}");
        let names: Vec<&str> = call["env_names"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap())
            .collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "sorted");
        assert!(names.contains(&"AA_SEEDED") && names.contains(&"ZZ_SEEDED"));
        assert!(call["cwd"].as_str().is_some_and(|cwd| !cwd.is_empty()));
    }
}

/// E13, story 5.2 AC 11: an argument holding a newline, quotes, a tab and
/// a backslash reaches the child as one argument.
#[test]
fn a_prompt_with_a_newline_stays_one_argument() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("calls.log");
    let prompt = "Bump \"serde\" to 1.0\nthen run the tests\tand commit \\ done";
    run(
        Path::new(common::FAKE_CLAUDE),
        &["--bg", "--name", "bump-1", "--", prompt],
        &[("FAKE_CLAUDE_LOG", log.to_str().unwrap())],
    );
    let calls = common::fake_claude_calls(&log);
    assert_eq!(
        common::logged_argv(&calls[0]),
        ["--bg", "--name", "bump-1", "--", prompt]
    );
}

/// S7, E19, story 5.2 AC 11: with agent view off the fake refuses `--bg`,
/// `agents` and `logs` as `claude` does, and nothing else.
#[test]
fn agent_view_off_refuses_bg_agents_and_logs() {
    let fake = Path::new(common::FAKE_CLAUDE);
    let off = [("CLAUDE_CODE_DISABLE_AGENT_VIEW", "1")];
    for args in [
        &["--bg", "--name", "probe", "--", "x"][..],
        &["agents", "--json", "--all"][..],
        &["logs", "bg01"][..],
    ] {
        let out = run(fake, args, &off);
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr)
                .contains("is disabled by CLAUDE_CODE_DISABLE_AGENT_VIEW."),
            "{args:?}"
        );
        // The control: the same call with agent view on is not refused.
        assert_eq!(run(fake, args, &[]).status.code(), Some(0), "{args:?}");
    }
    for args in [&["--version"][..], &["attach", "bg01"][..]] {
        assert_eq!(run(fake, args, &off).status.code(), Some(0), "{args:?}");
    }
}
