//! Read-only `git` (AD-73, D-22, D-128, D-163): the one place that starts
//! `git`, for the tree fingerprint (AD-33) and later the meeting (AD-84).
//! Every call carries [`GIT_GLOBAL_ARGS`] and [`OPTIONAL_LOCKS_VAR`]` = 0`,
//! so a read never writes the index back (R42), and opens no console
//! window. Which `git`, and the environment it gets, are the caller's
//! (`relay::tree::resolve_git` and `git_env`).

use std::path::Path;
use std::process::{Command, Stdio};

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
