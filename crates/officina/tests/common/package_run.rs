//! A run launched through the window's functions on the fake engine, whose
//! packer copy streams a package built by real git in a clone of the
//! project (D-23), then collected (stories 6.13 to 6.15): the harness
//! `workshop_collect.rs` and `workshop_review.rs` share.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use officina_core::relay::journal::JournalEntry;
use officina_core::workshop::tar::{
    PACKAGE_BUNDLE, PACKAGE_DIR, PACKAGE_MANIFEST, PACKAGE_TIP, stream_of,
};
use officina_host::registry::workshop::RunRecord;
use officina_host::workshop::follow::FollowContext;
use officina_host::workshop::launch::WorkshopRuntime;

use super::git_support::run_git;
use super::workshop_home::{Home, Options};

pub const WAIT: Duration = Duration::from_secs(60);

pub fn until(what: &str, mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < WAIT, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub fn base_env() -> Vec<(String, String)> {
    vec![(
        "SYSTEMROOT".to_owned(),
        std::env::var("SYSTEMROOT").unwrap_or_default(),
    )]
}

pub fn said(text: &str) -> String {
    serde_json::json!({"type": "assistant", "message": {"content": [{"type": "text", "text": text}]}}).to_string()
}

pub fn result() -> String {
    serde_json::json!({"type": "result", "subtype": "success", "is_error": false, "total_cost_usd": 0.02, "num_turns": 2, "duration_ms": 900, "session_id": "s"}).to_string()
}

/// One package entry: its name in the stream and its bytes.
pub type Entry = (String, Vec<u8>);

/// A launched run of a home.
pub struct Run<'h> {
    pub home: &'h Home,
    pub runtime: WorkshopRuntime,
    pub id: String,
}

/// A home whose fake engine replays `worker` as the worker's output.
pub fn home(options: &Options, worker: &[String]) -> Option<Home> {
    let home = Home::start(options)?;
    home.set_engine(
        "follow",
        serde_json::json!({"worker": {"stdout": worker.join("\n")}, "gateway": {"stdout": ""}}),
    );
    Some(home)
}

/// A card issued and launched with `runtime`.
pub fn launch<'h>(home: &'h Home, runtime: WorkshopRuntime) -> Run<'h> {
    let card = home.issue(&runtime);
    home.launch(&card, &runtime);
    Run {
        home,
        runtime,
        id: card.id,
    }
}

pub fn git_text(home: &Home, dir: &Path, args: &[&str]) -> String {
    let out = run_git(&home.git, dir, args);
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

impl Run<'_> {
    pub fn row(&self) -> RunRecord {
        self.home.run(&self.id)
    }

    pub fn run_dir(&self) -> PathBuf {
        self.home
            .env
            .officina_home()
            .join("workshop")
            .join("runs")
            .join(&self.id)
    }

    /// The worker's clone: the project cloned, on the run's branch.
    pub fn clone(&self) -> PathBuf {
        let dir = self
            .home
            .env
            .root
            .path()
            .join(format!("clone-{}", &self.id[..8]));
        let project = self.home.project();
        git_text(
            self.home,
            self.home.env.root.path(),
            &[
                "clone",
                "-q",
                &project.to_string_lossy(),
                &dir.to_string_lossy(),
            ],
        );
        git_text(
            self.home,
            &dir,
            &["checkout", "-q", "-b", &self.row().branch],
        );
        dir
    }

    pub fn commit(&self, clone: &Path, path: &str, text: &str) {
        let file = clone.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, text).unwrap();
        git_text(self.home, clone, &["add", "-f", "--", path]);
        git_text(
            self.home,
            clone,
            &["commit", "-q", "-m", &format!("add {path}")],
        );
    }

    /// What the packer writes from `clone` (AD-110): the tip, a bundle of
    /// `refs` past `exclude` when asked, a manifest.
    pub fn packed(&self, clone: &Path, bundle: Option<(&[&str], &str)>) -> Vec<Entry> {
        let tip = git_text(self.home, clone, &["rev-parse", "HEAD"]);
        let mut entries = vec![
            (PACKAGE_DIR.to_owned(), Vec::new()),
            (PACKAGE_TIP.to_owned(), format!("{tip}\n").into_bytes()),
            (PACKAGE_MANIFEST.to_owned(), b"{\"steps\":[]}".to_vec()),
        ];
        if let Some((refs, exclude)) = bundle {
            let path = clone.join("work.bundle.tmp");
            let mut args = vec!["bundle", "create", "-q"];
            let target = path.to_string_lossy().into_owned();
            args.push(&target);
            args.extend_from_slice(refs);
            let excluded = format!("^{exclude}");
            args.push(&excluded);
            git_text(self.home, clone, &args);
            entries.push((PACKAGE_BUNDLE.to_owned(), std::fs::read(&path).unwrap()));
            std::fs::remove_file(&path).unwrap();
        }
        entries
    }

    /// A bundle of the run's branch past the base.
    pub fn packed_branch(&self, clone: &Path) -> Vec<Entry> {
        let branch = format!("refs/heads/{}", self.row().branch);
        let base = self.row().base_commit;
        self.packed(clone, Some((&[branch.as_str()], &base)))
    }

    /// The fake engine's packer copy streams `entries`, then `extra` zero
    /// bytes.
    pub fn serve(&self, entries: &[Entry], extra: u64) {
        let list: Vec<(&str, &[u8], u8)> = entries
            .iter()
            .map(|(name, bytes)| {
                let kind = if name.ends_with('/') { b'5' } else { b'0' };
                (name.as_str(), bytes.as_slice(), kind)
            })
            .collect();
        let path = self
            .home
            .env
            .root
            .path()
            .join(format!("package-{}.tar", &self.id[..8]));
        std::fs::write(&path, stream_of(&list)).unwrap();
        self.home
            .set_engine("package", path.to_string_lossy().into_owned().into());
        self.home.set_engine("package_extra_bytes", extra.into());
    }

    /// The worker's container exits with `code`.
    pub fn exit(&self, code: i64) {
        let path = self.home.engine.dir.path().join("fake-docker-state.json");
        let mut all: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let worker = format!("officina-worker-{}", self.id);
        for c in all["containers"].as_array_mut().unwrap() {
            if c["name"] == worker.as_str() {
                c["state"] = "exited".into();
                c["exit_code"] = code.into();
            }
        }
        std::fs::write(&path, all.to_string()).unwrap();
    }

    /// The run once collection decided it.
    pub fn collected(&self) -> RunRecord {
        until("the run's outcome", || self.row().outcome.is_some());
        self.home.settle();
        self.row()
    }

    pub fn lines(&self) -> Vec<JournalEntry> {
        self.home
            .env
            .journal()
            .into_iter()
            .map(|line| line.entry)
            .filter(|entry| run_of(entry) == Some(self.id.as_str()))
            .collect()
    }

    pub fn context(&self) -> FollowContext {
        FollowContext {
            runtime: self.runtime.clone(),
            collector: self.home.sender().clone(),
            home: self.home.env.officina_home(),
            base_env: base_env(),
        }
    }
}

pub fn run_of(entry: &JournalEntry) -> Option<&str> {
    match entry {
        JournalEntry::WorkshopFinished(l) => Some(&l.run_id),
        JournalEntry::WorkshopRemoved(l) => Some(&l.run_id),
        JournalEntry::GatewaySummary(l) => Some(&l.run_id),
        JournalEntry::ReviewPackage(l) => Some(&l.run_id),
        JournalEntry::ReviewOpened(l) => Some(&l.run_id),
        JournalEntry::SensitiveApproved(l) => Some(&l.run_id),
        _ => None,
    }
}
