//! Starting a process action (AD-30, AD-31): its command, the job it runs
//! in, its deadline, and the one ending every way out of it goes through.

use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use officina_core::relay::argv::Launch;
use officina_core::relay::journal::RunOutcome;
use officina_core::relay::run::{EXIT_NOT_STARTED, EXIT_TIMEOUT, RunPlan};

use super::OUTPUT_GRACE;
use super::journaling::{end_run, process_finished};
use super::output::{Output, no_streams};
use crate::process::{Waited, hide_console, wait_until};
use crate::relay::home::RelayHome;

/// The `PATH` a launch gets off Windows (Linux is v1): the system folders
/// only an administrator writes.
pub const PATH_OFF_WINDOWS: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// Where Windows keeps the machine's own variables, which only an
/// administrator writes.
pub const MACHINE_ENVIRONMENT_KEY: &str =
    r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";

/// The prefix of the fake tool's settings, which the tests of the
/// `officina` crate set on `officina run` for the action to read (feature
/// `test-seams` only, never in a shipped binary).
#[cfg(feature = "test-seams")]
pub const TEST_PASSED_PREFIX: &str = "FAKE_TOOL_";

/// The environment every launch starts from (D-173, D-175): the one Windows
/// builds from the registry for the user, as Explorer gives a new process,
/// with `ComSpec` the system folder's `cmd.exe` and `PATHEXT` the
/// machine's, since a user value of either is the user's, so a session's,
/// to set. Nothing comes from the caller's environment: a variable a session
/// sets for one `officina run` (`PYTHONPATH`, `NODE_OPTIONS`, `KUBECONFIG`)
/// never reaches the program the author approved. When the block cannot be
/// built: `PATH` the system and Windows folders, `SystemRoot` the Windows
/// folder, `ComSpec` the system `cmd.exe`. Off Windows (Linux is v1):
/// [`PATH_OFF_WINDOWS`] alone.
pub fn launch_env() -> Vec<(String, String)> {
    #[cfg(windows)]
    {
        use crate::platform::windows::{Hive, registry_environment, registry_string};
        let system = crate::relay::home::system_dir().unwrap_or_default();
        let windows_dir = system
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default();
        let mut vars = registry_environment().unwrap_or_else(|_| {
            vec![
                (
                    "PATH".to_owned(),
                    format!("{};{}", system.display(), windows_dir.display()),
                ),
                ("SystemRoot".to_owned(), windows_dir.display().to_string()),
            ]
        });
        vars.retain(|(name, _)| {
            !name.eq_ignore_ascii_case("ComSpec") && !name.eq_ignore_ascii_case("PATHEXT")
        });
        vars.push((
            "ComSpec".to_owned(),
            system.join("cmd.exe").display().to_string(),
        ));
        if let Some(pathext) =
            registry_string(Hive::LocalMachine, MACHINE_ENVIRONMENT_KEY, "PATHEXT")
        {
            vars.push(("PATHEXT".to_owned(), pathext));
        }
        vars
    }
    #[cfg(not(windows))]
    {
        vec![("PATH".to_owned(), PATH_OFF_WINDOWS.to_owned())]
    }
}

/// The command for a launch (AD-30): the program itself, its arguments as a
/// vector, the project folder as working directory, an environment cleared
/// then built from `base` ([`launch_env`]), the declared `env` added and
/// `env_remove` removed, stdin null, no console window.
pub fn command_for(launch: &Launch, base: &[(String, String)]) -> Command {
    let mut command = Command::new(&launch.program);
    command
        .args(&launch.args)
        .current_dir(&launch.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear()
        .envs(base.iter().map(|(name, value)| (name, value)));
    #[cfg(feature = "test-seams")]
    command.envs(
        std::env::vars_os()
            .filter(|(name, _)| name.to_string_lossy().starts_with(TEST_PASSED_PREFIX)),
    );
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

    /// AC 3 of story 2.14, D-175: the launch's environment is the
    /// registry's, its `PATH` holding the system folder, `ComSpec` the
    /// system `cmd.exe` and each of `ComSpec` and `PATHEXT` once.
    #[cfg(windows)]
    #[test]
    fn launch_env_is_the_registrys() {
        let vars = launch_env();
        let value = |name: &str| {
            let found: Vec<&String> = vars
                .iter()
                .filter(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value)
                .collect();
            assert!(found.len() <= 1, "{name} once: {found:?}");
            found.first().map(|v| (*v).clone())
        };
        let path = value("PATH").expect("the registry's environment has a Path");
        let system_dir = crate::relay::home::system_dir().unwrap();
        let system_text = system_dir.to_string_lossy().to_lowercase();
        assert!(
            std::env::split_paths(&path)
                .any(|dir| dir.to_string_lossy().to_lowercase() == system_text),
            "{path}"
        );
        assert_eq!(
            value("ComSpec"),
            Some(system_dir.join("cmd.exe").display().to_string())
        );
        let machine = crate::platform::windows::registry_string(
            crate::platform::windows::Hive::LocalMachine,
            MACHINE_ENVIRONMENT_KEY,
            "PATHEXT",
        );
        assert_eq!(value("PATHEXT"), machine, "the machine's PATHEXT only");
    }

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
