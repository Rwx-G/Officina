//! The only code that creates a `claude` process (AD-73, AD-79, D-128 as
//! amended by D-561 and D-562): the resolution of the binary (AD-6),
//! moved here from `claude.rs`, and [`execute`], which turns a core
//! [`ClaudeVector`] into a process.
//!
//! Every kind refuses a binary that is not a native executable before
//! anything starts; resolution still finds a shim, so doctor and the board
//! say why. The environment is the vector's rule applied to a base the
//! caller gives ([`BaseEnv`], D-86), the process's own one cleared. A
//! captured kind runs through [`run_contained`] against its deadline and
//! caps, both streams drained past them (D-97), in the job its kind's
//! [`job_rule`] names; a console kind starts in a new console, is not
//! waited on, and gives back its pid and creation time.
//!
//! The launch half (kinds whose [`VectorKind::is_launch`] holds) compiles
//! only under `window-authority`, its error carrying the authority marker
//! (D-9); built without the feature, [`execute`] refuses a launch kind
//! before any process starts.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use officina_core::spawn::{
    AGENTS_DEADLINE_MS, ClaudeVector, Console, CwdRule, StdinRule, VectorKind, binary_extension,
};

use crate::paths::{ENV_CLAUDE_BIN, user_home};
pub use crate::process::JobRule;
use crate::process::{Contained, RunError, RunLimits, run_contained};

/// Deadline of one `claude agents` or `claude --version` call.
pub const AGENTS_TIMEOUT: Duration = Duration::from_millis(AGENTS_DEADLINE_MS);

/// How long the output pipes may stay open once `claude agents` or
/// `claude --version` has exited. Short, because the collector thread,
/// which builds the board, waits on it; only a process `claude` started and
/// left running keeps them open longer.
pub const OUTPUT_GRACE: Duration = Duration::from_millis(500);

/// How long the output pipes may stay open once a contained call exited
/// and its job was ended. Longer than [`OUTPUT_GRACE`]: the call runs on a
/// thread nothing waits on, and its tree is already ended, so the pipes
/// close as soon as the reader threads are scheduled.
pub const CONTAINED_GRACE: Duration = Duration::from_secs(2);

/// `CREATE_NO_WINDOW`: a captured kind shows no console.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// `CREATE_NEW_CONSOLE`: a console kind gets one of its own.
pub const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

/// A `claude` binary found by [`resolve_claude`] or named by a caller, with
/// the extension that decides whether Officina runs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedClaude {
    path: PathBuf,
    extension: Option<String>,
}

impl ResolvedClaude {
    pub fn new(path: PathBuf) -> ResolvedClaude {
        let extension = binary_extension(&path.to_string_lossy());
        ResolvedClaude { path, extension }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The file name's extension as Windows reads it, in lowercase.
    pub fn extension(&self) -> Option<&str> {
        self.extension.as_deref()
    }

    /// True when Officina runs it: a native executable
    /// ([`officina_core::spawn::is_native_binary`]).
    pub fn is_native(&self) -> bool {
        self.extension.as_deref().is_none_or(|e| e == "exe")
    }
}

/// Locates `claude`: `OFFICINA_CLAUDE_BIN`, then `PATH` (`claude.exe`, then
/// `claude.cmd` on Windows), then `~/.local/bin`, where the native installer
/// puts it (E8) and where a GUI started with a stale `PATH` still finds it.
/// A shim is found so doctor and the board can say why it is refused.
pub fn resolve_claude() -> Option<ResolvedClaude> {
    if let Some(explicit) = std::env::var_os(ENV_CLAUDE_BIN).filter(|v| !v.is_empty()) {
        return Some(ResolvedClaude::new(PathBuf::from(explicit)));
    }
    let names: &[&str] = if cfg!(windows) {
        &["claude.exe", "claude.cmd"]
    } else {
        &["claude"]
    };
    let path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    for name in names {
        // Relative PATH entries resolve against whatever directory Officina
        // was started from; they are skipped so a planted `claude.exe` there
        // is never run.
        if let Some(found) = path_dirs
            .iter()
            .filter(|d| d.is_absolute())
            .map(|d| d.join(name))
            .find(|p| p.is_file())
        {
            return Some(ResolvedClaude::new(found));
        }
    }
    let local = user_home()?.join(".local").join("bin").join(names[0]);
    local.is_file().then(|| ResolvedClaude::new(local))
}

/// The environment a vector's own is built from (D-86): the window's, read
/// once at its start, or one a test builds. No test sets a process
/// variable.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BaseEnv(Arc<Vec<(String, String)>>);

impl BaseEnv {
    pub fn new(vars: Vec<(String, String)>) -> BaseEnv {
        BaseEnv(Arc::new(vars))
    }

