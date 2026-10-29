//! Read-only `git` (AD-73, D-22, D-128, D-154, D-163): the one place that
//! starts `git`, for the tree fingerprint (AD-33) and the meeting (AD-84).
//! Every call carries [`GIT_GLOBAL_ARGS`] and [`OPTIONAL_LOCKS_VAR`]` = 0`,
//! so a read never writes the index back (R42), and opens no console
//! window. Which `git`, and the environment it gets, are the caller's
//! (`relay::tree::resolve_git` and `git_env`).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use officina_core::meeting::{DIFF_CAP_BYTES, GitRead, LOG_FORMAT, UNTRACKED_LISTED};

use crate::process::{JobRule, RunError, RunLimits, run_contained};

/// Options before every `git` subcommand: no pager, no optional lock (no
/// index refresh written back), no fsmonitor hook, no untracked cache.
pub const GIT_GLOBAL_ARGS: &[&str] = &[
    "--no-pager",
    "--no-optional-locks",
    "-c",
    "core.fsmonitor=false",
    "-c",
    "core.untrackedCache=false",
];

/// Set to `0`, `git` takes no optional lock, so a command that would
/// refresh the index (`status`, `diff`) leaves it as it was. The same as
/// `--no-optional-locks`, kept in the environment too so a `git` a call
/// starts in turn inherits it.
pub const OPTIONAL_LOCKS_VAR: &str = "GIT_OPTIONAL_LOCKS";

#[expect(
    clippy::disallowed_methods,
    reason = "host::git is where every read-only git call starts (AD-73)"
)]
fn git_command(program: &Path) -> Command {
    Command::new(program)
}

/// The command `git <args>` in `dir`: [`GIT_GLOBAL_ARGS`] first, an
/// environment cleared then set to `env` and [`OPTIONAL_LOCKS_VAR`]` = 0`,
/// stdin null, stdout piped, stderr dropped, no console window.
pub fn command(program: &Path, dir: &Path, args: &[&str], env: &[(String, String)]) -> Command {
    let mut command = git_command(program);
    command
        .args(GIT_GLOBAL_ARGS)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    crate::relay::environment::start_from(&mut command, env);
    command.env(OPTIONAL_LOCKS_VAR, "0");
    crate::process::hide_console(&mut command);
    command
}

/// Runs [`command`]; its exit code and stdout, or why it could not start.
pub fn output(
    program: &Path,
    dir: &Path,
    args: &[&str],
    env: &[(String, String)],
) -> Result<(Option<i32>, Vec<u8>), String> {
    let output = command(program, dir, args, env)
        .output()
        .map_err(|e| e.to_string())?;
    Ok((output.status.code(), output.stdout))
}

/// How long the meeting's `git` reads may take in all (AD-84).
pub const MEETING_DEADLINE: Duration = Duration::from_secs(10);

/// The most commits a meeting lists: a session's work, not a history.
pub const MEETING_COMMITS: usize = 200;

/// The most `git log` output a meeting reads.
const LOG_CAP_BYTES: usize = 1024 * 1024;

/// The most `ls-files --others` output a meeting reads.
const UNTRACKED_CAP_BYTES: usize = 256 * 1024;

/// Options of every `log` and `diff` the meeting runs (D-22, D-154): no
/// external diff program, no textconv filter, no colour, and for `log` no
/// signature check, so neither `diff.external`, a `textconv` driver,
/// `log.showSignature` with `gpg.program` nor a pager runs anything the
/// repository configures. Fsmonitor is off through [`GIT_GLOBAL_ARGS`].
pub const MEETING_DIFF_ARGS: &[&str] = &["--no-ext-diff", "--no-textconv", "--no-color"];

/// What one capped `git` call gave.
struct Ran {
    code: Option<i32>,
    stdout: Vec<u8>,
    over: bool,
}

