//! The Slice 0 counters of v0.4 (story 4.1; AD-178 gates 1, 4 and 5, D-518):
//! read-only passes over the Officina home's event buffer and registry that
//! print the figures each gate's research-notes entry records. Run by the
//! owner, by hand; never by a test, never part of a shipped binary.
//!
//! Usage: `cargo run -p officina-host --example slice0_probe -- <command>`,
//! the Officina home being `OFFICINA_HOME`, else `~/.officina`:
//!
//! - `subagents`: gate 1. Pairs every `SubagentStart` and `SubagentStop`
//!   line by `agent_id`, and checks that each start names a session that
//!   also sent a `UserPromptSubmit` (a lead).
//! - `notifications`: gate 4's count. `Notification` lines per local day
//!   and `notification_type`.
//! - `wait <session id>`: gate 4's case. The events of one session in order,
//!   each with the milliseconds since the line before it.
//! - `projects`: gate 5. Per local day, the distinct projects with a session
//!   live that day and the most live at one time, from `registry.db`.
//!
//! It prints counts, event names, notification types and times: never a
//! prompt, a message, a path or a project name, so its output can be pasted
//! into an entry as is. The registry is opened read-only; local days come
//! from SQLite's `localtime`, which reads the system's time zone.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::ExitCode;

use officina_core::events::{EventLine, ParsedLine, SUBAGENT_EVENTS, parse_line};
use officina_host::paths::{events_dir_of, officina_home_from_env};
use officina_host::registry::RegistryPaths;
use officina_host::registry::store::read_only_connection;
use rusqlite::Connection;

/// Every parsable line of every file in `events_dir`, oldest first.
fn all_events(events_dir: &Path) -> Vec<EventLine> {
    let mut lines = Vec::new();
    let Ok(entries) = std::fs::read_dir(events_dir) else {
        return lines;
    };
    for entry in entries.flatten() {
        let Ok(bytes) = std::fs::read(entry.path()) else {
            continue;
        };
        lines.extend(
            bytes
                .split(|b| *b == b'\n')
                .filter_map(|raw| match parse_line(raw) {
                    ParsedLine::Event(event) => Some(*event),
                    _ => None,
                }),
        );
    }
    lines.sort_by_key(|line| line.ts);
    lines
}

/// The local day (`YYYY-MM-DD`) of each time in `times_ms`, by SQLite.
fn local_days(times_ms: &[i64]) -> rusqlite::Result<Vec<String>> {
    let conn = Connection::open_in_memory()?;
    let mut statement = conn.prepare("SELECT date(?1 / 1000, 'unixepoch', 'localtime')")?;
    times_ms
        .iter()
        .map(|ms| statement.query_row([ms], |row| row.get(0)))
        .collect()
}

fn subagents(events: &[EventLine]) -> ExitCode {
    let leads: BTreeSet<&str> = events
        .iter()
        .filter(|l| l.event == "UserPromptSubmit")
        .filter_map(|l| l.session_id.as_deref())
        .collect();
    let lines: Vec<&EventLine> = events
        .iter()
        .filter(|l| SUBAGENT_EVENTS.contains(&l.event.as_str()))
        .collect();
    let (start_event, stop_event) = (SUBAGENT_EVENTS[0], SUBAGENT_EVENTS[1]);
    // Per agent id: start times, stop times, and whether a start named a lead.
    let mut agents: BTreeMap<&str, (Vec<i64>, Vec<i64>, bool)> = BTreeMap::new();
    let mut without_id = 0;
    let mut absent_fields: BTreeMap<String, u32> = BTreeMap::new();
    let mut starts_naming_a_lead = 0;
    let mut starts = 0;
    for line in &lines {
        for field in &line.absent {
            *absent_fields
                .entry(format!("{}.{field}", line.event))
                .or_default() += 1;
        }
        let Some(id) = line.agent_id.as_deref() else {
            without_id += 1;
            continue;
        };
        let entry = agents.entry(id).or_default();
        if line.event == start_event {
            starts += 1;
            let lead = line
                .session_id
                .as_deref()
                .is_some_and(|sid| leads.contains(sid));
            starts_naming_a_lead += usize::from(lead);
            entry.0.push(line.ts);
            entry.2 |= lead;
        } else {
            entry.1.push(line.ts);
        }
    }
    let started: Vec<_> = agents.values().filter(|a| !a.0.is_empty()).collect();
    let with_a_later_stop = started
        .iter()
        .filter(|(begin, end, _)| end.iter().any(|t| *t >= begin[0]))
        .count();
    let stops_without_start = agents.values().filter(|a| a.0.is_empty()).count();
    let repeated_starts = started.iter().filter(|a| a.0.len() > 1).count();
    let most_starts = started.iter().map(|a| a.0.len()).max().unwrap_or(0);
    let most_stops = started.iter().map(|a| a.1.len()).max().unwrap_or(0);
    println!("gate 1: sub-agent signals in the event buffer");
    println!(
        "  {start_event} lines: {starts}; {stop_event} lines: {}",
        lines.len() - starts - without_id
    );
    println!("  lines without agent_id: {without_id}");
    println!(
        "  starts naming a session that sent UserPromptSubmit: {starts_naming_a_lead} of {starts}"
    );
    println!(
        "  started agent ids: {}; with a later stop: {with_a_later_stop}",
        started.len()
    );
    println!(
        "  agent ids stopped without any start (prompt suggestions, /btw): {stops_without_start}"
    );
    println!(
        "  agent ids started more than once (resume, teammates): {repeated_starts}; most starts for one id: {most_starts}; most stops: {most_stops}"
    );
    for (field, count) in &absent_fields {
        println!("  absent {field}: {count}");
    }
    let pass = starts > 0
        && starts_naming_a_lead == starts
        && with_a_later_stop == started.len()
        && without_id == 0;
    println!(
        "  reading: {}",
        if starts == 0 {
            "no sub-agent seen yet: paste the hooks block again and run sessions with sub-agents"
        } else if pass {
            "every start names a lead and has a later stop (gate 1's pass, the /btw and teammate cases still to read)"
        } else {
            "a start names no lead or has no stop: gate 1 does not pass on these lines"
        }
    );
    ExitCode::SUCCESS
}

