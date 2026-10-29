//! Replays the `validate` section of the work order vector set (AD-77,
//! FR-3) through `ApprovedTemplate::new` and `WorkOrder::validate`, and
//! its `card` section (AD-78, story 5.7) through `card::issue` and
//! `card::check_launch`, and refuses a set that leaves a refusal without a
//! case.
//!
//! Every case reads the set's floor, approves it, and validates; a case
//! with `snapshot_template` or `over_limit` first edits the approved
//! snapshot's template, its hash kept, as a record edited by hand would
//! keep it: what `validate` checks beyond the closed lists is what such a
//! snapshot can carry.

use officina_core::orders::card::{
    CardInputs, CardRequest, CardState, LaunchFacts, LaunchRefusal, WorkOrderCard, check_launch,
    issue,
};
use officina_core::orders::work_order::{WorkOrder, WorkOrderRefusal};
use officina_core::registry::types::{Source, Sourced, SubjectRecord, SubjectStatus};
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
    card: Vec<CardCase>,
}

/// One case of the `card` section: an issue, compared with the expected
/// card's fields or refusal, or a launch's checks on the first issue
/// case's card with the facts `launch` changes.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CardCase {
    name: String,
    #[serde(default)]
    issue: Option<CardRequest>,
    #[serde(default)]
    seq: u32,
    #[serde(default)]
    subjects: Vec<SubjectRow>,
    #[serde(default)]
    launch: Option<LaunchCase>,
    expect: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubjectRow {
    id: i64,
    project: String,
    name: String,
    status: SubjectStatus,
}

/// What differs from a launch every check passes: the card's state or an
/// unknown id, the clicked hash, either catalogue hash (`null`: none), and
/// the host's findings.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct LaunchCase {
    #[serde(default)]
    card: Option<String>,
    #[serde(default)]
    clicked: Option<String>,
    #[serde(default, deserialize_with = "present")]
    approved: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    current: Option<Option<String>>,
    #[serde(default)]
    template_approved: Option<bool>,
    #[serde(default)]
    claude_native: Option<bool>,
    #[serde(default)]
    folder_exists: Option<bool>,
    #[serde(default)]
    subject_open: Option<bool>,
    #[serde(default)]
    name_taken: Option<bool>,
}

/// A key given, `null` included, as `Some`; absent as `None`.
fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(Some)
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

fn subject_records(rows: &[SubjectRow]) -> Vec<SubjectRecord> {
    rows.iter()
        .map(|row| SubjectRecord {
            id: row.id,
            project: row.project.clone(),
            name: row.name.clone(),
            status: Sourced {
                value: row.status,
                src: Source::Author,
                at_ms: 0,
            },
            created_at_ms: 0,
            routine: false,
            merged_into: None,
        })
        .collect()
}

const CARD_ID: &str = "0123456789abcdef0123456789abcdef";

/// `request` issued with `seq` and `subjects` on the set's floor, approved.
fn issued(
    set: &VectorSet,
    seq: u32,
    request: &CardRequest,
    subjects: &[SubjectRecord],
) -> Result<officina_core::orders::card::IssuedCard, officina_core::orders::card::IssueRefusal> {
    let current = current(set);
    let approved = ApprovedCatalogue::approving(&current, &set.floor_path, 0);
    issue(
        CARD_ID,
        seq,
        request,
        &CardInputs {
            approved: &approved,
            current: &current,
            subjects,
            quota: None,
            config_dir: None,
            now_ms: 0,
        },
    )
}

/// The card or the refusal `case` gives, as JSON: the card's fields the
/// expectation names only, so a case states what it is about.
fn replay_issue(set: &VectorSet, case: &CardCase, request: &CardRequest) -> Value {
    match issued(set, case.seq, request, &subject_records(&case.subjects)) {
        Err(refusal) => serde_json::json!({"refused": refusal}),
        Ok(issued) => {
            assert_eq!(
                issued.card().argv,
                officina_core::spawn::work_order(issued.order(), case.seq, None).args(),
                "{}: the card's argv is the vector spawn builds",
                case.name
            );
            let card = serde_json::to_value(issued.card()).unwrap();
            let wanted = case.expect["card"].as_object().cloned().unwrap_or_default();
            let fields: serde_json::Map<String, Value> = wanted
                .keys()
                .map(|key| (key.clone(), card[key].clone()))
                .collect();
            serde_json::json!({ "card": fields })
        }
    }
}

/// The first issue case's card, which every launch case checks.
fn launch_card(set: &VectorSet) -> WorkOrderCard {
    let case = set
        .card
        .iter()
        .find(|case| case.issue.is_some() && case.expect.get("card").is_some())
        .expect("an issue case that issues");
    issued(set, case.seq, case.issue.as_ref().unwrap(), &[])
        .unwrap()
        .card()
        .clone()
}

/// What `launch` gives on `card`: `"launched"` or the refusal.
fn replay_launch(card: &WorkOrderCard, launch: &LaunchCase) -> Value {
    let other = "f".repeat(64);
    let hash = |given: &Option<Option<String>>| -> Option<String> {
        match given {
            None => Some(card.catalogue_hash.clone()),
            Some(None) => None,
            Some(Some(_)) => Some(other.clone()),
        }
    };
    let mut stated = card.clone();
    if let Some(state) = launch.card.as_deref().filter(|c| *c != "unknown") {
        stated.state = CardState::from_code(state).unwrap_or_else(|| panic!("card state {state}"));
    }
    let (approved, current) = (hash(&launch.approved), hash(&launch.current));
    let clicked = match &launch.clicked {
        None => card.prompt_sha256.clone(),
        Some(_) => other.clone(),
    };
    let facts = LaunchFacts {
        card: (launch.card.as_deref() != Some("unknown")).then_some(&stated),
        clicked_sha256: &clicked,
        approved_hash: approved.as_deref(),
        current_hash: current.as_deref(),
        template_approved: launch.template_approved.unwrap_or(true),
        claude_native: launch.claude_native.unwrap_or(true),
        folder_exists: launch.folder_exists.unwrap_or(true),
        subject_open: launch.subject_open.unwrap_or(true),
        name_taken: launch.name_taken.unwrap_or(false),
    };
    match check_launch(&facts) {
        Ok(()) => Value::from("launched"),
        Err(refusal) => serde_json::json!({ "refused": refusal }),
    }
}

#[test]
fn card_cases_match() {
    let set = load_set();
    assert!(!set.card.is_empty());
    let card = launch_card(&set);
    for case in &set.card {
        let got = match (&case.issue, &case.launch) {
            (Some(request), None) => replay_issue(&set, case, request),
            (None, Some(launch)) => replay_launch(&card, launch),
            _ => panic!("{}: an issue or a launch", case.name),
        };
        assert_eq!(got, case.expect, "case: {}", case.name);
    }
}

#[test]
fn every_launch_refusal_has_a_case() {
    let set = load_set();
    for refusal in LaunchRefusal::ALL {
        let wanted = serde_json::json!({ "refused": refusal });
        assert!(
            set.card
                .iter()
                .any(|case| case.launch.is_some() && case.expect == wanted),
            "{refusal:?} has no launch case in tests/vectors/orders.json"
        );
    }
    assert!(
        set.card
            .iter()
            .any(|case| case.launch.is_some() && case.expect == "launched"),
        "no launch passes"
    );
}
