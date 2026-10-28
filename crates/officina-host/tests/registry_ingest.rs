//! Ingesting event files (story 3.2 AC 2 to 5; AD-43, D-49, D-53, D-79,
//! D-84, D-97): passes over every incomplete file, offsets at line ends,
//! chunks, idempotent replay, session creation and excerpts. Every pass
//! runs at an explicit `now_ms`.

mod common;

use common::registry::{DAY, Home, NOW, answer, day_file, line, prompt};
use officina_core::agents::parse_agents;
use officina_core::board::ProjectRule;
use officina_core::contract::HOOK_FIELDS;
use officina_core::dates::{day_start_ms, utc_date};
use officina_core::registry::ingestion::{CHUNK_MAX_LINES, COMPLETE_GRACE_MS};
use officina_core::registry::types::{
    Ending, EventFileRecord, ExcerptRecord, FactValue, SessionFact, Source,
};
use officina_host::collector::{ContractRecord, ContractStore, record_hook_fields};
use officina_host::doctor::BUFFERED_EVENTS;
use officina_host::registry::ingest::IngestContext;
use officina_host::registry::store::open_reader;

const CWD: &str = "C:/work/api";

/// Midnight starting today, UTC.
fn midnight() -> i64 {
    day_start_ms(utc_date(NOW))
}

fn row(home: &Home, name: &str) -> EventFileRecord {
    open_reader(&home.home, &[])
        .unwrap()
        .event_files()
        .unwrap()
        .into_iter()
        .find(|row| row.name == name)
        .unwrap_or_else(|| panic!("no row for {name}"))
}

fn excerpt(home: &Home, session: &str) -> Option<ExcerptRecord> {
    open_reader(&home.home, &[])
        .unwrap()
        .excerpts()
        .unwrap()
        .into_iter()
        .find(|e| e.session_id == session)
}

fn project_of(home: &Home, session: &str) -> String {
    open_reader(&home.home, &[])
        .unwrap()
        .sessions()
        .unwrap()
        .into_iter()
        .find(|s| s.session_id == session)
        .unwrap_or_else(|| panic!("no session {session}"))
        .project
}

fn file_len(home: &Home, name: &str) -> u64 {
    std::fs::metadata(home.events.join(name)).unwrap().len()
}

/// AD-43, D-53: valid names ascending, lines in file order. A session's
/// project is taken from the first line ingested, so it shows the order.
#[test]
fn files_are_ingested_oldest_first() {
    let home = Home::new();
    home.append(&day_file(NOW), &[line("Stop", "s1", NOW, "C:/work/newest")]);
    home.append(
        &day_file(NOW - DAY),
        &[
            line("SessionStart", "s1", NOW - DAY, "C:/work/older"),
            line("SessionStart", "s2", NOW - DAY, "C:/work/first-line"),
            line("Stop", "s2", NOW - DAY, "C:/work/second-line"),
        ],
    );
    home.append("notes.txt", &["not an event file\n".to_owned()]);
    let mut writer = home.writer(NOW);
    writer.ingest(&home.ctx(), NOW).unwrap();
    assert_eq!(project_of(&home, "s1"), "older");
    assert_eq!(project_of(&home, "s2"), "first-line");
    let names: Vec<String> = writer
        .event_files()
        .unwrap()
        .into_iter()
        .map(|r| r.name)
        .collect();
    assert_eq!(names, [day_file(NOW - DAY), day_file(NOW)]);
}

/// D-84: a window running across midnight completes yesterday's file in a
/// later pass, whatever triggers it.
#[test]
fn every_pass_covers_every_incomplete_file() {
    let home = Home::new();
    let yesterday = day_file(NOW - DAY);
    let older = day_file(NOW - 3 * DAY);
    home.append(&older, &[line("Stop", "s0", NOW - 3 * DAY, CWD)]);
    home.append(&yesterday, &[line("Stop", "s1", midnight() - 1_000, CWD)]);
    let mut writer = home.writer(midnight());
    writer.ingest(&home.ctx(), midnight() + 1_000).unwrap();
    assert!(row(&home, &older).complete_at_ms.is_some());
    assert_eq!(
        row(&home, &yesterday).complete_at_ms,
        None,
        "within the grace"
    );
    // A line the hook took at 23:59:59 lands after midnight.
    home.append(&yesterday, &[line("Stop", "s2", midnight() - 1, CWD)]);
    let report = writer
        .ingest(&home.ctx(), midnight() + COMPLETE_GRACE_MS)
        .unwrap();
    assert_eq!(report.lines, 1);
    assert_eq!(report.completed, std::slice::from_ref(&yesterday));
    assert_eq!(project_of(&home, "s2"), "api");
    assert_eq!(
        row(&home, &yesterday).ingested_bytes,
        file_len(&home, &yesterday)
    );
}

