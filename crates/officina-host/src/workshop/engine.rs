//! The engine driver (AD-98, D-204, D-616, D-623): which `docker.exe`, and
//! the one function that turns a core-built [`EngineCall`] into a process.
//! The launch half (every kind that creates, starts, copies, builds or
//! removes) runs only in a build with `window-authority`; `officina.exe`
//! keeps the read half for `stop --all`, `workshop --json` and doctor.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use officina_core::workshop::engine::{
    EngineCall, EngineCallKind, STDERR_CAP, STDOUT_CAP, engine_env,
};

use crate::process::{JobRule, RunError, RunLimits, run_contained};

/// The override of the `docker` binary, for development and tests (AD-7).
pub const ENV_DOCKER_BIN: &str = "OFFICINA_DOCKER_BIN";
/// Where Docker Desktop records its binaries (AD-98).
pub const DOCKER_KEY: &str = "SOFTWARE\\Docker Inc.\\Docker\\1.0";
/// The value under [`DOCKER_KEY`] naming the folder of `docker.exe`; the
/// key's version values are never read (addendum section 1 item 3).
pub const DOCKER_BIN_VALUE: &str = "BinPath";

/// Where a resolved binary came from (AD-98, AD-111), as doctor shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinarySource {
    Override,
    Hklm,
    Path,
}

impl BinarySource {
    pub fn text(self) -> &'static str {
        match self {
            BinarySource::Override => "override",
            BinarySource::Hklm => "hklm",
            BinarySource::Path => "path",
        }
    }
}

/// A resolved `docker` binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDocker {
    path: PathBuf,
    source: BinarySource,
}

impl ResolvedDocker {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn source(&self) -> BinarySource {
        self.source
    }

    /// A binary named directly, for tests and the gate probes.
    pub fn at(path: PathBuf, source: BinarySource) -> ResolvedDocker {
        ResolvedDocker { path, source }
    }
}

/// Why no `docker` could be resolved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    #[error("{0} is not a native .exe: a shim is refused (AD-51)")]
    NotExe(String),
    #[error("{name} was not found ({looked})")]
    NotFound { name: &'static str, looked: String },
}

/// True when `path` may be started: on Windows a file ending in `.exe`, any
/// case; elsewhere any file.
pub fn is_native(path: &Path) -> bool {
    if cfg!(windows) {
        path.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
    } else {
        true
    }
}

/// The first `name` on `path_var`'s folders that is a file.
pub fn on_path(name: &str, path_var: Option<&OsStr>) -> Option<PathBuf> {
    std::env::split_paths(path_var?)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// Resolution order of AD-98 and AD-111: the override, else the HKLM
/// folder joined with `exe`, else `exe` on `PATH`. On Windows only a file
/// ending in `.exe` is accepted; off Windows (CI) the override or `PATH`.
/// Inputs are arguments, so tests inject the registry and the environment.
pub fn resolve_binary(
    name: &'static str,
    override_value: Option<&OsStr>,
    hklm_path: Option<PathBuf>,
    path_var: Option<&OsStr>,
) -> Result<(PathBuf, BinarySource), ResolveError> {
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    let check = |path: PathBuf, source| {
        if is_native(&path) {
            Ok((path, source))
        } else {
            Err(ResolveError::NotExe(path.display().to_string()))
        }
    };
    if let Some(value) = override_value.filter(|v| !v.is_empty()) {
        return check(PathBuf::from(value), BinarySource::Override);
    }
    if let Some(path) = hklm_path.filter(|p| p.is_file()) {
        return check(path, BinarySource::Hklm);
    }
    match on_path(&exe, path_var) {
        Some(path) => check(path, BinarySource::Path),
        None => Err(ResolveError::NotFound {
            name,
            looked: "override, HKLM, PATH".to_owned(),
        }),
    }
}

/// The `docker.exe` Docker Desktop's HKLM key names, if it says one.
fn hklm_docker() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use crate::platform::windows::{Hive, registry_string};
        registry_string(Hive::LocalMachine, DOCKER_KEY, DOCKER_BIN_VALUE)
            .map(|bin| Path::new(&bin).join("docker.exe"))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Resolves `docker` from this process's environment and the registry.
pub fn resolve() -> Result<ResolvedDocker, ResolveError> {
    let (path, source) = resolve_binary(
        "docker",
        std::env::var_os(ENV_DOCKER_BIN).as_deref(),
        hklm_docker(),
        std::env::var_os("PATH").as_deref(),
    )?;
    Ok(ResolvedDocker { path, source })
}

/// What one call printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineOutcome {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    /// Over [`STDOUT_CAP`]: the rest was dropped.
    pub stdout_over: bool,
    /// The first [`STDERR_CAP`] bytes.
    pub stderr: String,
}

impl EngineOutcome {
    pub fn succeeded(&self) -> bool {
        self.code == Some(0)
    }
}

