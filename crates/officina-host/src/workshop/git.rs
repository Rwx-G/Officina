//! The workshop's `git` (AD-111, D-238, D-623): which binary, and the rule
//! every call that touches a package or the project for the workshop runs
//! under. The process itself starts in `host::git` (the lint's site); this
//! module adds the rule's flags, its environment and its job.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::engine::{BinarySource, ResolveError, resolve_binary};
use crate::process::{Contained, JobRule, RunError, RunLimits, run_contained};
use crate::relay::tree::{
    GIT_FOR_WINDOWS_KEY, GIT_INSTALL_PATH_VALUE, GIT_REMOVED_ENV, GIT_UNDER_INSTALL,
};

/// The override of the workshop's `git`, for development and tests (AD-7).
pub const ENV_GIT_BIN: &str = "OFFICINA_GIT_BIN";
/// Each workshop git call's deadline (AD-111).
pub const DEADLINE: Duration = Duration::from_secs(60);
/// The most stdout a workshop git call keeps.
pub const STDOUT_CAP: usize = 8 * 1024 * 1024;
/// The stderr kept of a failed call.
pub const STDERR_CAP: usize = 4 * 1024;

/// The configuration every workshop call carries before its own arguments
/// (AD-111, D-238), after `host::git`'s read-only flags: no hook (the hooks
/// path is an empty folder), fsck on every transfer, local transfers only,
/// no background maintenance, no submodule, no signature check on `log`.
pub fn rule_args(hooks_dir: &Path) -> Vec<String> {
    let mut args = vec![
        "-c".to_owned(),
        format!(
            "core.hooksPath={}",
            hooks_dir.to_string_lossy().replace('\\', "/")
        ),
    ];
    for setting in [
        "transfer.fsckObjects=true",
        "fetch.fsckObjects=true",
        "protocol.allow=never",
        "protocol.file.allow=always",
        "gc.auto=0",
        "maintenance.auto=false",
        "fetch.writeCommitGraph=false",
        "fetch.recurseSubmodules=false",
        "submodule.recurse=false",
        "log.showSignature=false",
    ] {
        args.push("-c".to_owned());
        args.push(setting.to_owned());
    }
    args
}

/// The full argument vector of a workshop call: the rule, then `args`, with
/// `--no-show-signature` right after a `log` subcommand (D-238, D-154).
pub fn workshop_args(hooks_dir: &Path, args: &[&str]) -> Vec<String> {
    let mut all = rule_args(hooks_dir);
    for (index, arg) in args.iter().enumerate() {
        all.push((*arg).to_owned());
        if index == 0 && *arg == "log" {
            all.push("--no-show-signature".to_owned());
        }
    }
    all
}

/// `<home>/workshop/empty-hooks` (AD-124).
pub fn empty_hooks_dir(home: &Path) -> PathBuf {
    home.join("workshop").join("empty-hooks")
}

/// The hooks folder, created when absent; refused unless empty (AD-111), so
/// a hook planted there could never run under the rule.
pub fn ensure_empty_hooks(home: &Path) -> Result<PathBuf, String> {
    let dir = empty_hooks_dir(home);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if entries.next().is_some() {
        return Err(format!("{} is not empty", dir.display()));
    }
    Ok(dir)
}

/// The installed `git.exe` Git for Windows records in HKLM, if any.
fn hklm_git() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use crate::platform::windows::{Hive, registry_string};
        registry_string(
            Hive::LocalMachine,
            GIT_FOR_WINDOWS_KEY,
            GIT_INSTALL_PATH_VALUE,
        )
        .map(|install| Path::new(&install).join(GIT_UNDER_INSTALL))
    }
    #[cfg(not(windows))]
    {
        let _ = (
            GIT_FOR_WINDOWS_KEY,
            GIT_INSTALL_PATH_VALUE,
            GIT_UNDER_INSTALL,
        );
        None
    }
}

/// The workshop's `git` (AD-111): the override, else HKLM's install, else
/// `git.exe` on `PATH`; `.exe` only on Windows.
pub fn resolve() -> Result<(PathBuf, BinarySource), ResolveError> {
    resolve_binary(
        "git",
        std::env::var_os(ENV_GIT_BIN).as_deref(),
        hklm_git(),
        std::env::var_os("PATH").as_deref(),
    )
}

/// `base` without the caller's `GIT_*` names and [`GIT_REMOVED_ENV`], which
/// would add configuration or move the repository (D-22).
pub fn workshop_env(base: &[(String, String)]) -> Vec<(String, String)> {
    base.iter()
        .filter(|(name, _)| {
            !name.to_ascii_uppercase().starts_with("GIT_")
                && !GIT_REMOVED_ENV.iter().any(|r| r.eq_ignore_ascii_case(name))
        })
        .cloned()
        .collect()
}

