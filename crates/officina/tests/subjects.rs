//! `officina subjects --json` end to end (story 3.4 AC 8; FR-6 as amended,
//! AD-53, D-47, D-82, D-97): the Subjects view from the read-only registry
//! and a fresh board, with the window closed; exit 0 when the registry was
//! read, 3 with the reason when it was not, 64 on usage, never 2.

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
