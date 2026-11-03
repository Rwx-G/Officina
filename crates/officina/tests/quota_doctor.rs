//! `officina quota write`, `officina doctor`, and the zero-write guarantee on
//! the Claude config dir (stories 1.8, 1.9, 1.11; FR-5, FR-11, FR-13).

mod common;
#[path = "../../../tests/support/stub_server.rs"]
mod stub_server;

use std::time::{Duration, Instant};

use common::{TestEnv, fingerprint, now_ms, printed_hooks_block};
use officina_core::orders::card::{CardAnswer, CardRequest, LaunchAnswer};
use officina_core::registry::types::LaunchOutcome;
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
    // The quota line names the windows the file carries (D-1042).
    assert!(text.contains("windows five_hour and seven_day"), "{text}");
    assert!(text.contains("9.9.9"));

    // A statusline without the five-hour window: the file and the line
    // drop it, and the board carries no five-hour window.
    env.run(
        &["quota", "write"],
        br#"{"session_id":"s1","rate_limits":{"seven_day":{"used_percentage":41.0,"resets_at":1800500000}}}"#,
    );
    let text = String::from_utf8_lossy(&env.run(&["doctor"], b"").stdout).to_string();
    assert!(text.contains("[ok  ] quota"), "{text}");
    assert!(text.contains("window seven_day"), "{text}");
    assert!(!text.contains("five_hour"), "{text}");
    let board: serde_json::Value =
        serde_json::from_slice(&env.run(&["board", "--json"], b"").stdout).unwrap();
    assert!(board["quota"]["five_hour"].is_null());
    assert_eq!(board["quota"]["seven_day"]["used_percentage"], 41.0);
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
/// AC 6), a dry run's call made through the fake (story 3.9 AC 12), and a
/// work order launched from a card through the fake (story 5.7 AC 8), a
/// review of an `auto` request through the fake (story 5.13 AC 11), a
/// scripted `officina mcp` session calling every tool (story 5.15 AC 9), and
/// the orchestrator started from the window through the fake (story 5.16
/// AC 8), the hook's `PermissionRequest` path through no window, allow,
/// deny, decline and the deadline (story 5.22 AC 6), and its
/// `PermissionDenied` line (story 5.23 AC 5).
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
    let collector = Collector::from_parts(paths, LoadedFloor::default(), Some(fake))
        .with_confirm_window(Duration::from_secs(1))
        .with_base_env(officina_host::spawn::BaseEnv::new(vec![(
            "SYSTEMROOT".to_owned(),
            std::env::var("SYSTEMROOT").unwrap_or_default(),
        )]));
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
        "schema = 2\n[[project]]\nname = \"lab\"\npath = \"{lab_path}\"\nactions = [\"probe\", \"restart\", \"health\"]\n[[template]]\nname = \"probe-notes\"\ndescription = \"Write the probe notes\"\nprojects = [\"lab\"]\nprompt = \"Write the notes for {{host}}.\"\n[template.params.host]\nvalues = [\"web01\"]\n[[action]]\nname = \"probe\"\nclass = \"free\"\nprogram = \"{{tool}}\"\n[[action]]\nname = \"restart\"\nclass = \"human\"\nprogram = \"{{tool}}\"\n[[action]]\nname = \"health\"\nkind = \"http\"\nclass = \"free\"\nmethod = \"GET\"\nurl = \"{health}\"\n"
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
        // It reads the permissions and autoMode keys and writes neither
        // (story 5.20 AC 5).
        (&["doctor", "--permissions"], 1),
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

    // A scripted `officina mcp` session in the orchestrator's folder,
    // calling every tool, the proposal tools included (story 5.15 AC 9).
    let workdir = env
        .officina_home()
        .join(officina_core::orders::ORCHESTRATOR_DIR)
        .join(officina_core::orders::ORCHESTRATOR_WORKDIR);
    std::fs::create_dir_all(&workdir).unwrap();
    let mut mcp = env.officina(&["mcp"]);
    mcp.env("CLAUDE_PROJECT_DIR", &workdir)
        .current_dir(&workdir);
    let mut requests = common::mcp::handshake();
    requests.extend(common::mcp::every_tool(&common::mcp::Sources::neutral(), 1));
    let (out, responses) = common::mcp::McpHome::exchange(mcp, &requests);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        responses.len(),
        requests.len() - 1,
        "every request but the notification answered"
    );

    // A work order launched from a card, as the window's commands do, with
    // the collector's fake answering `--bg` (story 5.7 AC 8, D-80).
    let request = CardRequest {
        template: "probe-notes".to_owned(),
        project: "lab".to_owned(),
        values: vec![("host".to_owned(), "web01".to_owned())],
        subject: None,
    };
    let card = match officina_host::orders::issue_card(
        &watch.sender,
        request,
        &common::workshop::host_runtime(),
    )
    .unwrap()
    {
        CardAnswer::Issued(card) => card,
        refused => panic!("{refused:?}"),
    };
    assert_eq!(
        officina_host::orders::launch_card(
            &watch.sender,
            &card.id,
            &card.prompt_sha256,
            None,
            &common::workshop::host_runtime()
        ),
        Ok(LaunchAnswer::Launched(LaunchOutcome::Started))
    );

    // An answer resumed through the fake, as the meeting's Send does
    // (story 5.11 AC 8, D-80): a background session listed in its folder,
    // the console started there, its confirmation watched.
    #[cfg(windows)]
    {
        use officina_core::meeting::{AnswerRefused, AnswerResult, Delivery};
        let folder = env.root.path().join("work").join("answered");
        std::fs::create_dir_all(&folder).unwrap();
        let mut listed: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(env.fixture()).unwrap()).unwrap();
        listed.push(
            serde_json::json!({"id": "bg0042", "sessionId": "0f1e2d3c-4b5a-4978-8796-a5b4c3d2e1f0",
            "cwd": folder, "kind": "background", "pid": 4242, "startedAt": now_ms() - 60_000,
            "status": "idle", "state": "done"}),
        );
        env.write_agents(&serde_json::Value::from(listed).to_string());
        watch.sender.send(CollectorMsg::Refresh).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let answered = loop {
            let result = officina_host::meeting::send_answer(
                &watch.sender,
                "bg:bg0042",
                Delivery::ResumeLive,
                officina_host::meeting::AnswerFrom::Author("Keep going.".to_owned()),
            )
            .unwrap();
            let unseen = matches!(
                result,
                AnswerResult::Refused(AnswerRefused::UnknownSession | AnswerRefused::StateChanged)
            );
            if !unseen || Instant::now() > deadline {
                break result;
            }
            std::thread::sleep(Duration::from_millis(200));
        };
        assert!(
            matches!(
                answered,
                AnswerResult::Confirmed | AnswerResult::NotConfirmed
            ),
            "{answered:?}"
        );

        // An answer proposal sent by its id through the fake (story 5.21
        // AC 9, D-80): a second session idle in its turn, the proposal
        // ingested once the board shows it, then the click's resume.
        use officina_core::orders::proposal::{AnswerProposal, ProposalFile, file_name};
        use officina_core::registry::types::ProposalStatus;
        let proposed_session = "1a2b3c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d";
        let mut listed: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(env.fixture()).unwrap()).unwrap();
        listed.push(
            serde_json::json!({"id": "bg0043", "sessionId": proposed_session,
            "cwd": folder, "kind": "background", "pid": 4243, "startedAt": now_ms() - 60_000,
            "status": "idle", "state": "done"}),
        );
        env.write_agents(&serde_json::Value::from(listed).to_string());
        watch.sender.send(CollectorMsg::Refresh).unwrap();
        let dir = officina_host::paths::orchestrator_proposals_dir(&env.officina_home());
        let deadline = Instant::now() + Duration::from_secs(10);
        let pending = loop {
            let at = now_ms();
            let file = ProposalFile::answer(
                AnswerProposal {
                    session_id: proposed_session.to_owned(),
                    text: "Keep the 1.x line.".to_owned(),
                    rationale: "It asked which line.".to_owned(),
                },
                None,
                at,
            );
            let id = officina_host::mcp::write_proposal(&dir, &file, at).unwrap();
            while dir.join(file_name(&id)).exists() {
                assert!(Instant::now() < deadline, "the window did not ingest {id}");
                std::thread::sleep(Duration::from_millis(50));
            }
            let status = officina_host::registry::store::open_reader(&env.officina_home(), &[])
                .unwrap()
                .proposals()
                .unwrap()
                .into_iter()
                .find(|p| p.id == id)
                .map(|p| p.status);
            if status == Some(ProposalStatus::Pending) {
                break id;
            }
            assert!(Instant::now() < deadline, "no proposal stayed pending");
            std::thread::sleep(Duration::from_millis(200));
        };
        let sent = officina_host::meeting::send_answer(
            &watch.sender,
            "bg:bg0043",
            Delivery::ResumeLive,
            officina_host::meeting::AnswerFrom::Proposal(pending),
        )
        .unwrap();
        assert!(
            matches!(sent, AnswerResult::Confirmed | AnswerResult::NotConfirmed),
            "{sent:?}"
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

    // A review of an `auto` request through the fake, pointed at the same
    // config dir (story 5.13 AC 11, D-80).
    let review_fake = common::fake_claude_in(
        &env.root.path().join("bin-review"),
        &serde_json::json!({"mode": "review"}),
    );
    let reviewed = officina_host::relay::review::run_call(
        &officina_host::relay::review::ReviewCall {
            approval_id: "0123456789abcdef-1".into(),
            model: officina_core::relay::template::ModelAlias::Haiku,
            prompt: "FAKE-REVIEW:approve".into(),
            claude: review_fake,
            officina_home: env.officina_home(),
            config_dir: Some(config.clone()),
            base_env: officina_host::spawn::BaseEnv::new(vec![(
                "SYSTEMROOT".to_owned(),
                std::env::var("SYSTEMROOT").unwrap_or_default(),
            )]),
            timeout: officina_host::relay::review::review_timeout(),
        },
        &officina_host::files::now_ms,
    );
    assert_eq!(
        reviewed.outcome,
        officina_core::registry::types::ReviewOutcome::Answered,
        "{reviewed:?}"
    );

    // The orchestrator started from the window, through the collector's
    // fake, pointed at the same config dir (story 5.16 AC 8, D-80). Off
    // Windows no console opens, and the Start says so.
    let exe_dir = env.root.path().join("app");
    std::fs::create_dir_all(&exe_dir).unwrap();
    std::fs::write(
        exe_dir.join(officina_host::orders::orchestrator::officina_exe_name()),
        b"",
    )
    .unwrap();
    {
        use officina_core::orders::orchestrator::{StartAnswer, StartRefusal};
        let expected = if cfg!(windows) {
            StartAnswer::Started
        } else {
            StartAnswer::Refused(StartRefusal::Unsupported)
        };
        assert_eq!(
            officina_host::orders::orchestrator::start_orchestrator(&watch.sender, &exe_dir),
            Ok(expected)
        );
    }

    // The hook's `PermissionRequest` path (story 5.22 AC 6, D-80): with the
    // collector's marker, which routes nothing yet, then with a routing
    // one through allow, deny, decline and the deadline.
    prompt_paths(&env);

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
    // `officina stop --all` last, since it stops the relay (story 6.17
    // AC 3). Its engine is a path that does not exist, so the machine's
    // own Docker, found through the registry, is never reached: it writes
    // the flags and exits 1.
    let mut stop = env.officina(&["stop", "--all"]);
    stop.current_dir(&lab).env(
        "OFFICINA_DOCKER_BIN",
        env.root.path().join("no-docker").join("docker.exe"),
    );
    let out = common::run_with(stop, b"");
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(env.relay_home().stop_flags().workshop);
    assert_eq!(fingerprint(&config), before);
    assert_eq!(std::fs::read(&claude_json).unwrap(), claude_json_before);
}

