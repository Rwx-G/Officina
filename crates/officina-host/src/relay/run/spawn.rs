//! Starting a process action (AD-30, AD-31): its command, the job it runs
//! in, its deadline, and the one ending every way out of it goes through.
//! Also the command of doctor's WSL probe (story 2.10), which runs a
//! catalogue program's path the way an action would: `relay::run` is where
//! a catalogue program starts (AD-73, D-128).

use std::ffi::OsStr;
use std::io::Write;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use officina_core::relay::argv::{Launch, WSL_REMOVED_ENV};
use officina_core::relay::journal::RunOutcome;
use officina_core::relay::run::{EXIT_NOT_STARTED, EXIT_TIMEOUT, RunPlan};

use super::OUTPUT_GRACE;
use super::journaling::{end_run, process_finished};
use super::output::{Output, no_streams};
use crate::process::{Waited, hide_console, wait_until};
use crate::relay::doctor::WSL_TEST_PROGRAM;
use crate::relay::environment::{launch_env, start_from};
use crate::relay::home::RelayHome;

#[expect(
    clippy::disallowed_methods,
    reason = "relay::run is where a catalogue program starts (AD-73)"
)]
fn program_command(program: &OsStr) -> Command {
    Command::new(program)
}

/// The command probing `path` in `distro` (the default one when `None`):
/// `wsl.exe [-d <distro>] --exec /usr/bin/test -x <path>`, never through a
/// shell, stdio null, no console window, the environment `officina run`
/// gives `wsl.exe` (D-6).
pub fn wsl_probe_command(wsl: &Path, distro: Option<&str>, path: &str) -> Command {
    let mut command = program_command(wsl.as_os_str());
    if let Some(distro) = distro {
        command.args(["-d", distro]);
    }
    command
        .args(["--exec", WSL_TEST_PROGRAM, "-x", path])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for key in WSL_REMOVED_ENV {
        command.env_remove(key);
    }
    hide_console(&mut command);
    command
}