fn notifications(events: &[EventLine]) -> ExitCode {
    let lines: Vec<&EventLine> = events
        .iter()
        .filter(|l| l.event == "Notification")
        .collect();
    let days = match local_days(&lines.iter().map(|l| l.ts).collect::<Vec<_>>()) {
        Ok(days) => days,
        Err(e) => {
            eprintln!("slice0_probe: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut counts: BTreeMap<(String, String), u32> = BTreeMap::new();
    for (line, day) in lines.iter().zip(days) {
        let kind = line
            .notification_type
            .clone()
            .unwrap_or_else(|| "(none)".to_owned());
        *counts.entry((day, kind)).or_default() += 1;
    }
    println!("gate 4: Notification lines per local day and notification_type");
    for ((day, kind), count) in counts {
        println!("  {day}  {kind}: {count}");
    }
    ExitCode::SUCCESS
}

fn wait(events: &[EventLine], session: &str) -> ExitCode {
    let lines: Vec<&EventLine> = events
        .iter()
        .filter(|l| l.session_id.as_deref() == Some(session))
        .collect();
    if lines.is_empty() {
        eprintln!("slice0_probe: no line for that session id in the buffer");
        return ExitCode::FAILURE;
    }
    println!("gate 4: the events of one session, ms since the line before");
    let mut previous = lines[0].ts;
    for line in lines {
        let detail = line
            .notification_type
            .as_deref()
            .or(line.source.as_deref())
            .or(line.reason.as_deref())
            .unwrap_or("");
        println!("  +{:>8} ms  {} {detail}", line.ts - previous, line.event);
        previous = line.ts;
    }
    ExitCode::SUCCESS
}

/// Per local day: the distinct projects live that day, and the most live at
/// one time. A session is live from `first_seen_ms` to `last_seen_ms`.
const PROJECTS_PER_DAY: &str = "
WITH RECURSIVE span(project, day, last_day, first_ms, last_ms) AS (
    SELECT project,
           date(first_seen_ms / 1000, 'unixepoch', 'localtime'),
           date(last_seen_ms / 1000, 'unixepoch', 'localtime'),
           first_seen_ms, last_seen_ms
    FROM session
    UNION ALL
    SELECT project, date(day, '+1 day'), last_day, first_ms, last_ms
    FROM span WHERE day < last_day
)
SELECT day, COUNT(DISTINCT project) FROM span GROUP BY day ORDER BY day";

fn projects(home: &Path) -> ExitCode {
    let db = RegistryPaths::new(home).database();
    let run = || -> rusqlite::Result<()> {
        let conn = read_only_connection(&db)?;
        let per_day: Vec<(String, i64)> = conn
            .prepare(PROJECTS_PER_DAY)?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let sessions: Vec<(String, i64, i64)> = conn
            .prepare("SELECT project, first_seen_ms, last_seen_ms FROM session")?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        // A sweep over every start and end: at each instant, the projects
        // with at least one live session.
        let mut edges: Vec<(i64, bool, &str)> = sessions
            .iter()
            .flat_map(|(p, first, last)| [(*first, true, p.as_str()), (*last, false, p.as_str())])
            .collect();
        edges.sort_by_key(|(at, starts, _)| (*at, !*starts));
        let days = local_days(&edges.iter().map(|e| e.0).collect::<Vec<_>>())?;
        let mut live: BTreeMap<&str, u32> = BTreeMap::new();
        let mut most: BTreeMap<String, usize> = BTreeMap::new();
        for ((_, starts, project), day) in edges.iter().zip(days) {
            let count = live.entry(project).or_default();
            if *starts {
                *count += 1;
            } else {
                *count = count.saturating_sub(1);
            }
            let now_live = live.values().filter(|c| **c > 0).count();
            let best = most.entry(day).or_default();
            *best = (*best).max(now_live);
        }
        println!("gate 5: projects with a live session, per local day (registry, read-only)");
        for (day, distinct) in per_day {
            let at_once = most.get(&day).copied().unwrap_or(0);
            println!("  {day}  distinct: {distinct}  most at once: {at_once}");
        }
        Ok(())
    };
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("slice0_probe: the registry could not be read: {e}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let Some(home) = officina_home_from_env() else {
        eprintln!("slice0_probe: no Officina home (OFFICINA_HOME or the user profile)");
        return ExitCode::FAILURE;
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let events = || all_events(&events_dir_of(&home));
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["subagents"] => subagents(&events()),
        ["notifications"] => notifications(&events()),
        ["wait", session] => wait(&events(), session),
        ["projects"] => projects(&home),
        _ => {
            eprintln!(
                "usage: slice0_probe subagents | notifications | wait <session id> | projects"
            );
            ExitCode::from(64)
        }
    }
}
