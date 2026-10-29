//! The `officina mcp` harness of the CLI tests (story 5.15): a home whose
//! orchestrator folder exists, a scripted client speaking E11's handshake
//! to the real binary, and a seeding of every source of session text an
//! output can carry, each source's text chosen by the test.

use std::path::PathBuf;
use std::process::{Command, Output};

use officina_core::mcp::{McpTool, UNTRUSTED_NOTE};
use officina_core::orders::{ORCHESTRATOR_DIR, ORCHESTRATOR_WORKDIR, PROPOSALS_DIR};
use officina_core::registry::confirm::SubjectOrigin;
use officina_core::registry::dry_run::Tally;
use officina_core::registry::triage_call::CallAnswer;
use officina_core::registry::types::{ModelVerdict, TriageOutcome};
use officina_host::registry::ingest::IngestContext;
use officina_host::registry::store::{Writer, open_writer};
use officina_host::registry::triage::{CallResult, Triage};
use serde_json::{Value, json};

use super::relay::slashed as slashed_path;
use super::{TestEnv, now_ms, run_with};

/// The orchestrator session's id the server is started with: a label.
pub const SESSION_LABEL: &str = "00000000-0000-0000-0000-0000000000aa";

/// A floor with one project, an action and a template, `{lab}` its folder.
const FLOOR: &str = r#"schema = 2
[[project]]
name = "lab"
path = "{lab}"
actions = ["probe"]
[[template]]
name = "probe-notes"
description = "Write the probe notes"
projects = ["lab"]
prompt = "Write the notes for {host}."
[template.params.host]
values = ["web01"]
[[action]]
name = "probe"
class = "free"
program = "{tool}"
args = ["{host}"]
[action.params.host]
values = ["web01"]
"#;

/// The text of each source of session text an output may carry; a test
/// puts its marker in one or all of them.
#[derive(Debug, Clone)]
pub struct Sources {
    /// The live session's name in `claude agents`.
    pub card_name: String,
    /// A folder under the project the live session runs in, as `claude
    /// agents` lists it; empty for the project's own folder.
    pub card_cwd: String,
    /// Its prompt, which the board and the registry's excerpts show.
    pub prompt: String,
    /// Its last answer: head and tail on the card, and in the registry.
    pub answer: String,
    /// The triage model's reason for the ended session's row.
    pub model_reason: String,
    /// A command the gate denied, its excerpt journaled.
    pub command: String,
    /// The name and the value of a parameter `officina run` refused.
    pub param_name: String,
    pub param_value: String,
    /// An action `officina run` refused as unknown.
    pub action: String,
    /// A proposal's texts and values, written through the tools.
    pub proposal_text: String,
    pub proposal_value: String,
}

impl Sources {
    /// Every source neutral.
    pub fn neutral() -> Sources {
        Sources {
            card_name: "api".into(),
            card_cwd: String::new(),
            prompt: "tighten the alert rules".into(),
            answer: "Rules tightened.".into(),
            model_reason: "The work is done.".into(),
            command: "ssh host01".into(),
            param_name: "region".into(),
            param_value: "eu".into(),
            action: "deploy".into(),
            proposal_text: "Tests pass.".into(),
            proposal_value: "web01".into(),
        }
    }

    /// Every source holding `marker`.
    pub fn all(marker: &str) -> Sources {
        SOURCES.iter().fold(Sources::neutral(), |sources, source| {
            sources.with(source, marker)
        })
    }

    /// These sources with `source`, one of [`SOURCES`], holding `marker`:
    /// in an answer's head and tail, after a letter where a name could
    /// otherwise start with an option's dash.
    pub fn with(mut self, source: &str, marker: &str) -> Sources {
        let field = match source {
            "card_name" => &mut self.card_name,
            "card_cwd" => &mut self.card_cwd,
            "prompt" => &mut self.prompt,
            "answer" => &mut self.answer,
            "model_reason" => &mut self.model_reason,
            "command" => &mut self.command,
            "param_name" => &mut self.param_name,
            "param_value" => &mut self.param_value,
            "action" => &mut self.action,
            "proposal_text" => &mut self.proposal_text,
            "proposal_value" => &mut self.proposal_value,
            other => panic!("unknown source {other}"),
        };
        *field = match source {
            "answer" => format!("{marker} {} {marker}", "x".repeat(4_000)),
            "command" => format!("ssh host01 {marker}"),
            _ => format!("s{marker}"),
        };
        self
    }
}

