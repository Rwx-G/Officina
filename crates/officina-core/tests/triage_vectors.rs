//! Replays the hand-written triage vector set (AD-57) through the core's
//! rules, every section the set holds, and refuses a set that no longer
//! covers every documented value. `officina-host`'s
//! `tests/registry_ingest.rs` replays the `fold` section through the real
//! writer, so the SQL and these rules cannot drift apart.

use std::collections::{BTreeMap, BTreeSet};

use officina_core::agents::{LifecycleState, parse_agents};
use officina_core::board::project_for;
use officina_core::events::{EventLine, SESSION_END_REASONS, SESSION_START_SOURCES};
use officina_core::registry::fold::{
    AgentsSnapshot, FactWrite, Mark, candidates_needed, fold_agents, line_facts, link_for_line,
    pivot, replaces,
};
use officina_core::registry::types::{
    Ending, FactRow, FactValue, SessionFact, SessionRecord, SessionRow, Source,
};
use serde::Deserialize;

const VECTORS: &str = include_str!("../../../tests/vectors/triage.json");

/// Every section of the set; a section this test does not replay fails to
/// parse, so none is skipped silently.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VectorSet {
    schema: u32,
    fold: Vec<FoldCase>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FoldCase {
    name: String,
    steps: Vec<Step>,
    expect: Vec<ExpectedSession>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Step {
    /// Event lines, ingested in this order.
    Events(Vec<EventLine>),
    /// One successful `claude agents` refresh at `now_ms`.
    Agents {
        now_ms: i64,
        records: Vec<serde_json::Value>,
    },
    /// The window closes and starts again: the snapshot is seeded anew.
    Restart,
}

#[derive(Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
struct ExpectedSession {
    session_id: String,
    #[serde(default)]
    project: Option<String>,
    /// Every fact the session holds: its value and its source.
    facts: BTreeMap<String, (serde_json::Value, Source)>,
    ended: Option<Ending>,
    superseded: bool,
    non_interactive: bool,
}

fn load() -> VectorSet {
    let set: VectorSet = serde_json::from_str(VECTORS).expect("vector set parses");
    assert_eq!(set.schema, 1);
    set
}

/// The registry in memory, written as the host writes it: a session is
/// created at first sight with its project, a fact is replaced only by a
/// later one, and a line's own facts are stored before it looks for its
/// supersession link.
#[derive(Default)]
struct Registry {
    rows: BTreeMap<String, SessionRow>,
    facts: BTreeMap<(String, String), FactRow>,
    snapshot: Option<AgentsSnapshot>,
}

impl Registry {
    fn ensure(&mut self, session_id: &str, cwd: Option<&str>, seen_ms: i64) {
        let row = self
            .rows
            .entry(session_id.to_owned())
            .or_insert_with(|| SessionRow {
                session_id: session_id.to_owned(),
                project: project_for(cwd, &[]).name,
                first_seen_ms: seen_ms,
                last_seen_ms: seen_ms,
                subject: None,
            });
        row.first_seen_ms = row.first_seen_ms.min(seen_ms);
        row.last_seen_ms = row.last_seen_ms.max(seen_ms);
    }

    fn write(&mut self, write: FactWrite) {
        let key = (write.session_id.clone(), write.fact.code().to_owned());
        if replaces(self.facts.get(&key).map(|f| f.at_ms), write.at_ms) {
            self.facts.insert(
                key,
                FactRow {
                    session_id: write.session_id,
                    fact: write.fact.code().to_owned(),
                    value: write.value,
                    src: write.src,
                    at_ms: write.at_ms,
                },
            );
        }
    }

    fn records(&self) -> BTreeMap<String, SessionRecord> {
        let rows: Vec<SessionRow> = self.rows.values().cloned().collect();
        let facts: Vec<FactRow> = self.facts.values().cloned().collect();
        pivot(&rows, &facts)
            .into_iter()
            .map(|r| (r.session_id.clone(), r))
            .collect()
    }

    fn marks(&self) -> Vec<Mark> {
        self.records()
            .values()
            .flat_map(|r| {
                [
                    r.ended
                        .as_ref()
                        .and_then(|e| Mark::of_end(&r.session_id, e)),
                    r.started
                        .as_ref()
                        .and_then(|s| Mark::of_start(&r.session_id, s)),
                ]
            })
            .flatten()
            .collect()
    }

    fn line(&mut self, line: &EventLine) {
        let Some(session_id) = line.session_id.as_deref().filter(|id| !id.is_empty()) else {
            return;
        };
        self.ensure(session_id, line.cwd.as_deref(), line.ts);
        for write in line_facts(line) {
            self.write(write);
        }
        if candidates_needed(line).is_some()
            && let Some(link) = link_for_line(line, &self.marks())
        {
            self.write(link);
        }
    }

    fn agents(&mut self, records: &[serde_json::Value], now_ms: i64) {
        let records = parse_agents(&serde_json::to_string(records).unwrap()).unwrap();
        let known = self.records();
        let previous = self.snapshot.take().unwrap_or_else(|| {
            AgentsSnapshot::seeded(&known.values().cloned().collect::<Vec<_>>())
        });
        let (next, changes) = fold_agents(&previous, &records, &known, now_ms);
        for listed in changes.newly_listed {
            self.ensure(&listed.session_id, listed.cwd.as_deref(), now_ms);
        }
        for write in changes.writes {
            self.write(write);
        }
        for session_id in changes.touched {
            self.ensure(&session_id, None, now_ms);
        }
        self.snapshot = Some(next);
    }
}

/// A stored fact as the vector set writes it: structured values as their
/// JSON, the rest as they are.
fn fact_json(row: &FactRow) -> serde_json::Value {
    match (&row.value, SessionFact::from_code(&row.fact)) {
        (FactValue::Text(text), Some(SessionFact::Started | SessionFact::Ended)) => {
            serde_json::from_str(text).unwrap()
        }
        (FactValue::Text(text), _) => text.clone().into(),
        (FactValue::Integer(i), _) => (*i).into(),
    }
}

/// What the registry holds, in the vector set's terms.
fn observed(registry: &Registry, expected: &[ExpectedSession]) -> Vec<ExpectedSession> {
    let projects: BTreeMap<&str, bool> = expected
        .iter()
        .map(|e| (e.session_id.as_str(), e.project.is_some()))
        .collect();
    registry
        .records()
        .into_values()
        .map(|record| ExpectedSession {
            facts: registry
                .facts
                .values()
                .filter(|f| f.session_id == record.session_id)
                .map(|f| (f.fact.clone(), (fact_json(f), f.src)))
                .collect(),
            ended: record.ending().map(|e| e.value),
            superseded: record.is_superseded(),
            non_interactive: record.is_non_interactive(),
            project: projects
                .get(record.session_id.as_str())
                .copied()
                .unwrap_or(false)
                .then(|| record.project.clone()),
            session_id: record.session_id,
        })
        .collect()
}

#[test]
fn every_case_matches() {
    let set = load();
    assert!(!set.fold.is_empty());
    for case in &set.fold {
        let mut registry = Registry::default();
        for step in &case.steps {
            match step {
                Step::Events(lines) => lines.iter().for_each(|line| registry.line(line)),
                Step::Agents { now_ms, records } => registry.agents(records, *now_ms),
                Step::Restart => registry.snapshot = None,
            }
        }
        assert_eq!(
            observed(&registry, &case.expect),
            case.expect,
            "fold case: {}",
            case.name
        );
    }
}

/// The wire name serde gives an enum value.
fn wire<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn every_value_has_a_case() {
    let set = load();
    let mut end_reasons = BTreeSet::new();
    let mut start_sources = BTreeSet::new();
    let mut states = BTreeSet::new();
    for step in set.fold.iter().flat_map(|case| &case.steps) {
        match step {
            Step::Events(lines) => {
                for line in lines {
                    match line.event.as_str() {
                        "SessionEnd" => end_reasons.extend(line.reason.clone()),
                        "SessionStart" => start_sources.extend(line.source.clone()),
                        _ => {}
                    }
                }
            }
            Step::Agents { records, .. } => states.extend(
                records
                    .iter()
                    .filter_map(|r| r.get("state")?.as_str().map(str::to_owned)),
            ),
            Step::Restart => {}
        }
    }
    for reason in SESSION_END_REASONS {
        assert!(
            end_reasons.contains(reason),
            "no fold case for SessionEnd reason {reason}"
        );
    }
    for source in SESSION_START_SOURCES {
        assert!(
            start_sources.contains(source),
            "no fold case for SessionStart source {source}"
        );
    }
    for state in LifecycleState::ALL {
        assert!(
            states.contains(&wire(&state)),
            "no fold case for state {state:?}"
        );
    }
}