#[test]
fn offset_stops_at_the_last_newline() {
    let home = Home::new();
    let today = day_file(NOW);
    let complete = [line("Stop", "s1", NOW, CWD), line("Stop", "s2", NOW, CWD)];
    home.append(&today, &complete);
    home.append(&today, &["{\"v\":1,\"ts\":5,\"ev".to_owned()]);
    let mut writer = home.writer(NOW);
    writer.ingest(&home.ctx(), NOW).unwrap();
    let expected: usize = complete.iter().map(String::len).sum();
    assert_eq!(row(&home, &today).ingested_bytes, expected as u64);
    assert_eq!(row(&home, &today).corrupt_lines, 0);
}

#[test]
fn partial_tail_stays_unread_until_it_is_completed() {
    let home = Home::new();
    let today = day_file(NOW);
    let whole = line("UserPromptSubmit", "s-late", NOW, CWD);
    let (start, rest) = whole.split_at(whole.len() / 2);
    home.append(&today, &[line("Stop", "s1", NOW, CWD), start.to_owned()]);
    let mut writer = home.writer(NOW);
    writer.ingest(&home.ctx(), NOW).unwrap();
    assert!(
        writer
            .sessions()
            .unwrap()
            .iter()
            .all(|s| s.session_id != "s-late")
    );
    home.append(&today, &[rest.to_owned()]);
    let report = writer.ingest(&home.ctx(), NOW).unwrap();
    assert_eq!((report.lines, report.corrupt_lines), (1, 0));
    assert_eq!(project_of(&home, "s-late"), "api");
    assert_eq!(row(&home, &today).ingested_bytes, file_len(&home, &today));
}

#[test]
fn partial_tail_before_yesterday_is_one_corrupt_line() {
    let home = Home::new();
    let old = day_file(NOW - 2 * DAY);
    let yesterday = day_file(NOW - DAY);
    for name in [&old, &yesterday] {
        home.append(
            name,
            &[
                line("Stop", "s1", NOW - 2 * DAY, CWD),
                "{\"v\":1,\"ts".to_owned(),
            ],
        );
    }
    let mut writer = home.writer(NOW);
    writer.ingest(&home.ctx(), NOW).unwrap();
    let old_row = row(&home, &old);
    assert_eq!(old_row.corrupt_lines, 1);
    assert_eq!(old_row.ingested_bytes, file_len(&home, &old));
    assert!(old_row.complete_at_ms.is_some());
    let yesterday_row = row(&home, &yesterday);
    assert_eq!(
        yesterday_row.corrupt_lines, 0,
        "yesterday's tail may still be written"
    );
    assert!(yesterday_row.ingested_bytes < file_len(&home, &yesterday));
    assert_eq!(yesterday_row.complete_at_ms, None, "not fully read");
}

#[test]
fn yesterday_completes_after_the_grace_in_a_growth_pass() {
    let home = Home::new();
    let yesterday = day_file(NOW - DAY);
    let today = day_file(NOW);
    home.append(&yesterday, &[line("Stop", "s1", NOW - DAY, CWD)]);
    let mut writer = home.writer(midnight());
    writer
        .ingest(&home.ctx(), midnight() + COMPLETE_GRACE_MS - 1)
        .unwrap();
    assert_eq!(row(&home, &yesterday).complete_at_ms, None);
    // Today's file grows: the pass it triggers covers yesterday's too.
    home.append(
        &today,
        &[line("Stop", "s2", midnight() + COMPLETE_GRACE_MS, CWD)],
    );
    let at = midnight() + COMPLETE_GRACE_MS;
    let report = writer.ingest(&home.ctx(), at).unwrap();
    assert_eq!(report.completed, std::slice::from_ref(&yesterday));
    assert_eq!(row(&home, &yesterday).complete_at_ms, Some(at));
    assert_eq!(
        row(&home, &today).complete_at_ms,
        None,
        "today's file is never complete"
    );
}

#[test]
fn corrupt_lines_are_counted_per_file() {
    let home = Home::new();
    let first = day_file(NOW - DAY);
    let second = day_file(NOW);
    home.append(
        &first,
        &[line("Stop", "s1", NOW - DAY, CWD), "garbage\n".to_owned()],
    );
    home.append(
        &second,
        &[
            "{\"v\":1,\"bad\":\n".to_owned(),
            "\u{FFFD} not json\n".to_owned(),
            "{\"v\":2,\"ts\":1,\"event\":\"Future\"}\n".to_owned(),
            "\n".to_owned(),
            line("Stop", "s2", NOW, CWD),
        ],
    );
    let mut writer = home.writer(NOW);
    let report = writer.ingest(&home.ctx(), NOW).unwrap();
    assert_eq!(report.corrupt_lines, 3);
    assert_eq!(row(&home, &first).corrupt_lines, 1);
    assert_eq!(
        row(&home, &second).corrupt_lines,
        2,
        "another version and a blank line are not corrupt"
    );
}

