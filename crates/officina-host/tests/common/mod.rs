//! What the host's integration tests share: a temporary Officina home, its
//! files written, an approval recorded as the window records it where the
//! window's authority is not under test, and `git`.

#![allow(dead_code)] // each test file uses a different subset of the helpers

#[path = "../../../../tests/support/git.rs"]
pub mod git_support;
#[path = "../../../../tests/support/previous_format.rs"]
pub mod previous_format;
#[cfg(feature = "window-authority")]
pub mod registry;

use std::path::{Path, PathBuf};

use officina_core::relay::catalogue::ApprovedCatalogue;
use officina_core::relay::record::DecisionRecord;
use officina_host::files::now_ms;
use officina_host::relay::home::{RelayHome, load_relay_floor};

/// A temporary folder holding an Officina home, `officina-home`.
pub struct TestHome {
    _dir: tempfile::TempDir,
    pub root: PathBuf,
}

impl TestHome {
    pub fn new() -> TestHome {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        TestHome { _dir: dir, root }
    }

    /// The relay's view of the home, with `OFFICINA_FLOOR` as
    /// `floor_override`.
    pub fn relay(&self, floor_override: Option<PathBuf>) -> RelayHome {
        RelayHome::new(self.root.join("officina-home"), floor_override)
    }

    /// Writes `content` at `path`, its folder created.
    pub fn write(&self, path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    /// Records the approval of `home`'s relay floor in
    /// `relay/approved.json` and in the window's record, as the window does.
    pub fn approve_by_hand(&self, home: &RelayHome) -> ApprovedCatalogue {
        let floor = home.relay_floor(&home.read_approved());
        let relay = load_relay_floor(&floor).unwrap();
        let approved = ApprovedCatalogue::approving(&relay, &floor.to_string_lossy(), now_ms());
        self.write(
            &home.approved_file(),
            &serde_json::to_string(&approved).unwrap(),
        );
        let record = DecisionRecord::start(now_ms(), Some(approved.hash.clone()), None);
        self.write(
            &home.decisions_file(),
            &serde_json::to_string(&record).unwrap(),
        );
        approved
    }
}
