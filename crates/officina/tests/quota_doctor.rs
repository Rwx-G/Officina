//! `officina quota write`, `officina doctor`, and the zero-write guarantee on
//! the Claude config dir (stories 1.8, 1.9, 1.11; FR-5, FR-11, FR-13).

mod common;
#[path = "../../../tests/support/stub_server.rs"]
mod stub_server;

use std::time::{Duration, Instant};

use common::{TestEnv, fingerprint, now_ms, printed_hooks_block};
use officina_core::relay::gate::{gated_tools, gated_tools_matcher};
use officina_core::relay::run::{EXIT_NOT_STARTED, EXIT_REFUSED};
use officina_host::collector::{Collector, LoadedFloor};
use officina_host::paths::Paths;
use officina_host::watch::{CollectorMsg, spawn};

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
    let block = printed_hooks_block(&first.stdout);
    assert!(
        block["hooks"].get("PreToolUse").is_none(),
        "no guarded project, no gate hook (D-18)"
    );
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
fn doctor_with_a_guarded_project_requires_pre_tool_use() {
    let env = TestEnv::new();
    env.write_agents("[]");
    env.write_floor("schema = 2\n[[project]]\nname = \"lab\"\npath = \"Lab\"\n");
    // The buffered events are already registered, as on a v0.1 workstation.
    let v0_1 = TestEnv::new();
    v0_1.write_agents("[]");
    let v0_1_block = printed_hooks_block(&v0_1.run(&["doctor"], b"").stdout);
    let settings = env.claude_config().join("settings.json");
    std::fs::write(&settings, v0_1_block.to_string()).unwrap();

    let out = env.run(&["doctor"], b"");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(
        text.contains("[FAIL] hooks: not registered for PreToolUse"),
        "{text}"
    );
    let block = printed_hooks_block(&out.stdout);
    assert_eq!(
        block["hooks"]["PreToolUse"][0]["matcher"],
        gated_tools_matcher()
    );

    std::fs::write(&settings, block.to_string()).unwrap();
    let text = String::from_utf8_lossy(&env.run(&["doctor"], b"").stdout).to_string();
    assert!(text.contains("[ok  ] hooks"), "{text}");
}

/// D-171, D-173: a `PreToolUse` registration merged before stories 2.12
/// and 2.14, naming one gated tool, is reported with the tools it leaves
/// out; pasting the block doctor prints then covers every one.
#[test]
fn doctor_with_a_guarded_project_requires_every_gated_tool() {
    let env = TestEnv::new();
    env.write_agents("[]");
    env.write_floor("schema = 2\n[[project]]\nname = \"lab\"\npath = \"Lab\"\n");
    let full = printed_hooks_block(&env.run(&["doctor"], b"").stdout);
    let names = gated_tools();
    let (first, others) = names.split_first().unwrap();
    let mut partial = full.clone();
    partial["hooks"]["PreToolUse"][0]["matcher"] = (*first).into();
    let settings = env.claude_config().join("settings.json");
    std::fs::write(&settings, partial.to_string()).unwrap();

    let out = env.run(&["doctor"], b"");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(
        text.contains(&format!(
            "[FAIL] hooks: not registered for PreToolUse ({})",
            others.join(", ")
        )),
        "{text}"
    );
    let printed = printed_hooks_block(&out.stdout);
    std::fs::write(&settings, printed.to_string()).unwrap();
    let text = String::from_utf8_lossy(&env.run(&["doctor"], b"").stdout).to_string();
    assert!(text.contains("[ok  ] hooks"), "{text}");
}

