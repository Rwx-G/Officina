//! Folding what Officina observes into session records (FR-3, AD-40,
//! AD-43, AD-46, AD-47, D-51, D-52, D-77, D-78): the facts an event line
//! and an agents snapshot carry, the supersession link, and the pivot of
//! `session_fact` rows into [`SessionRecord`]s. The host reads and writes
//! the rows; every decision is taken here.
//!
//! A fact is replaced only by an observation with a greater time
//! ([`replaces`]), so folding is order-independent and a replayed line
//! changes nothing (AD-43).

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::agents::{AgentRecord, LifecycleState, LiveStatus, SessionKind};
use crate::events::EventLine;

use super::types::{
    FactRow, FactValue, HookEnd, HookStart, SessionFact, SessionRecord, SessionRow, Source, Sourced,
};

/// How long after a `SessionEnd` of `clear` or `resume` the `SessionStart`
/// of the same source, in the same process, still links to it (D-52). A
/// guess, not a measurement: story 3.12 records the real gap on the
/// workstation, and this constant moves with that fact.
pub const SUPERSESSION_WINDOW_MS: i64 = 60_000;

/// The `SessionEnd` reasons, equal to the `SessionStart` sources, by which
/// one session replaces another in the same process (D-52).
pub const SUPERSEDING_REASONS: [&str; 2] = ["clear", "resume"];

/// A listed session whose facts did not change gets its `last_seen_ms`
/// written again once the stored one is this old (AD-46).
pub const LAST_SEEN_REFRESH_MS: i64 = 10 * 60_000;

/// One write to `session_fact`: it replaces the stored row only when
/// [`replaces`] says so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactWrite {
    pub session_id: String,
    pub fact: SessionFact,
    pub value: FactValue,
    pub src: Source,
    pub at_ms: i64,
}

/// True when an observation at `at_ms` replaces a fact stored at
/// `stored_at_ms`: only a strictly later one does, whatever the order the
/// observations arrive in (AD-43).
pub fn replaces(stored_at_ms: Option<i64>, at_ms: i64) -> bool {
    stored_at_ms.is_none_or(|stored| at_ms > stored)
}

/// The name serde gives a fieldless enum value.
fn wire_name<T: Serialize>(value: &T) -> Option<String> {
    serde_json::to_value(value)
        .ok()?
        .as_str()
        .map(str::to_owned)
}

/// The enum value serde reads from `text`.
fn from_wire_name<T: DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(text.to_owned())).ok()
}

fn text_value<T: Serialize>(value: &T) -> Option<FactValue> {
    wire_name(value).map(FactValue::Text)
}

fn json_value<T: Serialize>(value: &T) -> Option<FactValue> {
    serde_json::to_string(value).ok().map(FactValue::Text)
}

fn sourced<T>(row: &FactRow, value: T) -> Option<Sourced<T>> {
    Some(Sourced {
        value,
        src: row.src,
        at_ms: row.at_ms,
    })
}

/// Sets the field of `record` that `row` names; false when the row names a
/// fact this build does not know, or holds a value it cannot read.
fn apply_row(record: &mut SessionRecord, row: &FactRow) -> bool {
    let Some(fact) = SessionFact::from_code(&row.fact) else {
        return false;
    };
    let text = match &row.value {
        FactValue::Text(text) => Some(text.as_str()),
        FactValue::Integer(_) => None,
    };
    let integer = match row.value {
        FactValue::Integer(i) => Some(i),
        FactValue::Text(_) => None,
    };
    let slot_set = match fact {
        SessionFact::Kind => text
            .and_then(from_wire_name::<SessionKind>)
            .map(|v| record.kind = sourced(row, v)),
        SessionFact::Name => text.map(|v| record.name = sourced(row, v.to_owned())),
        SessionFact::Cwd => text.map(|v| record.cwd = sourced(row, v.to_owned())),
        SessionFact::ClaudePid => integer
            .and_then(|i| u32::try_from(i).ok())
            .map(|v| record.claude_pid = sourced(row, v)),
        SessionFact::BackgroundId => {
            text.map(|v| record.background_id = sourced(row, v.to_owned()))
        }
        SessionFact::Status => text
            .and_then(from_wire_name::<LiveStatus>)
            .map(|v| record.status = sourced(row, v)),
        SessionFact::State => text
            .and_then(from_wire_name::<LifecycleState>)
            .map(|v| record.state = sourced(row, v)),
        SessionFact::Listed => integer.map(|v| record.listed = sourced(row, v)),
        SessionFact::Started => text
            .and_then(|t| serde_json::from_str::<HookStart>(t).ok())
            .map(|v| record.started = sourced(row, v)),
        SessionFact::LastPromptAt => integer.map(|v| record.last_prompt_at = sourced(row, v)),
        SessionFact::LastStopAt => integer.map(|v| record.last_stop_at = sourced(row, v)),
        SessionFact::Ended => text
            .and_then(|t| serde_json::from_str::<HookEnd>(t).ok())
            .map(|v| record.ended = sourced(row, v)),
        SessionFact::Interrupted => integer.map(|v| record.interrupted = sourced(row, v)),
        SessionFact::Successor => text.map(|v| record.successor = sourced(row, v.to_owned())),
    };
    slot_set.is_some()
}

