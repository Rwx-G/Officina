//! The fake claude's configuration (story 3.2 AC 12, D-86): the
//! environment, or a sidecar next to a copy of it, so a test running the
//! collector in its own process configures the fake without setting the
//! process environment.

mod common;

use std::process::{Command, Output};

/// Runs `fake` with only `vars` in its environment.
fn run(fake: &std::path::Path, args: &[&str], vars: &[(&str, &str)]) -> Output {
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
    let logged = std::fs::read_to_string(&log).unwrap();
    assert_eq!(logged.lines().count(), 2, "{logged}");
    assert!(logged.contains("agents --json --all"));
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