/// FR-13, D-80: Officina writes nothing under the Claude config dir nor
/// into `~/.claude.json`, whatever command runs, with the window's
/// collector running beside them, ingesting into its registry.
#[test]
fn no_command_writes_under_the_claude_config_dir() {
    let env = TestEnv::new();
    let config = env.claude_config();
    std::fs::write(config.join("settings.json"), r#"{"permissions":{}}"#).unwrap();
    std::fs::write(config.join("sessions").join("1234.json"), "{}").unwrap();
    let claude_json = env.user_home().join(".claude.json");
    std::fs::write(&claude_json, r#"{"numStartups":1}"#).unwrap();
    env.write_agents(
        r#"[{"pid":1,"cwd":"C:/x","kind":"interactive","startedAt":1,"sessionId":"s","status":"busy"}]"#,
    );
    let before = fingerprint(&config);
    let claude_json_before = std::fs::read(&claude_json).unwrap();

    // The window's collector thread runs alongside the commands, pointed at
    // the same config dir, as a floor's `config_dir` would point it, with
    // a copy of the fake listing the agents fixture (D-86).
    let paths = Paths {
        officina_home: env.officina_home(),
        claude_config_dir: config.clone(),
        claude_config_override: Some(config.clone()),
        floor_file: None,
    };
    let fake = common::fake_claude_in(
        &env.root.path().join("bin"),
        &serde_json::json!({"fixture": env.fixture()}),
    );
    let collector = Collector::from_parts(paths, LoadedFloor::default(), Some(fake));
    let (sink, emitted) = common::sink(|_| {});
    let watch = spawn(collector, sink).unwrap();
    emitted
        .boards
        .recv_timeout(Duration::from_secs(10))
        .expect("first board");
    let running_since = Instant::now();

    env.run(&["hook"], br#"{"hook_event_name":"Stop","session_id":"s"}"#);
    env.run(&["quota", "write", "--tee"], STATUSLINE.as_bytes());
    env.run(&["board", "--json"], b"");
    env.run(&["subjects", "--json"], b"");
    env.run(&["doctor"], b"");

    // A gate deny, the relay's own commands, an approval request and
    // doctor's relay checks, with a catalogue approved in this home.
    let lab = env.root.path().join("work").join("Lab");
    std::fs::create_dir_all(&lab).unwrap();
    let lab_path = lab.to_string_lossy().replace('\\', "/");
    let stub = stub_server::Stub::start(vec![stub_server::Reply::ok("up")]);
    let health = stub.url("/health");
    env.write_floor(&format!(
        "schema = 2\n[[project]]\nname = \"lab\"\npath = \"{lab_path}\"\nactions = [\"probe\", \"restart\", \"health\"]\n[[action]]\nname = \"probe\"\nclass = \"free\"\nprogram = \"{{tool}}\"\n[[action]]\nname = \"restart\"\nclass = \"human\"\nprogram = \"{{tool}}\"\n[[action]]\nname = \"health\"\nkind = \"http\"\nclass = \"free\"\nmethod = \"GET\"\nurl = \"{health}\"\n"
    ));
    env.approve();
    let payload = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": "ssh host01"},
        "cwd": lab,
        "session_id": "s",
    });
    let deny = env.run(&["hook"], payload.to_string().as_bytes());
    assert!(String::from_utf8_lossy(&deny.stdout).contains("\"deny\""));
    let mut powershell = payload.clone();
    powershell["tool_name"] = "PowerShell".into();
    let deny = env.run(&["hook"], powershell.to_string().as_bytes());
    assert!(String::from_utf8_lossy(&deny.stdout).contains("\"deny\""));
    // Each command did what it does, by its documented exit code: so the
    // fingerprint below compares a config dir every one of them went
    // through, not one a failing command left alone. Off Windows the floor
    // names the tool by a Windows path (Linux is v1): the run is checked,
    // journaled and not started.
    let probe = if cfg!(windows) { 0 } else { EXIT_NOT_STARTED };
    for (args, code) in [
        (&["run", "probe"][..], probe),
        (&["run", "health"], 0),
        (&["run", "--list"], 0),
        (&["run", "restart", "--wait", "1"], EXIT_REFUSED),
        (&["relay", "--json"], 0),
        // The hooks block is not merged in this config dir: doctor fails.
        (&["doctor"], 1),
    ] {
        let mut cmd = env.officina(args);
        cmd.current_dir(&lab);
        let out = common::run_with(cmd, b"");
        assert_eq!(
            out.status.code(),
            Some(code),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // Long enough for the collector to see the hook and quota writes, spawn
    // `claude` again and run its follow-up refresh.
    std::thread::sleep(Duration::from_secs(2).saturating_sub(running_since.elapsed()));
    watch.sender.send(CollectorMsg::Shutdown).unwrap();
    watch.thread.join().unwrap();

    assert_eq!(stub.requests().len(), 1, "the HTTP run went out");
    let ingested: Vec<String> =
        officina_host::registry::store::open_reader(&env.officina_home(), &[])
            .unwrap()
            .sessions()
            .unwrap()
            .into_iter()
            .map(|s| s.session_id)
            .collect();
    assert!(
        ingested.contains(&"s".to_owned()),
        "the collector ingested: {ingested:?}"
    );
    assert_eq!(fingerprint(&config), before);
    assert_eq!(std::fs::read(&claude_json).unwrap(), claude_json_before);
}

/// Story 3.8 AC 4 (AD-56, D-71): doctor prints, for the running version,
/// the hook fields its lines left out and the buffered events none of its
/// lines carried yet, from the record `board --json` started and the
/// window's ingestion filled.
#[test]
fn doctor_reports_hook_fields_missing_and_not_observed() {
    use officina_core::contract::HOOK_FIELDS;
    use officina_host::collector::{ContractStore, record_hook_fields};
    use officina_host::doctor::BUFFERED_EVENTS;
    use officina_host::registry::ingest::IngestContext;
    use officina_host::registry::store::open_writer;

    let env = TestEnv::new();
    env.write_agents(
        r#"[{"pid":1,"cwd":"C:/work/api","kind":"interactive","startedAt":1,"sessionId":"s","status":"busy"}]"#,
    );
    let out = env.run(&["doctor"], b"");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("hook fields") && text.contains("not checked yet"),
        "no record before a version was first seen: {text}"
    );
    assert!(env.run(&["board", "--json"], b"").status.success());
    let contract = env.officina_home().join("state").join("contract.json");
    let version = ContractStore::read(&contract)
        .versions
        .keys()
        .next()
        .cloned()
        .expect("board --json records the running version");

    let (event, fields) = HOOK_FIELDS
        .iter()
        .find(|(event, _)| *event == "SessionEnd")
        .unwrap();
    let mut payload = serde_json::json!({"hook_event_name": event});
    for field in fields.iter().skip(1) {
        payload[*field] = serde_json::json!("redacted");
    }
    assert!(
        env.run(&["hook"], payload.to_string().as_bytes())
            .status
            .success()
    );
    // The window's ingestion, then its fold into the version's record.
    let (events, home) = (env.events_dir(), env.officina_home());
    let ctx = IngestContext {
        events_dir: &events,
        officina_home: &home,
        projects: &[],
        no_excerpts: false,
    };
    let mut writer = open_writer(&home, &[], now_ms()).writer().unwrap();
    let report = writer.ingest(&ctx, now_ms()).unwrap();
    drop(writer);
    record_hook_fields(&contract, &version, &report.hook_sightings).unwrap();

    let out = env.run(&["doctor"], b"");
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text
        .lines()
        .find(|line| line.contains("hook fields"))
        .unwrap_or_else(|| panic!("no hook fields line: {text}"));
    assert!(line.contains("[FAIL]"), "{line}");
    assert!(
        line.contains(&format!("missing: {event}.{}", fields[0])),
        "{line}"
    );
    let not_observed: Vec<&str> = BUFFERED_EVENTS
        .iter()
        .copied()
        .filter(|e| e != event)
        .collect();
    assert!(
        line.contains(&format!("not observed yet: {}", not_observed.join(", "))),
        "{line}"
    );
    assert_eq!(out.status.code(), Some(1));
}
