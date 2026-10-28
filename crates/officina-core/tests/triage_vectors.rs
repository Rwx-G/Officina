//! Replays the hand-written triage vector set (AD-57) through the core's
//! rules, every section the set holds, and refuses a set that no longer
//! covers every documented value. `officina-host` replays the same sections
//! through the real writer (`tests/registry_ingest.rs` the `fold` section,
//! `tests/registry_write.rs` the `filing` and `confirm` ones), so the SQL and
//! these rules cannot drift apart.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use officina_core::agents::{LifecycleState, parse_agents};
use officina_core::board::project_for;
use officina_core::events::{EventLine, SESSION_END_REASONS, SESSION_START_SOURCES};
use officina_core::registry::confirm::{AuthorAction, AuthorContext, Plan, Target, plan, wake};
use officina_core::registry::filing::{excerpts_to_clear, filing_at_creation};
use officina_core::registry::fold::{
    AgentsSnapshot, FactWrite, Mark, candidates_needed, fold_agents, line_facts, link_for_line,
    pivot, replaces,
};
use officina_core::registry::ingestion::{ExcerptObservation, excerpt_observation};
use officina_core::registry::types::{
    AuthorRefusal, Ending, ExcerptRecord, FactRow, FactValue, SessionFact, SessionRecord,
    SessionRow, Source, Sourced, SubjectRecord, SubjectStatus,
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
    filing: Vec<SubjectCase>,
    confirm: Vec<SubjectCase>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FoldCase {
    name: String,
    steps: Vec<Step>,
    expect: Vec<ExpectedSession>,
}

/// A case of the `filing` or `confirm` section.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubjectCase {
    name: String,
    steps: Vec<Step>,
    expect: ExpectedSubjects,
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
    /// An author action at `at_ms`, refused for the reason given, if any.
    Author {
        at_ms: i64,
        action: AuthorAction,
        #[serde(default)]
        refused: Option<AuthorRefusal>,
    },
    /// A confirmation of `subject` at `at_ms`, as story 3.7 records one.
    Confirmed { subject: i64, at_ms: i64 },
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

#[derive(Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
struct ExpectedSubjects {
    /// Every session: the subject it counts under, and who filed it last.
    sessions: BTreeMap<String, Filing>,
    subjects: Vec<ExpectedSubject>,
    /// The sessions holding an excerpt row.
    excerpts: Vec<String>,
    /// Confirmations stamped reopened: subject, confirmed at, reopened at.
    #[serde(default)]
    reopened: Vec<(i64, i64, i64)>,
}

#[derive(Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
struct Filing {
    filed: Option<i64>,
    src: Option<Source>,
}

#[derive(Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
struct ExpectedSubject {
    id: i64,
    project: String,
    name: String,
    status: SubjectStatus,
    status_src: Source,
    status_at_ms: i64,
    routine: bool,
    merged_into: Option<i64>,
}

fn load() -> VectorSet {
    let set: VectorSet = serde_json::from_str(VECTORS).expect("vector set parses");
    assert_eq!(set.schema, 1);
    set
}

/// One confirmation, as far as reopening reads it.
struct Confirmation {
    subject: i64,
    at_ms: i64,
    reopened_at_ms: Option<i64>,
}

/// The registry in memory, written as the host writes it: a session is
/// created at first sight with its project and filed in the same step, a
/// fact is replaced only by a later one, a line's own facts are stored
/// before it looks for its supersession link, and an author action applies
/// the core's plan.
#[derive(Default)]
struct Registry {
    rows: BTreeMap<String, SessionRow>,
    facts: BTreeMap<(String, String), FactRow>,
    snapshot: Option<AgentsSnapshot>,
    subjects: Vec<SubjectRecord>,
    excerpts: BTreeMap<String, ExcerptRecord>,
    confirmations: Vec<Confirmation>,
}

impl Registry {
    fn ensure(&mut self, session_id: &str, cwd: Option<&str>, seen_ms: i64) {
        if let Some(row) = self.rows.get_mut(session_id) {
            row.first_seen_ms = row.first_seen_ms.min(seen_ms);
            row.last_seen_ms = row.last_seen_ms.max(seen_ms);
            return;
        }
        let project = project_for(cwd, &[]).name;
        let active: Vec<SubjectRecord> = self
            .subjects
            .iter()
            .filter(|s| s.project == project && s.status.value == SubjectStatus::Active)
            .cloned()
            .collect();
        let subject = filing_at_creation(&project, &active).map(|id| Sourced {
            value: Some(id),
            src: Source::Rules,
            at_ms: seen_ms,
        });
        self.rows.insert(
            session_id.to_owned(),
            SessionRow {
                session_id: session_id.to_owned(),
                project,
                first_seen_ms: seen_ms,
                last_seen_ms: seen_ms,
                subject,
            },
        );
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

    fn raw_subject(&self, session_id: &str) -> Option<i64> {
        self.rows
            .get(session_id)
            .and_then(|r| r.subject.as_ref())
            .and_then(|s| s.value)
    }

    fn store_excerpt(&mut self, session_id: &str, line: &EventLine) {
        let entry = self
            .excerpts
            .entry(session_id.to_owned())
            .or_insert_with(|| ExcerptRecord {
                session_id: session_id.to_owned(),
                ..ExcerptRecord::default()
            });
        match excerpt_observation(line) {
            Some(ExcerptObservation::Prompt { text })
                if entry.prompt_at_ms.is_none_or(|at| line.ts > at) =>
            {
                entry.prompt = Some(text);
                entry.prompt_at_ms = Some(line.ts);
            }
            Some(ExcerptObservation::Answer { head, tail })
                if entry.answer_at_ms.is_none_or(|at| line.ts > at) =>
            {
                entry.answer_head = Some(head);
                entry.answer_tail = tail;
                entry.answer_at_ms = Some(line.ts);
            }
            _ => {}
        }
        if entry.prompt_at_ms.is_none() && entry.answer_at_ms.is_none() {
            self.excerpts.remove(session_id);
        }
    }

    /// D-50: the subject keeps its latest excerpts only.
    fn prune(&mut self, subject: i64) {
        let records = self.records();
        let sessions: Vec<SessionRecord> = records
            .into_values()
            .filter(|r| self.raw_subject(&r.session_id) == Some(subject))
            .collect();
        let excerpts: Vec<ExcerptRecord> = sessions
            .iter()
            .filter_map(|s| self.excerpts.get(&s.session_id).cloned())
            .collect();
        for id in excerpts_to_clear(&sessions, &excerpts) {
            self.excerpts.remove(&id);
        }
    }

    fn stamp_reopen(&mut self, subject: i64, at_ms: i64) {
        if let Some(latest) = self
            .confirmations
            .iter_mut()
            .filter(|c| c.subject == subject)
            .max_by_key(|c| c.at_ms)
            && latest.reopened_at_ms.is_none()
        {
            latest.reopened_at_ms = Some(at_ms);
        }
    }

    /// D-77: a session alive again wakes its subject.
    fn wake(&mut self, before: &SessionRecord, after: &SessionRecord, at_ms: i64) {
        if let Some(wake) = wake(before, after, &self.subjects) {
            if let Some(subject) = self.subjects.iter_mut().find(|s| s.id == wake.subject) {
                subject.status = Sourced {
                    value: SubjectStatus::Active,
                    src: Source::Rules,
                    at_ms,
                };
            }
            if wake.stamp_reopen {
                self.stamp_reopen(wake.subject, at_ms);
            }
        }
    }

    fn line(&mut self, line: &EventLine) {
        let Some(session_id) = line.session_id.as_deref().filter(|id| !id.is_empty()) else {
            return;
        };
        self.ensure(session_id, line.cwd.as_deref(), line.ts);
        let filed = self.raw_subject(session_id).is_some();
        let before = (filed && line.event == "UserPromptSubmit")
            .then(|| self.records().remove(session_id))
            .flatten()
            .filter(SessionRecord::is_ended);
        self.store_excerpt(session_id, line);
        for write in line_facts(line) {
            self.write(write);
        }
        if candidates_needed(line).is_some()
            && let Some(link) = link_for_line(line, &self.marks())
        {
            self.write(link);
        }
        let moved = excerpt_observation(line).is_some() || line.event == "SessionEnd";
        if filed && (before.is_some() || moved) {
            let after = self.records().remove(session_id).unwrap();
            if let Some(before) = &before {
                self.wake(before, &after, line.ts);
            }
            if moved && let Some(subject) = after.filed_subject() {
                self.prune(subject);
            }
        }
    }

    fn agents(&mut self, records: &[serde_json::Value], now_ms: i64) {
        let records = parse_agents(&serde_json::to_string(records).unwrap()).unwrap();
        let known = self.records();
        let previous = self.snapshot.take().unwrap_or_else(|| {
            AgentsSnapshot::seeded(&known.values().cloned().collect::<Vec<_>>())
        });
        let (next, changes) = fold_agents(&previous, &records, &known, now_ms);
        for listed in &changes.newly_listed {
            self.ensure(&listed.session_id, listed.cwd.as_deref(), now_ms);
        }
        let written: BTreeSet<String> = changes
            .writes
            .iter()
            .map(|w| w.session_id.clone())
            .collect();
        for write in changes.writes {
            self.write(write);
        }
        for session_id in changes.touched {
            self.ensure(&session_id, None, now_ms);
        }
        for session_id in written {
            let after = self.records().remove(&session_id).unwrap();
            if let Some(before) = known.get(&session_id) {
                self.wake(before, &after, now_ms);
            }
            if changes
                .newly_listed
                .iter()
                .any(|n| n.session_id == session_id)
                && let Some(subject) = after.filed_subject()
            {
                self.prune(subject);
            }
        }
        self.snapshot = Some(next);
    }

    fn author(&mut self, action: &AuthorAction, at_ms: i64) -> Result<(), AuthorRefusal> {
        let sessions: Vec<SessionRecord> = self.records().into_values().collect();
        let known_projects: Vec<String> = self
            .rows
            .values()
            .map(|r| r.project.clone())
            .chain(self.subjects.iter().map(|s| s.project.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let planned = plan(
            action,
            &AuthorContext {
                subjects: &self.subjects,
                sessions: &sessions,
                known_projects: &known_projects,
            },
        )?;
        self.apply(&planned, at_ms);
        Ok(())
    }

    fn apply(&mut self, plan: &Plan, at_ms: i64) {
        let author = |value| Sourced {
            value,
            src: Source::Author,
            at_ms,
        };
        let created = plan.create.as_ref().map(|new| {
            let id = self.subjects.iter().map(|s| s.id).max().unwrap_or(0) + 1;
            self.subjects.push(SubjectRecord {
                id,
                project: new.project.clone(),
                name: new.name.clone(),
                status: author(SubjectStatus::Active),
                created_at_ms: at_ms,
                routine: false,
                merged_into: None,
            });
            id
        });
        let resolve = |target: Target| match target {
            Target::Existing(id) => Some(id),
            Target::Created => created,
        };
        for change in &plan.changes {
            let subject = self
                .subjects
                .iter_mut()
                .find(|s| s.id == change.id)
                .unwrap();
            if let Some(name) = &change.name {
                subject.name.clone_from(name);
            }
            if let Some(status) = change.status {
                subject.status = author(status);
            }
            if let Some(routine) = change.routine {
                subject.routine = routine;
            }
            if change.merged_into.is_some() {
                subject.merged_into = change.merged_into;
            }
        }
        for (session_id, target) in &plan.filings {
            self.rows.get_mut(session_id).unwrap().subject = Some(Sourced {
                value: target.and_then(resolve),
                src: Source::Author,
                at_ms,
            });
        }
        for session_id in &plan.clear_excerpts {
            self.excerpts.remove(session_id);
        }
        if let Some(subject) = plan.stamp_reopen {
            self.stamp_reopen(subject, at_ms);
        }
        if let Some(subject) = plan.prune.and_then(resolve) {
            self.prune(subject);
        }
    }

    fn run(&mut self, steps: &[Step], case: &str) {
        for step in steps {
            match step {
                Step::Events(lines) => lines.iter().for_each(|line| self.line(line)),
                Step::Agents { now_ms, records } => self.agents(records, *now_ms),
                Step::Restart => self.snapshot = None,
                Step::Author {
                    at_ms,
                    action,
                    refused,
                } => assert_eq!(
                    self.author(action, *at_ms).err(),
                    *refused,
                    "{case}: {action:?}"
                ),
                Step::Confirmed { subject, at_ms } => self.confirmations.push(Confirmation {
                    subject: *subject,
                    at_ms: *at_ms,
                    reopened_at_ms: None,
                }),
            }
        }
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

/// What the registry holds, in the `fold` section's terms.
fn observed_sessions(registry: &Registry, expected: &[ExpectedSession]) -> Vec<ExpectedSession> {
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

/// What the registry holds, in the `filing` and `confirm` sections' terms.
fn observed_subjects(registry: &Registry) -> ExpectedSubjects {
    ExpectedSubjects {
        sessions: registry
            .records()
            .into_values()
            .map(|record| {
                let filing = Filing {
                    filed: record.filed_subject(),
                    src: record.subject.as_ref().map(|s| s.src),
                };
                (record.session_id, filing)
            })
            .collect(),
        subjects: registry
            .subjects
            .iter()
            .map(|s| ExpectedSubject {
                id: s.id,
                project: s.project.clone(),
                name: s.name.clone(),
                status: s.status.value,
                status_src: s.status.src,
                status_at_ms: s.status.at_ms,
                routine: s.routine,
                merged_into: s.merged_into,
            })
            .collect(),
        excerpts: registry.excerpts.keys().cloned().collect(),
        reopened: registry
            .confirmations
            .iter()
            .filter_map(|c| Some((c.subject, c.at_ms, c.reopened_at_ms?)))
            .collect(),
    }
}

#[test]
fn every_case_matches() {
    let set = load();
    assert!(!set.fold.is_empty() && !set.filing.is_empty() && !set.confirm.is_empty());
    for case in &set.fold {
        let mut registry = Registry::default();
        registry.run(&case.steps, &case.name);
        assert_eq!(
            observed_sessions(&registry, &case.expect),
            case.expect,
            "fold case: {}",
            case.name
        );
    }
    for (section, cases) in [("filing", &set.filing), ("confirm", &set.confirm)] {
        for case in cases {
            let mut registry = Registry::default();
            registry.run(&case.steps, &case.name);
            assert_eq!(
                observed_subjects(&registry),
                case.expect,
                "{section} case: {}",
                case.name
            );
        }
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
    let mut refusals = HashSet::new();
    let mut statuses = HashSet::new();
    let steps = set.fold.iter().flat_map(|case| &case.steps).chain(
        set.filing
            .iter()
            .chain(&set.confirm)
            .flat_map(|case| &case.steps),
    );
    for step in steps {
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
            Step::Author { refused, .. } => refusals.extend(*refused),
            Step::Restart | Step::Confirmed { .. } => {}
        }
    }
    for case in set.filing.iter().chain(&set.confirm) {
        statuses.extend(case.expect.subjects.iter().map(|s| s.status));
    }
    for reason in SESSION_END_REASONS {
        assert!(
            end_reasons.contains(reason),
            "no case for SessionEnd reason {reason}"
        );
    }
    for source in SESSION_START_SOURCES {
        assert!(
            start_sources.contains(source),
            "no case for SessionStart source {source}"
        );
    }
    for state in LifecycleState::ALL {
        assert!(
            states.contains(&wire(&state)),
            "no case for state {state:?}"
        );
    }
    for status in SubjectStatus::ALL {
        assert!(
            statuses.contains(status),
            "no case for subject status {status:?}"
        );
    }
    for refusal in AuthorRefusal::ALL {
        assert!(
            refusals.contains(refusal),
            "no confirm case for refusal {refusal:?}"
        );
    }
}