    /// This process's environment; a name or value that is not Unicode is
    /// left out.
    pub fn from_process() -> BaseEnv {
        BaseEnv::new(
            std::env::vars_os()
                .filter_map(|(name, value)| {
                    Some((name.into_string().ok()?, value.into_string().ok()?))
                })
                .collect(),
        )
    }

    pub fn vars(&self) -> &[(String, String)] {
        &self.0
    }
}

/// How a started kind ended.
#[derive(Debug)]
pub enum SpawnOutcome {
    /// A captured kind ran to its exit.
    Captured(Contained),
    /// A console kind started and was left to the author: its pid and
    /// creation time (100 ns ticks), which together name it.
    Started { pid: u32, created: Option<u64> },
}

/// Why a `claude` call produced no usable outcome.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SpawnError {
    #[error("claude not found (searched OFFICINA_CLAUDE_BIN, PATH, ~/.local/bin)")]
    NotFound,
    #[error(
        "claude resolves to {0}, which is not a native claude.exe, and Officina runs no other (a shim only cmd.exe runs, or another file type): install the native claude.exe or set OFFICINA_CLAUDE_BIN to it"
    )]
    NotNative(String),
    #[error("claude could not start: {0}")]
    Spawn(String),
    #[error(
        "claude could not be put in a job of its own, so it was stopped before anything was sent: {0}"
    )]
    NotContained(String),
    #[error("claude did not answer within {0} s")]
    Timeout(u64),
    #[error("lost track of claude: {0}")]
    Lost(String),
    #[error("claude exited but a process it started still holds its output")]
    OutputHeld,
    #[error("claude exited with {code}: {stderr}")]
    Failed { code: String, stderr: String },
    #[error("a console of its own is opened on Windows only")]
    Unsupported,
    /// Built without `window-authority`: only the window launches.
    #[error("{} launches work, which only the window does; this build cannot", .0.code())]
    NoAuthority(VectorKind),
    #[cfg(feature = "window-authority")]
    #[error(transparent)]
    Launch(LaunchError),
}

impl SpawnError {
    /// The error itself, past the launch half's wrapper.
    pub fn cause(&self) -> &SpawnError {
        match self {
            #[cfg(feature = "window-authority")]
            SpawnError::Launch(launch) => &launch.cause,
            other => other,
        }
    }
}

/// A launch kind's failure, as the window's authority reports it (D-9):
/// the marker lives in this type, compiled only into the window.
#[cfg(feature = "window-authority")]
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}: {}: {cause}", crate::relay::authority::AUTHORITY_MARKER, .kind.code())]
pub struct LaunchError {
    pub kind: VectorKind,
    pub cause: Box<SpawnError>,
}

/// The job rule of `kind` (D-128 review F10): the read calls end their
/// tree on a timeout, a contained call owns its tree, and a work order
/// and a console kind run in no job.
pub const fn job_rule(kind: VectorKind) -> JobRule {
    match kind {
        VectorKind::Agents | VectorKind::Version | VectorKind::Logs => JobRule::TreeOnTimeout,
        VectorKind::TriageCall | VectorKind::ReviewCall => JobRule::KillOnClose,
        VectorKind::Attach
        | VectorKind::WorkOrder
        | VectorKind::Orchestrator
        | VectorKind::Answer => JobRule::NoJob,
    }
}

/// True when `kind`'s run that exited 0 keeps what it read although a
/// process it started still holds the output pipes (D-128 review F10):
/// `claude --bg` may start the supervisor of every background session,
/// which inherits them; any other captured kind answered in full or not
/// at all.
pub const fn keeps_held_output(kind: VectorKind) -> bool {
    match kind {
        VectorKind::WorkOrder => true,
        VectorKind::Agents
        | VectorKind::Version
        | VectorKind::TriageCall
        | VectorKind::ReviewCall
        | VectorKind::Attach
        | VectorKind::Orchestrator
        | VectorKind::Logs
        | VectorKind::Answer => false,
    }
}