/// Pivots `session` rows and their `session_fact` rows into records, in
/// the order of `rows` (AD-40). A fact this build cannot read is counted in
/// the record's `unknown_facts`, never dropped silently.
///
/// ```
/// use officina_core::registry::fold::pivot;
/// use officina_core::registry::types::{FactRow, FactValue, SessionRow, Source};
/// let row = SessionRow {
///     session_id: "s1".into(), project: "api".into(),
///     first_seen_ms: 1, last_seen_ms: 2, subject: None,
/// };
/// let facts = [
///     FactRow { session_id: "s1".into(), fact: "cwd".into(),
///               value: FactValue::Text("C:/work/api".into()), src: Source::Hook, at_ms: 2 },
///     FactRow { session_id: "s1".into(), fact: "from_a_newer_officina".into(),
///               value: FactValue::Integer(1), src: Source::Rules, at_ms: 2 },
/// ];
/// let records = pivot(&[row], &facts);
/// assert_eq!(records[0].cwd.as_ref().unwrap().src, Source::Hook);
/// assert_eq!(records[0].unknown_facts, 1);
/// ```
pub fn pivot(rows: &[SessionRow], facts: &[FactRow]) -> Vec<SessionRecord> {
    let mut records: Vec<SessionRecord> = rows
        .iter()
        .map(|row| SessionRecord {
            session_id: row.session_id.clone(),
            project: row.project.clone(),
            first_seen_ms: row.first_seen_ms,
            last_seen_ms: row.last_seen_ms,
            subject: row.subject.clone(),
            ..SessionRecord::default()
        })
        .collect();
    let index: BTreeMap<String, usize> = records
        .iter()
        .enumerate()
        .map(|(i, r)| (r.session_id.clone(), i))
        .collect();
    for fact in facts {
        if let Some(&i) = index.get(&fact.session_id)
            && !apply_row(&mut records[i], fact)
        {
            records[i].unknown_facts += 1;
        }
    }
    records
}

/// The facts one event line records about its own session, with source
/// `hook` at the line's time (AD-43, story 3.3 AC 2): the working
/// directory, the process, and, by event, the start, the prompt's and the
/// `Stop`'s times, the end and its reason. A line without a session id
/// records nothing.
pub fn line_facts(line: &EventLine) -> Vec<FactWrite> {
    let Some(session_id) = line.session_id.as_deref().filter(|id| !id.is_empty()) else {
        return Vec::new();
    };
    let mut values: Vec<(SessionFact, Option<FactValue>)> = vec![
        (SessionFact::Cwd, line.cwd.clone().map(FactValue::Text)),
        (
            SessionFact::ClaudePid,
            line.claude_pid.map(|pid| FactValue::Integer(pid.into())),
        ),
    ];
    match line.event.as_str() {
        "SessionStart" => values.push((
            SessionFact::Started,
            json_value(&HookStart {
                source: line.source.clone(),
                claude_pid: line.claude_pid,
            }),
        )),
        "UserPromptSubmit" => {
            values.push((SessionFact::LastPromptAt, Some(FactValue::Integer(line.ts))))
        }
        "Stop" => values.push((SessionFact::LastStopAt, Some(FactValue::Integer(line.ts)))),
        "SessionEnd" => values.push((
            SessionFact::Ended,
            json_value(&HookEnd {
                reason: line.reason.clone(),
                claude_pid: line.claude_pid,
            }),
        )),
        _ => {}
    }
    values
        .into_iter()
        .filter_map(|(fact, value)| {
            Some(FactWrite {
                session_id: session_id.to_owned(),
                fact,
                value: value?,
                src: Source::Hook,
                at_ms: line.ts,
            })
        })
        .collect()
}

/// One end of a possible supersession (D-52): a `SessionEnd` or a
/// `SessionStart` of [`SUPERSEDING_REASONS`], with the process it ran in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub session_id: String,
    /// `clear` or `resume`.
    pub via: String,
    pub claude_pid: u32,
    pub at_ms: i64,
}

fn mark(session_id: &str, via: Option<&str>, claude_pid: Option<u32>, at_ms: i64) -> Option<Mark> {
    let via = via.filter(|v| SUPERSEDING_REASONS.contains(v))?;
    Some(Mark {
        session_id: session_id.to_owned(),
        via: via.to_owned(),
        claude_pid: claude_pid?,
        at_ms,
    })
}

impl Mark {
    /// The end mark a stored [`SessionFact::Ended`] fact makes, if any.
    pub fn of_end(session_id: &str, end: &Sourced<HookEnd>) -> Option<Mark> {
        mark(
            session_id,
            end.value.reason.as_deref(),
            end.value.claude_pid,
            end.at_ms,
        )
    }

    /// The start mark a stored [`SessionFact::Started`] fact makes, if any.
    pub fn of_start(session_id: &str, start: &Sourced<HookStart>) -> Option<Mark> {
        mark(
            session_id,
            start.value.source.as_deref(),
            start.value.claude_pid,
            start.at_ms,
        )
    }
}

/// True when the session ended at `end` is replaced by the one started at
/// `start` (D-52): same reason and source, same process, the start at or
/// after the end and within [`SUPERSESSION_WINDOW_MS`].
pub fn links(end: &Mark, start: &Mark) -> bool {
    end.session_id != start.session_id
        && end.via == start.via
        && end.claude_pid == start.claude_pid
        && start.at_ms >= end.at_ms
        && start.at_ms - end.at_ms <= SUPERSESSION_WINDOW_MS
}

