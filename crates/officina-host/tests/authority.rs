//! The window's authority over the catalogue (AD-21, AD-24, D-8, D-15).
//! Built only with `window-authority`, like the window.

use std::path::{Path, PathBuf};

use officina_core::quota::normalise_path;
use officina_host::relay::authority::{AUTHORITY_MARKER, AuthorityError, approve_catalogue};
use officina_host::relay::home::{ApprovedState, RelayHome, load_relay_floor};

const FLOOR: &str = "schema = 2\n\n[[project]]\nname = \"lab\"\npath = \"Lab\"\nactions = [\"probe\"]\n\n[[project]]\nname = \"notes\"\npath = \"Notes\"\nautonomy = \"free\"\n\n[[action]]\nname = \"probe\"\nclass = \"free\"\nprogram = \"C:/path/to/tool.exe\"\nargs = [\"{host}\"]\n\n[action.params.host]\nvalues = [\"web01\"]\n";

struct Home {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Home {
    fn new() -> Home {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        Home { _dir: dir, root }
    }

    fn relay(&self, floor_override: Option<PathBuf>) -> RelayHome {
        RelayHome::new(self.root.join("officina-home"), floor_override)
    }

    fn write(&self, path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    /// The hash the window would display for the floor at `path`.
    fn displayed(&self, path: &Path) -> String {
        load_relay_floor(path).unwrap().hash
    }
}

#[test]
fn approves_the_displayed_hash() {
    let home = Home::new();
    let relay = home.relay(None);
    home.write(&relay.default_floor(), FLOOR);
    let displayed = home.displayed(&relay.default_floor());

    let approved = approve_catalogue(&relay, &displayed).unwrap();
    assert_eq!(approved.hash, displayed);
    assert!(approved.approved_at_ms > 0);
    assert_eq!(
        relay.read_approved(),
        ApprovedState::Valid(approved.clone())
    );
    assert!(approved.canonical.actions.contains_key("probe"));
    assert!(
        !relay.relay_dir().read_dir().unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")),
        "atomic writes leave no temp file"
    );
}

#[test]
fn edited_floor_between_display_and_click_is_not_approved() {
    let home = Home::new();
    let relay = home.relay(None);
    home.write(&relay.default_floor(), FLOOR);
    let displayed = home.displayed(&relay.default_floor());
    home.write(
        &relay.default_floor(),
        &FLOOR.replace("C:/path/to/tool.exe", "C:/Windows/System32/cmd.exe"),
    );

    let error = approve_catalogue(&relay, &displayed).unwrap_err();
    assert!(matches!(error, AuthorityError::CatalogueChanged { .. }));
    assert!(error.to_string().starts_with(AUTHORITY_MARKER));
    assert_eq!(relay.read_approved(), ApprovedState::Missing);
    assert!(!relay.known_projects_file().exists());
}

#[test]
fn invalid_actions_do_not_block_approval() {
    let home = Home::new();
    let relay = home.relay(None);
    let with_broken = format!(
        "{FLOOR}\n[[action]]\nname = \"broken\"\nclass = \"free\"\nprogram = \"relative.exe\"\n"
    );
    home.write(&relay.default_floor(), &with_broken);
    let loaded = load_relay_floor(&relay.default_floor()).unwrap();
    assert!(!loaded.catalogue.issues.is_empty());

    let approved = approve_catalogue(&relay, &loaded.hash).unwrap();
    assert!(!approved.canonical.actions.contains_key("broken"));
    home.write(&relay.default_floor(), FLOOR);
    assert_eq!(
        home.displayed(&relay.default_floor()),
        approved.hash,
        "an invalid action is no part of the approved meaning"
    );
}

#[test]
fn approval_hashes_the_relay_floor_not_another() {
    let home = Home::new();
    let window_floor = home.root.join("window").join("officina.toml");
    let relay_floor = home.root.join("relay").join("officina.toml");
    home.write(&window_floor, FLOOR);
    home.write(
        &relay_floor,
        &FLOOR.replace("values = [\"web01\"]", "values = [\"web01\", \"db01\"]"),
    );
    let relay = home.relay(Some(relay_floor.clone()));

    let error = approve_catalogue(&relay, &home.displayed(&window_floor)).unwrap_err();
    assert!(matches!(error, AuthorityError::CatalogueChanged { .. }));
    assert_eq!(relay.read_approved(), ApprovedState::Missing);

    let approved = approve_catalogue(&relay, &home.displayed(&relay_floor)).unwrap();
    assert_eq!(
        approved.canonical.floor_path,
        normalise_path(&relay_floor.to_string_lossy())
    );

    // Without the override, the floor recorded in the approval is the relay's.
    let after = home.relay(None);
    assert_eq!(
        normalise_path(&after.relay_floor(&after.read_approved()).to_string_lossy()),
        normalise_path(&relay_floor.to_string_lossy())
    );
    assert!(approve_catalogue(&after, &approved.hash).is_ok());
}

#[test]
fn approval_records_known_projects() {
    let home = Home::new();
    let relay = home.relay(None);
    home.write(&relay.default_floor(), FLOOR);
    let approved = approve_catalogue(&relay, &home.displayed(&relay.default_floor())).unwrap();

    let known = relay.read_known_projects().unwrap();
    assert_eq!(known.approved_hash.as_deref(), Some(approved.hash.as_str()));
    let floor_dir = normalise_path(&relay.officina_home().to_string_lossy());
    assert_eq!(
        known.project_paths,
        [format!("{floor_dir}/lab"), format!("{floor_dir}/notes")],
        "every declared project, free ones included"
    );
}

#[test]
fn a_missing_floor_is_an_error_that_writes_nothing() {
    let home = Home::new();
    let relay = home.relay(None);
    let error = approve_catalogue(&relay, "00").unwrap_err();
    assert!(matches!(error, AuthorityError::Floor(_)));
    assert!(error.to_string().starts_with(AUTHORITY_MARKER));
    assert!(!relay.relay_dir().exists());
}
