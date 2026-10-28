//! Replays the hand-written triage vector set (AD-57) through the core's
//! rules, every section the set holds, and refuses a set that no longer
//! covers every documented value. `officina-host` replays the `fold`,
//! `filing` and `confirm` sections through the real writer
//! (`tests/registry_ingest.rs`, `tests/registry_write.rs`), so the SQL and
//! these rules cannot drift apart; the `triage` section's rows are computed
//! here from the state the steps build, as the view computes them from the
//! registry it reads, and the `confirm` section's triage confirmations are
//! applied here as the writer applies them.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use officina_core::agents::{LifecycleState, parse_agents};
use officina_core::board::project_for;
use officina_core::events::{EventLine, SESSION_END_REASONS, SESSION_START_SOURCES};
use officina_core::registry::confirm::{
    AuthorAction, AuthorContext, ConfirmItem, ConfirmationInput, ItemAnswer, ItemOutcome, Plan,
    Target, TriageContext, confirm_item, plan, wake,
};
use officina_core::registry::dry_run::{Evaluation, Tally};
use officina_core::registry::filing::{excerpts_to_clear, filing_at_creation};
use officina_core::registry::fold::{
    AgentsSnapshot, FactWrite, Mark, candidates_needed, fold_agents, line_facts, link_for_line,
    pivot, replaces,
};
use officina_core::registry::ingestion::{ExcerptObservation, excerpt_observation};
use officina_core::registry::model::{
    LastCall, ModelFacts, ModelPause, ModelSetting, RowModel, StoredAnswer, case_key, case_of,
    row_model,
};
use officina_core::registry::proposal::ModelInput;
use officina_core::registry::proposal::Proposal;
use officina_core::registry::rates::{GateState, SourceRate, VerdictRecord, gates, rates};
use officina_core::registry::triage_call::{
    CallAnswer, CallUsage, ModelOutputError, PromptError, STDOUT_CAP_BYTES, TriageCase, prompt,
    read_output,
};
use officina_core::registry::types::{
    AuthorRefusal, ConfirmRefusal, ConfirmationRecord, Ending, ExcerptRecord, FactRow, FactValue,
    ModelVerdict, NotNeeded, ProposalSource, ReasonCode, RowKind, SessionFact, SessionRecord,
    SessionRow, Source, Sourced, SubjectRecord, SubjectStatus, TriageOutcome, TriageRefusal,
    Verdict,
};
use officina_core::registry::verdict::{VerdictInput, rows};
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
    triage: Vec<RowsCase>,
    prompt: Vec<PromptCase>,
    model_output: Vec<ModelOutputCase>,
    rates: Vec<RatesCase>,
}

/// A case of the `rates` section (FR-11, FR-12, AD-50): confirmations and
/// the latest dry run in, every source's rate and the three gates out.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RatesCase {
    name: String,
    now_ms: i64,
    confirmations: Vec<RatedConfirmation>,
    latest_dry_run: Option<DryRunSummary>,
    /// When the model last turned itself off (D-69).
    #[serde(default)]
    self_disabled_at: Option<i64>,
    expect: RatesExpect,
}

/// A confirmation as the `rates` section writes it; `times` repeats it with
/// ids counting up and times a second apart, going back.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RatedConfirmation {
    id: i64,
    proposed: (ModelVerdict, ProposalSource, ReasonCode),
    verdict: Verdict,
    at: i64,
    #[serde(default)]
    auto: bool,
    #[serde(default)]
    reopened: Option<i64>,
    #[serde(default)]
    times: Option<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DryRunSummary {
    at_ms: i64,
    cases: u32,
    model_correct: u32,
    default_correct: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RatesExpect {
    rates: Vec<SourceRate>,
    gates: GatesExpect,
}

#[derive(Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
struct GatesExpect {
    auto_archive: GateState,
    model_enable: GateState,
    model_self_disable: GateState,
}

/// A case of the `prompt` section (AD-51): a case and a nonce in, the data
/// line the prompt carries out, or a refusal.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PromptCase {
    name: String,
    nonce: String,
    case: TriageCase,
    expect: PromptExpect,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PromptExpect {
    #[serde(default)]
    data_line: Option<String>,
    #[serde(default)]
    refused: bool,
}

/// A case of the `model_output` section (AD-51): raw stdout in, an answer
/// or an error out, and the usage when the case says it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelOutputCase {
    name: String,
    stdout: String,
    /// Spaces appended until stdout is this long, for the cap's cases.
    #[serde(default)]
    pad_to_bytes: Option<usize>,
    expect: OutputExpect,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputExpect {
    #[serde(default)]
    answer: Option<CallAnswer>,
    #[serde(default)]
    error: Option<ModelOutputError>,
    #[serde(default)]
    usage: Option<CallUsage>,
}
/// A case of the `triage` section: state in, rows out (AD-49, D-76, and
/// AD-52 for the model's part).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RowsCase {
    name: String,
    /// The triage model, off when absent.
    #[serde(default)]
    model: ModelCase,
    steps: Vec<Step>,
    expect: Vec<ExpectedRow>,
}