/// The creation flags of a process with `console`.
pub const fn creation_flags(console: Console) -> u32 {
    match console {
        Console::None => CREATE_NO_WINDOW,
        Console::New => CREATE_NEW_CONSOLE,
    }
}

/// Starts `vector` with `claude`, its environment built from `base`.
pub fn execute(
    claude: &ResolvedClaude,
    vector: &ClaudeVector,
    base: &BaseEnv,
) -> Result<SpawnOutcome, SpawnError> {
    if vector.kind().is_launch() {
        return launch(claude, vector, base);
    }
    start(claude, vector, base)
}

#[cfg(feature = "window-authority")]
fn launch(
    claude: &ResolvedClaude,
    vector: &ClaudeVector,
    base: &BaseEnv,
) -> Result<SpawnOutcome, SpawnError> {
    start(claude, vector, base).map_err(|cause| {
        SpawnError::Launch(LaunchError {
            kind: vector.kind(),
            cause: Box::new(cause),
        })
    })
}

#[cfg(not(feature = "window-authority"))]
fn launch(
    _claude: &ResolvedClaude,
    vector: &ClaudeVector,
    _base: &BaseEnv,
) -> Result<SpawnOutcome, SpawnError> {
    Err(SpawnError::NoAuthority(vector.kind()))
}

fn start(
    claude: &ResolvedClaude,
    vector: &ClaudeVector,
    base: &BaseEnv,
) -> Result<SpawnOutcome, SpawnError> {
    if !claude.is_native() {
        return Err(SpawnError::NotNative(claude.path().display().to_string()));
    }
    let env = vector.child_env(base.vars());
    let cwd = working_dir(claude, vector.cwd());
    match vector.console() {
        Console::New => start_in_console(claude, vector, &env, cwd.as_deref()),
        Console::None => run_captured(claude, vector, env, cwd.as_deref()),
    }
}

/// The folder a vector's [`CwdRule`] names; `None` keeps Officina's own.
fn working_dir(claude: &ResolvedClaude, rule: &CwdRule) -> Option<PathBuf> {
    match rule {
        CwdRule::Inherited => None,
        CwdRule::BinaryFolder => claude
            .path()
            .parent()
            .filter(|dir| !dir.as_os_str().is_empty())
            .map(Path::to_path_buf),
        CwdRule::Officina { home, dir } => Some(
            dir.segments()
                .iter()
                .fold(PathBuf::from(home), |path, segment| path.join(segment)),
        ),
        CwdRule::Project { folder } | CwdRule::SessionFolder { folder } => {
            Some(PathBuf::from(folder))
        }
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "the spawn module is where every claude process starts (AD-73)"
)]
fn claude_command(claude: &ResolvedClaude) -> Command {
    Command::new(claude.path())
}

fn run_captured(
    claude: &ResolvedClaude,
    vector: &ClaudeVector,
    env: Vec<(String, String)>,
    cwd: Option<&Path>,
) -> Result<SpawnOutcome, SpawnError> {
    let (Some(capture), Some(deadline_ms)) = (vector.capture(), vector.deadline_ms()) else {
        return Err(SpawnError::Spawn(format!(
            "a {} vector captures nothing",
            vector.kind().code()
        )));
    };
    let job = job_rule(vector.kind());
    let grace = match job {
        JobRule::TreeOnTimeout => OUTPUT_GRACE,
        JobRule::KillOnClose | JobRule::NoJob => CONTAINED_GRACE,
    };
    let timeout = Duration::from_millis(deadline_ms);
    let limits = RunLimits {
        timeout,
        stdout_cap: capture.stdout_cap,
        stderr_cap: capture.stderr_cap,
        grace,
        job,
        keep_held_output: keeps_held_output(vector.kind()),
    };
    let mut command = claude_command(claude);
    command.args(vector.args()).env_clear().envs(env);
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    let stdin = (vector.stdin() == StdinRule::Data).then(|| vector.stdin_data().to_vec());
    run_contained(command, stdin, &limits)
        .map(SpawnOutcome::Captured)
        .map_err(|error| match error {
            RunError::Timeout(_) => SpawnError::Timeout(timeout.as_secs()),
            RunError::OutputHeld => SpawnError::OutputHeld,
            RunError::Spawn(e) => SpawnError::Spawn(e.to_string()),
            RunError::NotContained(e) => SpawnError::NotContained(e.to_string()),
            RunError::Lost(e) => SpawnError::Lost(e.to_string()),
        })
}

