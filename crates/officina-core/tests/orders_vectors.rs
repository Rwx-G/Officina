//! Replays the `validate` section of the work order vector set (AD-77,
//! FR-3) through `ApprovedTemplate::new` and `WorkOrder::validate`, and
//! refuses a set that leaves a refusal without a case.
//!
//! Every case reads the set's floor, approves it, and validates; a case
//! with `snapshot_template` or `over_limit` first edits the approved
//! snapshot's template, its hash kept, as a record edited by hand would
//! keep it: what `validate` checks beyond the closed lists is what such a
//! snapshot can carry.

use officina_core::orders::work_order::{WorkOrder, WorkOrderRefusal};
use officina_core::relay::catalogue::{ApprovedCatalogue, CanonicalTemplate, RelayFloor, load};
use officina_core::relay::template::{
    ApprovedTemplate, MAX_PROMPT_CHARS, ParamName, ProjectName, TemplateName, render,
};
use serde::Deserialize;
use serde_json::Value;

const VECTORS: &str = include_str!("../../../tests/vectors/orders.json");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VectorSet {
    schema: u32,
    floor_path: String,
    floor: String,
    validate: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    template: String,
    project: String,
    values: Vec<(String, String)>,
    /// Replaces the approved snapshot's template of that name.
    #[serde(default)]
    snapshot_template: Option<CanonicalTemplate>,
    /// A parameter whose snapshot list gains a value just long enough for
    /// the rendering to be one character over `MAX_PROMPT_CHARS`, given as
    /// its value.
    #[serde(default)]
    over_limit: Option<String>,
    expect: Expect,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Expect {
    /// The order as it serializes, without `catalogue_hash`, which is the
    /// floor's (pinned by `catalogue_hash.rs`, not here).
    Order(Value),
    Refused(WorkOrderRefusal),
}

fn load_set() -> VectorSet {
    let set: VectorSet = serde_json::from_str(VECTORS).expect("the order vector set parses");
    assert_eq!(set.schema, 1);
    set
}

fn current(set: &VectorSet) -> RelayFloor {
    let relay = load(&set.floor, &set.floor_path).expect("the set's floor parses");
    assert!(
        relay.catalogue.issues.is_empty(),
        "the set's floor is fully valid: {:?}",
        relay.catalogue.issues
    );
    relay
}

/// The value of `prompt` rendered over the limit by one character, every
/// other placeholder filled with `values`'s.
fn over_limit_value(prompt: &str, param: &str, values: &[(String, String)]) -> String {
    let mut filled: std::collections::BTreeMap<String, String> = values.iter().cloned().collect();
    filled.insert(param.to_owned(), String::new());
    let base = render(prompt, &filled).expect("the prompt renders");
    "a".repeat(MAX_PROMPT_CHARS + 1 - base.chars().count())
}

/// What `case` gives: the expected form of the order or the refusal.
fn replay(set: &VectorSet, case: &Case) -> Expect {
    let current = current(set);
    let mut approved = ApprovedCatalogue::approving(&current, &set.floor_path, 0);
    let mut values = case.values.clone();
    let snapshot = approved
        .canonical
        .templates
        .get_mut(&case.template)
        .unwrap_or_else(|| panic!("{}: no template {}", case.name, case.template));
    if let Some(template) = &case.snapshot_template {
        *snapshot = template.clone();
    }
    if let Some(param) = &case.over_limit {
        let long = over_limit_value(&snapshot.prompt, param, &values);
        snapshot.params.get_mut(param).unwrap().push(long.clone());
        for (name, value) in &mut values {
            if name == param {
                *value = long.clone();
            }
        }
    }
    let name = TemplateName::parse(&case.template).unwrap();
    let template = ApprovedTemplate::new(&approved, &current, &name)
        .unwrap_or_else(|e| panic!("{}: {e:?}", case.name));
    let project = ProjectName::parse(&case.project).unwrap();
    let pairs: Vec<(ParamName, &str)> = values
        .iter()
        .map(|(name, value)| {
            let name = ParamName::parse(name)
                .unwrap_or_else(|| panic!("{}: parameter name {name}", case.name));
            (name, value.as_str())
        })
        .collect();
    match WorkOrder::validate(&template, &project, &pairs) {
        Err(refusal) => Expect::Refused(refusal),
        Ok(order) => {
            assert_eq!(order.catalogue_hash(), current.hash, "{}", case.name);
            let mut form = serde_json::to_value(&order).unwrap();
            let removed = form.as_object_mut().unwrap().remove("catalogue_hash");
            assert_eq!(removed, Some(Value::from(current.hash.clone())));
            Expect::Order(form)
        }
    }
}

#[test]
fn validate_cases_match() {
    let set = load_set();
    assert!(!set.validate.is_empty());
    for case in &set.validate {
        assert_eq!(replay(&set, case), case.expect, "case: {}", case.name);
    }
}

#[test]
fn every_refusal_has_a_case() {
    let set = load_set();
    for refusal in WorkOrderRefusal::ALL {
        assert!(
            set.validate
                .iter()
                .any(|case| case.expect == Expect::Refused(*refusal)),
            "{refusal:?} has no case in tests/vectors/orders.json"
        );
    }
    assert!(
        set.validate
            .iter()
            .any(|case| matches!(case.expect, Expect::Order(_))),
        "no case validates"
    );
}

/// Each case's expected order holds the rendered prompt's digest, and a
/// valid order's values are entries of the floor's own lists.
#[test]
fn every_expected_order_is_consistent() {
    let set = load_set();
    let current = current(&set);
    for case in &set.validate {
        let Expect::Order(order) = &case.expect else {
            continue;
        };
        let prompt = order["prompt"].as_str().unwrap();
        assert_eq!(
            order["prompt_sha256"].as_str(),
            Some(officina_core::sha256_hex(prompt.as_bytes()).as_str()),
            "{}",
            case.name
        );
        let lists = &current.canonical.templates[&case.template].params;
        for (name, value) in order["values"].as_object().unwrap() {
            assert!(
                lists[name]
                    .iter()
                    .any(|entry| Some(entry.as_str()) == value.as_str()),
                "{}: {name}",
                case.name
            );
        }
    }
}
