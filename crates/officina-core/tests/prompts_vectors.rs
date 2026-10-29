//! Replays the hand-written `PermissionRequest` vector set (AD-181, story
//! 5.22 AC 1): each payload gives the request `from_payload` makes, or
//! none; and the hook's output, for every behaviour, holds exactly the
//! property names the set pins, at every depth, so a field such as
//! `updatedPermissions` cannot join it unnoticed.

use std::collections::BTreeSet;

use officina_core::relay::prompts::{
    INPUT_CAP_BYTES, PROMPT_WAIT_MS, PromptBehavior, PromptOutput, PromptRequest, canonical_json,
    input_sha256,
};
use serde::Deserialize;

const VECTORS: &str = include_str!("../../../tests/vectors/prompts.json");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VectorSet {
    schema: u32,
    #[serde(rename = "_doc")]
    _doc: String,
    payload_cases: Vec<PayloadCase>,
    output_properties: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PayloadCase {
    name: String,
    payload: serde_json::Value,
    #[serde(default)]
    pad: Option<Pad>,
    expect: Option<Expected>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pad {
    char: char,
    count: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Expected {
    session_id: String,
    #[serde(default)]
    tool_name: Option<String>,
    #[serde(default)]
    permission_mode: Option<String>,
    #[serde(default)]
    canonical: Option<String>,
    #[serde(default)]
    kept_bytes: Option<usize>,
}

fn load() -> VectorSet {
    let set: VectorSet = serde_json::from_str(VECTORS).expect("vector set parses");
    assert_eq!(set.schema, 1);
    set
}

fn padded(case: &PayloadCase) -> serde_json::Value {
    let mut payload = case.payload.clone();
    if let Some(pad) = &case.pad {
        let command = &mut payload["tool_input"]["command"];
        let text = format!(
            "{}{}",
            command.as_str().unwrap_or_default(),
            pad.char.to_string().repeat(pad.count)
        );
        *command = text.into();
    }
    payload
}

#[test]
fn payload_cases_match() {
    let set = load();
    for case in &set.payload_cases {
        let payload = padded(case);
        let request = PromptRequest::from_payload(&payload, Some(4242), 1, 1_000, PROMPT_WAIT_MS);
        let Some(expected) = &case.expect else {
            assert!(request.is_none(), "case: {}", case.name);
            continue;
        };
        let request = request.unwrap_or_else(|| panic!("case: {} made no request", case.name));
        let input = payload
            .get("tool_input")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        assert_eq!(
            request.session_id, expected.session_id,
            "case: {}",
            case.name
        );
        assert_eq!(request.tool_name, expected.tool_name, "case: {}", case.name);
        assert_eq!(
            request.permission_mode, expected.permission_mode,
            "case: {}",
            case.name
        );
        assert_eq!(request.claude_pid, Some(4242));
        assert_eq!(request.deadline_ms, 1_000 + PROMPT_WAIT_MS);
        assert_eq!(
            request.input_sha256,
            input_sha256(&input),
            "case: {}",
            case.name
        );
        let kept = serde_json::to_value(&request).unwrap()["tool_input"]
            .as_str()
            .unwrap()
            .to_owned();
        if let Some(canonical) = &expected.canonical {
            assert_eq!(&canonical_json(&input), canonical, "case: {}", case.name);
            assert_eq!(&kept, canonical, "case: {}", case.name);
        }
        if let Some(bytes) = expected.kept_bytes {
            assert_eq!(kept.len(), bytes, "case: {}", case.name);
            assert!(canonical_json(&input).len() > INPUT_CAP_BYTES);
            assert!(canonical_json(&input).starts_with(&kept));
        }
    }
    // The set covers both outcomes and the cap.
    assert!(set.payload_cases.iter().any(|c| c.expect.is_none()));
    assert!(
        set.payload_cases
            .iter()
            .any(|c| c.expect.as_ref().is_some_and(|e| e.kept_bytes.is_some()))
    );
}

fn property_paths(value: &serde_json::Value, prefix: &str, out: &mut BTreeSet<String>) {
    if let serde_json::Value::Object(map) = value {
        for (key, inner) in map {
            let path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            property_paths(inner, &path, out);
            out.insert(path);
        }
    }
}

#[test]
fn the_output_schema_is_pinned() {
    let mut serialized = BTreeSet::new();
    for behavior in PromptBehavior::ALL {
        let value: serde_json::Value =
            serde_json::from_str(&PromptOutput::of(*behavior).to_json()).unwrap();
        property_paths(&value, "", &mut serialized);
    }
    let pinned: BTreeSet<String> = load().output_properties.into_iter().collect();
    assert_eq!(serialized, pinned);
    for forbidden in ["updatedPermissions", "updatedInput", "interrupt"] {
        assert!(
            !pinned
                .iter()
                .any(|p| p.split('.').any(|name| name == forbidden)),
            "{forbidden} pinned"
        );
    }
}