/// The marks a line looks for among the other sessions' facts, and the
/// times they may have: an end looks for starts after it, a start for ends
/// before it. The host narrows its query with it; [`link_for_line`]
/// decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkQuery {
    /// [`SessionFact::Started`] facts timed within the range.
    Starts { from_ms: i64, to_ms: i64 },
    /// [`SessionFact::Ended`] facts timed within the range.
    Ends { from_ms: i64, to_ms: i64 },
}

fn line_mark(line: &EventLine) -> Option<(bool, Mark)> {
    let session_id = line.session_id.as_deref().filter(|id| !id.is_empty())?;
    match line.event.as_str() {
        "SessionEnd" => {
            mark(session_id, line.reason.as_deref(), line.claude_pid, line.ts).map(|m| (true, m))
        }
        "SessionStart" => {
            mark(session_id, line.source.as_deref(), line.claude_pid, line.ts).map(|m| (false, m))
        }
        _ => None,
    }
}

/// What `line` needs to know to link, if it can link at all: a line
/// without `claude_pid` (a v0.1 hook's) or of another reason links nothing.
pub fn candidates_needed(line: &EventLine) -> Option<MarkQuery> {
    let (is_end, mark) = line_mark(line)?;
    Some(if is_end {
        MarkQuery::Starts {
            from_ms: mark.at_ms,
            to_ms: mark.at_ms + SUPERSESSION_WINDOW_MS,
        }
    } else {
        MarkQuery::Ends {
            from_ms: mark.at_ms - SUPERSESSION_WINDOW_MS,
            to_ms: mark.at_ms,
        }
    })
}

/// The supersession link `line` makes with one of `candidates`, whichever
/// of the two lines is ingested first (D-52): an end takes the earliest
/// start linking to it, a start the latest end. The write sets the ended
/// session's [`SessionFact::Successor`], source `hook`, at the start's time.
pub fn link_for_line(line: &EventLine, candidates: &[Mark]) -> Option<FactWrite> {
    let (is_end, own) = line_mark(line)?;
    let (end, start) = if is_end {
        let start = candidates
            .iter()
            .filter(|start| links(&own, start))
            .min_by_key(|start| (start.at_ms, start.session_id.clone()))?;
        (own, start.clone())
    } else {
        let end = candidates
            .iter()
            .filter(|end| links(end, &own))
            .max_by_key(|end| (end.at_ms, end.session_id.clone()))?;
        (end.clone(), own)
    };
    Some(FactWrite {
        session_id: end.session_id,
        fact: SessionFact::Successor,
        value: FactValue::Text(start.session_id),
        src: Source::Hook,
        at_ms: start.at_ms,
    })
}

/// What `claude agents` reports about a session that the registry keeps
/// (AD-46): never `waitingFor`, which only the board shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListedFacts {
    pub kind: Option<SessionKind>,
    pub name: Option<String>,
    pub cwd: Option<String>,
    pub claude_pid: Option<u32>,
    pub background_id: Option<String>,
    pub status: Option<LiveStatus>,
    pub state: Option<LifecycleState>,
}

impl ListedFacts {
    /// What one `claude agents` record reports.
    pub fn of_agent(record: &AgentRecord) -> ListedFacts {
        ListedFacts {
            kind: record.kind,
            name: record.name.clone(),
            cwd: record.cwd.clone(),
            claude_pid: record.pid,
            background_id: record.id.clone(),
            status: record.status,
            state: record.state,
        }
    }

    /// What the registry last stored, whatever source wrote it.
    pub fn of_record(record: &SessionRecord) -> ListedFacts {
        ListedFacts {
            kind: record.kind.as_ref().map(|s| s.value),
            name: record.name.as_ref().map(|s| s.value.clone()),
            cwd: record.cwd.as_ref().map(|s| s.value.clone()),
            claude_pid: record.claude_pid.as_ref().map(|s| s.value),
            background_id: record.background_id.as_ref().map(|s| s.value.clone()),
            status: record.status.as_ref().map(|s| s.value),
            state: record.state.as_ref().map(|s| s.value),
        }
    }

    /// The values this report changes from `before`, one fact each. A field
    /// the report leaves out is unknown, not a change: the last known value
    /// stays (FR-3).
    fn changes_from(&self, before: &ListedFacts) -> Vec<(SessionFact, FactValue)> {
        fn changed<T: PartialEq + Clone>(
            now: &Option<T>,
            before: &Option<T>,
            value: impl Fn(&T) -> Option<FactValue>,
        ) -> Option<FactValue> {
            now.as_ref()
                .filter(|now| before.as_ref() != Some(*now))
                .and_then(value)
        }
        [
            (
                SessionFact::Kind,
                changed(&self.kind, &before.kind, text_value),
            ),
            (
                SessionFact::Name,
                changed(&self.name, &before.name, |v| {
                    Some(FactValue::Text(v.clone()))
                }),
            ),
            (
                SessionFact::Cwd,
                changed(&self.cwd, &before.cwd, |v| Some(FactValue::Text(v.clone()))),
            ),
            (
                SessionFact::ClaudePid,
                changed(&self.claude_pid, &before.claude_pid, |v| {
                    Some(FactValue::Integer((*v).into()))
                }),
            ),
            (
                SessionFact::BackgroundId,
                changed(&self.background_id, &before.background_id, |v| {
                    Some(FactValue::Text(v.clone()))
                }),
            ),
            (
                SessionFact::Status,
                changed(&self.status, &before.status, text_value),
            ),
            (
                SessionFact::State,
                changed(&self.state, &before.state, text_value),
            ),
        ]
        .into_iter()
        .filter_map(|(fact, value)| Some((fact, value?)))
        .collect()
    }

