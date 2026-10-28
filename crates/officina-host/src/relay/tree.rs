//! The tree fingerprint of check and `after-check` runs (AD-33, D-22): what
//! the code looked like when the run started, so an `after-check` action
//! cannot apply code that changed since its check.
//!
//! SHA-256, through core's [`StreamHash`], over `HEAD` (or the [`NO_HEAD`]
//! marker in a work tree without a commit), `git diff HEAD --binary`, and the
//! files that diff cannot see, each hashed by content up to
//! [`UNTRACKED_CONTENT_LIMIT`] and by size beyond: every untracked file `git`
//! does not ignore, and every tracked file flagged `assume-unchanged` or
//! `skip-worktree`, whose edits `git diff` hides. Without a commit, every
//! file `git` lists is bound that way. The whole work tree binds, whichever
//! folder of it the project is.
//!
//! Every `git` call disables what a repository's configuration could run or
//! vary (external diff, textconv, fsmonitor, the untracked cache, optional
//! locks) and drops the caller's `GIT_*` variables, which could point it at
//! another repository. Known limits of this guardrail (PRD section 1): clean
//! and smudge filters a repository configures still run when `git` compares
//! a modified file; a tracked edit whose size and modification time are put
//! back escapes `git`'s stat check; `git` is resolved through `PATH`.

use std::path::Path;
use std::process::{Command, Stdio};

use officina_core::relay::StreamHash;
use officina_core::relay::journal::TreeFingerprint;

/// The program the fingerprint runs, resolved through `PATH`.
pub const GIT: &str = "git";
/// Files up to this size are hashed by content, larger ones by size only
/// (D-22).
pub const UNTRACKED_CONTENT_LIMIT: u64 = 1024 * 1024;
/// What stands for `HEAD` in a work tree with no commit yet, which stays
/// bound (D-22).
pub const NO_HEAD: &str = "(no HEAD)";
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
/// Options of the diff: no external diff program, no textconv driver.
pub const DIFF_ARGS: &[&str] = &["--no-ext-diff", "--no-textconv", "--binary"];
/// `git ls-files -v` tag of an untracked file.
const UNTRACKED_TAG: u8 = b'?';
/// `git ls-files -v` tag of a `skip-worktree` file; a lowercase tag marks an
/// `assume-unchanged` one.
const SKIP_WORKTREE_TAG: u8 = b'S';

/// Runs `git <args>` in `dir`; its exit code and stdout, or `None` when it
/// cannot start.
fn git(program: &Path, dir: &Path, args: &[&str]) -> Option<(Option<i32>, Vec<u8>)> {
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
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().ok()?;
    Some((output.status.code(), output.stdout))
}

/// How one listed file is bound.
enum Listed {
    Content(String),
    Size(u64),
    Unreadable,
}

fn bind(path: &Path) -> Listed {
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() && meta.len() > UNTRACKED_CONTENT_LIMIT => {
            Listed::Size(meta.len())
        }
        Ok(meta) if meta.is_file() => match std::fs::read(path) {
            Ok(bytes) => {
                let mut hash = StreamHash::new();
                hash.update(&bytes);
                Listed::Content(hash.hex())
            }
            Err(_) => Listed::Unreadable,
        },
        _ => Listed::Unreadable,
    }
}

/// Hashes one length-prefixed field, so no two inputs read alike.
fn field(hash: &mut StreamHash, name: &str, bytes: &[u8]) {
    hash.update(format!("{name} {}\n", bytes.len()).as_bytes());
    hash.update(bytes);
    hash.update(b"\n");
}

/// True when `git diff HEAD` does not show this `ls-files -v` entry's edits.
fn diff_cannot_see(tag: u8) -> bool {
    tag == UNTRACKED_TAG || tag == SKIP_WORKTREE_TAG || tag.is_ascii_lowercase()
}

/// The fingerprint of the work tree `dir` lies in, with `git` from `PATH`;
/// `None` outside a work tree or when `git` cannot run (D-22).
pub fn fingerprint(dir: &Path) -> Option<TreeFingerprint> {
    fingerprint_with(Path::new(GIT), dir)
}

/// [`fingerprint`] with `program` as `git`.
pub fn fingerprint_with(program: &Path, dir: &Path) -> Option<TreeFingerprint> {
    let (code, out) = git(
        program,
        dir,
        &[
            "rev-parse",
            "--is-inside-work-tree",
            "--show-cdup",
            "--verify",
            "-q",
            "HEAD",
        ],
    )?;
    let text = String::from_utf8_lossy(&out);
    let mut lines = text.lines();
    if lines.next() != Some("true") {
        return None;
    }
    // The way up to the top of the work tree, relative (empty at the top),
    // so it joins `dir` whatever path style `git` prints.
    let top = dir.join(lines.next()?);
    let head = match (code, lines.next()) {
        (Some(0), Some(head)) if !head.is_empty() => Some(head.to_owned()),
        (Some(1), None) => None,
        _ => return None,
    };

    let mut hash = StreamHash::new();
    hash.update(b"officina-tree 1\n");
    field(
        &mut hash,
        "head",
        head.as_deref().unwrap_or(NO_HEAD).as_bytes(),
    );
    if head.is_some() {
        let mut diff = vec!["diff"];
        diff.extend_from_slice(DIFF_ARGS);
        diff.push("HEAD");
        match git(program, dir, &diff)? {
            (Some(0), patch) => field(&mut hash, "diff", &patch),
            _ => return None,
        }
    }
    let (code, out) = git(
        program,
        dir,
        &[
            "ls-files",
            "-z",
            "-v",
            "--full-name",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
            ":/",
        ],
    )?;
    if code != Some(0) {
        return None;
    }
    // Each entry is a tag, a space and the path from the top of the work
    // tree. With a commit, the diff already binds the plain tracked files.
    let mut paths: Vec<&[u8]> = out
        .split(|b| *b == 0)
        .filter(|entry| entry.len() > 2 && entry[1] == b' ')
        .filter(|entry| head.is_none() || diff_cannot_see(entry[0]))
        .map(|entry| &entry[2..])
        .collect();
    paths.sort_unstable();
    paths.dedup();
    let mut untracked_by_size = 0u32;
    for path in paths {
        field(&mut hash, "file", path);
        let name = String::from_utf8_lossy(path);
        match bind(&top.join(name.as_ref())) {
            Listed::Content(sha256) => field(&mut hash, "content", sha256.as_bytes()),
            Listed::Size(len) => {
                untracked_by_size += 1;
                field(&mut hash, "size", len.to_string().as_bytes());
            }
            Listed::Unreadable => field(&mut hash, "unreadable", b""),
        }
    }
    Some(TreeFingerprint {
        sha256: hash.hex(),
        untracked_by_size,
    })
}
