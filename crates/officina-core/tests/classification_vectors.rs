//! Replays the hand-written classification vector set (AD-14) through
//! `build_board`, and refuses a set that no longer covers every value.

use std::collections::BTreeSet;

use officina_core::agents::parse_agents;
use officina_core::board::{BoardInput, Column, build_board};
use officina_core::events::EventLine;
use serde::Deserialize;

const VECTORS: &str = include_str!("../../../tests/vectors/classification.json");

#[derive(Deserialize)]
struct VectorSet {
    schema: u32,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    now_ms: i64,
    agents: Vec<serde_json::Value>,
    events: Vec<EventLine>,
    expect: Vec<Expected>,
}

#[derive(Deserialize, Debug, PartialEq)]
struct Expected {
    key: String,
    column: Column,
    badge: Option<officina_core::board::Badge>,
    waiting_for: Option<String>,
}

fn load() -> VectorSet {
    let set: VectorSet = serde_json::from_str(VECTORS).expect("vector set parses");
    assert_eq!(set.schema, 1);
    set
}

#[test]
fn every_case_classifies_as_expected() {
    for case in load().cases {
        let agents_json = serde_json::to_string(&case.agents).unwrap();
        let input = BoardInput {
            agents: Some(parse_agents(&agents_json).unwrap()),
            events: case.events,
            ..Default::default()
        };
        let board = build_board(&input, case.now_ms);
        let actual: Vec<Expected> = board
            .columns
            .all()
            .map(|card| Expected {
                key: card.key.clone(),
                column: card.column,
                badge: card.badge,
                waiting_for: card.waiting_for.clone(),
            })
            .collect();
        assert_eq!(actual, case.expect, "case: {}", case.name);
        assert_eq!(
            board.waiting_count as usize,
            board.columns.needs_you.len(),
            "case: {}",
            case.name
        );
    }
}

fn serialized<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn vector_set_covers_every_column_badge_and_documented_value() {
    let set = load();
    let mut columns = BTreeSet::new();
    let mut badges = BTreeSet::new();
    let mut statuses = BTreeSet::new();
    let mut states = BTreeSet::new();
    let mut end_reasons = BTreeSet::new();
    for case in &set.cases {
        for e in &case.expect {
            columns.insert(serialized(&e.column));
            if let Some(b) = e.badge {
                badges.insert(serialized(&b));
            }
        }
        for a in &case.agents {
            if let Some(s) = a.get("status").and_then(|v| v.as_str()) {
                statuses.insert(s.to_owned());
            }
            if let Some(s) = a.get("state").and_then(|v| v.as_str()) {
                states.insert(s.to_owned());
            }
        }
        for e in &case.events {
            if let Some(r) = &e.reason {
                end_reasons.insert(r.clone());
            }
        }
    }
    for column in Column::ALL {
        assert!(
            columns.contains(&serialized(&column)),
            "no case for column {column:?}"
        );
    }
    for badge in officina_core::board::Badge::ALL {
        assert!(
            badges.contains(&serialized(&badge)),
            "no case for badge {badge:?}"
        );
    }
    for status in officina_core::agents::LiveStatus::ALL {
        assert!(
            statuses.contains(&serialized(&status)),
            "no case for status {status:?}"
        );
    }
    for state in officina_core::agents::LifecycleState::ALL {
        assert!(
            states.contains(&serialized(&state)),
            "no case for state {state:?}"
        );
    }
    for reason in officina_core::events::SESSION_END_REASONS {
        assert!(
            end_reasons.contains(reason),
            "no case for SessionEnd reason {reason}"
        );
    }
}
