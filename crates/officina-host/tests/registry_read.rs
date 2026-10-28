//! Reading session records back (story 3.3 AC 1; AD-40): every fact the
//! writer stores is read into its field with its source and time, and the
//! filing column keeps its own.

mod common;

use common::registry::{Home, NOW, day_file, line};
use officina_core::registry::fold::FactWrite;
use officina_core::registry::types::{FactValue, HookEnd, HookStart, SessionFact, Source};
use officina_host::registry::store::open_reader;

/// A plausible value for each fact, so a new fact fails to compile here
/// until it has one.
fn value_for(fact: SessionFact) -> FactValue {
    let json = |v: serde_json::Value| FactValue::Text(v.to_string());
    match fact {
        SessionFact::Kind => FactValue::Text("background".into()),
        SessionFact::Name => FactValue::Text("nightly".into()),
        SessionFact::Cwd => FactValue::Text("C:/work/api".into()),
        SessionFact::ClaudePid => FactValue::Integer(4100),
        SessionFact::BackgroundId => FactValue::Text("bg01".into()),
        SessionFact::Status => FactValue::Text("busy".into()),
        SessionFact::State => FactValue::Text("working".into()),
        SessionFact::Started => json(
            serde_json::to_value(HookStart {
                source: Some("startup".into()),
                claude_pid: Some(4100),
            })
            .unwrap(),
        ),
        SessionFact::Ended => json(
            serde_json::to_value(HookEnd {
                reason: Some("other".into()),
                claude_pid: Some(4100),
            })
            .unwrap(),
        ),
        SessionFact::Listed
        | SessionFact::LastPromptAt
        | SessionFact::LastStopAt
        | SessionFact::Interrupted => FactValue::Integer(NOW - 5),
        SessionFact::Successor => FactValue::Text("s2".into()),
    }
}

#[test]
fn session_record_round_trips_with_each_source() {
    let home = Home::new();
    home.append(
        &day_file(NOW),
        &[line("SessionStart", "s1", NOW - 10, "C:/work/api")],
    );
    let mut writer = home.writer(NOW);
    writer.ingest(&home.ctx(), NOW).unwrap();
    // Every fact, and every source, cycling so each appears at least once.
    let count = SessionFact::ALL.len().max(Source::ALL.len());
    let writes: Vec<FactWrite> = (0..count)
        .map(|i| {
            let fact = SessionFact::ALL[i % SessionFact::ALL.len()];
            FactWrite {
                session_id: "s1".into(),
                fact,
                value: value_for(fact),
                src: Source::ALL[i % Source::ALL.len()],
                at_ms: NOW + i64::try_from(i).unwrap(),
            }
        })
        .collect();
    writer.record_facts(&writes).unwrap();
    drop(writer);
    {
        let conn = rusqlite::Connection::open(home.paths().database()).unwrap();
        conn.execute(
            "INSERT INTO subject (id, project, name, status, status_src, status_at_ms, created_at_ms)
             VALUES (7, 'api', 'rollout', 'active', 'author', ?1, ?1)",
            [NOW],
        )
        .unwrap();
        conn.execute(
            "UPDATE session SET subject_id = 7, subject_src = 'author', subject_at_ms = ?1",
            [NOW + 99],
        )
        .unwrap();
    }

    let reader = open_reader(&home.home, &[]).unwrap();
    let record = reader.session_records().unwrap().pop().unwrap();
    assert_eq!(record.unknown_facts, 0);
    let stored = reader.facts().unwrap();
    for write in &writes {
        let row = stored
            .iter()
            .find(|row| row.fact == write.fact.code())
            .unwrap();
        assert_eq!(
            (&row.value, row.src, row.at_ms),
            (&write.value, write.src, write.at_ms),
            "{:?}",
            write.fact
        );
    }
    let src_of = |fact: SessionFact| {
        let as_json = serde_json::to_value(&record).unwrap();
        let field = &as_json[fact.code()];
        assert!(!field.is_null(), "{fact:?} is not in the record");
        serde_json::from_value::<Source>(field["src"].clone()).unwrap()
    };
    for write in writes.iter().rev().take(SessionFact::ALL.len()) {
        assert_eq!(src_of(write.fact), write.src, "{:?}", write.fact);
    }
    let subject = record.subject.unwrap();
    assert_eq!(
        (subject.value, subject.src, subject.at_ms),
        (Some(7), Source::Author, NOW + 99)
    );
}

/// Adversarial pass (D-84): a file rewritten shorter than its offset while
/// the window was closed is read again from its start, so all of it is
/// pending, not the difference against the old offset.
#[test]
fn pending_bytes_count_a_rewritten_file_whole() {
    use officina_core::registry::types::{RegistryReport, RegistryState};
    let home = Home::new();
    let today = day_file(NOW);
    home.append(
        &today,
        &[
            line("Stop", "s1", NOW - 3, "C:/work/api"),
            line("Stop", "s2", NOW - 2, "C:/work/api"),
        ],
    );
    let mut writer = home.writer(NOW);
    writer.ingest(&home.ctx(), NOW).unwrap();
    drop(writer);
    let rewritten = line("Stop", "s3", NOW - 1, "C:/work/api");
    std::fs::write(home.events.join(&today), &rewritten).unwrap();
    let ready = RegistryReport {
        state: RegistryState::Ready,
        detail: None,
        exclude_from_sync: None,
    };
    let view = open_reader(&home.home, &[])
        .unwrap()
        .view(ready, None, &home.events, NOW)
        .unwrap();
    assert_eq!(view.ingestion.pending_bytes, rewritten.len() as u64);
    assert_eq!(view.ingestion.incomplete_files, 1);
}
