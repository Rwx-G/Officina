//! The tree fingerprint (AD-33, D-22, D-173): what the files of a project's
//! git work tree held when a check, an `after-check` action or a `human`
//! action's request ran, so the run that follows cannot apply other code.
//!
//! `git` only lists the files: tracked and unignored ones, from the whole
//! work tree, whichever folder of it the project is. Officina then reads
//! every listed file and hashes its bytes itself, so neither `git`'s stat
//! cache (a content change whose size and modification time are put back)
//! nor a filter the repository configures (clean, smudge, textconv) can hide
//! a change. The fingerprint is SHA-256, through core's [`StreamHash`], over
//! each path, sorted, and the hash of its content, within [`TreeLimits`].
//!
//! `git` is the one Git for Windows' installer records in the registry
//! ([`resolve_git`]), never the caller's `PATH`, and runs with the caller's
//! `GIT_*` variables removed and fsmonitor and the untracked cache off.
//! Known limits of this guardrail (PRD section 1): files `git` ignores are
//! not bound, and the repository's own ignore rules decide which those are;
//! programs and collections outside the work tree are not bound.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use officina_core::relay::StreamHash;
use officina_core::relay::journal::TreeFingerprint;

/// Where Git for Windows records its install folder, under `HKLM`, then
/// `HKCU` (D-173).
pub const GIT_FOR_WINDOWS_KEY: &str = r"SOFTWARE\GitForWindows";
/// The value holding the install folder.
pub const GIT_INSTALL_PATH_VALUE: &str = "InstallPath";
/// The `git` under the install folder: the launcher of its `cmd` folder, an
/// `.exe`, which Git for Windows puts on `PATH` itself.
pub const GIT_UNDER_INSTALL: &str = r"cmd\git.exe";
/// Where `git` is taken from off Windows (Linux is v1): a system folder
/// only an administrator writes.
pub const GIT_OFF_WINDOWS: &str = "/usr/bin/git";

/// Options before every `git` subcommand: no pager, no index refresh
/// written back, no fsmonitor hook, no untracked cache.
pub const GIT_GLOBAL_ARGS: &[&str] = &[
    "--no-pager",
    "--no-optional-locks",
    "-c",
    "core.fsmonitor=false",
    "-c",
    "core.untrackedCache=false",
];
/// The listing of the files a fingerprint binds: tracked and unignored ones,
/// NUL-separated, paths from the top of the work tree, from anywhere in it.
pub const LS_FILES_ARGS: &[&str] = &[
    "ls-files",
    "-z",
    "--full-name",
    "--cached",
    "--others",
    "--exclude-standard",
    "--",
    ":/",
];
/// Variables removed from `git`'s environment besides `GIT_*`: another
/// global configuration could add ignore rules.
pub const GIT_REMOVED_ENV: &[&str] = &["XDG_CONFIG_HOME"];

/// How much a fingerprint reads before it refuses `tree_too_large` (D-173).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeLimits {
    pub file_bytes: u64,
    pub files: usize,
    pub total_bytes: u64,
}

/// The limits `officina run` and the window use: 64 MiB for one file,
/// 20,000 files, 512 MiB in all.
pub const TREE_LIMITS: TreeLimits = TreeLimits {
    file_bytes: 64 * 1024 * 1024,
    files: 20_000,
    total_bytes: 512 * 1024 * 1024,
};

/// Why a work tree has no fingerprint, when it is one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TreeError {
    /// The folder lies in a git work tree and no `git` is installed where
    /// [`resolve_git`] looks, or it cannot start.
    #[error("{0}")]
    GitUnavailable(String),
    /// Past one of the [`TreeLimits`].
    #[error("{0}")]
    TooLarge(String),
    /// A listed file exists and cannot be read: a process holding it locked
    /// could change it unseen.
    #[error("{0}")]
    Unreadable(String),
}

/// A work tree's fingerprint, `None` outside one, or why it has none.
pub type TreeResult = Result<Option<TreeFingerprint>, TreeError>;

/// What the tests of the `officina` crate set to point `git` resolution at
/// another program, or at none when empty (feature `test-seams` only).
#[cfg(feature = "test-seams")]
pub const ENV_TEST_GIT: &str = "OFFICINA_TEST_GIT";

