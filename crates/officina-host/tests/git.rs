//! Read-only git (story 5.2 AC 9, story 5.10 AC 3; AD-73, AD-84, D-22,
//! D-128, D-154, D-163, R42): a fingerprint, and any call through
//! `host::git`, leaves the index as it was even when plain `git` would
//! refresh it; the control shows plain `git` does. The meeting reads the
//! commits since a session's start and one diff to the working tree, lists
//! untracked files without reading them, and runs no program a
//! repository's configuration names, which plain `git` would. Each test
//! skips with a printed reason without `git` (D-23).

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use common::git_support::{git_ok, git_or_skip, run_git};
use officina_core::meeting::{
    Changes, ChangesRead, Delivery, GitRead, MeetingCard, MeetingInput, since_iso,
};
use officina_core::untrusted::Untrusted;
use officina_host::git::meeting_changes;
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

// ------------------------------------------------------------ the meeting

/// One second, in milliseconds.
const SECOND_MS: i64 = 1000;

/// When the session of the meeting tests started (2026-10-27 12:00 UTC).
const SESSION_START_MS: i64 = 1_793_102_400_000;

/// Runs `git <args>` in `dir` with the fixed identity of `run_git` and
/// both dates set to `at_ms`, so a commit lands before or after the
/// session's start.
#[expect(
    clippy::disallowed_methods,
    reason = "the tests date their commits with git outside host::git"
)]
fn git_at(git: &Path, dir: &Path, args: &[&str], at_ms: i64) {
    let date = format!("{} +0000", at_ms / SECOND_MS);
    let out = std::process::Command::new(git)
        .args([
            "-c",
            "user.name=user01",
            "-c",
            "user.email=user@example.com",
            "-c",
            "core.autocrlf=false",
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgSign=false",
        ])
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_DATE", &date)
        .env("GIT_COMMITTER_DATE", &date)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A work tree with one commit before the session's start, two after it,
/// then a working-tree change and an untracked file.
struct MeetingRepo {
    dir: tempfile::TempDir,
    root: PathBuf,
    git: PathBuf,
}

impl MeetingRepo {
    fn new(git: PathBuf) -> MeetingRepo {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Backend");
        std::fs::create_dir_all(&root).unwrap();
        git_ok(&git, &root, &["init", "-q"]);
        let commit = |file: &str, text: &str, subject: &str, at: i64| {
            std::fs::write(root.join(file), text).unwrap();
            git_at(&git, &root, &["add", file], at);
            git_at(&git, &root, &["commit", "-q", "-m", subject], at);
        };
        commit(
            "app.txt",
            "base line\n",
            "before the session",
            SESSION_START_MS - 86_400_000,
        );
        commit(
            "app.txt",
            "base line\nfirst change\n",
            "first since the start",
            SESSION_START_MS + 3_600_000,
        );
        commit(
            "notes.txt",
            "notes\n",
            "second since the start",
            SESSION_START_MS + 7_200_000,
        );
        std::fs::write(
            root.join("app.txt"),
            "base line\nfirst change\nworking tree\n",
        )
        .unwrap();
        std::fs::write(root.join("scratch.tmp"), b"12345").unwrap();
        MeetingRepo { dir, root, git }
    }

    fn read(&self) -> GitRead {
        meeting_changes(
            &self.git,
            &self.root,
            &since_iso(SESSION_START_MS),
            &git_env(),
            Instant::now(),
        )
    }
}

fn meeting_repo_or_skip(test: &str) -> Option<MeetingRepo> {
    git_or_skip(test).map(MeetingRepo::new)
}

/// The changes section of `read`, which must be one.
fn changes(read: GitRead) -> ChangesRead {
    match build_changes(read) {
        Changes::Read(read) => *read,
        other => panic!("{other:?}"),
    }
}

/// `read` through core's view, as the meeting builds it.
fn build_changes(read: GitRead) -> Changes {
    use officina_core::board::{BoardInput, build_board};
    let board = build_board(
        &BoardInput {
            agents: Some(
                officina_core::agents::parse_agents(
                    r#"[{"pid":7,"sessionId":"s","kind":"interactive","status":"idle","startedAt":1}]"#,
                )
                .unwrap(),
            ),
            ..Default::default()
        },
        10,
    );
    let card = MeetingCard::from(&board.columns.your_turn[0]);
    officina_core::meeting::build(&MeetingInput {
        card: &card,
        notification: None,
        journal: &[],
        approvals: &[],
        git: read,
        logs: None,
        delivery: Delivery::FocusOnly,
        now_ms: 10,
    })
    .changes
}

fn text(value: &Untrusted<String>) -> String {
    serde_json::to_value(value)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

/// AD-84: the commits since the session's start, newest first, each with
/// its files; one diff from the parent of the oldest of them to the
/// working tree, so the commit before the start is not in it.
#[test]
fn commits_since_start_and_one_diff_to_the_working_tree() {
    let Some(repo) = meeting_repo_or_skip("commits_since_start_and_one_diff_to_the_working_tree")
    else {
        return;
    };
    let read = changes(repo.read());
    let subjects: Vec<String> = read.commits.iter().map(|c| text(&c.subject)).collect();
    assert_eq!(
        subjects,
        ["second since the start", "first since the start"]
    );
    assert_eq!(text(&read.commits[0].files[0]), "A notes.txt");
    assert_eq!(text(&read.commits[1].files[0]), "M app.txt");
    let diff = text(&read.diff.text);
    assert!(diff.contains("+first change"), "{diff}");
    assert!(diff.contains("+working tree"), "{diff}");
    assert!(diff.contains("+notes"), "{diff}");
    assert!(
        !diff.contains("+base line"),
        "the commit before the start is the base: {diff}"
    );
    let paths: Vec<String> = read.diff.files.iter().map(|f| text(&f.path)).collect();
    assert_eq!(paths, ["app.txt", "notes.txt"]);
    assert!(read.diff.files.iter().all(|f| !f.hunks.is_empty()));
    assert_eq!(read.diff_scope_note, None);
}

/// AD-84: an untracked file is listed by name and size and never read:
/// held open with no sharing, which makes any read fail, it is still
/// listed with its size.
#[test]
fn untracked_files_are_listed_never_read() {
    let Some(repo) = meeting_repo_or_skip("untracked_files_are_listed_never_read") else {
        return;
    };
    let held = hold_exclusively(&repo.root.join("scratch.tmp"));
    let read = changes(repo.read());
    let listed: Vec<(String, Option<u64>)> = read
        .untracked
        .iter()
        .map(|f| (text(&f.path), f.bytes))
        .collect();
    assert_eq!(listed, [("scratch.tmp".to_owned(), Some(5))]);
    assert!(!read.untracked_more);
    if held.is_some() {
        assert!(
            std::fs::read(repo.root.join("scratch.tmp")).is_err(),
            "the control: a read of the held file fails"
        );
    }
}

/// The file opened with no sharing on Windows, so no other open succeeds
/// while it is held; `None` elsewhere.
fn hold_exclusively(path: &Path) -> Option<std::fs::File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        Some(
            std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(path)
                .unwrap(),
        )
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}

/// R42, D-163: the meeting's reads leave an index whose stat data plain
/// git would refresh unchanged; `without_the_lock_variable_the_index_changes`
/// is the control.
#[test]
fn a_meeting_leaves_a_stale_index_unchanged() {
    let Some(repo) = repo_or_skip("a_meeting_leaves_a_stale_index_unchanged") else {
        return;
    };
    let before = repo.index();
    let read = meeting_changes(
        &repo.git,
        &repo.root,
        &since_iso(0),
        &git_env(),
        Instant::now(),
    );
    assert!(matches!(read, GitRead::Read { .. }), "{read:?}");
    assert!(repo.index() == before, "the index was written");
}

/// A marker program: appends a line to the returned file whenever
/// anything runs it.
fn marker_program(dir: &Path) -> (PathBuf, PathBuf) {
    let ran = dir.join("marker-ran.txt");
    let program = dir.join("marker.sh");
    let posix = |p: &Path| p.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &program,
        format!("#!/bin/sh\necho ran >> \"{}\"\nexit 0\n", posix(&ran)),
    )
    .unwrap();
    (program, ran)
}

