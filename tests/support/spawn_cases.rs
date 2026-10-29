//! The golden spawn vectors (AD-79): `tests/vectors/spawn.json`, each case
//! the inputs of one kind's builder and the vector it must build. Included
//! by `#[path]` from the core and CLI test crates, so both replay the same
//! cases through the same builders. Read as plain JSON, since the CLI test
//! crate has no serde derive.

#![allow(dead_code)] // each test crate uses a different subset

use officina_core::orders::work_order::WorkOrder;
use officina_core::registry::triage_call::schema;
use officina_core::relay::catalogue::{self, ApprovedCatalogue};
use officina_core::relay::template::{
    ApprovedTemplate, ModelAlias, ParamName, ProjectName, TemplateName,
};
use officina_core::spawn::{self, BackgroundId, ClaudeVector, McpServerInputs, VectorKind};
use serde_json::Value;

pub const VECTORS: &str = include_str!("../vectors/spawn.json");

/// The keys a case's `input` may hold; each kind reads the ones it takes.
const INPUT_KEYS: &[&str] = &[
    "config_dir",
    "background_id",
    "model",
    "schema",
    "officina_home",
    "floor",
    "floor_path",
    "template",
    "project",
    "values",
    "seq",
    "officina_exe",
    "floor_file",
    "no_excerpts",
    "claude_bin",
    "answer",
    "session_id",
    "folder",
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

    /// The work order of the case's floor, approved as read: its template
    /// in its project with its values. With `home`, the floor's folder is
    /// `home`, so a relative project folder lies under it.
    pub fn work_order(&self, home: Option<&str>) -> WorkOrder {
        let floor_path = match home {
            Some(home) => {
                let file = self.required("floor_path").rsplit(['/', '\\']).next();
                format!("{home}/{}", file.unwrap_or_default())
            }
            None => self.required("floor_path").to_owned(),
        };
        let current = catalogue::load(self.required("floor"), &floor_path)
            .unwrap_or_else(|e| panic!("case {}: floor: {e}", self.name));
        assert!(
            current.catalogue.issues.is_empty(),
            "case {}: {:?}",
            self.name,
            current.catalogue.issues
        );
        let approved = ApprovedCatalogue::approving(&current, &floor_path, 0);
        let name = TemplateName::parse(self.required("template")).expect("a template name");
        let template = ApprovedTemplate::new(&approved, &current, &name)
            .unwrap_or_else(|e| panic!("case {}: {e:?}", self.name));
        let project = ProjectName::parse(self.required("project")).expect("a project name");
        let values: Vec<(ParamName, &str)> = self.input["values"]
            .as_array()
            .unwrap_or_else(|| panic!("case {}: no values", self.name))
            .iter()
            .map(|pair| {
                let name = pair[0].as_str().and_then(ParamName::parse);
                let value = pair[1].as_str();
                match (name, value) {
                    (Some(name), Some(value)) => (name, value),
                    _ => panic!("case {}: a value is a [name, value] pair", self.name),
                }
            })
            .collect();
        WorkOrder::validate(&template, &project, &values)
            .unwrap_or_else(|e| panic!("case {}: {e:?}", self.name))
    }

    /// The case's expected arguments with its Officina home replaced by
    /// `home`, as [`Case::build`] replaces it: the orchestrator's MCP
    /// config names the home inside its JSON argument (D-128), escaped as
    /// JSON escapes it.
    pub fn expected_args(&self, home: Option<&str>) -> Vec<String> {
        let args: Vec<String> =
            serde_json::from_value(self.expect["args"].clone()).expect("args are strings");
        match (self.kind(), home) {
            (VectorKind::Orchestrator, Some(home)) => {
                let quoted = |text: &str| Value::from(text).to_string();
                let from = quoted(self.required("officina_home"));
                let to = quoted(home);
                args.into_iter()
                    .map(|arg| arg.replace(&from, &to))
                    .collect()
            }
            _ => args,
        }
    }

    /// The vector the kind's builder makes from the case's inputs, with
    /// the Officina home replaced by `home` when one is given (for a work
    /// order, the floor's folder: [`Case::work_order`]).
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
            VectorKind::WorkOrder => {
                let seq = self.input["seq"]
                    .as_u64()
                    .and_then(|seq| u32::try_from(seq).ok())
                    .unwrap_or_else(|| panic!("case {}: no seq", self.name));
                spawn::work_order(&self.work_order(home), seq, config_dir)
            }
            VectorKind::ReviewCall => {
                let model = ModelAlias::from_code(self.required("model"))
                    .unwrap_or_else(|| panic!("case {}: unknown model", self.name));
                assert_eq!(self.required("schema"), "review", "case {}", self.name);
                let officina_home = home.unwrap_or_else(|| self.required("officina_home"));
                spawn::review_call(
                    model,
                    &officina_core::relay::auto::schema(),
                    config_dir,
                    officina_home,
                )
            }
            VectorKind::Orchestrator => {
                let model = self.text("model").map(|code| {
                    ModelAlias::from_code(code)
                        .unwrap_or_else(|| panic!("case {}: unknown model", self.name))
                });
                let server = McpServerInputs {
                    officina_exe: self.required("officina_exe"),
                    officina_home: home.unwrap_or_else(|| self.required("officina_home")),
                    floor: self.text("floor_file"),
                    no_excerpts: self.input["no_excerpts"]
                        .as_bool()
                        .unwrap_or_else(|| panic!("case {}: no no_excerpts", self.name)),
                    claude_bin: self.required("claude_bin"),
                };
                spawn::orchestrator(&server, model, config_dir)
            }
            VectorKind::Logs => {
                let id = BackgroundId::parse(self.required("background_id"))
                    .unwrap_or_else(|| panic!("case {}: invalid background_id", self.name));
                spawn::logs(&id, config_dir)
            }
            VectorKind::Answer => {
                let answer = officina_core::meeting::Answer::parse(self.required("answer"))
                    .unwrap_or_else(|e| panic!("case {}: answer {e:?}", self.name));
                let session = spawn::SessionId::parse(self.required("session_id"))
                    .unwrap_or_else(|| panic!("case {}: invalid session_id", self.name));
                // With a home, the session's folder is its last segment
                // under it, so a test can create it.
                let folder = match home {
                    Some(home) => {
                        let last = self.required("folder").rsplit(['/', '\\']).next();
                        format!("{home}/{}", last.unwrap_or_default())
                    }
                    None => self.required("folder").to_owned(),
                };
                spawn::answer(&answer, &session, &folder, config_dir)
            }
        }
    }
}
