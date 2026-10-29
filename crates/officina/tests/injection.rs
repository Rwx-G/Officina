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
//! Sections: `card_launch` (story 5.7); later stories add theirs.

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

/// The entry points the battery covers, one section each (D-147).
const SECTIONS: &[&str] = &["card_launch", "review_excerpt"];

/// The attack families of PRD section 7's Gesture row the sections hold,
/// declared here once (D-147), and what a case of none of them is.
const FAMILIES: &[&str] = &[
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

/// D-147: every section has a case, every family a case and every case a
/// declared family; every variant of `WorkOrderRefusal` and
/// `LaunchRefusal` is some case's refusal; and every mutant of core is
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