/// A configuration a repository a session controls could carry, which
/// would run a program on a plain read, and the plain git command that
/// runs it, the control.
struct Configured {
    name: &'static str,
    control: &'static [&'static str],
}

const CONFIGURED: &[Configured] = &[
    Configured {
        name: "external diff",
        control: &["diff"],
    },
    Configured {
        name: "textconv",
        control: &["diff"],
    },
    Configured {
        name: "fsmonitor",
        control: &["status", "--porcelain"],
    },
    Configured {
        name: "log.showSignature with gpg.program",
        control: &["log", "-1"],
    },
    Configured {
        name: "filter driver",
        control: &["diff"],
    },
];

/// Sets `case` up in `repo`, pointing it at `program`.
fn configure(repo: &MeetingRepo, case: &Configured, program: &Path) {
    let program = program.to_string_lossy().replace('\\', "/");
    let set = |key: &str, value: &str| {
        git_ok(&repo.git, &repo.root, &["config", "--local", key, value]);
    };
    let attributes = |line: &str| std::fs::write(repo.root.join(".gitattributes"), line).unwrap();
    match case.name {
        "external diff" => set("diff.external", &program),
        "textconv" => {
            attributes("*.txt diff=mark\n");
            set("diff.mark.textconv", &program);
        }
        "fsmonitor" => set("core.fsmonitor", &program),
        "log.showSignature with gpg.program" => {
            set("log.showSignature", "true");
            set("gpg.program", &program);
            signed_commit(repo);
        }
        "filter driver" => {
            attributes("*.txt filter=mark\n");
            set("filter.mark.clean", &program);
        }
        other => panic!("no setup for {other}"),
    }
}

