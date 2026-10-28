//! Ingesting event files (story 3.2 AC 2 to 5; AD-43, D-49, D-53, D-79,
//! D-84, D-97): passes over every incomplete file, offsets at line ends,
//! chunks, idempotent replay, session creation and excerpts. Every pass
//! runs at an explicit `now_ms`.

mod common;

use common::registry::{DAY, Home, NOW, answer, day_file, line, prompt};
use officina_core::board::ProjectRule;
use officina_core::dates::{day_start_ms, utc_date};
use officina_core::registry::ingestion::{CHUNK_MAX_LINES, COMPLETE_GRACE_MS};
use officina_core::registry::types::{EventFileRecord, ExcerptRecord};
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