    /// `before` with every value this report gives.
    fn over(&self, before: &ListedFacts) -> ListedFacts {
        ListedFacts {
            kind: self.kind.or(before.kind),
            name: self.name.clone().or_else(|| before.name.clone()),
            cwd: self.cwd.clone().or_else(|| before.cwd.clone()),
            claude_pid: self.claude_pid.or(before.claude_pid),
            background_id: self
                .background_id
                .clone()
                .or_else(|| before.background_id.clone()),
            status: self.status.or(before.status),
            state: self.state.or(before.state),
        }
    }
}

/// One session of the previous agents snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Listed {
    facts: ListedFacts,
    /// The `last_seen_ms` the registry holds for it.
    last_seen_ms: i64,
}

/// The previous `claude agents` snapshot, held in memory by the collector
/// (AD-46): sessions by id, after routing to their chain head. Never
/// stored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentsSnapshot {
    sessions: BTreeMap<String, Listed>,
}

impl AgentsSnapshot {
    /// The snapshot a starting window assumes (D-51): the registry's
    /// sessions that are not ended and whose status came from `claude
    /// agents`, so a session that died while the window was closed is
    /// `interrupted` at the first successful refresh.
    pub fn seeded(records: &[SessionRecord]) -> AgentsSnapshot {
        AgentsSnapshot {
            sessions: records
                .iter()
                .filter(|r| r.status.as_ref().is_some_and(|s| s.src == Source::Agents))
                .filter(|r| !r.is_ended())
                .map(|r| {
                    (
                        r.session_id.clone(),
                        Listed {
                            facts: ListedFacts::of_record(r),
                            last_seen_ms: r.last_seen_ms,
                        },
                    )
                })
                .collect(),
        }
    }

    /// The session ids the snapshot holds.
    pub fn session_ids(&self) -> impl Iterator<Item = &str> {
        self.sessions.keys().map(String::as_str)
    }
}

/// A session `claude agents` began to list: the host creates it through the
/// one creation function (D-79), or widens its `last_seen_ms`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewlyListed {
    pub session_id: String,
    pub cwd: Option<String>,
}

/// What one successful refresh writes (AD-46).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentsChanges {
    /// Created first, so every fact write has its session.
    pub newly_listed: Vec<NewlyListed>,
    /// Changed facts with source `agents`, [`SessionFact::Listed`] for the
    /// sessions newly listed, and `interrupted` (source `rules`) for those
    /// gone without an end.
    pub writes: Vec<FactWrite>,
    /// Sessions whose `last_seen_ms` goes to the refresh's time: with a
    /// change, or once the stored one is [`LAST_SEEN_REFRESH_MS`] old.
    pub touched: Vec<String>,
}

/// The session a `claude agents` record of `session_id`, in process
/// `claude_pid`, stands for (AD-46, E14): a session superseded in that same
/// process stands for its successor, and so on to the chain's head. A
/// successor in another process, one `known` does not hold, or a session
/// that continued since it was replaced ends the chain.
pub fn chain_head(
    session_id: &str,
    claude_pid: Option<u32>,
    known: &BTreeMap<String, SessionRecord>,
) -> String {
    let mut head = session_id.to_owned();
    let mut seen = BTreeSet::new();
    while seen.insert(head.clone()) {
        let Some(next) = known
            .get(&head)
            .and_then(SessionRecord::current_successor)
            .map(|s| s.value.clone())
        else {
            break;
        };
        let next_pid = known
            .get(&next)
            .and_then(|r| r.claude_pid.as_ref())
            .map(|p| p.value);
        if claude_pid.is_none() || next_pid != claude_pid {
            break;
        }
        head = next;
    }
    head
}

