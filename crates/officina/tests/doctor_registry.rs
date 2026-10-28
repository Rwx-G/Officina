//! `officina doctor` on the registry (story 3.1 AC 13, story 3.12 AC 1;
//! FR-1 as amended, AD-53, D-47, D-82, D-91): the network folder fails end
//! to end, an absent registry is reported and not created, the database
//! file is left as it was, the registry section reads in one stable order,
//! its warnings alone exit 0 and a failed integrity check exits 1.

mod common;

use common::{TestEnv, fingerprint};
use officina_core::registry::types::{SettingKey, Source};
use officina_host::doctor::REGISTRY_SECTION;
use officina_host::registry::RegistryPaths;
use officina_host::registry::store::{SYNC_ROOT_MARKERS, open_writer};

fn doctor_text(env: &TestEnv, home: Option<&str>) -> String {
    env.write_agents("[]");
    let mut cmd = env.officina(&["doctor"]);
    // Off Windows a UNC home is a relative path, which doctor's home check
    // would create in its working folder: that folder is the test's.
    cmd.current_dir(env.root.path());
    if let Some(home) = home {
        cmd.env("OFFICINA_HOME", home);
    }
    let out = common::run_with(cmd, b"");
    assert!(out.status.code().is_some_and(|code| code != 2));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// D-91: the classification calls a UNC path network without asking the
/// system, so no remote host is waited on; the other checks may fail on it,
/// only the registry line is asserted.
#[test]
fn unc_officina_home_fails_the_registry_location() {
    let env = TestEnv::new();
    let text = doctor_text(&env, Some(r"\\localhost\officina-missing-share\home"));
    let line = text
        .lines()
        .find(|line| line.contains("registry location"))
        .unwrap_or_else(|| panic!("{text}"));
    assert!(line.starts_with("[FAIL] registry location"), "{line}");
    assert!(line.contains("network folder"), "{line}");
}

#[test]
fn absent_registry_is_reported_not_created() {
    let env = TestEnv::new();
    let text = doctor_text(&env, None);
    assert!(text.contains("[ok  ] registry: absent"), "{text}");
    assert!(
        !RegistryPaths::new(&env.officina_home()).dir().exists(),
        "doctor created the registry"
    );
}

/// D-82: a reader may create `-wal` and `-shm`; everything else under
/// `registry/` is compared byte for byte.
#[test]
fn doctor_leaves_the_database_file_unchanged() {
    let env = TestEnv::new();
    let now = common::now_ms();
    let mut writer = open_writer(&env.officina_home(), &[], now)
        .writer()
        .unwrap();
    writer
        .set_setting(SettingKey::AutoArchive, "false", Source::Author, now)
        .unwrap();
    writer.copy_if_due(now).unwrap();
    drop(writer);
    let registry = RegistryPaths::new(&env.officina_home());
    let files = || {
        fingerprint(registry.dir())
            .into_iter()
            .filter(|(name, _)| !name.ends_with("-wal") && !name.ends_with("-shm"))
            .collect::<Vec<_>>()
    };
    let before = files();
    assert!(before.iter().any(|(name, _)| name == "registry.db"));
    let text = doctor_text(&env, None);
    assert!(text.contains("[ok  ] registry: schema version"), "{text}");
    assert!(text.contains("[ok  ] registry integrity"), "{text}");
    assert!(
        text.contains("[ok  ] registry copies: 1 daily copies"),
        "{text}"
    );
    assert_eq!(files(), before);
}

/// The hook's own registrations merged into the user settings, a hook line
/// seen and the quota written: doctor's checks outside the registry pass,
/// so its exit code says what the registry section says.
fn wire_everything_else(env: &TestEnv) {
    let first = env.run(&["doctor"], b"");
    let block = common::printed_hooks_block(&first.stdout);
    std::fs::write(env.claude_config().join("settings.json"), block.to_string()).unwrap();
    env.run(
        &["hook"],
        br#"{"hook_event_name":"SessionStart","session_id":"s"}"#,
    );
    env.run(&["quota", "write"], STATUSLINE.as_bytes());
}

const STATUSLINE: &str = r#"{"session_id":"s1","model":{"display_name":"Opus"},
  "rate_limits":{"five_hour":{"used_percentage":62.0,"resets_at":1800000000},
                 "seven_day":{"used_percentage":41.0,"resets_at":1800500000}}}"#;

