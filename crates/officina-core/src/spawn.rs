//! Every `claude` process Officina starts, as data (AD-79, AD-80, D-128 as
//! amended by D-561 and D-562). A [`ClaudeVector`] fixes, for one
//! [`VectorKind`], the arguments after the binary, the working directory
//! rule, the console, stdin, what is captured, the deadline, the variables
//! set and the environment rule. Each kind's builder is the only way to
//! get one, from typed inputs; the host's `spawn::execute` is the only
//! place that turns one into a process, and `tests/vectors/spawn.json`
//! pins each kind.
//!
//! Two environment rules (D-561): a session kind runs as a session the
//! author starts would, with the base environment minus the variables
//! Claude Code gives a session's children ([`SESSION_ENV_REMOVED`]) and
//! minus [`AGENT_VIEW_VAR`], so its hooks still find the author's
//! `OFFICINA_*` settings; a contained kind (Officina's own calls) gets the
//! base's names of the contained allow list only, so a session's
//! variables and Officina's own never reach it, by construction. Every
//! kind then sets [`NO_CURRENT_DIR_VAR`] and, when the floor declares one,
//! [`CONFIG_DIR_VAR`], never one inherited from the base. Names compare
//! ignoring ASCII case, as Windows compares them.

use serde::{Deserialize, Serialize};

use crate::focus::is_valid_attach_id;
use crate::orders::work_order::WorkOrder;
use crate::registry::ingestion::TRIAGE_DIR;
use crate::registry::triage_call::{CALL_TIMEOUT_MS, STDOUT_CAP_BYTES, WORKDIR_NAME};
use crate::relay::template::ModelAlias;

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What a `claude` process is for. Kinds join with the story that
    /// builds them, each with its golden case (D-128).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum VectorKind {
        /// `claude agents --json --all`, the board's session list.
        Agents,
        /// `claude --version`, the contract's version check.
        Version,
        /// `claude attach <id>` in a console of its own (FR-10).
        Attach,
        /// The contained triage call (AD-51).
        TriageCall,
        /// `claude --bg` with a validated work order's prompt (FR-3).
        WorkOrder,
    }
}

impl VectorKind {
    /// True for a kind that launches work on the author's behalf, which
    /// only the window may do (AD-73, AD-21): the host compiles its path
    /// under `window-authority` only.
    ///
    /// ```
    /// use officina_core::spawn::VectorKind;
    /// assert!(VectorKind::TriageCall.is_launch());
    /// assert!(!VectorKind::Agents.is_launch());
    /// ```
    pub const fn is_launch(self) -> bool {
        match self {
            VectorKind::Agents | VectorKind::Version | VectorKind::Attach => false,
            VectorKind::TriageCall | VectorKind::WorkOrder => true,
        }
    }

    /// The kind's environment rule (D-561).
    pub const fn env_rule(self) -> EnvRule {
        match self {
            VectorKind::Agents
            | VectorKind::Version
            | VectorKind::Attach
            | VectorKind::WorkOrder => EnvRule::Session,
            VectorKind::TriageCall => EnvRule::Contained,
        }
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// How a vector's environment is built from the base (D-561).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum EnvRule {
        /// The base minus [`SESSION_ENV_REMOVED`] and [`AGENT_VIEW_VAR`].
        Session,
        /// The base's names of the contained allow list, nothing else.
        Contained,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Whether the process gets a console of its own.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum Console {
        /// No console window; the process is captured.
        None,
        /// A new console the author types into; the process is not waited
        /// on.
        New,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What the process reads on stdin.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum StdinRule {
        /// Nothing: a null device.
        Null,
        /// The console it gets ([`Console::New`]).
        Console,
        /// The bytes given with [`ClaudeVector::with_stdin`], on a pipe.
        Data,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// A folder under the Officina home a kind runs in.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum OfficinaDir {
        /// `<home>/triage/empty` (AD-51), skipped by ingestion (AD-43).
        TriageWorkdir,
    }
}

impl OfficinaDir {
    /// The folder's path segments under the Officina home.
    pub const fn segments(self) -> &'static [&'static str] {
        match self {
            OfficinaDir::TriageWorkdir => &[TRIAGE_DIR, WORKDIR_NAME],
        }
    }
}

/// Where the process runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "rule", rename_all = "snake_case", deny_unknown_fields)]
pub enum CwdRule {
    /// Officina's own working directory.
    Inherited,
    /// The folder of the resolved `claude` binary, which the session it
    /// serves does not write (AD-15).
    BinaryFolder,
    /// A fixed folder under the Officina home.
    Officina { home: String, dir: OfficinaDir },
    /// A catalogue project's folder as the floor declares it (D-6).
    Project { folder: String },
}

/// What a captured kind keeps of its output; both streams are drained to
/// their end past it (D-97).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub stdout_cap: usize,
    pub stderr_cap: usize,
}

