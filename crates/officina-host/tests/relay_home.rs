//! The relay's files under the Officina home (AD-25), the states of
//! `relay/approved.json` and the relay floor resolution (AD-24). Every test
//! works in its own temp home and sets nothing process-wide.

mod common;

use std::path::{Path, PathBuf};

use common::TestHome;
use officina_core::relay::approvals::{
    FileKind, KEPT_OUTPUT_RETENTION_MS, RETENTION_MS, file_name, kept_output_name,
};
use officina_core::relay::catalogue::{ApprovedCatalogue, load};
use officina_core::relay::journal::{GateError, GateFault};
use officina_core::relay::view::RecentEvent;
use officina_host::files::now_ms;
use officina_host::relay::approvals::prune;
use officina_host::relay::home::{
    ApprovedState, KnownProjectsState, RelayFloorError, RelayHome, load_relay_floor,
};
use officina_host::relay::known_projects::KnownProjectsKeeper;
use officina_host::relay::view::read_view;

const FLOOR: &str = "schema = 2\n\n[[project]]\nname = \"lab\"\npath = \"Lab\"\nactions = [\"probe\"]\n\n[[action]]\nname = \"probe\"\nclass = \"free\"\nprogram = \"C:/path/to/tool.exe\"\n";

fn approved_for(floor_path: &Path) -> ApprovedCatalogue {
    let path = floor_path.to_string_lossy();
    ApprovedCatalogue::approving(&load(FLOOR, &path).unwrap(), &path, 1_793_102_400_000)
}

fn write_approved(home: &RelayHome, content: &[u8]) {
    std::fs::create_dir_all(home.relay_dir()).unwrap();
    std::fs::write(home.approved_file(), content).unwrap();
}

#[test]
fn paths_follow_the_relay_file_table() {
    let home = RelayHome::new("/officina", None);
    let relative = |path: PathBuf| {
        path.strip_prefix("/officina")
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/")
    };
    assert_eq!(relative(home.approved_file()), "relay/approved.json");
    assert_eq!(relative(home.stopped_file()), "relay/stopped");
    assert_eq!(relative(home.approvals_dir()), "relay/approvals");
    assert_eq!(
        relative(home.known_projects_file()),
        "relay/known-projects.json"
    );
    assert_eq!(relative(home.final_roots_file()), "relay/final-roots.json");
    assert_eq!(relative(home.journal_dir()), "journal");
    assert_eq!(relative(home.journal_lock_file()), "journal/.lock");
    assert_eq!(relative(home.default_floor()), "floor.toml");
}

#[test]
fn missing_approved_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let home = RelayHome::new(dir.path(), None);
    assert_eq!(home.read_approved(), ApprovedState::Missing);
}

#[test]
fn corrupt_approved_is_reported_as_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    let home = RelayHome::new(dir.path(), None);
    let valid = serde_json::to_value(approved_for(&dir.path().join("floor.toml"))).unwrap();
    let mut unknown_field = valid.clone();
    unknown_field["extra"] = true.into();
    let mut wrong_type = valid;
    wrong_type["hash"] = 7.into();
    for content in [
        b"{not json".to_vec(),
        b"".to_vec(),
        b"[]".to_vec(),
        serde_json::to_vec(&unknown_field).unwrap(),
        serde_json::to_vec(&wrong_type).unwrap(),
    ] {
        write_approved(&home, &content);
        assert!(
            matches!(home.read_approved(), ApprovedState::Unreadable(ref m) if m.starts_with("corrupt")),
            "{}",
            String::from_utf8_lossy(&content)
        );
    }

    std::fs::remove_file(home.approved_file()).unwrap();
    std::fs::create_dir_all(home.approved_file()).unwrap();
    assert!(
        matches!(home.read_approved(), ApprovedState::Unreadable(_)),
        "a directory in the file's place is unreadable, not missing"
    );
}

#[test]
fn approved_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let home = RelayHome::new(dir.path(), None);
    let approved = approved_for(&dir.path().join("floor.toml"));
    write_approved(&home, &serde_json::to_vec_pretty(&approved).unwrap());
    assert_eq!(home.read_approved(), ApprovedState::Valid(approved.clone()));
    assert_eq!(home.read_approved().approved(), Some(&approved));

    let mut older = serde_json::to_value(&approved).unwrap();
    older.as_object_mut().unwrap().remove("floor_file");
    write_approved(&home, &serde_json::to_vec(&older).unwrap());
    let read = home.read_approved();
    let read = read
        .approved()
        .expect("a record without floor_file still reads");
    assert_eq!(read.floor_file, None);
    assert_eq!(read.floor_file(), approved.canonical.floor_path);
}

