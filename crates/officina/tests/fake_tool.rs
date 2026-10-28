//! `officina-fake-tool`, the stand-in for a relay action's program (AD-37):
//! what it logs and how each mode behaves, so the relay tests built on it
//! can trust it.

mod common;

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use common::fake_output::{stderr_byte, stdout_byte, stream};
use common::{FAKE_TOOL, heartbeat_len, read_tool_log, stop_heartbeat};

fn tool(dir: &std::path::Path) -> Command {
    let mut cmd = Command::new(FAKE_TOOL);
    cmd.env_clear()
        .env(
            "SYSTEMROOT",
            std::env::var_os("SYSTEMROOT").unwrap_or_default(),
        )
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

#[test]
fn logs_argv_cwd_and_env_keys() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("tool.log");
    for round in 0..2 {
        let mut cmd = tool(dir.path());
        cmd.args(["--limit", "two words", "say \"hi\"", "folder\\", ""])
            .env("FAKE_TOOL_LOG", &log)
            .env("PROBE_MODE", "read-only")
            .env("FAKE_TOOL_LOG_VALUES", "PROBE_MODE,ABSENT");
        let out = common::run_with(cmd, b"twelve bytes");
        assert!(out.status.success());
        assert_eq!(read_tool_log(&log).len(), round + 1, "one line per launch");
    }
    let entry = &read_tool_log(&log)[0];
    assert_eq!(
        entry["argv"],
        serde_json::json!(["--limit", "two words", "say \"hi\"", "folder\\", ""])
    );
    assert_eq!(
        std::path::Path::new(entry["cwd"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        dir.path().canonicalize().unwrap()
    );
    let keys: Vec<&str> = entry["env_keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap())
        .collect();
    assert!(keys.contains(&"PROBE_MODE") && keys.contains(&"FAKE_TOOL_LOG"));
    assert_eq!(entry["env"], serde_json::json!({"PROBE_MODE": "read-only"}));
    assert_eq!(entry["stdin_bytes"], 12);

    let quiet = tool(dir.path())
        .env("FAKE_TOOL_LOG", &log)
        .stdin(Stdio::null())
        .output();
    assert!(quiet.unwrap().status.success());
    assert_eq!(
        read_tool_log(&log)[2]["stdin_bytes"],
        0,
        "null stdin reads nothing"
    );
}

#[test]
fn each_mode_behaves() {
    let dir = tempfile::tempdir().unwrap();
    let run = |mode: &str| {
        let mut cmd = tool(dir.path());
        cmd.env("FAKE_TOOL_MODE", mode).stdin(Stdio::null());
        cmd.output().unwrap()
    };

    assert_eq!(run("exit:3").status.code(), Some(3));
    assert_eq!(run("exit:0").status.code(), Some(0));
    assert_eq!(run("").status.code(), Some(0));
    #[cfg(windows)]
    assert_eq!(
        run("exit:3221225477").status.code(),
        Some(0xC000_0005_u32 as i32),
        "a full Windows code"
    );

    let bytes = run("bytes:70000");
    assert!(bytes.status.success());
    assert_eq!(bytes.stdout, stream(70_000, stdout_byte));
    assert_eq!(bytes.stderr, stream(70_000, stderr_byte));
    assert_ne!(bytes.stdout, bytes.stderr);

    let started = Instant::now();
    assert!(run("sleep:0.5").status.success());
    assert!(started.elapsed() >= Duration::from_millis(500));

    let heartbeat = dir.path().join("heartbeat");
    let mut tree = tool(dir.path());
    tree.env("FAKE_TOOL_MODE", "tree")
        .env("FAKE_TOOL_HEARTBEAT", &heartbeat)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = tree.spawn().unwrap();
    std::thread::sleep(Duration::from_millis(600));
    let first = heartbeat_len(&heartbeat);
    std::thread::sleep(Duration::from_millis(400));
    assert!(
        heartbeat_len(&heartbeat) > first && first > 0,
        "the grandchild writes a heartbeat"
    );
    assert!(
        child.try_wait().unwrap().is_none(),
        "the child itself keeps running"
    );
    let _ = child.kill();
    let _ = child.wait();
    // The grandchild is not in any job here: it outlives the child, which is
    // what the relay's job object exists to prevent.
    let after_kill = heartbeat_len(&heartbeat);
    std::thread::sleep(Duration::from_millis(400));
    assert!(heartbeat_len(&heartbeat) > after_kill);
    stop_heartbeat(&heartbeat);
}