/// The command for a launch (AD-30): the program itself, its arguments as a
/// vector, the project folder as working directory, an environment cleared
/// then built from `base` ([`launch_env`], D-175), the declared `env` added
/// and `env_remove` removed, stdin null, no console window.
pub fn command_for(launch: &Launch, base: &[(String, String)]) -> Command {
    let mut command = program_command(OsStr::new(&launch.program));
    command
        .args(&launch.args)
        .current_dir(&launch.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    start_from(&mut command, base);
    command.envs(&launch.env);
    for key in &launch.env_remove {
        command.env_remove(key);
    }
    hide_console(&mut command);
    command
}

/// The process tree of a run (AD-31). On Windows `officina run` puts itself
/// in a kill-on-close job before it starts anything, so every process of the
/// action is in the job from its start and dies with `officina run`, however
/// it ends. If that assignment fails, the child is assigned right after its
/// start instead. Elsewhere the child alone is killed (Linux is v1).
struct ProcessTree {
    #[cfg(windows)]
    job: Option<crate::platform::windows::Job>,
    #[cfg(windows)]
    contains_self: bool,
}

impl ProcessTree {
    fn new() -> ProcessTree {
        #[cfg(windows)]
        {
            let job = crate::platform::windows::Job::new(true).ok();
            let contains_self = job
                .as_ref()
                .is_some_and(|job| job.assign_current_process().is_ok());
            ProcessTree { job, contains_self }
        }
        #[cfg(not(windows))]
        {
            ProcessTree {}
        }
    }

    fn started(&self, child: &Child) {
        #[cfg(windows)]
        if !self.contains_self
            && let Some(job) = &self.job
        {
            let _ = job.assign(child);
        }
        #[cfg(not(windows))]
        let _ = child;
    }

    /// Ends the whole tree with `code`, `officina run` included when it is
    /// in the job; returns only if it is not.
    fn end(&self, child: &mut Child, code: i32) {
        self.exit_with(code);
        let _ = child.kill();
        let _ = child.wait();
    }

    /// Ends whatever the action left running, and `officina run` itself
    /// when it is in the job, with `code`. A plain exit would let the
    /// kill-on-close job end the exiting process a second time and replace
    /// the program's exit code with 0 (found by `exit_code_passes_through`).
    /// The bits of a negative code are the full Windows value.
    fn exit_with(&self, code: i32) {
        #[cfg(windows)]
        if let Some(job) = &self.job {
            let _ = job.terminate(code as u32);
        }
        #[cfg(not(windows))]
        let _ = code;
    }
}

/// Starts a process action in its process tree, copies its output, waits
/// for it against its deadline, and ends the run. Every way out (exit,
/// timeout, a wait that fails) goes through one ending: the cut marker
/// after the output it closes, then [`end_run`]; the tree is ended last,
/// since ending it can end `officina run` too.
pub(super) fn spawn(
    home: &RelayHome,
    plan: &RunPlan,
    launch: &Launch,
    run_id: &str,
    err: &mut dyn Write,
) -> i32 {
    let tree = ProcessTree::new();
    let started = Instant::now();
    let mut child = match command_for(launch, &launch_env()).spawn() {
        Ok(child) => child,
        Err(e) => {
            let _ = writeln!(err, "officina: could not start {}: {e}", launch.program);
            let line = process_finished(
                run_id,
                RunOutcome::SpawnFailed,
                None,
                started,
                no_streams(),
                Some(e.to_string()),
            );
            end_run(home, err, run_id, line, None);
            tree.exit_with(EXIT_NOT_STARTED);
            return EXIT_NOT_STARTED;
        }
    };
    tree.started(&child);
    let output = Output::start(&mut child, plan.max_output_kib, plan.keeps_output);
    let end = |err: &mut dyn Write, outcome, exit_code, detail| {
        output.mark_cut();
        let line = process_finished(
            run_id,
            outcome,
            exit_code,
            started,
            output.streams(),
            detail,
        );
        end_run(home, err, run_id, line, output.kept());
    };
    let deadline = started + Duration::from_secs(u64::from(plan.timeout_s));
    match wait_until(&mut child, deadline) {
        Waited::Exited(status) => {
            output.drain(OUTPUT_GRACE);
            // Windows always reports a code; elsewhere a signal has none.
            let code = status.code().unwrap_or(1);
            end(err, RunOutcome::Exited, Some(i64::from(code)), None);
            tree.exit_with(code);
            code
        }
        Waited::Deadline => {
            end(err, RunOutcome::Timeout, None, None);
            let _ = writeln!(
                err,
                "officina: timeout: {} ran past its {} s; it and every process it started were stopped",
                plan.action, plan.timeout_s
            );
            tree.end(&mut child, EXIT_TIMEOUT);
            EXIT_TIMEOUT
        }
        Waited::Lost(e) => {
            end(err, RunOutcome::Exited, None, Some(e.to_string()));
            let _ = writeln!(err, "officina: lost track of {}: {e}", plan.action);
            tree.end(&mut child, 1);
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn wsl_command_removes_wslenv() {
        let launch = Launch {
            program: r"C:\Windows\System32\wsl.exe".to_owned(),
            args: vec!["--exec".to_owned(), "/usr/bin/env".to_owned()],
            cwd: "C:/path/to/Lab".to_owned(),
            env: BTreeMap::from([("PROBE_MODE".to_owned(), "read-only".to_owned())]),
            env_remove: officina_core::relay::argv::WSL_REMOVED_ENV
                .iter()
                .map(|k| (*k).to_owned())
                .collect(),
        };
        let base = vec![
            ("PATH".to_owned(), r"C:\Windows\System32".to_owned()),
            ("WSLENV".to_owned(), "USERPROFILE/p".to_owned()),
        ];
        let command = command_for(&launch, &base);
        let envs: Vec<(String, Option<String>)> = command
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        // A cleared environment lists only what it sets: a removed name is
        // simply absent.
        for removed in officina_core::relay::argv::WSL_REMOVED_ENV {
            assert!(
                !envs.iter().any(|(name, _)| name == removed),
                "{removed} reaches wsl.exe: {envs:?}"
            );
        }
        assert!(
            envs.contains(&("PATH".to_owned(), Some(r"C:\Windows\System32".to_owned()))),
            "the base PATH, not the caller's: {envs:?}"
        );
        assert!(
            envs.iter()
                .all(|(name, _)| name == "PATH" || name == "PROBE_MODE"),
            "only the base and the catalogue's env: {envs:?}"
        );
        assert!(envs.contains(&("PROBE_MODE".to_owned(), Some("read-only".to_owned()))));
        assert_eq!(
            command.get_current_dir(),
            Some(std::path::Path::new("C:/path/to/Lab"))
        );
        assert_eq!(command.get_args().count(), 2);
    }
}
