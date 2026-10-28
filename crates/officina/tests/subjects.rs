//! `officina subjects --json` end to end (story 3.4 AC 8, story 3.6 AC 5
//! and 6; FR-6 and FR-7 as amended, AD-53, D-47, D-82, D-97): the Subjects
//! view from the read-only registry and a fresh board, with the window
//! closed, the rows that need a verdict included; exit 0 when the registry
//! was read, 3 with the reason when it was not, 64 on usage, never 2.

mod common;

use common::{TestEnv, fingerprint, now_ms};
use officina_core::registry::confirm::SubjectOrigin;
use officina_core::registry::types::RegistryState;
use officina_core::registry::view::REGISTRY_VIEW_SCHEMA;
use officina_host::registry::RegistryPaths;
use officina_host::registry::ingest::IngestContext;
use officina_host::registry::schema::MIGRATIONS;
use officina_host::registry::store::{Writer, open_writer, open_writer_with};

fn hook(env: &TestEnv, payload: serde_json::Value) {
    let out = env.run(&["hook"], payload.to_string().as_bytes());
    assert_eq!(out.status.code(), Some(0));
}

/// The window's writer on `env`'s home, after ingesting its event files.
fn window(env: &TestEnv, now: i64) -> Writer {
    let mut writer = open_writer(&env.officina_home(), &[], now)
        .writer()
        .unwrap();
    let events = env.events_dir();
    let home = env.officina_home();
    writer
        .ingest(
            &IngestContext {
                events_dir: &events,
                officina_home: &home,
                projects: &[],
                no_excerpts: false,
            },
            now,
        )
        .unwrap();
    writer
}

fn subjects(env: &TestEnv) -> (Option<i32>, serde_json::Value) {
    let out = env.run(&["subjects", "--json"], b"");
    let json = serde_json::from_slice(&out.stdout).unwrap_or(serde_json::Value::Null);
    (out.status.code(), json)
}

/// Two sessions of `api`, one filed under "alert rules" by the window,
/// which then closed; `claude agents` lists the filed one.
fn closed_window_with_a_subject(env: &TestEnv) -> i64 {
    let now = now_ms();
    for (session, prompt) in [
        ("s-filed", "tighten the alert rules"),
        ("s-loose", "look around"),
    ] {
        hook(
            env,
            serde_json::json!({"hook_event_name": "UserPromptSubmit", "session_id": session,
                "cwd": "C:/work/api", "prompt": prompt}),
        );
    }
    env.write_agents(
        &serde_json::json!([{"pid": 4100, "cwd": "C:/work/api", "kind": "interactive",
            "startedAt": now - 60_000, "sessionId": "s-filed", "name": "api", "status": "busy"}])
        .to_string(),
    );
    let mut writer = window(env, now);
    let id = writer
        .create_subject(
            "alert rules",
            SubjectOrigin::Sessions(vec!["s-filed".into()]),
            now,
        )
        .unwrap();
    drop(writer);
    id
}

/// AD-53, D-82: the view prints from the read-only registry with the window
/// closed, its sessions joined to the fresh board's live cards.
#[test]
fn prints_the_registry_view_with_the_window_closed() {
    let env = TestEnv::new();
    let id = closed_window_with_a_subject(&env);
    let (code, view) = subjects(&env);
    assert_eq!(code, Some(0), "{view}");
    assert_eq!(view["schema"], REGISTRY_VIEW_SCHEMA);
    assert_eq!(view["registry"]["state"], RegistryState::Ready.code());
    let subject = &view["subjects"][0];
    assert_eq!(
        (subject["id"].as_i64(), subject["name"].as_str()),
        (Some(id), Some("alert rules"))
    );
    assert_eq!(subject["sessions"][0]["session_id"], "s-filed");
    assert_eq!(subject["sessions"][0]["card"]["column"], "working");
    assert_eq!(subject["excerpts"]["prompt"], "tighten the alert rules");
    assert_eq!(view["unfiled"][0]["project"], "api");
    assert_eq!(view["unfiled"][0]["sessions"][0]["session_id"], "s-loose");
}