/// The triage model as a `triage` case writes it (AD-52, D-81, D-87): on
/// since `on_at`, paused, unavailable; the stored answers and the last
/// proposal call by session, each on the session's current case unless
/// `stale`.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ModelCase {
    #[serde(default)]
    on: bool,
    #[serde(default)]
    on_at: i64,
    #[serde(default)]
    paused: bool,
    #[serde(default)]
    unavailable: Option<TriageRefusal>,
    #[serde(default)]
    answers: BTreeMap<String, AnswerCase>,
    #[serde(default)]
    calls: BTreeMap<String, CallCase>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnswerCase {
    verdict: ModelVerdict,
    call_id: i64,
    #[serde(default)]
    stale: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CallCase {
    outcome: TriageOutcome,
    at: i64,
    #[serde(default)]
    stale: bool,
}

/// What a row shows of the model, beside its proposal.
#[derive(Deserialize, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum ExpectedModel {
    Pending,
    Answered,
    NoAnswer(TriageOutcome),
}

/// A row as the `triage` section writes it: the proposal as verdict,
/// source and reason code.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
struct ExpectedRow {
    kind: RowKind,
    key: String,
    subject: Option<i64>,
    session: Option<String>,
    proposal: (ModelVerdict, ProposalSource, ReasonCode),
    /// The call that answered a model proposal.
    #[serde(default)]
    call_id: Option<i64>,
    verdict: Option<Verdict>,
    proposed_subject: Option<i64>,
    choices: Vec<i64>,
    /// What the row shows of the model; nothing when absent.
    #[serde(default)]
    model: Option<ExpectedModel>,
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
    /// A confirmation of `subject` at `at_ms`, on the row's `session` when
    /// it had one, written directly, for the cases about what a
    /// confirmation answers.
    Confirmed {
        subject: i64,
        at_ms: i64,
        #[serde(default)]
        session: Option<String>,
    },
    /// The author confirms triage rows at `at_ms`, in one batch, and each
    /// item is answered as `answers` says (AD-49).
    Confirm {
        at_ms: i64,
        items: Vec<ConfirmItem>,
        answers: Vec<ItemAnswer>,
    },
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
    /// Every confirmation the author made, in the order written, with what
    /// it copied; a case without the key does not compare them.
    #[serde(default)]
    confirmations: Option<Vec<ExpectedConfirmation>>,
}

/// A confirmation as the `confirm` section writes it (AD-49, P-11, D-88).
#[derive(Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
struct ExpectedConfirmation {
    subject: i64,
    session: Option<String>,
    proposed: (ModelVerdict, ProposalSource, ReasonCode),
    call_id: Option<i64>,
    verdict: Verdict,
    input: Option<ConfirmationInput>,
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

/// One confirmation: what reopening and the rows read, and, for one the
/// author made, the proposal it answered, its verdict and its input.
struct Confirmation {
    subject: i64,
    session: Option<String>,
    at_ms: i64,
    reopened_at_ms: Option<i64>,
    answered: Option<(Proposal, Verdict, Option<ConfirmationInput>)>,
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
        let known_projects = self.known_projects();
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

    fn confirmation_records(&self) -> Vec<ConfirmationRecord> {
        self.confirmations
            .iter()
            .map(|c| ConfirmationRecord {
                subject_id: c.subject,
                session_id: c.session.clone(),
                confirmed_at_ms: c.at_ms,
                reopened_at_ms: c.reopened_at_ms,
            })
            .collect()
    }