/// The variables Claude Code gives a session's children: the session's
/// identity, project and process, the entrypoint and nesting markers, and
/// the messaging socket and token (AD-80). Every `CLAUDE*` name and
/// `AI_AGENT` of the session-variables entry (research notes E84, Claude
/// Code 2.1.312), not the defaults that entry lists apart. A child holding
/// a session's messaging token is accepted as that session's own child, so
/// none of these reaches a session kind (D-128).
pub const SESSION_ENV_REMOVED: &[&str] = &[
    "AI_AGENT",
    "CLAUDECODE",
    "CLAUDE_CODE_BRIDGE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_DISABLE_TERMINAL_TITLE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_SSE_PORT",
    "CLAUDE_EFFORT",
    "CLAUDE_PID",
    "CLAUDE_PROJECT_DIR",
];

/// Set to `1`, it turns off `--bg`, `claude agents` and `claude logs`
/// (S7). A session kind removes it, since what it starts needs agent view.
pub const AGENT_VIEW_VAR: &str = "CLAUDE_CODE_DISABLE_AGENT_VIEW";

/// What a process needs from its environment to start and find its files,
/// on Windows and on Linux: the system folders, the user's folders, the
/// temporary folder, the program search path, the locale. Part of the
/// contained allow list (epic-3-decisions.md D-439, moved here by D-561).
pub const CONTAINED_ENV_SYSTEM: &[&str] = &[
    "SystemRoot",
    "SystemDrive",
    "windir",
    "ComSpec",
    "PATH",
    "PATHEXT",
    "TEMP",
    "TMP",
    "TMPDIR",
    "USERPROFILE",
    "HOMEDRIVE",
    "HOMEPATH",
    "HOME",
    "APPDATA",
    "LOCALAPPDATA",
    "ProgramData",
    "ProgramFiles",
    "ProgramFiles(x86)",
    "ProgramW6432",
    "CommonProgramFiles",
    "CommonProgramFiles(x86)",
    "CommonProgramW6432",
    "USERNAME",
    "USERDOMAIN",
    "USER",
    "LOGNAME",
    "COMPUTERNAME",
    "OS",
    "PROCESSOR_ARCHITECTURE",
    "NUMBER_OF_PROCESSORS",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "XDG_STATE_HOME",
    "XDG_RUNTIME_DIR",
    "LANG",
    "LC_ALL",
    "TZ",
];

/// What `claude` needs to reach the API through a corporate network: the
/// proxy variables, and the certificate variables of a proxy that
/// inspects TLS or asks for a client certificate (Claude Code's network
/// configuration documentation). Part of the contained allow list.
pub const CONTAINED_ENV_NETWORK: &[&str] = &[
    "HTTPS_PROXY",
    "HTTP_PROXY",
    "NO_PROXY",
    "ALL_PROXY",
    "NODE_EXTRA_CA_CERTS",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "CLAUDE_CODE_CLIENT_CERT",
    "CLAUDE_CODE_CLIENT_KEY",
    "CLAUDE_CODE_CLIENT_KEY_PASSPHRASE",
];

/// What `claude` authenticates with, as Claude Code documents it: an API
/// key or a token, the endpoint and the headers a gateway the author's own
/// sessions go through asks for. The endpoint is kept because a contained
/// call sends where the author's sessions already send the same text; with
/// it dropped, a key valid only at a gateway would fail every call.
/// Variables that choose the model (an alias's mapping, a default model)
/// are not kept: `--model` pins it (AD-51). Neither are the Bedrock and
/// Vertex switches: the call was verified against Anthropic's API only
/// (E16). Part of the contained allow list.
pub const CONTAINED_ENV_AUTH: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_CUSTOM_HEADERS",
];

/// Claude Code's config dir variable: a vector sets the floor's (AD-6),
/// never one inherited from the base environment.
pub const CONFIG_DIR_VAR: &str = "CLAUDE_CONFIG_DIR";

/// Set to `1` on every vector, it stops `cmd.exe` and `CreateProcess`
/// searching the current folder for a program (AD-15).
pub const NO_CURRENT_DIR_VAR: &str = "NoDefaultCurrentDirectoryInExePath";

/// How long `claude agents` and `claude --version` may run.
pub const AGENTS_DEADLINE_MS: u64 = 5_000;

/// The most stdout `claude agents` and `claude --version` keep; `claude
/// agents --all` stays far below it.
pub const AGENTS_STDOUT_CAP: usize = 8 * 1024 * 1024;

/// How much of a captured child's stderr is kept, for a failure's message.
pub const STDERR_CAP_BYTES: usize = 4 * 1024;

/// What a work order keeps of each output stream (AD-79): `claude --bg`
/// prints a few lines and exits (E20).
pub const WORK_ORDER_OUTPUT_CAP: usize = 64 * 1024;

/// How long `claude --bg` may run before its process, and it alone, is
/// ended (AD-79, D-128): it exits within seconds once the session is handed
/// to the supervisor (E20).
pub const WORK_ORDER_DEADLINE_MS: u64 = 60_000;