/// Runs `git <args>` in `dir` like [`command`], its tree in a job ended
/// with it, `stdin` written when given, stdout kept within `cap`, until
/// `deadline`.
fn run_until(
    program: &Path,
    dir: &Path,
    args: &[&str],
    env: &[(String, String)],
    stdin: Option<Vec<u8>>,
    cap: usize,
    deadline: Instant,
) -> Result<Ran, String> {
    let timeout = deadline.saturating_duration_since(Instant::now());
    if timeout.is_zero() {
        return Err(timed_out());
    }
    let limits = RunLimits {
        timeout,
        stdout_cap: cap,
        stderr_cap: 1024,
        grace: Duration::from_secs(1),
        job: JobRule::KillOnClose,
        keep_held_output: false,
    };
    match run_contained(command(program, dir, args, env), stdin, &limits) {
        Ok(ran) => Ok(Ran {
            code: ran.status.code(),
            stdout: ran.stdout,
            over: ran.stdout_over,
        }),
        Err(RunError::Timeout(_)) => Err(timed_out()),
        Err(error) => Err(format!("git {error}")),
    }
}

fn timed_out() -> String {
    format!("git did not answer within {} s", MEETING_DEADLINE.as_secs())
}

/// The hashes of `git log` output in [`LOG_FORMAT`], newest first.
fn logged_hashes(log: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(log)
        .split('\u{1e}')
        .filter_map(|record| {
            let hash = record.split('\u{1f}').next()?.trim();
            (!hash.is_empty()).then(|| hash.to_owned())
        })
        .collect()
}

/// The meeting's reads of the work tree `dir` lies in, for the commits
/// since `since` (ISO 8601 UTC), within [`MEETING_DEADLINE`] from
/// `started` (AD-84, D-154). Read-only: every call carries
/// [`GIT_GLOBAL_ARGS`] and `GIT_OPTIONAL_LOCKS=0`, `log` and `diff` carry
/// [`MEETING_DIFF_ARGS`] and `log` `--no-show-signature`. Before the
/// working-tree diff, `config --local --get-regexp ^filter\.` (a read,
/// which runs nothing) says whether a filter driver is configured; then
/// the diff covers the commits only, since diffing the working tree would
/// run its clean filter. Untracked files are listed and stat'ed, never
/// read.
pub fn meeting_changes(
    program: &Path,
    dir: &Path,
    since: &str,
    env: &[(String, String)],
    started: Instant,
) -> GitRead {
    match read_changes(program, dir, since, env, started + MEETING_DEADLINE) {
        Ok(read) => read,
        Err(reason) => GitRead::Unavailable(reason),
    }
}

