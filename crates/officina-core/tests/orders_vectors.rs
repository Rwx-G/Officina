//! Replays the `validate` section of the work order vector set (AD-77,
//! FR-3) through `ApprovedTemplate::new` and `WorkOrder::validate`, and
//! its `card` section (AD-78, story 5.7) through `card::issue` and
//! `card::check_launch`, and its `briefing` section (AD-83, story 5.8)
//! through `briefing::build`, and refuses a set that leaves a refusal, a
//! change kind or a run kind without a case.
//!
//! Every case reads the set's floor, approves it, and validates; a case
//! with `snapshot_template` or `over_limit` first edits the approved
//! snapshot's template, its hash kept, as a record edited by hand would
//! keep it: what `validate` checks beyond the closed lists is what such a
//! snapshot can carry.

use std::collections::BTreeMap;

use officina_core::board::{Card, Column, Columns};
use officina_core::orders::briefing::{
    self, BRIEFING_MAX_BYTES, Briefing, BriefingInput, ChangeKind, ChangedItem, Cursor, RunKind,
    Section,
};
use officina_core::orders::card::{
    CardInputs, CardRequest, CardState, LaunchFacts, LaunchRefusal, WorkOrderCard, check_launch,
    issue,
};
use officina_core::orders::work_order::{WorkOrder, WorkOrderRefusal};
use officina_core::registry::types::{
    LaunchOutcome, Source, Sourced, SubjectRecord, SubjectStatus, WorkOrderRecord, WorkOrderSource,
};
use officina_core::registry::view::{
    EndHow, EndView, LiveCard, SessionView, SubjectView, TriageRow,
};
use officina_core::relay::approvals::ApprovalView;
use officina_core::relay::catalogue::{ApprovedCatalogue, CanonicalTemplate, RelayFloor, load};
use officina_core::relay::template::{
    ApprovedTemplate, MAX_PROMPT_CHARS, ParamName, ProjectName, TemplateName, render,
};
use officina_core::relay::view::{RecentEntry, RecentEvent, tone_of};
use officina_core::untrusted::Untrusted;
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
    briefing: Vec<BriefingCase>,
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