/// The `git` the fingerprint runs (D-173): on Windows `cmd\git.exe` under
/// the `InstallPath` Git for Windows records in `HKLM`, else in `HKCU`, when
/// that file exists; off Windows [`GIT_OFF_WINDOWS`]. Never looked up on
/// `PATH`. `None` when not installed there.
pub fn resolve_git() -> Option<PathBuf> {
    #[cfg(feature = "test-seams")]
    if let Some(value) = std::env::var_os(ENV_TEST_GIT) {
        return Some(PathBuf::from(value)).filter(|path| path.is_file());
    }
    resolve_installed_git()
}

#[cfg(windows)]
fn resolve_installed_git() -> Option<PathBuf> {
    use crate::platform::windows::{Hive, registry_string};
    [Hive::LocalMachine, Hive::CurrentUser]
        .into_iter()
        .filter_map(|hive| registry_string(hive, GIT_FOR_WINDOWS_KEY, GIT_INSTALL_PATH_VALUE))
        .map(|install| Path::new(&install).join(GIT_UNDER_INSTALL))
        .find(|git| git.is_file())
}

#[cfg(not(windows))]
fn resolve_installed_git() -> Option<PathBuf> {
    Some(PathBuf::from(GIT_OFF_WINDOWS)).filter(|path| path.is_file())
}

/// Runs `git <args>` in `dir`; its exit code and stdout, or the reason it
/// could not start.
fn git(program: &Path, dir: &Path, args: &[&str]) -> Result<(Option<i32>, Vec<u8>), String> {
    let mut command = Command::new(program);
    command
        .args(GIT_GLOBAL_ARGS)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for (key, _) in std::env::vars_os() {
        if key
            .to_str()
            .is_some_and(|k| k.to_ascii_uppercase().starts_with("GIT_"))
        {
            command.env_remove(key);
        }
    }
    for key in GIT_REMOVED_ENV {
        command.env_remove(key);
    }
    crate::process::hide_console(&mut command);
    let output = command.output().map_err(|e| e.to_string())?;
    Ok((output.status.code(), output.stdout))
}

/// Hashes one length-prefixed field, so no two inputs read alike.
fn field(hash: &mut StreamHash, name: &str, bytes: &[u8]) {
    hash.update(format!("{name} {}\n", bytes.len()).as_bytes());
    hash.update(bytes);
    hash.update(b"\n");
}

/// True when `dir` or a folder above it holds a `.git` entry: the folder
/// lies in a git work tree even when no `git` can say so.
pub fn looks_like_a_work_tree(dir: &Path) -> bool {
    let absolute = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    absolute
        .ancestors()
        .any(|folder| folder.join(".git").exists())
}

/// The fingerprint of the work tree `dir` lies in, with the `git` of
/// [`resolve_git`] and [`TREE_LIMITS`]: `Ok(None)` outside a work tree.
/// Without that `git`, a folder that [`looks_like_a_work_tree`] cannot be
/// bound ([`TreeError::GitUnavailable`]); any other folder is outside.
pub fn fingerprint(dir: &Path) -> TreeResult {
    match resolve_git() {
        Some(git) => fingerprint_within(&git, dir, &TREE_LIMITS),
        None if looks_like_a_work_tree(dir) => Err(TreeError::GitUnavailable(format!(
            "no git is installed where Git for Windows records it ({GIT_FOR_WINDOWS_KEY} {GIT_INSTALL_PATH_VALUE})"
        ))),
        None => Ok(None),
    }
}

/// [`fingerprint`] with `program` as `git` and [`TREE_LIMITS`].
pub fn fingerprint_with(program: &Path, dir: &Path) -> TreeResult {
    fingerprint_within(program, dir, &TREE_LIMITS)
}