/// Story 5.20 AC 3 (D-574): `officina doctor --permissions` prints both
/// blocks, writes them under the Officina home and nothing in the config
/// dir or `~/.claude.json`; a plain run then removes the two files.
#[test]
fn doctor_permissions_writes_no_settings_file() {
    let env = TestEnv::new();
    let config = env.claude_config();
    std::fs::write(
        config.join("settings.json"),
        r#"{"permissions": {"allow": ["Read"], "defaultMode": "bypassPermissions"}}"#,
    )
    .unwrap();
    let claude_json = env.user_home().join(".claude.json");
    std::fs::write(&claude_json, r#"{"numStartups":1}"#).unwrap();
    let before = fingerprint(&config);
    let out = env.run(&["doctor", "--permissions"], b"");
    let printed = String::from_utf8_lossy(&out.stdout);
    assert!(printed.contains("\"defaultMode\": \"auto\""), "{printed}");
    assert!(printed.contains("<trusted repository>"), "{printed}");
    assert!(printed.contains("your decision"), "{printed}");
    let doctor_dir = env.officina_home().join("doctor");
    for name in ["user-permissions.json", "user-autoMode.json"] {
        assert!(doctor_dir.join(name).is_file(), "{name}");
    }
    assert_eq!(fingerprint(&config), before);
    assert_eq!(
        std::fs::read(&claude_json).unwrap(),
        br#"{"numStartups":1}"#
    );
    let plain = env.run(&["doctor"], b"");
    assert!(String::from_utf8_lossy(&plain.stdout).contains("officina doctor --permissions"));
    for name in ["user-permissions.json", "user-autoMode.json"] {
        assert!(!doctor_dir.join(name).exists(), "{name}");
    }
    assert_eq!(fingerprint(&config), before);
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

/// The project's files under `.git` but its objects, by content: where a
/// reference, the index, `FETCH_HEAD`, a reflog or the configuration
/// would change.
fn git_dir_but_objects(project: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    fingerprint(&project.join(".git"))
        .into_iter()
        .filter(|(path, _)| !path.starts_with("objects/"))
        .collect()
}

/// A tracked file's modification time moved, so a plain `git status` (or
/// any call taking the optional index lock) would rewrite the index.
fn touch_tracked(project: &std::path::Path) {
    let file = std::fs::File::options()
        .write(true)
        .open(project.join("README.md"))
        .unwrap();
    file.set_modified(std::time::SystemTime::now() + Duration::from_secs(5))
        .unwrap();
}

/// Story 6.16 AC 2 (PRD section 7 Host untouched, AD-125, D-80, D-163):
/// a launch, a collection, a quarantine and a discard through the fake
/// engine leave the Claude config dir and the project repository (its
/// references, index, objects and work tree) as they were.
#[test]
fn a_quarantine_and_a_discard_leave_the_project_unchanged() {
    use common::package_run::{home, launch, work_tree};
    use common::workshop_home::Options;
    use officina_core::workshop::review::{DiscardAnswer, DiscardReason};
    use officina_core::workshop::run::RunState;

    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let project = home.project();
    touch_tracked(&project);
    let config = fingerprint(&home.env.claude_config());
    let repository = fingerprint(&project);
    let tree = work_tree(&home);
    for (path, state) in [
        (".claude/settings.json", RunState::Quarantined),
        ("src/a.rs", RunState::Review),
    ] {
        let run = launch(&home, home.seeded());
        let clone = run.clone();
        run.commit(&clone, path, "{}\n");
        run.serve(&run.packed_branch(&clone), 0);
        run.exit(0);
        assert_eq!(run.collected().state, state, "{path}");
        assert_eq!(
            run.discard(DiscardReason::ShouldNotHaveRun),
            DiscardAnswer::Discarded
        );
    }
    assert_eq!(fingerprint(&home.env.claude_config()), config);
    assert_eq!(fingerprint(&project), repository);
    assert_eq!(work_tree(&home), tree);
}

/// Story 6.16 AC 2 (AD-114, AD-125): Import adds
/// `refs/heads/officina/<name>` and objects, and nothing else: no other
/// file under `.git` (no `FETCH_HEAD`, no reflog, the index as it was),
/// no object removed or rewritten, the work tree and the Claude config
/// dir untouched.
#[test]
fn import_changes_only_its_reference_and_objects() {
    use common::package_run::{home, launch, work_tree};
    use common::workshop_home::Options;
    use officina_core::workshop::review::ImportAnswer;

    let Some(home) = home(&Options::default(), &[]) else {
        return;
    };
    let project = home.project();
    touch_tracked(&project);
    let config = fingerprint(&home.env.claude_config());
    let run = launch(&home, home.seeded());
    let clone = run.clone();
    run.commit(&clone, "build.rs", "fn main() {}\n");
    run.serve(&run.packed_branch(&clone), 0);
    run.exit(0);
    run.collected();
    run.approve_all();
    let git_dir = git_dir_but_objects(&project);
    let objects = fingerprint(&project.join(".git").join("objects"));
    let tree = work_tree(&home);
    let row = run.row();
    let tip = row.tip_commit.clone().unwrap();
    assert_eq!(run.import(), ImportAnswer::Imported(row.branch.clone()));
    let mut expected = git_dir.clone();
    let reference = format!("refs/heads/{}", row.branch);
    let segments: Vec<&str> = reference.split('/').collect();
    for end in 1..segments.len() {
        let dir = format!("{}/", segments[..end].join("/"));
        if !expected.iter().any(|(path, _)| *path == dir) {
            expected.push((dir, Vec::new()));
        }
    }
    expected.push((reference, format!("{tip}\n").into_bytes()));
    expected.sort();
    assert_eq!(git_dir_but_objects(&project), expected);
    let after = fingerprint(&project.join(".git").join("objects"));
    for object in &objects {
        assert!(after.contains(object), "{} changed or went", object.0);
    }
    assert!(after.len() > objects.len(), "no object was fetched");
    assert_eq!(work_tree(&home), tree);
    assert_eq!(fingerprint(&home.env.claude_config()), config);
}

/// Runs `officina hook` on a `PermissionRequest` through every end of its
/// path (story 5.22 AC 6): no window (the collector's marker routes
/// nothing, D-580), then a routing marker with allow, deny and decline
/// written while it waits, and the deadline.
fn prompt_paths(env: &TestEnv) {
    use officina_core::relay::prompts::{PromptAnswer, WindowMarker};
    use officina_host::relay::authority::decide_prompt;
    use officina_host::relay::prompts::{ENV_TEST_PROMPT_WAIT_MS, MarkerFile, read_requests};
    use std::io::Write;
    let payload = serde_json::json!({
        "hook_event_name": "PermissionRequest",
        "session_id": "s",
        "permission_mode": "default",
        "tool_name": "Bash",
        "tool_input": {"command": "rm -rf build"},
    })
    .to_string();
    let no_window = env.run(&["hook"], payload.as_bytes());
    assert!(no_window.status.success() && no_window.stdout.is_empty());
    let hook = |wait_ms: &str| {
        let mut cmd = env.officina(&["hook"]);
        cmd.env(ENV_TEST_PROMPT_WAIT_MS, wait_ms);
        let mut child = cmd.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(payload.as_bytes())
            .unwrap();
        child
    };
    let _window = MarkerFile::write(
        &env.relay_home(),
        &WindowMarker {
            pid: std::process::id(),
            started_at_ms: now_ms(),
            routes_prompts: true,
        },
    )
    .unwrap();
    for answer in PromptAnswer::ALL {
        let child = hook("20000");
        let deadline = Instant::now() + Duration::from_secs(10);
        let id = loop {
            if let Some(request) = read_requests(&env.relay_home()).into_iter().next() {
                break request.id;
            }
            assert!(Instant::now() < deadline, "the hook wrote no request");
            std::thread::sleep(Duration::from_millis(10));
        };
        decide_prompt(&env.relay_home(), &id, *answer, now_ms()).unwrap();
        let out = child.wait_with_output().unwrap();
        assert_eq!(out.status.code(), Some(0));
        assert_eq!(out.stdout.is_empty(), answer.behavior().is_none());
    }
    let out = hook("300").wait_with_output().unwrap();
    assert!(out.status.success() && out.stdout.is_empty());
    // Story 5.23 AC 5: auto mode's denial, journaled.
    let denied = serde_json::json!({
        "hook_event_name": "PermissionDenied",
        "session_id": "s",
        "permission_mode": "auto",
        "tool_name": "Bash",
        "tool_input": {"command": "rm -rf build"},
        "reason": "[Irreversible Local Destruction]",
    });
    let out = env.run(&["hook"], denied.to_string().as_bytes());
    assert!(out.status.success() && out.stdout.is_empty());
}