/// D-84: a file shorter than its offset was rewritten; folding is
/// idempotent, so it is read again from 0 and only its corrupt count
/// restarts.
#[test]
fn a_file_shorter_than_its_offset_is_reingested_and_reported() {
    let home = Home::new();
    let today = day_file(NOW);
    home.append(
        &today,
        &[
            line("Stop", "s1", NOW, CWD),
            "garbage\n".to_owned(),
            line("Stop", "s2", NOW, CWD),
        ],
    );
    let mut writer = home.writer(NOW);
    writer.ingest(&home.ctx(), NOW).unwrap();
    assert_eq!(row(&home, &today).corrupt_lines, 1);
    std::fs::write(home.events.join(&today), line("Stop", "s3", NOW, CWD)).unwrap();
    let report = writer.ingest(&home.ctx(), NOW).unwrap();
    assert_eq!(report.rewritten, std::slice::from_ref(&today));
    let rewritten = row(&home, &today);
    assert_eq!(
        (
            rewritten.rewrites,
            rewritten.corrupt_lines,
            rewritten.ingested_bytes
        ),
        (1, 0, file_len(&home, &today))
    );
    let sessions: Vec<String> = writer
        .sessions()
        .unwrap()
        .into_iter()
        .map(|s| s.session_id)
        .collect();
    assert_eq!(sessions, ["s1", "s2", "s3"]);
}

/// AD-43: one transaction per chunk. Inside the second chunk's
/// transaction, another connection already reads the first chunk's offset
/// and no more.
#[test]
fn chunk_boundary_commits_each_chunk() {
    let home = Home::new();
    let today = day_file(NOW);
    let lines: Vec<String> = (0..CHUNK_MAX_LINES + 10)
        .map(|i| line("Stop", &format!("s{i:05}"), NOW + i as i64, CWD))
        .collect();
    home.append(&today, &lines);
    let first_chunk: usize = lines[..CHUNK_MAX_LINES].iter().map(String::len).sum();
    let mut writer = home.writer(NOW);
    let reader = open_reader(&home.home, &[]).unwrap();
    let mut seen_inside_second = None;
    writer
        .ingest_observed(&home.ctx(), NOW, &mut |read| {
            if read == CHUNK_MAX_LINES as u64 + 1 {
                let rows = reader.event_files().unwrap();
                seen_inside_second =
                    Some((rows[0].ingested_bytes, reader.sessions().unwrap().len()));
            }
        })
        .unwrap();
    assert_eq!(
        seen_inside_second,
        Some((first_chunk as u64, CHUNK_MAX_LINES))
    );
    assert_eq!(row(&home, &today).ingested_bytes, file_len(&home, &today));
}

/// Everything the registry holds about ingestion, for comparisons.
fn state(
    home: &Home,
) -> (
    Vec<officina_core::registry::types::SessionRow>,
    Vec<ExcerptRecord>,
) {
    let reader = open_reader(&home.home, &[]).unwrap();
    (reader.sessions().unwrap(), reader.excerpts().unwrap())
}

#[test]
fn replayed_chunk_changes_nothing() {
    let home = Home::new();
    let today = day_file(NOW);
    home.append(
        &today,
        &[
            prompt("s1", NOW - 5, CWD, "fix the build"),
            answer("s1", NOW - 3, CWD, "Fixed.", None),
            line("SessionEnd", "s1", NOW - 1, CWD),
            prompt("s2", NOW - 4, "C:/work/web", "add a test"),
        ],
    );
    let mut writer = home.writer(NOW);
    writer.ingest(&home.ctx(), NOW).unwrap();
    let before = state(&home);
    // A crash after the chunk's rows and before its offset is impossible,
    // since both commit together; a replay from 0 is what a rewrite or a
    // restored copy causes.
    rusqlite::Connection::open(home.paths().database())
        .unwrap()
        .execute("UPDATE event_file SET ingested_bytes = 0", [])
        .unwrap();
    writer.ingest(&home.ctx(), NOW).unwrap();
    assert_eq!(state(&home), before);
}

