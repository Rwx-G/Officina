//! Subjects through the window's writer (story 3.4 AC 1 to 6; AD-48, D-50,
//! D-54 to D-58, D-77, D-95): author actions, their refusals, their source
//! and time, the latest excerpts per subject, archive's clearing, and a
//! session alive again waking its subject. Every call runs at an explicit
//! `now_ms`.

mod common;

use std::collections::BTreeMap;

use common::registry::{DAY, Home, NOW, answer, day_file, file_contains, line, prompt};
use officina_core::agents::parse_agents;
use officina_core::board::ProjectRule;
use officina_core::registry::confirm::{
    AuthorAction, ConfirmItem, ConfirmationInput, ItemAnswer, ItemOutcome, SubjectOrigin,
};
use officina_core::registry::proposal::Proposal;
use officina_core::registry::retention::EXCERPT_TTL_MS;
use officina_core::registry::types::{
    AuthorRefusal, ConfirmRefusal, ModelVerdict, ProposalSource, ReasonCode, SessionRecord, Source,
    SubjectRecord, SubjectStatus, Verdict,
};
use officina_host::registry::ingest::IngestContext;
use officina_host::registry::store::{Writer, open_reader};
use officina_host::registry::write::AuthorError;
use serde::Deserialize;

const CWD: &str = "C:/work/api";

fn subjects(home: &Home) -> Vec<SubjectRecord> {
    open_reader(&home.home, &[]).unwrap().subjects().unwrap()
}

fn subject(home: &Home, id: i64) -> SubjectRecord {
    subjects(home).into_iter().find(|s| s.id == id).unwrap()
}

fn sessions(home: &Home) -> BTreeMap<String, SessionRecord> {
    open_reader(&home.home, &[])
        .unwrap()
        .session_records()
        .unwrap()
        .into_iter()
        .map(|r| (r.session_id.clone(), r))
        .collect()
}

fn excerpt_holders(home: &Home) -> Vec<String> {
    open_reader(&home.home, &[])
        .unwrap()
        .excerpts()
        .unwrap()
        .into_iter()
        .map(|e| e.session_id)
        .collect()
}

/// A home whose registry ingested `lines`, dated today, at `NOW`.
fn ingested(lines: &[String]) -> (Home, Writer) {
    let home = Home::new();
    home.append(&day_file(NOW), lines);
    let mut writer = home.writer(NOW);
    writer.ingest(&home.ctx(), NOW).unwrap();
    (home, writer)
}

fn refusal<T: std::fmt::Debug>(result: Result<T, AuthorError>) -> AuthorRefusal {
    match result {
        Err(AuthorError::Refused(refusal)) => refusal,
        other => panic!("not refused: {other:?}"),
    }
}

fn ids(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| (*n).to_owned()).collect()
}

/// Lists `sessions` in `claude agents`, interactive in `CWD`, so they are
/// seen live.
fn listed(
    writer: &mut Writer,
    snapshot: &mut Option<officina_core::registry::fold::AgentsSnapshot>,
    sessions: &[&str],
    at: i64,
) {
    let records: Vec<serde_json::Value> = sessions
        .iter()
        .enumerate()
        .map(|(i, id)| {
            serde_json::json!({"pid": 4100 + i, "sessionId": id, "kind": "interactive",
                "cwd": CWD, "name": "api", "status": "idle", "startedAt": NOW - DAY})
        })
        .collect();
    let records = parse_agents(&serde_json::Value::from(records).to_string()).unwrap();
    writer.apply_agents(snapshot, &records, &[], at).unwrap();
}

/// Inserts a confirmation of `subject` at `at_ms`, as story 3.7 will.
fn confirmed(home: &Home, subject: i64, at_ms: i64) {
    rusqlite::Connection::open(home.paths().database())
        .unwrap()
        .execute(
            "INSERT INTO confirmation (subject_id, proposed_verdict, proposed_source, proposed_reason,
                 verdict, confirmed_at_ms)
             VALUES (?1, 'finished', 'state', 'state_done', 'finished', ?2)",
            [subject, at_ms],
        )
        .unwrap();
}

