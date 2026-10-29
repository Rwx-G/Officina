//! The relay harness of the CLI tests: an isolated environment whose floor
//! names real project folders and the fake tool, approved through the
//! window's authority, and `officina run` from a project folder. A test
//! file adds the methods only it needs in its own `impl Relay`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use officina_core::relay::journal::{
    JournalEntry, RefusalReason, Refused, RunFinished, RunStarted,
};

use super::{TestEnv, read_tool_log, run_with};

/// A path as a floor writes it: forward slashes, so a TOML string needs no
/// escaping.
pub fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// The floor builder: each `{name}` of `folders` becomes that folder, as a
/// floor writes it. [`TestEnv::write_floor`] fills `{tool}`.
pub fn fill(floor: &str, folders: &[(&str, PathBuf)]) -> String {
    folders.iter().fold(floor.to_owned(), |text, (name, dir)| {
        text.replace(&format!("{{{name}}}"), &slashed(dir))
    })
}

pub struct Relay {
    pub env: TestEnv,
    /// `git`'s folder on the commands' `PATH`, for a project in git.
    pub git_dir: Option<PathBuf>,
}

impl Relay {
    /// A relay whose floor is `floor`, its folders created and filled in
    /// ([`Relay::write`]), not approved.
    pub fn unapproved(floor: &str) -> Relay {
        let relay = Relay {
            env: TestEnv::new(),
            git_dir: None,
        };
        for dir in [relay.project(), relay.notes(), relay.outside()] {
            std::fs::create_dir_all(dir).unwrap();
        }
        relay.write(floor);
        relay
    }

    /// [`Relay::unapproved`], approved through the window's authority
    /// (story 2.2), never written by hand.
    pub fn approved(floor: &str) -> Relay {
        let relay = Relay::unapproved(floor);
        relay.env.approve();
        relay
    }

    /// The golden floor, approved by the previous version in the previous
    /// canonical format (AD-76, D-131): its `approved.json` as that version
    /// wrote it, and the window's record started from it as the window
    /// starts one for a record it finds missing.
    pub fn approved_in_the_previous_format() -> Relay {
        let relay = Relay::unapproved(super::previous_format::GOLDEN_FLOOR);
        let home = relay.env.relay_home();
        std::fs::create_dir_all(home.relay_dir()).unwrap();
        std::fs::write(
            home.approved_file(),
            super::previous_format::previous_approval_at(&relay.env.floor_file()),
        )
        .unwrap();
        officina_host::relay::authority::acknowledge_anomaly(
            &home,
            &officina_core::relay::record::record_anomaly_key(),
        )
        .unwrap();
        relay
    }

    /// The golden floor's guarded project, `supervision`, beside the floor.
    pub fn golden_project(&self) -> PathBuf {
        let dir = self.env.officina_home().join("Supervision");
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Writes `floor` unapproved, `{project}` and `{notes}` filled in.
    pub fn write(&self, floor: &str) {
        self.env.write_floor(&fill(
            floor,
            &[("project", self.project()), ("notes", self.notes())],
        ));
    }

    /// The guarded project's folder.
    pub fn project(&self) -> PathBuf {
        self.env.root.path().join("work").join("Supervision")
    }

    /// A free project's folder.
    pub fn notes(&self) -> PathBuf {
        self.env.root.path().join("work").join("Notes")
    }

    /// A folder in no project.
    pub fn outside(&self) -> PathBuf {
        self.env.root.path().join("elsewhere")
    }

    /// `officina run <args>` from `dir`, the tool logging its launches, with
    /// `git`'s folder as the whole `PATH` when the project is in git.
    pub fn command(&self, dir: &Path, args: &[&str]) -> Command {
        let mut full = vec!["run"];
        full.extend_from_slice(args);
        let mut cmd = self.env.officina(&full);
        cmd.current_dir(dir)
            .env("FAKE_TOOL_LOG", self.env.tool_log());
        if let Some(git_dir) = &self.git_dir {
            cmd.env("PATH", git_dir);
        }
        cmd
    }

    pub fn run_in(&self, dir: &Path, args: &[&str]) -> Output {
        run_with(self.command(dir, args), b"")
    }

    /// `officina run <args>` from the guarded project.
    pub fn run(&self, args: &[&str]) -> Output {
        self.run_in(&self.project(), args)
    }

    pub fn launches(&self) -> Vec<serde_json::Value> {
        read_tool_log(&self.env.tool_log())
    }

    pub fn entries(&self) -> Vec<JournalEntry> {
        self.env.journal_after_approvals()
    }

    pub fn refused(&self) -> Vec<Refused> {
        self.entries()
            .into_iter()
            .filter_map(|e| match e {
                JournalEntry::Refused(r) => Some(r),
                _ => None,
            })
            .collect()
    }

    pub fn started(&self) -> Vec<RunStarted> {
        self.entries()
            .into_iter()
            .filter_map(|e| match e {
                JournalEntry::RunStarted(s) => Some(s),
                _ => None,
            })
            .collect()
    }

    pub fn finished(&self) -> Vec<RunFinished> {
        self.entries()
            .into_iter()
            .filter_map(|e| match e {
                JournalEntry::RunFinished(f) => Some(f),
                _ => None,
            })
            .collect()
    }
}

pub fn stderr_text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

pub fn stderr_lines(out: &Output) -> Vec<String> {
    stderr_text(out).lines().map(str::to_owned).collect()
}

/// A refusal: exit 126, one stderr line naming the code, the journal's last
/// refusal the same, and the tool started no more than the
/// `launches_before` times of earlier calls.
pub fn assert_refused_after(
    relay: &Relay,
    out: &Output,
    reason: RefusalReason,
    launches_before: usize,
) {
    let lines = stderr_lines(out);
    assert_eq!(out.status.code(), Some(126), "{lines:?}");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("officina: refused: "), "{lines:?}");
    assert!(
        lines[0].ends_with(&format!("({})", reason.code())),
        "{lines:?}"
    );
    assert_eq!(
        relay.launches().len(),
        launches_before,
        "the tool must not start"
    );
    assert_eq!(relay.refused().last().map(|r| r.reason), Some(reason));
}

/// [`assert_refused_after`] a relay that never started the tool.
pub fn assert_refused(relay: &Relay, out: &Output, reason: RefusalReason) {
    assert_refused_after(relay, out, reason, 0);
}