/// [`fingerprint`] with `program` as `git` and `limits`. A `git` that
/// cannot start is [`TreeError::GitUnavailable`]; one that says the folder
/// is no work tree, or fails in it, gives `Ok(None)`.
pub fn fingerprint_within(program: &Path, dir: &Path, limits: &TreeLimits) -> TreeResult {
    if !dir.is_dir() {
        return Ok(None);
    }
    let unavailable =
        |e: String| TreeError::GitUnavailable(format!("{} cannot start: {e}", program.display()));
    let (code, out) = git(
        program,
        dir,
        &["rev-parse", "--is-inside-work-tree", "--show-cdup"],
    )
    .map_err(unavailable)?;
    let text = String::from_utf8_lossy(&out);
    let mut lines = text.lines();
    if code != Some(0) || lines.next() != Some("true") {
        return Ok(None);
    }
    // The way up to the top of the work tree, relative (empty at the top),
    // so it joins `dir` whatever path style `git` prints.
    let top = dir.join(lines.next().unwrap_or_default());
    let (code, listing) = git(program, dir, LS_FILES_ARGS).map_err(unavailable)?;
    if code != Some(0) {
        return Ok(None);
    }
    let mut paths: Vec<&[u8]> = listing
        .split(|b| *b == 0)
        .filter(|path| !path.is_empty())
        .collect();
    paths.sort_unstable();
    paths.dedup();
    if paths.len() > limits.files {
        return Err(TreeError::TooLarge(format!(
            "{} files, more than the {} the fingerprint reads",
            paths.len(),
            limits.files
        )));
    }
    let files: Vec<(&[u8], PathBuf)> = paths
        .into_iter()
        .map(|path| (path, top.join(String::from_utf8_lossy(path).as_ref())))
        .collect();
    // Sizes first, so a tree past the limits refuses before anything is read.
    let mut total = 0u64;
    for (_, path) in &files {
        if let Ok(meta) = std::fs::metadata(path)
            && meta.is_file()
        {
            if meta.len() > limits.file_bytes {
                return Err(too_big(path, limits.file_bytes));
            }
            total += meta.len();
        }
    }
    if total > limits.total_bytes {
        return Err(TreeError::TooLarge(format!(
            "{total} bytes in its files, more than the {} the fingerprint reads",
            limits.total_bytes
        )));
    }

    let mut hash = StreamHash::new();
    hash.update(b"officina-tree 2\n");
    for (name, path) in &files {
        field(&mut hash, "file", name);
        match content_hash(path, limits.file_bytes)? {
            Content::Hashed(sha256) => field(&mut hash, "content", sha256.as_bytes()),
            Content::Missing => field(&mut hash, "missing", b""),
            Content::NotAFile => field(&mut hash, "not-a-file", b""),
        }
    }
    Ok(Some(TreeFingerprint {
        sha256: hash.hex(),
        files: u32::try_from(files.len()).unwrap_or(u32::MAX),
        git: Some(program.to_string_lossy().into_owned()),
    }))
}

fn too_big(path: &Path, limit: u64) -> TreeError {
    TreeError::TooLarge(format!(
        "{} is over the {} bytes the fingerprint reads in one file",
        path.display(),
        limit
    ))
}

/// What one listed path holds.
enum Content {
    Hashed(String),
    /// A tracked file deleted from the work tree.
    Missing,
    /// A folder (a submodule) or anything else that is not a file.
    NotAFile,
}