/// Each confirmation stamped reopened: subject, confirmed at, reopened at.
fn reopened(home: &Home) -> Vec<(i64, i64, i64)> {
    let conn = rusqlite::Connection::open(home.paths().database()).unwrap();
    let mut statement = conn
        .prepare(
            "SELECT subject_id, confirmed_at_ms, reopened_at_ms FROM confirmation
             WHERE reopened_at_ms IS NOT NULL ORDER BY id",
        )
        .unwrap();
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

/// A case of the `filing` or `confirm` section of `tests/vectors/triage.json`.
#[derive(Deserialize)]
struct SubjectCase {
    name: String,
    steps: Vec<serde_json::Value>,
    expect: ExpectedSubjects,
}

#[derive(Deserialize, Debug, PartialEq)]
struct ExpectedSubjects {
    sessions: BTreeMap<String, Filing>,
    subjects: Vec<ExpectedSubject>,
    excerpts: Vec<String>,
    #[serde(default)]
    reopened: Vec<(i64, i64, i64)>,
    #[serde(default)]
    confirmations: Option<Vec<ExpectedConfirmation>>,
}

/// A `confirmation` row and its `confirmation_input`, as the `confirm`
/// section writes them.
#[derive(Deserialize, Debug, PartialEq)]
struct ExpectedConfirmation {
    subject: i64,
    session: Option<String>,
    proposed: (String, String, String),
    call_id: Option<i64>,
    verdict: String,
    input: Option<ConfirmationInput>,
}

/// Every confirmation the registry holds, by id, with its input.
fn confirmations(home: &Home) -> Vec<ExpectedConfirmation> {
    let conn = rusqlite::Connection::open(home.paths().database()).unwrap();
    let mut statement = conn
        .prepare(
            "SELECT c.subject_id, c.session_id, c.proposed_verdict, c.proposed_source,
                    c.proposed_reason, c.call_id, c.verdict, i.confirmation_id, i.prompt,
                    i.answer_head, i.answer_tail, i.end_reason, i.cut_mid_turn, i.expires_at_ms
             FROM confirmation c LEFT JOIN confirmation_input i ON i.confirmation_id = c.id
             ORDER BY c.id",
        )
        .unwrap();
    statement
        .query_map([], |row| {
            let input = row
                .get::<_, Option<i64>>(7)?
                .map(|_| -> rusqlite::Result<_> {
                    Ok(ConfirmationInput {
                        prompt: row.get(8)?,
                        answer_head: row.get(9)?,
                        answer_tail: row.get(10)?,
                        end_reason: row.get(11)?,
                        cut_mid_turn: row.get::<_, i64>(12)? == 1,
                        expires_at_ms: row.get(13)?,
                    })
                });
            Ok(ExpectedConfirmation {
                subject: row.get(0)?,
                session: row.get(1)?,
                proposed: (row.get(2)?, row.get(3)?, row.get(4)?),
                call_id: row.get(5)?,
                verdict: row.get(6)?,
                input: input.transpose()?,
            })
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

#[derive(Deserialize, Debug, PartialEq)]
struct Filing {
    filed: Option<i64>,
    src: Option<Source>,
}

#[derive(Deserialize, Debug, PartialEq)]
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

fn observed(home: &Home, expected: &ExpectedSubjects) -> ExpectedSubjects {
    ExpectedSubjects {
        confirmations: expected.confirmations.as_ref().map(|_| confirmations(home)),
        sessions: sessions(home)
            .into_values()
            .map(|record| {
                let filing = Filing {
                    filed: record.filed_subject(),
                    src: record.subject.as_ref().map(|s| s.src),
                };
                (record.session_id, filing)
            })
            .collect(),
        subjects: subjects(home)
            .into_iter()
            .map(|s| ExpectedSubject {
                id: s.id,
                project: s.project,
                name: s.name,
                status: s.status.value,
                status_src: s.status.src,
                status_at_ms: s.status.at_ms,
                routine: s.routine,
                merged_into: s.merged_into,
            })
            .collect(),
        excerpts: excerpt_holders(home),
        reopened: reopened(home),
    }
}

/// Story 3.4 AC 1 to 3, derived-not-transcribed: the core replays the
/// `filing` and `confirm` sections through its rules; this replays them
/// through the real writer, event files and snapshot.
#[test]
fn subject_vectors_hold_through_the_writer() {
    let set: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/vectors/triage.json")).unwrap();
    for section in ["filing", "confirm"] {
        let cases: Vec<SubjectCase> = serde_json::from_value(set[section].clone()).unwrap();
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
                } else if let Some(author) = step.get("author") {
                    let action: AuthorAction =
                        serde_json::from_value(author["action"].clone()).unwrap();
                    let expected: Option<AuthorRefusal> =
                        serde_json::from_value(author["refused"].clone()).unwrap_or(None);
                    let outcome = writer.author(&action, author["at_ms"].as_i64().unwrap());
                    assert_eq!(
                        outcome.err().and_then(|e| e.refusal()),
                        expected,
                        "{section} case {}: {action:?}",
                        case.name
                    );
                } else if let Some(confirmation) = step.get("confirmed") {
                    confirmed(
                        &home,
                        confirmation["subject"].as_i64().unwrap(),
                        confirmation["at_ms"].as_i64().unwrap(),
                    );
                } else if let Some(confirm) = step.get("confirm") {
                    let items: Vec<ConfirmItem> =
                        serde_json::from_value(confirm["items"].clone()).unwrap();
                    let answers: Vec<ItemAnswer> =
                        serde_json::from_value(confirm["answers"].clone()).unwrap();
                    let answered = writer
                        .confirm_triage(&items, confirm["at_ms"].as_i64().unwrap())
                        .unwrap();
                    assert_eq!(answered, answers, "{section} case {}", case.name);
                } else {
                    assert_eq!(step, "restart", "{}", case.name);
                    snapshot = None;
                }
            }
            assert_eq!(
                observed(&home, &case.expect),
                case.expect,
                "{section} case: {}",
                case.name
            );
        }
    }
}

#[test]
fn author_actions_round_trip() {
    let (home, mut writer) = ingested(&[
        prompt("s1", NOW - 9, CWD, "rotate the keys"),
        prompt("s2", NOW - 8, CWD, "renew the certificate"),
    ]);
    let rollout = writer
        .create_subject("rollout", SubjectOrigin::Project("api".into()), NOW)
        .unwrap();
    writer
        .file_sessions(&ids(&["s1", "s2"]), rollout, NOW + 1)
        .unwrap();
    writer
        .rename_subject(rollout, "key rollout", NOW + 2)
        .unwrap();
    let renewal = writer
        .create_subject("renewal", SubjectOrigin::Sessions(ids(&["s2"])), NOW + 3)
        .unwrap();
    writer.unfile_sessions(&ids(&["s1"]), NOW + 4).unwrap();
    writer.archive_subject(renewal, true, NOW + 5).unwrap();
    writer.reopen_subject(renewal, NOW + 6).unwrap();
    writer.merge_subjects(renewal, rollout, NOW + 7).unwrap();
    drop(writer);

    let read = subjects(&home);
    assert_eq!(
        read.iter()
            .map(|s| (
                s.id,
                s.name.as_str(),
                s.status.value,
                s.routine,
                s.merged_into
            ))
            .collect::<Vec<_>>(),
        [
            (rollout, "key rollout", SubjectStatus::Active, false, None),
            (
                renewal,
                "renewal",
                SubjectStatus::Archived,
                true,
                Some(rollout)
            ),
        ]
    );
    let sessions = sessions(&home);
    assert_eq!(sessions["s1"].filed_subject(), None);
    assert_eq!(sessions["s1"].subject.as_ref().unwrap().at_ms, NOW + 4);
    assert_eq!(sessions["s2"].filed_subject(), Some(rollout));
}

/// D-95: a subject created from sessions takes their project, whatever
/// the floor calls it, and files them; one created in a project names one
/// the registry knows.
#[test]
fn create_takes_the_project_of_the_selected_sessions() {
    let home = Home::new();
    home.append(
        &day_file(NOW),
        &[
            prompt("s1", NOW - 9, "C:/work/api/src", "a"),
            prompt("s2", NOW - 8, "C:/work/api", "b"),
            prompt("w1", NOW - 7, "C:/work/web", "c"),
        ],
    );
    let floor = [ProjectRule {
        name: "billing".into(),
        path: "c:/work/api".into(),
        color: None,
    }];
    let mut writer = home.writer(NOW);
    writer
        .ingest(
            &IngestContext {
                projects: &floor,
                ..home.ctx()
            },
            NOW,
        )
        .unwrap();
    let id = writer
        .create_subject("invoices", SubjectOrigin::Sessions(ids(&["s1", "s2"])), NOW)
        .unwrap();
    assert_eq!(subject(&home, id).project, "billing");
    let sessions = sessions(&home);
    assert_eq!(sessions["s1"].filed_subject(), Some(id));
    assert_eq!(sessions["s2"].filed_subject(), Some(id));
    assert_eq!(
        refusal(writer.create_subject("mixed", SubjectOrigin::Sessions(ids(&["s1", "w1"])), NOW)),
        AuthorRefusal::SessionsOfSeveralProjects
    );
    // Outside the floor's projects, w1 is in "other", a project too.
    let other = writer
        .create_subject("site", SubjectOrigin::Project("other".into()), NOW)
        .unwrap();
    assert_eq!(subject(&home, other).project, "other");
    assert_eq!(
        refusal(writer.create_subject("x", SubjectOrigin::Project("nowhere".into()), NOW)),
        AuthorRefusal::UnknownProject
    );
}

/// D-54: filing into an archived subject is refused and writes nothing;
/// once reopened, it is accepted.
#[test]
fn filing_into_an_archived_subject_is_refused() {
    let (home, mut writer) = ingested(&[prompt("s1", NOW - 9, CWD, "a")]);
    let id = writer
        .create_subject("rollout", SubjectOrigin::Project("api".into()), NOW)
        .unwrap();
    writer.archive_subject(id, false, NOW + 1).unwrap();
    let before = (sessions(&home), subjects(&home));
    assert_eq!(
        refusal(writer.file_sessions(&ids(&["s1"]), id, NOW + 2)),
        AuthorRefusal::Archived
    );
    assert_eq!(
        (sessions(&home), subjects(&home)),
        before,
        "nothing written"
    );
    writer.reopen_subject(id, NOW + 3).unwrap();
    writer.file_sessions(&ids(&["s1"]), id, NOW + 4).unwrap();
    assert_eq!(sessions(&home)["s1"].filed_subject(), Some(id));
}

/// D-55: merge moves the source's sessions, archives it with
/// `merged_into`, and the target is active when either was.
#[test]
fn merge_moves_sessions_and_marks_the_source() {
    let (home, mut writer) = ingested(&[
        prompt("s1", NOW - 9, CWD, "a"),
        prompt("s2", NOW - 8, CWD, "b"),
        prompt("s3", NOW - 7, CWD, "c"),
    ]);
    let source = writer
        .create_subject("first", SubjectOrigin::Sessions(ids(&["s1", "s2"])), NOW)
        .unwrap();
    let target = writer
        .create_subject("second", SubjectOrigin::Sessions(ids(&["s3"])), NOW)
        .unwrap();
    writer.archive_subject(target, false, NOW + 1).unwrap();
    assert_eq!(
        refusal(writer.merge_subjects(source, target, NOW + 2)),
        AuthorRefusal::Archived
    );
    writer.reopen_subject(target, NOW + 3).unwrap();
    writer.merge_subjects(source, target, NOW + 4).unwrap();
    let merged = subject(&home, source);
    assert_eq!(
        (merged.status.value, merged.merged_into),
        (SubjectStatus::Archived, Some(target))
    );
    assert_eq!(subject(&home, target).status.value, SubjectStatus::Active);
    let sessions = sessions(&home);
    for id in ["s1", "s2", "s3"] {
        assert_eq!(sessions[id].filed_subject(), Some(target), "{id}");
    }
    assert_eq!(
        refusal(writer.reopen_subject(source, NOW + 5)),
        AuthorRefusal::Merged
    );
}

/// D-57: trimmed, 1 to 80 characters, no control or hidden formatting
/// character, unique ignoring case among the project's open subjects.
#[test]
fn name_rules_are_enforced() {
    let (home, mut writer) = ingested(&[line("SessionStart", "s1", NOW - 9, CWD)]);
    let api = || SubjectOrigin::Project("api".into());
    for (name, refused) in [
        ("  \t ", AuthorRefusal::NameEmpty),
        (&*"x".repeat(81), AuthorRefusal::NameTooLong),
        ("tab\there", AuthorRefusal::NameHasForbiddenCharacters),
        (
            "zero\u{200B}width",
            AuthorRefusal::NameHasForbiddenCharacters,
        ),
        (
            "right\u{202E}left",
            AuthorRefusal::NameHasForbiddenCharacters,
        ),
        (
            "tag\u{E0041}char",
            AuthorRefusal::NameHasForbiddenCharacters,
        ),
    ] {
        assert_eq!(
            refusal(writer.create_subject(name, api(), NOW)),
            refused,
            "{name:?}"
        );
    }
    let id = writer.create_subject("  Rollout  ", api(), NOW).unwrap();
    assert_eq!(subject(&home, id).name, "Rollout");
    assert_eq!(
        refusal(writer.create_subject("ROLLOUT", api(), NOW)),
        AuthorRefusal::NameTaken
    );
    let other = writer.create_subject(&"é".repeat(80), api(), NOW).unwrap();
    assert_eq!(
        refusal(writer.rename_subject(other, "rollout", NOW)),
        AuthorRefusal::NameTaken
    );
    writer.archive_subject(id, false, NOW).unwrap();
    writer.rename_subject(other, "rollout", NOW).unwrap();
    assert_eq!(
        subject(&home, other).name,
        "rollout",
        "archived names are free"
    );
}

/// D-57: every refusal has a scenario that raises it, and a refused action
/// writes nothing. The `match` names every variant, so a new refusal fails
/// to compile here until it has one.
#[test]
fn every_author_refusal_has_a_case() {
    for refused in AuthorRefusal::ALL {
        let (home, mut writer) = ingested(&[
            prompt("s1", NOW - 9, CWD, "a"),
            prompt("w1", NOW - 8, "C:/work/web", "b"),
        ]);
        let api = || SubjectOrigin::Project("api".into());
        let open = writer.create_subject("open", api(), NOW).unwrap();
        let archived = writer.create_subject("archived", api(), NOW).unwrap();
        writer.archive_subject(archived, false, NOW).unwrap();
        let action = match refused {
            AuthorRefusal::NameEmpty => AuthorAction::Rename {
                subject: open,
                name: " ".into(),
            },
            AuthorRefusal::NameTooLong => AuthorAction::Rename {
                subject: open,
                name: "n".repeat(81),
            },
            AuthorRefusal::NameHasForbiddenCharacters => AuthorAction::Rename {
                subject: open,
                name: "a\u{7}b".into(),
            },
            AuthorRefusal::NameTaken => AuthorAction::Create {
                name: "OPEN".into(),
                origin: api(),
            },
            AuthorRefusal::UnknownSubject => AuthorAction::Archive {
                subject: 999,
                routine: false,
            },
            AuthorRefusal::UnknownSession => AuthorAction::File {
                sessions: ids(&["missing"]),
                subject: open,
            },
            AuthorRefusal::UnknownProject => AuthorAction::Create {
                name: "x".into(),
                origin: SubjectOrigin::Project("nowhere".into()),
            },
            AuthorRefusal::NoSession => AuthorAction::Unfile { sessions: vec![] },
            AuthorRefusal::SessionsOfSeveralProjects => AuthorAction::Create {
                name: "x".into(),
                origin: SubjectOrigin::Sessions(ids(&["s1", "w1"])),
            },
            AuthorRefusal::Archived => AuthorAction::File {
                sessions: ids(&["s1"]),
                subject: archived,
            },
            AuthorRefusal::NotArchived => AuthorAction::Reopen { subject: open },
            AuthorRefusal::Merged => {
                let merged = writer.create_subject("merged", api(), NOW).unwrap();
                writer.merge_subjects(merged, open, NOW).unwrap();
                AuthorAction::Reopen { subject: merged }
            }
            AuthorRefusal::SameSubject => AuthorAction::Merge {
                source: open,
                target: open,
            },
        };
        let before = (sessions(&home), subjects(&home), excerpt_holders(&home));
        assert_eq!(
            refusal(writer.author(&action, NOW + 1)),
            *refused,
            "{action:?}"
        );
        assert_eq!(
            (sessions(&home), subjects(&home), excerpt_holders(&home)),
            before,
            "{refused:?} wrote something"
        );
    }
}

/// AD-39, AD-40: author writes carry source `author` and their time in the
/// source columns they touch, on a connection that commits `FULL`.
#[test]
fn author_writes_carry_source_author_and_time() {
    let (home, mut writer) = ingested(&[prompt("s0", NOW - 9, CWD, "a")]);
    assert_eq!(writer.pragma("synchronous").as_deref(), Some("2"), "FULL");
    let id = writer
        .create_subject("rollout", SubjectOrigin::Project("api".into()), NOW)
        .unwrap();
    home.append(&day_file(NOW), &[prompt("s1", NOW + 10, CWD, "b")]);
    writer.ingest(&home.ctx(), NOW + 20).unwrap();
    let by_rule = sessions(&home)["s1"].subject.clone().unwrap();
    assert_eq!((by_rule.src, by_rule.at_ms), (Source::Rules, NOW + 10));
    writer.file_sessions(&ids(&["s0"]), id, NOW + 30).unwrap();
    writer.rename_subject(id, "key rollout", NOW + 40).unwrap();
    let status = subject(&home, id).status;
    assert_eq!(
        (status.src, status.at_ms),
        (Source::Author, NOW),
        "a rename touches no status"
    );
    writer.archive_subject(id, false, NOW + 50).unwrap();
    let status = subject(&home, id).status;
    assert_eq!((status.src, status.at_ms), (Source::Author, NOW + 50));
    let filed = sessions(&home)["s0"].subject.clone().unwrap();
    assert_eq!((filed.src, filed.at_ms), (Source::Author, NOW + 30));
}

/// FR-4, D-50: a session filed under a subject loses its excerpt row once
/// a later session of the subject has excerpts, including when it is filed
/// or merged into such a subject.
#[test]
fn older_session_loses_its_excerpts_once_a_later_one_has_them() {
    let (home, mut writer) = ingested(&[line("SessionStart", "s0", NOW - DAY, CWD)]);
    let id = writer
        .create_subject("rollout", SubjectOrigin::Project("api".into()), NOW)
        .unwrap();
    let mut snapshot = None;
    listed(
        &mut writer,
        &mut snapshot,
        &["s1", "s2", "s3", "s4"],
        NOW + 1,
    );
    // Filed at creation, s1 and s2; then s3 and s4 taken out of it.
    writer
        .unfile_sessions(&ids(&["s3", "s4"]), NOW + 2)
        .unwrap();
    home.append(
        &day_file(NOW),
        &[
            prompt("s3", NOW + 10, CWD, "older, unfiled"),
            prompt("s4", NOW + 11, CWD, "older, in another subject"),
            prompt("s1", NOW + 20, CWD, "first"),
        ],
    );
    writer.ingest(&home.ctx(), NOW + 100).unwrap();
    let other = writer
        .create_subject("other", SubjectOrigin::Sessions(ids(&["s4"])), NOW + 101)
        .unwrap();
    assert_eq!(excerpt_holders(&home), ["s1", "s3", "s4"]);
    home.append(&day_file(NOW), &[prompt("s2", NOW + 30, CWD, "second")]);
    writer.ingest(&home.ctx(), NOW + 100).unwrap();
    assert_eq!(excerpt_holders(&home), ["s2", "s3", "s4"], "s1 is older");
    writer.file_sessions(&ids(&["s3"]), id, NOW + 102).unwrap();
    assert_eq!(excerpt_holders(&home), ["s2", "s4"], "filed into it");
    writer.merge_subjects(other, id, NOW + 103).unwrap();
    assert_eq!(excerpt_holders(&home), ["s2"], "merged into it");
}

/// FR-4, AD-44: archiving clears the excerpts of the subject's sessions
/// from the database and its WAL.
#[test]
fn archive_clears_excerpts_from_the_file_and_the_wal() {
    let marker = "MARKER-ARCHIVED-PROMPT-7d1f";
    let (home, mut writer) = ingested(&[
        prompt("s1", NOW - 9, CWD, marker),
        prompt("s2", NOW - 8, CWD, "kept: another subject"),
    ]);
    let id = writer
        .create_subject("rollout", SubjectOrigin::Sessions(ids(&["s1"])), NOW)
        .unwrap();
    let db = home.paths().database();
    let wal = home.paths().wal();
    assert!(
        file_contains(&db, marker) || file_contains(&wal, marker),
        "positive control"
    );
    writer.archive_subject(id, true, NOW + 1).unwrap();
    assert!(!file_contains(&db, marker), "in the database");
    assert!(!file_contains(&wal, marker), "in the WAL");
    assert_eq!(excerpt_holders(&home), ["s2"]);
    assert!(subject(&home, id).routine);
}

/// D-58, D-77: a session alive again wakes its subject, source `rules`; an
/// archived one gets its latest confirmation stamped reopened.
#[test]
fn alive_again_reactivates_and_stamps_the_reopen() {
    let (home, mut writer) = ingested(&[
        prompt("s1", NOW - 9, CWD, "a"),
        line("SessionEnd", "s1", NOW - 8, CWD),
        prompt("s2", NOW - 7, CWD, "b"),
        line("SessionEnd", "s2", NOW - 6, CWD),
    ]);
    let archived = writer
        .create_subject("archived", SubjectOrigin::Sessions(ids(&["s1"])), NOW)
        .unwrap();
    let to_resume = writer
        .create_subject("to resume", SubjectOrigin::Sessions(ids(&["s2"])), NOW)
        .unwrap();
    confirmed(&home, archived, NOW + 1);
    confirmed(&home, archived, NOW + 2);
    writer.archive_subject(archived, false, NOW + 3).unwrap();
    writer.archive_subject(to_resume, false, NOW + 3).unwrap();
    writer.reopen_subject(to_resume, NOW + 4).unwrap();
    assert!(reopened(&home).is_empty(), "no confirmation of to_resume");

    home.append(
        &day_file(NOW),
        &[prompt("s1", NOW + 10, CWD, "one more thing")],
    );
    writer.ingest(&home.ctx(), NOW + 20).unwrap();
    let woken = subject(&home, archived).status;
    assert_eq!(
        (woken.value, woken.src, woken.at_ms),
        (SubjectStatus::Active, Source::Rules, NOW + 10)
    );
    assert_eq!(
        reopened(&home),
        [(archived, NOW + 2, NOW + 10)],
        "the latest only"
    );

    let mut snapshot = None;
    listed(&mut writer, &mut snapshot, &["s2"], NOW + 30);
    let woken = subject(&home, to_resume).status;
    assert_eq!(
        (woken.value, woken.src, woken.at_ms),
        (SubjectStatus::Active, Source::Rules, NOW + 30)
    );
}

/// The default proposal: `to_resume`, by rule.
fn default_to_resume() -> Proposal {
    Proposal {
        verdict: ModelVerdict::ToResume,
        source: ProposalSource::Rules,
        reason: ReasonCode::DefaultToResume,
        call_id: None,
    }
}

fn confirm(key: &str, shown: Proposal, verdict: Verdict) -> ConfirmItem {
    ConfirmItem {
        key: key.to_owned(),
        shown,
        verdict,
        routine: false,
        file_under: None,
    }
}

/// A session of `CWD` prompted with `text`, its turn closed, then ended.
fn finished_session(session: &str, at: i64, text: &str) -> [String; 3] {
    [
        prompt(session, at, CWD, text),
        answer(session, at + 1, CWD, "Done.", None),
        line("SessionEnd", session, at + 2, CWD),
    ]
}

/// How many `confirmation` rows another connection sees committed.
fn committed_confirmations(home: &Home) -> i64 {
    rusqlite::Connection::open_with_flags(
        home.paths().database(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap()
    .query_row("SELECT count(*) FROM confirmation", [], |row| row.get(0))
    .unwrap()
}

/// AD-49, AD-39: a batch is one transaction, answered per item: while the
/// writer is between two items, another connection sees nothing of the
/// first; once it commits, each item has its own outcome, a refused one
/// wrote nothing and a key given twice is confirmed once.
#[test]
fn batch_is_one_transaction_answered_per_item() {
    let lines: Vec<String> = [
        finished_session("s1", NOW - 90, "first"),
        finished_session("s2", NOW - 80, "second"),
    ]
    .concat();
    let (home, mut writer) = ingested(&lines);
    writer
        .create_subject("first", SubjectOrigin::Sessions(ids(&["s1"])), NOW)
        .unwrap();
    writer
        .create_subject("second", SubjectOrigin::Sessions(ids(&["s2"])), NOW)
        .unwrap();
    assert_eq!(writer.pragma("synchronous").as_deref(), Some("2"), "FULL");
    let moved = Proposal {
        verdict: ModelVerdict::Finished,
        ..default_to_resume()
    };
    let items = [
        confirm("subject:1", default_to_resume(), Verdict::Finished),
        confirm("subject:2", moved, Verdict::Finished),
        confirm("subject:1", default_to_resume(), Verdict::Finished),
        confirm("subject:2", default_to_resume(), Verdict::ToResume),
    ];
    let mut seen_between = Vec::new();
    let answers = writer
        .confirm_triage_observed(&items, NOW + 1, &mut |index| {
            seen_between.push((index, committed_confirmations(&home)));
        })
        .unwrap();
    assert_eq!(
        seen_between,
        [(0, 0), (1, 0), (2, 0), (3, 0)],
        "nothing is committed before the batch ends"
    );
    assert_eq!(
        answers
            .iter()
            .map(|a| a.outcome.clone())
            .collect::<Vec<_>>(),
        [
            ItemOutcome::Confirmed {
                subject: 1,
                verdict: Verdict::Finished
            },
            ItemOutcome::Refused {
                refusal: ConfirmRefusal::ProposalChanged
            },
            ItemOutcome::Refused {
                refusal: ConfirmRefusal::RowGone
            },
            ItemOutcome::Confirmed {
                subject: 2,
                verdict: Verdict::ToResume
            },
        ]
    );
    assert_eq!(answers[1].key, "subject:2");
    assert_eq!(committed_confirmations(&home), 2);
    let statuses: Vec<SubjectStatus> = subjects(&home).iter().map(|s| s.status.value).collect();
    assert_eq!(statuses, [SubjectStatus::Archived, SubjectStatus::ToResume]);
}

/// AD-49, D-85, FR-4: on `finished`, the session's excerpt row is gone and
/// the only copy of its text is the confirmation's input, expiring when the
/// excerpt was due to; once it expires and the pass truncates the WAL, the
/// text is in neither the database nor its WAL.
#[test]
fn finished_leaves_one_expiring_copy_then_none() {
    let marker = "MARKER-CONFIRMED-PROMPT-3c9a";
    let prompted_at = NOW - 90;
    let (home, mut writer) = ingested(&finished_session("s1", prompted_at, marker));
    writer
        .create_subject("rollout", SubjectOrigin::Sessions(ids(&["s1"])), NOW)
        .unwrap();
    let db = home.paths().database();
    let wal = home.paths().wal();
    let answers = writer
        .confirm_triage(
            &[confirm("subject:1", default_to_resume(), Verdict::Finished)],
            NOW + 1,
        )
        .unwrap();
    assert!(matches!(answers[0].outcome, ItemOutcome::Confirmed { .. }));
    assert!(excerpt_holders(&home).is_empty(), "the excerpt row is gone");
    let kept = confirmations(&home);
    let input = kept[0].input.as_ref().expect("the confirmation's copy");
    assert_eq!(input.prompt.as_deref(), Some(marker));
    assert_eq!(input.expires_at_ms, prompted_at + EXCERPT_TTL_MS);
    assert!(
        file_contains(&db, marker) || file_contains(&wal, marker),
        "the one copy is still held"
    );

    let mut pass_at = |now: i64| writer.hourly_pass(&home.ctx(), now).unwrap();
    pass_at(input.expires_at_ms - 1);
    assert_eq!(
        confirmations(&home)[0]
            .input
            .as_ref()
            .map(|i| i.prompt.clone()),
        Some(Some(marker.to_owned())),
        "kept until it expires"
    );
    let report = pass_at(input.expires_at_ms);
    assert!(report.checkpointed);
    assert_eq!(confirmations(&home)[0].input, None, "the copy expired");
    assert_eq!(confirmations(&home).len(), 1, "the confirmation stays");
    assert!(!file_contains(&db, marker), "in the database");
    assert!(!file_contains(&wal, marker), "in the WAL");
}

/// P-11, D-63, D-88: a confirmation copies the proposal it answered, its
/// verdict, source, reason code and call id, never prose, beside the
/// verdict the author chose, and its input keeps the facts the decision
/// rested on.
#[test]
fn confirmation_copies_the_proposal_it_answered() {
    let (home, mut writer) = ingested(&[
        prompt("b1", NOW - 90, CWD, "write the weekly report"),
        answer("b1", NOW - 80, CWD, "Report written.", None),
    ]);
    let records = parse_agents(
        &serde_json::json!([{"pid": 4300, "id": "bg01", "sessionId": "b1", "kind": "background",
            "cwd": CWD, "name": "report", "status": "idle", "state": "done", "startedAt": NOW - DAY}])
        .to_string(),
    )
    .unwrap();
    writer
        .apply_agents(&mut None, &records, &[], NOW - 70)
        .unwrap();
    writer
        .create_subject("report", SubjectOrigin::Sessions(ids(&["b1"])), NOW)
        .unwrap();
    let state_done = Proposal {
        verdict: ModelVerdict::Finished,
        source: ProposalSource::State,
        reason: ReasonCode::StateDone,
        call_id: None,
    };
    let row = writer
        .view(None, &home.events, NOW + 1)
        .unwrap()
        .triage
        .remove(0);
    assert_eq!((row.key.as_str(), row.proposal), ("subject:1", state_done));
    writer
        .confirm_triage(
            &[confirm("subject:1", row.proposal, Verdict::ToResume)],
            NOW + 2,
        )
        .unwrap();
    assert_eq!(
        confirmations(&home),
        [ExpectedConfirmation {
            subject: 1,
            session: Some("b1".into()),
            proposed: ("finished".into(), "state".into(), "state_done".into()),
            call_id: None,
            verdict: "to_resume".into(),
            input: Some(ConfirmationInput {
                prompt: Some("write the weekly report".into()),
                answer_head: Some("Report written.".into()),
                answer_tail: None,
                end_reason: Some("done".into()),
                cut_mid_turn: false,
                expires_at_ms: NOW - 90 + EXCERPT_TTL_MS,
            }),
        }]
    );
    let conn = rusqlite::Connection::open(home.paths().database()).unwrap();
    let (auto, at): (i64, i64) = conn
        .query_row(
            "SELECT auto, confirmed_at_ms FROM confirmation",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((auto, at), (0, NOW + 2), "the author's, at its time");
    assert_eq!(
        subject(&home, 1).status.value,
        SubjectStatus::ToResume,
        "the verdict chosen, not the one proposed"
    );
}