/// What the triage call's `--settings` sets (AD-51 as amended): a `-p`
/// session binds an inbox, and another session must not deliver text into
/// it mid-call.
pub const CONTAINED_SETTINGS: &str = r#"{"crossSessionInbound":"refuse"}"#;

/// True when a contained kind receives `name` from the base: one of
/// [`CONTAINED_ENV_SYSTEM`], [`CONTAINED_ENV_NETWORK`] or
/// [`CONTAINED_ENV_AUTH`], ignoring ASCII case. [`CONFIG_DIR_VAR`] and
/// [`NO_CURRENT_DIR_VAR`] are not: the vector sets its own.
///
/// ```
/// use officina_core::spawn::passed_to_contained;
/// assert!(passed_to_contained("path"));
/// assert!(!passed_to_contained("CLAUDE_CODE_SESSION_ID"));
/// ```
pub fn passed_to_contained(name: &str) -> bool {
    CONTAINED_ENV_SYSTEM
        .iter()
        .chain(CONTAINED_ENV_NETWORK)
        .chain(CONTAINED_ENV_AUTH)
        .any(|kept| kept.eq_ignore_ascii_case(name))
}

/// True when a session kind drops `name` from the base:
/// [`SESSION_ENV_REMOVED`], [`AGENT_VIEW_VAR`], and the two variables every
/// vector sets itself, ignoring ASCII case.
///
/// ```
/// use officina_core::spawn::removed_from_sessions;
/// assert!(removed_from_sessions("claude_code_session_id"));
/// assert!(!removed_from_sessions("OFFICINA_HOME"));
/// ```
pub fn removed_from_sessions(name: &str) -> bool {
    SESSION_ENV_REMOVED
        .iter()
        .chain(&[AGENT_VIEW_VAR, CONFIG_DIR_VAR, NO_CURRENT_DIR_VAR])
        .any(|removed| removed.eq_ignore_ascii_case(name))
}

/// The extension of `binary`'s file name as Windows reads it, trailing
/// dots and spaces dropped, in lowercase; `None` without one.
///
/// ```
/// use officina_core::spawn::binary_extension;
/// assert_eq!(binary_extension("C:/npm/claude.CMD. ").as_deref(), Some("cmd"));
/// assert_eq!(binary_extension("C:/x.bat/claude"), None);
/// ```
pub fn binary_extension(binary: &str) -> Option<String> {
    let name = binary.rsplit(['/', '\\']).next().unwrap_or(binary);
    let name = name.trim_end_matches(['.', ' ']);
    name.rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
}

/// True when Officina may run `binary` (D-562): a native executable, whose
/// name ends in `.exe` or has no extension (a Linux binary; on Windows the
/// program search adds `.exe`). A `.cmd` or `.bat` shim, which only
/// `cmd.exe` runs and which re-parses what it is given, is refused for
/// every kind, and so is any other extension.
///
/// ```
/// use officina_core::spawn::is_native_binary;
/// assert!(is_native_binary("C:/bin/claude.exe"));
/// assert!(is_native_binary("/usr/local/bin/claude"));
/// assert!(!is_native_binary("C:/npm/claude.cmd"));
/// assert!(!is_native_binary("C:/bin/claude.com"));
/// ```
pub fn is_native_binary(binary: &str) -> bool {
    binary_extension(binary).is_none_or(|extension| extension == "exe")
}

/// A background session's id, as `claude attach` and `claude logs` take
/// it: [`is_valid_attach_id`]'s rule, the one copy of it (AD-15).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BackgroundId(String);

