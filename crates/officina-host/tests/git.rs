//! Read-only git (story 5.2 AC 9; AD-73, D-22, D-128, D-163, R42): a
//! fingerprint, and any call through `host::git`, leaves the index as it
//! was even when plain `git` would refresh it; the control shows plain
//! `git` does. Each test skips with a printed reason without `git` (D-23).

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use common::git_support::{git_ok, git_or_skip};
use officina_host::relay::tree::{fingerprint_with, git_env};

/// A work tree with one committed file whose modification time was then
/// moved, so `git status` without optional locks off would rewrite the
/// index's stat data.
struct StaleRepo {
    _dir: tempfile::TempDir,
    root: PathBuf,
    git: PathBuf,
}

impl StaleRepo {
    fn new(git: PathBuf) -> StaleRepo {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Supervision");
        std::fs::create_dir_all(&root).unwrap();
        git_ok(&git, &root, &["init", "-q"]);
        let file = root.join("playbook.yml");
        std::fs::write(&file, b"- hosts: lab\n").unwrap();
        git_ok(&git, &root, &["add", "playbook.yml"]);
        git_ok(&git, &root, &["commit", "-q", "-m", "first"]);
        let touched = SystemTime::now() - Duration::from_secs(3600);
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(touched)
            .unwrap();
        StaleRepo {
            _dir: dir,
            root,
            git,
        }
    }

    fn index(&self) -> Vec<u8> {
        std::fs::read(self.root.join(".git").join("index")).unwrap()
    }
}

fn repo_or_skip(test: &str) -> Option<StaleRepo> {
    git_or_skip(test).map(StaleRepo::new)
}

#[test]
fn a_fingerprint_leaves_a_stale_index_unchanged() {
    let Some(repo) = repo_or_skip("a_fingerprint_leaves_a_stale_index_unchanged") else {
        return;
    };
    let before = repo.index();
    let found = fingerprint_with(&repo.git, &repo.root).unwrap();
    assert!(found.is_some(), "a work tree has a fingerprint");
    // A command that refreshes the index, through `host::git`.
    let (code, _) = officina_host::git::output(
        &repo.git,
        &repo.root,
        &["status", "--porcelain"],
        &git_env(),
    )
    .unwrap();
    assert_eq!(code, Some(0));
    assert!(repo.index() == before, "the index was written");
}

/// The control: the same `status` without the flag and the variable
/// rewrites the index, so the test above can fail.
#[test]
fn without_the_lock_variable_the_index_changes() {
    let Some(repo) = repo_or_skip("without_the_lock_variable_the_index_changes") else {
        return;
    };
    let before = repo.index();
    let status = plain_git(&repo.git, &repo.root, &["status", "--porcelain"]);
    assert!(status, "plain git status ran");
    assert!(repo.index() != before, "plain git left the index unchanged");
}

#[expect(
    clippy::disallowed_methods,
    reason = "the control runs git without host::git's flags"
)]
fn plain_git(git: &Path, dir: &Path, args: &[&str]) -> bool {
    std::process::Command::new(git)
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_OPTIONAL_LOCKS")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}