/// AD-53: with the window closed, the event lines it has not ingested show
/// as `ingestion.pending_bytes`, from the files' sizes against the offsets.
#[test]
fn pending_bytes_show_the_lag_behind_a_closed_window() {
    let env = TestEnv::new();
    closed_window_with_a_subject(&env);
    let (_, view) = subjects(&env);
    assert_eq!(view["ingestion"]["pending_bytes"], 0);
    let today = env
        .events_dir()
        .join(officina_core::dates::event_file_name(now_ms()));
    let before = std::fs::metadata(&today).unwrap().len();
    hook(
        &env,
        serde_json::json!({"hook_event_name": "Stop", "session_id": "s-loose", "cwd": "C:/work/api"}),
    );
    let grown = std::fs::metadata(&today).unwrap().len() - before;
    assert!(grown > 0);
    let (code, view) = subjects(&env);
    assert_eq!(code, Some(0));
    assert_eq!(view["ingestion"]["pending_bytes"], grown);
    assert_eq!(view["ingestion"]["incomplete_files"], 1);
}

/// D-97: a stale board inside the view does not make the registry unread.
#[test]
fn a_stale_board_still_exits_0() {
    let env = TestEnv::new();
    closed_window_with_a_subject(&env);
    let mut cmd = env.officina(&["subjects", "--json"]);
    cmd.env("FAKE_CLAUDE_MODE", "exit1");
    let out = common::run_with(cmd, b"");
    assert_eq!(out.status.code(), Some(0));
    let view: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(view["subjects"][0]["name"], "alert rules");
    assert!(
        view["subjects"][0]["sessions"][0]["card"].is_null(),
        "no live card"
    );
}

/// D-47: every reader state in which nothing is read exits 3 and prints
/// the view with the reason; the readable one exits 0. The `match` names
/// every state, so a new one fails to compile here until it has a case.
#[test]
fn every_unreadable_state_exits_3_with_its_reason() {
    for state in RegistryState::ALL {
        let env = TestEnv::new();
        env.write_agents("[]");
        let paths = RegistryPaths::new(&env.officina_home());
        // A window of another build writes the version its migrations
        // reach.
        let written_with = |migrations: &[&str]| {
            drop(open_writer_with(&env.officina_home(), &[], migrations, now_ms()).writer());
        };
        let mut cmd = env.officina(&["subjects", "--json"]);
        match state {
            RegistryState::Ready => drop(window(&env, now_ms())),
            RegistryState::Absent => {}
            RegistryState::Older => match MIGRATIONS.split_last() {
                Some((_, older)) if !older.is_empty() => written_with(older),
                // With one schema version, no older one exists to write:
                // `registry_store.rs::reader_reports_each_state` covers the
                // state in the host.
                _ => continue,
            },
            RegistryState::Newer => {
                let mut newer = MIGRATIONS.to_vec();
                newer.push("CREATE TABLE written_later (x INTEGER) STRICT;");
                written_with(&newer);
            }
            RegistryState::Unavailable => {
                std::fs::create_dir_all(paths.dir()).unwrap();
                std::fs::write(paths.database(), "not a database, ".repeat(300)).unwrap();
            }
            RegistryState::Network => {
                // Off Windows a UNC home is a relative path: keep it in the
                // test's folder.
                cmd.current_dir(env.root.path());
                cmd.env("OFFICINA_HOME", r"\\localhost\officina-missing-share\home");
            }
        }
        let out = common::run_with(cmd, b"");
        let view: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(view["registry"]["state"], state.code(), "{state:?}");
        let expected = if *state == RegistryState::Ready { 0 } else { 3 };
        assert_eq!(out.status.code(), Some(expected), "{state:?}");
        if *state != RegistryState::Ready {
            assert_eq!(view["subjects"], serde_json::json!([]), "{state:?}");
        }
        if *state == RegistryState::Unavailable {
            assert!(view["registry"]["detail"].as_str().is_some());
        }
    }
}

#[test]
fn usage_error_exits_64() {
    let env = TestEnv::new();
    for args in [&["subjects"][..], &["subjects", "--json", "--floor"]] {
        let out = env.run(args, b"");
        assert_eq!(out.status.code(), Some(64), "{args:?}");
    }
}

/// AD-53, D-82: printing the view changes no row; with the window closed
/// a reader may create `-wal` and `-shm`, so the database file is compared.
#[test]
fn subjects_json_changes_no_row() {
    let env = TestEnv::new();
    closed_window_with_a_subject(&env);
    let registry = RegistryPaths::new(&env.officina_home());
    let database = |entries: Vec<(String, Vec<u8>)>| {
        entries
            .into_iter()
            .filter(|(name, _)| !name.ends_with("-wal") && !name.ends_with("-shm"))
            .collect::<Vec<_>>()
    };
    let before = database(fingerprint(registry.dir()));
    let (code, _) = subjects(&env);
    assert_eq!(code, Some(0));
    assert_eq!(database(fingerprint(registry.dir())), before);
}