    fn known_projects(&self) -> Vec<String> {
        self.rows
            .values()
            .map(|r| r.project.clone())
            .chain(self.subjects.iter().map(|s| s.project.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// One item, as the writer applies it: the filing plan, then the
    /// verdict on the subject, its excerpts and the confirmation.
    fn confirm(&mut self, item: &ConfirmItem, at_ms: i64) -> ItemOutcome {
        let sessions: Vec<SessionRecord> = self.records().into_values().collect();
        let confirmations = self.confirmation_records();
        let excerpts: Vec<ExcerptRecord> = self.excerpts.values().cloned().collect();
        let known_projects = self.known_projects();
        let model = ModelInput::default();
        let context = TriageContext {
            subjects: &self.subjects,
            sessions: &sessions,
            confirmations: &confirmations,
            excerpts: &excerpts,
            model: &model,
            known_projects: &known_projects,
        };
        let planned = match confirm_item(item, &context) {
            Ok(planned) => planned,
            Err(outcome) => return outcome,
        };
        let before = self.subjects.len();
        if let Some(filing) = &planned.filing {
            self.apply(filing, at_ms);
        }
        let subject = match planned.subject {
            Target::Existing(id) => id,
            Target::Created => {
                assert_eq!(self.subjects.len(), before + 1, "a subject was created");
                self.subjects.last().unwrap().id
            }
        };
        if let Ok(decision) = &planned.decision {
            let found = self.subjects.iter_mut().find(|s| s.id == subject).unwrap();
            found.status = Sourced {
                value: decision.status,
                src: Source::Author,
                at_ms,
            };
            if let Some(routine) = decision.routine {
                found.routine = routine;
            }
            for id in &decision.clear_excerpts {
                self.excerpts.remove(id);
            }
            self.confirmations.push(Confirmation {
                subject,
                session: decision.session_id.clone(),
                at_ms,
                reopened_at_ms: None,
                answered: Some((decision.proposal, decision.verdict, decision.input.clone())),
            });
        }
        planned.outcome(subject)
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
                Step::Confirmed {
                    subject,
                    at_ms,
                    session,
                } => self.confirmations.push(Confirmation {
                    subject: *subject,
                    session: session.clone(),
                    at_ms: *at_ms,
                    reopened_at_ms: None,
                    answered: None,
                }),
                Step::Confirm {
                    at_ms,
                    items,
                    answers,
                } => {
                    let answered: Vec<ItemAnswer> = items
                        .iter()
                        .map(|item| ItemAnswer {
                            key: item.key.clone(),
                            outcome: self.confirm(item, *at_ms),
                        })
                        .collect();
                    assert_eq!(&answered, answers, "{case}");
                }
            }
        }
    }
}

/// The model's facts a `triage` case describes, its hashes taken from the
/// registry's sessions as they stand.
fn model_facts(
    case: &ModelCase,
    sessions: &[SessionRecord],
    excerpts: &[ExcerptRecord],
) -> ModelFacts {
    let hash = |session: &str, stale: bool| {
        if stale {
            return Some("a case that changed since".to_owned());
        }
        let record = sessions.iter().find(|s| s.session_id == session)?;
        let excerpt = excerpts.iter().find(|e| e.session_id == session);
        case_of(record, excerpt).map(|c| c.input_hash())
    };
    let mut last_calls: BTreeMap<String, LastCall> = case
        .answers
        .iter()
        .map(|(session, answer)| {
            (
                case_key(session),
                LastCall {
                    outcome: TriageOutcome::Answer,
                    refusal: None,
                    input_hash: hash(session, answer.stale),
                    started_at_ms: case.on_at,
                },
            )
        })
        .collect();
    for (session, call) in &case.calls {
        last_calls.insert(
            case_key(session),
            LastCall {
                outcome: call.outcome,
                refusal: (call.outcome == TriageOutcome::Refused)
                    .then_some(TriageRefusal::WorkdirNotEmpty),
                input_hash: hash(session, call.stale),
                started_at_ms: call.at,
            },
        );
    }
    ModelFacts {
        setting: case.on.then_some(Sourced {
            value: ModelSetting {
                on: true,
                reason: None,
            },
            src: Source::Author,
            at_ms: case.on_at,
        }),
        pause: case.paused.then_some(Sourced {
            value: ModelPause {
                paused: true,
                stamp: None,
            },
            src: Source::Rules,
            at_ms: case.on_at,
        }),
        unavailable: case.unavailable,
        answers: case
            .answers
            .iter()
            .map(|(session, answer)| StoredAnswer {
                session_id: session.clone(),
                verdict: answer.verdict,
                reason: format!("the model's reason for {session}"),
                call_id: Some(answer.call_id),
                input_hash: hash(session, answer.stale),
                at_ms: case.on_at,
            })
            .collect(),
        last_calls,
    }
}

/// The rows that need a verdict, in the `triage` section's terms.
fn observed_rows(registry: &Registry, case: &ModelCase) -> Vec<ExpectedRow> {
    let sessions: Vec<SessionRecord> = registry.records().into_values().collect();
    let excerpts: Vec<ExcerptRecord> = registry.excerpts.values().cloned().collect();
    let confirmations = registry.confirmation_records();
    let facts = model_facts(case, &sessions, &excerpts);
    let model = facts.input(&sessions, &excerpts);
    rows(&VerdictInput {
        subjects: &registry.subjects,
        sessions: &sessions,
        confirmations: &confirmations,
        model: &model,
    })
    .into_iter()
    .map(|row| ExpectedRow {
        model: match row_model(&row, &facts, &sessions, &excerpts) {
            RowModel::None => None,
            RowModel::Pending => Some(ExpectedModel::Pending),
            RowModel::Answered { .. } => Some(ExpectedModel::Answered),
            RowModel::NoAnswer { outcome, .. } => Some(ExpectedModel::NoAnswer(outcome)),
        },
        kind: row.kind,
        key: row.key,
        subject: row.subject,
        session: row.session_id,
        proposal: (
            row.proposal.verdict,
            row.proposal.source,
            row.proposal.reason,
        ),
        call_id: row.proposal.call_id,
        verdict: row.proposal.definite(),
        proposed_subject: row.proposed_subject,
        choices: row.choices,
    })
    .collect()
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

/// What the registry holds, in the `filing` and `confirm` sections' terms;
/// the confirmations only when `expected` lists them.
fn observed_subjects(registry: &Registry, expected: &ExpectedSubjects) -> ExpectedSubjects {
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
        confirmations: expected.confirmations.as_ref().map(|_| {
            registry
                .confirmations
                .iter()
                .filter_map(|c| {
                    let (proposal, verdict, input) = c.answered.clone()?;
                    Some(ExpectedConfirmation {
                        subject: c.subject,
                        session: c.session.clone(),
                        proposed: (proposal.verdict, proposal.source, proposal.reason),
                        call_id: proposal.call_id,
                        verdict,
                        input,
                    })
                })
                .collect()
        }),
    }
}

#[test]
fn every_case_matches() {
    let set = load();
    assert!(
        !set.fold.is_empty()
            && !set.filing.is_empty()
            && !set.confirm.is_empty()
            && !set.triage.is_empty()
    );
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
                observed_subjects(&registry, &case.expect),
                case.expect,
                "{section} case: {}",
                case.name
            );
        }
    }
    for case in &set.triage {
        let mut registry = Registry::default();
        registry.run(&case.steps, &case.name);
        assert_eq!(
            observed_rows(&registry, &case.model),
            case.expect,
            "triage case: {}",
            case.name
        );
    }
    assert!(!set.prompt.is_empty() && !set.model_output.is_empty());
    for case in &set.prompt {
        replay_prompt(case);
    }
    for case in &set.model_output {
        replay_output(case);
    }
    assert!(!set.rates.is_empty());
    for case in &set.rates {
        replay_rates(case);
    }
}

