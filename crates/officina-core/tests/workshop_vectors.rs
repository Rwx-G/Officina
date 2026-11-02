//! The workshop's image check (story 6.8 AC 2, AD-99, D-207, D-233) and
//! the inventory's extraction (story 6.9 AC 1, AD-118, D-211): the
//! `images` and `inventory` sections of `tests/vectors/workshop.json`
//! replayed against core. The other sections are replayed by the unit
//! tests of the modules whose builders are crate-private (D-1121).

use std::collections::{BTreeMap, BTreeSet};

use officina_core::workshop::engine::ImageFacts;
use officina_core::workshop::image::{Expected, ImageMismatch, Recorded, check};
use serde_json::Value;

const VECTORS: &str = include_str!("../../../tests/vectors/workshop.json");

fn cases() -> Vec<Value> {
    let all: Value = serde_json::from_str(VECTORS).expect("workshop.json parses");
    all["images"].as_array().expect("an images section").clone()
}

fn labels(value: &Value) -> BTreeMap<String, String> {
    value
        .as_object()
        .expect("labels")
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().expect("a label value").to_owned()))
        .collect()
}

fn text(value: &Value) -> Option<String> {
    value.as_str().map(str::to_owned)
}

fn expected(value: &Value) -> Expected {
    Expected {
        labels: labels(&value["labels"]),
        base_id: text(&value["base_id"]),
        base_programs_sha256: text(&value["base_programs_sha256"]),
    }
}

fn inspected(value: &Value) -> Option<ImageFacts> {
    (!value.is_null()).then(|| ImageFacts {
        id: text(&value["id"]).expect("an id"),
        labels: labels(&value["labels"]),
        env_names: value["env_names"]
            .as_array()
            .expect("env names")
            .iter()
            .filter_map(text)
            .collect(),
    })
}

fn recorded(value: &Value) -> Option<Recorded> {
    (!value.is_null()).then(|| Recorded {
        image_id: text(&value["image_id"]),
        programs_sha256: text(&value["programs_sha256"]),
        built: value["built"].as_bool().expect("built"),
    })
}

fn outcome(case: &Value) -> Value {
    let result = check(
        &expected(&case["expected"]),
        inspected(&case["inspected"]).as_ref(),
        recorded(&case["recorded"]).as_ref(),
    );
    match result {
        Ok(id) => serde_json::json!({ "ok": id }),
        Err(mismatch) => serde_json::json!({
            "mismatch": mismatch.cause.code(),
            "label": mismatch.label,
        }),
    }
}

#[test]
fn image_check_cases_match() {
    let cases = cases();
    assert!(!cases.is_empty());
    for case in &cases {
        assert_eq!(outcome(case), case["expect"], "{}", case["name"]);
    }
}

#[test]
fn every_mismatch_has_a_case() {
    let seen: BTreeSet<String> = cases()
        .iter()
        .filter_map(|case| text(&case["expect"]["mismatch"]))
        .collect();
    for cause in ImageMismatch::ALL {
        assert!(
            seen.contains(cause.code()),
            "{cause:?} has no case in the images section"
        );
    }
    assert!(
        cases().iter().any(|case| case["expect"]["ok"].is_string()),
        "a passing case"
    );
}

fn inventory_input(
    case: &Value,
) -> (
    Vec<officina_core::workshop::inventory::SettingsFile>,
    Option<String>,
    Vec<String>,
) {
    use officina_core::workshop::inventory::{SettingsFile, SettingsSource};
    let settings = case["settings"]
        .as_array()
        .expect("settings")
        .iter()
        .map(|file| SettingsFile {
            source: SettingsSource::from_code(file["source"].as_str().unwrap()).unwrap(),
            bytes: file["text"].as_str().unwrap().as_bytes().to_vec(),
        })
        .collect();
    let mounts = case["mounts"]
        .as_array()
        .expect("mounts")
        .iter()
        .filter_map(text)
        .collect();
    (settings, text(&case["mcp"]), mounts)
}

fn extracted(case: &Value) -> Value {
    use officina_core::workshop::inventory::{extract, inventory_key};
    let (settings, mcp, mounts) = inventory_input(case);
    let commands = extract(&settings, mcp.as_deref().map(str::as_bytes), &mounts).unwrap();
    let key = inventory_key(&commands, case["image_id"].as_str().unwrap());
    serde_json::json!({ "commands": commands, "key": key })
}

#[expect(
    clippy::disallowed_methods,
    reason = "a test's write mode for a reviewed change of the golden file"
)]
fn write_inventory(cases: Vec<Value>) {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/vectors/workshop.json"
    );
    let mut all: Value = serde_json::from_str(VECTORS).unwrap();
    all["inventory"] = Value::Array(cases);
    std::fs::write(path, serde_json::to_string_pretty(&all).unwrap() + "\n").unwrap();
}

#[expect(
    clippy::disallowed_methods,
    reason = "a test's write mode for a reviewed change of the golden file"
)]
fn writing() -> bool {
    std::env::var("OFFICINA_WRITE_VECTORS").is_ok_and(|v| v == "1")
}

fn inventory_cases() -> Vec<Value> {
    let all: Value = serde_json::from_str(VECTORS).expect("workshop.json parses");
    all["inventory"]
        .as_array()
        .expect("an inventory section")
        .clone()
}