/// AD-43: a fact or excerpt is replaced only by a later observation,
/// whatever the order of the lines.
#[test]
fn later_observation_wins_whatever_the_order() {
    let home = Home::new();
    home.append(
        &day_file(NOW),
        &[
            prompt("s1", NOW - 10, CWD, "newer prompt"),
            prompt("s1", NOW - 20, CWD, "older prompt"),
            answer("s1", NOW - 5, CWD, "newer answer", Some("…newer end")),
            answer("s1", NOW - 15, CWD, "older answer", Some("…older end")),
            prompt("s2", NOW - 20, CWD, "older prompt"),
            prompt("s2", NOW - 10, CWD, "newer prompt"),
        ],
    );
    home.append(
        &day_file(NOW - DAY),
        &[line("SessionStart", "s1", NOW - DAY, CWD)],
    );
    let mut writer = home.writer(NOW);
    writer.ingest(&home.ctx(), NOW).unwrap();
    for session in ["s1", "s2"] {
        let stored = excerpt(&home, session).unwrap();
        assert_eq!(stored.prompt.as_deref(), Some("newer prompt"), "{session}");
        assert_eq!(stored.prompt_at_ms, Some(NOW - 10));
    }
    let s1 = excerpt(&home, "s1").unwrap();
    assert_eq!(
        (
            s1.answer_head.as_deref(),
            s1.answer_tail.as_deref(),
            s1.answer_at_ms
        ),
        (Some("newer answer"), Some("…newer end"), Some(NOW - 5))
    );
    let session = writer
        .sessions()
        .unwrap()
        .into_iter()
        .find(|s| s.session_id == "s1")
        .unwrap();
    assert_eq!(
        (session.first_seen_ms, session.last_seen_ms),
        (NOW - DAY, NOW - 5)
    );
}

fn projects(name: &str) -> Vec<ProjectRule> {
    vec![ProjectRule {
        name: name.to_owned(),
        path: "c:/work/api".to_owned(),
        color: None,
    }]
}

#[test]
fn first_line_creates_the_session_with_its_project() {
    let home = Home::new();
    home.append(
        &day_file(NOW),
        &[line("SessionStart", "s1", NOW - 7, "C:/work/api/src")],
    );
    let floor = projects("billing");
    let ctx = IngestContext {
        projects: &floor,
        ..home.ctx()
    };
    let mut writer = home.writer(NOW);
    writer.ingest(&ctx, NOW).unwrap();
    let sessions = writer.sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(
        (
            sessions[0].project.as_str(),
            sessions[0].first_seen_ms,
            sessions[0].last_seen_ms
        ),
        ("billing", NOW - 7, NOW - 7)
    );
}

/// D-53: the project is recorded at first sight; the author moves
/// sessions, a floor change does not.
#[test]
fn project_is_not_rewritten_by_a_floor_change() {
    let home = Home::new();
    let today = day_file(NOW);
    home.append(
        &today,
        &[line("SessionStart", "s1", NOW - 7, "C:/work/api")],
    );
    let before = projects("billing");
    let mut writer = home.writer(NOW);
    writer
        .ingest(
            &IngestContext {
                projects: &before,
                ..home.ctx()
            },
            NOW,
        )
        .unwrap();
    home.append(&today, &[line("Stop", "s1", NOW - 2, "C:/work/api")]);
    let after = projects("invoices");
    writer
        .ingest(
            &IngestContext {
                projects: &after,
                ..home.ctx()
            },
            NOW,
        )
        .unwrap();
    assert_eq!(project_of(&home, "s1"), "billing");
}

/// Security audit M1: a line dated in the future is recorded at the time
/// of its ingestion, so its excerpt expires within the privacy bound and a
/// genuine later prompt still replaces it; the session's times stay
/// within its file's day.
#[test]
fn a_future_dated_line_is_recorded_no_later_than_its_ingestion() {
    let home = Home::new();
    let year_2100 = 4_102_444_800_000;
    home.append(
        &day_file(NOW),
        &[prompt("s1", year_2100, CWD, "forged into the future")],
    );
    let mut writer = home.writer(NOW);
    writer.ingest(&home.ctx(), NOW).unwrap();
    let stored = excerpt(&home, "s1").unwrap();
    assert_eq!(stored.prompt_at_ms, Some(NOW));
    let session = writer.sessions().unwrap().remove(0);
    assert_eq!((session.first_seen_ms, session.last_seen_ms), (NOW, NOW));

    home.append(
        &day_file(NOW),
        &[prompt("s1", NOW + 10, CWD, "genuine later prompt")],
    );
    writer.ingest(&home.ctx(), NOW + 20).unwrap();
    let stored = excerpt(&home, "s1").unwrap();
    assert_eq!(
        (stored.prompt.as_deref(), stored.prompt_at_ms),
        (Some("genuine later prompt"), Some(NOW + 10))
    );
}

