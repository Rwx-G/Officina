//! The tree fingerprint (stories 2.6 and 2.14, AD-33, D-22, D-23, D-173), on
//! temporary repositories built with `git init`. Each test skips with a
//! printed reason on a machine without `git`.

#![expect(
    clippy::disallowed_methods,
    reason = "a test reruns itself in a child process with git variables set"
)]

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::git_support::{git_ok, git_or_skip, run_git};

use officina_core::relay::journal::TreeFingerprint;
use officina_host::relay::tree::{TreeError, fingerprint_with};

/// A temporary work tree: `git init`, no line ending conversion, and
/// optionally one commit holding `playbook.yml`.
struct Repo {
    _dir: tempfile::TempDir,
    root: PathBuf,
    git: PathBuf,
}

impl Repo {
    fn new(git: PathBuf, commit: bool) -> Repo {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Supervision");
        std::fs::create_dir_all(&root).unwrap();
        git_ok(&git, &root, &["init", "-q"]);
        git_ok(&git, &root, &["config", "core.autocrlf", "false"]);
        let repo = Repo {
            _dir: dir,
            root,
            git,
        };
        repo.write("playbook.yml", b"- hosts: lab\n");
        if commit {
            repo.git(&["add", "playbook.yml"]);
            repo.git(&["commit", "-q", "-m", "first"]);
        }
        repo
    }

    fn git(&self, args: &[&str]) {
        git_ok(&self.git, &self.root, args);
    }

