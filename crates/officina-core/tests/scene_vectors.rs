//! Replays the hand-written scene vector set (AD-167, AD-14's pattern)
//! through `build_board`, and refuses a set that no longer covers every
//! zone and alert cause.

use std::collections::{BTreeMap, BTreeSet};

use officina_core::agents::parse_agents;
use officina_core::board::{BoardInput, ProjectRule, Staleness, SubagentView, build_board};
use officina_core::events::EventLine;
use officina_core::floor::Priority;
use officina_core::relay::approvals::{ApprovalState, ApprovalView};
use officina_core::relay::catalogue::ActionClass;
use officina_core::scene::{AlertCause, SceneState, Zone};
use serde::Deserialize;

const VECTORS: &str = include_str!("../../../tests/vectors/scene.json");

#[derive(Deserialize)]
struct VectorSet {
    schema: u32,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct FloorProject {
    name: String,
    path: String,
    priority: Priority,
}

#[derive(Deserialize)]
struct Request {
    session_id: String,
    requested_at_ms: i64,
    #[serde(default = "valid")]
    valid: bool,
    #[serde(default = "pending")]
    state: ApprovalState,
}

fn pending() -> ApprovalState {
    ApprovalState::Pending
}

fn valid() -> bool {
    true
}

#[derive(Deserialize)]
struct Case {
    name: String,
    now_ms: i64,
    #[serde(default)]
    stale: bool,
    #[serde(default)]
    floor: Vec<FloorProject>,
    alert_after_minutes: Option<u32>,
    #[serde(default)]
    approvals: Vec<Request>,
    #[serde(default)]
    previous: BTreeMap<String, SceneState>,
    agents: Vec<serde_json::Value>,
    events: Vec<EventLine>,
    expect: Vec<KeyedExpected>,
    expect_alert_count: u32,
    expect_next_escalation_at_ms: Option<i64>,
}

#[derive(Deserialize)]
struct KeyedExpected {
    key: String,
    #[serde(flatten)]
    scene: SceneState,
    /// The card's running sub-agents (AD-176); none when absent.
    #[serde(default)]
    subagents: Vec<SubagentView>,
}

fn load() -> Vec<Case> {
    let set: VectorSet = serde_json::from_str(VECTORS).expect("vector set parses");
    assert_eq!(set.schema, 1);
    set.cases
}

/// A pending request as `relay::approvals::pending` lists it, reduced to
/// what the scene reads.
fn approval(request: &Request) -> ApprovalView {
    ApprovalView {
        id: format!("0123456789abcdef-{}", request.requested_at_ms),
        project: "api".to_owned(),
        action: "restart".to_owned(),
        class: ActionClass::Auto,
        state: request.state,
        escalation: None,
        description: String::new(),
        argv: Vec::new(),
        http: None,
        valid: request.valid,
        invalid_reason: (!request.valid).then(|| "forged".to_owned()),
        session_id: Some(request.session_id.clone()),
        session_name: None,
        requested_at_ms: request.requested_at_ms,
        expires_at_ms: request.requested_at_ms + 3_600_000,
        dry_run: None,
        binding: None,
        bound_files: None,
        files_unchanged: None,
    }
}

fn input_of(case: &Case) -> BoardInput {
    BoardInput {
        agents: Some(parse_agents(&serde_json::to_string(&case.agents).unwrap()).unwrap()),
        stale: case.stale.then(|| Staleness {
            since_ms: case.now_ms - 60_000,
            error: "claude agents failed".to_owned(),
        }),
        events: case.events.clone(),
        projects: case
            .floor
            .iter()
            .map(|p| ProjectRule {
                name: p.name.clone(),
                path: p.path.clone(),
                color: None,
                priority: p.priority,
            })
            .collect(),
        approvals: case.approvals.iter().map(approval).collect(),
        alert_after_minutes: case.alert_after_minutes,
        previous_scene: case.previous.clone(),
        ..Default::default()
    }
}

#[test]
fn every_case_places_its_cards() {
    let cases = load();
    assert!(!cases.is_empty());
    for case in &cases {
        let board = build_board(&input_of(case), case.now_ms);
        let actual: BTreeMap<String, Option<SceneState>> = board
            .columns
            .all()
            .map(|card| (card.key.clone(), card.scene))
            .collect();
        let expected: BTreeMap<String, Option<SceneState>> = case
            .expect
            .iter()
            .map(|e| (e.key.clone(), Some(e.scene)))
            .collect();
        assert_eq!(actual, expected, "case: {}", case.name);
        let actual_subagents: BTreeMap<&str, &[SubagentView]> = board
            .columns
            .all()
            .map(|card| (card.key.as_str(), card.subagents.as_slice()))
            .collect();
        let expected_subagents: BTreeMap<&str, &[SubagentView]> = case
            .expect
            .iter()
            .map(|e| (e.key.as_str(), e.subagents.as_slice()))
            .collect();
        assert_eq!(actual_subagents, expected_subagents, "case: {}", case.name);
        assert_eq!(
            board.alert_count, case.expect_alert_count,
            "case: {}",
            case.name
        );
        assert_eq!(
            board.next_escalation_at_ms, case.expect_next_escalation_at_ms,
            "case: {}",
            case.name
        );
    }
}

/// D-467: a board built at the next escalation has the card in alert, and
/// the next escalation moves on to the next wait or none.
#[test]
fn building_at_the_next_escalation_alerts_the_card() {
    let cases = load();
    let mut checked = 0;
    for case in cases
        .iter()
        .filter(|c| c.expect_next_escalation_at_ms.is_some())
    {
        let at = case.expect_next_escalation_at_ms.unwrap();
        let before = build_board(&input_of(case), case.now_ms);
        let later = build_board(&input_of(case), at);
        assert_eq!(
            later.alert_count,
            before.alert_count + 1,
            "case: {}",
            case.name
        );
        assert!(
            later.next_escalation_at_ms.is_none_or(|next| next > at),
            "case: {}",
            case.name
        );
        let just_before = build_board(&input_of(case), at - 1);
        assert_eq!(
            just_before.alert_count, before.alert_count,
            "case: {}",
            case.name
        );
        checked += 1;
    }
    assert!(checked > 0, "no case has a next escalation");
}

#[test]
fn every_zone_and_cause_has_a_case() {
    let scenes: Vec<SceneState> = load()
        .iter()
        .flat_map(|case| case.expect.iter().map(|e| e.scene))
        .collect();
    let zones: BTreeSet<&str> = scenes.iter().map(|s| s.zone.code()).collect();
    for zone in Zone::ALL {
        assert!(
            zones.contains(zone.code()),
            "no case places a card in {zone:?}"
        );
    }
    let causes: BTreeSet<&str> = scenes
        .iter()
        .filter_map(|s| s.alert)
        .map(AlertCause::code)
        .collect();
    for cause in AlertCause::ALL {
        assert!(
            causes.contains(cause.code()),
            "no case alerts for {cause:?}"
        );
    }
    assert!(scenes.iter().any(|s| s.approval), "no approval-only case");
}