#[test]
fn floor_resolution_order() {
    let dir = tempfile::tempdir().unwrap();
    let override_floor = dir.path().join("override").join("officina.toml");
    let approved_floor = dir.path().join("Approved").join("Officina.toml");
    let approved = ApprovedState::Valid(approved_for(&approved_floor));

    let with_override = RelayHome::new(dir.path(), Some(override_floor.clone()));
    for state in [
        approved.clone(),
        ApprovedState::Missing,
        ApprovedState::Unreadable("corrupt".into()),
    ] {
        assert_eq!(with_override.relay_floor(&state), override_floor);
    }

    let without = RelayHome::new(dir.path(), None);
    assert_eq!(
        without.relay_floor(&approved),
        approved_floor,
        "the approved floor path as the approval read it, not normalised"
    );
    let mut older = approved_for(&approved_floor);
    older.floor_file = None;
    assert_eq!(
        without
            .relay_floor(&ApprovedState::Valid(older.clone()))
            .to_string_lossy(),
        older.canonical.floor_path,
        "a record written before floor_file falls back to the canonical path"
    );
    for state in [
        ApprovedState::Missing,
        ApprovedState::Unreadable("corrupt".into()),
    ] {
        assert_eq!(without.relay_floor(&state), dir.path().join("floor.toml"));
    }
}

#[test]
fn relay_floor_loads_or_says_why_not() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("officina.toml");
    assert!(matches!(
        load_relay_floor(&path),
        Err(RelayFloorError::Unreadable { .. })
    ));
    std::fs::write(&path, "schema = 9\n").unwrap();
    assert!(matches!(
        load_relay_floor(&path),
        Err(RelayFloorError::Invalid { .. })
    ));
    std::fs::write(&path, FLOOR).unwrap();
    let loaded = load_relay_floor(&path).unwrap();
    assert!(loaded.catalogue.actions.contains_key("probe"));
    assert_eq!(loaded.hash, approved_for(&path).hash);
}

#[test]
fn known_projects_are_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let home = RelayHome::new(dir.path(), None);
    assert_eq!(home.read_known_projects(), KnownProjectsState::Missing);
    std::fs::create_dir_all(home.relay_dir()).unwrap();
    std::fs::write(
        home.known_projects_file(),
        r#"{"schema":1,"approved_hash":null,"project_paths":["c:/work/lab"]}"#,
    )
    .unwrap();
    let known = home.read_known_projects().valid().cloned().unwrap();
    assert_eq!(known.project_paths, ["c:/work/lab"]);
}

/// Story 2.16 AC 1: a record that exists but does not read is told apart
/// from one never written, so the gate's fault says which.
#[test]
fn an_unreadable_known_projects_record_is_not_missing() {
    let dir = tempfile::tempdir().unwrap();
    let home = RelayHome::new(dir.path(), None);
    std::fs::create_dir_all(home.relay_dir()).unwrap();
    for content in ["{}", "not json", r#"{"schema":99,"project_paths":[]}"#] {
        std::fs::write(home.known_projects_file(), content).unwrap();
        assert!(
            matches!(
                home.read_known_projects(),
                KnownProjectsState::Unreadable(_)
            ),
            "{content}"
        );
    }
    std::fs::remove_file(home.known_projects_file()).unwrap();
    std::fs::create_dir_all(home.known_projects_file()).unwrap();
    assert!(matches!(
        home.read_known_projects(),
        KnownProjectsState::Unreadable(_)
    ));
}

// ---- story 2.8: approval files and kept outputs (AD-33, D-170) ----

/// Writes `name` in `dir` and dates it `age` ago.
fn aged_file(dir: &Path, name: &str, age: std::time::Duration) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, b"{}").unwrap();
    let file = std::fs::File::options().write(true).open(&path).unwrap();
    file.set_modified(std::time::SystemTime::now() - age)
        .unwrap();
    path
}

/// A retention of `ms` milliseconds, as a file age.
fn age(ms: i64) -> std::time::Duration {
    std::time::Duration::from_millis(u64::try_from(ms).unwrap())
}

#[test]
fn approval_files_older_than_their_retention_are_pruned() {
    let dir = tempfile::tempdir().unwrap();
    let home = RelayHome::new(dir.path(), None);
    let approvals = home.approvals_dir();
    let past = age(RETENTION_MS * 2);
    let old: Vec<PathBuf> = FileKind::ALL
        .iter()
        .map(|kind| aged_file(&approvals, &file_name("0123456789abcdef-1", *kind), past))
        .collect();
    let fresh = aged_file(
        &approvals,
        &file_name("0123456789abcdef-2", FileKind::Request),
        age(RETENTION_MS / 2),
    );
    let foreign = aged_file(&approvals, "notes.txt", past);
    assert_eq!(prune(&home, std::time::SystemTime::now()), old.len());
    for path in &old {
        assert!(!path.exists(), "{}", path.display());
    }
    assert!(fresh.exists(), "younger than the retention");
    assert!(foreign.exists(), "not an approval file");
}