    fn write(&self, name: &str, content: &[u8]) {
        let path = self.root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn fingerprint(&self) -> TreeFingerprint {
        fingerprint_with(&self.git, &self.root)
            .unwrap()
            .expect("a work tree has a fingerprint")
    }
}

macro_rules! repo {
    ($test:literal, $commit:expr) => {
        match git_or_skip($test) {
            Some(git) => Repo::new(git, $commit),
            None => return,
        }
    };
}

#[test]
fn same_tree_same_fingerprint() {
    let repo = repo!("same_tree_same_fingerprint", true);
    repo.write("notes/vars.yml", b"group: lab-web\n");
    let first = repo.fingerprint();
    assert_eq!(first, repo.fingerprint());
    assert_eq!(first.sha256.len(), 64);
    assert_eq!(first.files, 2);
    assert_eq!(
        first.git.as_deref(),
        Some(repo.git.to_string_lossy().as_ref()),
        "the record names the git that listed the files"
    );
    let sub = repo.root.join("notes");
    assert_eq!(
        fingerprint_with(&repo.git, &sub).unwrap(),
        Some(first),
        "a project in a subfolder binds the whole work tree"
    );
}

#[test]
fn tracked_edit_changes_it() {
    let repo = repo!("tracked_edit_changes_it", true);
    let before = repo.fingerprint();
    repo.write("playbook.yml", b"- hosts: all\n");
    let edited = repo.fingerprint();
    assert_ne!(before, edited);
    repo.git(&["add", "playbook.yml"]);
    assert_eq!(edited, repo.fingerprint(), "staging changes nothing");
    repo.git(&["commit", "-q", "-m", "second"]);
    assert_eq!(
        edited,
        repo.fingerprint(),
        "a commit of the same content changes nothing: the files bind, not history"
    );
}

#[test]
fn same_size_untracked_edit_changes_it() {
    let repo = repo!("same_size_untracked_edit_changes_it", true);
    repo.write("vars.yml", b"group: aaaa\n");
    let before = repo.fingerprint();
    repo.write("vars.yml", b"group: bbbb\n");
    assert_ne!(before, repo.fingerprint());
    repo.write("vars.yml", b"group: aaaa\n");
    assert_eq!(before, repo.fingerprint());
}

/// Adversarial pass: `assume-unchanged` and `skip-worktree` hide a tracked
/// edit from `git diff HEAD`, so a session could check, flag the playbook,
/// edit it and apply. Flagged files are bound by content.
#[test]
fn flagged_tracked_edits_change_it() {
    let repo = repo!("flagged_tracked_edits_change_it", true);
    repo.write("inventory.yml", b"lab: [web01]\n");
    repo.git(&["add", "inventory.yml"]);
    repo.git(&["commit", "-q", "-m", "inventory"]);
    for (flag, file) in [
        ("--assume-unchanged", "playbook.yml"),
        ("--skip-worktree", "inventory.yml"),
    ] {
        repo.git(&["update-index", flag, file]);
        let before = repo.fingerprint();
        repo.write(file, b"- hosts: everything\n");
        let diff = run_git(&repo.git, &repo.root, &["diff", "HEAD"]);
        assert!(diff.stdout.is_empty(), "{flag}: git diff hides the edit");
        assert_ne!(before, repo.fingerprint(), "{flag}");
    }
}

#[test]
fn ignored_files_do_not_count() {
    let repo = repo!("ignored_files_do_not_count", true);
    repo.write(".gitignore", b"*.retry\nvault/\n");
    let before = repo.fingerprint();
    repo.write("site.retry", b"web01\n");
    repo.write("vault/secrets.yml", b"x\n");
    assert_eq!(before, repo.fingerprint());
}

#[test]
fn repository_without_commit_is_bound() {
    let repo = repo!("repository_without_commit_is_bound", false);
    let before = repo.fingerprint();
    repo.write("playbook.yml", b"- hosts: all\n");
    let edited = repo.fingerprint();
    assert_ne!(before, edited, "an untracked file of a new repository");
    repo.git(&["add", "playbook.yml"]);
    assert_eq!(
        edited,
        repo.fingerprint(),
        "staged or not, the content binds"
    );
    repo.write("playbook.yml", b"- hosts: web\n");
    assert_ne!(edited, repo.fingerprint(), "a staged file edited again");
    repo.git(&["commit", "-q", "-m", "first"]);
    assert_ne!(
        edited,
        repo.fingerprint(),
        "the no HEAD marker never equals a commit"
    );
}

/// A command for `git` to run through its shell that appends `tag` to the
/// marker file, then does what `rest` says.
fn trap(marker: &Path, tag: &str, rest: &str) -> String {
    let marker = marker.to_string_lossy().replace('\\', "/");
    format!("echo {tag} >> '{marker}'; {rest}")
}

#[test]
fn external_diff_and_textconv_are_not_run() {
    let repo = repo!("external_diff_and_textconv_are_not_run", true);
    let marker = repo.root.parent().unwrap().join("marker.txt");
    repo.write(".gitattributes", b"*.yml diff=trap\n");
    repo.git(&["add", ".gitattributes"]);
    repo.git(&["commit", "-q", "-m", "attributes"]);
    repo.git(&[
        "config",
        "diff.external",
        &trap(&marker, "external", "true"),
    ]);
    repo.git(&[
        "config",
        "diff.trap.textconv",
        &trap(&marker, "textconv", "cat"),
    ]);
    repo.write("playbook.yml", b"- hosts: all\n");

    let fingerprint = repo.fingerprint();
    assert!(
        !marker.exists(),
        "the fingerprint ran a program the repository configured"
    );
    assert_eq!(fingerprint, repo.fingerprint());

    // Positive control: a plain `git diff` runs the trap, so the assertion
    // above can fail.
    let control = run_git(&repo.git, &repo.root, &["diff", "HEAD"]);
    assert!(control.status.success());
    assert!(marker.exists(), "the trap is armed");
}

#[test]
fn git_variables_do_not_redirect_it() {
    let repo = repo!("git_variables_do_not_redirect_it", true);
    let other = repo!("git_variables_do_not_redirect_it", true);
    other.write("playbook.yml", b"- hosts: elsewhere\n");
    let before = repo.fingerprint();
    assert_ne!(before, other.fingerprint(), "a redirect would show");
    // The variables go to a child test process: setting them here would
    // reach every test running alongside.
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "git_variables_child", "--nocapture"])
        .env("OFFICINA_TREE_CHILD_REPO", &repo.root)
        .env("OFFICINA_TREE_CHILD_GIT", &repo.git)
        .env("GIT_DIR", other.root.join(".git"))
        .env("GIT_WORK_TREE", &other.root)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&child.stdout);
    assert!(child.status.success(), "{text}");
    let line = text
        .lines()
        .find_map(|l| l.strip_prefix("fingerprint="))
        .unwrap_or_else(|| panic!("{text}"));
    assert_eq!(line, before.sha256);
}

/// Run by `git_variables_do_not_redirect_it` in a process whose `GIT_DIR`
/// and `GIT_WORK_TREE` name another repository; does nothing otherwise.
#[test]
fn git_variables_child() {
    let (Some(repo), Some(git)) = (
        std::env::var_os("OFFICINA_TREE_CHILD_REPO"),
        std::env::var_os("OFFICINA_TREE_CHILD_GIT"),
    ) else {
        return;
    };
    let found = fingerprint_with(Path::new(&git), Path::new(&repo))
        .unwrap()
        .unwrap();
    println!("fingerprint={}", found.sha256);
}

#[test]
fn outside_git_it_is_absent() {
    let Some(git) = git_or_skip("outside_git_it_is_absent") else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("Notes");
    std::fs::create_dir_all(&plain).unwrap();
    assert_eq!(fingerprint_with(&git, &plain), Ok(None), "not a work tree");
    let repo = Repo::new(git, true);
    assert!(
        fingerprint_with(&repo.root.join(".git"), &repo.root).is_err(),
        "a git that cannot start cannot bind a work tree"
    );
    assert!(
        matches!(
            fingerprint_with(&repo.git, &repo.root.join(".git")),
            Err(TreeError::Unreadable(_))
        ),
        "inside the git directory is no work tree, and lies under a .git: refused (D-175)"
    );
}
