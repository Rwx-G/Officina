//! An approval written in the previous canonical format (AD-76, D-131): the
//! approval of the golden floor that `main` serialized before the format
//! changed, moved to another Officina home. Only its paths change; its
//! fields, its format and its stored hash stay as written, so the file on
//! disk is one the previous version wrote. Included by `#[path]` from the
//! host and CLI test crates and from the `approve_fixture` example.

#![allow(dead_code)] // each including crate uses a different subset

use std::path::Path;

use officina_core::floor::floor_dir_of;
use officina_core::quota::normalise_path;

/// The golden floor that approval was given for.
pub const GOLDEN_FLOOR: &str =
    include_str!("../../crates/officina-core/tests/data/relay/golden-floor.toml");

/// The approval, as serialized before the format changed.
pub const PREVIOUS_APPROVAL: &str =
    include_str!("../../crates/officina-core/tests/data/relay/approved-previous-format.json");

/// The folder the previous approval read its floor in, normalised.
pub const PREVIOUS_FLOOR_DIR: &str = "c:/path/to/work";

/// [`PREVIOUS_APPROVAL`] as read at `floor_file`: the floor path and every
/// project path under [`PREVIOUS_FLOOR_DIR`] moved to `floor_file`'s
/// folder. Edited as JSON, so no field of the current format appears.
pub fn previous_approval_at(floor_file: &Path) -> String {
    let mut approval: serde_json::Value =
        serde_json::from_str(PREVIOUS_APPROVAL).expect("the fixture is JSON");
    let floor_file = floor_file.to_string_lossy().into_owned();
    let dir = normalise_path(floor_dir_of(&floor_file));
    approval["floor_file"] = floor_file.clone().into();
    approval["canonical"]["floor_path"] = normalise_path(&floor_file).into();
    let projects = approval["canonical"]["projects"]
        .as_object_mut()
        .expect("the fixture's projects");
    for project in projects.values_mut() {
        let path = project["path"].as_str().unwrap_or_default().to_owned();
        if let Some(rest) = path.strip_prefix(PREVIOUS_FLOOR_DIR) {
            project["path"] = format!("{dir}{rest}").into();
        }
    }
    serde_json::to_string_pretty(&approval).expect("JSON serializes")
}