/// Every source of session text a test can seed (story 5.15 AC 8).
pub const SOURCES: &[&str] = &[
    "card_name",
    "card_cwd",
    "prompt",
    "answer",
    "model_reason",
    "command",
    "param_name",
    "param_value",
    "action",
    "proposal_text",
    "proposal_value",
];

/// A home with the orchestrator's folder and the floor approved.
pub struct McpHome {
    pub env: TestEnv,
}

impl McpHome {
    pub fn new() -> McpHome {
        McpHome::with_floor_extra("")
    }

    /// The same with `extra` prepended to the floor (a top-level key).
    pub fn with_floor_extra(extra: &str) -> McpHome {
        let env = TestEnv::new();
        std::fs::create_dir_all(
            env.officina_home()
                .join(ORCHESTRATOR_DIR)
                .join(ORCHESTRATOR_WORKDIR),
        )
        .unwrap();
        let home = McpHome { env };
        std::fs::create_dir_all(home.lab()).unwrap();
        home.env.write_floor(&format!(
            "{extra}{}",
            FLOOR.replace("{lab}", &slashed_path(&home.lab()))
        ));
        home.env.approve();
        home.env.write_agents("[]");
        home
    }

    pub fn lab(&self) -> PathBuf {
        self.env.root.path().join("work").join("Lab")
    }

    pub fn workdir(&self) -> PathBuf {
        self.env
            .officina_home()
            .join(ORCHESTRATOR_DIR)
            .join(ORCHESTRATOR_WORKDIR)
    }

    pub fn proposals_dir(&self) -> PathBuf {
        self.env
            .officina_home()
            .join(ORCHESTRATOR_DIR)
            .join(PROPOSALS_DIR)
    }

    /// `officina mcp <args>` as the orchestrator's MCP config starts it.
    pub fn command(&self, args: &[&str]) -> Command {
        let mut all = vec!["mcp"];
        all.extend_from_slice(args);
        let mut cmd = self.env.officina(&all);
        cmd.env("CLAUDE_PROJECT_DIR", self.workdir())
            .env("CLAUDE_CODE_SESSION_ID", SESSION_LABEL)
            .current_dir(self.workdir());
        cmd
    }