/// Folds one successful `claude agents` refresh into the registry (AD-46,
/// D-51, D-78): diffs `records` with `previous`, after routing each record
/// to its chain head, and returns the next snapshot and what to write.
/// `known` holds the registry's records of every session involved: those
/// of `previous`, those listed, and their successors.
///
/// Only changed facts are written, with source `agents`; a session the
/// snapshot did not hold is created (D-79) and gets
/// [`SessionFact::Listed`]; a session gone from the list that is not ended
/// gets `interrupted`, source `rules`. Since each tick ingests the event
/// files first (D-78), a `SessionEnd` seen in the same tick is already
/// stored, and the session is not interrupted.
pub fn fold_agents(
    previous: &AgentsSnapshot,
    records: &[AgentRecord],
    known: &BTreeMap<String, SessionRecord>,
    now_ms: i64,
) -> (AgentsSnapshot, AgentsChanges) {
    let mut next: BTreeMap<String, Listed> = BTreeMap::new();
    let mut changes = AgentsChanges::default();
    let write = |session_id: &str, fact, value, src| FactWrite {
        session_id: session_id.to_owned(),
        fact,
        value,
        src,
        at_ms: now_ms,
    };
    for record in records {
        let Some(listed_id) = record.session_id.as_deref().filter(|id| !id.is_empty()) else {
            continue;
        };
        let head = chain_head(listed_id, record.pid, known);
        let reported = ListedFacts::of_agent(record);
        let held = next.get(&head).or_else(|| previous.sessions.get(&head));
        let before = held.map_or_else(
            || {
                known
                    .get(&head)
                    .map(ListedFacts::of_record)
                    .unwrap_or_default()
            },
            |l| l.facts.clone(),
        );
        let changed = reported.changes_from(&before);
        let any_change = !changed.is_empty();
        changes.writes.extend(
            changed
                .into_iter()
                .map(|(fact, value)| write(&head, fact, value, Source::Agents)),
        );
        let last_seen_ms = match held.map(|l| l.last_seen_ms) {
            None => {
                changes.newly_listed.push(NewlyListed {
                    session_id: head.clone(),
                    cwd: record.cwd.clone(),
                });
                changes.writes.push(write(
                    &head,
                    SessionFact::Listed,
                    FactValue::Integer(now_ms),
                    Source::Agents,
                ));
                now_ms
            }
            Some(last) if any_change || now_ms - last >= LAST_SEEN_REFRESH_MS => {
                if !changes.touched.contains(&head) {
                    changes.touched.push(head.clone());
                }
                now_ms
            }
            Some(last) => last,
        };
        next.insert(
            head,
            Listed {
                facts: reported.over(&before),
                last_seen_ms,
            },
        );
    }
    for gone in previous
        .sessions
        .keys()
        .filter(|id| !next.contains_key(*id))
    {
        if known.get(gone).is_some_and(|r| !r.is_ended()) {
            changes.writes.push(write(
                gone,
                SessionFact::Interrupted,
                FactValue::Integer(now_ms),
                Source::Rules,
            ));
        }
    }
    (AgentsSnapshot { sessions: next }, changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::parse_agents;
    use crate::registry::types::Ending;

    const NOW: i64 = 1_793_102_400_000;
    const PID: u32 = 4100;

    fn row(id: &str) -> SessionRow {
        SessionRow {
            session_id: id.to_owned(),
            project: "api".to_owned(),
            first_seen_ms: NOW - 1_000,
            last_seen_ms: NOW,
            subject: None,
        }
    }

    fn fact(id: &str, fact: &str, value: FactValue, src: Source, at_ms: i64) -> FactRow {
        FactRow {
            session_id: id.to_owned(),
            fact: fact.to_owned(),
            value,
            src,
            at_ms,
        }
    }

    /// A small registry in memory, folding as the host does: a fact is
    /// replaced only by a later one, and a line's own facts are stored
    /// before it looks for a link.
    #[derive(Default)]
    struct Memory {
        rows: BTreeMap<String, SessionRow>,
        facts: BTreeMap<(String, String), FactRow>,
    }

    impl Memory {
        fn ensure(&mut self, id: &str, at_ms: i64) {
            let entry = self
                .rows
                .entry(id.to_owned())
                .or_insert_with(|| SessionRow {
                    first_seen_ms: at_ms,
                    last_seen_ms: at_ms,
                    ..row(id)
                });
            entry.first_seen_ms = entry.first_seen_ms.min(at_ms);
            entry.last_seen_ms = entry.last_seen_ms.max(at_ms);
        }

        fn write(&mut self, w: FactWrite) {
            let key = (w.session_id.clone(), w.fact.code().to_owned());
            if replaces(self.facts.get(&key).map(|f| f.at_ms), w.at_ms) {
                self.facts.insert(
                    key,
                    fact(&w.session_id, w.fact.code(), w.value, w.src, w.at_ms),
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

        fn line(&mut self, line: &EventLine) {
            let Some(id) = line.session_id.clone() else {
                return;
            };
            self.ensure(&id, line.ts);
            for w in line_facts(line) {
                self.write(w);
            }
            if candidates_needed(line).is_some() {
                let marks: Vec<Mark> = self
                    .records()
                    .values()
                    .flat_map(|r| {
                        let end = r
                            .ended
                            .as_ref()
                            .and_then(|e| Mark::of_end(&r.session_id, e));
                        let start = r
                            .started
                            .as_ref()
                            .and_then(|s| Mark::of_start(&r.session_id, s));
                        [end, start]
                    })
                    .flatten()
                    .collect();
                if let Some(w) = link_for_line(line, &marks) {
                    self.write(w);
                }
            }
        }

        fn agents(&mut self, previous: &AgentsSnapshot, json: &str, now: i64) -> AgentsSnapshot {
            let records = parse_agents(json).unwrap();
            let (next, changes) = fold_agents(previous, &records, &self.records(), now);
            for n in changes.newly_listed {
                self.ensure(&n.session_id, now);
            }
            for w in changes.writes {
                self.write(w);
            }
            for id in changes.touched {
                self.ensure(&id, now);
            }
            next
        }
    }

    fn event(event: &str, id: &str, ts: i64) -> EventLine {
        EventLine {
            v: 1,
            ts,
            event: event.to_owned(),
            session_id: Some(id.to_owned()),
            cwd: Some("C:/work/api".to_owned()),
            source: None,
            reason: None,
            notification_type: None,
            message: None,
            prompt: None,
            last_message: None,
            last_message_tail: None,
            claude_pid: Some(PID),
            absent: Vec::new(),
            truncated: false,
        }
    }

    fn end(id: &str, reason: &str, ts: i64) -> EventLine {
        EventLine {
            reason: Some(reason.to_owned()),
            ..event("SessionEnd", id, ts)
        }
    }

    fn start(id: &str, source: &str, ts: i64) -> EventLine {
        EventLine {
            source: Some(source.to_owned()),
            ..event("SessionStart", id, ts)
        }
    }

    fn listed(id: &str, extra: &str) -> String {
        format!(
            r#"[{{"pid":{PID},"sessionId":"{id}","kind":"interactive","cwd":"C:/work/api","name":"api","status":"idle"{extra}}}]"#
        )
    }

    #[test]
    fn rows_pivot_into_sourced_fields() {
        let start = HookStart {
            source: Some("startup".to_owned()),
            claude_pid: Some(PID),
        };
        let facts = [
            fact(
                "s1",
                "kind",
                FactValue::Text("background".into()),
                Source::Agents,
                5,
            ),
            fact(
                "s1",
                "name",
                FactValue::Text("nightly".into()),
                Source::Agents,
                5,
            ),
            fact(
                "s1",
                "cwd",
                FactValue::Text("C:/work/api".into()),
                Source::Hook,
                6,
            ),
            fact(
                "s1",
                "claude_pid",
                FactValue::Integer(PID.into()),
                Source::Hook,
                6,
            ),
            fact(
                "s1",
                "background_id",
                FactValue::Text("bg01".into()),
                Source::Agents,
                5,
            ),
            fact(
                "s1",
                "status",
                FactValue::Text("busy".into()),
                Source::Agents,
                5,
            ),
            fact(
                "s1",
                "state",
                FactValue::Text("working".into()),
                Source::Agents,
                5,
            ),
            fact("s1", "listed", FactValue::Integer(5), Source::Agents, 5),
            fact(
                "s1",
                "started",
                FactValue::Text(serde_json::to_string(&start).unwrap()),
                Source::Hook,
                4,
            ),
            fact(
                "s1",
                "last_prompt_at",
                FactValue::Integer(7),
                Source::Hook,
                7,
            ),
            fact("s1", "last_stop_at", FactValue::Integer(8), Source::Hook, 8),
            fact("s1", "interrupted", FactValue::Integer(9), Source::Rules, 9),
            fact(
                "s1",
                "successor",
                FactValue::Text("s2".into()),
                Source::Hook,
                9,
            ),
        ];
        let mut with_subject = row("s1");
        with_subject.subject = Some(Sourced {
            value: Some(3),
            src: Source::Author,
            at_ms: 2,
        });
        let records = pivot(&[with_subject, row("s2")], &facts);
        let r = &records[0];
        assert_eq!(r.subject.as_ref().unwrap().value, Some(3));
        assert_eq!(
            r.kind.as_ref().map(|k| (k.value, k.src)),
            Some((SessionKind::Background, Source::Agents))
        );
        assert_eq!(r.name.as_ref().unwrap().value, "nightly");
        assert_eq!(
            r.cwd.as_ref().map(|c| (c.src, c.at_ms)),
            Some((Source::Hook, 6))
        );
        assert_eq!(r.claude_pid.as_ref().unwrap().value, PID);
        assert_eq!(r.background_id.as_ref().unwrap().value, "bg01");
        assert_eq!(r.status.as_ref().unwrap().value, LiveStatus::Busy);
        assert_eq!(r.state.as_ref().unwrap().value, LifecycleState::Working);
        assert_eq!(r.listed.as_ref().unwrap().at_ms, 5);
        assert_eq!(r.started.as_ref().unwrap().value, start);
        assert_eq!(r.last_prompt_at.as_ref().unwrap().value, 7);
        assert_eq!(r.last_stop_at.as_ref().unwrap().value, 8);
        assert_eq!(r.interrupted.as_ref().unwrap().src, Source::Rules);
        assert_eq!(r.successor.as_ref().unwrap().value, "s2");
        assert_eq!(r.unknown_facts, 0);
        assert_eq!(records[1].session_id, "s2");
        assert!(records[1].kind.is_none(), "facts stay with their session");
    }

    #[test]
    fn unknown_fact_names_are_counted() {
        let facts = [
            fact(
                "s1",
                "mood",
                FactValue::Text("calm".into()),
                Source::Model,
                5,
            ),
            fact("s1", "weather", FactValue::Integer(1), Source::Rules, 5),
            fact(
                "s1",
                "status",
                FactValue::Text("pondering".into()),
                Source::Agents,
                5,
            ),
            fact(
                "s1",
                "claude_pid",
                FactValue::Text("not a number".into()),
                Source::Hook,
                5,
            ),
            fact(
                "s1",
                "name",
                FactValue::Text("kept".into()),
                Source::Agents,
                5,
            ),
        ];
        let records = pivot(&[row("s1")], &facts);
        assert_eq!(records[0].unknown_facts, 4);
        assert_eq!(records[0].name.as_ref().unwrap().value, "kept");
        assert!(records[0].status.is_none() && records[0].claude_pid.is_none());
    }

    #[test]
    fn only_changed_facts_are_written() {
        let mut memory = Memory::default();
        let first = memory.agents(&AgentsSnapshot::default(), &listed("s1", ""), NOW);
        let facts: BTreeSet<SessionFact> = memory
            .facts
            .values()
            .filter_map(|f| SessionFact::from_code(&f.fact))
            .collect();
        assert_eq!(
            facts,
            BTreeSet::from([
                SessionFact::Kind,
                SessionFact::Name,
                SessionFact::Cwd,
                SessionFact::ClaudePid,
                SessionFact::Status,
                SessionFact::Listed,
            ])
        );
        assert!(memory.facts.values().all(|f| f.src == Source::Agents));

        let records = parse_agents(&listed("s1", "")).unwrap();
        let (_, unchanged) = fold_agents(&first, &records, &memory.records(), NOW + 1_000);
        assert_eq!(unchanged, AgentsChanges::default(), "nothing changed");

        let busy = parse_agents(&listed("s1", "").replace("idle", "busy")).unwrap();
        let (_, changed) = fold_agents(&first, &busy, &memory.records(), NOW + 2_000);
        let written: Vec<SessionFact> = changed.writes.iter().map(|w| w.fact).collect();
        assert_eq!(written, [SessionFact::Status]);
        assert_eq!(changed.writes[0].value, FactValue::Text("busy".into()));

        let waiting = parse_agents(
            &listed("s1", r#","waitingFor":"permission prompt""#).replace("idle", "waiting"),
        )
        .unwrap();
        let (_, changes) = fold_agents(&first, &waiting, &memory.records(), NOW + 3_000);
        assert!(
            changes
                .writes
                .iter()
                .all(|w| w.value != FactValue::Text("permission prompt".into())),
            "waitingFor is never stored"
        );

        let without_name = parse_agents(&listed("s1", "").replace(r#""name":"api","#, "")).unwrap();
        let (_, changes) = fold_agents(&first, &without_name, &memory.records(), NOW + 4_000);
        assert!(
            changes.writes.is_empty(),
            "a field left out is unknown, not a change"
        );
    }

    #[test]
    fn last_seen_with_a_change_or_after_10_min() {
        let mut memory = Memory::default();
        let snapshot = memory.agents(&AgentsSnapshot::default(), &listed("s1", ""), NOW);
        let records = parse_agents(&listed("s1", "")).unwrap();
        let (_, quiet) = fold_agents(
            &snapshot,
            &records,
            &memory.records(),
            NOW + LAST_SEEN_REFRESH_MS - 1,
        );
        assert!(quiet.touched.is_empty());
        let (later, old) = fold_agents(
            &snapshot,
            &records,
            &memory.records(),
            NOW + LAST_SEEN_REFRESH_MS,
        );
        assert_eq!(old.touched, ["s1"]);
        assert!(old.writes.is_empty(), "the time alone, no fact");
        let (_, after) = fold_agents(
            &later,
            &records,
            &memory.records(),
            NOW + LAST_SEEN_REFRESH_MS + 1_000,
        );
        assert!(after.touched.is_empty(), "counted from the last write");
        let busy = parse_agents(&listed("s1", "").replace("idle", "busy")).unwrap();
        let (_, changed) = fold_agents(&snapshot, &busy, &memory.records(), NOW + 1_000);
        assert_eq!(changed.touched, ["s1"], "with another change");
    }

    #[test]
    fn vanished_without_session_end_is_interrupted() {
        let mut memory = Memory::default();
        let snapshot = memory.agents(&AgentsSnapshot::default(), &listed("s1", ""), NOW);
        let next = memory.agents(&snapshot, "[]", NOW + 15_000);
        assert_eq!(next, AgentsSnapshot::default());
        let s1 = &memory.records()["s1"];
        assert_eq!(s1.interrupted.as_ref().map(|i| i.src), Some(Source::Rules));
        assert_eq!(s1.ending().map(|e| e.value), Some(Ending::Interrupted));

        // Ended through the hook before it vanished: the real end stays.
        let mut memory = Memory::default();
        let snapshot = memory.agents(&AgentsSnapshot::default(), &listed("s1", ""), NOW);
        memory.line(&end("s1", "prompt_input_exit", NOW + 5_000));
        memory.agents(&snapshot, "[]", NOW + 15_000);
        let s1 = &memory.records()["s1"];
        assert!(s1.interrupted.is_none());
        assert_eq!(
            s1.ending().map(|e| e.value),
            Some(Ending::SessionEnd {
                reason: Some("prompt_input_exit".to_owned())
            })
        );
    }

    #[test]
    fn record_of_a_superseded_session_goes_to_its_chain_head() {
        let mut memory = Memory::default();
        let snapshot = memory.agents(&AgentsSnapshot::default(), &listed("s1", ""), NOW);
        memory.line(&end("s1", "clear", NOW + 1_000));
        memory.line(&start("s2", "clear", NOW + 1_200));
        memory.line(&end("s2", "clear", NOW + 2_000));
        memory.line(&start("s3", "clear", NOW + 2_100));
        // `claude agents` still names the first session of the process.
        let busy = listed("s1", "").replace("idle", "busy");
        let next = memory.agents(&snapshot, &busy, NOW + 3_000);
        assert_eq!(next.session_ids().collect::<Vec<_>>(), ["s3"]);
        let records = memory.records();
        assert_eq!(
            records["s3"].status.as_ref().map(|s| s.value),
            Some(LiveStatus::Busy)
        );
        assert!(
            records["s1"].interrupted.is_none(),
            "s1 ended through the hook"
        );
        assert_eq!(
            records["s1"].status.as_ref().unwrap().value,
            LiveStatus::Idle
        );

        // Listed again in another process (`claude --resume s1`), s1 is
        // itself again.
        let elsewhere = listed("s1", "").replace(&PID.to_string(), "5200");
        let next = memory.agents(&next, &elsewhere, NOW + 4_000);
        assert!(next.session_ids().any(|id| id == "s1"));
        assert!(!memory.records()["s1"].is_ended(), "listed again: alive");
    }

    /// Adversarial pass: `/resume` back to a conversation another `/resume`
    /// left (D-75 keeps its id) makes it itself again; its old link no
    /// longer marks it superseded, and the chain does not loop back to the
    /// session that replaced it.
    #[test]
    fn a_session_resumed_after_its_successor_is_itself_again() {
        let mut memory = Memory::default();
        memory.line(&event("UserPromptSubmit", "s1", NOW));
        memory.line(&end("s1", "resume", NOW + 1_000));
        memory.line(&start("s3", "resume", NOW + 1_200));
        memory.line(&event("UserPromptSubmit", "s3", NOW + 2_000));
        memory.line(&end("s3", "resume", NOW + 3_000));
        memory.line(&start("s1", "resume", NOW + 3_200));
        memory.line(&event("UserPromptSubmit", "s1", NOW + 4_000));
        let records = memory.records();
        assert_eq!(records["s1"].successor.as_ref().unwrap().value, "s3");
        assert!(!records["s1"].is_superseded(), "it continued since");
        assert!(!records["s1"].is_ended());
        assert!(records["s3"].is_superseded());
        for named in ["s1", "s3"] {
            assert_eq!(chain_head(named, Some(PID), &records), "s1", "{named}");
        }
    }

    #[test]
    fn supersession_links_whichever_line_comes_first() {
        for (first, second) in [
            (end("a", "clear", NOW), start("b", "clear", NOW + 500)),
            (start("b", "clear", NOW + 500), end("a", "clear", NOW)),
            (
                end("a", "resume", NOW),
                start("b", "resume", NOW + SUPERSESSION_WINDOW_MS),
            ),
            (start("b", "resume", NOW), end("a", "resume", NOW)),
        ] {
            let mut memory = Memory::default();
            memory.line(&first);
            memory.line(&second);
            let records = memory.records();
            let successor = records["a"].successor.as_ref().expect("linked");
            assert_eq!(successor.value, "b");
            assert_eq!(successor.src, Source::Hook);
            assert!(records["b"].successor.is_none());
            assert!(records["a"].is_superseded() && !records["b"].is_superseded());
            assert!(records["b"].subject.is_none(), "the link files nothing");
        }
    }

    #[test]
    fn start_outside_the_window_links_nothing() {
        for (a, b) in [
            (
                end("a", "clear", NOW),
                start("b", "clear", NOW + SUPERSESSION_WINDOW_MS + 1),
            ),
            (end("a", "clear", NOW), start("b", "clear", NOW - 1)),
            (end("a", "clear", NOW), start("b", "resume", NOW + 100)),
            (end("a", "other", NOW), start("b", "startup", NOW + 100)),
            (
                end("a", "clear", NOW),
                EventLine {
                    claude_pid: Some(PID + 1),
                    ..start("b", "clear", NOW + 100)
                },
            ),
        ] {
            for order in [[&a, &b], [&b, &a]] {
                let mut memory = Memory::default();
                memory.line(order[0]);
                memory.line(order[1]);
                assert!(memory.records()["a"].successor.is_none(), "{:?}", order);
            }
        }
    }

    #[test]
    fn line_without_claude_pid_links_nothing() {
        let unpinned_end = EventLine {
            claude_pid: None,
            ..end("a", "clear", NOW)
        };
        let unpinned_start = EventLine {
            claude_pid: None,
            ..start("b", "clear", NOW + 100)
        };
        assert_eq!(candidates_needed(&unpinned_end), None);
        assert_eq!(candidates_needed(&unpinned_start), None);
        for (a, b) in [
            (unpinned_end.clone(), start("b", "clear", NOW + 100)),
            (end("a", "clear", NOW), unpinned_start.clone()),
        ] {
            for order in [[&a, &b], [&b, &a]] {
                let mut memory = Memory::default();
                memory.line(order[0]);
                memory.line(order[1]);
                assert!(memory.records()["a"].successor.is_none(), "{:?}", order);
            }
        }
    }

    #[test]
    fn seeding_keeps_live_sessions_whose_status_came_from_agents() {
        let mut memory = Memory::default();
        memory.agents(
            &AgentsSnapshot::default(),
            &format!(
                "[{},{}]",
                listed("live", "").trim_matches(['[', ']']),
                listed("ended", "")
                    .replace("4100", "4200")
                    .trim_matches(['[', ']'])
            ),
            NOW,
        );
        memory.line(&end("ended", "prompt_input_exit", NOW + 1_000));
        memory.line(&event("UserPromptSubmit", "hook-only", NOW + 1_000));
        let records: Vec<SessionRecord> = memory.records().into_values().collect();
        let seeded = AgentsSnapshot::seeded(&records);
        assert_eq!(seeded.session_ids().collect::<Vec<_>>(), ["live"]);
    }
}
