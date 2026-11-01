//! The workshop's image check (story 6.8 AC 2, AD-99, D-207, D-233): the
//! `images` section of `tests/vectors/workshop.json` replayed against
//! `image::check`. The other sections are replayed by the unit tests of the
//! modules whose builders are crate-private (D-1121).

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
