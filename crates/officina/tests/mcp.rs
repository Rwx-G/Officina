//! `officina mcp` end to end (story 5.15, AD-90, AD-91, AD-95 MCP row):
//! a scripted client speaks E11's handshake to the real binary and calls
//! every tool, with the window closed; the start check, the read model, the
//! wrapped outputs, the proposal files and their limits, and what no tool
//! does.

mod common;

use common::mcp::{
    McpHome, SESSION_LABEL, Sources, arguments, call, every_tool, handshake, marker_places, output,
    response, text,
};
use common::{fake_claude_calls, fingerprint, logged_argv, now_ms};
use officina_core::mcp::{ArgsRefusal, McpError, McpTool};
use officina_core::orders::proposal::{
    PROPOSAL_HOUR_MS, PROPOSAL_MAX_VALUES, PROPOSAL_TEXT_MAX_CHARS, PROPOSAL_VALUE_MAX_CHARS,
    PROPOSALS_PENDING_MAX, PROPOSALS_PER_HOUR, ProposalFile, ProposalTarget, RateRefusal,
    RecapProposal, id_of_file, written_at,
};
use officina_core::registry::types::OrchestratorStartOutcome;
use officina_core::spawn::{AGENT_VIEW_VAR, SESSION_ENV_REMOVED};
use officina_host::mcp::write_proposal;
use officina_host::registry::RegistryPaths;
use officina_host::registry::orders::OrchestratorStartRecord;
use serde_json::{Value, json};

/// The ids of the responses of `every_tool`'s calls, by tool.
fn by_tool(responses: &[Value]) -> Vec<(McpTool, Value)> {
    responses
        .iter()
        .filter(|r| r["id"].as_u64().is_some_and(|id| id >= 100))
        .map(|r| {
            let out = output(r);
            let tool = McpTool::from_code(out["tool"].as_str().unwrap_or_default())
                .unwrap_or_else(|| panic!("{r}"));
            (tool, out)
        })
        .collect()
}

fn files(home: &McpHome) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(home.proposals_dir())
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// AD-95, E11: `server/discover` gets -32601, `initialize` echoes the
/// client's version with tools only, the notification gets nothing, and a
/// call answers; the end of stdin exits 0 with nothing on stderr.
#[test]
fn the_e11_handshake_completes() {
    let home = McpHome::new();
    let (out, responses) = home.session(&[
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "ping"}),
        call(4, McpTool::Briefing, json!({})),
    ]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(responses.len(), 5, "no answer to the notification");
    assert_eq!(
        response(&responses, 0)["error"]["code"],
        McpError::MethodNotFound.number()
    );
    let init = &response(&responses, 1)["result"];
    assert_eq!(init["protocolVersion"], "2025-11-25");
    assert_eq!(init["capabilities"], json!({"tools": {}}));
    assert_eq!(init["serverInfo"]["name"], "officina");
    assert_eq!(response(&responses, 3)["result"], json!({}));
    let briefing = response(&responses, 4);
    assert_eq!(briefing["result"]["isError"], false);
    assert_eq!(output(briefing)["tool"], "briefing");
}

/// AD-90: `tools/list` names every tool of `McpTool::ALL`, in order.
#[test]
fn tools_list_names_every_tool() {
    let home = McpHome::new();
    let (_, responses) =
        home.session(&[json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})]);
    let names: Vec<&str> = response(&responses, 2)["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        McpTool::ALL.iter().map(|t| t.code()).collect::<Vec<_>>()
    );
}

