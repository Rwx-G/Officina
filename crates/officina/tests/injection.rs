//! The injection battery (AD-95, PRD section 7's Gesture row, D-147):
//! hostile inputs at each entry point of the launch path, replayed from
//! `tests/vectors/injection.json` through the functions the window's
//! commands call, with a copy of the fake claude counting the `--bg`
//! spawns it logs (D-129). Every case expects its refusal or outcome, no
//! spawn before the first click, and its count of spawns at the end; a
//! launch that started ran the card's argument vector, equal to the one
//! `spawn::work_order` rebuilds from the approved catalogue with the card's
//! `seq`.
//!
//! A case that names the mutant it guards (`guards`) is the one CI's
//! `mutants` job expects to fail with that mutant on (D-148): the failure
//! message names it, `guarded by <feature>`.
//!
//! Sections: `card_launch` (story 5.7), `review_excerpt` (story 5.13),
//! `auto_approved_run` (story 5.14), `mcp_tool` and `mcp_output` (story 5.15),
//! `proposal_file` (story 5.17), whose answer cases and the `mcp_tool` ones
//! of `propose_answer` story 5.21 adds, counting `--resume` spawns.

mod common;

use std::path::PathBuf;

use common::TestEnv;
use common::orders::OrdersHome;
use officina_core::orders::card::{
    CardAnswer, CardChange, CardRequest, LaunchRefusal, WorkOrderCard,
};
use officina_core::orders::work_order::{SubjectChoice, WorkOrder, WorkOrderRefusal};
use officina_core::registry::confirm::AuthorAction;
use officina_core::registry::types::WorkOrderSource;
use officina_core::relay::catalogue::ApprovedCatalogue;
use officina_core::relay::params::TEMPLATE_VALUE_MAX_CHARS;
use officina_core::relay::template::{
    ApprovedTemplate, MAX_PROMPT_CHARS, ParamName, ProjectName, TemplateName,
};
use officina_host::registry::orders::LaunchRecord;
use officina_host::registry::store::open_writer;
use officina_host::relay::home::load_relay_floor;
use officina_host::watch::{AUTHOR_WAIT, AuthorCommand, ask};
use serde_json::Value;

const VECTORS: &str = include_str!("../../../tests/vectors/injection.json");

/// The manifests and the constant the mutants live in (D-148).
const CORE_MANIFEST: &str = include_str!("../../officina-core/Cargo.toml");
const HOST_MANIFEST: &str = include_str!("../../officina-host/Cargo.toml");
const CLI_MANIFEST: &str = include_str!("../Cargo.toml");
const CORE_LIB: &str = include_str!("../../officina-core/src/lib.rs");

/// The entry points the battery covers, one section each (D-147), in the
/// order the vector set's keys read back.
const SECTIONS: &[&str] = &[
    "auto_approved_run",
    "card_launch",
    "mcp_output",
    "mcp_tool",
    "proposal_file",
    "review_excerpt",
];

/// The attack families of PRD section 7's Gesture and Auto fail-closed
/// rows the sections hold, declared here once (D-147), and what a case of
/// none of them is.
const FAMILIES: &[&str] = &[
    "tree_change",
    "argument_fragment",
    "option_string",
    "settings",
    "command_prefix",
    "bidi_invisible",
    "oversized",
    "unknown_template",
    "sentence",
    "outside_list",
    "launch_now",
    "stale_card",
    "edited_snapshot",
    "launch_conditions",
    "markup",
    "unknown_reference",
    "malformed",
    "rate_limit",
    "control",
];

/// Stand for a value one character over a limit, built from the limit
/// itself so the vector set types no oversized string (D-655).
const OVER_VALUE_LIMIT: &str = "<over TEMPLATE_VALUE_MAX_CHARS>";
const OVER_PROMPT_LIMIT: &str = "<over MAX_PROMPT_CHARS>";

/// How many cases run at once, each with its own home and collector.
const PARALLEL_CASES: usize = 6;

fn load() -> Value {
    let set: Value = serde_json::from_str(VECTORS).expect("the injection battery parses");
    assert_eq!(set["schema"], 1);
    set
}

fn cases(set: &Value, section: &str) -> Vec<Value> {
    set[section].as_array().cloned().unwrap_or_default()
}