#[cfg(windows)]
fn start_in_console(
    claude: &ResolvedClaude,
    vector: &ClaudeVector,
    env: &[(String, String)],
    cwd: Option<&Path>,
) -> Result<SpawnOutcome, SpawnError> {
    // `CreateProcess` adds no extension to the program it is given by
    // name, where the program search would add `.exe`.
    let program = match claude.extension() {
        Some(_) => claude.path().to_path_buf(),
        None => claude.path().with_extension("exe"),
    };
    #[expect(
        clippy::disallowed_methods,
        reason = "a console kind starts here, in the spawn module, as every claude process does (AD-73, D-963)"
    )]
    let started = crate::platform::windows::start_in_new_console(
        &program,
        vector.args(),
        cwd,
        env,
        creation_flags(vector.console()),
    );
    started
        .map(|(pid, created)| SpawnOutcome::Started { pid, created })
        .map_err(|e| SpawnError::Spawn(e.to_string()))
}

#[cfg(not(windows))]
fn start_in_console(
    _claude: &ResolvedClaude,
    _vector: &ClaudeVector,
    _env: &[(String, String)],
    _cwd: Option<&Path>,
) -> Result<SpawnOutcome, SpawnError> {
    Err(SpawnError::Unsupported)
}

/// The stdout of a captured run that exited 0.
fn answered(outcome: SpawnOutcome) -> Result<String, SpawnError> {
    match outcome {
        SpawnOutcome::Captured(ran) if ran.status.success() => {
            Ok(String::from_utf8_lossy(&ran.stdout).into_owned())
        }
        SpawnOutcome::Captured(ran) => Err(SpawnError::Failed {
            code: crate::process::exit_code_text(ran.status),
            stderr: ran.quoted_stderr(),
        }),
        SpawnOutcome::Started { .. } => Err(SpawnError::Spawn("it was not captured".into())),
    }
}

fn config_text(config_dir: Option<&Path>) -> Option<String> {
    config_dir.map(|dir| dir.to_string_lossy().into_owned())
}

/// `claude agents --json --all`: its stdout. Returns within about
/// [`AGENTS_TIMEOUT`] plus [`OUTPUT_GRACE`] whatever `claude` leaves
/// running.
pub fn agents_json(
    claude: &ResolvedClaude,
    config_dir: Option<&Path>,
    base: &BaseEnv,
) -> Result<String, SpawnError> {
    let vector = officina_core::spawn::agents(config_text(config_dir).as_deref());
    answered(execute(claude, &vector, base)?)
}

/// `claude logs <id>`: its stdout, at most
/// [`officina_core::spawn::LOGS_STDOUT_CAP`] of it, within about
/// [`officina_core::spawn::LOGS_DEADLINE_MS`] plus [`OUTPUT_GRACE`].
pub fn logs(
    claude: &ResolvedClaude,
    id: &officina_core::spawn::BackgroundId,
    config_dir: Option<&Path>,
    base: &BaseEnv,
) -> Result<Vec<u8>, SpawnError> {
    let vector = officina_core::spawn::logs(id, config_text(config_dir).as_deref());
    match execute(claude, &vector, base)? {
        SpawnOutcome::Captured(ran) if ran.status.success() => Ok(ran.stdout),
        SpawnOutcome::Captured(ran) => Err(SpawnError::Failed {
            code: crate::process::exit_code_text(ran.status),
            stderr: ran.quoted_stderr(),
        }),
        SpawnOutcome::Started { .. } => Err(SpawnError::Spawn("it was not captured".into())),
    }
}