/// Runs `git <rule> <args>` in `dir` under the workshop rule: read-only
/// flags and `GIT_OPTIONAL_LOCKS=0` from `host::git`, the rule's flags, the
/// environment of [`workshop_env`], a kill-on-close job assigned as soon as
/// the child exists (D-238, D-623), [`DEADLINE`].
pub fn run(
    git: &Path,
    dir: &Path,
    args: &[&str],
    base: &[(String, String)],
    hooks_dir: &Path,
    stdin: Option<Vec<u8>>,
) -> Result<Contained, RunError> {
    let all = workshop_args(hooks_dir, args);
    let all: Vec<&str> = all.iter().map(String::as_str).collect();
    let command = crate::git::command(git, dir, &all, &workshop_env(base));
    let limits = RunLimits {
        timeout: DEADLINE,
        stdout_cap: STDOUT_CAP,
        stderr_cap: STDERR_CAP,
        grace: Duration::from_millis(500),
        job: JobRule::KillOnClose,
        keep_held_output: false,
    };
    run_contained(command, stdin, &limits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_workshop_rule_carries_every_flag() {
        let hooks = Path::new("C:/home/workshop/empty-hooks");
        let args = workshop_args(hooks, &["log", "--format=%H"]);
        let joined = args.join(" ");
        for flag in [
            "core.hooksPath=C:/home/workshop/empty-hooks",
            "transfer.fsckObjects=true",
            "fetch.fsckObjects=true",
            "protocol.allow=never",
            "protocol.file.allow=always",
            "gc.auto=0",
            "maintenance.auto=false",
            "fetch.writeCommitGraph=false",
            "fetch.recurseSubmodules=false",
            "submodule.recurse=false",
            "log.showSignature=false",
        ] {
            assert!(joined.contains(&format!("-c {flag}")), "{flag}");
        }
        let log = args.iter().position(|a| a == "log").unwrap();
        assert_eq!(args[log + 1], "--no-show-signature");
        assert_eq!(args.last().unwrap(), "--format=%H");
        let other = workshop_args(hooks, &["rev-parse", "log"]);
        assert!(!other.contains(&"--no-show-signature".to_owned()));
    }

    #[test]
    fn a_non_empty_hooks_dir_is_refused() {
        let home = tempfile::tempdir().unwrap();
        let dir = ensure_empty_hooks(home.path()).unwrap();
        assert_eq!(dir, empty_hooks_dir(home.path()));
        std::fs::write(dir.join("reference-transaction"), "#!/bin/sh\n").unwrap();
        assert!(ensure_empty_hooks(home.path()).is_err());
    }

    #[test]
    fn resolution_order_is_override_hklm_path() {
        let dir = tempfile::tempdir().unwrap();
        let exe = if cfg!(windows) { "git.exe" } else { "git" };
        let over = dir.path().join(format!("over-{exe}"));
        let hklm = dir.path().join(format!("hklm-{exe}"));
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        for file in [&over, &hklm, &bin.join(exe)] {
            std::fs::write(file, b"").unwrap();
        }
        let path_var = bin.as_os_str();
        assert_eq!(
            resolve_binary(
                "git",
                Some(over.as_os_str()),
                Some(hklm.clone()),
                Some(path_var)
            )
            .unwrap(),
            (over, BinarySource::Override)
        );
        assert_eq!(
            resolve_binary("git", None, Some(hklm.clone()), Some(path_var)).unwrap(),
            (hklm, BinarySource::Hklm)
        );
        assert_eq!(
            resolve_binary("git", None, None, Some(path_var)).unwrap(),
            (bin.join(exe), BinarySource::Path)
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_non_exe_git_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let shim = dir.path().join("git.cmd");
        std::fs::write(&shim, b"").unwrap();
        assert!(matches!(
            resolve_binary("git", Some(shim.as_os_str()), None, None),
            Err(ResolveError::NotExe(_))
        ));
    }

    #[test]
    fn the_callers_git_names_are_removed() {
        let base: Vec<(String, String)> = [
            "GIT_DIR",
            "git_config_global",
            "XDG_CONFIG_HOME",
            "PATH",
            "HOME",
        ]
        .iter()
        .map(|n| ((*n).to_owned(), "x".to_owned()))
        .collect();
        let names: Vec<String> = workshop_env(&base).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["PATH", "HOME"]);
    }
}