    /// Runs `cmd` with `requests`, one a line, and returns the process's
    /// output and each response line.
    pub fn exchange(cmd: Command, requests: &[Value]) -> (Output, Vec<Value>) {
        let input: String = requests.iter().map(|r| format!("{r}\n")).collect();
        let out = run_with(cmd, input.as_bytes());
        let responses = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|line| serde_json::from_str(line).unwrap_or_else(|_| panic!("{line}")))
            .collect();
        (out, responses)
    }

    /// E11's handshake, then `calls`, on the default command.
    pub fn session(&self, calls: &[Value]) -> (Output, Vec<Value>) {
        let mut requests = handshake();
        requests.extend_from_slice(calls);
        McpHome::exchange(self.command(&[]), &requests)
    }

    /// The window's writer on this home, after ingesting its event files.
    pub fn window(&self, now: i64) -> Writer {
        let mut writer = open_writer(&self.env.officina_home(), &[], now)
            .writer()
            .unwrap();
        let events = self.env.events_dir();
        let home = self.env.officina_home();
        writer
            .ingest(
                &IngestContext {
                    events_dir: &events,
                    officina_home: &home,
                    projects: &[],
                    no_excerpts: false,
                },
                now,
            )
            .unwrap();
        writer
    }

    fn hook(&self, payload: Value) {
        assert!(
            self.env
                .run(&["hook"], payload.to_string().as_bytes())
                .status
                .success()
        );
    }

    /// `officina run <args>` from the lab's folder, refused.
    fn refused_run(&self, args: &[&str]) {
        let mut all = vec!["run"];
        all.extend_from_slice(args);
        let mut cmd = self.env.officina(&all);
        cmd.current_dir(self.lab());
        assert_eq!(run_with(cmd, b"").status.code(), Some(126), "{args:?}");
    }

    /// Seeds every source of `sources` with the window closed: a live
    /// session filed under a subject, an ended one whose row the triage
    /// model answered, a gate deny and two refused runs. Returns the
    /// subject's id.
    pub fn seed(&self, sources: &Sources) -> i64 {
        let now = now_ms();
        let lab = slashed_path(&self.lab());
        self.hook(
            json!({"hook_event_name": "UserPromptSubmit", "session_id": "s-live",
            "cwd": lab, "prompt": sources.prompt}),
        );
        self.hook(
            json!({"hook_event_name": "Stop", "session_id": "s-live", "cwd": lab,
            "last_assistant_message": sources.answer}),
        );
        for (event, extra) in [
            ("UserPromptSubmit", json!({"prompt": "look around"})),
            ("Stop", json!({"last_assistant_message": "Looked."})),
            ("SessionEnd", json!({"reason": "prompt_input_exit"})),
        ] {
            let mut payload =
                json!({"hook_event_name": event, "session_id": "s-ended", "cwd": lab});
            payload
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            self.hook(payload);
        }
        self.env.write_agents(
            &json!([{"pid": 4100, "kind": "interactive", "startedAt": now - 60_000,
                "cwd": if sources.card_cwd.is_empty() { lab.clone() } else { format!("{lab}/{}", sources.card_cwd) },
                "sessionId": "s-live", "name": sources.card_name, "status": "idle"}])
            .to_string(),
        );
        self.hook(json!({"hook_event_name": "PreToolUse", "tool_name": "Bash",
            "tool_input": {"command": sources.command}, "cwd": lab, "session_id": "s-live"}));
        self.refused_run(&[&sources.action]);
        self.refused_run(&[
            "probe",
            &format!("host={}", sources.param_value),
            &format!("{}=web01", sources.param_name),
        ]);
        let mut writer = self.window(now);
        let subject = writer
            .create_subject(
                "alert rules",
                SubjectOrigin::Sessions(vec!["s-live".into()]),
                now,
            )
            .unwrap();
        writer
            .record_evaluation(
                &Tally {
                    cases: 30,
                    model_correct: 20,
                    default_correct: 10,
                    ..Tally::default()
                },
                now - 60_000,
            )
            .unwrap();
        writer.set_triage_model(true, now).unwrap().unwrap();
        let mut triage = Triage::default();
        let next = triage
            .next_call(&mut writer, now)
            .unwrap()
            .expect("the ended session's row is queued for the model");
        let result = CallResult {
            purpose: next.purpose,
            case_key: next.case_key.clone(),
            input_hash: next.case.input_hash(),
            started_at_ms: now,
            duration_ms: 1_000,
            outcome: TriageOutcome::Answer,
            refusal: None,
            answer: Some(CallAnswer {
                verdict: ModelVerdict::Finished,
                reason: sources.model_reason.clone(),
            }),
            usage: Default::default(),
            detail: None,
        };
        triage
            .take_result(&mut writer, &result, now + 1_000)
            .unwrap();
        subject
    }
}

/// `server/discover` first, then `initialize` and its notification (E11).
pub fn handshake() -> Vec<Value> {
    vec![
        json!({"jsonrpc": "2.0", "id": 0, "method": "server/discover",
            "params": {"_meta": {"protocol": "2026-07-28"}}}),
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2025-11-25", "capabilities": {},
                "clientInfo": {"name": "claude-code", "version": "2.1.312"}}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    ]
}

/// A `tools/call` of `tool` with `arguments`.
pub fn call(id: u64, tool: McpTool, arguments: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": {"name": tool.code(), "arguments": arguments,
            "_meta": {"claudecode/toolUseId": format!("toolu_{id}"), "progressToken": id}}})
}