/// Why a call gave no outcome.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("docker: {0}")]
    Run(#[from] RunError),
    /// A follower or D-218's stream, which run through their own readers
    /// (stories 6.12 and 6.13), never through [`execute`].
    #[error("{} runs through its own bounded reader, not execute", .0.code())]
    NotCaptured(EngineCallKind),
    /// Built without `window-authority`: only the window launches.
    #[error("{} changes the engine, which only the window does; this build cannot", .0.code())]
    NoAuthority(EngineCallKind),
    #[cfg(feature = "window-authority")]
    #[error(transparent)]
    Launch(LaunchError),
}

/// A launch kind's failure, as the window's authority reports it (D-9).
#[cfg(feature = "window-authority")]
#[derive(Debug, thiserror::Error)]
#[error("{}: {}: {cause}", crate::relay::authority::AUTHORITY_MARKER, .kind.code())]
pub struct LaunchError {
    pub kind: EngineCallKind,
    pub cause: Box<EngineError>,
}

/// The grace for a child's pipes after it exits.
const GRACE: Duration = Duration::from_millis(500);

/// Runs `call` with `docker`, its environment built from `base` by core's
/// rule (D-616), in `home`, stdin fed `stdin` for an attached start that
/// writes it (AD-98, D-623).
pub fn execute(
    docker: &ResolvedDocker,
    call: &EngineCall,
    base: &[(String, String)],
    home: &Path,
    stdin: Option<Vec<u8>>,
) -> Result<EngineOutcome, EngineError> {
    if call.kind().is_launch() {
        return launch(docker, call, base, home, stdin);
    }
    run(docker, call, base, home, stdin)
}

#[cfg(feature = "window-authority")]
fn launch(
    docker: &ResolvedDocker,
    call: &EngineCall,
    base: &[(String, String)],
    home: &Path,
    stdin: Option<Vec<u8>>,
) -> Result<EngineOutcome, EngineError> {
    run(docker, call, base, home, stdin).map_err(|cause| {
        EngineError::Launch(LaunchError {
            kind: call.kind(),
            cause: Box::new(cause),
        })
    })
}

#[cfg(not(feature = "window-authority"))]
fn launch(
    _docker: &ResolvedDocker,
    call: &EngineCall,
    _base: &[(String, String)],
    _home: &Path,
    _stdin: Option<Vec<u8>>,
) -> Result<EngineOutcome, EngineError> {
    Err(EngineError::NoAuthority(call.kind()))
}

#[expect(
    clippy::disallowed_methods,
    reason = "workshop::engine is where every docker process starts (AD-73, D-204)"
)]
fn docker_command(docker: &ResolvedDocker) -> Command {
    Command::new(docker.path())
}