/// A home whose registry section warns on every check that can warn at
/// once: a synchronised folder (a planted marker), a quarantine, a recent
/// restore, waiting and rewritten event files, a hook field the running
/// version leaves out, and a paused model.
fn warned_home(env: &TestEnv) {
    use officina_core::contract::HOOK_FIELDS;
    use officina_core::registry::types::RestoreNotice;
    use officina_host::collector::{ContractStore, record_hook_fields};
    use officina_host::registry::ingest::IngestContext;

    env.write_agents(
        r#"[{"pid":1,"cwd":"C:/work/api","kind":"interactive","startedAt":1,"sessionId":"s","status":"busy"}]"#,
    );
    std::fs::write(env.root.path().join(SYNC_ROOT_MARKERS[0]), b"").unwrap();
    wire_everything_else(env);
    assert!(env.run(&["board", "--json"], b"").status.success());
    let contract = env.officina_home().join("state").join("contract.json");
    let version = ContractStore::read(&contract)
        .versions
        .keys()
        .next()
        .cloned()
        .expect("board --json records the running version");
    let (event, fields) = HOOK_FIELDS
        .iter()
        .find(|(event, _)| *event == "SessionEnd")
        .unwrap();
    let mut payload = serde_json::json!({"hook_event_name": event});
    for field in fields.iter().skip(1) {
        payload[*field] = serde_json::json!("redacted");
    }
    assert!(
        env.run(&["hook"], payload.to_string().as_bytes())
            .status
            .success()
    );
    let (events, home) = (env.events_dir(), env.officina_home());
    let ctx = IngestContext {
        events_dir: &events,
        officina_home: &home,
        projects: &[],
        no_excerpts: false,
    };
    let now = common::now_ms();
    let mut writer = open_writer(&home, &[], now).writer().unwrap();
    let report = writer.ingest(&ctx, now).unwrap();
    record_hook_fields(&contract, &version, &report.hook_sightings).unwrap();
    // Today's file found shorter than what was ingested: read again (D-84).
    let today = events.join(officina_core::dates::event_file_name(now));
    std::fs::write(&today, b"").unwrap();
    writer.ingest(&ctx, now + 1).unwrap();
    for day in 1..=8 {
        std::fs::write(events.join(format!("2026-01-0{day}.jsonl")), b"").unwrap();
    }
    writer.copy_if_due(now).unwrap();
    let notice = RestoreNotice {
        copy: "registry-2026-01-01.db".into(),
        restored_at_ms: now,
    };
    writer
        .set_setting(
            SettingKey::RestoreNotice,
            &serde_json::to_string(&notice).unwrap(),
            Source::Rules,
            now,
        )
        .unwrap();
    writer
        .set_setting(
            SettingKey::TriageModel,
            r#"{"on":true}"#,
            Source::Author,
            now,
        )
        .unwrap();
    writer
        .set_setting(
            SettingKey::TriageModelPause,
            r#"{"paused":true}"#,
            Source::Rules,
            now,
        )
        .unwrap();
    drop(writer);
    let quarantine = RegistryPaths::new(&home).quarantine_dir().join("1");
    std::fs::create_dir_all(&quarantine).unwrap();
    std::fs::write(quarantine.join("registry.db"), b"set aside").unwrap();
}

/// Doctor's run of `env`: its exit code and its lines.
fn doctor(env: &TestEnv) -> (Option<i32>, Vec<String>) {
    let out = env.run(&["doctor"], b"");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    (out.status.code(), text.lines().map(str::to_owned).collect())
}

/// The title of a doctor line, `[level] title: detail`.
fn title_of(line: &str) -> Option<&str> {
    let rest = line.split_once("] ")?.1;
    Some(rest.split_once(": ")?.0)
}