/// Appends event lines at their own times, as the hook writes them, so a
/// test sets when a session prompted, stopped and ended.
fn events(env: &TestEnv, lines: &[serde_json::Value]) {
    use std::io::Write;
    std::fs::create_dir_all(env.events_dir()).unwrap();
    for line in lines {
        let ts = line["ts"].as_i64().unwrap();
        let file = env
            .events_dir()
            .join(officina_core::dates::event_file_name(ts));
        let mut out = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(file)
            .unwrap();
        writeln!(out, "{line}").unwrap();
    }
}

/// One event line of `session` in `C:/work/api`, `at` ms after `base`.
fn line(
    base: i64,
    at: i64,
    event: &str,
    session: &str,
    extra: serde_json::Value,
) -> serde_json::Value {
    let mut line = serde_json::json!({"v": 1, "ts": base + at, "event": event,
        "session_id": session, "cwd": "C:/work/api", "claude_pid": 4100});
    for (key, value) in extra.as_object().unwrap() {
        line[key] = value.clone();
    }
    line
}

/// A session that prompted, answered and exited normally.
fn finished_turn(base: i64, at: i64, session: &str) -> Vec<serde_json::Value> {
    vec![
        line(
            base,
            at,
            "UserPromptSubmit",
            session,
            serde_json::json!({"prompt": "go"}),
        ),
        line(
            base,
            at + 1_000,
            "Stop",
            session,
            serde_json::json!({"last_message": "Done."}),
        ),
        line(
            base,
            at + 2_000,
            "SessionEnd",
            session,
            serde_json::json!({"reason": "prompt_input_exit"}),
        ),
    ]
}

fn rows(view: &serde_json::Value) -> &Vec<serde_json::Value> {
    view["triage"].as_array().unwrap()
}