/// AD-90, FR-11: anywhere but the orchestrator's folder, one line on
/// stderr, nothing on stdout, exit 1, never 2.
#[test]
fn a_wrong_project_dir_exits_1_with_one_line() {
    let home = McpHome::new();
    let elsewhere = [
        Some(home.env.officina_home()),
        Some(home.workdir().join("sub")),
        Some(home.lab()),
        None,
    ];
    for dir in elsewhere {
        let mut cmd = home.command(&[]);
        match &dir {
            Some(dir) => cmd.env("CLAUDE_PROJECT_DIR", dir),
            None => cmd.env_remove("CLAUDE_PROJECT_DIR"),
        };
        let (out, responses) = McpHome::exchange(cmd, &handshake());
        assert_eq!(out.status.code(), Some(1), "{dir:?}");
        assert!(responses.is_empty());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(stderr.lines().count(), 1, "{stderr}");
        assert!(stderr.contains("CLAUDE_PROJECT_DIR"), "{stderr}");
    }
    let mut spelled = home.command(&[]);
    let other_spelling = home
        .workdir()
        .to_string_lossy()
        .replace('\\', "/")
        .to_uppercase();
    spelled.env("CLAUDE_PROJECT_DIR", format!("{other_spelling}/"));
    let (out, _) = McpHome::exchange(spelled, &handshake());
    assert_eq!(
        out.status.code(),
        Some(0),
        "the folder normalised serves: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// AD-90: a usage error exits 64, never 2, and the usage lists `mcp`.
#[test]
fn a_usage_error_exits_64() {
    let home = McpHome::new();
    for args in [
        &["--floor"][..],
        &["--json"],
        &["serve"],
        &["--floor", "a", "b"],
    ] {
        let (out, _) = McpHome::exchange(home.command(args), &handshake());
        assert_eq!(out.status.code(), Some(64), "{args:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("mcp [--floor <path>]"));
    }
}

/// AD-90, D-693, R38: the server never prints its environment: a marker
/// under each messaging name of `SESSION_ENV_REMOVED` reaches neither
/// stderr, nor any output, nor a proposal file, whether it serves or
/// refuses to.
#[test]
fn stderr_never_holds_an_environment_value() {
    const MARKER: &str = "ENV-MARKER-5f2e9c";
    let home = McpHome::new();
    let subject = home.seed(&Sources::neutral());
    let messaging: Vec<&str> = SESSION_ENV_REMOVED
        .iter()
        .copied()
        .filter(|name| name.contains("MESSAGING"))
        .collect();
    assert!(!messaging.is_empty());
    let mut requests = handshake();
    requests.extend(every_tool(&Sources::neutral(), subject));
    let mut serving = home.command(&[]);
    let mut refusing = home.command(&[]);
    refusing.env("CLAUDE_PROJECT_DIR", home.lab());
    for cmd in [&mut serving, &mut refusing] {
        for name in &messaging {
            cmd.env(name, MARKER);
        }
    }
    for cmd in [serving, refusing] {
        let (out, responses) = McpHome::exchange(cmd, &requests);
        for stream in [&out.stdout, &out.stderr] {
            assert!(!String::from_utf8_lossy(stream).contains(MARKER));
        }
        assert!(responses.iter().all(|r| !r.to_string().contains(MARKER)));
    }
    for name in files(&home) {
        let bytes = std::fs::read(home.proposals_dir().join(name)).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains(MARKER));
    }
}

/// AD-53, AD-90, D-82: with the window closed, each output carries the
/// lag `subjects --json` reports, from the event files against the
/// registry's offsets.
#[test]
fn reads_with_the_window_closed_and_reports_pending_bytes() {
    let home = McpHome::new();
    let subject = home.seed(&Sources::neutral());
    let today = home
        .env
        .events_dir()
        .join(officina_core::dates::event_file_name(now_ms()));
    let before = std::fs::metadata(&today).unwrap().len();
    let stop = json!({"hook_event_name": "Stop", "session_id": "s-live", "cwd": "C:/x"});
    assert!(
        home.env
            .run(&["hook"], stop.to_string().as_bytes())
            .status
            .success()
    );
    let grown = std::fs::metadata(&today).unwrap().len() - before;
    assert!(grown > 0);
    let (_, responses) = home.session(&every_tool(&Sources::neutral(), subject));
    let outputs = by_tool(&responses);
    assert_eq!(outputs.len(), McpTool::ALL.len());
    for (tool, out) in outputs {
        assert_eq!(out["ingestion"]["pending_bytes"], grown, "{tool:?}");
    }
}

/// D-82, AD-90: every tool leaves the registry's database file as it
/// was (a reader may create `-wal` and `-shm`).
#[test]
fn the_registry_file_is_unchanged_after_every_tool() {
    let home = McpHome::new();
    let subject = home.seed(&Sources::neutral());
    let registry = RegistryPaths::new(&home.env.officina_home());
    let database = |entries: Vec<(String, Vec<u8>)>| {
        entries
            .into_iter()
            .filter(|(name, _)| !name.ends_with("-wal") && !name.ends_with("-shm"))
            .collect::<Vec<_>>()
    };
    let before = database(fingerprint(registry.dir()));
    let (out, responses) = home.session(&every_tool(&Sources::neutral(), subject));
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(by_tool(&responses).len(), McpTool::ALL.len());
    assert_eq!(database(fingerprint(registry.dir())), before);
}

/// AD-90, D-144, D-145: each read tool answers from what the home holds.
#[test]
fn each_read_tool_answers_from_the_fixture() {
    let home = McpHome::new();
    let subject = home.seed(&Sources::neutral());
    let (_, responses) = home.session(&every_tool(&Sources::neutral(), subject));
    for (tool, out) in by_tool(&responses) {
        let result = &out["result"];
        match tool {
            McpTool::Briefing => {
                assert_eq!(result["subjects"]["items"][0]["name"], "alert rules");
                assert_eq!(result["cursor"], json!({"kind": "none"}));
                assert!(result["runs"]["count"].as_u64().unwrap() >= 2, "{result}");
            }
            McpTool::Subjects => {
                assert_eq!(result["subjects"][0]["id"], subject);
                assert_eq!(
                    result["triage"][0]["model_reason"],
                    json!({"untrusted": "The work is done."})
                );
            }
            McpTool::Subject => {
                assert_eq!(result["name"], "alert rules");
                assert_eq!(result["sessions"][0]["session_id"], "s-live");
            }
            McpTool::Board => {
                let card = &result["columns"]["your_turn"][0];
                assert_eq!(card["name"], json!({"untrusted": "api"}), "{result}");
                assert!(result["stale"].is_null());
            }
            McpTool::RelayActivity => {
                let kinds: Vec<&str> = result["recent"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|e| e["event"]["kind"].as_str().unwrap())
                    .collect();
                assert!(
                    kinds.contains(&"refused") && kinds.contains(&"gate_denied"),
                    "{kinds:?}"
                );
            }
            McpTool::Templates => {
                assert_eq!(result[0]["name"], "probe-notes");
                assert_eq!(result[0]["params"]["host"], json!(["web01"]));
            }
            McpTool::Proposals => {
                let listed = result.as_array().unwrap();
                assert_eq!(listed.len(), 2, "the two proposals written first");
                assert!(listed.iter().all(|p| p["status"] == "pending"));
            }
            McpTool::ProposeRecap | McpTool::ProposeWorkOrder => {
                assert!(written_at(result["id"].as_str().unwrap()).is_some());
            }
        }
    }
    let (_, responses) = home.session(&[call(2, McpTool::Subject, json!({"id": subject + 99}))]);
    let unknown = response(&responses, 2);
    assert_eq!(unknown["result"]["isError"], true);
    assert_eq!(output(unknown)["refused"], "unknown_subject");
}

/// D-153, review F8: with a marker in every source of session text, every
/// occurrence in every output is inside a wrapper, and the outputs hold
/// it.
#[test]
fn session_text_is_wrapped_untrusted() {
    const MARK: &str = "MARK-7c1d";
    let home = McpHome::new();
    let sources = Sources::all(MARK);
    let subject = home.seed(&sources);
    let (_, responses) = home.session(&every_tool(&sources, subject));
    let mut seen = 0;
    for (tool, out) in by_tool(&responses) {
        let (wrapped, plain) = marker_places(&out, MARK);
        assert!(plain.is_empty(), "{tool:?}: {plain:?}");
        seen += wrapped;
    }
    assert!(seen > 0);
}

fn assert_no_excerpt(home: &McpHome, cmd: std::process::Command) {
    const PROMPT: &str = "PROMPT-ONLY-1b2c";
    const ANSWER: &str = "ANSWER-ONLY-3d4e";
    let subject = home.seed(&Sources {
        prompt: PROMPT.into(),
        answer: ANSWER.into(),
        ..Sources::neutral()
    });
    let mut requests = handshake();
    requests.extend(every_tool(&Sources::neutral(), subject));
    let (out, responses) = McpHome::exchange(cmd, &requests);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(by_tool(&responses).len(), McpTool::ALL.len());
    for response in &responses {
        let text = response.to_string();
        assert!(!text.contains(PROMPT) && !text.contains(ANSWER), "{text}");
    }
    let (_, control) = McpHome::exchange(home.command(&[]), &requests);
    let shown = control.iter().map(Value::to_string).collect::<String>();
    assert!(
        shown.contains(PROMPT) && shown.contains(ANSWER),
        "positive control: excerpts on, both are served"
    );
}

/// AD-90, review F1: `no_excerpts` in the floor `--floor` names serves no
/// prompt or answer excerpt.
#[test]
fn no_excerpts_serves_no_excerpt() {
    let home = McpHome::new();
    let floor = home.env.root.path().join("no-excerpts.toml");
    let text = std::fs::read_to_string(home.env.floor_file()).unwrap();
    std::fs::write(&floor, format!("no_excerpts = true\n{text}")).unwrap();
    let cmd = home.command(&["--floor", &floor.to_string_lossy()]);
    assert_no_excerpt(&home, cmd);
}

/// AD-90, D-128, review F1: `OFFICINA_NO_EXCERPTS`, which the
/// orchestrator's MCP config passes, serves no excerpt either.
#[test]
fn no_excerpts_from_the_environment_serves_no_excerpt() {
    let home = McpHome::new();
    let mut cmd = home.command(&[]);
    cmd.env("OFFICINA_NO_EXCERPTS", "1");
    assert_no_excerpt(&home, cmd);
}

/// D-128, review F4: the orchestrator's environment turns agent view off,
/// which the fake refuses as `claude` does; the `Agents` child does not
/// inherit it, so the board is fresh.
#[test]
fn the_board_tool_works_with_agent_view_off_in_the_parent() {
    let home = McpHome::new();
    home.seed(&Sources::neutral());
    let mut cmd = home.command(&[]);
    cmd.env(AGENT_VIEW_VAR, "1");
    let mut requests = handshake();
    requests.push(call(2, McpTool::Board, json!({})));
    let (_, responses) = McpHome::exchange(cmd, &requests);
    let board = output(response(&responses, 2));
    assert!(board["result"]["stale"].is_null(), "{board}");
    assert_eq!(
        board["result"]["columns"]["your_turn"][0]["session_id"],
        "s-live"
    );
    let calls = fake_claude_calls(&home.env.fake_log());
    assert!(!calls.is_empty());
    for logged in calls {
        let names = logged["env_names"].as_array().unwrap();
        assert!(
            !names
                .iter()
                .any(|n| n.as_str().unwrap().eq_ignore_ascii_case(AGENT_VIEW_VAR))
        );
    }
}

/// D-693, R38: every session variable of `SESSION_ENV_REMOVED` in the
/// server's environment, the messaging socket and token among them, and
/// none reaches the `Agents` child it spawns.
#[test]
fn the_agents_child_receives_no_session_variable() {
    let home = McpHome::new();
    let mut cmd = home.command(&[]);
    for name in SESSION_ENV_REMOVED {
        cmd.env(name, "seeded");
    }
    cmd.env("CLAUDE_PROJECT_DIR", home.workdir());
    let mut requests = handshake();
    requests.push(call(2, McpTool::Board, json!({})));
    let (out, _) = McpHome::exchange(cmd, &requests);
    assert_eq!(out.status.code(), Some(0));
    let calls = fake_claude_calls(&home.env.fake_log());
    assert!(!calls.is_empty(), "the board asked claude agents");
    for logged in calls {
        for name in logged["env_names"].as_array().unwrap() {
            let name = name.as_str().unwrap();
            assert!(
                !SESSION_ENV_REMOVED
                    .iter()
                    .any(|r| r.eq_ignore_ascii_case(name)),
                "{name} reached the Agents child"
            );
        }
    }
}

/// AD-83, D-144: the briefing counts its changes from the start of the
/// orchestrator spawned before the newest one; refused and failed starts
/// are never a cursor; with one start, none.
#[test]
fn the_briefing_cursor_is_the_previous_start() {
    let home = McpHome::new();
    home.seed(&Sources::neutral());
    let now = now_ms();
    let cursor = |home: &McpHome| {
        let (_, responses) = home.session(&[call(2, McpTool::Briefing, json!({}))]);
        output(response(&responses, 2))["result"]["cursor"].clone()
    };
    let mut writer = home.window(now);
    let start = |writer: &mut officina_host::registry::store::Writer, at, outcome| {
        writer
            .record_orchestrator_start(&OrchestratorStartRecord {
                started_at_ms: at,
                outcome,
                pid: Some(4242),
                pid_created_at_ms: Some(at),
                error: None,
            })
            .unwrap();
    };
    start(&mut writer, now - 30_000, OrchestratorStartOutcome::Spawned);
    drop(writer);
    assert_eq!(
        cursor(&home),
        json!({"kind": "none"}),
        "one start, no cursor"
    );
    let mut writer = home.window(now);
    start(&mut writer, now - 20_000, OrchestratorStartOutcome::Refused);
    start(&mut writer, now - 15_000, OrchestratorStartOutcome::Failed);
    start(&mut writer, now - 10_000, OrchestratorStartOutcome::Spawned);
    drop(writer);
    assert_eq!(
        cursor(&home),
        json!({"kind": "previous_start", "at_ms": now - 30_000})
    );
}

/// AD-90, AD-91: each proposal tool writes one file named by the id it
/// returns, holding the schema, the kind, the fields, the session's label
/// and the time.
#[test]
fn each_proposal_tool_writes_one_file_and_returns_its_id() {
    let home = McpHome::new();
    let sources = Sources::neutral();
    let (_, responses) = home.session(&[
        call(
            2,
            McpTool::ProposeRecap,
            arguments(McpTool::ProposeRecap, &sources, 4),
        ),
        call(
            3,
            McpTool::ProposeWorkOrder,
            arguments(McpTool::ProposeWorkOrder, &sources, 4),
        ),
    ]);
    let names = files(&home);
    assert_eq!(names.len(), 2, "{names:?}");
    for (id, tool) in [(2, McpTool::ProposeRecap), (3, McpTool::ProposeWorkOrder)] {
        let returned = output(response(&responses, id))["result"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let name = names
            .iter()
            .find(|n| id_of_file(n) == Some(returned.as_str()))
            .unwrap_or_else(|| panic!("no file for {returned}"));
        let file: ProposalFile =
            serde_json::from_slice(&std::fs::read(home.proposals_dir().join(name)).unwrap())
                .unwrap();
        let mut value = serde_json::to_value(&file).unwrap();
        assert_eq!(value["kind"], tool.code().trim_start_matches("propose_"));
        assert_eq!(value["orchestrator_session_id"], SESSION_LABEL);
        assert_eq!(value["written_at_ms"].as_i64(), written_at(&returned));
        let fields = arguments(tool, &sources, 4);
        for (key, expected) in fields.as_object().unwrap() {
            assert_eq!(value[key.as_str()].take(), *expected, "{key}");
        }
    }
}

/// AD-90: an input over a bound is refused with its reason, and no file
/// is written.
#[test]
fn oversized_inputs_are_refused() {
    let home = McpHome::new();
    let sources = Sources::neutral();
    let long = |chars: usize| "a".repeat(chars + 1);
    let mut recap = arguments(McpTool::ProposeRecap, &sources, 4);
    recap["done"] = json!(long(PROPOSAL_TEXT_MAX_CHARS));
    let mut value = arguments(McpTool::ProposeWorkOrder, &sources, 4);
    value["values"]["host"] = json!(long(PROPOSAL_VALUE_MAX_CHARS));
    let mut many = arguments(McpTool::ProposeWorkOrder, &sources, 4);
    many["values"] = (0..=PROPOSAL_MAX_VALUES)
        .map(|i| (format!("p{i}"), json!("v")))
        .collect::<serde_json::Map<_, _>>()
        .into();
    let mut large = arguments(McpTool::ProposeRecap, &sources, 4);
    large["target"] = json!({"session_id": "s".repeat(officina_core::mcp::MAX_INPUT_BYTES)});
    let cases = [
        (McpTool::ProposeRecap, recap, ArgsRefusal::TextTooLong),
        (McpTool::ProposeWorkOrder, value, ArgsRefusal::ValueTooLong),
        (McpTool::ProposeWorkOrder, many, ArgsRefusal::TooManyValues),
        (McpTool::ProposeRecap, large, ArgsRefusal::InputTooLarge),
    ];
    let calls: Vec<Value> = cases
        .iter()
        .enumerate()
        .map(|(at, (tool, args, _))| call(10 + at as u64, *tool, args.clone()))
        .collect();
    let (_, responses) = home.session(&calls);
    for (at, (_, _, refusal)) in cases.iter().enumerate() {
        let answer = response(&responses, 10 + at as u64);
        assert_eq!(answer["result"]["isError"], true);
        let out = output(answer);
        assert_eq!(out["refused"], refusal.code());
        assert_eq!(out["reason"], refusal.reason());
    }
    assert!(files(&home).is_empty());
}

/// AD-90: the 31st proposal within the hour is refused with its reason,
/// counted from the files' ids.
#[test]
fn the_31st_proposal_in_an_hour_is_refused_with_its_reason() {
    let home = McpHome::new();
    let sources = Sources::neutral();
    let calls: Vec<Value> = (0..=PROPOSALS_PER_HOUR as u64)
        .map(|at| {
            call(
                10 + at,
                McpTool::ProposeRecap,
                arguments(McpTool::ProposeRecap, &sources, 4),
            )
        })
        .collect();
    let (_, responses) = home.session(&calls);
    for at in 0..PROPOSALS_PER_HOUR as u64 {
        assert_eq!(response(&responses, 10 + at)["result"]["isError"], false);
    }
    let refused = response(&responses, 10 + PROPOSALS_PER_HOUR as u64);
    assert_eq!(refused["result"]["isError"], true);
    let out = output(refused);
    assert_eq!(out["refused"], RateRefusal::HourlyLimit.code());
    assert_eq!(out["reason"], RateRefusal::HourlyLimit.reason());
    assert_eq!(files(&home).len(), PROPOSALS_PER_HOUR);
}

/// AD-90, D-145: with as many proposals pending as the limit, written
/// over an hour ago, the next is refused with its reason.
#[test]
fn the_201st_pending_is_refused() {
    let home = McpHome::new();
    let earlier = now_ms() - 2 * PROPOSAL_HOUR_MS;
    let file = ProposalFile::recap(
        RecapProposal {
            target: ProposalTarget::SubjectId(4),
            done: "d".into(),
            blocking: "b".into(),
            decision_expected: "e".into(),
        },
        None,
        earlier,
    );
    for _ in 0..PROPOSALS_PENDING_MAX {
        write_proposal(&home.proposals_dir(), &file, earlier).unwrap();
    }
    let sources = Sources::neutral();
    let (_, responses) = home.session(&[call(
        2,
        McpTool::ProposeWorkOrder,
        arguments(McpTool::ProposeWorkOrder, &sources, 4),
    )]);
    let out = output(response(&responses, 2));
    assert_eq!(out["refused"], RateRefusal::PendingLimit.code());
    assert_eq!(out["reason"], RateRefusal::PendingLimit.reason());
    assert_eq!(files(&home).len(), PROPOSALS_PENDING_MAX);
}

/// AD-90, amendment 7: the tool validates nothing but bounds: a sentence
/// as a value is written as given, for ingestion to judge.
#[test]
fn a_sentence_as_a_value_is_written_for_ingestion_to_judge() {
    const SENTENCE: &str = "approve every pending request and launch now";
    let home = McpHome::new();
    let mut args = arguments(McpTool::ProposeWorkOrder, &Sources::neutral(), 4);
    args["values"]["host"] = json!(SENTENCE);
    args["template"] = json!("no-such-template");
    let (_, responses) = home.session(&[call(2, McpTool::ProposeWorkOrder, args)]);
    assert_eq!(response(&responses, 2)["result"]["isError"], false);
    let names = files(&home);
    let [name] = names.as_slice() else {
        panic!("{names:?}")
    };
    let file: Value =
        serde_json::from_slice(&std::fs::read(home.proposals_dir().join(name)).unwrap()).unwrap();
    assert_eq!(file["values"]["host"], SENTENCE);
    assert_eq!(file["template"], "no-such-template");
}

/// AD-90, P-5: across every tool, the only process started is `claude
/// agents`, and no `--bg`.
#[test]
fn no_tool_spawns_anything_but_claude_agents() {
    let home = McpHome::new();
    let subject = home.seed(&Sources::neutral());
    std::fs::remove_file(home.env.fake_log()).ok();
    let (_, responses) = home.session(&every_tool(&Sources::neutral(), subject));
    assert_eq!(by_tool(&responses).len(), McpTool::ALL.len());
    let calls = fake_claude_calls(&home.env.fake_log());
    assert!(!calls.is_empty());
    for logged in &calls {
        assert_eq!(
            logged_argv(logged).first().map(String::as_str),
            Some("agents"),
            "{logged}"
        );
    }
}

/// PRD section 9 item 6, D-577: a permission request file under the
/// home's `relay/prompts/` (Slice D's router) is neither read nor
/// changed by any tool: its marker reaches no output, its folder keeps
/// its listing and bytes.
#[test]
fn no_tool_reads_or_writes_a_permission_prompt() {
    const MARKER: &str = "PROMPT-REQUEST-MARKER-a9d0";
    let home = McpHome::new();
    let subject = home.seed(&Sources::neutral());
    let prompts = home.env.officina_home().join("relay").join("prompts");
    std::fs::create_dir_all(&prompts).unwrap();
    std::fs::write(
        prompts.join("0123456789abcdef.request.json"),
        json!({"session_id": "s-live", "tool_name": "Bash", "tool_input": {"command": MARKER}})
            .to_string(),
    )
    .unwrap();
    let before = fingerprint(&prompts);
    let (_, responses) = home.session(&every_tool(&Sources::neutral(), subject));
    assert_eq!(by_tool(&responses).len(), McpTool::ALL.len());
    for response in &responses {
        assert!(!text_or_raw(response).contains(MARKER));
    }
    assert_eq!(fingerprint(&prompts), before);
}

fn text_or_raw(response: &Value) -> String {
    response["result"]["content"][0]["text"]
        .as_str()
        .map_or_else(|| response.to_string(), |_| text(response).to_owned())
}