/// Valid arguments for `tool`, the proposal tools' texts and values from
/// `sources`, `subject` the subject asked for.
pub fn arguments(tool: McpTool, sources: &Sources, subject: i64) -> Value {
    match tool {
        McpTool::Briefing | McpTool::Subjects | McpTool::Board | McpTool::Templates => json!({}),
        McpTool::Subject => json!({"id": subject}),
        McpTool::RelayActivity => json!({"limit": 50}),
        McpTool::Proposals => json!({}),
        McpTool::ProposeRecap => json!({"target": {"subject_id": subject},
            "done": sources.proposal_text, "blocking": sources.proposal_text,
            "decision_expected": sources.proposal_text}),
        McpTool::ProposeWorkOrder => json!({"template": "probe-notes", "project": "lab",
            "values": {"host": sources.proposal_value},
            "subject": {"new": sources.proposal_text}, "rationale": sources.proposal_text}),
        McpTool::ProposeAnswer => json!({"session_id": "00000000-0000-0000-0000-000000000001",
            "text": sources.proposal_text, "rationale": sources.proposal_text}),
    }
}

/// A call of every tool, the proposal tools first, so `proposals` reads
/// what they wrote; ids from 100.
pub fn every_tool(sources: &Sources, subject: i64) -> Vec<Value> {
    let mut tools: Vec<McpTool> = McpTool::ALL.to_vec();
    tools.sort_by_key(|tool| tool.reads_only());
    tools
        .iter()
        .enumerate()
        .map(|(at, tool)| call(100 + at as u64, *tool, arguments(*tool, sources, subject)))
        .collect()
}

/// A tool response's text.
pub fn text(response: &Value) -> &str {
    response["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no text in {response}"))
}

/// A tool response's JSON, below the note it starts with.
pub fn output(response: &Value) -> Value {
    let (note, json) = text(response)
        .split_once('\n')
        .unwrap_or_else(|| panic!("{response}"));
    assert_eq!(note, UNTRUSTED_NOTE);
    serde_json::from_str(json).unwrap()
}

/// The response to request `id`.
pub fn response(responses: &[Value], id: u64) -> &Value {
    responses
        .iter()
        .find(|r| r["id"] == id)
        .unwrap_or_else(|| panic!("no response {id} in {responses:?}"))
}

/// Where `marker` sits in `value`: how many times inside an
/// `{"untrusted": ...}` wrapper, and the paths of every string or key
/// holding it outside one (D-147, D-153).
pub fn marker_places(value: &Value, marker: &str) -> (usize, Vec<String>) {
    fn walk(value: &Value, marker: &str, path: &str, wrapped: &mut usize, plain: &mut Vec<String>) {
        match value {
            Value::Object(map) if map.len() == 1 && map.contains_key("untrusted") => {
                *wrapped += occurrences(&map["untrusted"], marker);
            }
            Value::Object(map) => {
                for (key, item) in map {
                    if key.contains(marker) {
                        plain.push(format!("{path} (key)"));
                    }
                    walk(item, marker, &format!("{path}/{key}"), wrapped, plain);
                }
            }
            Value::Array(items) => {
                for (at, item) in items.iter().enumerate() {
                    walk(item, marker, &format!("{path}/{at}"), wrapped, plain);
                }
            }
            Value::String(text) if text.contains(marker) => plain.push(path.to_owned()),
            _ => {}
        }
    }
    fn occurrences(value: &Value, marker: &str) -> usize {
        match value {
            Value::String(text) => text.matches(marker).count(),
            Value::Array(items) => items.iter().map(|v| occurrences(v, marker)).sum(),
            Value::Object(map) => map
                .iter()
                .map(|(key, v)| key.matches(marker).count() + occurrences(v, marker))
                .sum(),
            _ => 0,
        }
    }
    let (mut wrapped, mut plain) = (0, Vec::new());
    walk(value, marker, "", &mut wrapped, &mut plain);
    (wrapped, plain)
}