/// Story 3.12 AC 1 (AD-53): the registry's checks, the canary's hook fields
/// and the triage model's form one section, in `REGISTRY_SECTION`'s order,
/// each once, with no other line among them.
#[test]
fn registry_section_lists_each_check_once() {
    let env = TestEnv::new();
    warned_home(&env);
    let (_, lines) = doctor(&env);
    let places: Vec<(usize, usize)> = lines
        .iter()
        .enumerate()
        .filter_map(|(at, line)| {
            let title = title_of(line)?;
            Some((at, REGISTRY_SECTION.iter().position(|t| *t == title)?))
        })
        .collect();
    let titles: Vec<&str> = places
        .iter()
        .map(|(_, place)| REGISTRY_SECTION[*place])
        .collect();
    // Every line of the section shows here but the shim's, which a native
    // `claude` never gets.
    for expected in REGISTRY_SECTION.iter().filter(|t| **t != "triage call") {
        assert_eq!(
            titles.iter().filter(|t| *t == expected).count(),
            1,
            "{expected} once: {lines:#?}"
        );
    }
    assert!(
        places.windows(2).all(|pair| pair[0].1 < pair[1].1),
        "in the section's order: {titles:?}"
    );
    let (first, last) = (places[0].0, places[places.len() - 1].0);
    assert_eq!(last - first + 1, places.len(), "contiguous: {lines:#?}");
}

/// Story 3.12 AC 1 (v0.1's exit rule): every registry warning at once, and
/// a registry written by a newer Officina, still exit 0.
#[test]
fn warnings_alone_exit_0() {
    let env = TestEnv::new();
    warned_home(&env);
    let (code, lines) = doctor(&env);
    let warned: Vec<&str> = lines
        .iter()
        .filter(|line| line.starts_with("[warn]"))
        .filter_map(|line| title_of(line))
        .collect();
    // Every line of the section warns but those that read the registry
    // sound, and the shim's, absent here.
    let sound = [
        "registry",
        "registry integrity",
        "registry copies",
        "triage call",
    ];
    for expected in REGISTRY_SECTION.iter().filter(|t| !sound.contains(t)) {
        assert!(warned.contains(expected), "{expected} warns: {lines:#?}");
    }
    assert!(
        lines.iter().all(|line| !line.starts_with("[FAIL]")),
        "{lines:#?}"
    );
    assert_eq!(code, Some(0), "{lines:#?}");

    // Written by a newer Officina: read-only for the window, a warning.
    let db = RegistryPaths::new(&env.officina_home()).database();
    let mut bytes = std::fs::read(&db).unwrap();
    // `user_version` is the header's big-endian integer at offset 60.
    bytes[60..64].copy_from_slice(&99u32.to_be_bytes());
    std::fs::write(&db, bytes).unwrap();
    let (code, lines) = doctor(&env);
    let line = lines
        .iter()
        .find(|line| title_of(line) == Some("registry"))
        .unwrap();
    assert!(
        line.starts_with("[warn] registry: written by a newer"),
        "{line}"
    );
    assert_eq!(code, Some(0), "{lines:#?}");
}

/// Story 3.12 AC 1 (AD-42): a damaged database fails its integrity check,
/// and doctor exits 1.
#[test]
fn a_failed_quick_check_exits_1() {
    let env = TestEnv::new();
    env.write_agents("[]");
    wire_everything_else(&env);
    let now = common::now_ms();
    let mut writer = open_writer(&env.officina_home(), &[], now)
        .writer()
        .unwrap();
    for key in SettingKey::ALL {
        writer
            .set_setting(*key, "\"filler\"", Source::Author, now)
            .unwrap();
    }
    assert!(writer.checkpoint().unwrap());
    drop(writer);
    let db = RegistryPaths::new(&env.officina_home()).database();
    let mut bytes = std::fs::read(&db).unwrap();
    let page = usize::from(u16::from_be_bytes([bytes[16], bytes[17]]));
    assert!(bytes.len() >= 2 * page, "page 2 exists");
    bytes[page..2 * page].fill(0xFF);
    std::fs::write(&db, bytes).unwrap();
    let (code, lines) = doctor(&env);
    let line = lines
        .iter()
        .find(|line| title_of(line) == Some("registry integrity"))
        .unwrap_or_else(|| panic!("{lines:#?}"));
    assert!(line.starts_with("[FAIL] registry integrity: "), "{line}");
    assert_eq!(code, Some(1));
}