/// Story 3.6 AC 5: `subjects --json` prints every row with its proposal,
/// the proposal's source and reason code, and their count.
#[test]
fn rows_carry_proposal_source_and_reason() {
    let env = TestEnv::new();
    let base = now_ms() - 3_600_000;
    let mut lines = vec![
        line(
            base,
            0,
            "UserPromptSubmit",
            "s-cut",
            serde_json::json!({"prompt": "migrate"}),
        ),
        line(
            base,
            1_000,
            "SessionEnd",
            "s-cut",
            serde_json::json!({"reason": "prompt_input_exit"}),
        ),
        line(
            base,
            5_000,
            "UserPromptSubmit",
            "s-bg",
            serde_json::json!({"prompt": "nightly report"}),
        ),
    ];
    lines.extend(finished_turn(base, 10_000, "s-loose"));
    events(&env, &lines);
    let now = now_ms();
    let mut writer = window(&env, now);
    let background = officina_core::agents::parse_agents(
        &serde_json::json!([{"pid": 4300, "id": "bg01", "sessionId": "s-bg", "kind": "background",
            "cwd": "C:/work/api", "name": "nightly", "status": "idle", "state": "done",
            "startedAt": base}])
        .to_string(),
    )
    .unwrap();
    writer
        .apply_agents(&mut None, &background, &[], now)
        .unwrap();
    let migration = writer
        .create_subject(
            "migration",
            SubjectOrigin::Sessions(vec!["s-cut".into()]),
            now,
        )
        .unwrap();
    let nightly = writer
        .create_subject("nightly", SubjectOrigin::Sessions(vec!["s-bg".into()]), now)
        .unwrap();
    let empty = writer
        .create_subject("empty", SubjectOrigin::Project("api".into()), now)
        .unwrap();
    drop(writer);
    env.write_agents("[]");

    let (code, view) = subjects(&env);
    assert_eq!(code, Some(0), "{view}");
    let found: std::collections::BTreeMap<String, (String, String, String, String)> = rows(&view)
        .iter()
        .map(|row| {
            let text = |v: &serde_json::Value| v.as_str().unwrap_or_default().to_owned();
            (
                text(&row["key"]),
                (
                    text(&row["kind"]),
                    text(&row["proposal"]["verdict"]),
                    text(&row["proposal"]["source"]),
                    text(&row["proposal"]["reason"]),
                ),
            )
        })
        .collect();
    let expect = |kind: &str, verdict: &str, source: &str, reason: &str| {
        (
            kind.to_owned(),
            verdict.to_owned(),
            source.to_owned(),
            reason.to_owned(),
        )
    };
    assert_eq!(
        found,
        [
            (
                format!("subject:{migration}"),
                expect("subject", "to_resume", "rules", "cut_mid_turn")
            ),
            (
                format!("subject:{nightly}"),
                expect("subject", "finished", "state", "state_done")
            ),
            (
                format!("subject:{empty}"),
                expect("empty_subject", "finished", "rules", "empty_subject")
            ),
            (
                "session:s-loose".to_owned(),
                expect("unfiled_session", "to_resume", "rules", "default_to_resume")
            ),
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(view["to_sort"], rows(&view).len());
    let loose = rows(&view)
        .iter()
        .find(|r| r["key"] == "session:s-loose")
        .unwrap();
    assert_eq!(
        loose["proposed_subject"],
        serde_json::Value::Null,
        "three open subjects"
    );
    assert_eq!(loose["choices"].as_array().unwrap().len(), 3);
    assert_eq!(loose["excerpts"]["prompt"], "go");
    assert_eq!(loose["verdict"], "to_resume");
}

/// PRD section 7 Attention, story 3.6 AC 6: a subject of several ended
/// sessions, one of them cleared, needs exactly one verdict, on its latest
/// session that saw a prompt.
#[test]
fn multi_session_subject_needs_one_verdict() {
    let env = TestEnv::new();
    let base = now_ms() - 3_600_000;
    let mut lines = finished_turn(base, 0, "s-first");
    lines.extend([
        line(
            base,
            10_000,
            "SessionStart",
            "s-cleared",
            serde_json::json!({"source": "startup"}),
        ),
        line(
            base,
            11_000,
            "UserPromptSubmit",
            "s-cleared",
            serde_json::json!({"prompt": "apply it"}),
        ),
        line(
            base,
            12_000,
            "Stop",
            "s-cleared",
            serde_json::json!({"last_message": "Applied."}),
        ),
        line(
            base,
            13_000,
            "SessionEnd",
            "s-cleared",
            serde_json::json!({"reason": "clear"}),
        ),
        line(
            base,
            13_000,
            "SessionStart",
            "s-after",
            serde_json::json!({"source": "clear"}),
        ),
        line(
            base,
            14_000,
            "SessionEnd",
            "s-after",
            serde_json::json!({"reason": "prompt_input_exit"}),
        ),
    ]);
    events(&env, &lines);
    let now = now_ms();
    let mut writer = window(&env, now);
    let ids = ["s-first", "s-cleared", "s-after"]
        .map(str::to_owned)
        .to_vec();
    let rollout = writer
        .create_subject("rollout", SubjectOrigin::Sessions(ids), now)
        .unwrap();
    drop(writer);
    env.write_agents("[]");

    let (code, view) = subjects(&env);
    assert_eq!(code, Some(0), "{view}");
    let subject = &view["subjects"][0];
    assert_eq!(subject["sessions"].as_array().unwrap().len(), 3);
    assert!(
        subject["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["session_id"] == "s-cleared" && s["superseded"] == true),
        "{subject}"
    );
    assert_eq!(view["to_sort"], 1, "{view}");
    let row = &rows(&view)[0];
    assert_eq!(row["key"], format!("subject:{rollout}"));
    assert_eq!(row["session_id"], "s-cleared");
}

/// Story 3.10 AC 7 (FR-13, P-10, D-55): subjects archived with the routine
/// box are printed as candidates for the v0.7 catalogue, the latest first;
/// a plainly archived one and a merged source are not.
#[test]
fn routine_candidates_are_printed() {
    let env = TestEnv::new();
    let now = now_ms();
    for session in ["s-1", "s-2", "s-3", "s-4"] {
        hook(
            &env,
            serde_json::json!({"hook_event_name": "UserPromptSubmit", "session_id": session,
                "cwd": "C:/work/api", "prompt": "rotate the logs"}),
        );
    }
    env.write_agents("[]");
    let mut writer = window(&env, now);
    let mut create = |name: &str, session: &str| {
        writer
            .create_subject(name, SubjectOrigin::Sessions(vec![session.into()]), now)
            .unwrap()
    };
    let older = create("log rotation", "s-1");
    let plain = create("one-off fix", "s-2");
    let source = create("cert renewal draft", "s-3");
    let target = create("cert renewal", "s-4");
    writer.archive_subject(older, true, now + 1_000).unwrap();
    writer.archive_subject(plain, false, now + 2_000).unwrap();
    writer.merge_subjects(source, target, now + 3_000).unwrap();
    writer.archive_subject(target, true, now + 4_000).unwrap();
    drop(writer);
    let (code, view) = subjects(&env);
    assert_eq!(code, Some(0));
    let listed: Vec<(i64, &str)> = view["routine_candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["subject"]["id"].as_i64().unwrap(),
                c["subject"]["name"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(listed, [(target, "cert renewal"), (older, "log rotation")]);
}