/// Security audit M2: a line longer than the hook writes is one corrupt
/// line, and an excerpt longer than the hook cuts is stored cut.
#[test]
fn text_beyond_the_hooks_bounds_never_reaches_the_registry() {
    use officina_core::events::{EXCERPT_CHARS, MAX_LINE_BYTES};
    let home = Home::new();
    let oversized = prompt("big", NOW - 9, CWD, &"x".repeat(MAX_LINE_BYTES));
    let long = prompt("s1", NOW - 5, CWD, &"y".repeat(3 * EXCERPT_CHARS));
    assert!(
        long.len() < MAX_LINE_BYTES,
        "the second line fits the hook's bound"
    );
    home.append(&day_file(NOW), &[oversized, long]);
    let mut writer = home.writer(NOW);
    let report = writer.ingest(&home.ctx(), NOW).unwrap();
    assert_eq!(report.corrupt_lines, 1);
    let sessions: Vec<String> = writer
        .sessions()
        .unwrap()
        .into_iter()
        .map(|s| s.session_id)
        .collect();
    assert_eq!(sessions, ["s1"]);
    let stored = excerpt(&home, "s1").unwrap().prompt.unwrap();
    assert_eq!(stored.chars().count(), EXCERPT_CHARS + 1);
}

#[test]
fn excerpts_keep_the_prompt_and_the_answer_head_and_tail() {
    let home = Home::new();
    home.append(
        &day_file(NOW),
        &[
            prompt("s1", NOW - 9, CWD, "rotate the certificates"),
            answer(
                "s1",
                NOW - 4,
                CWD,
                "Rotated two of three…",
                Some("…the third waits for approval."),
            ),
        ],
    );
    let mut writer = home.writer(NOW);
    writer.ingest(&home.ctx(), NOW).unwrap();
    assert_eq!(
        excerpt(&home, "s1").unwrap(),
        ExcerptRecord {
            session_id: "s1".to_owned(),
            prompt: Some("rotate the certificates".to_owned()),
            prompt_at_ms: Some(NOW - 9),
            answer_head: Some("Rotated two of three…".to_owned()),
            answer_tail: Some("…the third waits for approval.".to_owned()),
            answer_at_ms: Some(NOW - 4),
        }
    );
}

#[test]
fn no_excerpts_drops_excerpts_at_ingestion() {
    let home = Home::new();
    home.append(
        &day_file(NOW),
        &[
            prompt("s1", NOW - 9, CWD, "secret paste"),
            answer("s1", NOW - 4, CWD, "ok", Some("…end")),
        ],
    );
    let mut writer = home.writer(NOW);
    writer
        .ingest(
            &IngestContext {
                no_excerpts: true,
                ..home.ctx()
            },
            NOW,
        )
        .unwrap();
    assert_eq!(
        writer.sessions().unwrap().len(),
        1,
        "the session is recorded"
    );
    assert!(writer.excerpts().unwrap().is_empty());
}