/// The confirmations a `rates` case writes, its repeats expanded.
fn verdict_records(case: &RatesCase) -> Vec<VerdictRecord> {
    case.confirmations
        .iter()
        .flat_map(|c| {
            (0..c.times.unwrap_or(1)).map(move |k| VerdictRecord {
                id: c.id + k,
                subject_id: c.id + k,
                proposal: Proposal {
                    verdict: c.proposed.0,
                    source: c.proposed.1,
                    reason: c.proposed.2,
                    call_id: None,
                },
                verdict: c.verdict,
                auto: c.auto,
                confirmed_at_ms: c.at - k * 1_000,
                reopened_at_ms: c.reopened,
            })
        })
        .collect()
}

/// FR-11, FR-12, AD-50: every source's counts and each gate's state.
fn replay_rates(case: &RatesCase) {
    let records = verdict_records(case);
    let latest = case.latest_dry_run.as_ref().map(|d| Evaluation {
        at_ms: d.at_ms,
        tally: Tally {
            cases: d.cases,
            model_correct: d.model_correct,
            default_correct: d.default_correct,
            ..Tally::default()
        },
    });
    assert_eq!(
        rates(&records, case.now_ms),
        case.expect.rates,
        "rates case: {}",
        case.name
    );
    let found = gates(
        &records,
        latest.as_ref(),
        case.self_disabled_at,
        case.now_ms,
    );
    assert_eq!(
        GatesExpect {
            auto_archive: found.auto_archive.state,
            model_enable: found.model_enable.state,
            model_self_disable: found.model_self_disable.state,
        },
        case.expect.gates,
        "rates case gates: {}",
        case.name
    );
}

