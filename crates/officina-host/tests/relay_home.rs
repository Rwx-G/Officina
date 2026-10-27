//! The relay's files under the Officina home (AD-25), the states of
//! `relay/approved.json` and the relay floor resolution (AD-24). Every test
//! works in its own temp home and sets nothing process-wide.

use std::path::{Path, PathBuf};

use officina_core::relay::catalogue::{ApprovedCatalogue, load};
use officina_host::relay::home::{ApprovedState, RelayFloorError, RelayHome, load_relay_floor};

const FLOOR: &str = "schema = 2\n\n[[project]]\nname = \"lab\"\npath = \"Lab\"\nactions = [\"probe\"]\n\n[[action]]\nname = \"probe\"\nclass = \"free\"\nprogram = \"C:/path/to/tool.exe\"\n";

fn approved_for(floor_path: &Path) -> ApprovedCatalogue {
    let relay_floor = load(FLOOR, &floor_path.to_string_lossy()).unwrap();
    ApprovedCatalogue {
        approved_at_ms: 1_793_102_400_000,
        hash: relay_floor.hash,
        floor_file: Some(floor_path.to_string_lossy().into_owned()),
        canonical: relay_floor.canonical,
    }
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
    assert_eq!(home.read_known_projects(), None);
    std::fs::create_dir_all(home.relay_dir()).unwrap();
    std::fs::write(
        home.known_projects_file(),
        r#"{"schema":1,"approved_hash":null,"project_paths":["c:/work/lab"]}"#,
    )
    .unwrap();
    let known = home.read_known_projects().unwrap();
    assert_eq!(known.project_paths, ["c:/work/lab"]);
}