fn run(
    docker: &ResolvedDocker,
    call: &EngineCall,
    base: &[(String, String)],
    home: &Path,
    stdin: Option<Vec<u8>>,
) -> Result<EngineOutcome, EngineError> {
    let Some(deadline_ms) = call.deadline_ms() else {
        return Err(EngineError::NotCaptured(call.kind()));
    };
    if call.kind() == EngineCallKind::CpOut {
        return Err(EngineError::NotCaptured(call.kind()));
    }
    let mut command = docker_command(docker);
    command
        .args(call.args())
        .current_dir(home)
        .env_clear()
        .envs(engine_env(call.kind(), base));
    crate::process::hide_console(&mut command);
    let limits = RunLimits {
        timeout: Duration::from_millis(deadline_ms),
        stdout_cap: STDOUT_CAP,
        stderr_cap: STDERR_CAP,
        grace: GRACE,
        job: JobRule::KillOnClose,
        keep_held_output: false,
    };
    let stdin = if call.writes_stdin() { stdin } else { None };
    let ran = run_contained(command, stdin, &limits)?;
    Ok(EngineOutcome {
        code: ran.status.code(),
        stdout: ran.stdout,
        stdout_over: ran.stdout_over,
        stderr: String::from_utf8_lossy(&ran.stderr).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"").unwrap();
        path
    }

    #[test]
    fn the_override_wins() {
        let dir = tempfile::tempdir().unwrap();
        let exe = if cfg!(windows) {
            "docker.exe"
        } else {
            "docker"
        };
        let over = file(dir.path(), &format!("over-{exe}"));
        let hklm = file(dir.path(), &format!("hklm-{exe}"));
        let path_dir = dir.path().join("bin");
        std::fs::create_dir(&path_dir).unwrap();
        file(&path_dir, exe);
        let path_var = path_dir.as_os_str();
        let (found, source) = resolve_binary(
            "docker",
            Some(over.as_os_str()),
            Some(hklm.clone()),
            Some(path_var),
        )
        .unwrap();
        assert_eq!((found, source), (over, BinarySource::Override));
        let (found, source) =
            resolve_binary("docker", None, Some(hklm.clone()), Some(path_var)).unwrap();
        assert_eq!((found, source), (hklm, BinarySource::Hklm));
        let (found, source) = resolve_binary("docker", None, None, Some(path_var)).unwrap();
        assert_eq!((found, source), (path_dir.join(exe), BinarySource::Path));
        assert!(matches!(
            resolve_binary("docker", None, None, None),
            Err(ResolveError::NotFound { .. })
        ));
    }

    #[cfg(windows)]
    #[test]
    fn a_cmd_shim_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["docker.cmd", "docker.bat", "docker.ps1"] {
            let shim = file(dir.path(), name);
            assert!(matches!(
                resolve_binary("docker", Some(shim.as_os_str()), None, None),
                Err(ResolveError::NotExe(_))
            ));
        }
        let upper = file(dir.path(), "DOCKER.EXE");
        assert!(resolve_binary("docker", Some(upper.as_os_str()), None, None).is_ok());
    }

    #[test]
    fn the_source_is_reported() {
        for (source, text) in [
            (BinarySource::Override, "override"),
            (BinarySource::Hklm, "hklm"),
            (BinarySource::Path, "path"),
        ] {
            assert_eq!(source.text(), text);
        }
    }

    #[test]
    fn followers_and_streams_are_not_captured() {
        let dir = tempfile::tempdir().unwrap();
        let docker = ResolvedDocker::at(dir.path().join("missing.exe"), BinarySource::Override);
        let name = officina_core::workshop::names::ObjectName::parse("officina-worker-0a").unwrap();
        let follower = execute(&docker, &EngineCall::logs(&name), &[], dir.path(), None);
        assert!(matches!(
            follower,
            Err(EngineError::NotCaptured(EngineCallKind::Logs))
        ));
    }

    /// AD-97, D-204, review F35: built without `window-authority`, every
    /// launch kind is refused before any process starts, with an error that
    /// carries no authority marker. The resolver points at a path that does
    /// not exist: had anything tried to start it, the error would be a
    /// spawn failure. Run alone by CI (`cargo test -p officina-host --lib
    /// workshop::engine`), since every other test build turns the feature
    /// on.
    #[cfg(not(feature = "window-authority"))]
    #[test]
    fn execute_refuses_every_launch_kind_without_the_feature() {
        use officina_core::workshop::engine::BuildInputs;
        use officina_core::workshop::names::ObjectName;
        let marker = include_str!("../relay/authority.rs")
            .lines()
            .find_map(|line| {
                line.strip_prefix("pub const AUTHORITY_MARKER: &str = \"")?
                    .strip_suffix("\";")
            })
            .expect("the marker's declaration");
        let dir = tempfile::tempdir().unwrap();
        let docker = ResolvedDocker::at(
            dir.path().join("missing-docker.exe"),
            BinarySource::Override,
        );
        let name = ObjectName::parse("officina-worker-0a").unwrap();
        let build = BuildInputs {
            tag: "t:local".to_owned(),
            context: ".".to_owned(),
            dockerfile: None,
            labels: Default::default(),
            build_args: Default::default(),
            builder: None,
        };
        let call_of = |kind: EngineCallKind| -> Option<EngineCall> {
            Some(match kind {
                EngineCallKind::Build => EngineCall::build(&build),
                EngineCallKind::NetworkCreate => {
                    EngineCall::network_create(&name, true, &Default::default())
                }
                EngineCallKind::NetworkConnect => EngineCall::network_connect(&name, &name),
                EngineCallKind::NetworkRm => EngineCall::network_rm(std::slice::from_ref(&name)),
                EngineCallKind::VolumeCreate => {
                    EngineCall::volume_create(&name, &Default::default())
                }
                EngineCallKind::VolumeRm => EngineCall::volume_rm(std::slice::from_ref(&name)),
                EngineCallKind::CpIn => EngineCall::cp_in(".", &name, "/work"),
                EngineCallKind::CpOut => EngineCall::cp_out(&name, "/work"),
                EngineCallKind::Start => EngineCall::start(&name),
                EngineCallKind::StartAttached => EngineCall::start_attached(
                    &name,
                    officina_core::workshop::vector::ContainerRole::Packer,
                    0,
                    false,
                ),
                EngineCallKind::Rm => EngineCall::rm(std::slice::from_ref(&name)),
                // `create` needs a vector only story 6.6's builders make; its
                // kind is launch like the others, which the core test of
                // `is_launch` proves.
                EngineCallKind::Create => return None,
                _ => return None,
            })
        };
        let launches: Vec<EngineCallKind> = EngineCallKind::ALL
            .iter()
            .copied()
            .filter(|k| k.is_launch())
            .collect();
        assert!(!launches.is_empty());
        for kind in launches {
            let Some(call) = call_of(kind) else { continue };
            let refused = execute(&docker, &call, &[], dir.path(), None).unwrap_err();
            assert!(
                matches!(refused, EngineError::NoAuthority(k) if k == kind),
                "{kind:?}: {refused:?}"
            );
            assert!(!refused.to_string().contains(marker), "{refused}");
        }
        // The control: a read kind does try the missing binary.
        let read = execute(&docker, &EngineCall::version(), &[], dir.path(), None).unwrap_err();
        assert!(
            matches!(read, EngineError::Run(RunError::Spawn(_))),
            "{read:?}"
        );
    }
}