/// AD-51: the prompt carries the expected line, alone on its own line
/// between the markers, and the line reads back as the case; or the case
/// is refused for holding the nonce.
fn replay_prompt(case: &PromptCase) {
    let built = prompt(&case.case, &case.nonce);
    match (&case.expect.data_line, case.expect.refused) {
        (Some(line), false) => {
            let text = built.unwrap_or_else(|e| panic!("prompt case {}: {e}", case.name));
            let opening = format!(
                "<<<OFFICINA-DATA-{}
",
                case.nonce
            );
            let closing = format!(
                "
OFFICINA-DATA-{}>>>
",
                case.nonce
            );
            let start = text.find(&opening).expect("opening marker") + opening.len();
            let end = text.rfind(&closing).expect("closing marker");
            assert_eq!(&text[start..end], line, "prompt case: {}", case.name);
            assert!(
                line.chars().all(|c| !c.is_control()) && !line.contains(['<', '>']),
                "prompt case {}: the line holds a raw control or angle bracket",
                case.name
            );
            assert_eq!(
                serde_json::from_str::<TriageCase>(line).unwrap(),
                case.case,
                "prompt case {}: the line reads back as the case",
                case.name
            );
            assert_eq!(text.matches(&opening).count(), 1, "{}", case.name);
            assert_eq!(text.matches(&closing).count(), 1, "{}", case.name);
        }
        (None, true) => assert_eq!(
            built,
            Err(PromptError::NonceInInput),
            "prompt case: {}",
            case.name
        ),
        _ => panic!("prompt case {}: expect a data line or a refusal", case.name),
    }
}

