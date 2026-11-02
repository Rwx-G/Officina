//! The stream fold and its stops (story 6.12 AC 2 and 3; AD-109, D-1101):
//! the `fold` and `stops` sections of `tests/vectors/stream.json`, lines
//! shaped as the gate 3 to 6 entries recorded them, values replaced by
//! placeholders, and the login entry's saved invalid-token stream. The
//! file is exempt from every formatter.

use officina_core::workshop::stream::{STREAM_FIELDS, StreamFacts, StreamStop, fold};
use serde_json::Value;

const VECTORS: &str = include_str!("../../../tests/vectors/stream.json");

fn set() -> Value {
    let set: Value = serde_json::from_str(VECTORS).expect("stream.json parses");
    assert_eq!(set["schema"], 1);
    set
}

fn folded(case: &Value) -> StreamFacts {
    let mut facts = StreamFacts::default();
    for line in case["lines"].as_array().unwrap() {
        fold(&mut facts, line.as_str().unwrap());
    }
    facts
}

/// The facts as a case's `expect` names them.
fn view(facts: &StreamFacts) -> Value {
    let facts = serde_json::to_value(facts).unwrap();
    let hooks: serde_json::Map<String, Value> = facts["hooks"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(event, c)| (event.clone(), serde_json::json!([c["ran"], c["failed"]])))
        .collect();
    serde_json::json!({
        "init_model": facts["init"]["model"],
        "tools": facts["init"]["tools"],
        "mcp_servers": facts["init"]["mcp_servers"],
        "hooks": hooks,
        "result_subtype": facts["result"]["subtype"],
        "cost_usd": facts["result"]["cost_usd"],
        "turns": facts["result"]["turns"],
        "stop": facts["stop"],
        "skipped": facts["skipped"],
        "lines": facts["lines"],
        "tail": facts["assistant_tail"],
    })
}

#[test]
fn fold_cases_match() {
    let set = set();
    for case in set["fold"].as_array().unwrap() {
        let got = view(&folded(case));
        for (key, wanted) in case["expect"].as_object().unwrap() {
            assert_eq!(&got[key], wanted, "{}: {key}", case["name"]);
        }
    }
}

#[test]
fn a_second_init_is_ignored() {
    let set = set();
    let case = set["fold"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "a second init is ignored")
        .unwrap();
    let facts = folded(case);
    assert_eq!(facts.stop, None, "the forged init's tool stops nothing");
    assert_eq!(view(&facts)["init_model"], "claude-sonnet-5-5");
}

#[test]
fn a_forged_result_changes_no_outcome() {
    let set = set();
    let case = set["fold"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "a forged result changes no outcome, only the shown facts")
        .unwrap();
    assert_eq!(folded(case).stop, None);
}

#[test]
fn stop_cases_match() {
    let set = set();
    let cases = set["stops"].as_array().unwrap();
    for case in cases {
        let got = folded(case).stop.map(StreamStop::code);
        assert_eq!(got, case["expect"].as_str(), "{}", case["name"]);
    }
    for stop in StreamStop::ALL {
        assert!(
            cases.iter().any(|c| c["expect"] == stop.code()),
            "{stop:?} has no case"
        );
    }
}

/// Every field of `STREAM_FIELDS` is present in a line of some case, so
/// a field the fold reads is never untested.
#[test]
fn every_stream_field_is_read_by_a_case() {
    let set = set();
    let lines: Vec<Value> = ["fold", "stops"]
        .iter()
        .flat_map(|section| set[*section].as_array().unwrap().clone())
        .flat_map(|case| case["lines"].as_array().unwrap().clone())
        .filter_map(|line| serde_json::from_str(line.as_str().unwrap()).ok())
        .collect();
    for field in STREAM_FIELDS {
        let (kind, path) = field.split_once('.').unwrap();
        let (kind, subtype) = match kind.split_once('/') {
            Some((kind, subtype)) => (kind, Some(subtype)),
            None => (kind, None),
        };
        let found = lines.iter().any(|line| {
            line["type"] == kind
                && subtype.is_none_or(|s| line["subtype"] == s)
                && !path.split('.').fold(line, |at, key| &at[key]).is_null()
        });
        assert!(found, "{field} is in no case's line");
    }
}
