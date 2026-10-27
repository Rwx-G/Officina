//! Replays the hand-written argument vector set (AD-30) through phase 2 and
//! `argv::build`, and refuses a set that no longer covers every runner,
//! every value refusal and every character class E13 verified through WSL.

use std::collections::BTreeMap;

use officina_core::relay::argv::{ArgvRefusal, Launch, WSL_VERIFIED_CLASSES, build};
use officina_core::relay::catalogue::{Action, Runner, action_from_value};
use officina_core::relay::params::ValueRefusal;
use serde::Deserialize;

const VECTORS: &str = include_str!("../../../tests/vectors/argv.json");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VectorSet {
    schema: u32,
    system_dir: String,
    project_dir: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    /// An `[[action]]` table, written as JSON.
    action: serde_json::Value,
    params: BTreeMap<String, String>,
    #[serde(default)]
    expect: Option<Launch>,
    #[serde(default)]
    refusal: Option<ArgvRefusal>,
}

fn load() -> VectorSet {
    let set: VectorSet = serde_json::from_str(VECTORS).expect("vector set parses");
    assert_eq!(set.schema, 1);
    set
}

/// The case's action through phase 2, then its launch or refusal.
fn replay(set: &VectorSet, case: &Case) -> (Action, Result<Launch, ArgvRefusal>) {
    let table: toml::Value = serde_json::from_value(case.action.clone())
        .unwrap_or_else(|e| panic!("case {}: action is not a table: {e}", case.name));
    let action = action_from_value(&table)
        .unwrap_or_else(|issues| panic!("case {}: invalid action: {issues:#?}", case.name));
    let outcome = build(&action, &case.params, &set.project_dir, &set.system_dir);
    (action, outcome)
}

#[test]
fn every_case_matches() {
    let set = load();
    for case in &set.cases {
        let expected = match (&case.expect, &case.refusal) {
            (Some(launch), None) => Ok(launch.clone()),
            (None, Some(refusal)) => Err(refusal.clone()),
            _ => panic!("case {}: exactly one of expect or refusal", case.name),
        };
        let (_, outcome) = replay(&set, case);
        assert_eq!(outcome, expected, "case: {}", case.name);
    }
}

#[test]
fn every_rule_and_runner_has_a_case() {
    let set = load();
    let replayed: Vec<(Action, Result<Launch, ArgvRefusal>)> =
        set.cases.iter().map(|case| replay(&set, case)).collect();

    for runner in Runner::ALL {
        assert!(
            replayed
                .iter()
                .any(|(action, outcome)| action.runner == *runner && outcome.is_ok()),
            "no launch case for runner {runner:?}"
        );
    }
    for refusal in ValueRefusal::ALL {
        assert!(
            replayed.iter().any(|(_, outcome)| matches!(
                outcome,
                Err(ArgvRefusal::Value { refusal: r, .. }) if r == refusal
            )),
            "no case for the value refusal {refusal:?}"
        );
    }
    for class in WSL_VERIFIED_CLASSES {
        let covered = set
            .cases
            .iter()
            .zip(&replayed)
            .any(|(case, (action, outcome))| {
                action.runner == Runner::Wsl
                    && outcome.is_ok()
                    && case.params.values().any(|value| (class.matches)(value))
            });
        assert!(
            covered,
            "no wsl launch case for the E13 class {}",
            class.name
        );
    }
}