#[test]
fn kept_outputs_older_than_their_retention_are_pruned() {
    let dir = tempfile::tempdir().unwrap();
    let home = RelayHome::new(dir.path(), None);
    let outputs = home.outputs_dir();
    let old = aged_file(
        &outputs,
        &kept_output_name("old-run"),
        age(KEPT_OUTPUT_RETENTION_MS * 2),
    );
    let recent = aged_file(
        &outputs,
        &kept_output_name("recent-run"),
        age(KEPT_OUTPUT_RETENTION_MS / 2),
    );
    assert_eq!(prune(&home, std::time::SystemTime::now()), 1);
    assert!(!old.exists());
    assert!(recent.exists(), "younger than the retention");
}

#[test]
fn run_never_deletes_approval_files() {
    let test = TestHome::new();
    let home = test.relay(None);
    let lab = test.root.join("Lab");
    std::fs::create_dir_all(&lab).unwrap();
    let floor = format!(
        "schema = 2\n[[project]]\nname = \"lab\"\npath = \"{}\"\nactions = [\"restart\"]\n[[action]]\nname = \"restart\"\nclass = \"human\"\nprogram = \"C:/path/to/tool.exe\"\n",
        lab.to_string_lossy().replace('\\', "/")
    );
    test.write(&home.default_floor(), &floor);
    test.approve_by_hand(&home);
    let old: Vec<PathBuf> = FileKind::ALL
        .iter()
        .map(|kind| {
            aged_file(
                &home.approvals_dir(),
                &file_name("0123456789abcdef-1", *kind),
                age(RETENTION_MS * 2),
            )
        })
        .collect();

    let code = officina_host::relay::run::run(
        &home,
        &lab.to_string_lossy(),
        "restart",
        &std::collections::BTreeMap::new(),
        None,
        std::time::Duration::from_secs(1),
    );
    assert_eq!(code, 126, "no decision within the wait");
    for path in &old {
        assert!(path.exists(), "{}", path.display());
    }
    let requests = std::fs::read_dir(home.approvals_dir())
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".request.json")
        })
        .count();
    assert_eq!(requests, 2, "the old request and the new one");
}

/// Story 2.16 AC 6 (D-175): a request file names its dry run's id, and any
/// process can write it; an id that is no run id, an absolute path to
/// another `.txt` file say, reads nothing, while a run's own id still
/// reads its kept output.
#[test]
fn a_dry_run_id_that_is_no_run_id_reads_nothing() {
    use officina_host::relay::approvals::{keep_output, read_output};
    let test = TestHome::new();
    let home = test.relay(None);
    let elsewhere = test.root.join("notes");
    test.write(&elsewhere.with_extension("txt"), "not a dry run's output");
    let named = elsewhere.to_string_lossy().into_owned();
    match read_output(&home, &named) {
        Some(Err(why)) => assert!(why.contains("no valid dry run"), "{why}"),
        other => panic!("{other:?}"),
    }
    let run = officina_core::relay::approvals::run_id(now_ms(), std::process::id());
    keep_output(&home, &run, b"PLAY [lab]").unwrap();
    assert_eq!(read_output(&home, &run), Some(Ok("PLAY [lab]".to_owned())));
}

/// Story 2.15 AC 6 (D-15): a known-projects record the window cannot write
/// is journaled as a gate error, once and not once per pass, which the
/// relay view lists; a write that succeeds again ends it.
#[test]
fn a_known_projects_write_error_is_reported() {
    let test = TestHome::new();
    let home = test.relay(None);
    test.write(&home.default_floor(), FLOOR);
    std::fs::create_dir_all(home.known_projects_file()).unwrap();
    let mut keeper = KnownProjectsKeeper::default();
    let now = now_ms();
    let failing = keeper
        .keep(&home, now)
        .map(str::to_owned)
        .expect("a folder where the record goes fails the write");
    assert!(failing.contains("known-projects.json"), "{failing}");
    assert_eq!(keeper.keep(&home, now), Some(failing.as_str()));

    let view = read_view(&home, now).unwrap();
    let errors: Vec<&GateError> = view
        .recent
        .iter()
        .filter_map(|entry| match &entry.event {
            RecentEvent::GateError(error) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].cause, GateFault::KnownProjectsUnwritable);
    assert_eq!(errors[0].detail, failing);
    assert_eq!(view.gate_errors, 1);

    std::fs::remove_dir(home.known_projects_file()).unwrap();
    assert_eq!(keeper.keep(&home, now), None, "the write succeeds again");
    assert!(home.read_known_projects().valid().is_some());
}
