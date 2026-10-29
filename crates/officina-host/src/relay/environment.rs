//! The environment of what the relay starts (D-173, D-175): an approved
//! action, and the `git` that lists a work tree's files. Both start from the
//! environment Windows builds from the registry for the user, never from
//! the caller's: a variable a session sets for one `officina run`
//! (`PATH`, `PYTHONPATH`, `NODE_OPTIONS`, `KUBECONFIG`, `HOME`) changes
//! neither what an approved program loads nor which ignore rules bind a
//! tree.

use std::process::Command;

/// The `PATH` a launch gets off Windows (Linux is v1): the system folders
/// only an administrator writes.
pub const PATH_OFF_WINDOWS: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// Where Windows keeps the machine's own variables, which only an
/// administrator writes.
pub const MACHINE_ENVIRONMENT_KEY: &str =
    r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";

/// The prefix of the fake tool's settings, which the tests of the
/// `officina` crate set on `officina run` for the programs it starts to read
/// (feature `test-seams` only, never in a shipped binary).
#[cfg(feature = "test-seams")]
pub const TEST_PASSED_PREFIX: &str = "FAKE_TOOL_";

/// The environment every launch starts from: the one Windows builds from
/// the registry for the user, as Explorer gives a new process, with
/// `ComSpec` the system folder's `cmd.exe` and `PATHEXT` the machine's,
/// since a user value of either is the user's, so a session's, to set.
/// Nothing comes from the caller's environment. When the block cannot be
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

/// Clears `command`'s environment and sets `base` ([`launch_env`]) in its
/// place; with `test-seams`, the fake tool's settings too.
pub fn start_from(command: &mut Command, base: &[(String, String)]) {
    command
        .env_clear()
        .envs(base.iter().map(|(name, value)| (name, value)));
    #[cfg(feature = "test-seams")]
    command.envs(
        std::env::vars_os()
            .filter(|(name, _)| name.to_string_lossy().starts_with(TEST_PASSED_PREFIX)),
    );
}

/// The value of `name` in `env`, compared without case as Windows names
/// are; `None` when absent or empty.
pub fn value_of<'e>(env: &'e [(String, String)], name: &str) -> Option<&'e str> {
    env.iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "a test inspects the environment start_from gives a command"
)]
mod tests {
    use super::*;

    /// AC 3 of story 2.14, D-175: the launch's environment is the
    /// registry's, its `PATH` holding the system folder, `ComSpec` the
    /// system `cmd.exe` and each of `ComSpec` and `PATHEXT` once.
    #[cfg(windows)]
    #[test]
    fn launch_env_is_the_registrys() {
        let vars = launch_env();
        for name in ["ComSpec", "PATHEXT", "PATH"] {
            let count = vars
                .iter()
                .filter(|(key, _)| key.eq_ignore_ascii_case(name))
                .count();
            assert!(count <= 1, "{name} once: {vars:?}");
        }
        let path = value_of(&vars, "PATH").expect("the registry's environment has a Path");
        let system_dir = crate::relay::home::system_dir().unwrap();
        let system_text = system_dir.to_string_lossy().to_lowercase();
        assert!(
            std::env::split_paths(path)
                .any(|dir| dir.to_string_lossy().to_lowercase() == system_text),
            "{path}"
        );
        assert_eq!(
            value_of(&vars, "ComSpec").map(str::to_owned),
            Some(system_dir.join("cmd.exe").display().to_string())
        );
        let machine = crate::platform::windows::registry_string(
            crate::platform::windows::Hive::LocalMachine,
            MACHINE_ENVIRONMENT_KEY,
            "PATHEXT",
        );
        assert_eq!(
            value_of(&vars, "PATHEXT").map(str::to_owned),
            machine,
            "the machine's PATHEXT only"
        );
    }

    #[test]
    fn a_command_starts_from_the_base_alone() {
        let mut command = Command::new("program");
        command.env("SEEDED_BY_THE_CALLER", "x");
        start_from(&mut command, &[("PATH".to_owned(), "/usr/bin".to_owned())]);
        let names: Vec<String> = command
            .get_envs()
            .map(|(name, _)| name.to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["PATH"]);
        assert_eq!(
            value_of(&[("Path".to_owned(), String::new())], "PATH"),
            None,
            "empty is absent"
        );
    }
}