/// AD-51: raw stdout in, the answer or the error out; the usage, read
/// permissively, when the case states it.
fn replay_output(case: &ModelOutputCase) {
    let mut stdout = case.stdout.clone().into_bytes();
    if let Some(len) = case.pad_to_bytes {
        assert!(len >= stdout.len(), "{}", case.name);
        stdout.resize(len, b' ');
        assert!(
            len.abs_diff(STDOUT_CAP_BYTES) <= 1,
            "{}: padding is for the cap's edge",
            case.name
        );
    }
    let (answer, usage) = read_output(&stdout);
    match (&case.expect.answer, case.expect.error) {
        (Some(expected), None) => {
            assert_eq!(
                answer.as_ref(),
                Ok(expected),
                "model_output case: {}",
                case.name
            )
        }
        (None, Some(error)) => assert_eq!(answer, Err(error), "model_output case: {}", case.name),
        _ => panic!(
            "model_output case {}: expect an answer or an error",
            case.name
        ),
    }
    if let Some(expected) = &case.expect.usage {
        assert_eq!(&usage, expected, "model_output case usage: {}", case.name);
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
    let mut confirm_refusals = HashSet::new();
    let mut not_needed = HashSet::new();
    let mut confirmed = HashSet::new();
    let steps = set
        .fold
        .iter()
        .flat_map(|case| &case.steps)
        .chain(
            set.filing
                .iter()
                .chain(&set.confirm)
                .flat_map(|case| &case.steps),
        )
        .chain(set.triage.iter().flat_map(|case| &case.steps));
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
            Step::Confirm { answers, .. } => {
                for answer in answers {
                    match answer.outcome {
                        ItemOutcome::Refused { refusal } => {
                            confirm_refusals.insert(refusal);
                        }
                        ItemOutcome::VerdictNotNeeded { cause, .. } => {
                            not_needed.insert(cause);
                        }
                        ItemOutcome::FilingRefused { refusal } => {
                            refusals.insert(refusal);
                        }
                        ItemOutcome::Confirmed { verdict, .. } => {
                            confirmed.insert(verdict);
                        }
                    }
                }
            }
            Step::Restart | Step::Confirmed { .. } => {}
        }
    }
    for refusal in ConfirmRefusal::ALL {
        assert!(
            confirm_refusals.contains(refusal),
            "no confirm case refusing an item for {refusal:?}"
        );
    }
    for cause in NotNeeded::ALL {
        assert!(
            not_needed.contains(cause),
            "no confirm case whose verdict is not needed for {cause:?}"
        );
    }
    for verdict in Verdict::ALL {
        assert!(
            confirmed.contains(verdict),
            "no confirm case confirming {verdict:?}"
        );
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
    let expected = || set.triage.iter().flat_map(|case| &case.expect);
    let kinds: HashSet<RowKind> = expected().map(|r| r.kind).collect();
    let reasons: HashSet<ReasonCode> = expected().map(|r| r.proposal.2).collect();
    let sources: HashSet<ProposalSource> = expected().map(|r| r.proposal.1).collect();
    let proposed: HashSet<ModelVerdict> = expected().map(|r| r.proposal.0).collect();
    let verdicts: HashSet<Verdict> = expected().filter_map(|r| r.verdict).collect();
    // Every provenance a case expects (D-45): the facts' sources, who filed
    // each session and set each subject's status, and a stored model
    // answer as a `model` row.
    let mut provenances: HashSet<Source> = set
        .fold
        .iter()
        .flat_map(|case| &case.expect)
        .flat_map(|session| session.facts.values().map(|(_, src)| *src))
        .collect();
    for case in set.filing.iter().chain(&set.confirm) {
        provenances.extend(case.expect.sessions.values().filter_map(|f| f.src));
        provenances.extend(case.expect.subjects.iter().map(|s| s.status_src));
    }
    if set.triage.iter().any(|case| !case.model.answers.is_empty()) {
        provenances.insert(Source::Model);
    }
    for source in Source::ALL {
        assert!(
            provenances.contains(source),
            "no case expecting a {source:?} provenance"
        );
    }
    let gate_states: HashSet<GateState> = set
        .rates
        .iter()
        .flat_map(|c| {
            [
                c.expect.gates.auto_archive,
                c.expect.gates.model_enable,
                c.expect.gates.model_self_disable,
            ]
        })
        .collect();
    for state in GateState::ALL {
        assert!(
            gate_states.contains(state),
            "no rates case for gate state {state:?}"
        );
    }
    let output_errors: HashSet<ModelOutputError> = set
        .model_output
        .iter()
        .filter_map(|c| c.expect.error)
        .collect();
    for error in ModelOutputError::ALL {
        assert!(
            output_errors.contains(error),
            "no model_output case for {error:?}"
        );
    }
    let answered: HashSet<ModelVerdict> = set
        .model_output
        .iter()
        .filter_map(|c| c.expect.answer.as_ref().map(|a| a.verdict))
        .collect();
    for verdict in ModelVerdict::ALL {
        assert!(
            answered.contains(verdict),
            "no model_output case answering {verdict:?}"
        );
    }
    for kind in RowKind::ALL {
        assert!(kinds.contains(kind), "no triage case for row kind {kind:?}");
    }
    for reason in ReasonCode::ALL {
        assert!(
            reasons.contains(reason),
            "no triage case for reason {reason:?}"
        );
    }
    for source in ProposalSource::ALL {
        assert!(
            sources.contains(source),
            "no triage case for proposal source {source:?}"
        );
    }
    for verdict in ModelVerdict::ALL {
        assert!(
            proposed.contains(verdict),
            "no triage case proposing {verdict:?}"
        );
    }
    for verdict in Verdict::ALL {
        assert!(
            verdicts.contains(verdict),
            "no triage case whose verdict is {verdict:?}"
        );
    }
}
