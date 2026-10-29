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

/// D-530: with user settings holding foreign hooks and other keys, doctor
/// prints the file's whole `hooks` key with Officina's registrations merged
/// in, and nothing of the other keys; once it replaces the key, doctor
/// reports the hooks registered and prints no block for them.
#[test]
fn doctor_prints_the_whole_hooks_key_merged() {
    const SECRET: &str = "sk-not-a-real-token";
    let env = TestEnv::new();
    env.write_agents("[]");
    env.write_floor("schema = 2\n[[project]]\nname = \"lab\"\npath = \"Lab\"\n");
    let foreign = serde_json::json!({
        "PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "python guard.py", "onFailure": "block"}]}],
        "PostToolUse": [{"matcher": "Edit", "hooks": [{"type": "command", "command": "npx prettier --write"}]}]
    });
    let settings = env.claude_config().join("settings.json");
    let mut file = serde_json::json!({"env": {"API_TOKEN": SECRET}, "hooks": foreign});
    std::fs::write(&settings, file.to_string()).unwrap();

    let out = env.run(&["doctor"], b"");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        text.contains(&format!(
            "Replace the whole \"hooks\" key of {}",
            settings.display()
        )),
        "{text}"
    );
    assert!(
        !text.contains(SECRET) && !text.contains("\"env\""),
        "{text}"
    );
    // D-534: the same block, its lines whole, in the file the line names.
    let saved = env.officina_home().join("doctor").join("user-hooks.json");
    assert!(text.contains(&saved.display().to_string()), "{text}");
    let saved_text = std::fs::read_to_string(&saved).unwrap();
    assert!(text.contains(saved_text.trim_end()), "{saved_text}");
    let merged = printed_hooks_block(&out.stdout)["hooks"].clone();
    for (event, groups) in foreign.as_object().unwrap() {
        assert_eq!(merged[event][0], groups[0], "{event}");
    }
    assert_eq!(merged["PreToolUse"][1]["matcher"], gated_tools_matcher());

    file["hooks"] = merged;
    std::fs::write(&settings, file.to_string()).unwrap();
    let text = String::from_utf8_lossy(&env.run(&["doctor"], b"").stdout).to_string();
    let hooks_line = text
        .lines()
        .position(|line| line.starts_with("[ok  ] hooks:"))
        .unwrap_or_else(|| panic!("{text}"));
    assert!(
        text.lines().nth(hooks_line + 1).unwrap().starts_with('['),
        "nothing printed under the hooks line: {text}"
    );
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
/// collector running beside them, ingesting into its registry and sending a
/// row to the enabled model through the fake (story 3.11, story 3.12
/// AC 6), and a dry run's call made through the fake (story 3.9 AC 12).
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
    // A row the window's model is sent: a session ended after a prompt,
    // and the model on after a winning dry run.
    for payload in [
        r#"{"hook_event_name":"UserPromptSubmit","session_id":"t","cwd":"C:/x","prompt":"Ship it FAKE-VERDICT:finished"}"#,
        r#"{"hook_event_name":"Stop","session_id":"t","cwd":"C:/x","last_assistant_message":"Shipped."}"#,
        r#"{"hook_event_name":"SessionEnd","session_id":"t","cwd":"C:/x","reason":"prompt_input_exit"}"#,
    ] {
        assert!(env.run(&["hook"], payload.as_bytes()).status.success());
    }
    {
        let (events, home) = (env.events_dir(), env.officina_home());
        let mut writer = officina_host::registry::store::open_writer(&home, &[], now_ms())
            .writer()
            .unwrap();
        writer
            .ingest(
                &officina_host::registry::ingest::IngestContext {
                    events_dir: &events,
                    officina_home: &home,
                    projects: &[],
                    no_excerpts: false,
                },
                now_ms(),
            )
            .unwrap();
        writer
            .record_evaluation(
                &officina_core::registry::dry_run::Tally {
                    cases: 30,
                    model_correct: 20,
                    default_correct: 10,
                    ..Default::default()
                },
                now_ms() - 60_000,
            )
            .unwrap();
        writer.set_triage_model(true, now_ms()).unwrap().unwrap();
    }
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
        &serde_json::json!({"fixture": env.fixture(), "mode": "triage"}),
    );
    let collector = Collector::from_parts(paths, LoadedFloor::default(), Some(fake)).with_base_env(
        officina_host::spawn::BaseEnv::new(vec![(
            "SYSTEMROOT".to_owned(),
            std::env::var("SYSTEMROOT").unwrap_or_default(),
        )]),
    );
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

    // A triage call through the fake, pointed at the same config dir as a
    // floor's `config_dir` would point it (story 3.9 AC 12, D-80).
    let triage_fake = common::fake_claude_in(
        &env.root.path().join("bin-triage"),
        &serde_json::json!({"mode": "triage"}),
    );
    let call = officina_host::registry::triage::run_call(
        &officina_host::registry::triage::TriageJob {
            purpose: officina_core::registry::types::TriagePurpose::DryRun,
            case_key: "confirmation:1".into(),
            case: officina_core::registry::triage_call::TriageCase {
                last_prompt: Some("Ship it FAKE-VERDICT:finished".into()),
                ..Default::default()
            },
            claude: Some(triage_fake),
            officina_home: env.officina_home(),
            config_dir: Some(config.clone()),
            no_excerpts: false,
            base_env: officina_host::spawn::BaseEnv::new(vec![(
                "SYSTEMROOT".to_owned(),
                std::env::var("SYSTEMROOT").unwrap_or_default(),
            )]),
            limits: officina_host::registry::triage::CallLimits::default(),
        },
        &officina_host::files::now_ms,
    );
    assert_eq!(
        call.outcome,
        officina_core::registry::types::TriageOutcome::Answer,
        "{call:?}"
    );

    // Long enough for the collector to see the hook and quota writes, spawn
    // `claude` again and run its follow-up refresh, and for its model call
    // to come back.
    std::thread::sleep(Duration::from_secs(2).saturating_sub(running_since.elapsed()));
    let deadline = Instant::now() + Duration::from_secs(10);
    let answered = || {
        officina_host::registry::store::open_reader(&env.officina_home(), &[])
            .ok()
            .and_then(|reader| reader.model_facts(None).ok())
            .is_some_and(|facts| !facts.answers.is_empty())
    };
    while !answered() {
        assert!(
            Instant::now() < deadline,
            "the window's model call never came back"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
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
    // A new agents sample rewrites the record's agents fields (a session
    // now waits, so `waitingFor` is observed); the hook fields stay.
    let before = ContractStore::read(&contract).versions[&version].clone();
    env.write_agents(
        r#"[{"pid":1,"cwd":"C:/work/api","kind":"interactive","startedAt":1,"sessionId":"s","status":"waiting","waitingFor":"permission prompt"}]"#,
    );
    assert!(env.run(&["board", "--json"], b"").status.success());
    let after = ContractStore::read(&contract).versions[&version].clone();
    assert_ne!(
        after.not_observed, before.not_observed,
        "the record was rewritten"
    );
    assert_eq!(
        (
            after.hook_missing,
            after.hook_not_observed,
            after.first_seen_at
        ),
        (
            before.hook_missing,
            before.hook_not_observed,
            before.first_seen_at
        )
    );

    let out = env.run(&["doctor"], b"");
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text
        .lines()
        .find(|line| line.contains("hook fields"))
        .unwrap_or_else(|| panic!("no hook fields line: {text}"));
    assert!(line.contains("[warn]"), "{line}");
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
    // Exit 1 comes from the hooks this home never registered, not from the
    // hook fields line, which only warns (`doctor_registry.rs`,
    // `warnings_alone_exit_0`, holds that rule).
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("[FAIL] hooks"), "{text}");
    assert_eq!(out.status.code(), Some(1));
}