/// `value` with each stand-in replaced by the text it stands for.
fn with_values(value: &Value) -> Value {
    match value {
        Value::String(text) if text == OVER_VALUE_LIMIT => {
            Value::from("a".repeat(TEMPLATE_VALUE_MAX_CHARS + 1))
        }
        Value::String(text) if text == OVER_PROMPT_LIMIT => {
            Value::from("a".repeat(MAX_PROMPT_CHARS + 1))
        }
        Value::Array(items) => Value::Array(items.iter().map(with_values).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), with_values(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The mutant features officina-core declares (D-148).
fn mutant_features() -> Vec<String> {
    CORE_MANIFEST
        .lines()
        .filter_map(|line| line.split_once(" = ").map(|(name, _)| name.trim()))
        .filter(|name| name.starts_with("mutant-"))
        .map(str::to_owned)
        .collect()
}

/// The vector `card` would run, rebuilt from the approved catalogue on
/// disk with the card's `seq`.
fn rebuilt(home: &OrdersHome, card: &WorkOrderCard) -> Vec<String> {
    let relay = home.env.relay_home();
    let state = relay.read_approved();
    let approved = state.approved().expect("an approved catalogue");
    let current = load_relay_floor(&relay.relay_floor(&state)).unwrap();
    let template = ApprovedTemplate::new(
        approved,
        &current,
        &TemplateName::parse(&card.template).unwrap(),
    )
    .unwrap();
    let values: Vec<(ParamName, &str)> = card
        .values
        .iter()
        .map(|choice| {
            (
                ParamName::parse(&choice.name).unwrap(),
                choice.value.as_str(),
            )
        })
        .collect();
    let order = WorkOrder::validate(
        &template,
        &ProjectName::parse(&card.project).unwrap(),
        &values,
    )
    .unwrap();
    officina_core::spawn::work_order(&order, card.seq, None)
        .args()
        .to_vec()
}

/// The order `card` stands for, validated from the approved catalogue.
fn order_of(home: &OrdersHome, card: &WorkOrderCard) -> WorkOrder {
    let relay = home.env.relay_home();
    let state = relay.read_approved();
    let current = load_relay_floor(&relay.relay_floor(&state)).unwrap();
    let template = ApprovedTemplate::new(
        state.approved().unwrap(),
        &current,
        &TemplateName::parse(&card.template).unwrap(),
    )
    .unwrap();
    let values: Vec<(ParamName, &str)> = card
        .values
        .iter()
        .map(|choice| {
            (
                ParamName::parse(&choice.name).unwrap(),
                choice.value.as_str(),
            )
        })
        .collect();
    WorkOrder::validate(
        &template,
        &ProjectName::parse(&card.project).unwrap(),
        &values,
    )
    .unwrap()
}

fn card_at<'a>(cards: &'a [WorkOrderCard], index: &Value) -> Result<&'a WorkOrderCard, String> {
    index
        .as_u64()
        .and_then(|at| cards.get(at as usize))
        .ok_or_else(|| format!("no card {index}"))
}

/// Edits the approved snapshot by hand, its hash kept, as a record edited
/// outside the window would be (D-654).
fn edit_snapshot(home: &OrdersHome, edit: impl FnOnce(&mut Value)) {
    let file = home.env.relay_home().approved_file();
    let mut approved: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    edit(&mut approved["canonical"]["templates"]);
    let parsed: ApprovedCatalogue = serde_json::from_value(approved.clone()).unwrap();
    std::fs::write(&file, serde_json::to_vec(&parsed).unwrap()).unwrap();
}

/// What a `tamper` step changes after a card was issued, or before.
fn tamper(home: &OrdersHome, step: &Value, cards: &[WorkOrderCard]) -> Result<(), String> {
    match step["kind"].as_str() {
        Some("snapshot") => {
            let name = step["template"].as_str().unwrap().to_owned();
            let step = with_values(step);
            edit_snapshot(home, |templates| {
                let template = &mut templates[name.as_str()];
                if let Some(prompt) = step.get("prompt") {
                    template["prompt"] = prompt.clone();
                }
                if let Some(params) = step.get("params").and_then(Value::as_object) {
                    for (param, list) in params {
                        template["params"][param] = list.clone();
                    }
                }
            });
        }
        Some("template_unapproved") => {
            let name = step["template"].as_str().unwrap().to_owned();
            edit_snapshot(home, |templates| {
                templates.as_object_mut().unwrap().remove(&name);
            });
        }
        Some("floor_edited") => {
            let floor = home.env.floor_file();
            let text = std::fs::read_to_string(&floor).unwrap();
            let edited = text.replace("Bump one dependency", "Bump one dependency, edited");
            assert_ne!(text, edited);
            std::fs::write(&floor, edited).unwrap();
        }
        Some("folder_missing") => {
            let mut moved = home.project().into_os_string();
            moved.push("-moved");
            std::fs::rename(home.project(), PathBuf::from(moved)).unwrap();
        }
        Some("subject_archived") => {
            let card = card_at(cards, &step["card"])?;
            let SubjectChoice::Existing(subject) = card.subject else {
                return Err(format!("the card files under {:?}", card.subject));
            };
            let action = AuthorAction::Archive {
                subject,
                routine: false,
            };
            ask(
                &home.handle().sender,
                AuthorCommand::Act(action),
                AUTHOR_WAIT,
            )
            .map_err(|e| format!("archive: {e:?}"))?;
        }
        Some("name_taken") => {
            let card = card_at(cards, &step["card"])?;
            let order = order_of(home, card);
            let now = common::now_ms();
            let mut writer = open_writer(&home.env.officina_home(), &[], now)
                .writer()
                .unwrap();
            writer
                .insert_work_order(
                    &LaunchRecord {
                        order: &order,
                        card_id: "taken-by-another-row",
                        seq: card.seq,
                        source: WorkOrderSource::Composer,
                        subject: &SubjectChoice::New("Elsewhere".into()),
                        proposal_id: None,
                    },
                    now,
                )
                .unwrap()
                .map_err(|_| "the name was already taken".to_owned())?;
        }
        other => return Err(format!("unknown tamper {other:?}")),
    }
    Ok(())
}

/// Checks a card answer against `expect`; an issued card joins `cards`.
fn take_card(
    answer: CardAnswer,
    expect: &Value,
    cards: &mut Vec<WorkOrderCard>,
) -> Result<(), String> {
    let got = serde_json::to_value(&answer).unwrap();
    let matches = match expect["outcome"].as_str() {
        Some("issued") => got["outcome"] == "issued",
        _ => got == *expect,
    };
    if !matches {
        return Err(format!("expected {expect}, got {got}"));
    }
    if let CardAnswer::Issued(card) = answer {
        cards.push(*card);
    }
    Ok(())
}

/// One click on `step`'s card with the hash it names: the card's own, a
/// hash of no prompt, or another card's.
fn click(home: &OrdersHome, step: &Value, cards: &[WorkOrderCard]) -> Result<(), String> {
    let launch = &step["launch"];
    let card = match &launch["card"] {
        Value::String(text) if text == "unknown" => None,
        index => Some(card_at(cards, index)?),
    };
    let id = card.map_or_else(|| "f".repeat(32), |card| card.id.clone());
    let hash = match &launch["hash"] {
        Value::String(text) if text == "card" => card.map(|c| c.prompt_sha256.clone()),
        Value::String(text) if text == "other" => Some("f".repeat(64)),
        index => Some(card_at(cards, index)?.prompt_sha256.clone()),
    }
    .ok_or("the card's own hash of no card")?;
    let before = home.bg_calls().len();
    let answer = home.launch(&id, &hash);
    let got = serde_json::to_value(answer).unwrap();
    if got != step["expect"] {
        return Err(format!("expected {}, got {got}", step["expect"]));
    }
    let calls = home.bg_calls();
    if got["outcome"] == "launched" {
        let card = card.ok_or("a launch without a card")?;
        let ran = calls
            .get(before)
            .map(common::logged_argv)
            .ok_or("a launch that spawned nothing")?;
        if ran != card.argv {
            return Err(format!("ran {ran:?}, the card shows {:?}", card.argv));
        }
        let rebuilt = rebuilt(home, card);
        if ran != rebuilt {
            return Err(format!("ran {ran:?}, the catalogue rebuilds {rebuilt:?}"));
        }
    }
    Ok(())
}

/// Runs one case of `card_launch` in a home of its own.
fn run_card_launch(case: &Value) -> Result<(), String> {
    let claude = case["claude"].as_str().unwrap_or("claude");
    let home = OrdersHome::start_with(TestEnv::new(), serde_json::json!({"mode": "bg"}), claude);
    let mut cards = Vec::new();
    let mut clicked = false;
    for (at, step) in case["steps"].as_array().unwrap().iter().enumerate() {
        let failed = |why: String| format!("step {at}: {why}");
        if let Some(request) = step.get("issue") {
            let request: CardRequest = serde_json::from_value(with_values(request)).unwrap();
            take_card(home.issue(&request), &step["expect"], &mut cards).map_err(failed)?;
        } else if let Some(edit) = step.get("edit") {
            let card = card_at(&cards, &edit["card"]).map_err(failed)?.id.clone();
            let change: CardChange = serde_json::from_value(with_values(&edit["change"])).unwrap();
            take_card(home.edit(&card, change), &step["expect"], &mut cards).map_err(failed)?;
        } else if step.get("launch").is_some() {
            if !clicked && !home.bg_calls().is_empty() {
                return Err(failed("a spawn before the first click".to_owned()));
            }
            clicked = true;
            click(&home, step, &cards).map_err(failed)?;
        } else if let Some(change) = step.get("tamper") {
            tamper(&home, change, &cards).map_err(failed)?;
        } else {
            return Err(failed(format!("unknown step {step}")));
        }
    }
    let spawns = home.bg_calls().len() as u64;
    let expected = case["bg_spawns"].as_u64().unwrap();
    if spawns != expected {
        return Err(format!("{spawns} --bg spawns, expected {expected}"));
    }
    Ok(())
}

/// AD-95, D-147, story 5.7 AC 6: every case of `card_launch` gets its
/// refusal or outcome and its count of `--bg` spawns; the failures are
/// named together, with the mutant each guards.
#[test]
fn card_launch_cases() {
    let set = load();
    let cases = cases(&set, "card_launch");
    assert!(!cases.is_empty());
    let mut failures = Vec::new();
    for chunk in cases.chunks(PARALLEL_CASES) {
        let results: Vec<(String, Option<String>, Result<(), String>)> =
            std::thread::scope(|scope| {
                let runs: Vec<_> = chunk
                    .iter()
                    .map(|case| scope.spawn(move || run_card_launch(case)))
                    .collect();
                chunk
                    .iter()
                    .zip(runs)
                    .map(|(case, run)| {
                        let result = run
                            .join()
                            .unwrap_or_else(|_| Err("the case panicked".to_owned()));
                        (
                            case["name"].as_str().unwrap().to_owned(),
                            case["guards"].as_str().map(str::to_owned),
                            result,
                        )
                    })
                    .collect()
            });
        for (name, guards, result) in results {
            if let Err(why) = result {
                let guarded = guards
                    .map(|feature| format!(" (guarded by {feature})"))
                    .unwrap_or_default();
                failures.push(format!("case {name}{guarded}: {why}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} case(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// What every answer of the session carries besides its case's text: it
/// must reach the review nowhere.
#[cfg(windows)]
const ANSWER_ONLY: &str = "ANSWER-ONLY-c0de";

/// Stand for a text ten times the hook's excerpt bound (D-438).
#[cfg(windows)]
const OVER_EXCERPT: &str = "<over EXCERPT_CHARS>";

/// One `review_excerpt` case (story 5.13 AC 9, D-569): the case's prompt
/// as the author's last, its answer as the session's last, an `auto`
/// request reviewed by the window through the fake; the prompt reaches the
/// call on stdin only, between the markers and cut to the hook's bound;
/// the answer nowhere; the outcome is the decision `structured_output`
/// gave; no approval file and no `--bg` spawn.
#[cfg(windows)]
fn run_review_excerpt(case: &Value) -> Result<(), String> {
    use common::auto::{Auto, Options, SESSION, stop};
    use officina_core::events::EXCERPT_CHARS;
    let filled = |text: &str| text.replace(OVER_EXCERPT, &"x".repeat(10 * EXCERPT_CHARS));
    let marker = case["marker"].as_str().unwrap();
    let options = Options {
        prompt: filled(case["prompt"].as_str().unwrap()),
        ..Options::default()
    };
    let auto = Auto::new(&options);
    let now = common::now_ms();
    let answer = format!("{} {ANSWER_ONLY}", filled(case["answer"].as_str().unwrap()));
    auto.event(serde_json::json!({
        "v": 1, "ts": now - 30_000, "event": "Stop", "session_id": SESSION,
        "cwd": auto.relay.project(), "last_message": answer, "last_message_tail": answer,
    }));
    let id = auto.request("lab-web", Some(SESSION));
    let (handle, _emitted) = auto.window(&options);
    let row = auto.row_of(&id);
    stop(handle);
    let calls = auto.calls();
    let [call] = calls.as_slice() else {
        return Err(format!("{} review calls", calls.len()));
    };
    let stdin = call["stdin"].as_str().unwrap_or_default();
    // The markers open a line; the instruction names them mid-sentence.
    let open = stdin
        .find("\n<<<OFFICINA-DATA-")
        .ok_or("no opening marker")?
        + 1;
    let close = stdin.rfind("\nOFFICINA-DATA-").ok_or("no closing marker")?;
    let data: Value = serde_json::from_str(stdin[open..close].lines().nth(1).unwrap_or_default())
        .map_err(|e| format!("the data line does not parse: {e}"))?;
    let sent = data["last_prompt"].as_str().unwrap_or_default();
    if !sent.contains(marker) {
        return Err(format!("the prompt did not reach the data line: {sent}"));
    }
    if sent.chars().count() > EXCERPT_CHARS {
        return Err(format!(
            "{} characters of prompt sent",
            sent.chars().count()
        ));
    }
    for (place, text) in [
        ("argv", call["argv"].to_string()),
        ("env", call["env"].to_string()),
        ("instruction", stdin[..open].to_owned()),
    ] {
        if text.contains(marker) {
            return Err(format!("the prompt reached the call's {place}"));
        }
    }
    if stdin.contains(ANSWER_ONLY) || call.to_string().contains(ANSWER_ONLY) {
        return Err("the session's answer reached the review".to_owned());
    }
    let decision = row.decision.map(|d| d.code().to_owned());
    if decision.as_deref() != case["decision"].as_str() {
        return Err(format!("decision {decision:?}"));
    }
    let names: Vec<String> = std::fs::read_dir(auto.relay.env.relay_home().approvals_dir())
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    if names.iter().any(|n| !n.ends_with(".request.json")) {
        return Err(format!("decision files written: {names:?}"));
    }
    let bg = common::fake_claude_calls(&auto.relay.env.fake_log())
        .iter()
        .filter(|c| common::logged_argv(c).iter().any(|a| a == "--bg"))
        .count();
    if bg != 0 {
        return Err(format!("{bg} --bg spawns"));
    }
    Ok(())
}

/// Story 5.13 AC 9 (AD-95, D-147, D-569): every case of `review_excerpt`.
#[cfg(windows)]
#[test]
fn review_excerpt_cases() {
    let set = load();
    let cases = cases(&set, "review_excerpt");
    assert!(!cases.is_empty());
    let mut failures = Vec::new();
    for chunk in cases.chunks(PARALLEL_CASES) {
        let results: Vec<(String, Result<(), String>)> = std::thread::scope(|scope| {
            let runs: Vec<_> = chunk
                .iter()
                .map(|case| scope.spawn(move || run_review_excerpt(case)))
                .collect();
            chunk
                .iter()
                .zip(runs)
                .map(|(case, run)| {
                    (
                        case["name"].as_str().unwrap().to_owned(),
                        run.join()
                            .unwrap_or_else(|_| Err("the case panicked".to_owned())),
                    )
                })
                .collect()
        });
        failures.extend(
            results
                .into_iter()
                .filter_map(|(name, result)| result.err().map(|why| format!("case {name}: {why}"))),
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The board the window publishes once its card for `id` satisfies
/// `holds`.
#[cfg(windows)]
fn board_where(
    emitted: &common::Emitted,
    id: &str,
    holds: impl Fn(&officina_core::relay::approvals::ApprovalView) -> bool,
) -> Result<officina_core::board::Board, String> {
    loop {
        let board = emitted
            .boards
            .recv_timeout(common::auto::PATIENCE)
            .map_err(|_| format!("no board shows {id} as expected"))?;
        if board.approvals.iter().any(|a| a.id == id && holds(a)) {
            return Ok(board);
        }
    }
}

/// The request file of `id` as the window reads it, and the argument
/// vector the approved catalogue on disk rebuilds for it (D-27).
#[cfg(windows)]
fn request_and_rebuild(
    auto: &common::auto::Auto,
    id: &str,
) -> Result<
    (
        officina_core::relay::approvals::ApprovalRequest,
        Vec<String>,
    ),
    String,
> {
    let home = auto.relay.env.relay_home();
    let files = officina_host::relay::approvals::read_files(&home);
    let request = files
        .requests
        .get(id)
        .cloned()
        .ok_or("the request file is gone")?;
    let sources = officina_host::relay::approvals::ContextSources::read(&home);
    let rebuilt = officina_core::relay::approvals::rebuild(&sources.context(), id, &request)?;
    Ok((request, rebuilt.argv))
}

/// One `auto_approved_run` case (story 5.14 AC 6, AD-95, D-147, D-151):
/// auto approval on in the case's mode, an `auto` request made by
/// `officina run`, the case's setup applied, and what came of it.
#[cfg(windows)]
fn run_auto_approved_run(case: &Value) -> Result<(), String> {
    use common::auto::{Auto, Options, SESSION, stop};
    use officina_core::relay::auto::{AutoMode, EscalationCause};
    let setup = case["setup"].as_str().unwrap();
    let mode = match case["mode"].as_str().unwrap() {
        "rule" => AutoMode::Rule,
        _ => AutoMode::Review,
    };
    let options = Options {
        mode: match setup {
            "extra_field" => "review-extra-field",
            "catalogue_change" => "review-slow",
            _ => "review",
        },
        hang_ms: 3_000,
        prompt: "Apply the playbook. FAKE-REVIEW:approve".to_owned(),
        ..Options::default()
    };
    let auto = Auto::new(&options);
    if setup == "tree_change" && !auto.git_project(case["name"].as_str().unwrap()) {
        return Ok(());
    }
    let home = auto.relay.env.relay_home();
    auto.set_auto(true, mode);
    if mode == AutoMode::Rule && setup == "approve" {
        // The rule needs a last run of the action that succeeded.
        let first = auto.request("lab-web", Some(SESSION));
        officina_host::relay::authority::decide_approval(&home, &first, true, None)
            .map_err(|e| e.to_string())?;
        let ran = auto.run(&["apply", "group=lab-web"], Some(SESSION));
        if ran.status.code() != Some(0) {
            return Err(format!("the author's run: {ran:?}"));
        }
    }
    let id = auto.request("lab-web", Some(SESSION));
    if setup == "forged_argv" {
        let path = home.approvals_dir().join(format!("{id}.request.json"));
        let mut request: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        request["argv"] = serde_json::json!(["C:/Windows/System32/cmd.exe", "/c", "calc"]);
        std::fs::write(&path, request.to_string()).unwrap();
    }
    let fake = auto.fake(&options, "bin");
    let (handle, emitted) =
        auto.window_with(auto.collector(Some(fake), std::time::Duration::from_secs(10)));
    let outcome = (|| -> Result<(), String> {
        match setup {
            "forged_argv" => {
                board_where(&emitted, &id, |a| !a.valid)?;
                // A few ticks more: the window never takes it up.
                std::thread::sleep(std::time::Duration::from_secs(2));
                if auto.rows().iter().any(|r| r.approval_id == id) || !auto.calls().is_empty() {
                    return Err("the forged request was reviewed".to_owned());
                }
            }
            "catalogue_change" => {
                auto.wait_for_file(&format!("{id}.reviewing"));
                let floor = std::fs::read_to_string(home.default_floor()).unwrap();
                std::fs::write(
                    home.default_floor(),
                    floor.replace("Apply the playbook", "Apply the playbook again"),
                )
                .unwrap();
                let hash = officina_host::relay::home::load_relay_floor(&home.default_floor())
                    .map_err(|e| e.to_string())?
                    .hash;
                officina_host::relay::authority::approve_catalogue(&home, &hash)
                    .map_err(|e| e.to_string())?;
                // The row is written before its approval is tried, and a
                // refused approval sets its cause after (D-734): wait for
                // the cause, not the row alone.
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                let mut row = auto.row_of(&id);
                while row.cause.is_none() && std::time::Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    row = auto.row_of(&id);
                }
                if row.cause != Some(EscalationCause::ApprovalRefused) {
                    return Err(format!("cause {:?}", row.cause));
                }
            }
            "extra_field" => {
                let row = auto.row_of(&id);
                if row.cause != Some(EscalationCause::InvalidAnswer) {
                    return Err(format!("cause {:?}", row.cause));
                }
            }
            _ => auto.wait_for_file(&format!("{id}.approved.json")),
        }
        Ok(())
    })();
    stop(handle);
    outcome?;
    let decided = auto
        .approval_files()
        .into_iter()
        .filter(|n| n.starts_with(&id) && !n.ends_with(".request.json"))
        .collect::<Vec<_>>();
    match case["expect"].as_str().unwrap() {
        "never_reviewed" | "approval_refused" | "invalid_answer" => {
            if !decided.is_empty() {
                return Err(format!("decision files written: {decided:?}"));
            }
            return Ok(());
        }
        "check_not_satisfied" => {
            std::fs::write(auto.relay.project().join("playbook.yml"), "- hosts: all\n").unwrap();
            let refused = auto.run(&["apply", "group=lab-web"], Some(SESSION));
            if refused.status.code() == Some(0) {
                return Err("a run started after the tree changed".to_owned());
            }
            if !auto
                .approval_files()
                .contains(&format!("{id}.approved.json"))
                || auto
                    .approval_files()
                    .contains(&format!("{id}.consumed.json"))
            {
                return Err("the approval was consumed".to_owned());
            }
            return Ok(());
        }
        _ => {}
    }
    let (_, rebuilt) = request_and_rebuild(&auto, &id)?;
    let ran = auto.run(&["apply", "group=lab-web"], Some(SESSION));
    if ran.status.code() != Some(0) {
        return Err(format!("the approved run: {ran:?}"));
    }
    let started: Vec<_> = auto
        .relay
        .env
        .journal()
        .into_iter()
        .filter_map(|line| match line.entry {
            officina_core::relay::journal::JournalEntry::RunStarted(s)
                if s.approval_id.as_deref() == Some(id.as_str()) =>
            {
                Some(s)
            }
            _ => None,
        })
        .collect();
    match started.as_slice() {
        [run] if run.argv == rebuilt => Ok(()),
        [run] => Err(format!(
            "ran {:?}, the catalogue rebuilds {rebuilt:?}",
            run.argv
        )),
        other => Err(format!("{} runs of the approval", other.len())),
    }
}

/// Story 5.14 AC 6 (AD-95, D-147): every case of `auto_approved_run`.
#[cfg(windows)]
#[test]
fn auto_approved_run_cases() {
    run_section("auto_approved_run", run_auto_approved_run);
}

/// Runs `work` on each case of `section`, a few at once, and names every
/// failure.
fn run_section(section: &str, work: fn(&Value) -> Result<(), String>) {
    let set = load();
    let cases = cases(&set, section);
    assert!(!cases.is_empty());
    let mut failures = Vec::new();
    for chunk in cases.chunks(PARALLEL_CASES) {
        let results: Vec<(String, Result<(), String>)> = std::thread::scope(|scope| {
            let runs: Vec<_> = chunk
                .iter()
                .map(|case| scope.spawn(move || work(case)))
                .collect();
            chunk
                .iter()
                .zip(runs)
                .map(|(case, run)| {
                    (
                        case["name"].as_str().unwrap().to_owned(),
                        run.join()
                            .unwrap_or_else(|_| Err("the case panicked".to_owned())),
                    )
                })
                .collect()
        });
        failures.extend(
            results
                .into_iter()
                .filter_map(|(name, result)| result.err().map(|why| format!("case {name}: {why}"))),
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// `value` with each `mcp_tool` stand-in replaced, from its bound.
fn with_mcp_bounds(value: &Value) -> Value {
    use officina_core::mcp::MAX_INPUT_BYTES;
    use officina_core::orders::proposal::{
        PROPOSAL_ANSWER_MAX_CHARS, PROPOSAL_TEXT_MAX_CHARS, PROPOSAL_VALUE_MAX_CHARS,
    };
    match value {
        Value::String(text) => match text.as_str() {
            "<over PROPOSAL_ANSWER_MAX_CHARS>" => {
                Value::from("a".repeat(PROPOSAL_ANSWER_MAX_CHARS + 1))
            }
            "<over PROPOSAL_VALUE_MAX_CHARS>" => {
                Value::from("a".repeat(PROPOSAL_VALUE_MAX_CHARS + 1))
            }
            "<over PROPOSAL_TEXT_MAX_CHARS>" => {
                Value::from("a".repeat(PROPOSAL_TEXT_MAX_CHARS + 1))
            }
            "<over MAX_INPUT_BYTES>" => Value::from("a".repeat(MAX_INPUT_BYTES)),
            _ => value.clone(),
        },
        Value::Array(items) => Value::Array(items.iter().map(with_mcp_bounds).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), with_mcp_bounds(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// What the fake claude ran in `home`: only `claude agents`, never a
/// `--bg` (AD-90, P-5).
fn only_agents_ran(home: &common::mcp::McpHome) -> Result<(), String> {
    for call in common::fake_claude_calls(&home.env.fake_log()) {
        let argv = common::logged_argv(&call);
        if argv.iter().any(|a| a == "--bg") || argv.first().map(String::as_str) != Some("agents") {
            return Err(format!("the server ran claude {argv:?}"));
        }
    }
    Ok(())
}

/// One `mcp_tool` case (story 5.15 AC 8, D-147): the hostile input given
/// to a proposal tool is refused by its bounds, or written as a file
/// holding it as given and nothing else; nothing but `claude agents`
/// runs, no `--bg`.
fn run_mcp_tool(case: &Value) -> Result<(), String> {
    use common::mcp::{McpHome, call, output, response};
    use officina_core::mcp::McpTool;
    let home = McpHome::new();
    let tool = McpTool::from_code(case["tool"].as_str().unwrap()).ok_or("unknown tool")?;
    let arguments = with_mcp_bounds(&case["arguments"]);
    let (_, responses) = home.session(&[
        call(2, tool, arguments.clone()),
        call(3, McpTool::Board, serde_json::json!({})),
    ]);
    let answer = response(&responses, 2);
    let out = output(answer);
    let files: Vec<PathBuf> = std::fs::read_dir(home.proposals_dir())
        .map(|entries| entries.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    match &case["expect"] {
        Value::String(written) if written == "written" => {
            if answer["result"]["isError"] != false {
                return Err(format!("refused: {out}"));
            }
            let [file] = files.as_slice() else {
                return Err(format!("{} files written", files.len()));
            };
            let held: Value = serde_json::from_slice(&std::fs::read(file).unwrap())
                .map_err(|e| format!("the file does not parse: {e}"))?;
            for (key, given) in arguments.as_object().unwrap() {
                if held[key.as_str()] != *given {
                    return Err(format!(
                        "{key}: wrote {}, given {given}",
                        held[key.as_str()]
                    ));
                }
            }
            // Story 5.17 AC 8: the window validates what the tool wrote.
            ingested_as(&home, &case["ingested"])?;
        }
        expect => {
            if answer["result"]["isError"] != true || out["refused"] != expect["refused"] {
                return Err(format!("expected {expect}, got {out}"));
            }
            if !files.is_empty() {
                return Err(format!("{} files written", files.len()));
            }
        }
    }
    only_agents_ran(&home)
}

/// What the window stored of the one proposal file `home` holds, ingested
/// by its writer against the approved snapshot: its status and invalid
/// reason, as `ingested` names them (story 5.17 AC 8).
fn ingested_as(home: &common::mcp::McpHome, expected: &Value) -> Result<(), String> {
    use officina_host::registry::store::{open_reader, open_writer};
    let now = common::now_ms();
    let catalogue = home
        .env
        .relay_home()
        .read_approved()
        .approved()
        .map(|approved| approved.canonical.clone());
    let pass = open_writer(&home.env.officina_home(), &[], now)
        .writer()
        .ok_or("the registry cannot be written")?
        .ingest_proposals(&home.proposals_dir(), catalogue.as_ref(), now)
        .map_err(|e| e.to_string())?;
    if pass.ingested != 1 {
        return Err(format!("{} rows ingested", pass.ingested));
    }
    let rows = open_reader(&home.env.officina_home(), &[])
        .map_err(|e| format!("{e:?}"))?
        .proposals()
        .map_err(|e| e.to_string())?;
    let row = serde_json::to_value(&rows[0]).unwrap();
    if row["status"] != expected["status"] {
        return Err(format!("stored {}, expected {expected}", row["status"]));
    }
    if row["invalid_reason"] != expected.get("reason").cloned().unwrap_or(Value::Null) {
        return Err(format!(
            "reason {}, expected {expected}",
            row["invalid_reason"]
        ));
    }
    Ok(())
}

/// Story 5.15 AC 8 (AD-95, D-147): every case of `mcp_tool`.
#[test]
fn mcp_tool_cases() {
    run_section("mcp_tool", run_mcp_tool);
}

/// One `mcp_output` case (story 5.15 AC 8, D-147, D-153, review F8): the
/// case's marker seeded into one source of session text, every tool
/// called; the marker reaches some output, and every occurrence of it in
/// every output sits inside a wrapper.
fn run_mcp_output(case: &Value) -> Result<(), String> {
    use common::mcp::{McpHome, Sources, every_tool, marker_places, output};
    let marker = case["marker"].as_str().unwrap();
    let source = case["source"].as_str().unwrap();
    let home = McpHome::new();
    let sources = Sources::neutral().with(source, marker);
    let subject = home.seed(&sources);
    let wrapped_in = |responses: &[Value]| -> Result<usize, String> {
        let mut wrapped = 0;
        for answer in responses.iter().filter(|r| r["id"].as_u64() >= Some(100)) {
            if answer["result"]["isError"] != false {
                return Err(format!("a tool refused: {answer}"));
            }
            let out = output(answer);
            let (inside, plain) = marker_places(&out, marker);
            if !plain.is_empty() {
                return Err(format!(
                    "{} shows the marker unwrapped at {plain:?}",
                    out["tool"]
                ));
            }
            wrapped += inside;
        }
        Ok(wrapped)
    };
    let (_, responses) = home.session(&every_tool(&sources, subject));
    if wrapped_in(&responses)? == 0 {
        return Err(format!("the marker of {source} reached no output"));
    }
    // Story 5.17 (D-771): once the window ingested the proposals, their
    // rows read back, texts and invalid reasons, stay wrapped too.
    if source.starts_with("proposal_") {
        let now = common::now_ms();
        let catalogue = home
            .env
            .relay_home()
            .read_approved()
            .approved()
            .map(|approved| approved.canonical.clone());
        let mut writer =
            officina_host::registry::store::open_writer(&home.env.officina_home(), &[], now)
                .writer()
                .ok_or("the registry cannot be written")?;
        let pass = writer
            .ingest_proposals(&home.proposals_dir(), catalogue.as_ref(), now)
            .map_err(|e| e.to_string())?;
        drop(writer);
        if pass.ingested == 0 {
            return Err("no proposal ingested".to_owned());
        }
        let (_, responses) = home.session(&every_tool(&sources, subject));
        if wrapped_in(&responses)? == 0 {
            return Err(format!(
                "the marker of {source} reached no output from the rows"
            ));
        }
    }
    only_agents_ran(&home)
}

/// `value` with the seeded subjects' stand-ins replaced by their ids and
/// the bounds' stand-ins by texts one character over them.
fn with_proposal_refs(value: &Value, subjects: [i64; 3]) -> Value {
    match value {
        Value::String(text) => match text.as_str() {
            "<open subject>" => Value::from(subjects[0]),
            "<archived subject>" => Value::from(subjects[1]),
            "<lab subject>" => Value::from(subjects[2]),
            _ => with_mcp_bounds(value),
        },
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| with_proposal_refs(item, subjects))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), with_proposal_refs(value, subjects)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// One `proposal_file` case (story 5.17 AC 8, D-147, D-152, D-155): a file
/// written by hand into `orchestrator/proposals/`, which the window's
/// collector ingests. An invalid one is stored with its reason and opens
/// no card; a misnamed one, or one past the hourly limit, leaves no row; a
/// valid one opens a card through the author's validation, whose subject
/// never carries a suggested name, and launches once after the click, its
/// vector the golden rebuild's.
fn run_proposal_file(case: &Value) -> Result<(), String> {
    use officina_core::orders::card::IssueRefusal;
    use officina_core::orders::proposal::{
        PROPOSAL_FILE_MAX_BYTES, PROPOSAL_HOUR_MS, PROPOSALS_PER_HOUR, file_name, proposal_id,
    };
    if case["file"]["kind"] == "answer" {
        return run_answer_file(case);
    }
    let home = OrdersHome::start(serde_json::json!({"mode": "bg"}));
    let subjects = home.seed_subjects();
    let now = common::now_ms();
    let id = proposal_id(now, 1);
    let mut bytes = match (&case["file"], &case["raw"]) {
        (Value::Object(_), Value::Null) => {
            let mut file = with_proposal_refs(&case["file"], subjects);
            file["written_at_ms"] = Value::from(now);
            serde_json::to_vec(&file).unwrap()
        }
        (Value::Null, Value::String(raw)) => raw.as_bytes().to_vec(),
        _ => return Err("a file or raw bytes, one of them".to_owned()),
    };
    if case["oversized"] == true {
        bytes.resize(PROPOSAL_FILE_MAX_BYTES + 1, b' ');
    }
    if case["hourly_limit"] == true {
        let dir = home.proposals_dir();
        std::fs::create_dir_all(&dir).unwrap();
        for n in 0..PROPOSALS_PER_HOUR as i64 {
            let earlier = proposal_id(now - PROPOSAL_HOUR_MS / 2 + n, 2);
            std::fs::write(dir.join(file_name(&earlier)), &bytes).unwrap();
        }
    }
    let name = case["file_name"]
        .as_str()
        .map_or_else(|| file_name(&id), str::to_owned);
    home.propose_file(&name, &bytes);
    let expect = &case["expect"];
    let row = home.proposals().into_iter().find(|p| p.id == id);
    match expect["stored"].as_str() {
        Some("none") => {
            if row.is_some() {
                return Err("a row for a file that must leave none".to_owned());
            }
            let rows = home.proposals().len();
            let expected = if case["hourly_limit"] == true {
                PROPOSALS_PER_HOUR
            } else {
                0
            };
            if rows != expected {
                return Err(format!("{rows} rows, expected {expected}"));
            }
        }
        Some("invalid") => {
            let row = row.ok_or("no row")?;
            let stored = serde_json::to_value(&row).unwrap();
            if stored["status"] != "invalid" || stored["invalid_reason"] != expect["reason"] {
                return Err(format!(
                    "stored {} ({}), expected {expect}",
                    stored["status"], stored["invalid_reason"]
                ));
            }
            let unread = matches!(expect["reason"].as_str(), Some("unparsable" | "oversized"));
            if unread && (row.kind.is_some() || row.content.is_some()) {
                return Err("an unread file kept a kind or texts".to_owned());
            }
            if let CardAnswer::Issued(_) = home.issue_from_proposal(&id) {
                return Err("an invalid proposal opened a card".to_owned());
            }
        }
        Some("pending") => {
            let row = row.ok_or("no row")?;
            if serde_json::to_value(row.status).unwrap() != "pending" {
                return Err(format!("stored {:?}", row.status));
            }
            let answer = home.issue_from_proposal(&id);
            let card = match answer {
                CardAnswer::Issued(card) => *card,
                CardAnswer::Refused(IssueRefusal::Proposal(refusal))
                    if row.kind == Some(officina_core::registry::types::ProposalKind::Recap) =>
                {
                    if format!("{refusal:?}") != "NotAWorkOrder" {
                        return Err(format!("a recap refused {refusal:?}"));
                    }
                    return check_spawns(&home, case);
                }
                other => return Err(format!("no card: {other:?}")),
            };
            // D-152: the suggested name is never the card's subject nor
            // one of its choices.
            if let Some(suggested) = case["file"]["subject"]["new"].as_str() {
                let carried = match &card.subject {
                    SubjectChoice::New(name) => name == suggested,
                    SubjectChoice::Existing(_) => false,
                } || card.subject_options.iter().any(|s| s.name == suggested);
                if carried {
                    return Err("the card's subject carries the suggested name".to_owned());
                }
            }
            if !home.bg_calls().is_empty() {
                return Err("a spawn before the click".to_owned());
            }
            if expect["launch"] == true {
                let answer =
                    serde_json::to_value(home.launch(&card.id, &card.prompt_sha256)).unwrap();
                if answer != serde_json::json!({"outcome": "launched", "value": "started"}) {
                    return Err(format!("launch answered {answer}"));
                }
                let ran = home
                    .bg_calls()
                    .first()
                    .map(common::logged_argv)
                    .ok_or("a launch that spawned nothing")?;
                if ran != card.argv || ran != rebuilt(&home, &card) {
                    return Err(format!("ran {ran:?}, the card shows {:?}", card.argv));
                }
                // Clicked again, it never launches twice.
                home.launch(&card.id, &card.prompt_sha256);
            }
        }
        other => return Err(format!("unknown expectation {other:?}")),
    }
    check_spawns(&home, case)
}

/// The session an answer case lists, a canonical session id.
const LISTED_SESSION: &str = "0f1e2d3c-4b5a-4978-8796-a5b4c3d2e1f0";

/// `claude agents`' record of [`LISTED_SESSION`] in `folder`, in the
/// state an answer case names (story 5.21 AC 8): a background session idle
/// in its turn, waiting on a question or a permission prompt, or ended; an
/// interactive session in its turn; or none (`unknown`).
fn listed_session(state: &str, folder: &std::path::Path) -> Result<Value, String> {
    let now = common::now_ms();
    let mut entry = serde_json::json!({"id": "bg0001", "sessionId": LISTED_SESSION,
        "name": "bump", "cwd": folder, "kind": "background", "pid": 4200,
        "startedAt": now - 60_000, "status": "idle", "state": "done"});
    match state {
        "idle" => {}
        "interactive" => {
            entry["kind"] = "interactive".into();
            entry.as_object_mut().unwrap().remove("id");
        }
        "question" => {
            entry["status"] = "waiting".into();
            entry["state"] = "blocked".into();
            entry["waitingFor"] = "input needed".into();
        }
        "permission" => {
            entry["status"] = "waiting".into();
            entry["state"] = "blocked".into();
            entry["waitingFor"] = "permission prompt".into();
        }
        "ended" => {
            entry["state"] = "stopped".into();
            entry.as_object_mut().unwrap().remove("pid");
        }
        "unknown" => return Ok(Value::Array(Vec::new())),
        other => return Err(format!("unknown session state {other}")),
    }
    Ok(Value::Array(vec![entry]))
}

/// The `--resume` calls the fake logged.
fn resume_calls(home: &OrdersHome) -> Vec<Value> {
    common::fake_claude_calls(&home.env.fake_log())
        .into_iter()
        .filter(|call| common::logged_argv(call).iter().any(|a| a == "--resume"))
        .collect()
}

/// One answer case of `proposal_file` (story 5.21 AC 8, AD-182, D-147):
/// [`LISTED_SESSION`] listed in the case's `session` state, a file written
/// by hand naming it, which the window's collector ingests against the
/// board. An invalid one is stored with its reason and a click on it is
/// refused; a valid one is pending and spawns nothing until the click,
/// which, when the case clicks, spawns exactly one `--resume`, its vector
/// the golden `Answer` rebuild with the stored text in the folder `claude
/// agents` reported (a console start: Windows; elsewhere the start is
/// refused as unsupported and nothing runs). Never a `--bg`.
fn run_answer_file(case: &Value) -> Result<(), String> {
    use officina_core::meeting::{AnswerRefused, AnswerResult, Delivery};
    use officina_core::orders::proposal::{file_name, proposal_id};
    use officina_host::meeting::{AnswerFrom, send_answer};
    let home = OrdersHome::start(serde_json::json!({"mode": "resume"}));
    let folder = home.env.root.path().join("work").join("answered");
    std::fs::create_dir_all(&folder).unwrap();
    let state = case["session"]
        .as_str()
        .ok_or("an answer case names its session")?;
    home.env
        .write_agents(&listed_session(state, &folder)?.to_string());
    home.handle()
        .sender
        .send(officina_host::watch::CollectorMsg::Refresh)
        .unwrap();
    let key = if state == "unknown" {
        format!("sid:{LISTED_SESSION}")
    } else {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let board = home
                .emitted
                .boards
                .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
                .map_err(|_| "the board never listed the session")?;
            if let Some(card) = board
                .columns
                .all()
                .find(|c| c.session_id.as_deref() == Some(LISTED_SESSION))
            {
                break card.key.clone();
            }
        }
    };
    let now = common::now_ms();
    let id = proposal_id(now, 1);
    let mut file = with_mcp_bounds(&case["file"]);
    if file["session_id"] == "<listed session>" {
        file["session_id"] = LISTED_SESSION.into();
    }
    file["written_at_ms"] = Value::from(now);
    home.propose_file(&file_name(&id), &serde_json::to_vec(&file).unwrap());
    let row = home
        .proposals()
        .into_iter()
        .find(|p| p.id == id)
        .ok_or("no row")?;
    let stored = serde_json::to_value(&row).unwrap();
    let expect = &case["expect"];
    if stored["status"] != expect["stored"]
        || stored["invalid_reason"] != expect.get("reason").cloned().unwrap_or(Value::Null)
    {
        return Err(format!(
            "stored {} ({}), expected {expect}",
            stored["status"], stored["invalid_reason"]
        ));
    }
    if !resume_calls(&home).is_empty() {
        return Err("a resume before the click".to_owned());
    }
    let click = || {
        send_answer(
            &home.handle().sender,
            &key,
            Delivery::ResumeLive,
            AnswerFrom::Proposal(id.clone()),
        )
    };
    if expect["stored"] == "invalid" {
        let answer = click().map_err(|e| e.message)?;
        let refused = [
            AnswerResult::Refused(AnswerRefused::ProposalNotPending),
            AnswerResult::Refused(AnswerRefused::UnknownSession),
        ];
        if !refused.contains(&answer) {
            return Err(format!(
                "a click on an invalid proposal answered {answer:?}"
            ));
        }
    } else if case["click"] == true {
        let answer = click().map_err(|e| e.message)?;
        let text = file["text"].as_str().unwrap();
        if cfg!(windows) {
            if answer != AnswerResult::Confirmed {
                return Err(format!("the click answered {answer:?}"));
            }
            let golden = officina_core::spawn::answer(
                &officina_core::meeting::Answer::parse(text).map_err(|e| format!("{e:?}"))?,
                &officina_core::spawn::SessionId::parse(LISTED_SESSION).unwrap(),
                &folder.to_string_lossy(),
                None,
            );
            let calls = resume_calls(&home);
            let [call] = calls.as_slice() else {
                return Err(format!("{} resumes after the click", calls.len()));
            };
            if common::logged_argv(call) != golden.args() {
                return Err(format!(
                    "ran {:?}, the golden rebuild is {:?}",
                    common::logged_argv(call),
                    golden.args()
                ));
            }
            let ran_in = std::fs::canonicalize(call["cwd"].as_str().unwrap_or_default());
            if ran_in.ok() != std::fs::canonicalize(&folder).ok() {
                return Err(format!("resumed in {}", call["cwd"]));
            }
        } else if !matches!(answer, AnswerResult::SpawnFailed(_)) {
            return Err(format!("a console start off Windows answered {answer:?}"));
        }
        // Clicked again, it never sends twice.
        let again = click().map_err(|e| e.message)?;
        if again != AnswerResult::Refused(AnswerRefused::ProposalNotPending) {
            return Err(format!("a second click answered {again:?}"));
        }
    }
    let resumes = resume_calls(&home).len() as u64;
    let expected = if cfg!(windows) {
        case["resume_spawns"]
            .as_u64()
            .ok_or("an answer case counts its resumes")?
    } else {
        0
    };
    if resumes != expected {
        return Err(format!("{resumes} --resume spawns, expected {expected}"));
    }
    if !home.bg_calls().is_empty() {
        return Err("an answer spawned a --bg".to_owned());
    }
    Ok(())
}

/// The case's count of `--bg` spawns, exactly.
fn check_spawns(home: &OrdersHome, case: &Value) -> Result<(), String> {
    let spawns = home.bg_calls().len() as u64;
    let expected = case["bg_spawns"].as_u64().unwrap();
    if spawns != expected {
        return Err(format!("{spawns} --bg spawns, expected {expected}"));
    }
    Ok(())
}

/// Story 5.17 AC 8 (AD-95, D-147, D-152, D-155): every case of
/// `proposal_file`.
#[test]
fn proposal_file_cases() {
    run_section("proposal_file", run_proposal_file);
}

/// Story 5.15 AC 8 (AD-95, D-147): every case of `mcp_output`, and every
/// source of `common::mcp::SOURCES` seeded by one.
#[test]
fn mcp_output_cases() {
    let set = load();
    for source in common::mcp::SOURCES {
        assert!(
            cases(&set, "mcp_output")
                .iter()
                .any(|case| case["source"] == *source),
            "source {source} has no mcp_output case"
        );
    }
    run_section("mcp_output", run_mcp_output);
}

/// D-147: every section has a case, every family a case and every case a
/// declared family; every variant of `WorkOrderRefusal` and
/// `LaunchRefusal` is some case's refusal, and every variant of
/// `ProposalInvalidReason` some `proposal_file` case's; and every mutant of core is
/// guarded by a case, forwarded under its name by the host and the CLI
/// crates, and counted by `MUTANTS_ENABLED` (D-148).
#[test]
fn every_section_family_and_refusal_has_a_case() {
    let set = load();
    let sections: Vec<&str> = set
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .filter(|key| *key != "schema")
        .collect();
    assert_eq!(sections, SECTIONS, "the sections of injection.json");
    let all: Vec<Value> = SECTIONS
        .iter()
        .flat_map(|section| {
            let found = cases(&set, section);
            assert!(!found.is_empty(), "section {section} has no case");
            found
        })
        .collect();
    for case in &all {
        let family = case["family"].as_str().unwrap_or_default();
        assert!(
            FAMILIES.contains(&family),
            "{}: family {family:?}",
            case["name"]
        );
    }
    for family in FAMILIES {
        assert!(
            all.iter().any(|case| case["family"] == *family),
            "family {family} has no case"
        );
    }
    let expectations: Vec<&Value> = all
        .iter()
        .filter_map(|case| case["steps"].as_array())
        .flatten()
        .map(|step| &step["expect"])
        .collect();
    for refusal in WorkOrderRefusal::ALL {
        let wanted = serde_json::json!({"outcome": "refused", "value": {"kind": "order", "detail": refusal}});
        assert!(
            expectations.contains(&&wanted),
            "{refusal:?} has no case in injection.json"
        );
    }
    for refusal in LaunchRefusal::ALL {
        let wanted = serde_json::json!({"outcome": "refused", "value": refusal});
        assert!(
            expectations.contains(&&wanted),
            "{refusal:?} has no case in injection.json"
        );
    }
    let files = cases(&set, "proposal_file");
    for reason in officina_core::orders::proposal::ProposalInvalidReason::ALL {
        assert!(
            files
                .iter()
                .any(|case| case["expect"]["reason"] == reason.code()),
            "{reason:?} has no proposal_file case in injection.json"
        );
    }

    let mutants = mutant_features();
    assert!(!mutants.is_empty(), "officina-core declares the mutants");
    for case in &all {
        if let Some(guards) = case["guards"].as_str() {
            assert!(
                mutants.iter().any(|m| m == guards),
                "{}: {guards}",
                case["name"]
            );
        }
    }
    for mutant in &mutants {
        assert!(
            all.iter().any(|case| case["guards"] == mutant.as_str()),
            "{mutant} is guarded by no case"
        );
        for (crate_name, manifest, forwarded) in [
            (
                "officina-host",
                HOST_MANIFEST,
                format!("officina-core/{mutant}"),
            ),
            ("officina", CLI_MANIFEST, format!("officina-host/{mutant}")),
        ] {
            let line = format!("{mutant} = [\"{forwarded}\"]");
            assert!(
                manifest.lines().any(|l| l.trim() == line),
                "{crate_name} does not forward {mutant}"
            );
        }
        assert!(
            CORE_LIB.contains(&format!("feature = \"{mutant}\"")),
            "MUTANTS_ENABLED does not count {mutant}"
        );
    }
}
