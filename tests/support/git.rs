//! `git` for the tests that build work trees (D-22, D-23, D-173): the one
//! on this process's `PATH`, run with a fixed identity, branch name and no
//! line ending conversion. A test without it skips with a printed reason.
//! Included by `#[path]` from the host and CLI test crates.

#![allow(dead_code)] // each test crate uses a different subset

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

pub const GIT_EXE: &str = if cfg!(windows) { "git.exe" } else { "git" };

/// The folder holding `git` on this process's `PATH`, `None` without one.
pub fn git_dir() -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?).find(|dir| dir.join(GIT_EXE).is_file())
}

/// [`git_dir`], or `None` after printing why `test` skips (D-23).
pub fn git_dir_or_skip(test: &str) -> Option<PathBuf> {
    let dir = git_dir();
    if dir.is_none() {
        println!("skipped {test}: no git on this machine's PATH");
    }
    dir
}

/// The `git` program of [`git_dir_or_skip`].
pub fn git_or_skip(test: &str) -> Option<PathBuf> {
    git_dir_or_skip(test).map(|dir| dir.join(GIT_EXE))
}

/// Runs the `git` program `git` with `args` in `dir`; its output.
pub fn run_git(git: &Path, dir: &Path, args: &[&str]) -> Output {
    Command::new(git)
        .args([
            "-c",
            "user.name=user01",
            "-c",
            "user.email=user@example.com",
            "-c",
            "core.autocrlf=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

/// [`run_git`], asserting it succeeds.
pub fn git_ok(git: &Path, dir: &Path, args: &[&str]) {
    let out = run_git(git, dir, args);
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
