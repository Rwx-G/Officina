//! The golden spawn vectors (AD-79): `tests/vectors/spawn.json`, each case
//! the inputs of one kind's builder and the vector it must build. Included
//! by `#[path]` from the core and CLI test crates, so both replay the same
//! cases through the same builders. Read as plain JSON, since the CLI test
//! crate has no serde derive.

#![allow(dead_code)] // each test crate uses a different subset

use officina_core::registry::triage_call::schema;
use officina_core::relay::template::ModelAlias;
use officina_core::spawn::{self, BackgroundId, ClaudeVector, VectorKind};
use serde_json::Value;

pub const VECTORS: &str = include_str!("../vectors/spawn.json");

/// The keys a case's `input` may hold; each kind reads the ones it takes.
const INPUT_KEYS: &[&str] = &[
    "config_dir",
    "background_id",
    "model",
    "schema",
    "officina_home",
];

#[derive(Debug)]
pub struct VectorSet {
    pub cases: Vec<Case>,
}

#[derive(Debug)]
pub struct Case {
    pub name: String,
    /// A builder's inputs.
    pub input: serde_json::Map<String, Value>,
    /// The vector as it serializes: kind, arguments, directory rule,
    /// console, stdin, capture, deadline, variables set and rule.
    pub expect: Value,
}

pub fn load() -> VectorSet {
    let set: Value = serde_json::from_str(VECTORS).expect("the spawn vector set parses");
    assert_eq!(set["schema"], 1);
    let cases = set["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .map(|case| {
            let name = case["name"].as_str().expect("a case name").to_owned();
            let input = case["input"].as_object().expect("an input object").clone();
            for key in input.keys() {
                assert!(
                    INPUT_KEYS.contains(&key.as_str()),
                    "case {name}: input {key}"
                );
            }
            let object = case.as_object().expect("a case object");
            assert!(
                object
                    .keys()
                    .all(|k| ["name", "input", "expect"].contains(&k.as_str())),
                "case {name}: unknown key"
            );
            Case {
                name,
                input,
                expect: case["expect"].clone(),
            }
        })
        .collect();
    VectorSet { cases }
}

impl Case {
    /// The case's kind, read from its expected vector.
    pub fn kind(&self) -> VectorKind {
        self.expect["kind"]
            .as_str()
            .and_then(VectorKind::from_code)
            .unwrap_or_else(|| panic!("case {}: no known kind", self.name))
    }

    fn text(&self, key: &str) -> Option<&str> {
        self.input.get(key).and_then(Value::as_str)
    }

    fn required(&self, key: &str) -> &str {
        self.text(key)
            .unwrap_or_else(|| panic!("case {}: no {key}", self.name))
    }

    /// The vector the kind's builder makes from the case's inputs, with
    /// the Officina home replaced by `home` when one is given.
    pub fn build(&self, home: Option<&str>) -> ClaudeVector {
        let config_dir = self.text("config_dir");
        match self.kind() {
            VectorKind::Agents => spawn::agents(config_dir),
            VectorKind::Version => spawn::version(config_dir),
            VectorKind::Attach => {
                let id = BackgroundId::parse(self.required("background_id"))
                    .unwrap_or_else(|| panic!("case {}: invalid background_id", self.name));
                spawn::attach(&id, config_dir)
            }
            VectorKind::TriageCall => {
                let model = ModelAlias::from_code(self.required("model"))
                    .unwrap_or_else(|| panic!("case {}: unknown model", self.name));
                assert_eq!(self.required("schema"), "triage", "case {}", self.name);
                let officina_home = home.unwrap_or_else(|| self.required("officina_home"));
                spawn::triage_call(model, &schema(), config_dir, officina_home)
            }
        }
    }
}
