//! Replays the hand-written URL template vector set (AD-36, D-40) through
//! `relay::url`, and refuses a set that no longer covers every issue kind
//! and every byte class of the encoder.

use std::collections::{BTreeMap, BTreeSet};

use officina_core::relay::url::{ByteClass, UrlIssue, UrlRefusal, UrlTemplate, byte_class};
use serde::Deserialize;

const VECTORS: &str = include_str!("../../../tests/vectors/url.json");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VectorSet {
    schema: u32,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    template: String,
    #[serde(default)]
    params: BTreeMap<String, String>,
    expect: Expect,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Expect {
    Url(String),
    Issue(UrlIssue),
    Refusal(UrlRefusal),
}

fn load() -> VectorSet {
    let set: VectorSet = serde_json::from_str(VECTORS).expect("vector set parses");
    assert_eq!(set.schema, 1);
    set
}

fn replay(case: &Case) -> Expect {
    match UrlTemplate::parse(&case.template) {
        Err(issue) => Expect::Issue(issue),
        Ok(template) => match template.fill(&case.params) {
            Ok(url) => Expect::Url(url),
            Err(refusal) => Expect::Refusal(refusal),
        },
    }
}

#[test]
fn every_case_matches() {
    for case in &load().cases {
        assert_eq!(replay(case), case.expect, "case: {}", case.name);
    }
}

#[test]
fn every_rule_has_a_case() {
    let set = load();
    let issues: BTreeSet<String> = set
        .cases
        .iter()
        .filter_map(|case| match replay(case) {
            Expect::Issue(issue) => Some(format!("{issue:?}")),
            _ => None,
        })
        .collect();
    for issue in UrlIssue::ALL {
        assert!(
            issues.contains(&format!("{issue:?}")),
            "no case for the URL issue {issue:?}"
        );
    }

    // Every byte class of the encoder appears in a value that filled a URL.
    let classes: BTreeSet<String> = set
        .cases
        .iter()
        .filter(|case| matches!(replay(case), Expect::Url(_)))
        .flat_map(|case| case.params.values())
        .flat_map(|value| value.bytes())
        .map(|byte| format!("{:?}", byte_class(byte)))
        .collect();
    for class in ByteClass::ALL {
        assert!(
            classes.contains(&format!("{class:?}")),
            "no filled value holds a byte of the class {class:?}"
        );
    }
    assert!(
        set.cases
            .iter()
            .any(|case| matches!(replay(case), Expect::Refusal(_))),
        "no case refuses a value"
    );
}
