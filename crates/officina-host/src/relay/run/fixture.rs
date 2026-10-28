//! The relay home the unit tests of `officina run` share: one guarded
//! project with one `human` action, approved as `relay/approved.json`
//! records it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use officina_core::relay::catalogue::{ApprovedCatalogue, load};
use officina_core::relay::journal::JournalEntry;
use officina_core::relay::run::{RunPlan, plan};

use super::{Context, with_inputs};
use crate::files::now_ms;
use crate::relay::credential::SystemCredentialStore;
use crate::relay::home::RelayHome;
use crate::relay::journal;

pub(super) struct Fixture {
    _dir: tempfile::TempDir,
    pub home: RelayHome,
    pub project: PathBuf,
    pub params: BTreeMap<String, String>,
}

impl Fixture {
    /// `restart service=web01`, of class `human`, in project `lab`.
    pub fn human() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let home = RelayHome::new(dir.path().join("officina-home"), None);
        let project = dir.path().join("Lab");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(home.relay_dir()).unwrap();
        let floor = format!(
            "schema = 2\n[[project]]\nname = \"lab\"\npath = \"{}\"\nactions = [\"restart\"]\n[[action]]\nname = \"restart\"\nclass = \"human\"\nprogram = \"C:/path/to/tool.exe\"\nargs = [\"{{service}}\"]\n[action.params.service]\nvalues = [\"web01\"]\n",
            project.to_string_lossy().replace('\\', "/")
        );
        let floor_path = home.default_floor();
        std::fs::write(&floor_path, &floor).unwrap();
        let relay = load(&floor, &floor_path.to_string_lossy()).unwrap();
        assert!(
            relay.catalogue.issues.is_empty(),
            "{:?}",
            relay.catalogue.issues
        );
        let approved =
            ApprovedCatalogue::approving(&relay, &floor_path.to_string_lossy(), now_ms());
        std::fs::write(home.approved_file(), serde_json::to_vec(&approved).unwrap()).unwrap();
        Fixture {
            _dir: dir,
            home,
            project,
            params: BTreeMap::from([("service".to_owned(), "web01".to_owned())]),
        }
    }

    pub fn cwd(&self) -> String {
        self.project.to_string_lossy().into_owned()
    }

    pub fn context(&self) -> Context<'_> {
        Context {
            home: &self.home,
            cwd: self.project.to_str().unwrap(),
            action: "restart",
            params: &self.params,
            session_id: None,
            store: &SystemCredentialStore,
        }
    }

    /// The plan the checks give for `restart`.
    pub fn plan(&self) -> RunPlan {
        with_inputs(&self.home, &self.cwd(), |inputs| {
            plan(inputs, "restart", &self.params)
        })
        .unwrap()
    }

    pub fn entries(&self) -> Vec<JournalEntry> {
        let (lines, corrupt) = journal::read_recent(&self.home, now_ms());
        assert_eq!(corrupt, 0);
        lines.into_iter().map(|line| line.entry).collect()
    }
}