/// Streams `path` through SHA-256, refusing past `limit` bytes, which a
/// file growing since its size was read can reach.
fn content_hash(path: &Path, limit: u64) -> Result<Content, TreeError> {
    let unreadable = |e: std::io::Error| TreeError::Unreadable(format!("{}: {e}", path.display()));
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Content::Missing),
        Err(e) => return Err(unreadable(e)),
    };
    if !meta.is_file() {
        return Ok(Content::NotAFile);
    }
    let file = std::fs::File::open(path).map_err(unreadable)?;
    let mut reader = file.take(limit + 1);
    let mut hash = StreamHash::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut read = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(unreadable)?;
        if n == 0 {
            break;
        }
        read += n as u64;
        if read > limit {
            return Err(too_big(path, limit));
        }
        hash.update(&buf[..n]);
    }
    Ok(Content::Hashed(hash.hex()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `git` from this process's `PATH`, for building test repositories;
    /// `None` after printing why `test` skips (D-23).
    fn git_on_path(test: &str) -> Option<PathBuf> {
        let exe = if cfg!(windows) { "git.exe" } else { "git" };
        let found = std::env::var_os("PATH").and_then(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join(exe))
                .find(|candidate| candidate.is_file())
        });
        if found.is_none() {
            println!("skipped {test}: no git on this machine's PATH");
        }
        found
    }

    fn run_git(git: &Path, dir: &Path, args: &[&str]) -> std::process::Output {
        Command::new(git)
            .args([
                "-c",
                "user.name=user01",
                "-c",
                "user.email=user@example.com",
                "-c",
                "init.defaultBranch=main",
                "-c",
                "core.autocrlf=false",
            ])
            .args(args)
            .current_dir(dir)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }

    fn git_ok(git: &Path, dir: &Path, args: &[&str]) {
        let out = run_git(git, dir, args);
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A temporary work tree with one commit holding `playbook.yml`.
    struct Repo {
        _dir: tempfile::TempDir,
        root: PathBuf,
        git: PathBuf,
    }

    impl Repo {
        fn new(git: PathBuf) -> Repo {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().join("Supervision");
            std::fs::create_dir_all(&root).unwrap();
            git_ok(&git, &root, &["init", "-q"]);
            git_ok(&git, &root, &["config", "core.autocrlf", "false"]);
            std::fs::write(root.join("playbook.yml"), b"- hosts: lab\n").unwrap();
            git_ok(&git, &root, &["add", "playbook.yml"]);
            git_ok(&git, &root, &["commit", "-q", "-m", "first"]);
            Repo {
                _dir: dir,
                root,
                git,
            }
        }

        fn fingerprint(&self) -> TreeFingerprint {
            fingerprint_with(&self.git, &self.root)
                .unwrap()
                .expect("a work tree has a fingerprint")
        }
    }

    /// AC 2: a content change whose size and modification time are put back
    /// escapes `git`'s stat check; the fingerprint reads the bytes.
    #[test]
    fn restored_size_and_mtime_still_change_the_fingerprint() {
        let Some(git) = git_on_path("restored_size_and_mtime_still_change_the_fingerprint") else {
            return;
        };
        let repo = Repo::new(git);
        let playbook = repo.root.join("playbook.yml");
        let set_modified = |at: std::time::SystemTime| {
            std::fs::File::options()
                .write(true)
                .open(&playbook)
                .unwrap()
                .set_modified(at)
                .unwrap();
        };
        // An hour back, and recorded in the index now, so git does not see
        // the file as racily clean and compares its stat only.
        let earlier = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        set_modified(earlier);
        git_ok(&repo.git, &repo.root, &["update-index", "--refresh"]);
        let before = repo.fingerprint();
        std::fs::write(&playbook, b"- hosts: all\n").unwrap();
        set_modified(earlier);
        let status = run_git(&repo.git, &repo.root, &["status", "--porcelain"]);
        assert!(
            status.stdout.is_empty(),
            "positive control: git's stat cache misses the edit"
        );
        assert_ne!(before, repo.fingerprint());
    }

    /// AC 2: a clean filter the repository configures makes `git diff` see
    /// nothing; the fingerprint never runs a filter and reads the bytes.
    #[test]
    fn a_clean_filter_does_not_hide_a_change() {
        let Some(git) = git_on_path("a_clean_filter_does_not_hide_a_change") else {
            return;
        };
        let repo = Repo::new(git);
        let marker = repo.root.parent().unwrap().join("filter-ran.txt");
        let marker_text = marker.to_string_lossy().replace('\\', "/");
        std::fs::write(repo.root.join(".gitattributes"), b"*.yml filter=hide\n").unwrap();
        git_ok(&repo.git, &repo.root, &["add", ".gitattributes"]);
        git_ok(&repo.git, &repo.root, &["commit", "-q", "-m", "attributes"]);
        // The clean filter prints the committed content whatever the file
        // holds, and leaves a marker when it runs.
        git_ok(
            &repo.git,
            &repo.root,
            &[
                "config",
                "filter.hide.clean",
                &format!(
                    "cat > /dev/null; echo ran >> '{marker_text}'; git show HEAD:playbook.yml"
                ),
            ],
        );
        let before = repo.fingerprint();
        std::fs::write(repo.root.join("playbook.yml"), b"- hosts: everything\n").unwrap();
        let after = repo.fingerprint();
        assert!(!marker.exists(), "the fingerprint ran the clean filter");
        assert_ne!(before, after);
        let diff = run_git(&repo.git, &repo.root, &["diff", "HEAD"]);
        assert!(
            diff.stdout.is_empty(),
            "positive control: the filter hides the edit from git diff"
        );
        assert!(marker.exists(), "positive control: the filter is armed");
    }

    /// AC 2: past a limit, the fingerprint refuses rather than binding less.
    #[test]
    fn limits_refuse_tree_too_large() {
        let Some(git) = git_on_path("limits_refuse_tree_too_large") else {
            return;
        };
        let repo = Repo::new(git);
        let big = repo.root.join("dump.bin");
        let file = std::fs::File::create(&big).unwrap();
        file.set_len(TREE_LIMITS.file_bytes + 1).unwrap();
        drop(file);
        assert!(matches!(
            fingerprint_with(&repo.git, &repo.root),
            Err(TreeError::TooLarge(why)) if why.contains("dump.bin")
        ));
        std::fs::write(&big, b"sixteen bytes..\n").unwrap();
        let file_at = |file_bytes: u64| TreeLimits {
            file_bytes,
            ..TREE_LIMITS
        };
        let at_limit = fingerprint_within(&repo.git, &repo.root, &file_at(16))
            .unwrap()
            .unwrap();
        assert_eq!(at_limit.files, 2, "the playbook and the dump");
        assert!(matches!(
            fingerprint_within(&repo.git, &repo.root, &file_at(15)),
            Err(TreeError::TooLarge(why)) if why.contains("dump.bin")
        ));
        let files = TreeLimits {
            files: 1,
            ..TREE_LIMITS
        };
        assert!(matches!(
            fingerprint_within(&repo.git, &repo.root, &files),
            Err(TreeError::TooLarge(why)) if why.contains("2 files")
        ));
        let total = TreeLimits {
            total_bytes: 8,
            ..TREE_LIMITS
        };
        assert!(matches!(
            fingerprint_within(&repo.git, &repo.root, &total),
            Err(TreeError::TooLarge(why)) if why.contains("bytes in its files")
        ));
    }

    #[test]
    fn a_locked_file_is_unreadable_not_skipped() {
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let Some(git) = git_on_path("a_locked_file_is_unreadable_not_skipped") else {
                return;
            };
            let repo = Repo::new(git);
            let playbook = repo.root.join("playbook.yml");
            // No sharing: the holder could change the file through its own
            // handle while every reader is turned away.
            let _held = std::fs::File::options()
                .read(true)
                .write(true)
                .share_mode(0)
                .open(&playbook)
                .unwrap();
            assert!(matches!(
                fingerprint_with(&repo.git, &repo.root),
                Err(TreeError::Unreadable(why)) if why.contains("playbook.yml")
            ));
        }
    }

    #[test]
    fn a_deleted_tracked_file_is_bound_as_missing() {
        let Some(git) = git_on_path("a_deleted_tracked_file_is_bound_as_missing") else {
            return;
        };
        let repo = Repo::new(git);
        let before = repo.fingerprint();
        std::fs::remove_file(repo.root.join("playbook.yml")).unwrap();
        let missing = repo.fingerprint();
        assert_ne!(before, missing);
        assert_eq!(missing.files, 1, "still listed by the index");
    }

    #[test]
    fn without_git_only_a_work_tree_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("Notes");
        std::fs::create_dir_all(plain.join("sub")).unwrap();
        assert!(!looks_like_a_work_tree(&plain.join("sub")));
        std::fs::create_dir_all(plain.join(".git")).unwrap();
        assert!(looks_like_a_work_tree(&plain.join("sub")));
        let absent = dir.path().join("no-such-git.exe");
        assert!(matches!(
            fingerprint_with(&absent, &plain),
            Err(TreeError::GitUnavailable(why)) if why.contains("cannot start")
        ));
    }

    /// The `InstallPath` Git for Windows records, read by `reg.exe` rather
    /// than by the code under test.
    #[cfg(windows)]
    fn install_path_by_reg_exe() -> Option<PathBuf> {
        let system = crate::relay::home::system_dir()?;
        for hive in ["HKLM", "HKCU"] {
            let out = Command::new(system.join("reg.exe"))
                .args([
                    "query",
                    &format!(r"{hive}\{GIT_FOR_WINDOWS_KEY}"),
                    "/v",
                    GIT_INSTALL_PATH_VALUE,
                ])
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .ok()?;
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(found) = text.lines().find_map(|line| {
                let (_, rest) = line.split_once("REG_SZ")?;
                Some(PathBuf::from(rest.trim()))
            }) {
                return Some(found);
            }
        }
        None
    }

    /// AC 1: `git` comes from the install folder the registry names, and the
    /// same one whatever `PATH` holds.
    #[cfg(windows)]
    #[test]
    fn git_is_resolved_from_the_registry_not_path() {
        let Some(install) = install_path_by_reg_exe() else {
            println!("skipped: Git for Windows records no InstallPath on this machine");
            return;
        };
        let resolved = resolve_installed_git().expect("the recorded git exists");
        assert_eq!(resolved, install.join(GIT_UNDER_INSTALL));
        assert!(
            resolved
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        );
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "relay::tree::tests::resolve_child",
                "--nocapture",
            ])
            .env("OFFICINA_TREE_RESOLVE_CHILD", "1")
            .env("PATH", "")
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&child.stdout);
        assert!(child.status.success(), "{text}");
        let line = text
            .lines()
            .find_map(|l| l.strip_prefix("resolved="))
            .unwrap_or_else(|| panic!("{text}"));
        assert_eq!(Path::new(line), resolved, "an empty PATH changes nothing");
    }

    /// Run by `git_is_resolved_from_the_registry_not_path` with an empty
    /// `PATH`; does nothing otherwise.
    #[test]
    fn resolve_child() {
        if std::env::var_os("OFFICINA_TREE_RESOLVE_CHILD").is_none() {
            return;
        }
        let resolved = resolve_installed_git().map(|p| p.display().to_string());
        println!("resolved={}", resolved.unwrap_or_default());
    }

    /// The fake tool of the `officina` crate, next to the test executables
    /// when the workspace's tests are built; it appends a line to
    /// `FAKE_TOOL_LOG` when it runs.
    #[cfg(windows)]
    fn fake_tool() -> Option<PathBuf> {
        let deps = std::env::current_exe().ok()?.parent()?.to_path_buf();
        Some(deps.parent()?.join("officina-fake-tool.exe")).filter(|path| path.is_file())
    }

    /// AC 1, the audits' first chain: a session puts a `git.exe` first on
    /// `PATH` for one `officina run`. The fingerprint never runs it: its
    /// marker stays absent, and the fingerprint is the registry git's.
    #[cfg(windows)]
    #[test]
    fn a_shim_first_on_path_is_not_used() {
        let Some(registry_git) = resolve_installed_git() else {
            println!("skipped: Git for Windows records no InstallPath on this machine");
            return;
        };
        let Some(tool) = fake_tool() else {
            println!(
                "skipped: officina-fake-tool.exe is not built (cargo test --workspace builds it)"
            );
            return;
        };
        let repo = Repo::new(registry_git.clone());
        let shim = tempfile::tempdir().unwrap();
        std::fs::copy(&tool, shim.path().join("git.exe")).unwrap();
        let marker = shim.path().join("shim-ran.log");
        let path = std::env::join_paths(std::iter::once(shim.path().to_path_buf()).chain(
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
        ))
        .unwrap();
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "relay::tree::tests::shim_child", "--nocapture"])
            .env("OFFICINA_TREE_SHIM_REPO", &repo.root)
            .env("FAKE_TOOL_LOG", &marker)
            .env("PATH", &path)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&child.stdout);
        assert!(child.status.success(), "{text}");
        assert!(!marker.exists(), "the shim on PATH ran");
        let line = text
            .lines()
            .find_map(|l| l.strip_prefix("fingerprint="))
            .unwrap_or_else(|| panic!("{text}"));
        assert_eq!(line, repo.fingerprint().sha256);

        // Positive control: a lookup through that PATH finds the shim.
        let control = Command::new("git")
            .arg("--version")
            .env("FAKE_TOOL_LOG", &marker)
            .env("PATH", &path)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(control.status.success());
        assert!(marker.exists(), "the shim is armed");
    }

    /// Run by `a_shim_first_on_path_is_not_used` with the shim first on
    /// `PATH`; does nothing otherwise.
    #[test]
    fn shim_child() {
        let Some(repo) = std::env::var_os("OFFICINA_TREE_SHIM_REPO") else {
            return;
        };
        let found = fingerprint(Path::new(&repo)).unwrap().unwrap();
        println!("fingerprint={}", found.sha256);
    }
}