impl BackgroundId {
    /// The id, when it follows the rule.
    ///
    /// ```
    /// use officina_core::spawn::BackgroundId;
    /// assert!(BackgroundId::parse("5e8062f4").is_some());
    /// assert!(BackgroundId::parse("a;calc").is_none());
    /// ```
    pub fn parse(id: &str) -> Option<BackgroundId> {
        is_valid_attach_id(id).then(|| BackgroundId(id.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One `claude` process, as data. Its fields are private: the per-kind
/// builders of this module ([`agents`], [`version`], [`attach`],
/// [`triage_call`], [`work_order`]) are its only constructors.
///
/// ```compile_fail
/// use officina_core::spawn::{ClaudeVector, Console, CwdRule, EnvRule, StdinRule, VectorKind};
/// let vector = ClaudeVector {
///     kind: VectorKind::Attach,
///     args: vec!["attach".to_owned(), "x & calc".to_owned()],
///     cwd: CwdRule::Inherited,
///     console: Console::New,
///     stdin: StdinRule::Console,
///     capture: None,
///     deadline_ms: None,
///     env_set: Vec::new(),
///     env_rule: EnvRule::Session,
///     stdin_data: Vec::new(),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClaudeVector {
    kind: VectorKind,
    args: Vec<String>,
    cwd: CwdRule,
    console: Console,
    stdin: StdinRule,
    capture: Option<Capture>,
    deadline_ms: Option<u64>,
    env_set: Vec<(String, String)>,
    env_rule: EnvRule,
    #[serde(skip)]
    stdin_data: Vec<u8>,
}

impl ClaudeVector {
    pub fn kind(&self) -> VectorKind {
        self.kind
    }

    /// The arguments after the binary, each one argument.
    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn cwd(&self) -> &CwdRule {
        &self.cwd
    }

    pub fn console(&self) -> Console {
        self.console
    }

    pub fn stdin(&self) -> StdinRule {
        self.stdin
    }

    /// The bytes written on stdin under [`StdinRule::Data`].
    pub fn stdin_data(&self) -> &[u8] {
        &self.stdin_data
    }

    /// `None` for a console kind, which is not waited on.
    pub fn capture(&self) -> Option<Capture> {
        self.capture
    }

    /// `None` for a console kind.
    pub fn deadline_ms(&self) -> Option<u64> {
        self.deadline_ms
    }

    /// The variables the vector sets, after its rule built the rest.
    pub fn env_set(&self) -> &[(String, String)] {
        &self.env_set
    }

    pub fn env_rule(&self) -> EnvRule {
        self.env_rule
    }

    /// The vector with `data` to write on stdin; only a [`StdinRule::Data`]
    /// kind writes it.
    pub fn with_stdin(mut self, data: Vec<u8>) -> ClaudeVector {
        self.stdin_data = data;
        self
    }

    /// The vector with its deadline and stdout cap lowered to at most
    /// `deadline_ms` and `stdout_cap`; never raised, so the golden values
    /// stay the most a kind is given.
    pub fn tightened(mut self, deadline_ms: u64, stdout_cap: usize) -> ClaudeVector {
        self.deadline_ms = self.deadline_ms.map(|d| d.min(deadline_ms));
        if let Some(capture) = &mut self.capture {
            capture.stdout_cap = capture.stdout_cap.min(stdout_cap);
        }
        self
    }

    /// The child's whole environment, built from `base` by the vector's
    /// rule, then [`ClaudeVector::env_set`]. The host clears the child's
    /// environment and sets exactly this.
    ///
    /// ```
    /// use officina_core::spawn::{agents, triage_call};
    /// use officina_core::registry::triage_call::{TRIAGE_MODEL, schema};
    /// let base = vec![
    ///     ("CLAUDE_PID".to_owned(), "7".to_owned()),
    ///     ("OFFICINA_HOME".to_owned(), "C:/o".to_owned()),
    ///     ("PATH".to_owned(), "C:/bin".to_owned()),
    /// ];
    /// let session = agents(None).child_env(&base);
    /// assert!(session.iter().all(|(name, _)| name != "CLAUDE_PID"));
    /// assert!(session.iter().any(|(name, _)| name == "OFFICINA_HOME"));
    /// let contained = triage_call(TRIAGE_MODEL, &schema(), None, "C:/o").child_env(&base);
    /// assert!(contained.iter().all(|(name, _)| name != "OFFICINA_HOME"));
    /// assert!(contained.contains(&("PATH".to_owned(), "C:/bin".to_owned())));
    /// ```
    pub fn child_env(&self, base: &[(String, String)]) -> Vec<(String, String)> {
        base.iter()
            .filter(|(name, _)| match self.env_rule {
                EnvRule::Session => !removed_from_sessions(name),
                EnvRule::Contained => passed_to_contained(name),
            })
            .cloned()
            .chain(self.env_set.iter().cloned())
            .collect()
    }
}

/// [`NO_CURRENT_DIR_VAR`]` = 1`, then the floor's config dir when it names
/// one.
fn env_set(config_dir: Option<&str>) -> Vec<(String, String)> {
    std::iter::once((NO_CURRENT_DIR_VAR.to_owned(), "1".to_owned()))
        .chain(config_dir.map(|dir| (CONFIG_DIR_VAR.to_owned(), dir.to_owned())))
        .collect()
}

fn owned(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_owned()).collect()
}

/// `claude agents --json --all` (AD-4): from Officina's own folder, no
/// console, captured within [`AGENTS_STDOUT_CAP`] and
/// [`AGENTS_DEADLINE_MS`].
pub fn agents(config_dir: Option<&str>) -> ClaudeVector {
    ClaudeVector {
        kind: VectorKind::Agents,
        args: owned(&["agents", "--json", "--all"]),
        cwd: CwdRule::Inherited,
        console: Console::None,
        stdin: StdinRule::Null,
        capture: Some(Capture {
            stdout_cap: AGENTS_STDOUT_CAP,
            stderr_cap: STDERR_CAP_BYTES,
        }),
        deadline_ms: Some(AGENTS_DEADLINE_MS),
        env_set: env_set(config_dir),
        env_rule: VectorKind::Agents.env_rule(),
        stdin_data: Vec::new(),
    }
}

/// `claude --version`, as [`agents`] runs.
pub fn version(config_dir: Option<&str>) -> ClaudeVector {
    ClaudeVector {
        kind: VectorKind::Version,
        args: owned(&["--version"]),
        cwd: CwdRule::Inherited,
        console: Console::None,
        stdin: StdinRule::Null,
        capture: Some(Capture {
            stdout_cap: AGENTS_STDOUT_CAP,
            stderr_cap: STDERR_CAP_BYTES,
        }),
        deadline_ms: Some(AGENTS_DEADLINE_MS),
        env_set: env_set(config_dir),
        env_rule: VectorKind::Version.env_rule(),
        stdin_data: Vec::new(),
    }
}

/// `claude attach <id>` in a new console, from the binary's folder (v0.1's
/// rule, security audit of 2026-10-27): the session's folder could hold a
/// planted program, and [`NO_CURRENT_DIR_VAR`] stops the search there too.
pub fn attach(id: &BackgroundId, config_dir: Option<&str>) -> ClaudeVector {
    ClaudeVector {
        kind: VectorKind::Attach,
        args: vec!["attach".to_owned(), id.as_str().to_owned()],
        cwd: CwdRule::BinaryFolder,
        console: Console::New,
        stdin: StdinRule::Console,
        capture: None,
        deadline_ms: None,
        env_set: env_set(config_dir),
        env_rule: VectorKind::Attach.env_rule(),
        stdin_data: Vec::new(),
    }
}

/// The triage call (AD-51 as amended, E16): print mode, `model`, no tool,
/// no MCP server, no settings source, the inbox refused
/// ([`CONTAINED_SETTINGS`]), no session persistence, a JSON answer
/// constrained by `schema`; the prompt comes on stdin
/// ([`ClaudeVector::with_stdin`]). Runs in `<officina_home>/triage/empty`.
pub fn triage_call(
    model: ModelAlias,
    schema: &serde_json::Value,
    config_dir: Option<&str>,
    officina_home: &str,
) -> ClaudeVector {
    let args = owned(&[
        "-p",
        "--model",
        model.code(),
        "--tools",
        "",
        "--strict-mcp-config",
        "--mcp-config",
        r#"{"mcpServers":{}}"#,
        "--setting-sources",
        "",
        "--settings",
        CONTAINED_SETTINGS,
        "--no-session-persistence",
        "--output-format",
        "json",
        "--json-schema",
    ])
    .into_iter()
    .chain([schema.to_string()])
    .collect();
    ClaudeVector {
        kind: VectorKind::TriageCall,
        args,
        cwd: CwdRule::Officina {
            home: officina_home.to_owned(),
            dir: OfficinaDir::TriageWorkdir,
        },
        console: Console::None,
        stdin: StdinRule::Data,
        capture: Some(Capture {
            stdout_cap: STDOUT_CAP_BYTES,
            stderr_cap: STDERR_CAP_BYTES,
        }),
        deadline_ms: Some(CALL_TIMEOUT_MS),
        env_set: env_set(config_dir),
        env_rule: VectorKind::TriageCall.env_rule(),
        stdin_data: Vec::new(),
    }
}

/// A validated work order as a background session (AD-79, FR-3 as
/// amended): `--bg`, its name `<template>-<seq>`, the template's model when
/// it names one, then `--` and the rendered prompt as exactly one argument,
/// so no line of it reads as an option; E20 found the prompt delivered byte
/// for byte after `--` with `--bg`, so the separator stays (D-127). It runs
/// in the project's folder, without a console or stdin, its output
/// captured within [`WORK_ORDER_OUTPUT_CAP`] and [`WORK_ORDER_DEADLINE_MS`],
/// as a session the author starts would (D-128). No `--settings` and no
/// permission mode: the session takes the author's own.
///
/// `seq` is the number the card store reserved for the card (D-135), so
/// the name is unique and the vector the card shows is the one that runs.
pub fn work_order(order: &WorkOrder, seq: u32, config_dir: Option<&str>) -> ClaudeVector {
    let name = format!("{}-{seq}", order.template());
    let model = order
        .model()
        .map(|model| ["--model".to_owned(), model.code().to_owned()]);
    let args = ["--bg".to_owned(), "--name".to_owned(), name]
        .into_iter()
        .chain(model.into_iter().flatten())
        .chain(["--".to_owned(), order.prompt().to_owned()])
        .collect();
    ClaudeVector {
        kind: VectorKind::WorkOrder,
        args,
        cwd: CwdRule::Project {
            folder: order.project_folder().to_owned(),
        },
        console: Console::None,
        stdin: StdinRule::Null,
        capture: Some(Capture {
            stdout_cap: WORK_ORDER_OUTPUT_CAP,
            stderr_cap: WORK_ORDER_OUTPUT_CAP,
        }),
        deadline_ms: Some(WORK_ORDER_DEADLINE_MS),
        env_set: env_set(config_dir),
        env_rule: VectorKind::WorkOrder.env_rule(),
        stdin_data: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orders::work_order::sample_order;
    use crate::registry::triage_call::{DATA_MARKER, TRIAGE_MODEL, schema};

    const CONFIG: &str = "C:/path/to/claude-work";

    /// A vector of `kind` with `config_dir`, through its builder.
    fn built(kind: VectorKind, config_dir: Option<&str>) -> ClaudeVector {
        match kind {
            VectorKind::Agents => agents(config_dir),
            VectorKind::Version => version(config_dir),
            VectorKind::Attach => attach(&BackgroundId::parse("bg01").unwrap(), config_dir),
            VectorKind::TriageCall => {
                triage_call(TRIAGE_MODEL, &schema(), config_dir, "C:/path/to/home")
            }
            VectorKind::WorkOrder => work_order(&sample_order(), 7, config_dir),
        }
    }

    /// A base holding every name a rule could keep or drop: each session
    /// variable (one in mixed case), the allow list, `OFFICINA_*` names,
    /// agent view off, an inherited config dir and a name on no list.
    fn seeded_base() -> Vec<(String, String)> {
        let mut base: Vec<(String, String)> = SESSION_ENV_REMOVED
            .iter()
            .enumerate()
            .map(|(at, name)| {
                let name = if at == 0 {
                    mixed_case(name)
                } else {
                    (*name).to_owned()
                };
                (name, "session".to_owned())
            })
            .collect();
        for name in CONTAINED_ENV_SYSTEM
            .iter()
            .chain(CONTAINED_ENV_NETWORK)
            .chain(CONTAINED_ENV_AUTH)
        {
            base.push(((*name).to_ascii_lowercase(), "allowed".to_owned()));
        }
        for name in [
            "OFFICINA_HOME",
            "OFFICINA_FLOOR",
            "OFFICINA_NO_EXCERPTS",
            "NOT_ON_ANY_LIST",
        ] {
            base.push((name.to_owned(), "author".to_owned()));
        }
        base.push((AGENT_VIEW_VAR.to_owned(), "1".to_owned()));
        base.push((CONFIG_DIR_VAR.to_owned(), "C:/inherited".to_owned()));
        base.push((NO_CURRENT_DIR_VAR.to_ascii_uppercase(), "0".to_owned()));
        base
    }

    fn mixed_case(name: &str) -> String {
        name.chars()
            .enumerate()
            .map(|(at, c)| {
                if at % 2 == 0 {
                    c.to_ascii_lowercase()
                } else {
                    c
                }
            })
            .collect()
    }

    fn kinds_with(rule: EnvRule) -> impl Iterator<Item = VectorKind> {
        VectorKind::ALL
            .iter()
            .copied()
            .filter(move |k| k.env_rule() == rule)
    }

    #[test]
    fn background_id_follows_the_attach_rule() {
        for id in ["5e8062f4", "a_b-C9", &"a".repeat(64)] {
            assert_eq!(
                BackgroundId::parse(id).map(|b| b.as_str().to_owned()),
                Some(id.to_owned())
            );
            assert!(is_valid_attach_id(id));
        }
        for bad in ["", "a;b", "a b", "../x", "\"x\"", &"a".repeat(65)] {
            assert_eq!(BackgroundId::parse(bad), None, "{bad}");
            assert!(!is_valid_attach_id(bad), "{bad}");
        }
    }

    #[test]
    fn attach_takes_only_a_parsed_background_id() {
        let id = BackgroundId::parse("0f1e2d3c-4b5a").unwrap();
        let vector = attach(&id, None);
        assert_eq!(vector.args(), ["attach", "0f1e2d3c-4b5a"]);
        assert_eq!(vector.args().len(), 2, "the id is one argument");
        assert_eq!(vector.cwd(), &CwdRule::BinaryFolder);
        assert_eq!(vector.console(), Console::New);
        assert_eq!(vector.stdin(), StdinRule::Console);
        assert_eq!((vector.capture(), vector.deadline_ms()), (None, None));
    }

    #[test]
    fn is_launch_names_every_kind() {
        for kind in VectorKind::ALL {
            // A launch kind is a contained call or work for the author;
            // the read half is what the board and focus need.
            let read_half = matches!(
                kind,
                VectorKind::Agents | VectorKind::Version | VectorKind::Attach
            );
            assert_eq!(kind.is_launch(), !read_half, "{kind:?}");
        }
        assert!(VectorKind::ALL.iter().any(|k| k.is_launch()));
        assert!(VectorKind::ALL.iter().any(|k| !k.is_launch()));
    }

    #[test]
    fn session_kinds_remove_the_session_variables() {
        let base = seeded_base();
        for kind in kinds_with(EnvRule::Session) {
            let env = built(kind, Some(CONFIG)).child_env(&base);
            for removed in SESSION_ENV_REMOVED {
                assert!(
                    env.iter().all(|(n, _)| !n.eq_ignore_ascii_case(removed)),
                    "{kind:?}: {removed}"
                );
            }
            // Everything else of the base is kept as it was, the author's
            // `OFFICINA_*` names included.
            for (name, value) in &base {
                let kept = env.iter().any(|(n, v)| n == name && v == value);
                assert_eq!(kept, !removed_from_sessions(name), "{kind:?}: {name}");
            }
            for name in ["OFFICINA_HOME", "OFFICINA_FLOOR", "OFFICINA_NO_EXCERPTS"] {
                assert!(env.iter().any(|(n, _)| n == name), "{kind:?}: {name}");
            }
        }
    }

    #[test]
    fn contained_kinds_take_only_the_allow_list() {
        let base = seeded_base();
        for kind in kinds_with(EnvRule::Contained) {
            let env = built(kind, Some(CONFIG)).child_env(&base);
            for (name, value) in &base {
                let kept = env.iter().any(|(n, v)| n == name && v == value);
                assert_eq!(kept, passed_to_contained(name), "{kind:?}: {name}");
            }
            let inherited: Vec<&str> = env
                .iter()
                .map(|(n, _)| n.as_str())
                .filter(|n| {
                    !built(kind, Some(CONFIG))
                        .env_set()
                        .iter()
                        .any(|(s, _)| s == n)
                })
                .collect();
            assert!(
                inherited.iter().all(|n| passed_to_contained(n)),
                "{kind:?}: {inherited:?}"
            );
            assert!(
                env.iter()
                    .all(|(n, _)| !n.to_ascii_uppercase().starts_with("OFFICINA_")),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn every_kind_sets_the_exe_path_rule_and_the_config_dir() {
        let base = seeded_base();
        for kind in VectorKind::ALL.iter().copied() {
            for config in [Some(CONFIG), None] {
                let env = built(kind, config).child_env(&base);
                let named = |var: &str| -> Vec<&(String, String)> {
                    env.iter()
                        .filter(|(n, _)| n.eq_ignore_ascii_case(var))
                        .collect()
                };
                assert_eq!(
                    named(NO_CURRENT_DIR_VAR),
                    [&(NO_CURRENT_DIR_VAR.to_owned(), "1".to_owned())],
                    "{kind:?}"
                );
                let expected: Vec<(String, String)> = config
                    .map(|dir| (CONFIG_DIR_VAR.to_owned(), dir.to_owned()))
                    .into_iter()
                    .collect();
                assert_eq!(
                    named(CONFIG_DIR_VAR)
                        .into_iter()
                        .cloned()
                        .collect::<Vec<_>>(),
                    expected,
                    "{kind:?}: the floor's, never the base's"
                );
            }
        }
    }

    #[test]
    fn agent_view_is_removed_on_every_session_kind() {
        let base = seeded_base();
        for kind in VectorKind::ALL.iter().copied() {
            let env = built(kind, None).child_env(&base);
            assert!(
                env.iter()
                    .all(|(n, _)| !n.eq_ignore_ascii_case(AGENT_VIEW_VAR)),
                "{kind:?}"
            );
        }
    }

    /// D-561: the contained allow list is the triage call's of v0.3,
    /// moved unchanged. The digest was taken from `registry::triage_call`'s
    /// three lists before the move, in their order, one name per line.
    #[test]
    fn the_allow_list_is_triage_calls_unchanged() {
        let joined: String = [
            CONTAINED_ENV_SYSTEM,
            CONTAINED_ENV_NETWORK,
            CONTAINED_ENV_AUTH,
        ]
        .iter()
        .map(|list| list.join("\n"))
        .collect::<Vec<_>>()
        .join("\n--\n");
        assert_eq!(
            crate::sha256_hex(joined.as_bytes()),
            "7d873efaa9d1c99cbf81855b3d666858beca7a12917924260d14b63b0cb2066a",
            "the contained allow list changed"
        );
        // The prefixes a contained call must never take from the base.
        for name in CONTAINED_ENV_SYSTEM
            .iter()
            .chain(CONTAINED_ENV_NETWORK)
            .chain(CONTAINED_ENV_AUTH)
        {
            assert!(!removed_from_sessions(name), "{name}");
            assert!(
                !name.to_ascii_uppercase().starts_with("OFFICINA_"),
                "{name}"
            );
        }
    }

    #[test]
    fn a_tightened_vector_never_exceeds_its_kind() {
        let vector = triage_call(TRIAGE_MODEL, &schema(), None, "C:/h");
        assert!(
            !vector.args().iter().any(|a| a.contains(DATA_MARKER)),
            "the prompt never reaches the command line"
        );
        let vector = vector.tightened(2_000, 1 << 30);
        assert_eq!(vector.deadline_ms(), Some(2_000));
        assert_eq!(vector.capture().unwrap().stdout_cap, STDOUT_CAP_BYTES);
        let console = attach(&BackgroundId::parse("a").unwrap(), None).tightened(1, 1);
        assert_eq!((console.deadline_ms(), console.capture()), (None, None));
    }

    #[test]
    fn only_native_binaries_run() {
        for refused in [
            "claude.cmd",
            r"C:\npm\claude.Cmd",
            "C:/npm/claude.BAT",
            "claude.cmd.",
            "claude.bat ",
            "claude.cmd. . ",
            "C:/bin/claude.com",
            "C:/bin/claude.ps1",
        ] {
            assert!(!is_native_binary(refused), "{refused:?}");
        }
        for native in [
            "claude.exe",
            "C:/cmd/claude.EXE",
            "claude",
            "claude.cmd.exe",
            "C:/x.bat/claude",
        ] {
            assert!(is_native_binary(native), "{native:?}");
        }
    }

    /// The order of `floor`'s template `bump` in project `backend`, its
    /// one parameter `package` set to `serde_json`.
    fn order_of(floor: &str) -> WorkOrder {
        use crate::relay::catalogue::{ApprovedCatalogue, load};
        use crate::relay::template::{ApprovedTemplate, ParamName, ProjectName, TemplateName};
        let path = "C:/path/to/officina.toml";
        let current = load(floor, path).unwrap();
        assert!(
            current.catalogue.issues.is_empty(),
            "{:?}",
            current.catalogue.issues
        );
        let approved = ApprovedCatalogue::approving(&current, path, 0);
        let template =
            ApprovedTemplate::new(&approved, &current, &TemplateName::parse("bump").unwrap())
                .unwrap();
        let values = [(ParamName::parse("package").unwrap(), "serde_json")];
        WorkOrder::validate(&template, &ProjectName::parse("backend").unwrap(), &values).unwrap()
    }

    /// A template whose prompt holds what a command line could split or
    /// read as options: newlines, a tab, quotes, an inner `--`, `--bg` and
    /// `--model`, a line starting with a dash and trailing backslashes.
    const AWKWARD_FLOOR: &str = r#"schema = 2
[[project]]
name = "backend"
path = "Backend"
[[template]]
name = "bump"
description = "Bump one dependency"
projects = ["backend"]
prompt = "Bump {package}.\n--bg -- --model opus\n-x \"quoted\" and 'single'\ttab 100% & | < > ^ !\nC:\\dir\\ ends C:\\dir\\"
[template.params.package]
values = ["serde_json"]
"#;

    #[test]
    fn the_prompt_is_one_argument_after_the_separator() {
        let order = order_of(AWKWARD_FLOOR);
        assert!(order.prompt().contains("\n--bg -- --model opus\n-x \""));
        let vector = work_order(&order, 12, None);
        assert_eq!(
            vector.args(),
            ["--bg", "--name", "bump-12", "--", order.prompt()],
            "no model named, no --model"
        );
        let with_model = order_of(&AWKWARD_FLOOR.replace(
            "[template.params.package]",
            "model = \"opus\"\n[template.params.package]",
        ));
        let vector = work_order(&with_model, 3, Some("C:/path/to/claude-work"));
        assert_eq!(
            vector.args(),
            [
                "--bg",
                "--name",
                "bump-3",
                "--model",
                "opus",
                "--",
                with_model.prompt()
            ]
        );
        // The separator is the first `--`, just before the prompt, which is
        // the last argument and the only one holding author text.
        let separator = vector.args().iter().position(|a| a == "--").unwrap();
        assert_eq!(separator, vector.args().len() - 2);
        assert_eq!(
            vector.args().last().map(String::as_str),
            Some(with_model.prompt())
        );
        assert_eq!(
            vector.cwd(),
            &CwdRule::Project {
                folder: "C:/path/to/Backend".to_owned()
            }
        );
        assert_eq!(vector.kind(), VectorKind::WorkOrder);
        assert_eq!(
            (vector.console(), vector.stdin()),
            (Console::None, StdinRule::Null)
        );
        assert_eq!(
            vector.capture(),
            Some(Capture {
                stdout_cap: WORK_ORDER_OUTPUT_CAP,
                stderr_cap: WORK_ORDER_OUTPUT_CAP
            })
        );
        assert_eq!(vector.deadline_ms(), Some(WORK_ORDER_DEADLINE_MS));
    }

    #[test]
    fn a_work_order_vector_carries_no_settings_and_no_permission_mode() {
        for order in [sample_order(), order_of(AWKWARD_FLOOR)] {
            let vector = work_order(&order, 1, Some(CONFIG));
            let separator = vector.args().iter().position(|a| a == "--").unwrap();
            let options = &vector.args()[..separator];
            for option in options {
                assert!(
                    ["--bg", "--name", "--model"].contains(&option.as_str())
                        || options
                            .iter()
                            .zip(options.iter().skip(1))
                            .any(|(flag, value)| value == option
                                && (flag == "--name" || flag == "--model")),
                    "{option:?} is neither a work order option nor its value"
                );
                for refused in ["settings", "permission", "allowed", "dangerously", "-p"] {
                    assert!(!option.contains(refused), "{option:?}");
                }
            }
            // The session kind's environment: nothing set but the two every
            // vector sets, the author's own variables kept (D-128).
            assert_eq!(vector.env_rule(), EnvRule::Session);
            assert_eq!(vector.env_set(), env_set(Some(CONFIG)));
            assert!(VectorKind::WorkOrder.is_launch());
        }
    }
}