// The `briefing` section (AD-83). Each case gives the payloads as compact
// rows, which the harness turns into the window's types with every field a
// row does not name at a neutral value, and the sections as identifiers:
// a subject's id, a card's key, a request's id, a triage row's key, a run
// item's kind, an order's id, and a change as `kind:identifier`.

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BriefingCase {
    name: String,
    now_ms: i64,
    cursor: Cursor,
    #[serde(default)]
    cards: Vec<CardRow>,
    #[serde(default)]
    approvals: Vec<TimedRow>,
    #[serde(default)]
    runs: Vec<RunRow>,
    #[serde(default)]
    subjects: Vec<OpenSubjectRow>,
    #[serde(default)]
    triage: Vec<TriageRowCase>,
    #[serde(default)]
    subject_rows: Vec<SubjectStatusRow>,
    #[serde(default)]
    orders: Vec<OrderRow>,
    /// Needs you cards the harness adds, each a step newer than the last,
    /// with names of `name_chars` characters: the bound's input.
    #[serde(default)]
    crowd: Option<Crowd>,
    expect: BTreeMap<String, ExpectedSection>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CardRow {
    key: String,
    column: Column,
    name: String,
    at: Option<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TimedRow {
    id: String,
    at: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RunRow {
    ts: i64,
    /// A run item's kind, or `succeeded`.
    kind: String,
    #[serde(default)]
    action: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenSubjectRow {
    id: i64,
    name: String,
    at: i64,
    sessions: Vec<SessionRow>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionRow {
    id: String,
    #[serde(default)]
    card: Option<Column>,
    #[serde(default)]
    ended: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TriageRowCase {
    key: String,
    at: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubjectStatusRow {
    id: i64,
    status: SubjectStatus,
    at: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrderRow {
    id: String,
    at: i64,
    session: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Crowd {
    cards: usize,
    name_chars: usize,
    first_at: i64,
    step_ms: i64,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedSection {
    count: u32,
    dropped: u32,
    /// Omitted where the case checks the bound's property instead.
    #[serde(default)]
    items: Option<Vec<String>>,
}

/// When every subject row of the set was created.
const CREATED_AT_MS: i64 = 1_799_964_000_000;

fn card_of(row: &CardRow) -> Card {
    serde_json::from_value(serde_json::json!({
        "key": row.key,
        "column": row.column,
        "name": row.name,
        "project": { "name": "backend", "color": null, "priority": "normal" },
        "session_id": format!("s-{}", row.key),
        "last_event_at_ms": row.at,
        "attach_available": false,
        "subagents": [],
    }))
    .expect("a card")
}

fn crowd_key(index: usize) -> String {
    format!("k{index:04}")
}

fn columns_of(case: &BriefingCase) -> Columns {
    let mut columns = Columns::default();
    let crowd = case.crowd.iter().flat_map(|crowd| {
        (0..crowd.cards).map(move |i| CardRow {
            key: crowd_key(i),
            column: Column::NeedsYou,
            name: "n".repeat(crowd.name_chars),
            at: Some(crowd.first_at + i64::try_from(i).unwrap() * crowd.step_ms),
        })
    });
    let rows: Vec<CardRow> = crowd.collect();
    for card in case.cards.iter().chain(&rows).map(card_of) {
        match card.column {
            Column::NeedsYou => columns.needs_you.push(card),
            Column::YourTurn => columns.your_turn.push(card),
            Column::Working => columns.working.push(card),
            Column::ToResume => columns.to_resume.push(card),
            Column::Done => columns.done.push(card),
        }
    }
    columns
}

fn approval_of(row: &TimedRow) -> ApprovalView {
    serde_json::from_value(serde_json::json!({
        "id": row.id, "project": "backend", "action": "deploy",
        "class": "human", "state": "pending", "description": "",
        "argv": [], "http": null, "valid": true, "invalid_reason": null,
        "session_id": "s-a", "session_name": "deployer",
        "requested_at_ms": row.at, "expires_at_ms": row.at + 600_000,
        "dry_run": null, "binding": null, "bound_files": null, "files_unchanged": null,
    }))
    .expect("an approval")
}

fn run_event(row: &RunRow) -> Value {
    let started = serde_json::json!({
        "run_id": format!("r{}", row.ts), "project": "backend", "session_id": "s-a",
        "action": "lint", "class": "free", "params": {}, "argv": ["lint"], "catalogue_hash": "h",
    });
    let finished = |exit_code: i64| {
        serde_json::json!({
            "kind": "run", "state": "finished", "started": started,
            "finished": {
                "run_id": format!("r{}", row.ts), "outcome": "exited",
                "exit_code": exit_code, "duration_ms": 10,
            },
            "finished_at_ms": row.ts + 10,
        })
    };
    match row.kind.as_str() {
        "succeeded" => finished(0),
        "failed" => finished(1),
        "abandoned" => serde_json::json!({
            "kind": "run", "state": "abandoned", "started": started,
            "finished": null, "finished_at_ms": null,
        }),
        "refused" => serde_json::json!({
            "kind": "refused", "project": "backend", "session_id": "s-a",
            "action": row.action.as_deref().unwrap_or("lint"),
            "reason": "action_not_allowed", "detail": "not in the catalogue",
        }),
        "gate_denied" => serde_json::json!({
            "kind": "gate_denied", "project": "backend", "session_id": "s-a", "deny": "unparsable",
        }),
        "gate_error" => serde_json::json!({
            "kind": "gate_error", "project": "backend", "session_id": "s-a",
            "cause": "approved_unreadable", "detail": "unreadable",
        }),
        other => panic!("no run kind {other}"),
    }
}

fn recent_of(row: &RunRow) -> RecentEntry {
    let event: RecentEvent = serde_json::from_value(run_event(row)).expect("a recent event");
    RecentEntry {
        ts: row.ts,
        tone: tone_of(&event),
        event,
        review: None,
    }
}

fn open_subject_of(row: &OpenSubjectRow) -> SubjectView {
    SubjectView {
        id: row.id,
        project: "backend".into(),
        name: row.name.clone(),
        status: SubjectStatus::Active,
        status_src: Source::Author,
        status_at_ms: row.at,
        routine: false,
        sessions: row
            .sessions
            .iter()
            .map(|session| SessionView {
                session_id: session.id.clone(),
                project: "backend".into(),
                name: Some(Untrusted::new(session.id.clone())),
                kind: None,
                first_seen_ms: row.at,
                last_seen_ms: row.at,
                ended: session.ended.then_some(EndView {
                    how: EndHow::SessionEnd,
                    reason: None,
                    src: Source::Hook,
                    at_ms: row.at,
                }),
                superseded: false,
                non_interactive: false,
                card: session.card.map(|column| LiveCard {
                    key: session.id.clone(),
                    column,
                    badge: None,
                    waiting_for: None,
                }),
            })
            .collect(),
        excerpts: None,
    }
}

fn triage_of(row: &TriageRowCase) -> TriageRow {
    serde_json::from_value(serde_json::json!({
        "key": row.key, "kind": "unfiled_session", "project": "backend", "subject": null,
        "session_id": "s-t", "session_name": "triaged",
        "proposal": { "verdict": "to_resume", "source": "rules", "reason": "default_to_resume" },
        "verdict": "to_resume", "excerpts": null,
        "end": { "how": "session_end", "reason": null, "src": "hook", "at_ms": row.at },
        "proposed_subject": null, "choices": [], "model_pending": false,
        "model_reason": null, "model_call": null,
    }))
    .expect("a triage row")
}

fn subject_row_of(row: &SubjectStatusRow) -> SubjectRecord {
    SubjectRecord {
        id: row.id,
        project: "backend".into(),
        name: format!("subject {}", row.id),
        status: Sourced {
            value: row.status,
            src: Source::Author,
            at_ms: row.at,
        },
        created_at_ms: CREATED_AT_MS,
        routine: false,
        merged_into: None,
    }
}

fn order_of(row: &OrderRow) -> WorkOrderRecord {
    WorkOrderRecord {
        id: row.id.clone(),
        seq: 1,
        template: "dependency-bump".into(),
        project: "backend".into(),
        cwd: "C:/path/to/backend".into(),
        values: BTreeMap::new(),
        argv: Vec::new(),
        prompt_sha256: String::new(),
        catalogue_hash: String::new(),
        name: format!("dependency-bump-{}", row.id),
        subject_id: None,
        new_subject_name: None,
        source: WorkOrderSource::Composer,
        proposal_id: None,
        launched_at_ms: row.at,
        outcome: LaunchOutcome::Started,
        exit_code: Some(0),
        error: None,
        printed: None,
        bg_id: row.session.as_ref().map(|_| "bg".to_owned()),
        session_id: row.session.clone(),
        bound_at_ms: row.session.as_ref().map(|_| row.at),
        session: None,
    }
}

fn changed_id(item: &ChangedItem) -> String {
    let what = match item {
        ChangedItem::RunFinished { succeeded, .. } => {
            (if *succeeded { "ok" } else { "failed" }).to_owned()
        }
        ChangedItem::StartedWaiting { key, .. } => key.clone(),
        ChangedItem::SubjectStatus { id, .. } => id.to_string(),
        ChangedItem::OrderLaunched { id, .. } => id.clone(),
    };
    format!("{}:{what}", item.kind().code())
}

fn section<T>(section: &Section<T>, id: impl Fn(&T) -> String) -> ExpectedSection {
    ExpectedSection {
        count: section.count,
        dropped: section.dropped,
        items: Some(section.items.iter().map(id).collect()),
    }
}

/// Each section as the case names it, sections that hold nothing left out.
fn sections_of(briefing: &Briefing) -> BTreeMap<String, ExpectedSection> {
    [
        (
            "subjects",
            section(&briefing.subjects, |s| s.id.to_string()),
        ),
        ("waiting", section(&briefing.waiting, |w| w.key.clone())),
        ("approvals", section(&briefing.approvals, |a| a.id.clone())),
        ("triage", section(&briefing.triage, |t| t.key.clone())),
        (
            "runs",
            section(&briefing.runs, |r| r.kind.code().to_owned()),
        ),
        (
            "work_orders",
            section(&briefing.work_orders, |o| o.id.clone()),
        ),
        ("changed", section(&briefing.changed, changed_id)),
    ]
    .into_iter()
    .filter(|(_, s)| s.count > 0)
    .map(|(name, s)| (name.to_owned(), s))
    .collect()
}

/// The case's payloads, built.
struct Payloads {
    columns: Columns,
    approvals: Vec<ApprovalView>,
    recent: Vec<RecentEntry>,
    open: Vec<SubjectView>,
    triage: Vec<TriageRow>,
    subjects: Vec<SubjectRecord>,
    orders: Vec<WorkOrderRecord>,
}

impl Payloads {
    fn of(case: &BriefingCase) -> Payloads {
        Payloads {
            columns: columns_of(case),
            approvals: case.approvals.iter().map(approval_of).collect(),
            recent: case.runs.iter().map(recent_of).collect(),
            open: case.subjects.iter().map(open_subject_of).collect(),
            triage: case.triage.iter().map(triage_of).collect(),
            subjects: case.subject_rows.iter().map(subject_row_of).collect(),
            orders: case.orders.iter().map(order_of).collect(),
        }
    }

    fn input(&self) -> BriefingInput<'_> {
        BriefingInput {
            columns: Some(&self.columns),
            approvals: &self.approvals,
            recent: &self.recent,
            open_subjects: &self.open,
            triage: &self.triage,
            subjects: &self.subjects,
            work_orders: &self.orders,
        }
    }
}

/// The time of every list item `briefing` kept.
fn kept_times(briefing: &Briefing) -> Vec<Option<i64>> {
    let mut out: Vec<Option<i64>> = Vec::new();
    out.extend(briefing.subjects.items.iter().map(|s| Some(s.at_ms)));
    out.extend(briefing.waiting.items.iter().map(|w| w.at_ms));
    out.extend(briefing.approvals.items.iter().map(|a| Some(a.at_ms)));
    out.extend(briefing.triage.items.iter().map(|t| t.at_ms));
    out.extend(briefing.runs.items.iter().map(|r| Some(r.at_ms)));
    out.extend(briefing.work_orders.items.iter().map(|o| Some(o.at_ms)));
    out.extend(briefing.changed.items.iter().map(|c| Some(c.at_ms())));
    out
}

/// A crowd's oldest cards are the items the bound drops first: each card
/// stands in Needs you and in `changed` at the same time, so the cards
/// gone are the first ones, and every item kept is at least as new as the
/// newest card gone.
fn check_the_crowd(case: &BriefingCase, briefing: &Briefing) {
    let Some(crowd) = &case.crowd else {
        return;
    };
    let gone = usize::try_from(briefing.waiting.dropped).unwrap();
    assert!(gone > 0, "case {}: the crowd fits", case.name);
    let keys: Vec<&str> = briefing
        .waiting
        .items
        .iter()
        .map(|w| w.key.as_str())
        .collect();
    for index in 0..crowd.cards {
        assert_eq!(
            keys.contains(&crowd_key(index).as_str()),
            index >= gone,
            "case {}: card {index} of the crowd",
            case.name
        );
    }
    let newest_gone = crowd.first_at + i64::try_from(gone - 1).unwrap() * crowd.step_ms;
    let kept_oldest = kept_times(briefing).into_iter().min().flatten();
    assert!(
        kept_oldest >= Some(newest_gone),
        "case {}: a kept item is older than a dropped one",
        case.name
    );
}

#[test]
fn briefing_cases_match() {
    let set = load_set();
    assert!(!set.briefing.is_empty());
    for case in &set.briefing {
        let payloads = Payloads::of(case);
        let briefing = briefing::build(&payloads.input(), case.cursor, case.now_ms);
        let bytes = serde_json::to_vec(&briefing).unwrap();
        assert!(bytes.len() <= BRIEFING_MAX_BYTES, "case: {}", case.name);
        let again = briefing::build(&payloads.input(), case.cursor, case.now_ms);
        assert_eq!(
            bytes,
            serde_json::to_vec(&again).unwrap(),
            "same input, same bytes: {}",
            case.name
        );
        assert_eq!(briefing.cursor, case.cursor);
        let mut got = sections_of(&briefing);
        for (name, expected) in &case.expect {
            if expected.items.is_none()
                && let Some(section) = got.get_mut(name)
            {
                section.items = None;
            }
        }
        assert_eq!(got, case.expect, "case: {}", case.name);
        check_the_crowd(case, &briefing);
    }
}

#[test]
fn every_change_and_run_kind_has_a_briefing_case() {
    let set = load_set();
    let expected_items = |section: &str| -> Vec<String> {
        set.briefing
            .iter()
            .filter_map(|case| case.expect.get(section)?.items.clone())
            .flatten()
            .collect()
    };
    let changed = expected_items("changed");
    for kind in ChangeKind::ALL {
        assert!(
            changed
                .iter()
                .any(|id| id.starts_with(&format!("{}:", kind.code()))),
            "{kind:?} has no briefing case in tests/vectors/orders.json"
        );
    }
    let runs = expected_items("runs");
    for kind in RunKind::ALL {
        assert!(
            runs.iter().any(|id| id == kind.code()),
            "{kind:?} has no briefing case in tests/vectors/orders.json"
        );
    }
    for cursor in [
        Cursor::ReadAt { at_ms: 0 },
        Cursor::PreviousStart { at_ms: 0 },
        Cursor::None,
    ] {
        assert!(
            set.briefing
                .iter()
                .any(|case| std::mem::discriminant(&case.cursor) == std::mem::discriminant(&cursor)),
            "no briefing case reads from {cursor:?}"
        );
    }
    assert!(
        set.briefing
            .iter()
            .any(|case| case.expect.values().any(|s| s.dropped > 0)),
        "no briefing case goes over the bound"
    );
}