fn read_changes(
    program: &Path,
    dir: &Path,
    since: &str,
    env: &[(String, String)],
    deadline: Instant,
) -> Result<GitRead, String> {
    if !dir.is_dir() {
        return Err(format!("the folder {} no longer exists", dir.display()));
    }
    let run = |args: &[&str], cap: usize| run_until(program, dir, args, env, None, cap, deadline);
    let inside = run(
        &["rev-parse", "--is-inside-work-tree", "--show-toplevel"],
        64 * 1024,
    )?;
    let text = String::from_utf8_lossy(&inside.stdout).into_owned();
    let mut lines = text.lines();
    if inside.code != Some(0) || lines.next() != Some("true") {
        return if crate::relay::tree::looks_like_a_work_tree(dir) {
            Err("git cannot read the work tree this folder lies in".to_owned())
        } else {
            Ok(GitRead::OutsideGit)
        };
    }
    let top = PathBuf::from(lines.next().unwrap_or_default());
    let since_arg = format!("--since={since}");
    let count_arg = format!("--max-count={MEETING_COMMITS}");
    let mut log_args = vec!["log", "--no-show-signature"];
    log_args.extend(MEETING_DIFF_ARGS);
    log_args.extend([
        since_arg.as_str(),
        count_arg.as_str(),
        "--name-status",
        LOG_FORMAT,
    ]);
    let log = run(&log_args, LOG_CAP_BYTES)?;
    // A repository without a commit has no HEAD: its log fails, and the
    // diff starts from the empty tree.
    let log_text = if log.code == Some(0) {
        log.stdout
    } else {
        Vec::new()
    };
    let has_head = run(&["rev-parse", "--verify", "--quiet", "HEAD"], 1024)?.code == Some(0);
    let oldest_parent = match logged_hashes(&log_text).last() {
        Some(oldest) => {
            let parent = format!("{oldest}^");
            let found = run(&["rev-parse", "--verify", "--quiet", &parent], 1024)?;
            (found.code == Some(0))
                .then(|| String::from_utf8_lossy(&found.stdout).trim().to_owned())
        }
        None if has_head => Some("HEAD".to_owned()),
        None => None,
    };
    let base = match oldest_parent {
        Some(base) => base,
        None => {
            // `hash-object` without `-w` computes the empty tree's id in
            // the repository's own hash and writes nothing.
            let empty = run_until(
                program,
                dir,
                &["hash-object", "-t", "tree", "--stdin"],
                env,
                Some(Vec::new()),
                1024,
                deadline,
            )?;
            if empty.code != Some(0) {
                return Err("git could not name the empty tree".to_owned());
            }
            String::from_utf8_lossy(&empty.stdout).trim().to_owned()
        }
    };
    let filters = run(
        &["config", "--local", "--get-regexp", "^filter\\."],
        64 * 1024,
    )?;
    let filter_driver = filters.code == Some(0) && !filters.stdout.is_empty();
    let mut diff_args = vec!["diff"];
    diff_args.extend(MEETING_DIFF_ARGS);
    diff_args.push(base.as_str());
    let diff = if filter_driver && !has_head {
        Ran {
            code: Some(0),
            stdout: Vec::new(),
            over: false,
        }
    } else {
        if filter_driver {
            diff_args.push("HEAD");
        }
        run(&diff_args, DIFF_CAP_BYTES)?
    };
    if diff.code != Some(0) && !diff.over {
        return Err(format!(
            "git diff exited with {}",
            diff.code
                .map_or_else(|| "no code".to_owned(), |c| c.to_string())
        ));
    }
    let listed = run_until(
        program,
        &top,
        &["ls-files", "-z", "--others", "--exclude-standard"],
        env,
        None,
        UNTRACKED_CAP_BYTES,
        deadline,
    )?;
    let names: Vec<String> = listed
        .stdout
        .split(|b| *b == 0)
        .filter(|name| !name.is_empty())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect();
    let untracked_more = listed.over || names.len() > UNTRACKED_LISTED;
    let untracked = names
        .into_iter()
        .take(UNTRACKED_LISTED)
        .map(|name| {
            let bytes = std::fs::symlink_metadata(top.join(&name))
                .ok()
                .map(|meta| meta.len());
            (name, bytes)
        })
        .collect();
    Ok(GitRead::Read {
        log: log_text,
        diff: diff.stdout,
        diff_over: diff.over,
        filter_driver,
        untracked,
        untracked_more,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_call_carries_the_flags_and_the_lock_variable() {
        let env = vec![("PATH".to_owned(), "C:/bin".to_owned())];
        for args in [&["status", "--porcelain"][..], &["ls-files", "-z"][..]] {
            let built = command(
                Path::new("C:/git/cmd/git.exe"),
                Path::new("C:/repo"),
                args,
                &env,
            );
            let given: Vec<String> = built
                .get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            assert_eq!(&given[..GIT_GLOBAL_ARGS.len()], GIT_GLOBAL_ARGS, "{args:?}");
            assert_eq!(&given[GIT_GLOBAL_ARGS.len()..], args);
            let envs: Vec<(String, Option<String>)> = built
                .get_envs()
                .map(|(k, v)| {
                    (
                        k.to_string_lossy().into_owned(),
                        v.map(|v| v.to_string_lossy().into_owned()),
                    )
                })
                .collect();
            assert!(
                envs.contains(&(OPTIONAL_LOCKS_VAR.to_owned(), Some("0".to_owned()))),
                "{envs:?}"
            );
            assert!(envs.contains(&("PATH".to_owned(), Some("C:/bin".to_owned()))));
            assert_eq!(built.get_current_dir(), Some(Path::new("C:/repo")));
        }
    }
}