/// AD-43, D-97: lines from the triage workdir never reach the registry,
/// however the path is spelled.
#[test]
fn triage_workdir_lines_are_skipped() {
    let home = Home::new();
    let workdir = home.home.join("triage").join("empty");
    let backslash = workdir.to_string_lossy().replace('/', "\\");
    let forward = backslash.replace('\\', "/");
    let mixed_case: String = forward
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if i % 2 == 0 {
                c.to_ascii_uppercase()
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect();
    home.append(
        &day_file(NOW),
        &[
            prompt("t1", NOW - 3, &backslash, "judge this"),
            prompt("t2", NOW - 2, &forward, "judge this"),
            prompt("t3", NOW - 1, &mixed_case, "judge this"),
            prompt("s1", NOW, CWD, "real work"),
        ],
    );
    let mut writer = home.writer(NOW);
    let report = writer.ingest(&home.ctx(), NOW).unwrap();
    assert_eq!(report.lines, 1);
    let sessions: Vec<String> = writer
        .sessions()
        .unwrap()
        .into_iter()
        .map(|s| s.session_id)
        .collect();
    assert_eq!(sessions, ["s1"]);
}

/// Adversarial pass: an entry named like an event file that cannot be read
/// (a folder, or a file deleted between the listing and the read, as an
/// older `board --json` still installed would do) must not stop the pass,
/// or every later file waits forever.
#[test]
fn an_unreadable_entry_does_not_stop_the_pass() {
    let home = Home::new();
    std::fs::create_dir_all(home.events.join(day_file(NOW - DAY))).unwrap();
    home.append(&day_file(NOW), &[line("Stop", "s1", NOW, CWD)]);
    let mut writer = home.writer(NOW);
    let report = writer.ingest(&home.ctx(), NOW).unwrap();
    assert_eq!(report.lines, 1);
    assert_eq!(project_of(&home, "s1"), "api");
}

/// One case of the `fold` section of `tests/vectors/triage.json`.
#[derive(serde::Deserialize)]
struct FoldCase {
    name: String,
    steps: Vec<serde_json::Value>,
    expect: Vec<ExpectedSession>,
}

#[derive(serde::Deserialize, Debug, PartialEq)]
struct ExpectedSession {
    session_id: String,
    #[serde(default)]
    project: Option<String>,
    facts: std::collections::BTreeMap<String, (serde_json::Value, Source)>,
    ended: Option<Ending>,
    superseded: bool,
    non_interactive: bool,
}

/// What the registry holds, in the vector set's terms.
fn observed(home: &Home, expected: &[ExpectedSession]) -> Vec<ExpectedSession> {
    let reader = open_reader(&home.home, &[]).unwrap();
    let facts = reader.facts().unwrap();
    reader
        .session_records()
        .unwrap()
        .into_iter()
        .map(|record| ExpectedSession {
            facts: facts
                .iter()
                .filter(|f| f.session_id == record.session_id)
                .map(|f| {
                    let value = match (&f.value, SessionFact::from_code(&f.fact)) {
                        (FactValue::Text(t), Some(SessionFact::Started | SessionFact::Ended)) => {
                            serde_json::from_str(t).unwrap()
                        }
                        (FactValue::Text(t), _) => t.clone().into(),
                        (FactValue::Integer(i), _) => (*i).into(),
                    };
                    (f.fact.clone(), (value, f.src))
                })
                .collect(),
            ended: record.ending().map(|e| e.value),
            superseded: record.is_superseded(),
            non_interactive: record.is_non_interactive(),
            project: expected
                .iter()
                .find(|e| e.session_id == record.session_id && e.project.is_some())
                .map(|_| record.project.clone()),
            session_id: record.session_id,
        })
        .collect()
}

/// Story 3.3 AC 8, derived-not-transcribed: the core replays the `fold`
/// section through its rules; this replays it through the real writer, the
/// event files and the agents snapshot, so the SQL and the rules agree.
#[test]
fn fold_vectors_hold_through_the_writer() {
    let set: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/vectors/triage.json")).unwrap();
    let cases: Vec<FoldCase> = serde_json::from_value(set["fold"].clone()).unwrap();
    assert!(!cases.is_empty());
    for case in cases {
        let home = Home::new();
        let mut writer = home.writer(NOW);
        let mut snapshot = None;
        for step in &case.steps {
            if let Some(lines) = step.get("events").and_then(|l| l.as_array()) {
                let mut latest = 0;
                for line in lines {
                    let ts = line["ts"].as_i64().unwrap();
                    latest = latest.max(ts);
                    home.append(&day_file(ts), &[format!("{line}\n")]);
                }
                writer.ingest(&home.ctx(), latest + 1_000).unwrap();
            } else if let Some(agents) = step.get("agents") {
                let records = parse_agents(&agents["records"].to_string()).unwrap();
                let now = agents["now_ms"].as_i64().unwrap();
                writer
                    .apply_agents(&mut snapshot, &records, &[], now)
                    .unwrap();
            } else {
                assert_eq!(step, "restart", "{}", case.name);
                snapshot = None;
            }
        }
        assert_eq!(
            observed(&home, &case.expect),
            case.expect,
            "fold case: {}",
            case.name
        );
    }
}

/// AD-46: no raw snapshot, no status history and no `waitingFor` reach the
/// registry, its file or its log: one row per fact, the latest.
#[test]
fn no_waiting_for_and_no_status_history_are_stored() {
    let home = Home::new();
    let mut writer = home.writer(NOW);
    let mut snapshot = None;
    let reports = [
        ("busy", None),
        ("waiting", Some("permission prompt")),
        ("idle", None),
        ("waiting", Some("input needed")),
    ];
    for (i, (status, waiting_for)) in reports.iter().enumerate() {
        let mut record = serde_json::json!({"pid": 4100, "sessionId": "s1", "kind": "interactive",
            "cwd": "C:/work/api", "name": "api", "status": status, "startedAt": NOW - 60_000});
        if let Some(waiting_for) = waiting_for {
            record["waitingFor"] = (*waiting_for).into();
        }
        let records = parse_agents(&format!("[{record}]")).unwrap();
        let at = NOW + i64::try_from(i).unwrap() * 1_000;
        writer
            .apply_agents(&mut snapshot, &records, &[], at)
            .unwrap();
    }
    let reader = open_reader(&home.home, &[]).unwrap();
    let facts = reader.facts().unwrap();
    let statuses: Vec<_> = facts
        .iter()
        .filter(|f| f.fact == SessionFact::Status.code())
        .collect();
    assert_eq!(statuses.len(), 1, "one row, no history");
    assert_eq!(
        (&statuses[0].value, statuses[0].at_ms),
        (&FactValue::Text("waiting".into()), NOW + 3_000)
    );
    assert!(
        facts
            .iter()
            .all(|f| SessionFact::from_code(&f.fact).is_some())
    );
    drop(reader);
    writer.checkpoint().unwrap();
    for (_, waiting_for) in reports {
        if let Some(text) = waiting_for {
            for file in [home.paths().database(), home.paths().wal()] {
                assert!(
                    !common::registry::file_contains(&file, text),
                    "{text} in {}",
                    file.display()
                );
            }
        }
    }
}

/// AD-43, D-79: filing runs in the transaction that creates the session,
/// on both creation paths. Inside the chunk's transaction another
/// connection sees no trace of the new session; once it commits, the
/// session and its filing are there together, filed when it was first seen.
#[test]
fn filing_runs_in_the_creating_transaction() {
    use officina_core::registry::confirm::SubjectOrigin;
    let home = Home::new();
    home.append(
        &day_file(NOW),
        &[line("SessionStart", "s0", NOW - 100, CWD)],
    );
    let mut writer = home.writer(NOW);
    writer.ingest(&home.ctx(), NOW).unwrap();
    let id = writer
        .create_subject("rollout", SubjectOrigin::Project("api".into()), NOW)
        .unwrap();
    home.append(
        &day_file(NOW),
        &[
            line("SessionStart", "s1", NOW + 5, CWD),
            line("Stop", "s1", NOW + 6, CWD),
        ],
    );
    let reader = open_reader(&home.home, &[]).unwrap();
    let mut seen_inside = Vec::new();
    writer
        .ingest_observed(&home.ctx(), NOW + 10, &mut |_| {
            seen_inside.push(
                reader
                    .session_records()
                    .unwrap()
                    .iter()
                    .any(|r| r.session_id == "s1"),
            );
        })
        .unwrap();
    assert_eq!(seen_inside, [false, false], "nothing before the commit");
    let filed_at_first_sight = |session: &str, at: i64| {
        let record = reader
            .session_records()
            .unwrap()
            .into_iter()
            .find(|r| r.session_id == session)
            .unwrap();
        let filing = record.subject.clone().unwrap();
        assert_eq!(
            (filing.value, filing.src, filing.at_ms, record.first_seen_ms),
            (Some(id), Source::Rules, at, at),
            "{session}"
        );
    };
    filed_at_first_sight("s1", NOW + 5);

    let mut snapshot = None;
    let records = parse_agents(
        r#"[{"pid":4100,"sessionId":"s2","kind":"interactive","cwd":"C:/work/api","status":"idle"}]"#,
    )
    .unwrap();
    writer
        .apply_agents(&mut snapshot, &records, &[], NOW + 20)
        .unwrap();
    filed_at_first_sight("s2", NOW + 20);
}

/// Adversarial pass: a line without a session id, or with an empty one,
/// records nothing and is not corrupt.
#[test]
fn a_line_without_a_session_id_records_nothing() {
    let home = Home::new();
    home.append(
        &day_file(NOW),
        &[
            "{\"v\":1,\"ts\":5,\"event\":\"Stop\"}\n".to_owned(),
            line("Stop", "", NOW, CWD),
        ],
    );
    let mut writer = home.writer(NOW);
    let report = writer.ingest(&home.ctx(), NOW).unwrap();
    assert_eq!((report.lines, report.corrupt_lines), (0, 0));
    assert!(writer.sessions().unwrap().is_empty());
}

/// The running version of the hook-field cases, and when it was first seen.
const VERSION: &str = "2.1.400";
const FIRST_SEEN: i64 = NOW - 60_000;

/// An event line of `event`, at `ts`, whose payload lacked `absent`.
fn lacking(event: &str, ts: i64, absent: &[&str]) -> String {
    let mut value =
        serde_json::json!({"v": 1, "ts": ts, "event": event, "session_id": "s1", "cwd": CWD});
    if !absent.is_empty() {
        value["absent"] = serde_json::json!(absent);
    }
    format!("{value}\n")
}

/// `state/contract.json` with `VERSION` first seen at `FIRST_SEEN`, every
/// buffered event not observed yet, as the agents check leaves a new
/// version's record.
fn contract_file(home: &Home) -> std::path::PathBuf {
    let path = home.home.join("state").join("contract.json");
    let mut store = ContractStore {
        schema: 1,
        ..ContractStore::default()
    };
    store.versions.insert(
        VERSION.to_owned(),
        ContractRecord {
            checked_at: FIRST_SEEN,
            held: true,
            missing: Vec::new(),
            not_observed: Vec::new(),
            first_seen_at: Some(FIRST_SEEN),
            hook_missing: Vec::new(),
            hook_not_observed: BUFFERED_EVENTS.iter().map(|e| (*e).to_owned()).collect(),
        },
    );
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    store.write(&path);
    path
}

/// The window's path: ingest the home's event files, then fold what the
/// lines said into `VERSION`'s record.
fn ingest_into_contract(home: &Home, contract: &std::path::Path) -> ContractRecord {
    let mut writer = home.writer(NOW);
    let report = writer.ingest(&home.ctx(), NOW).unwrap();
    record_hook_fields(contract, VERSION, &report.hook_sightings).unwrap()
}

/// Story 3.8 AC 3 (AD-56, D-71): a line carrying `absent` after the
/// version's first sighting adds `Event.field` to its `hook_missing`, in the
/// file the window and doctor read.
#[test]
fn absent_after_the_first_sighting_updates_hook_missing() {
    let (event, fields) = HOOK_FIELDS[2];
    let home = Home::new();
    let contract = contract_file(&home);
    home.append(&day_file(NOW), &[lacking(event, NOW - 1_000, &[fields[2]])]);
    let record = ingest_into_contract(&home, &contract);
    let expected = vec![format!("{event}.{}", fields[2])];
    assert_eq!(record.hook_missing, expected);
    let stored = ContractStore::read(&contract);
    assert_eq!(stored.versions[VERSION].hook_missing, expected);
    assert_eq!(stored.versions[VERSION].first_sighting(), FIRST_SEEN);
}

/// D-71: a line older than the version's first sighting describes the
/// version before, so an upgrade does not inherit its gaps.
#[test]
fn absent_before_the_first_sighting_is_ignored() {
    let (event, fields) = HOOK_FIELDS[0];
    let home = Home::new();
    let contract = contract_file(&home);
    home.append(
        &day_file(NOW),
        &[lacking(event, FIRST_SEEN - 1, &[fields[0]])],
    );
    let record = ingest_into_contract(&home, &contract);
    assert!(record.hook_missing.is_empty(), "{:?}", record.hook_missing);
    // Positive control: the same line after the sighting counts.
    home.append(&day_file(NOW), &[lacking(event, FIRST_SEEN, &[fields[0]])]);
    let record = ingest_into_contract(&home, &contract);
    assert_eq!(record.hook_missing, [format!("{event}.{}", fields[0])]);
}

/// AD-56: `hook_not_observed` lists the buffered events no line has
/// carried since the first sighting, and shrinks as they arrive.
#[test]
fn hook_not_observed_shrinks_as_events_arrive() {
    let home = Home::new();
    let contract = contract_file(&home);
    let (seen, before) = (BUFFERED_EVENTS[0], BUFFERED_EVENTS[1]);
    home.append(
        &day_file(NOW),
        &[
            lacking(seen, NOW - 1_000, &[]),
            lacking(before, FIRST_SEEN - 1, &[]),
        ],
    );
    let record = ingest_into_contract(&home, &contract);
    let left: Vec<String> = BUFFERED_EVENTS
        .iter()
        .filter(|e| **e != seen)
        .map(|e| (*e).to_owned())
        .collect();
    assert_eq!(
        record.hook_not_observed, left,
        "{before} came before the sighting"
    );
    let rest: Vec<String> = BUFFERED_EVENTS
        .iter()
        .map(|event| lacking(event, NOW - 500, &[]))
        .collect();
    home.append(&day_file(NOW), &rest);
    let record = ingest_into_contract(&home, &contract);
    assert!(record.hook_not_observed.is_empty());
    assert!(
        record.hook_missing.is_empty(),
        "complete lines miss nothing"
    );
}

/// D-71: a contract file a v0.2 window wrote, without the hook fields,
/// loads with its check time as the first sighting, nothing missing and
/// every buffered event not observed; a hook update then writes the fields.
#[test]
fn a_v0_2_contract_file_still_loads() {
    let home = Home::new();
    let path = home.home.join("state").join("contract.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let old = serde_json::json!({"schema": 1, "versions": {VERSION: {
        "checked_at": FIRST_SEEN, "held": true, "missing": [], "not_observed": ["waitingFor"]}}});
    std::fs::write(&path, old.to_string()).unwrap();
    let store = ContractStore::read(&path);
    let record = &store.versions[VERSION];
    assert_eq!(record.first_sighting(), FIRST_SEEN);
    assert!(record.hook_missing.is_empty());
    assert_eq!(record.hook_not_observed, BUFFERED_EVENTS);
    assert_eq!(record.not_observed, ["waitingFor"]);

    let (event, fields) = HOOK_FIELDS[1];
    home.append(&day_file(NOW), &[lacking(event, NOW - 1_000, &[fields[1]])]);
    let updated = ingest_into_contract(&home, &path);
    assert_eq!(updated.hook_missing, [format!("{event}.{}", fields[1])]);
    let written: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(written["versions"][VERSION]["first_seen_at"], FIRST_SEEN);
}
