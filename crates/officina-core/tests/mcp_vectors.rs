//! Replays `tests/vectors/mcp.json` (AD-90, AD-95, D-161) through
//! `mcp::route`: each line's route, the response it answers and the tool
//! call it asks for; and refuses a set that leaves a tool, an error or a
//! refusal without a case.
//!
//! A value over a bound is a stand-in the replay builds from the bound's
//! constant, never a typed string (D-655), and the answers that restate a
//! constant (the newest protocol version, the tool list) are taken from it.

use officina_core::mcp::{
    ArgsRefusal, MAX_INPUT_BYTES, MAX_LINE_BYTES, MCP_PROTOCOL_VERSIONS, McpError, McpTool, route,
    tools_list,
};
use officina_core::orders::proposal::{
    PROPOSAL_MAX_VALUES, PROPOSAL_NEW_SUBJECT_MAX_CHARS, PROPOSAL_TEXT_MAX_CHARS,
    PROPOSAL_VALUE_MAX_CHARS,
};
use officina_core::relay::view::RECENT_ENTRIES;
use serde_json::{Map, Value, json};

const VECTORS: &str = include_str!("../../../tests/vectors/mcp.json");

fn load() -> Value {
    let set: Value = serde_json::from_str(VECTORS).expect("the MCP vector set parses");
    assert_eq!(set["schema"], 1);
    set
}

fn over(chars: usize) -> Value {
    Value::from("a".repeat(chars + 1))
}

/// `value` with each stand-in replaced by what it stands for.
fn filled(value: &Value) -> Value {
    match value {
        Value::String(text) => match text.as_str() {
            "<newest protocol>" => Value::from(*MCP_PROTOCOL_VERSIONS.last().unwrap()),
            "<tools/list>" => tools_list(),
            "<RECENT_ENTRIES>" => json!(RECENT_ENTRIES),
            "<over RECENT_ENTRIES>" => json!(RECENT_ENTRIES + 1),
            "<at PROPOSAL_TEXT_MAX_CHARS>" => Value::from("é".repeat(PROPOSAL_TEXT_MAX_CHARS)),
            "<over PROPOSAL_TEXT_MAX_CHARS>" => over(PROPOSAL_TEXT_MAX_CHARS),
            "<over PROPOSAL_VALUE_MAX_CHARS>" => over(PROPOSAL_VALUE_MAX_CHARS),
            "<over PROPOSAL_NEW_SUBJECT_MAX_CHARS>" => over(PROPOSAL_NEW_SUBJECT_MAX_CHARS),
            "<over MAX_INPUT_BYTES>" => Value::from("a".repeat(MAX_INPUT_BYTES)),
            "<over PROPOSAL_MAX_VALUES>" => Value::Object(
                (0..=PROPOSAL_MAX_VALUES)
                    .map(|i| (format!("p{i}"), Value::from("v")))
                    .collect::<Map<_, _>>(),
            ),
            _ => value.clone(),
        },
        Value::Array(items) => Value::Array(items.iter().map(filled).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), filled(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The line a case sends: its `request` serialized, or its raw `line`.
fn line_of(case: &Value) -> Vec<u8> {
    match (&case["request"], case["line"].as_str()) {
        (Value::Null, Some("<over MAX_LINE_BYTES>")) => vec![b'a'; MAX_LINE_BYTES + 1],
        (Value::Null, Some(line)) => line.as_bytes().to_vec(),
        (request, None) => serde_json::to_vec(&filled(request)).unwrap(),
        _ => panic!("{}: a request or a line", case["name"]),
    }
}

/// AD-90, D-161: every case routes as it expects, and each route answers
/// the response its kind builds.
#[test]
fn framing_cases_match() {
    let set = load();
    let version = set["server_version"].as_str().unwrap();
    let cases = set["cases"].as_array().unwrap();
    assert!(!cases.is_empty());
    for case in cases {
        let routed = route(&line_of(case), version);
        let got = serde_json::to_value(&routed).unwrap();
        assert_eq!(got, filled(&case["expect"]), "case {}", case["name"]);
        match &got {
            Value::Object(map) if map.contains_key("error") => {
                let error = &routed.response().unwrap()["error"];
                let code = McpError::from_code(map["error"]["error"].as_str().unwrap()).unwrap();
                assert_eq!(error["code"], code.number(), "case {}", case["name"]);
                assert_eq!(error["message"], code.message());
            }
            Value::Object(map) if map.contains_key("refuse") => {
                let result = &routed.response().unwrap()["result"];
                assert_eq!(result["isError"], true, "case {}", case["name"]);
            }
            Value::Object(map) if map.contains_key("reply") => {
                let response = routed.response().unwrap();
                assert_eq!(response["jsonrpc"], "2.0");
                assert_eq!(response["result"], map["reply"]["result"]);
            }
            _ => assert_eq!(routed.response(), None, "case {}", case["name"]),
        }
    }
}

/// D-161, D-655: every tool is called by some case, every error and every
/// refusal answered by some case.
#[test]
fn every_tool_and_error_has_a_case() {
    let set = load();
    let expects: Vec<Value> = set["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case["expect"].clone())
        .collect();
    for tool in McpTool::ALL {
        assert!(
            expects
                .iter()
                .any(|e| e["call"]["args"]["tool"] == tool.code()),
            "{tool:?} has no call case in tests/vectors/mcp.json"
        );
    }
    for error in McpError::ALL {
        assert!(
            expects.iter().any(|e| e["error"]["error"] == error.code()),
            "{error:?} has no case in tests/vectors/mcp.json"
        );
    }
    for refusal in ArgsRefusal::ALL {
        assert!(
            expects
                .iter()
                .any(|e| e["refuse"]["refusal"] == refusal.code()),
            "{refusal:?} has no case in tests/vectors/mcp.json"
        );
    }
}