/// A commit after the session's start carrying a `gpgsig` header, so
/// `log.showSignature` hands it to `gpg.program`.
#[expect(
    clippy::disallowed_methods,
    reason = "the test writes a signed commit object with git outside host::git"
)]
fn signed_commit(repo: &MeetingRepo) {
    let run = |args: &[&str], stdin: &str| -> String {
        use std::io::Write;
        let mut child = std::process::Command::new(&repo.git)
            .args(args)
            .current_dir(&repo.root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "git {args:?}");
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    };
    let tree = run(&["rev-parse", "HEAD^{tree}"], "");
    let head = run(&["rev-parse", "HEAD"], "");
    let at = (SESSION_START_MS + 10_800_000) / SECOND_MS;
    let object = format!(
        "tree {tree}\nparent {head}\nauthor user01 <user@example.com> {at} +0000\ncommitter user01 <user@example.com> {at} +0000\ngpgsig -----BEGIN PGP SIGNATURE-----\n \n iQEzBAABCAAdFiEE\n -----END PGP SIGNATURE-----\n\nsigned since the start\n"
    );
    let signed = run(&["hash-object", "-t", "commit", "-w", "--stdin"], &object);
    run(&["update-ref", "HEAD", &signed], "");
}

/// R42, D-154: for each configuration a repository could carry to run a
/// program on a read, the meeting's reads run nothing, while the same
/// plain git command, the control, runs the marker program; with a
/// filter driver the diff covers the commits only and says why.
#[test]
fn no_configured_program_runs_on_receive() {
    let Some(git) = git_or_skip("no_configured_program_runs_on_receive") else {
        return;
    };
    for case in CONFIGURED {
        let repo = MeetingRepo::new(git.clone());
        let (program, ran) = marker_program(repo.dir.path());
        configure(&repo, case, &program);
        let read = repo.read();
        assert!(
            !ran.exists(),
            "{}: the meeting ran the marker program",
            case.name
        );
        let read = changes(read);
        if case.name == "filter driver" {
            assert!(read.diff_scope_note.is_some(), "{}", case.name);
            let diff = text(&read.diff.text);
            assert!(!diff.contains("+working tree"), "{}: {diff}", case.name);
            assert!(diff.contains("+first change"), "{}: {diff}", case.name);
        } else {
            assert_eq!(read.diff_scope_note, None, "{}", case.name);
        }
        // The control: plain git, the configuration in force, runs it.
        let _ = run_git(&repo.git, &repo.root, case.control);
        assert!(
            ran.exists(),
            "{}: plain git did not run the marker program",
            case.name
        );
    }
}

/// AD-84: a folder outside any work tree is said so, and a folder gone
/// since is unavailable.
#[test]
fn a_folder_outside_git_is_said() {
    let Some(git) = git_or_skip("a_folder_outside_git_is_said") else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let read = meeting_changes(&git, dir.path(), &since_iso(0), &git_env(), Instant::now());
    assert_eq!(read, GitRead::OutsideGit);
    let gone = meeting_changes(
        &git,
        &dir.path().join("gone"),
        &since_iso(0),
        &git_env(),
        Instant::now(),
    );
    assert!(matches!(gone, GitRead::Unavailable(_)), "{gone:?}");
}