/// Story 6.9 AC 1: each case's commands and key equal what `extract` and
/// `inventory_key` give; `OFFICINA_WRITE_VECTORS=1` writes them for a
/// reviewed change.
#[test]
fn inventory_extraction_cases_match() {
    let mut cases = inventory_cases();
    assert!(!cases.is_empty());
    if writing() {
        for case in &mut cases {
            case["expect"] = extracted(case);
        }
        write_inventory(cases);
        return;
    }
    for case in &cases {
        assert_eq!(extracted(case), case["expect"], "{}", case["name"]);
    }
}

#[test]
fn the_key_changes_with_a_tracked_script() {
    use officina_core::workshop::inventory::{extract, inventory_key};
    let case = &inventory_cases()[0];
    let (settings, mcp, mounts) = inventory_input(case);
    let mut commands = extract(&settings, mcp.as_deref().map(str::as_bytes), &mounts).unwrap();
    let image = case["image_id"].as_str().unwrap();
    let before = inventory_key(&commands, image);
    let tracked = commands
        .iter_mut()
        .find_map(|c| c.tracked.as_mut())
        .expect("a tracked script in the first case");
    tracked.present = true;
    tracked.blob = Some("a".repeat(40));
    let present = inventory_key(&commands, image);
    assert_ne!(before, present);
    commands
        .iter_mut()
        .find_map(|c| c.tracked.as_mut())
        .unwrap()
        .blob = Some("b".repeat(40));
    assert_ne!(present, inventory_key(&commands, image), "another blob");
    assert_ne!(
        inventory_key(&commands, image),
        inventory_key(&commands, "sha256:other"),
        "another image"
    );
}

#[test]
fn tracked_settings_hooks_are_listed() {
    let case = inventory_cases()
        .into_iter()
        .find(|c| {
            c["settings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["source"] == "tracked_settings")
        })
        .expect("a case with tracked settings");
    let commands = extracted(&case)["commands"].as_array().unwrap().clone();
    assert!(
        commands.iter().any(|c| c["event"] == "PreToolUse"),
        "the tracked settings' guard is listed: {commands:?}"
    );
    assert!(commands.iter().any(|c| c["event"] == "PostToolUse"));
}

/// The passing checks every `launch` case starts from.
fn passing_checks() -> Value {
    use officina_core::orders::workshop_card::{MountState, WorkshopChecks};
    use officina_core::workshop::floor::CredentialKind;
    use officina_core::workshop::launch::CredentialState;
    serde_json::to_value(WorkshopChecks {
        workshop_on: true,
        stopped: false,
        engine_ready: true,
        image_matches: true,
        battery_valid: true,
        mounts: vec![MountState::Ready],
        live_runs: 0,
        max_workers: 2,
        reviews_pending: 0,
        credential_kind: CredentialKind::OauthToken,
        credential_present: true,
        credential_state: CredentialState::Unchecked,
        proxy: None,
        project_exists: true,
        base_matches: Some(true),
        plan_matches: true,
        hooks_found: None,
    })
    .unwrap()
}

fn launch_cases() -> Vec<Value> {
    let all: Value = serde_json::from_str(VECTORS).expect("workshop.json parses");
    all["launch"].as_array().expect("a launch section").clone()
}

/// Story 6.11 AC 4 (AD-107, D-247): each case's checks give its refusal,
/// the first failing one in AD-107's order whatever the evaluation order,
/// the inventory run only when every other check passed, and the
/// readiness the card shows in that order.
#[test]
fn launch_checks_report_in_ads_order() {
    use officina_core::orders::card::LaunchRefusal;
    use officina_core::orders::workshop_card::{WorkshopChecks, check_workshop_launch, readiness};
    let cases = launch_cases();
    assert!(!cases.is_empty());
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        let mut checks = passing_checks();
        for (key, value) in case["checks"].as_object().unwrap() {
            assert!(checks.get(key).is_some(), "{name}: no check {key}");
            checks[key] = value.clone();
        }
        let checks: WorkshopChecks = serde_json::from_value(checks).unwrap();
        let mut ran = false;
        let got = check_workshop_launch(&checks, || {
            ran = true;
            case["inventory"].as_bool().unwrap()
        });
        let got = match got {
            Ok(()) => Value::from("launched"),
            Err(refusal) => serde_json::json!({ "refused": refusal }),
        };
        assert_eq!(got, case["expect"], "{name}");
        let shown = readiness(&checks);
        assert_eq!(
            ran,
            shown.is_empty(),
            "{name}: the inventory runs last, alone"
        );
        let order: Vec<usize> = shown
            .iter()
            .map(|r| {
                LaunchRefusal::ALL
                    .iter()
                    .position(|x| *x == r.refusal)
                    .unwrap()
            })
            .collect();
        assert!(order.windows(2).all(|w| w[0] <= w[1]), "{name}: {shown:?}");
        if let Some(wanted) = case.get("readiness") {
            assert_eq!(serde_json::to_value(&shown).unwrap(), *wanted, "{name}");
        }
    }
}

/// Every workshop refusal (AD-107) has a `launch` case that reports it.
#[test]
fn every_new_launch_refusal_has_a_case() {
    use officina_core::orders::card::LaunchRefusal;
    let cases = launch_cases();
    for refusal in LaunchRefusal::ALL.iter().filter(|r| r.is_workshop()) {
        let wanted = serde_json::json!({ "refused": refusal });
        assert!(
            cases.iter().any(|c| c["expect"] == wanted),
            "{refusal:?} has no launch case in tests/vectors/workshop.json"
        );
    }
}