/// `claude --version`, first line trimmed.
pub fn version(
    claude: &ResolvedClaude,
    config_dir: Option<&Path>,
    base: &BaseEnv,
) -> Result<String, SpawnError> {
    let vector = officina_core::spawn::version(config_text(config_dir).as_deref());
    answered(execute(claude, &vector, base)?)
        .map(|v| v.lines().next().unwrap_or_default().trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use officina_core::orders::work_order::WorkOrder;
    use officina_core::registry::triage_call::{TRIAGE_MODEL, schema};
    use officina_core::relay::catalogue::{ApprovedCatalogue, load};
    use officina_core::relay::template::{ApprovedTemplate, ParamName, ProjectName, TemplateName};
    use officina_core::spawn::BackgroundId;

    /// A vector of `kind`, through its builder.
    fn sample(kind: VectorKind) -> ClaudeVector {
        match kind {
            VectorKind::Agents => officina_core::spawn::agents(None),
            VectorKind::Version => officina_core::spawn::version(None),
            VectorKind::Attach => {
                officina_core::spawn::attach(&BackgroundId::parse("bg01").unwrap(), None)
            }
            VectorKind::TriageCall => {
                officina_core::spawn::triage_call(TRIAGE_MODEL, &schema(), None, "C:/path/to/home")
            }
            VectorKind::WorkOrder => officina_core::spawn::work_order(&order(), 1, None),
            VectorKind::ReviewCall => officina_core::spawn::review_call(
                officina_core::relay::template::ModelAlias::Haiku,
                &officina_core::relay::auto::schema(),
                None,
                "C:/path/to/home",
            ),
            VectorKind::Orchestrator => officina_core::spawn::orchestrator(
                &officina_core::spawn::McpServerInputs {
                    officina_exe: "C:/path/to/app/officina.exe",
                    officina_home: "C:/path/to/home",
                    floor: None,
                    no_excerpts: false,
                    claude_bin: "C:/path/to/bin/claude.exe",
                },
                None,
                None,
            ),
            VectorKind::Logs => {
                officina_core::spawn::logs(&BackgroundId::parse("bg01").unwrap(), None)
            }
            VectorKind::Answer => officina_core::spawn::answer(
                &officina_core::meeting::Answer::parse("Keep the 1.x line.").unwrap(),
                &officina_core::spawn::SessionId::parse("0f1e2d3c-4b5a-4978-8796-a5b4c3d2e1f0")
                    .unwrap(),
                "C:/path/to/repo",
                None,
            ),
        }
    }

    /// A validated order of a one-template floor.
    fn order() -> WorkOrder {
        let path = "C:/path/to/officina.toml";
        let floor = "schema = 2
[[project]]
name = \"backend\"
path = \"Backend\"
[[template]]
name = \"bump\"
description = \"Bump one dependency\"
projects = [\"backend\"]
prompt = \"Bump {package}.\"
[template.params.package]
values = [\"serde_json\"]
";
        let current = load(floor, path).unwrap();
        let approved = ApprovedCatalogue::approving(&current, path, 0);
        let name = TemplateName::parse("bump").unwrap();
        let template = ApprovedTemplate::new(&approved, &current, &name).unwrap();
        let values = [(ParamName::parse("package").unwrap(), "serde_json")];
        WorkOrder::validate(&template, &ProjectName::parse("backend").unwrap(), &values).unwrap()
    }

    #[test]
    fn creation_flags_follow_the_console_rule() {
        for kind in VectorKind::ALL.iter().copied() {
            let vector = sample(kind);
            let flags = creation_flags(vector.console());
            match vector.console() {
                Console::New => {
                    assert_eq!(flags, CREATE_NEW_CONSOLE, "{kind:?}");
                    assert_eq!(vector.capture(), None, "{kind:?}: not waited on");
                }
                Console::None => {
                    assert_eq!(flags, CREATE_NO_WINDOW, "{kind:?}");
                    assert!(vector.capture().is_some(), "{kind:?}: captured");
                }
            }
        }
        #[cfg(windows)]
        {
            assert_eq!(
                CREATE_NO_WINDOW,
                windows_sys::Win32::System::Threading::CREATE_NO_WINDOW
            );
            assert_eq!(
                CREATE_NEW_CONSOLE,
                windows_sys::Win32::System::Threading::CREATE_NEW_CONSOLE
            );
        }
    }

    #[test]
    fn the_job_rule_names_every_kind() {
        for kind in VectorKind::ALL.iter().copied() {
            let vector = sample(kind);
            let rule = job_rule(kind);
            // A console kind is the author's: no job. A contained call
            // owns its tree; the read calls end theirs on a timeout only;
            // a launch for the author (`claude --bg`) may start the
            // supervisor of every background session: no job either.
            let expected = match (vector.console(), vector.env_rule(), kind.is_launch()) {
                (Console::New, _, _) => JobRule::NoJob,
                (Console::None, officina_core::spawn::EnvRule::Contained, _) => {
                    JobRule::KillOnClose
                }
                (Console::None, officina_core::spawn::EnvRule::Session, true) => JobRule::NoJob,
                (Console::None, officina_core::spawn::EnvRule::Session, false) => {
                    JobRule::TreeOnTimeout
                }
            };
            assert_eq!(rule, expected, "{kind:?}");
        }
    }

    #[test]
    fn a_binary_folder_an_officina_folder_or_a_project_is_where_the_rule_says() {
        let claude = ResolvedClaude::new(PathBuf::from("C:/path/to/bin/claude.exe"));
        assert_eq!(working_dir(&claude, &CwdRule::Inherited), None);
        assert_eq!(
            working_dir(&claude, &CwdRule::BinaryFolder),
            Some(PathBuf::from("C:/path/to/bin"))
        );
        let triage = sample(VectorKind::TriageCall);
        let expected = officina_core::spawn::OfficinaDir::TriageWorkdir
            .segments()
            .iter()
            .fold(PathBuf::from("C:/path/to/home"), |p, s| p.join(s));
        assert_eq!(working_dir(&claude, triage.cwd()), Some(expected));
        let bare = ResolvedClaude::new(PathBuf::from("claude.exe"));
        assert_eq!(working_dir(&bare, &CwdRule::BinaryFolder), None);
        let order = sample(VectorKind::WorkOrder);
        assert_eq!(
            working_dir(&claude, order.cwd()),
            Some(PathBuf::from("C:/path/to/Backend"))
        );
    }

    /// AD-73, D-128 review F5: built without `window-authority`, every
    /// launch kind is refused before any process starts, with an error
    /// that carries no authority marker. The stand-in `claude.exe` is an
    /// empty file: had anything tried to start it, the error would be a
    /// spawn failure instead. Run alone by CI (`cargo test -p
    /// officina-host --lib spawn`), since every other test build turns the
    /// feature on.
    #[cfg(not(feature = "window-authority"))]
    #[test]
    fn execute_refuses_every_launch_kind_without_the_feature() {
        let marker = include_str!("relay/authority.rs")
            .lines()
            .find_map(|line| {
                line.strip_prefix("pub const AUTHORITY_MARKER: &str = \"")?
                    .strip_suffix("\";")
            })
            .expect("the marker's declaration");
        let dir = tempfile::tempdir().unwrap();
        let claude = dir.path().join("claude.exe");
        std::fs::write(&claude, b"").unwrap();
        let claude = ResolvedClaude::new(claude);
        let base = BaseEnv::new(Vec::new());
        let launches: Vec<VectorKind> = VectorKind::ALL
            .iter()
            .copied()
            .filter(|k| k.is_launch())
            .collect();
        assert!(!launches.is_empty());
        for kind in launches {
            let refused = execute(&claude, &sample(kind), &base).unwrap_err();
            assert_eq!(refused, SpawnError::NoAuthority(kind), "{kind:?}");
            assert!(!refused.to_string().contains(marker), "{refused}");
        }
        // The control: a read kind does try the stand-in, and fails to
        // start it.
        let read = execute(&claude, &sample(VectorKind::Version), &base).unwrap_err();
        assert!(matches!(read, SpawnError::Spawn(_)), "{read:?}");
    }

    #[test]
    fn a_shim_is_refused_before_anything_starts() {
        let dir = tempfile::tempdir().unwrap();
        let ran = dir.path().join("ran.txt");
        for name in ["claude.cmd", "claude.BAT", "claude.cmd.", "claude.com"] {
            let file = dir.path().join(name.trim_end_matches('.'));
            std::fs::write(&file, format!("@echo ran> \"{}\"\r\n", ran.display())).unwrap();
            let claude = ResolvedClaude::new(dir.path().join(name));
            assert!(!claude.is_native(), "{name}");
            for kind in VectorKind::ALL.iter().copied() {
                let refused = execute(&claude, &sample(kind), &BaseEnv::default()).unwrap_err();
                assert!(
                    matches!(refused.cause(), SpawnError::NotNative(_))
                        || refused == SpawnError::NoAuthority(kind),
                    "{name} {kind:?}: {refused:?}"
                );
            }
        }
        assert!(!ran.exists(), "a shim ran");
        // Positive control: the same script runs when cmd.exe is asked.
        #[cfg(windows)]
        {
            #[expect(
                clippy::disallowed_methods,
                reason = "the control runs the shim through cmd.exe, as a refused spawn would have"
            )]
            let status = Command::new("cmd.exe")
                .args(["/d", "/c"])
                .arg(dir.path().join("claude.cmd"))
                .status()
                .unwrap();
            assert!(status.success() && ran.exists());
        }
    }
}
