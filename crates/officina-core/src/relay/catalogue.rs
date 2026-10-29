//! The catalogue (FR-1, FR-2, FR-4, AD-22 to AD-24): phase 2 of the floor
//! parse, the canonical form the author approves, its hash and its diff.
//!
//! Phase 2 converts each raw `[[action]]`, `[[credential]]` and
//! `[[template]]` alone and validates it, then the `[orchestrator]` and
//! `[auto_review]` tables (AD-75); a failure becomes an [`ActionIssue`]
//! naming its table and drops that entry, never the floor. Issues go to the
//! relay view and doctor, never to the board's `floor_error`.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use super::http::HttpMethod;
use super::params::{self, Param, ParamRule, Pattern, PatternError, ValueTarget};
use super::template::{
    DEFAULT_REVIEW_MODEL, DESCRIPTION_CHARS, FORBIDDEN_PROMPT_STARTS, MAX_PROMPT_CHARS, ModelAlias,
    PROMPT_CONTROL_ALLOWED, Piece, Template, pieces, worst_case_chars,
};
use super::url::{UrlIssue, UrlScheme, UrlTemplate, host_entry};
use crate::floor::{
    Autonomy, FloorError, FloorFile, floor_dir_of, is_windows_absolute, join_project_path,
    parse_floor, resolve_project_path,
};
use crate::quota::normalise_path;

/// Programs the gate denies in a guarded project when the floor declares no
/// `guarded_programs`: PRD amendment 6's list, the WSL routes of D-11
/// (Windows `bash.exe`, the distro launchers) and `wslg`, which starts a
/// Linux program like `wsl`. A trailing `*` is a prefix; an entry ending in
/// `.exe` is matched on the word as written (`relay::gate::guarded_match`).
/// The only list.
pub const DEFAULT_GUARDED_PROGRAMS: &[&str] = &[
    "ansible*",
    "ssh",
    "scp",
    "sftp",
    "rsync",
    "wsl",
    "wslg",
    "bash.exe",
    "ubuntu*",
    "debian",
    "kali",
    "opensuse*",
    "sles*",
    "fedora*",
    "alpine*",
    "oracle*",
];

/// `timeout_s` when the action does not set it: fits the Bash tool's
/// 10-minute cap with a margin (AD-31).
pub const DEFAULT_TIMEOUT_S: u32 = 540;
/// Accepted `timeout_s` (AD-23).
pub const TIMEOUT_RANGE_S: RangeInclusive<i64> = 1..=3600;
/// `max_output_kib` when the action does not set it.
pub const DEFAULT_MAX_OUTPUT_KIB: u32 = 64;
/// Accepted `max_output_kib` (AD-23).
pub const MAX_OUTPUT_RANGE_KIB: RangeInclusive<i64> = 1..=1024;

/// The longest name, for [`MAX_NAME_CHARS`] and the rules' prose alike.
macro_rules! max_name_chars {
    () => {
        32
    };
}

/// Longest action, parameter or credential name.
pub const MAX_NAME_CHARS: usize = max_name_chars!();

/// What [`is_valid_action_name`] accepts, as every message about an
/// action's or a credential's name says it.
pub const NAME_RULE: &str = concat!(
    "a lowercase letter followed by lowercase letters, digits or `-`, at most ",
    max_name_chars!(),
    " characters"
);

/// What [`is_valid_param_name`] accepts, as every message about a
/// parameter's name says it.
pub const PARAM_NAME_RULE: &str = concat!(
    "a lowercase letter followed by lowercase letters, digits, `_` or `-`, at most ",
    max_name_chars!(),
    " characters"
);
/// Longest `env` key (D-1).
pub const MAX_ENV_KEY_CHARS: usize = 64;
/// Longest `distro` (D-4).
pub const MAX_DISTRO_CHARS: usize = 64;

/// Version of the canonical form. Bumping it invalidates every approval on
/// purpose (AD-24); 2 added credentials and HTTP actions (D-39), 3 an
/// action's `forwarded_values` (D-173), 4 templates, `auto_review` and
/// `auto_max_per_hour` (AD-76, D-131).
pub const CANONICAL_FORMAT: u32 = 4;

/// `auto_max_per_hour` when an `auto` action does not set it (AD-75).
pub const DEFAULT_AUTO_MAX_PER_HOUR: u32 = 6;
/// Accepted `auto_max_per_hour` (AD-75).
pub const AUTO_MAX_PER_HOUR_RANGE: RangeInclusive<i64> = 1..=60;

/// Version of `relay/known-projects.json` (D-15).
pub const KNOWN_PROJECTS_SCHEMA: u32 = 1;

/// The name an issue carries when the action has no readable name.
pub const UNNAMED: &str = "(unnamed)";

closed_enum! {
    #[serde(rename_all = "lowercase")]
    /// Where an action runs (AD-22, AD-30).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum Runner {
        /// The program itself, a Windows absolute path.
        #[default]
        Native,
        /// `wsl.exe --exec`, the program a POSIX absolute path.
        Wsl,
    }
}

closed_enum! {
    #[serde(rename_all = "kebab-case")]
    /// How an action is allowed (FR-8).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ActionClass {
        Free,
        AfterCheck,
        Human,
        /// Approved unattended after its check (AD-86, story 5.12); valid
        /// from v0.5 with a `check` (AD-180), not yet run
        /// (`classes::is_available`).
        Auto,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// One rule of AD-23, AD-75 or of D-1 to D-5. Every variant has a
    /// failing fixture in this module's tests. A rule that holds for
    /// actions and templates alike is one variant; the issue's
    /// [`IssueTable`] says which table broke it.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum IssueKind {
        /// The table does not convert: unknown key, wrong type, missing key,
        /// or an action's `kind` other than `http`.
        Malformed,
        /// An HTTP action declaring `runner`, `distro`, `program`, `env` or
        /// `args` (AD-36).
        HttpProgramFields,
        /// An HTTP action's `url` breaks a rule of `relay::url`.
        Url,
        /// An HTTP action's `url` carries a user or a password (D-40).
        UrlUserinfo,
        /// An HTTP action names a credential that is not declared or was
        /// dropped.
        UnknownCredential,
        /// An HTTP action's host is not in its credential's `hosts` (D-37).
        CredentialHostNotBound,
        /// An HTTP action sends a credential over plain `http` (AD-36).
        CredentialOverHttp,
        /// A credential's name is not `[a-z][a-z0-9-]` of at most 32
        /// characters.
        CredentialName,
        /// A credential's `hosts` is empty or holds an entry that is not a
        /// host without a port; no wildcard (D-37).
        CredentialHosts,
        /// A credential's `header` is not an HTTP token, or names a header
        /// Officina never sets (D-40).
        CredentialHeader,
        /// A credential's `scheme` is not an HTTP token (D-40).
        CredentialScheme,
        /// Two credentials share a name: both are dropped.
        DuplicateCredential,
        /// An `auto` action without a `check`: an unattended apply is
        /// always preceded by its dry run (AD-180, D-570).
        AutoWithoutCheck,
        /// `auto_max_per_hour` out of range, or set on a class other than
        /// `auto` (AD-75).
        AutoMaxPerHour,
        /// `class = "auto"` on an HTTP action, which stays on the `human`
        /// path (D-156).
        AutoOnHttp,
        /// An action's or a template's name breaks [`NAME_RULE`].
        Name,
        /// Two actions, or two templates, share a name: both are dropped.
        /// Actions and templates are separate namespaces.
        DuplicateName,
        /// `program` is not absolute for its runner.
        ProgramNotAbsolute,
        /// `program` holds a brace.
        ProgramPlaceholder,
        /// A `native` program ending in `.bat` or `.cmd`, which `cmd.exe`
        /// re-parses (D-5).
        BatchProgram,
        /// `distro` breaks D-4.
        Distro,
        /// `distro` on the `native` runner.
        DistroWithoutWsl,
        /// An `env` key breaks D-1.
        EnvKey,
        /// An `env` value holds a brace: values are literals (D-1).
        EnvValuePlaceholder,
        /// An unbalanced brace or a second placeholder in an `args` item, or
        /// a brace in a template's prompt that delimits no placeholder (D-2).
        Braces,
        /// A placeholder names no declared parameter.
        UndeclaredPlaceholder,
        /// A declared parameter no placeholder uses.
        UnusedParameter,
        /// A parameter name breaks [`PARAM_NAME_RULE`].
        ParamName,
        /// An action's parameter declares both or neither of `values` and
        /// `pattern`.
        ValuesOrPattern,
        /// A `values` entry breaks a value rule for the action's runner, or
        /// a template value breaks `params::template_value`.
        InvalidValue,
        /// A `pattern` over the length limit.
        PatternTooLong,
        /// A `pattern` over the compiled size limit.
        PatternTooComplex,
        /// A `pattern` that does not compile.
        PatternInvalid,
        /// A `pattern` on a parameter a program forwards to a remote shell
        /// or a template (a notice of D-172) that admits a character of
        /// [`FORWARDED_PROBE_CHARS`], without `forwarded_values =
        /// "accepted"` (D-173).
        ForwardedPattern,
        /// `check` is missing on an `after-check` action, set on a `free`
        /// one, or does not name a valid `free` action with the same
        /// parameter names.
        Check,
        /// `timeout_s` out of range.
        Timeout,
        /// `max_output_kib` out of range.
        MaxOutput,
        /// A `guarded_programs` entry breaks D-3; the entry is ignored.
        GuardedProgram,
        /// A project names an unknown or dropped action; its other actions
        /// stay.
        UnknownProjectAction,
        /// Two projects share a name: both leave the catalogue.
        DuplicateProject,
        /// A template's `description` is empty, longer than
        /// [`DESCRIPTION_CHARS`], or holds a control or formatting
        /// character.
        TemplateDescription,
        /// A template's `projects` is empty, or none of its projects is in
        /// the catalogue: the template is dropped.
        TemplateProjects,
        /// A template names a project that is not in the catalogue: the
        /// template stays with its other projects.
        UnknownTemplateProject,
        /// A template's prompt holds a control character other than newline
        /// and tab, or a formatting character.
        TemplatePromptCharacter,
        /// A template's prompt holds only whitespace, or its first character
        /// after leading whitespace is a placeholder or one of
        /// [`FORBIDDEN_PROMPT_STARTS`] (D-159).
        TemplatePromptStart,
        /// A template's longest rendering is over [`MAX_PROMPT_CHARS`].
        TemplatePromptTooLong,
        /// A template parameter without `values`, or with an empty list.
        TemplateValues,
        /// A template parameter declares `pattern`: templates take closed
        /// lists only (FR-2).
        TemplatePattern,
        /// A `model` is not a [`ModelAlias`].
        Model,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// The floor table an issue comes from (D-132).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum IssueTable {
        Action,
        Credential,
        Project,
        GuardedPrograms,
        Template,
        Orchestrator,
        AutoReview,
    }
}

/// A problem found in phase 2. `name` is the entry's (action, credential,
/// template, project), or the key for `guarded_programs`, `[orchestrator]`
/// and `[auto_review]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ActionIssue {
    pub table: IssueTable,
    pub name: String,
    /// 1-based line in the floor file, when the issue comes from one.
    pub line: Option<u32>,
    pub kind: IssueKind,
    pub message: String,
}

/// One `args` item: a literal, or one placeholder with an optional literal
/// prefix and suffix. Either way it yields exactly one argument (AD-22).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgTemplate {
    Literal(String),
    Placeholder {
        prefix: String,
        param: String,
        suffix: String,
    },
}

/// Why an `args` item is not one argument (D-2), with the item as written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BraceError {
    #[error("`}}` closes no placeholder in {0:?}")]
    StrayClose(String),
    #[error("unbalanced `{{` in {0:?}")]
    Unclosed(String),
    #[error("`{{` inside a placeholder in {0:?}")]
    Nested(String),
    #[error("a second placeholder in {0:?}: one argument holds at most one")]
    SecondPlaceholder(String),
    #[error("unbalanced brace in {0:?}")]
    Unbalanced(String),
}

impl ArgTemplate {
    /// Parses one `args` item. Braces delimit placeholders only (D-2).
    ///
    /// ```
    /// use officina_core::relay::catalogue::{ArgTemplate, BraceError};
    /// assert_eq!(
    ///     ArgTemplate::parse("--limit={group}").unwrap().param(),
    ///     Some("group"),
    /// );
    /// assert_eq!(
    ///     ArgTemplate::parse("{a}{b}"),
    ///     Err(BraceError::SecondPlaceholder("{a}{b}".to_owned())),
    /// );
    /// ```
    pub fn parse(item: &str) -> Result<ArgTemplate, BraceError> {
        let Some(open) = item.find(['{', '}']) else {
            return Ok(ArgTemplate::Literal(item.to_owned()));
        };
        if item[open..].starts_with('}') {
            return Err(BraceError::StrayClose(item.to_owned()));
        }
        let inner = &item[open + 1..];
        let Some(close) = inner.find(['{', '}']) else {
            return Err(BraceError::Unclosed(item.to_owned()));
        };
        if inner[close..].starts_with('{') {
            return Err(BraceError::Nested(item.to_owned()));
        }
        let suffix = &inner[close + 1..];
        if let Some(next) = suffix.find(['{', '}']) {
            let rest = &suffix[next..];
            return Err(if rest.starts_with('{') && rest.contains('}') {
                BraceError::SecondPlaceholder(item.to_owned())
            } else {
                BraceError::Unbalanced(item.to_owned())
            });
        }
        Ok(ArgTemplate::Placeholder {
            prefix: item[..open].to_owned(),
            param: inner[..close].to_owned(),
            suffix: suffix.to_owned(),
        })
    }

    /// The parameter this item fills, if any.
    pub fn param(&self) -> Option<&str> {
        match self {
            ArgTemplate::Literal(_) => None,
            ArgTemplate::Placeholder { param, .. } => Some(param),
        }
    }

    /// The item as the floor wrote it.
    pub fn source(&self) -> String {
        match self {
            ArgTemplate::Literal(text) => text.clone(),
            ArgTemplate::Placeholder {
                prefix,
                param,
                suffix,
            } => format!("{prefix}{{{param}}}{suffix}"),
        }
    }
}

/// A validated action, every default explicit. A process action starts
/// `program` with `args`; an HTTP action (`http` set) sends its request and
/// leaves the process fields at their defaults: `native`, no distro, an
/// empty program, no env, no args.
#[derive(Debug, Clone)]
pub struct Action {
    pub name: String,
    pub description: String,
    pub class: ActionClass,
    pub runner: Runner,
    pub distro: Option<String>,
    pub program: String,
    pub env: BTreeMap<String, String>,
    pub args: Vec<ArgTemplate>,
    pub http: Option<HttpSpec>,
    pub params: BTreeMap<String, Param>,
    pub check: Option<String>,
    pub timeout_s: u32,
    pub max_output_kib: u32,
    pub forwarded_values: Option<ForwardedValues>,
    /// `Some` for an `auto` action, the default applied; `None` otherwise.
    pub auto_max_per_hour: Option<u32>,
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// The author's word that a parameter a program forwards to a remote
    /// shell or a template may hold whatever its `pattern` admits (D-173).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ForwardedValues {
        Accepted,
    }
}

/// What an HTTP action sends (AD-36).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpSpec {
    pub method: HttpMethod,
    pub url: UrlTemplate,
    /// The `[[credential]]` whose header the request carries.
    pub credential: Option<String>,
}

/// A validated `[[credential]]` (AD-36, D-37, D-40): the secret is read from
/// the Credential Manager at `officina:<name>` and sent as
/// `<header>: <scheme> <secret>` to these hosts only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credential {
    pub name: String,
    /// Lowercase, sorted, deduplicated; no port.
    pub hosts: Vec<String>,
    pub header: String,
    pub scheme: String,
}

/// A project as the catalogue sees it: the path as declared, the autonomy
/// with the schema's default applied, the valid actions it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogueProject {
    pub name: String,
    pub path: String,
    pub autonomy: Autonomy,
    /// Sorted, deduplicated, valid actions only.
    pub actions: Vec<String>,
}

/// The result of phase 2: what may run, and what was dropped and why.
#[derive(Debug, Clone)]
pub struct Catalogue {
    /// Defaults applied, lowercase, sorted, deduplicated.
    pub guarded_programs: Vec<String>,
    /// Sorted by name.
    pub projects: Vec<CatalogueProject>,
    pub actions: BTreeMap<String, Action>,
    /// Valid credentials, by name.
    pub credentials: BTreeMap<String, Credential>,
    /// Valid templates, by name (AD-75).
    pub templates: BTreeMap<String, Template>,
    /// `[orchestrator] model`; `None` when absent or when the table fails,
    /// and the orchestrator then starts with no model flag (AD-75).
    pub orchestrator_model: Option<ModelAlias>,
    /// `[auto_review] model`, [`DEFAULT_REVIEW_MODEL`] when absent; `None`
    /// when the table fails, and every `auto` request then escalates
    /// (AD-75, AD-87's `review_config_invalid`).
    pub auto_review_model: Option<ModelAlias>,
    /// In file order; issues without a line last.
    pub issues: Vec<ActionIssue>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawHttpAction {
    name: String,
    #[serde(default)]
    description: String,
    /// `"http"`, read by `action_from_value` to choose this form.
    #[serde(rename = "kind")]
    _kind: String,
    class: ActionClass,
    method: HttpMethod,
    url: String,
    #[serde(default)]
    credential: Option<String>,
    #[serde(default)]
    params: BTreeMap<String, RawParam>,
    #[serde(default)]
    check: Option<String>,
    #[serde(default)]
    timeout_s: Option<i64>,
    #[serde(default)]
    max_output_kib: Option<i64>,
    #[serde(default)]
    auto_max_per_hour: Option<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCredential {
    name: String,
    hosts: Vec<String>,
    header: String,
    scheme: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAction {
    name: String,
    #[serde(default)]
    description: String,
    class: ActionClass,
    #[serde(default)]
    runner: Runner,
    #[serde(default)]
    distro: Option<String>,
    program: String,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    params: BTreeMap<String, RawParam>,
    #[serde(default)]
    check: Option<String>,
    #[serde(default)]
    timeout_s: Option<i64>,
    #[serde(default)]
    max_output_kib: Option<i64>,
    #[serde(default)]
    forwarded_values: Option<ForwardedValues>,
    #[serde(default)]
    auto_max_per_hour: Option<i64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawParam {
    #[serde(default)]
    values: Option<Vec<String>>,
    #[serde(default)]
    pattern: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTemplate {
    name: String,
    description: String,
    projects: Vec<String>,
    prompt: String,
    #[serde(default)]
    params: BTreeMap<String, RawParam>,
    #[serde(default)]
    model: Option<String>,
}

/// `[orchestrator]` and `[auto_review]`: an optional model each (AD-75).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawModelTable {
    #[serde(default)]
    model: Option<String>,
}

/// Why a `guarded_programs` entry is refused (D-3), if it is.
fn guarded_program_problem(entry: &str) -> Option<&'static str> {
    let stem = entry.strip_suffix('*').unwrap_or(entry);
    if stem.is_empty() {
        Some("an entry names a program: it cannot be empty or only `*`")
    } else if stem.contains('*') {
        Some("`*` is allowed only as the last character")
    } else {
        None
    }
}

/// True when a floor asks something of the relay: an action (valid or not)
/// or a guarded project. A floor without either, any schema 1 floor among
/// them, has nothing to approve, and the window keeps no relay record for it.
///
/// ```
/// let v0_1 = officina_core::floor::parse_floor(
///     "schema = 1\n[[project]]\nname = \"api\"\npath = \"C:/work/api\"\n",
/// ).unwrap();
/// assert!(!officina_core::relay::catalogue::relay_in_use(&v0_1));
/// let guarded = officina_core::floor::parse_floor(
///     "schema = 2\n[[project]]\nname = \"api\"\npath = \"C:/work/api\"\n",
/// ).unwrap();
/// assert!(officina_core::relay::catalogue::relay_in_use(&guarded));
/// ```
pub fn relay_in_use(floor: &FloorFile) -> bool {
    !floor.actions.is_empty()
        || floor
            .projects
            .iter()
            .any(|p| floor.autonomy_of(p) == Autonomy::Guarded)
}

/// The effective guarded programs of a floor: its list, or the default one,
/// lowercase (program names are matched lowercase, AD-27), sorted and
/// deduplicated, entries breaking D-3 left out. Cheap: the gate calls it on
/// every shell tool call (phase 1, AD-28).
pub fn guarded_programs(floor: &FloorFile) -> Vec<String> {
    let mut list: Vec<String> = match &floor.guarded_programs {
        Some(entries) => entries
            .iter()
            .filter(|e| guarded_program_problem(e).is_none())
            .map(|e| e.to_lowercase())
            .collect(),
        None => DEFAULT_GUARDED_PROGRAMS
            .iter()
            .map(|p| (*p).to_owned())
            .collect(),
    };
    list.sort();
    list.dedup();
    list
}

/// A lowercase letter, then lowercase letters, digits and `extra`, at most
/// [`MAX_NAME_CHARS`] in all.
fn is_lower_name(name: &str, extra: &[char]) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && name.chars().count() <= MAX_NAME_CHARS
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || extra.contains(&c))
}

/// An action or credential name: [`NAME_RULE`] (AD-23).
pub fn is_valid_action_name(name: &str) -> bool {
    is_lower_name(name, &['-'])
}

/// A parameter name: [`PARAM_NAME_RULE`], so it can be passed as
/// `name=value` and read back from a placeholder.
pub fn is_valid_param_name(name: &str) -> bool {
    is_lower_name(name, &['-', '_'])
}

/// An `env` key: `^[A-Za-z_][A-Za-z0-9_]{0,63}$` (D-1).
pub fn is_valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && key.len() <= MAX_ENV_KEY_CHARS
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A `distro`: `^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$` (D-4).
pub fn is_valid_distro(distro: &str) -> bool {
    let mut chars = distro.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && distro.len() <= MAX_DISTRO_CHARS
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// A program that `cmd.exe` would re-parse (D-5). Windows ignores trailing
/// dots and spaces in file names, so `run.bat.` is still a batch file.
fn is_batch_program(program: &str) -> bool {
    let trimmed = program.trim_end_matches(['.', ' ']).to_ascii_lowercase();
    trimmed.ends_with(".bat") || trimmed.ends_with(".cmd")
}

/// The `kind` of an HTTP action (AD-36). A process action has no `kind`.
pub const HTTP_KIND: &str = "http";

/// The keys of a process action an HTTP action never declares (AD-36).
pub const PROCESS_KEYS: &[&str] = &["runner", "distro", "program", "env", "args"];

/// The headers a credential may not name (D-40): they frame the request,
/// and Officina never sends a cookie.
pub const FORBIDDEN_CREDENTIAL_HEADERS: &[&str] = &[
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "cookie",
];

/// An HTTP token (RFC 9110 `token`): what a header name and an
/// authentication scheme are. No space, no CR or LF.
///
/// ```
/// use officina_core::relay::catalogue::is_http_token;
/// assert!(is_http_token("X-Api-Key") && is_http_token("Bearer"));
/// assert!(!is_http_token("") && !is_http_token("Bad Header") && !is_http_token("a:b"));
/// ```
pub fn is_http_token(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c))
}

/// The issues of one entry, all carrying its table and name.
struct Report<'a> {
    table: IssueTable,
    name: &'a str,
    issues: Vec<ActionIssue>,
}

impl Report<'_> {
    fn new(table: IssueTable, name: &str) -> Report<'_> {
        Report {
            table,
            name,
            issues: Vec::new(),
        }
    }

    fn issue(&self, kind: IssueKind, message: String) -> ActionIssue {
        ActionIssue {
            table: self.table,
            name: self.name.to_owned(),
            line: None,
            kind,
            message,
        }
    }

    fn add(&mut self, kind: IssueKind, message: String) {
        let issue = self.issue(kind, message);
        self.issues.push(issue);
    }
}

/// The `name` an entry declares, for its issues: [`UNNAMED`] without one.
fn name_of(value: &toml::Value) -> String {
    value
        .get("name")
        .and_then(toml::Value::as_str)
        .unwrap_or(UNNAMED)
        .to_owned()
}

/// `value` as its raw table, or the one `malformed` issue saying why it
/// does not convert.
fn parse_raw<T: serde::de::DeserializeOwned>(
    value: &toml::Value,
    report: &Report<'_>,
) -> Result<T, Vec<ActionIssue>> {
    value.clone().try_into().map_err(|e: toml::de::Error| {
        vec![report.issue(IssueKind::Malformed, e.message().trim().to_owned())]
    })
}

/// The names given more than once: every copy of each is dropped, so none
/// wins by its place in the file. One rule for actions, credentials and
/// projects.
fn duplicated<'n>(names: impl IntoIterator<Item = &'n str>) -> BTreeSet<&'n str> {
    let mut seen = BTreeSet::new();
    names
        .into_iter()
        .filter(|name| !seen.insert(*name))
        .collect()
}

/// What every action declares besides how it runs.
struct Common<'a> {
    name: &'a str,
    class: ActionClass,
    params: &'a BTreeMap<String, RawParam>,
    check: Option<&'a str>,
    timeout_s: Option<i64>,
    max_output_kib: Option<i64>,
    auto_max_per_hour: Option<i64>,
}

/// What the rules of every action leave: the parameters, the limits.
struct Checked {
    params: BTreeMap<String, Param>,
    timeout_s: u32,
    max_output_kib: u32,
    auto_max_per_hour: Option<u32>,
}

/// The rules every action follows, whatever runs it: its name, its
/// parameters (each value passing the rules of `target`), every placeholder
/// in `used` declared and every parameter used, its limits, its `check` and
/// its hourly cap.
fn check_common(
    report: &mut Report<'_>,
    common: &Common<'_>,
    target: ValueTarget,
    used: &BTreeSet<&str>,
) -> Checked {
    if !is_valid_action_name(common.name) {
        report.add(IssueKind::Name, format!("the name must be {NAME_RULE}"));
    }
    let params = check_params(report, common.params, target);
    check_placeholders(report, common.params, used);
    let (timeout_s, max_output_kib) = check_limits(report, common);
    check_check(report, common);
    let auto_max_per_hour = check_auto_max_per_hour(report, common);
    Checked {
        params,
        timeout_s,
        max_output_kib,
        auto_max_per_hour,
    }
}

/// `auto_max_per_hour`: an `auto` action's, in [`AUTO_MAX_PER_HOUR_RANGE`],
/// [`DEFAULT_AUTO_MAX_PER_HOUR`] when absent; no other class sets it.
fn check_auto_max_per_hour(report: &mut Report<'_>, common: &Common<'_>) -> Option<u32> {
    if common.class != ActionClass::Auto {
        if common.auto_max_per_hour.is_some() {
            report.add(
                IssueKind::AutoMaxPerHour,
                "only an `auto` action sets `auto_max_per_hour`".to_owned(),
            );
        }
        return None;
    }
    let cap = common
        .auto_max_per_hour
        .unwrap_or(i64::from(DEFAULT_AUTO_MAX_PER_HOUR));
    if !AUTO_MAX_PER_HOUR_RANGE.contains(&cap) {
        report.add(
            IssueKind::AutoMaxPerHour,
            format!(
                "`auto_max_per_hour` must be {} to {}",
                AUTO_MAX_PER_HOUR_RANGE.start(),
                AUTO_MAX_PER_HOUR_RANGE.end()
            ),
        );
    }
    Some(u32::try_from(cap).unwrap_or(DEFAULT_AUTO_MAX_PER_HOUR))
}

/// Each parameter's name follows [`PARAM_NAME_RULE`] and its rule holds
/// ([`param_rule`]); a parameter without a usable rule is left out.
fn check_params(
    report: &mut Report<'_>,
    raw_params: &BTreeMap<String, RawParam>,
    target: ValueTarget,
) -> BTreeMap<String, Param> {
    let mut checked = BTreeMap::new();
    for (param_name, raw_param) in raw_params {
        if !is_valid_param_name(param_name) {
            report.add(
                IssueKind::ParamName,
                format!("parameter {param_name:?} must be {PARAM_NAME_RULE}"),
            );
        }
        if let Some(rule) = param_rule(report, param_name, raw_param, target) {
            checked.insert(param_name.clone(), Param { rule });
        }
    }
    checked
}

/// Exactly one of a `values` list, each value passing the rules of
/// `target`, or a `pattern` that compiles within its limits.
fn param_rule(
    report: &mut Report<'_>,
    param_name: &str,
    raw_param: &RawParam,
    target: ValueTarget,
) -> Option<ParamRule> {
    match (&raw_param.values, &raw_param.pattern) {
        (Some(values), None) => {
            for value in values {
                if let Err(refusal) = params::check_value_for(target, value) {
                    report.add(
                        IssueKind::InvalidValue,
                        format!("value {value:?} of parameter {param_name}: {refusal}"),
                    );
                }
            }
            let mut sorted = values.clone();
            sorted.sort();
            sorted.dedup();
            Some(ParamRule::Values(sorted))
        }
        (None, Some(source)) => match params::compile_pattern(source) {
            Ok(pattern) => Some(ParamRule::Pattern(pattern)),
            Err(error) => {
                let kind = match error {
                    PatternError::TooLong => IssueKind::PatternTooLong,
                    PatternError::TooComplex => IssueKind::PatternTooComplex,
                    PatternError::Invalid(_) => IssueKind::PatternInvalid,
                };
                report.add(kind, format!("parameter {param_name}: {error}"));
                None
            }
        },
        _ => {
            report.add(
                IssueKind::ValuesOrPattern,
                format!("parameter {param_name} declares exactly one of `values` or `pattern`"),
            );
            None
        }
    }
}

/// Every placeholder in `used` names a declared parameter, and every
/// parameter is used by a placeholder.
fn check_placeholders(
    report: &mut Report<'_>,
    raw_params: &BTreeMap<String, RawParam>,
    used: &BTreeSet<&str>,
) {
    for placeholder in used {
        if !raw_params.contains_key(*placeholder) {
            report.add(
                IssueKind::UndeclaredPlaceholder,
                format!("placeholder {{{placeholder}}} names no declared parameter"),
            );
        }
    }
    for param_name in raw_params.keys() {
        if !used.contains(param_name.as_str()) {
            report.add(
                IssueKind::UnusedParameter,
                format!("parameter {param_name} is used by no placeholder"),
            );
        }
    }
}

/// `timeout_s` in [`TIMEOUT_RANGE_S`] and `max_output_kib` in
/// [`MAX_OUTPUT_RANGE_KIB`], each defaulted when absent.
fn check_limits(report: &mut Report<'_>, common: &Common<'_>) -> (u32, u32) {
    let timeout_s = common.timeout_s.unwrap_or(i64::from(DEFAULT_TIMEOUT_S));
    if !TIMEOUT_RANGE_S.contains(&timeout_s) {
        report.add(
            IssueKind::Timeout,
            format!(
                "`timeout_s` must be {} to {}",
                TIMEOUT_RANGE_S.start(),
                TIMEOUT_RANGE_S.end()
            ),
        );
    }
    let max_output_kib = common
        .max_output_kib
        .unwrap_or(i64::from(DEFAULT_MAX_OUTPUT_KIB));
    if !MAX_OUTPUT_RANGE_KIB.contains(&max_output_kib) {
        report.add(
            IssueKind::MaxOutput,
            format!(
                "`max_output_kib` must be {} to {}",
                MAX_OUTPUT_RANGE_KIB.start(),
                MAX_OUTPUT_RANGE_KIB.end()
            ),
        );
    }
    (
        u32::try_from(timeout_s).unwrap_or(DEFAULT_TIMEOUT_S),
        u32::try_from(max_output_kib).unwrap_or(DEFAULT_MAX_OUTPUT_KIB),
    )
}

/// An `after-check` action needs its check; so does an `auto` one, so an
/// unattended apply is always preceded by its dry run (AD-180, D-570); a
/// `human` action may name one, its dry run (D-170); a `free` action runs
/// without any, so naming one would promise a binding it never has.
fn check_check(report: &mut Report<'_>, common: &Common<'_>) {
    match (common.class, common.check) {
        (ActionClass::AfterCheck, None) => report.add(
            IssueKind::Check,
            "an `after-check` action names its check action in `check`".to_owned(),
        ),
        (ActionClass::Auto, None) => report.add(
            IssueKind::AutoWithoutCheck,
            "an `auto` action names its dry run in `check`: an unattended apply always runs after its check".to_owned(),
        ),
        (ActionClass::AfterCheck | ActionClass::Human | ActionClass::Auto, Some(_))
        | (ActionClass::Free | ActionClass::Human, None) => {}
        (ActionClass::Free, Some(_)) => report.add(
            IssueKind::Check,
            "a `free` action names no `check`: only `after-check`, `human` and `auto` actions run after one".to_owned(),
        ),
    }
}

/// Converts and validates one action alone: every rule that needs no other
/// action. The issues carry no line; [`validate`] adds it. An action with
/// `kind = "http"` is an HTTP action (AD-36); any other `kind` is malformed.
///
/// ```
/// let value: toml::Value = toml::from_str(
///     "name = \"probe\"\nclass = \"free\"\nprogram = \"C:/path/to/tool.exe\"\n",
/// ).unwrap();
/// let action = officina_core::relay::catalogue::action_from_value(&value).unwrap();
/// assert_eq!(action.timeout_s, officina_core::relay::catalogue::DEFAULT_TIMEOUT_S);
/// ```
pub fn action_from_value(value: &toml::Value) -> Result<Action, Vec<ActionIssue>> {
    let name = name_of(value);
    let mut report = Report::new(IssueTable::Action, &name);
    match value.get("kind") {
        None => {}
        Some(kind) if kind.as_str() == Some(HTTP_KIND) => {
            return http_action_from_value(value, report);
        }
        Some(_) => {
            return Err(vec![report.issue(
                IssueKind::Malformed,
                format!("`kind` is \"{HTTP_KIND}\" for an HTTP action and absent otherwise"),
            )]);
        }
    }
    let raw: RawAction = parse_raw(value, &report)?;
    check_program(&mut report, &raw);
    if let Some(distro) = &raw.distro {
        check_distro(&mut report, raw.runner, distro);
    }
    check_env(&mut report, &raw.env);
    let args = arg_templates(&mut report, &raw.args);
    let used: BTreeSet<&str> = args.iter().filter_map(ArgTemplate::param).collect();
    let common = Common {
        name: &raw.name,
        class: raw.class,
        params: &raw.params,
        check: raw.check.as_deref(),
        timeout_s: raw.timeout_s,
        max_output_kib: raw.max_output_kib,
        auto_max_per_hour: raw.auto_max_per_hour,
    };
    let checked = check_common(&mut report, &common, ValueTarget::from(raw.runner), &used);
    if raw.forwarded_values != Some(ForwardedValues::Accepted) {
        check_forwarded(&mut report, &raw, &checked.params);
    }

    if !report.issues.is_empty() {
        return Err(report.issues);
    }
    Ok(Action {
        name: raw.name,
        description: raw.description,
        class: raw.class,
        runner: raw.runner,
        distro: raw.distro,
        program: raw.program,
        env: raw.env,
        args,
        http: None,
        params: checked.params,
        check: raw.check,
        timeout_s: checked.timeout_s,
        max_output_kib: checked.max_output_kib,
        forwarded_values: raw.forwarded_values,
        auto_max_per_hour: checked.auto_max_per_hour,
    })
}

/// `program`: a fixed path, absolute for its runner, never a batch file
/// `cmd.exe` would parse again (D-5).
fn check_program(report: &mut Report<'_>, raw: &RawAction) {
    if raw.program.contains(['{', '}']) {
        report.add(
            IssueKind::ProgramPlaceholder,
            "`program` is a fixed path: it cannot hold a placeholder".to_owned(),
        );
    }
    let absolute = match raw.runner {
        Runner::Native => is_windows_absolute(&raw.program),
        Runner::Wsl => raw.program.starts_with('/'),
    };
    if !absolute {
        report.add(
            IssueKind::ProgramNotAbsolute,
            match raw.runner {
                Runner::Native => {
                    "`program` must be a Windows absolute path, like C:/path/to/tool.exe"
                }
                Runner::Wsl => "`program` must be a Linux absolute path, like /usr/bin/tool",
            }
            .to_owned(),
        );
    }
    if raw.runner == Runner::Native && is_batch_program(&raw.program) {
        report.add(
            IssueKind::BatchProgram,
            "a `.bat` or `.cmd` program is re-parsed by cmd.exe: run its interpreter's real program instead".to_owned(),
        );
    }
}

/// `distro`: for the `wsl` runner only, and a name WSL accepts (D-4).
fn check_distro(report: &mut Report<'_>, runner: Runner, distro: &str) {
    if runner != Runner::Wsl {
        report.add(
            IssueKind::DistroWithoutWsl,
            "`distro` applies to `runner = \"wsl\"` only".to_owned(),
        );
    }
    if !is_valid_distro(distro) {
        report.add(
            IssueKind::Distro,
            format!(
                "`distro` must start with a letter or digit, then letters, digits, `.`, `_` or `-`, at most {MAX_DISTRO_CHARS} characters"
            ),
        );
    }
}

/// `env`: each key a variable name (D-1), each value a literal.
fn check_env(report: &mut Report<'_>, env: &BTreeMap<String, String>) {
    for (key, value) in env {
        if !is_valid_env_key(key) {
            report.add(
                IssueKind::EnvKey,
                format!(
                    "`env` key {key:?} must be a letter or `_`, then letters, digits or `_`, at most {MAX_ENV_KEY_CHARS} characters"
                ),
            );
        }
        if value.contains(['{', '}']) {
            report.add(
                IssueKind::EnvValuePlaceholder,
                format!(
                    "`env` value of {key} holds a brace: env values are literals, never placeholders"
                ),
            );
        }
    }
}

/// `args`: each item a literal or one whole placeholder.
fn arg_templates(report: &mut Report<'_>, items: &[String]) -> Vec<ArgTemplate> {
    let mut args = Vec::new();
    for item in items {
        match ArgTemplate::parse(item) {
            Ok(template) => args.push(template),
            Err(error) => report.add(IssueKind::Braces, error.to_string()),
        }
    }
    args
}

/// A parameter the program forwards to a remote shell or a template
/// (D-172) has a `values` list, or a pattern that admits none of
/// [`FORWARDED_PROBE_CHARS`] (D-173).
fn check_forwarded(report: &mut Report<'_>, raw: &RawAction, params: &BTreeMap<String, Param>) {
    for (kind, item, param) in forwarded_items(&raw.program, &raw.args) {
        let Some(ParamRule::Pattern(pattern)) = params.get(&param).map(|p| &p.rule) else {
            continue;
        };
        if let Some(admitted) = admitted_forwarded_char(pattern) {
            report.add(
                IssueKind::ForwardedPattern,
                format!(
                    "parameter {param} (args item {item}) is read by {}, and its pattern admits {admitted:?}: give it a `values` list, or set `forwarded_values = \"accepted\"` to keep the action",
                    kind.program()
                ),
            );
        }
    }
}

/// [`action_from_value`] for `kind = "http"` (AD-36): `method`, `url` and an
/// optional `credential`, never a process key.
fn http_action_from_value(
    value: &toml::Value,
    mut report: Report<'_>,
) -> Result<Action, Vec<ActionIssue>> {
    if let Some(issue) = process_keys_issue(value, &report) {
        return Err(vec![issue]);
    }
    let raw: RawHttpAction = parse_raw(value, &report)?;
    let template = url_template(&mut report, &raw);
    let used = template
        .as_ref()
        .map(UrlTemplate::placeholders)
        .unwrap_or_default();
    let common = Common {
        name: &raw.name,
        class: raw.class,
        params: &raw.params,
        check: raw.check.as_deref(),
        timeout_s: raw.timeout_s,
        max_output_kib: raw.max_output_kib,
        auto_max_per_hour: raw.auto_max_per_hour,
    };
    let checked = check_common(&mut report, &common, ValueTarget::Http, &used);
    if raw.class == ActionClass::Auto {
        report.add(
            IssueKind::AutoOnHttp,
            "an HTTP action is `free`, `after-check` or `human`: its requests stay on the author's approval path, never `auto`".to_owned(),
        );
    }

    let Some(url) = template else {
        return Err(report.issues);
    };
    if !report.issues.is_empty() {
        return Err(report.issues);
    }
    Ok(Action {
        name: raw.name,
        description: raw.description,
        class: raw.class,
        runner: Runner::Native,
        distro: None,
        program: String::new(),
        env: BTreeMap::new(),
        args: Vec::new(),
        http: Some(HttpSpec {
            method: raw.method,
            url,
            credential: raw.credential,
        }),
        params: checked.params,
        check: raw.check,
        timeout_s: checked.timeout_s,
        max_output_kib: checked.max_output_kib,
        forwarded_values: None,
        auto_max_per_hour: checked.auto_max_per_hour,
    })
}

/// The issue of an HTTP action that declares a process key, if it does.
fn process_keys_issue(value: &toml::Value, report: &Report<'_>) -> Option<ActionIssue> {
    let process_keys: Vec<&str> = PROCESS_KEYS
        .iter()
        .copied()
        .filter(|key| value.get(key).is_some())
        .collect();
    (!process_keys.is_empty()).then(|| {
        report.issue(
            IssueKind::HttpProgramFields,
            format!(
                "an HTTP action declares `method` and `url`, never {}",
                process_keys
                    .iter()
                    .map(|key| format!("`{key}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )
    })
}

/// `url`: a template the rules of AD-36 accept, over `https` when the
/// action sends a credential.
fn url_template(report: &mut Report<'_>, raw: &RawHttpAction) -> Option<UrlTemplate> {
    let template = match UrlTemplate::parse(&raw.url) {
        Ok(template) => Some(template),
        Err(issue) => {
            let kind = match issue {
                UrlIssue::Userinfo => IssueKind::UrlUserinfo,
                _ => IssueKind::Url,
            };
            report.add(kind, format!("`url`: {issue}"));
            None
        }
    };
    if let (Some(template), Some(_)) = (&template, &raw.credential)
        && template.scheme() == UrlScheme::Http
    {
        report.add(
            IssueKind::CredentialOverHttp,
            "an action with a credential uses an `https` URL: plain `http` would send the secret in clear".to_owned(),
        );
    }
    template
}

/// Converts and validates one `[[credential]]` alone (AD-36, D-37, D-40).
fn credential_from_value(value: &toml::Value) -> Result<Credential, Vec<ActionIssue>> {
    let name = name_of(value);
    let mut report = Report::new(IssueTable::Credential, &name);
    let raw: RawCredential = parse_raw(value, &report)?;
    if !is_valid_action_name(&raw.name) {
        report.add(
            IssueKind::CredentialName,
            format!("a credential's name must be {NAME_RULE}"),
        );
    }
    let hosts = credential_hosts(&mut report, &raw.hosts);
    check_header(&mut report, &raw.header);
    check_scheme(&mut report, &raw.scheme);
    if !report.issues.is_empty() {
        return Err(report.issues);
    }
    Ok(Credential {
        name: raw.name,
        hosts,
        header: raw.header,
        scheme: raw.scheme,
    })
}

/// `hosts`: at least one, each a host name or address without a port or a
/// wildcard; sorted, deduplicated.
fn credential_hosts(report: &mut Report<'_>, entries: &[String]) -> Vec<String> {
    let mut hosts = BTreeSet::new();
    for entry in entries {
        match host_entry(entry) {
            Some(host) => {
                hosts.insert(host);
            }
            None => report.add(
                IssueKind::CredentialHosts,
                format!("host {entry:?} is not a host name or address without a port; no wildcard"),
            ),
        }
    }
    if entries.is_empty() {
        report.add(
            IssueKind::CredentialHosts,
            "`hosts` names at least one host the credential may be sent to".to_owned(),
        );
    }
    hosts.into_iter().collect()
}

/// `header`: an HTTP token, never one of [`FORBIDDEN_CREDENTIAL_HEADERS`].
fn check_header(report: &mut Report<'_>, header: &str) {
    let forbidden = FORBIDDEN_CREDENTIAL_HEADERS
        .iter()
        .any(|h| h.eq_ignore_ascii_case(header));
    if !is_http_token(header) || forbidden {
        report.add(
            IssueKind::CredentialHeader,
            if forbidden {
                format!("`header` cannot be {header}: it frames the request")
            } else {
                "`header` is a header name: letters, digits and `!#$%&'*+-.^_`|~`, no space"
                    .to_owned()
            },
        );
    }
}

/// `scheme`: one HTTP token.
fn check_scheme(report: &mut Report<'_>, scheme: &str) {
    if !is_http_token(scheme) {
        report.add(
            IssueKind::CredentialScheme,
            "`scheme` is one word, like Bearer or Basic: letters, digits and `!#$%&'*+-.^_`|~`"
                .to_owned(),
        );
    }
}

/// 1-based line of a byte offset.
fn line_at(content: &str, offset: usize) -> u32 {
    let end = offset.min(content.len());
    let newlines = content.as_bytes()[..end]
        .iter()
        .filter(|b| **b == b'\n')
        .count();
    u32::try_from(newlines + 1).unwrap_or(u32::MAX)
}

/// The line a spanned value starts on, when it was found.
fn line_of(content: &str, spanned: Option<&toml::Spanned<toml::Value>>) -> Option<u32> {
    spanned.map(|s| line_at(content, s.span().start))
}

/// Where the floor's projects and `guarded_programs` entries start. Phase 1
/// keeps them as plain values for its callers; phase 2 reads their positions
/// with a second, lenient parse of the same text.
#[derive(Deserialize, Default)]
struct Positions {
    #[serde(default)]
    guarded_programs: Vec<toml::Spanned<toml::Value>>,
    #[serde(default)]
    project: Vec<toml::Spanned<toml::Value>>,
}

/// Phase 2 of AD-23 and AD-75: converts and validates each action alone,
/// then the rules across actions (unique names, `check` targets), then the
/// projects' action lists, then each template against the projects, then
/// the `[orchestrator]` and `[auto_review]` tables. `content` is the text
/// `floor` was parsed from, for lines.
pub fn validate(floor: &FloorFile, content: &str) -> Catalogue {
    let positions: Positions = toml::from_str(content).unwrap_or_default();
    let mut issues = validate_guarded_programs(floor, &positions, content);
    let credentials = validate_credentials(floor, content, &mut issues);
    let actions = validate_actions(floor, content, &credentials, &mut issues);
    let projects = validate_projects(floor, &positions, content, &actions, &mut issues);
    let templates = validate_templates(floor, content, &projects, &mut issues);
    let orchestrator_model = model_table(
        floor.orchestrator.as_ref(),
        IssueTable::Orchestrator,
        content,
        &mut issues,
    )
    .unwrap_or_default();
    let auto_review_model = model_table(
        floor.auto_review.as_ref(),
        IssueTable::AutoReview,
        content,
        &mut issues,
    )
    .map(|model| model.unwrap_or(DEFAULT_REVIEW_MODEL));
    issues.sort_by_key(|issue| issue.line.unwrap_or(u32::MAX));
    Catalogue {
        guarded_programs: guarded_programs(floor),
        projects,
        actions,
        credentials,
        templates,
        orchestrator_model,
        auto_review_model,
        issues,
    }
}

/// `[orchestrator]` or `[auto_review]` (AD-75): `Some(model)` when the
/// table is absent (no model) or valid, `None` with its issue when it
/// fails.
fn model_table(
    table: Option<&toml::Spanned<toml::Value>>,
    kind: IssueTable,
    content: &str,
    issues: &mut Vec<ActionIssue>,
) -> Option<Option<ModelAlias>> {
    let Some(table) = table else {
        return Some(None);
    };
    let report = Report::new(kind, kind.code());
    let line = Some(line_at(content, table.span().start));
    let found = parse_raw::<RawModelTable>(table.get_ref(), &report).and_then(|raw| {
        raw.model
            .map(|model| parse_model(&report, &model))
            .transpose()
            .map_err(|issue| vec![issue])
    });
    match found {
        Ok(model) => Some(model),
        Err(found) => {
            issues.extend(found.into_iter().map(|issue| ActionIssue { line, ..issue }));
            None
        }
    }
}

/// A `model` as a [`ModelAlias`], or the issue naming the aliases.
fn parse_model(report: &Report<'_>, model: &str) -> Result<ModelAlias, ActionIssue> {
    ModelAlias::from_code(model).ok_or_else(|| {
        let aliases: Vec<&str> = ModelAlias::ALL.iter().map(|m| m.code()).collect();
        report.issue(
            IssueKind::Model,
            format!("`model` is one of {}, not {model:?}", aliases.join(", ")),
        )
    })
}

/// Converts and validates each `[[template]]` alone, drops every copy of a
/// duplicated name, then checks its projects against the catalogue's: an
/// unknown project leaves the template, which is dropped once none is left.
fn validate_templates(
    floor: &FloorFile,
    content: &str,
    projects: &[CatalogueProject],
    issues: &mut Vec<ActionIssue>,
) -> BTreeMap<String, Template> {
    let duplicates = duplicated(
        floor
            .templates
            .iter()
            .filter_map(|raw| raw.get_ref().get("name").and_then(toml::Value::as_str)),
    );
    let known: BTreeSet<&str> = projects.iter().map(|p| p.name.as_str()).collect();
    let mut templates = BTreeMap::new();
    for raw in &floor.templates {
        let line = Some(line_at(content, raw.span().start));
        let at_line = |issue: ActionIssue| ActionIssue { line, ..issue };
        let mut template = match template_from_value(raw.get_ref()) {
            Ok(template) => template,
            Err(found) => {
                issues.extend(found.into_iter().map(at_line));
                continue;
            }
        };
        let report = Report::new(IssueTable::Template, &template.name);
        if duplicates.contains(template.name.as_str()) {
            issues.push(at_line(report.issue(
                IssueKind::DuplicateName,
                format!(
                    "another template is also named {}: both are dropped",
                    template.name
                ),
            )));
            continue;
        }
        let (kept, unknown): (Vec<String>, Vec<String>) = template
            .projects
            .iter()
            .cloned()
            .partition(|p| known.contains(p.as_str()));
        for project in &unknown {
            issues.push(at_line(report.issue(
                IssueKind::UnknownTemplateProject,
                format!("project {project} is not in the catalogue: the template leaves it"),
            )));
        }
        if kept.is_empty() {
            issues.push(at_line(report.issue(
                IssueKind::TemplateProjects,
                "no project of the template is in the catalogue: it is dropped".to_owned(),
            )));
            continue;
        }
        template.projects = kept;
        templates.insert(template.name.clone(), template);
    }
    templates
}

/// Converts and validates one `[[template]]` alone (AD-75): every rule
/// that needs no other table. Its projects are checked by
/// [`validate_templates`]; the issues carry no line.
fn template_from_value(value: &toml::Value) -> Result<Template, Vec<ActionIssue>> {
    let name = name_of(value);
    let mut report = Report::new(IssueTable::Template, &name);
    let raw: RawTemplate = parse_raw(value, &report)?;
    if !is_valid_action_name(&raw.name) {
        report.add(IssueKind::Name, format!("the name must be {NAME_RULE}"));
    }
    check_description(&mut report, &raw.description);
    if raw.projects.is_empty() {
        report.add(
            IssueKind::TemplateProjects,
            "`projects` names at least one catalogue project".to_owned(),
        );
    }
    let params = template_params(&mut report, &raw.params);
    check_prompt(&mut report, &raw.prompt, &raw.params, &params);
    let model = raw
        .model
        .as_deref()
        .map(|model| parse_model(&report, model))
        .transpose()
        .unwrap_or_else(|issue| {
            report.issues.push(issue);
            None
        });
    if !report.issues.is_empty() {
        return Err(report.issues);
    }
    let mut projects = raw.projects;
    projects.sort();
    projects.dedup();
    Ok(Template {
        name: raw.name,
        description: raw.description,
        projects,
        prompt: raw.prompt,
        params,
        model,
    })
}

/// A control or formatting character, other than those `allowed`.
fn hidden_character(text: &str, allowed: &[char]) -> Option<char> {
    text.chars()
        .find(|c| (c.is_control() && !allowed.contains(c)) || params::is_forbidden_format(*c))
}

/// `description`: [`DESCRIPTION_CHARS`] long, one readable line.
fn check_description(report: &mut Report<'_>, description: &str) {
    let length = description.chars().count();
    if !DESCRIPTION_CHARS.contains(&length) {
        report.add(
            IssueKind::TemplateDescription,
            format!(
                "`description` is {} to {} characters",
                DESCRIPTION_CHARS.start(),
                DESCRIPTION_CHARS.end()
            ),
        );
    }
    if let Some(c) = hidden_character(description, &[]) {
        report.add(
            IssueKind::TemplateDescription,
            format!(
                "`description` holds the control or formatting character U+{:04X}",
                u32::from(c)
            ),
        );
    }
}

/// Each parameter: a valid name, `values` only, at least one, each passing
/// [`params::template_value`]; sorted and deduplicated. A parameter without
/// a usable list is left out.
fn template_params(
    report: &mut Report<'_>,
    raw_params: &BTreeMap<String, RawParam>,
) -> BTreeMap<String, Vec<String>> {
    let mut checked = BTreeMap::new();
    for (param_name, raw_param) in raw_params {
        if !is_valid_param_name(param_name) {
            report.add(
                IssueKind::ParamName,
                format!("parameter {param_name:?} must be {PARAM_NAME_RULE}"),
            );
        }
        if raw_param.pattern.is_some() {
            report.add(
                IssueKind::TemplatePattern,
                format!(
                    "parameter {param_name}: a template takes a `values` list only, never a `pattern`"
                ),
            );
        }
        let values = raw_param.values.as_deref().unwrap_or_default();
        if values.is_empty() {
            report.add(
                IssueKind::TemplateValues,
                format!("parameter {param_name} lists at least one value in `values`"),
            );
            continue;
        }
        for value in values {
            if let Err(refusal) = params::template_value(value) {
                report.add(
                    IssueKind::InvalidValue,
                    format!("value {value:?} of parameter {param_name}: {refusal}"),
                );
            }
        }
        let mut sorted = values.to_vec();
        sorted.sort();
        sorted.dedup();
        checked.insert(param_name.clone(), sorted);
    }
    checked
}

/// `prompt` (AD-75, D-2, D-159): readable characters, author text first,
/// braces delimiting placeholders that name declared parameters, every
/// parameter used, and a longest rendering within [`MAX_PROMPT_CHARS`].
fn check_prompt(
    report: &mut Report<'_>,
    prompt: &str,
    raw_params: &BTreeMap<String, RawParam>,
    params: &BTreeMap<String, Vec<String>>,
) {
    // A floor saved with Windows line ends may keep them in a multi-line
    // string: a carriage return before a newline is part of that newline.
    if let Some(c) = hidden_character(&prompt.replace("\r\n", "\n"), PROMPT_CONTROL_ALLOWED) {
        report.add(
            IssueKind::TemplatePromptCharacter,
            format!(
                "the prompt holds the control or formatting character U+{:04X}; only newline and tab are allowed",
                u32::from(c)
            ),
        );
    }
    let start = prompt.trim_start().chars().next();
    let starts_badly = match start {
        None => Some("the prompt holds only whitespace".to_owned()),
        Some('{') => Some(
            "the prompt starts with a placeholder: its first character after leading whitespace is the author's text".to_owned(),
        ),
        Some(c) if FORBIDDEN_PROMPT_STARTS.contains(&c) => Some(format!(
            "the prompt starts with `{c}`, which Claude Code would read as a command, a shell escape or an option"
        )),
        Some(_) => None,
    };
    if let Some(message) = starts_badly {
        report.add(IssueKind::TemplatePromptStart, message);
    }
    let found = match pieces(prompt) {
        Ok(found) => found,
        Err(error) => {
            report.add(IssueKind::Braces, format!("the prompt: {error}"));
            return;
        }
    };
    let used: BTreeSet<&str> = found
        .iter()
        .filter_map(|piece| match piece {
            Piece::Placeholder(name) => Some(*name),
            Piece::Text(_) => None,
        })
        .collect();
    check_placeholders(report, raw_params, &used);
    if let Some(longest) = worst_case_chars(prompt, params)
        && longest > MAX_PROMPT_CHARS
    {
        report.add(
            IssueKind::TemplatePromptTooLong,
            format!(
                "the longest rendering is {longest} characters, over {MAX_PROMPT_CHARS}: shorten the prompt or the longest values"
            ),
        );
    }
}

/// An entry of `guarded_programs` the gate cannot match is ignored, and
/// said.
fn validate_guarded_programs(
    floor: &FloorFile,
    positions: &Positions,
    content: &str,
) -> Vec<ActionIssue> {
    floor
        .guarded_programs
        .iter()
        .flatten()
        .enumerate()
        .filter_map(|(index, entry)| {
            let problem = guarded_program_problem(entry)?;
            Some(ActionIssue {
                table: IssueTable::GuardedPrograms,
                name: "guarded_programs".to_owned(),
                line: line_of(content, positions.guarded_programs.get(index)),
                kind: IssueKind::GuardedProgram,
                message: format!("entry {entry:?} is ignored: {problem}"),
            })
        })
        .collect()
}

/// Converts and validates each `[[credential]]` alone, then drops every
/// copy of a duplicated name. Issues carry the credential's line.
fn validate_credentials(
    floor: &FloorFile,
    content: &str,
    issues: &mut Vec<ActionIssue>,
) -> BTreeMap<String, Credential> {
    let duplicates = duplicated(
        floor
            .credentials
            .iter()
            .filter_map(|raw| raw.get_ref().get("name").and_then(toml::Value::as_str)),
    );
    let mut credentials = BTreeMap::new();
    for raw in &floor.credentials {
        let line = line_at(content, raw.span().start);
        match credential_from_value(raw.get_ref()) {
            Ok(credential) if duplicates.contains(credential.name.as_str()) => {
                issues.push(ActionIssue {
                    table: IssueTable::Credential,
                    name: credential.name.clone(),
                    line: Some(line),
                    kind: IssueKind::DuplicateCredential,
                    message: format!(
                        "another credential is also named {}: both are dropped",
                        credential.name
                    ),
                });
            }
            Ok(credential) => {
                credentials.insert(credential.name.clone(), credential);
            }
            Err(found) => issues.extend(found.into_iter().map(|issue| ActionIssue {
                line: Some(line),
                ..issue
            })),
        }
    }
    credentials
}

/// Converts and validates each `[[action]]` alone, drops every copy of a
/// duplicated name, then the actions whose credential (D-37) or `check`
/// target (AD-23) does not hold across the catalogue.
fn validate_actions(
    floor: &FloorFile,
    content: &str,
    credentials: &BTreeMap<String, Credential>,
    issues: &mut Vec<ActionIssue>,
) -> BTreeMap<String, Action> {
    let duplicates = duplicated(
        floor
            .actions
            .iter()
            .filter_map(|raw| raw.get_ref().get("name").and_then(toml::Value::as_str)),
    );
    let mut candidates: BTreeMap<String, (Action, u32)> = BTreeMap::new();
    for raw in &floor.actions {
        let line = line_at(content, raw.span().start);
        match action_from_value(raw.get_ref()) {
            Ok(action) if duplicates.contains(action.name.as_str()) => {
                issues.push(ActionIssue {
                    table: IssueTable::Action,
                    name: action.name.clone(),
                    line: Some(line),
                    kind: IssueKind::DuplicateName,
                    message: format!(
                        "another action is also named {}: both are dropped",
                        action.name
                    ),
                });
            }
            Ok(action) => {
                candidates.insert(action.name.clone(), (action, line));
            }
            Err(found) => issues.extend(found.into_iter().map(|issue| ActionIssue {
                line: Some(line),
                ..issue
            })),
        }
    }

    // Credentials first, so a check target dropped for its credential is
    // unknown to the actions naming it.
    candidates.retain(|name, (action, line)| {
        let Some((kind, message)) = credential_problem(action, credentials) else {
            return true;
        };
        issues.push(ActionIssue {
            table: IssueTable::Action,
            name: name.clone(),
            line: Some(*line),
            kind,
            message,
        });
        false
    });
    let mut actions = BTreeMap::new();
    for (name, (action, line)) in &candidates {
        if let Some(problem) = check_target_problem(action, &candidates) {
            issues.push(ActionIssue {
                table: IssueTable::Action,
                name: name.clone(),
                line: Some(*line),
                kind: IssueKind::Check,
                message: problem,
            });
        } else {
            actions.insert(name.clone(), action.clone());
        }
    }
    actions
}

/// The projects of the catalogue: every copy of a duplicated name leaves
/// it (the floor keeps them for the board and the gate), and each keeps
/// the actions of its list that are valid, the others said.
fn validate_projects(
    floor: &FloorFile,
    positions: &Positions,
    content: &str,
    actions: &BTreeMap<String, Action>,
    issues: &mut Vec<ActionIssue>,
) -> Vec<CatalogueProject> {
    let duplicates = duplicated(floor.projects.iter().map(|project| project.name.as_str()));
    let mut projects = Vec::new();
    for (index, project) in floor.projects.iter().enumerate() {
        let line = line_of(content, positions.project.get(index));
        if duplicates.contains(project.name.as_str()) {
            issues.push(ActionIssue {
                table: IssueTable::Project,
                name: project.name.clone(),
                line,
                kind: IssueKind::DuplicateProject,
                message: format!(
                    "another project is also named {}: both leave the catalogue",
                    project.name
                ),
            });
            continue;
        }
        let mut names = BTreeSet::new();
        for action in project.actions.iter().flatten() {
            if actions.contains_key(action) {
                names.insert(action.clone());
            } else {
                issues.push(ActionIssue {
                    table: IssueTable::Project,
                    name: project.name.clone(),
                    line,
                    kind: IssueKind::UnknownProjectAction,
                    message: format!(
                        "project {} names the action {action}, which is unknown or was dropped",
                        project.name
                    ),
                });
            }
        }
        projects.push(CatalogueProject {
            name: project.name.clone(),
            path: project.path.clone(),
            autonomy: floor.autonomy_of(project),
            actions: names.into_iter().collect(),
        });
    }
    projects.sort_by(|a, b| a.name.cmp(&b.name));
    projects
}

/// Why an HTTP action's credential does not hold across the catalogue, if
/// it does not: it must name a valid credential whose `hosts` hold the
/// URL's host (D-37).
fn credential_problem(
    action: &Action,
    credentials: &BTreeMap<String, Credential>,
) -> Option<(IssueKind, String)> {
    let spec = action.http.as_ref()?;
    let name = spec.credential.as_deref()?;
    let Some(credential) = credentials.get(name) else {
        return Some((
            IssueKind::UnknownCredential,
            format!("`credential` names {name}, which is not declared or was dropped"),
        ));
    };
    if !super::http::host_is_bound(&credential.hosts, spec.url.host()) {
        return Some((
            IssueKind::CredentialHostNotBound,
            format!(
                "the URL's host {} is not in the `hosts` of credential {name}: the secret goes to those hosts only",
                spec.url.host()
            ),
        ));
    }
    None
}

/// Why an action's `check` does not hold across the catalogue, if it does
/// not: it must name a valid `free` action with the same parameter names
/// (AD-23).
fn check_target_problem(
    action: &Action,
    candidates: &BTreeMap<String, (Action, u32)>,
) -> Option<String> {
    let check = action.check.as_deref()?;
    let Some((target, _)) = candidates.get(check) else {
        return Some(format!(
            "`check` names {check}, which is unknown or was dropped"
        ));
    };
    if target.class != ActionClass::Free {
        return Some(format!(
            "`check` names {check}, which is not a `free` action"
        ));
    }
    if !target.params.keys().eq(action.params.keys()) {
        return Some(format!(
            "`check` names {check}, whose parameter names differ from this action's"
        ));
    }
    None
}

/// The canonical form of one parameter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum CanonicalParam {
    /// Sorted, deduplicated.
    Values(Vec<String>),
    Pattern(String),
}

/// The canonical form of what an HTTP action sends (AD-36).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CanonicalHttp {
    pub method: HttpMethod,
    /// The URL template as the floor wrote it.
    pub url: String,
    pub credential: Option<String>,
}

/// The canonical form of one valid action, every default explicit. An HTTP
/// action carries `http` and the process fields' defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CanonicalAction {
    pub description: String,
    pub class: ActionClass,
    pub runner: Runner,
    pub distro: Option<String>,
    pub program: String,
    pub env: BTreeMap<String, String>,
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub http: Option<CanonicalHttp>,
    pub params: BTreeMap<String, CanonicalParam>,
    pub check: Option<String>,
    pub timeout_s: u32,
    pub max_output_kib: u32,
    /// Absent unless the author accepted forwarded values (D-173).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub forwarded_values: Option<ForwardedValues>,
    /// An `auto` action's hourly cap, the default explicit; absent for
    /// every other class (AD-76).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub auto_max_per_hour: Option<u32>,
}

/// The canonical form of one valid template (AD-76): projects and values
/// sorted, the prompt as written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CanonicalTemplate {
    pub description: String,
    pub projects: Vec<String>,
    pub prompt: String,
    /// Each parameter's closed list.
    pub params: BTreeMap<String, Vec<String>>,
    pub model: Option<ModelAlias>,
}

impl From<&Template> for CanonicalTemplate {
    fn from(template: &Template) -> CanonicalTemplate {
        CanonicalTemplate {
            description: template.description.clone(),
            projects: template.projects.clone(),
            prompt: template.prompt.clone(),
            params: template.params.clone(),
            model: template.model,
        }
    }
}

/// The canonical form of `[auto_review]` (AD-76): the review's model,
/// [`DEFAULT_REVIEW_MODEL`] when the table is absent; `null` when the
/// table fails phase 2, so an approval records that every `auto` request
/// escalates (AD-75, AD-87).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CanonicalAutoReview {
    pub model: Option<ModelAlias>,
}

impl Default for CanonicalAutoReview {
    fn default() -> CanonicalAutoReview {
        CanonicalAutoReview {
            model: Some(DEFAULT_REVIEW_MODEL),
        }
    }
}

/// The canonical form of one valid credential (D-39): hosts lowercase,
/// sorted, deduplicated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CanonicalCredential {
    pub hosts: Vec<String>,
    pub header: String,
    pub scheme: String,
}

impl From<&Action> for CanonicalAction {
    fn from(action: &Action) -> CanonicalAction {
        CanonicalAction {
            description: action.description.clone(),
            class: action.class,
            runner: action.runner,
            distro: action.distro.clone(),
            program: action.program.clone(),
            env: action.env.clone(),
            args: action.args.iter().map(ArgTemplate::source).collect(),
            http: action.http.as_ref().map(|spec| CanonicalHttp {
                method: spec.method,
                url: spec.url.source().to_owned(),
                credential: spec.credential.clone(),
            }),
            params: action
                .params
                .iter()
                .map(|(name, param)| {
                    let canonical = match &param.rule {
                        ParamRule::Values(values) => CanonicalParam::Values(values.clone()),
                        ParamRule::Pattern(pattern) => {
                            CanonicalParam::Pattern(pattern.source().to_owned())
                        }
                    };
                    (name.clone(), canonical)
                })
                .collect(),
            check: action.check.clone(),
            timeout_s: action.timeout_s,
            max_output_kib: action.max_output_kib,
            forwarded_values: action.forwarded_values,
            auto_max_per_hour: action.auto_max_per_hour,
        }
    }
}

/// The canonical form of one project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CanonicalProject {
    /// Resolved against the floor and normalised.
    pub path: String,
    pub autonomy: Autonomy,
    /// Sorted.
    pub actions: Vec<String>,
}

/// What the author approves (AD-24): the catalogue's meaning, not its text.
/// Colours, the floor's `name`, `config_dir`, `no_excerpts`, comments,
/// invalid actions and templates, and `[orchestrator]`, which approves and
/// launches nothing (AD-76), are not part of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CanonicalCatalogue {
    pub format: u32,
    /// Normalised (lowercase, forward slashes) so two spellings of one floor
    /// hash alike. Never opened: the approved record keeps the path as read
    /// in [`ApprovedCatalogue::floor_file`].
    pub floor_path: String,
    pub guarded_programs: Vec<String>,
    pub projects: BTreeMap<String, CanonicalProject>,
    pub actions: BTreeMap<String, CanonicalAction>,
    /// Absent from snapshots of format 1, which read as none: their hash
    /// differs anyway, so they are approved again (D-39).
    #[serde(default)]
    pub credentials: BTreeMap<String, CanonicalCredential>,
    /// Absent from snapshots before format 4, which read as none (AD-76,
    /// D-131).
    #[serde(default)]
    pub templates: BTreeMap<String, CanonicalTemplate>,
    /// Absent from snapshots before format 4, which read as the default.
    #[serde(default)]
    pub auto_review: CanonicalAutoReview,
}

/// The canonical form of a validated catalogue read from `floor_path`.
pub fn canonical(catalogue: &Catalogue, floor_path: &str) -> CanonicalCatalogue {
    let floor_dir = floor_dir_of(floor_path);
    CanonicalCatalogue {
        format: CANONICAL_FORMAT,
        floor_path: normalise_path(floor_path),
        guarded_programs: catalogue.guarded_programs.clone(),
        projects: catalogue
            .projects
            .iter()
            .map(|p| {
                let project = CanonicalProject {
                    path: resolve_project_path(floor_dir, &p.path),
                    autonomy: p.autonomy,
                    actions: p.actions.clone(),
                };
                (p.name.clone(), project)
            })
            .collect(),
        actions: catalogue
            .actions
            .iter()
            .map(|(name, action)| (name.clone(), CanonicalAction::from(action)))
            .collect(),
        credentials: catalogue
            .credentials
            .iter()
            .map(|(name, credential)| {
                let canonical = CanonicalCredential {
                    hosts: credential.hosts.clone(),
                    header: credential.header.clone(),
                    scheme: credential.scheme.clone(),
                };
                (name.clone(), canonical)
            })
            .collect(),
        templates: catalogue
            .templates
            .iter()
            .map(|(name, template)| (name.clone(), CanonicalTemplate::from(template)))
            .collect(),
        auto_review: CanonicalAutoReview {
            model: catalogue.auto_review_model,
        },
    }
}

/// The credentials of an approved snapshot as validated credentials, so a
/// request is rebuilt from the approved catalogue alone (D-27). A snapshot
/// entry that no longer validates is left out: no action can bind it.
pub fn credentials_from_canonical(canonical: &CanonicalCatalogue) -> BTreeMap<String, Credential> {
    canonical
        .credentials
        .iter()
        .filter_map(|(name, credential)| {
            let mut table = toml::Table::new();
            table.insert("name".to_owned(), name.clone().into());
            table.insert(
                "hosts".to_owned(),
                toml::Value::Array(credential.hosts.iter().cloned().map(Into::into).collect()),
            );
            table.insert("header".to_owned(), credential.header.clone().into());
            table.insert("scheme".to_owned(), credential.scheme.clone().into());
            let credential = credential_from_value(&toml::Value::Table(table)).ok()?;
            Some((name.clone(), credential))
        })
        .collect()
}

/// SHA-256 of `serde_json::to_vec` of the canonical form, lowercase hex
/// (AD-24).
pub fn hash(canonical: &CanonicalCatalogue) -> String {
    // Plain structs and string-keyed maps always serialize.
    let bytes = serde_json::to_vec(canonical).unwrap_or_default();
    super::sha256_hex(&bytes)
}

/// A floor read for the relay: both phases, the canonical form and its hash.
#[derive(Debug, Clone)]
pub struct RelayFloor {
    pub floor: FloorFile,
    pub catalogue: Catalogue,
    pub canonical: CanonicalCatalogue,
    pub hash: String,
}

/// Runs phase 1 and phase 2 on a floor's content and computes its canonical
/// form and hash. Only phase 1 can fail.
pub fn load(content: &str, floor_path: &str) -> Result<RelayFloor, FloorError> {
    let floor = parse_floor(content)?;
    let catalogue = validate(&floor, content);
    let canonical = canonical(&catalogue, floor_path);
    let hash = hash(&canonical);
    Ok(RelayFloor {
        floor,
        catalogue,
        canonical,
        hash,
    })
}

/// The validated action an approved canonical action stands for, so a
/// command can be rebuilt from the approved snapshot alone (D-27): the
/// canonical form keeps every field as the floor declares it, and goes
/// through the same conversion and rules as a floor's action.
pub fn action_from_canonical(
    name: &str,
    canonical: &CanonicalAction,
) -> Result<Action, Vec<ActionIssue>> {
    let mut fields = match serde_json::to_value(canonical) {
        Ok(serde_json::Value::Object(fields)) => fields,
        _ => serde_json::Map::new(),
    };
    // An HTTP action is written back as the floor declares one: `kind`, its
    // request's fields at the top, no process key.
    if let Some(serde_json::Value::Object(http)) = fields.remove("http") {
        for key in PROCESS_KEYS {
            fields.remove(*key);
        }
        fields.insert("kind".to_owned(), HTTP_KIND.into());
        fields.extend(http);
    }
    fields.retain(|_, value| !value.is_null());
    fields.insert("name".to_owned(), name.into());
    let value = toml::Value::try_from(serde_json::Value::Object(fields)).map_err(|e| {
        vec![ActionIssue {
            table: IssueTable::Action,
            name: name.to_owned(),
            line: None,
            kind: IssueKind::Malformed,
            message: e.to_string(),
        }]
    })?;
    action_from_value(&value)
}

/// Each project's folder as the floor declares it, by name: the floor's
/// directory joined to a relative path, an absolute one as written (D-6).
/// What `officina run` starts in, unlike the lowercased canonical path.
pub fn project_dirs(floor: &FloorFile, floor_path: &str) -> BTreeMap<String, String> {
    let floor_dir = floor_dir_of(floor_path);
    floor
        .projects
        .iter()
        .map(|p| (p.name.clone(), join_project_path(floor_dir, &p.path)))
        .collect()
}

/// One changed field, as JSON so any field type renders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FieldChange {
    pub field: String,
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub before: serde_json::Value,
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub after: serde_json::Value,
}

/// One entry present on both sides with different content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EntryChange {
    pub name: String,
    pub fields: Vec<FieldChange>,
}

/// Entries added, removed and changed, by name.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SectionDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<EntryChange>,
}

impl SectionDiff {
    fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}

/// Entries added to and removed from a list.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ListDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

/// What changed between the approved catalogue and the current one (AD-24).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CatalogueDiff {
    pub format: Option<FieldChange>,
    pub floor_path: Option<FieldChange>,
    pub guarded_programs: ListDiff,
    pub projects: SectionDiff,
    pub actions: SectionDiff,
    /// Credentials added, removed and changed per field (D-39).
    pub credentials: SectionDiff,
    /// Templates added, removed and changed per field (AD-76).
    pub templates: SectionDiff,
    /// The review's model, `field` being `model` (AD-76).
    pub auto_review: Option<FieldChange>,
}

impl CatalogueDiff {
    /// True when both sides mean the same thing.
    pub fn is_empty(&self) -> bool {
        self.format.is_none()
            && self.floor_path.is_none()
            && self.guarded_programs.added.is_empty()
            && self.guarded_programs.removed.is_empty()
            && self.projects.is_empty()
            && self.actions.is_empty()
            && self.credentials.is_empty()
            && self.templates.is_empty()
            && self.auto_review.is_none()
    }
}

fn scalar_change<T: Serialize + PartialEq>(
    field: &str,
    before: &T,
    after: &T,
) -> Option<FieldChange> {
    (before != after).then(|| FieldChange {
        field: field.to_owned(),
        before: serde_json::to_value(before).unwrap_or_default(),
        after: serde_json::to_value(after).unwrap_or_default(),
    })
}

/// Field by field over the serialized entries, so a field added to an entry
/// type is diffed without touching this function.
fn section_diff<T: Serialize + PartialEq>(
    before: &BTreeMap<String, T>,
    after: &BTreeMap<String, T>,
) -> SectionDiff {
    let mut diff = SectionDiff::default();
    for (name, old) in before {
        let Some(new) = after.get(name) else {
            diff.removed.push(name.clone());
            continue;
        };
        if old == new {
            continue;
        }
        let old_value = serde_json::to_value(old).unwrap_or_default();
        let new_value = serde_json::to_value(new).unwrap_or_default();
        let empty = serde_json::Map::new();
        let old_fields = old_value.as_object().unwrap_or(&empty);
        let new_fields = new_value.as_object().unwrap_or(&empty);
        let keys: BTreeSet<&String> = old_fields.keys().chain(new_fields.keys()).collect();
        let fields = keys
            .into_iter()
            .filter_map(|key| {
                let before = old_fields.get(key).cloned().unwrap_or_default();
                let after = new_fields.get(key).cloned().unwrap_or_default();
                (before != after).then(|| FieldChange {
                    field: key.clone(),
                    before,
                    after,
                })
            })
            .collect();
        diff.changed.push(EntryChange {
            name: name.clone(),
            fields,
        });
    }
    diff.added = after
        .keys()
        .filter(|name| !before.contains_key(*name))
        .cloned()
        .collect();
    diff
}

/// Compares the approved catalogue with the current one.
pub fn diff(approved: &CanonicalCatalogue, current: &CanonicalCatalogue) -> CatalogueDiff {
    let old: BTreeSet<&String> = approved.guarded_programs.iter().collect();
    let new: BTreeSet<&String> = current.guarded_programs.iter().collect();
    CatalogueDiff {
        format: scalar_change("format", &approved.format, &current.format),
        floor_path: scalar_change("floor_path", &approved.floor_path, &current.floor_path),
        guarded_programs: ListDiff {
            added: new.difference(&old).map(|p| (*p).clone()).collect(),
            removed: old.difference(&new).map(|p| (*p).clone()).collect(),
        },
        projects: section_diff(&approved.projects, &current.projects),
        actions: section_diff(&approved.actions, &current.actions),
        credentials: section_diff(&approved.credentials, &current.credentials),
        templates: section_diff(&approved.templates, &current.templates),
        auto_review: scalar_change(
            "model",
            &approved.auto_review.model,
            &current.auto_review.model,
        ),
    }
}

/// Content of `relay/approved.json`, written by the window only (AD-24,
/// AD-25).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovedCatalogue {
    pub approved_at_ms: i64,
    pub hash: String,
    /// The floor's path exactly as the approval read it, which the relay
    /// floor resolution opens (AD-24). Absent from records written before
    /// it existed; see [`ApprovedCatalogue::floor_file`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floor_file: Option<String>,
    pub canonical: CanonicalCatalogue,
}

impl ApprovedCatalogue {
    /// The record of approving `relay`, read at `floor_file`, at
    /// `approved_at_ms`: what the window writes, and what a test approves
    /// without it.
    pub fn approving(
        relay: &RelayFloor,
        floor_file: &str,
        approved_at_ms: i64,
    ) -> ApprovedCatalogue {
        ApprovedCatalogue {
            approved_at_ms,
            hash: relay.hash.clone(),
            floor_file: Some(floor_file.to_owned()),
            canonical: relay.canonical.clone(),
        }
    }

    /// The path of the approved floor: as read at approval, or for an older
    /// record the canonical path, which only a case-insensitive file system
    /// opens.
    pub fn floor_file(&self) -> &str {
        self.floor_file
            .as_deref()
            .unwrap_or(&self.canonical.floor_path)
    }
}

/// Content of `relay/known-projects.json` (D-15): the last readable state
/// the gate falls back to when both the floor and the snapshot fail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownProjects {
    pub schema: u32,
    /// The hash of the last approved catalogue, if any was approved.
    pub approved_hash: Option<String>,
    /// Every project path the floor declares, resolved and normalised,
    /// sorted and deduplicated.
    pub project_paths: Vec<String>,
}

impl KnownProjects {
    /// The record for a floor read from `floor_path`. Every declared project
    /// counts, valid actions or not: the record only ever widens the gate.
    pub fn from_floor(
        floor: &FloorFile,
        floor_path: &str,
        approved_hash: Option<String>,
    ) -> KnownProjects {
        let floor_dir = floor_dir_of(floor_path);
        let paths: BTreeSet<String> = floor
            .projects
            .iter()
            .map(|p| resolve_project_path(floor_dir, &p.path))
            .collect();
        KnownProjects {
            schema: KNOWN_PROJECTS_SCHEMA,
            approved_hash,
            project_paths: paths.into_iter().collect(),
        }
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What a program does with a value that a closed argument vector does
    /// not close (D-172): a notice, not an issue, since a `values` list
    /// keeps such a parameter safe.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum NoticeKind {
        /// An ssh action's placeholder after the destination: ssh joins
        /// those arguments into one command the remote shell parses.
        RemoteCommandValue,
        /// An Ansible action's placeholder in extra vars or module
        /// arguments, which Ansible templates.
        TemplatedValue,
    }
}

impl NoticeKind {
    /// The program family that reads the value.
    pub fn program(self) -> &'static str {
        match self {
            NoticeKind::RemoteCommandValue => "ssh",
            NoticeKind::TemplatedValue => "Ansible",
        }
    }

    /// One line for the author, the same on the banner, in
    /// `officina relay --json` and in doctor.
    pub fn explanation(self) -> &'static str {
        match self {
            NoticeKind::RemoteCommandValue => {
                "ssh hands this value to the remote shell, which reads it: a pattern admitting spaces or shell characters lets a session run any command on that host; prefer a `values` list"
            }
            NoticeKind::TemplatedValue => {
                "Ansible templates this value: a pattern admitting `{` or `%` lets a session run code on the control node; prefer a `values` list"
            }
        }
    }
}

/// One notice of the catalogue (D-172), derived from its canonical form: it
/// never changes the hash and never drops the action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Notice {
    pub action: String,
    pub kind: NoticeKind,
    pub param: String,
    /// 0-based index of the `args` item holding the placeholder.
    pub item: u32,
    /// [`NoticeKind::explanation`].
    pub explanation: String,
}

/// ssh options that take a value, so the next item is not the destination
/// (OpenSSH's `ssh(1)` synopsis).
pub const SSH_OPTIONS_WITH_VALUE: &[&str] = &[
    "-B", "-b", "-c", "-D", "-E", "-e", "-F", "-I", "-i", "-J", "-L", "-l", "-m", "-O", "-o", "-P",
    "-p", "-Q", "-R", "-S", "-W", "-w",
];

/// The Ansible options whose value Ansible templates: the next item holds
/// it, or the item itself after `=` for a long option.
pub const ANSIBLE_TEMPLATED_OPTIONS: &[&str] = &["-e", "--extra-vars", "-a", "--args"];

/// A program's file name: the last segment, lowercase, `.exe` stripped.
fn program_name(program: &str) -> String {
    let last = program.rsplit(['/', '\\']).next().unwrap_or(program);
    let lower = last.to_ascii_lowercase();
    lower.strip_suffix(".exe").unwrap_or(&lower).to_owned()
}

/// The parameters a process action's program reads past the argument
/// vector (D-172): what reads them, the index of the `args` item, and the
/// parameter.
fn forwarded_items(program: &str, args: &[String]) -> Vec<(NoticeKind, usize, String)> {
    let program = program_name(program);
    let items: Vec<Option<String>> = args
        .iter()
        .map(|item| {
            ArgTemplate::parse(item)
                .ok()
                .and_then(|t| t.param().map(str::to_owned))
        })
        .collect();
    let mut found = Vec::new();
    if program == "ssh" {
        let mut destination = None;
        let mut index = 0;
        while index < args.len() {
            let item = &args[index];
            if item == "--" {
                destination = Some(index + 1);
                break;
            }
            if item.starts_with('-') {
                if SSH_OPTIONS_WITH_VALUE.contains(&item.as_str()) {
                    index += 1;
                }
                index += 1;
                continue;
            }
            destination = Some(index);
            break;
        }
        if let Some(destination) = destination {
            for (index, param) in items.iter().enumerate().skip(destination + 1) {
                if let Some(param) = param {
                    found.push((NoticeKind::RemoteCommandValue, index, param.clone()));
                }
            }
        }
    } else if program.starts_with("ansible") {
        for (index, param) in items.iter().enumerate() {
            let Some(param) = param else { continue };
            let item = &args[index];
            let after_option =
                index > 0 && ANSIBLE_TEMPLATED_OPTIONS.contains(&args[index - 1].as_str());
            let inline = ANSIBLE_TEMPLATED_OPTIONS
                .iter()
                .filter(|option| option.starts_with("--"))
                .any(|option| item.starts_with(&format!("{option}=")));
            if after_option || inline {
                found.push((NoticeKind::TemplatedValue, index, param.clone()));
            }
        }
    }
    found
}

/// The notices of one canonical action (D-172).
fn action_notices(name: &str, action: &CanonicalAction) -> Vec<Notice> {
    if action.http.is_some() {
        return Vec::new();
    }
    forwarded_items(&action.program, &action.args)
        .into_iter()
        .map(|(kind, index, param)| Notice {
            action: name.to_owned(),
            kind,
            param,
            item: u32::try_from(index).unwrap_or(u32::MAX),
            explanation: kind.explanation().to_owned(),
        })
        .collect()
}

/// What a value forwarded to a remote shell or a template must not hold for
/// a `pattern` to stand without `forwarded_values = "accepted"` (D-173):
/// whitespace, the characters a POSIX shell reads, and the ones that open a
/// Jinja template. Control characters are refused in every value already.
pub const FORWARDED_PROBE_CHARS: &str = " ;&|$`'\"\\<>()*?[]{}~#!%";

/// Short values a probe builds on, between a pattern's literal edges.
const PROBE_FILLERS: &[&str] = &["", "a", "0", "aa", "00", "a0", "a-0", "aaa", "000"];

/// The literal text a pattern starts and ends with, escapes read: what the
/// probe of [`admitted_forwarded_char`] wraps its fillers in, so a pattern
/// such as `lab-[a-z ]+` or `.+\.service` is probed inside its shape.
fn literal_edges(source: &str) -> (String, String) {
    let mut prefix = String::new();
    let mut prefix_open = true;
    let mut run = String::new();
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        let literal = match c {
            '\\' => match chars.next() {
                Some(next) if next.is_ascii_punctuation() => Some(next),
                _ => None,
            },
            '.' | '^' | '$' | '|' | '(' | ')' | '[' | ']' | '{' | '}' => None,
            '*' | '+' | '?' => {
                // A quantifier makes the character before it optional or
                // repeated: neither edge may hold it.
                run.pop();
                if prefix_open {
                    prefix.pop();
                }
                None
            }
            other => Some(other),
        };
        match literal {
            Some(c) => {
                if prefix_open {
                    prefix.push(c);
                }
                run.push(c);
            }
            None => {
                prefix_open = false;
                run.clear();
            }
        }
    }
    (prefix, run)
}

/// What a padded probe repeats to reach a pattern's lengths: a letter, a
/// digit and a capital, one of which most value classes take.
const PROBE_PADS: &[char] = &['a', '0', 'A'];

/// A run of one pad character between a pattern's literal edges, at the
/// shortest and the longest lengths the pattern accepts within
/// [`params::VALUE_LENGTH`] (D-175).
struct PaddedShape<'e> {
    prefix: &'e str,
    suffix: &'e str,
    pad: char,
    /// Pad counts to probe at: the shortest accepted, one more, and the
    /// longest accepted.
    counts: Vec<usize>,
}

impl PaddedShape<'_> {
    /// True when `pattern` accepts `c` in place of a pad character, or
    /// inserted beside one, at any position of a run of each count.
    fn admits(&self, pattern: &Pattern, c: char) -> bool {
        let (prefix, suffix) = (self.prefix, self.suffix);
        self.counts.iter().any(|&count| {
            // ASCII pads: any byte index is a character boundary.
            let run = self.pad.to_string().repeat(count);
            (0..=count).any(|i| {
                let inserted = format!("{prefix}{}{c}{}{suffix}", &run[..i], &run[i..]);
                let replaced = i < count
                    && pattern.matches(&format!(
                        "{prefix}{}{c}{}{suffix}",
                        &run[..i],
                        &run[i + 1..]
                    ));
                replaced || pattern.matches(&inserted)
            })
        })
    }
}

/// The padded shapes of `pattern` (D-175): for each pad character and each
/// way of wrapping it in the literal edges, the pad counts the pattern
/// accepts, so a pattern that needs a long value (`.{5,}`, `\S{6,}`) is
/// probed at a length it takes. A shape the pattern never accepts is left
/// out.
fn padded_shapes<'e>(pattern: &Pattern, prefix: &'e str, suffix: &'e str) -> Vec<PaddedShape<'e>> {
    let mut edges = vec![("", ""), (prefix, ""), ("", suffix), (prefix, suffix)];
    edges.sort_unstable();
    edges.dedup();
    let longest = *params::VALUE_LENGTH.end();
    let mut shapes = Vec::new();
    for (prefix, suffix) in edges {
        for &pad in PROBE_PADS {
            let pad_text = pad.to_string();
            let accepted: Vec<usize> = (0..=longest)
                .filter(|&count| {
                    pattern.matches(&format!("{prefix}{}{suffix}", pad_text.repeat(count)))
                })
                .collect();
            let (Some(&shortest), Some(&longest_accepted)) = (accepted.first(), accepted.last())
            else {
                continue;
            };
            let mut counts = vec![shortest, shortest + 1, longest_accepted];
            counts.sort_unstable();
            counts.dedup();
            shapes.push(PaddedShape {
                prefix,
                suffix,
                pad,
                counts,
            });
        }
    }
    shapes
}

/// A character of [`FORWARDED_PROBE_CHARS`] that `pattern` admits in a
/// value, found by probing: each character alone and around the fillers,
/// then inserted at every place of each short value the pattern accepts,
/// those values built from its literal edges and the fillers, then at
/// every position of a run of pad characters as short and as long as the
/// pattern accepts ([`padded_shapes`], D-175). A probe, not a proof: a
/// `values` list is the only closed form.
pub fn admitted_forwarded_char(pattern: &Pattern) -> Option<char> {
    let (prefix, suffix) = literal_edges(pattern.source());
    let accepted: Vec<String> = PROBE_FILLERS
        .iter()
        .flat_map(|filler| {
            [
                (*filler).to_owned(),
                format!("{prefix}{filler}"),
                format!("{filler}{suffix}"),
                format!("{prefix}{filler}{suffix}"),
            ]
        })
        .filter(|value| pattern.matches(value))
        .collect();
    let shapes = padded_shapes(pattern, &prefix, &suffix);
    FORWARDED_PROBE_CHARS.chars().find(|&c| {
        let around = PROBE_FILLERS
            .iter()
            .any(|f| pattern.matches(&format!("{f}{c}")) || pattern.matches(&format!("{c}{f}")));
        around
            || accepted.iter().any(|value| {
                value
                    .char_indices()
                    .map(|(i, _)| i)
                    .chain(std::iter::once(value.len()))
                    .any(|i| pattern.matches(&format!("{}{c}{}", &value[..i], &value[i..])))
            })
            || shapes.iter().any(|shape| shape.admits(pattern, c))
    })
}

/// Every notice of a catalogue (D-172), by action then item. Derived from
/// the canonical form, so an approved snapshot and a pending floor give
/// theirs alike, and the hash never covers them.
pub fn notices(catalogue: &CanonicalCatalogue) -> Vec<Notice> {
    catalogue
        .actions
        .iter()
        .flat_map(|(name, action)| action_notices(name, action))
        .collect()
}

/// Parses `relay/known-projects.json`, refusing other schema versions.
pub fn parse_known_projects(content: &str) -> Option<KnownProjects> {
    serde_json::from_str::<KnownProjects>(content)
        .ok()
        .filter(|k| k.schema == KNOWN_PROJECTS_SCHEMA)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECT: &str = "[[project]]\nname = \"lab\"\npath = \"Lab\"\nactions = [\"probe\"]\n";
    const VALID_PROBE: &str = "name = \"probe\"\nclass = \"free\"\nprogram = \"C:/path/to/tool.exe\"\nargs = [\"--host\", \"{host}\"]\n[action.params.host]\nvalues = [\"web01\", \"db01\"]\n";

    const CREDENTIAL: &str = "name = \"monitoring\"\nhosts = [\"monitoring.example.com\"]\nheader = \"Authorization\"\nscheme = \"Bearer\"\n";
    /// A valid HTTP action named like [`VALID_PROBE`], so the project lists
    /// it, sending [`CREDENTIAL`].
    const VALID_HEALTH: &str = "name = \"probe\"\nkind = \"http\"\nclass = \"free\"\nmethod = \"GET\"\nurl = \"https://monitoring.example.com/api/{host}\"\ncredential = \"monitoring\"\n[action.params.host]\nvalues = [\"web01\", \"db01\"]\n";

    /// A valid template for the project of [`PROJECT`].
    const VALID_TEMPLATE: &str = "name = \"bump\"\ndescription = \"Bump one dependency\"\nprojects = [\"lab\"]\nprompt = \"Bump {package} to its latest {level} release.\"\nmodel = \"sonnet\"\n[template.params.package]\nvalues = [\"serde_json\", \"@scope/pkg\"]\n[template.params.level]\nvalues = [\"patch\", \"minor\"]\n";

    /// [`VALID_TEMPLATE`] with `from` replaced by `to`, which must occur.
    fn template_with(from: &str, to: &str) -> String {
        assert!(VALID_TEMPLATE.contains(from), "{from}");
        VALID_TEMPLATE.replacen(from, to, 1)
    }

    /// [`floor_with`] holding [`VALID_PROBE`], plus the given
    /// `[[template]]` tables.
    fn with_templates(templates: &[&str]) -> String {
        let mut text = floor_with(&[VALID_PROBE]);
        for template in templates {
            text.push_str("\n[[template]]\n");
            text.push_str(template);
        }
        text
    }

    /// An `auto` action naming [`CHECK`] as its dry run.
    fn auto_apply() -> String {
        apply("check").replace("\"after-check\"", "\"auto\"")
    }

    /// [`floor_with`] plus the given `[[credential]]` tables.
    fn with_credentials(actions: &[&str], credentials: &[&str]) -> String {
        let mut text = floor_with(actions);
        for credential in credentials {
            text.push_str("\n[[credential]]\n");
            text.push_str(credential);
        }
        text
    }

    /// A schema 2 floor with one project and the given `[[action]]` tables.
    fn floor_with(actions: &[&str]) -> String {
        let mut text = format!("schema = 2\n{PROJECT}");
        for action in actions {
            text.push_str("\n[[action]]\n");
            text.push_str(action);
        }
        text
    }

    fn validated(content: &str) -> Catalogue {
        validate(&parse_floor(content).unwrap(), content)
    }

    /// Validates `content`, asserts an issue of `kind` and returns the
    /// catalogue for further checks.
    fn with_issue(content: &str, kind: IssueKind) -> Catalogue {
        let catalogue = validated(content);
        assert!(
            catalogue.issues.iter().any(|i| i.kind == kind),
            "expected {kind:?} in {:#?}",
            catalogue.issues
        );
        catalogue
    }

    /// The first issue of `kind`.
    fn issue_of(catalogue: &Catalogue, kind: IssueKind) -> &ActionIssue {
        catalogue
            .issues
            .iter()
            .find(|i| i.kind == kind)
            .unwrap_or_else(|| panic!("no {kind:?} in {:#?}", catalogue.issues))
    }

    /// A schema 2 floor with the given `[[action]]` tables and no project.
    fn bare(actions: &[&str]) -> String {
        floor_with(actions).replace(PROJECT, "")
    }

    /// 1-based line of the first line of `content` equal to `text`.
    fn line_of_text(content: &str, text: &str) -> Option<u32> {
        content
            .lines()
            .position(|line| line == text)
            .map(|index| u32::try_from(index + 1).unwrap())
    }

    /// A floor that fails exactly the rule of `kind`. The match is
    /// exhaustive: a new kind does not compile until it has a fixture.
    fn failing_fixture(kind: IssueKind) -> String {
        let probe = |extra: &str| format!("{VALID_PROBE}{extra}");
        let replaced = |from: &str, to: &str| VALID_PROBE.replace(from, to);
        match kind {
            IssueKind::Malformed => floor_with(&[&replaced(
                "class = \"free\"\n",
                "class = \"free\"\nunknown = 1\n",
            )]),
            IssueKind::HttpProgramFields => with_credentials(
                &[&VALID_HEALTH.replace(
                    "class = \"free\"\n",
                    "class = \"free\"\nprogram = \"C:/x.exe\"\n",
                )],
                &[CREDENTIAL],
            ),
            IssueKind::Url => with_credentials(
                &[&VALID_HEALTH.replace("/api/{host}", "/api/v{host}")],
                &[CREDENTIAL],
            ),
            IssueKind::UrlUserinfo => with_credentials(
                &[&VALID_HEALTH.replace("https://", "https://user01@")],
                &[CREDENTIAL],
            ),
            IssueKind::UnknownCredential => with_credentials(&[VALID_HEALTH], &[]),
            IssueKind::CredentialHostNotBound => with_credentials(
                &[VALID_HEALTH],
                &[&CREDENTIAL.replace("monitoring.example.com", "other.example.com")],
            ),
            IssueKind::CredentialOverHttp => with_credentials(
                &[&VALID_HEALTH.replace("https://", "http://")],
                &[CREDENTIAL],
            ),
            IssueKind::CredentialName => with_credentials(
                &[VALID_HEALTH],
                &[&CREDENTIAL.replace("\"monitoring\"", "\"Monitoring\"")],
            ),
            IssueKind::CredentialHosts => with_credentials(
                &[VALID_HEALTH],
                &[&CREDENTIAL.replace("\"monitoring.example.com\"", "\"*.example.com\"")],
            ),
            IssueKind::CredentialHeader => with_credentials(
                &[VALID_HEALTH],
                &[&CREDENTIAL.replace("Authorization", "Cookie")],
            ),
            IssueKind::CredentialScheme => with_credentials(
                &[VALID_HEALTH],
                &[&CREDENTIAL.replace("\"Bearer\"", "\"Bearer token\"")],
            ),
            IssueKind::DuplicateCredential => {
                with_credentials(&[VALID_HEALTH], &[CREDENTIAL, CREDENTIAL])
            }
            IssueKind::AutoWithoutCheck => {
                bare(&[CHECK, &auto_apply().replace("check = \"check\"\n", "")])
            }
            IssueKind::AutoMaxPerHour => bare(&[
                CHECK,
                &auto_apply().replace(
                    "class = \"auto\"\n",
                    "class = \"auto\"\nauto_max_per_hour = 61\n",
                ),
            ]),
            IssueKind::AutoOnHttp => with_credentials(
                &[&VALID_HEALTH.replace("\"free\"", "\"auto\"")],
                &[CREDENTIAL],
            ),
            IssueKind::TemplateDescription => with_templates(&[&template_with(
                "description = \"Bump one dependency\"",
                "description = \"\"",
            )]),
            IssueKind::TemplateProjects => {
                with_templates(&[&template_with("projects = [\"lab\"]", "projects = []")])
            }
            IssueKind::UnknownTemplateProject => with_templates(&[&template_with(
                "projects = [\"lab\"]",
                "projects = [\"lab\", \"missing\"]",
            )]),
            IssueKind::TemplatePromptCharacter => {
                with_templates(&[&template_with("release.\"", "release.\\u0007\"")])
            }
            IssueKind::TemplatePromptStart => with_templates(&[&template_with(
                "prompt = \"Bump",
                "prompt = \"/compact Bump",
            )]),
            IssueKind::TemplatePromptTooLong => with_templates(&[&template_with(
                "release.\"",
                &format!("release. {}\"", "a".repeat(MAX_PROMPT_CHARS)),
            )]),
            IssueKind::TemplateValues => with_templates(&[&template_with(
                "values = [\"patch\", \"minor\"]",
                "values = []",
            )]),
            IssueKind::TemplatePattern => with_templates(&[&template_with(
                "values = [\"patch\", \"minor\"]",
                "pattern = \"[a-z]+\"",
            )]),
            IssueKind::Model => {
                with_templates(&[&template_with("model = \"sonnet\"", "model = \"gpt\"")])
            }
            IssueKind::Name => floor_with(&[&replaced("\"probe\"", "\"Probe\"")]),
            IssueKind::DuplicateName => floor_with(&[VALID_PROBE, VALID_PROBE]),
            IssueKind::ProgramNotAbsolute => {
                floor_with(&[&replaced("C:/path/to/tool.exe", "tool.exe")])
            }
            IssueKind::ProgramPlaceholder => {
                floor_with(&[&replaced("C:/path/to/tool.exe", "C:/path/{host}/tool.exe")])
            }
            IssueKind::BatchProgram => floor_with(&[&replaced("tool.exe", "run.CMD")]),
            IssueKind::Distro => floor_with(&[&replaced(
                "program = \"C:/path/to/tool.exe\"",
                "runner = \"wsl\"\ndistro = \"-Ubuntu\"\nprogram = \"/usr/bin/tool\"",
            )]),
            IssueKind::DistroWithoutWsl => floor_with(&[&replaced(
                "class = \"free\"\n",
                "class = \"free\"\ndistro = \"Ubuntu\"\n",
            )]),
            IssueKind::EnvKey => floor_with(&[&replaced(
                "class = \"free\"\n",
                "class = \"free\"\nenv = { \"1PATH\" = \"x\" }\n",
            )]),
            IssueKind::EnvValuePlaceholder => floor_with(&[&replaced(
                "class = \"free\"\n",
                "class = \"free\"\nenv = { TARGET = \"{host}\" }\n",
            )]),
            IssueKind::Braces => floor_with(&[&replaced("\"{host}\"", "\"{host}{host}\"")]),
            IssueKind::UndeclaredPlaceholder => floor_with(&[&replaced(
                "[\"--host\", \"{host}\"]",
                "[\"--host\", \"{host}\", \"{port}\"]",
            )]),
            IssueKind::UnusedParameter => {
                floor_with(&[&probe("[action.params.port]\nvalues = [\"22\"]\n")])
            }
            IssueKind::ParamName => floor_with(&[&replaced("host", "Host")]),
            IssueKind::ValuesOrPattern => floor_with(&[&probe("pattern = \"web[0-9]+\"\n")]),
            IssueKind::ForwardedPattern => forwarding("pattern = \".+\"", false),
            IssueKind::InvalidValue => floor_with(&[&replaced("\"db01\"", "\"--db01\"")]),
            IssueKind::PatternTooLong => floor_with(&[&replaced(
                "values = [\"web01\", \"db01\"]",
                &format!(
                    "pattern = \"{}\"",
                    "a".repeat(params::MAX_PATTERN_CHARS + 1)
                ),
            )]),
            IssueKind::PatternTooComplex => floor_with(&[&replaced(
                "values = [\"web01\", \"db01\"]",
                "pattern = \"(?:a{1000}){1000}\"",
            )]),
            IssueKind::PatternInvalid => floor_with(&[&replaced(
                "values = [\"web01\", \"db01\"]",
                "pattern = \"(\"",
            )]),
            IssueKind::Check => floor_with(&[&replaced("\"free\"", "\"after-check\"")]),
            IssueKind::Timeout => floor_with(&[&probe_top("timeout_s = 0\n")]),
            IssueKind::MaxOutput => floor_with(&[&probe_top("max_output_kib = 2048\n")]),
            IssueKind::GuardedProgram => format!(
                "schema = 2\nguarded_programs = [\"ssh\", \"ans*ible\"]\n{PROJECT}\n[[action]]\n{VALID_PROBE}"
            ),
            IssueKind::UnknownProjectAction => {
                floor_with(&[VALID_PROBE]).replace("[\"probe\"]", "[\"probe\", \"missing\"]")
            }
            IssueKind::DuplicateProject => format!("{}{PROJECT}", floor_with(&[VALID_PROBE])),
        }
    }

    /// `VALID_PROBE` with `keys` added among its top-level keys.
    fn probe_top(keys: &str) -> String {
        VALID_PROBE.replace("class = \"free\"\n", &format!("class = \"free\"\n{keys}"))
    }

    /// An ssh action forwarding `{unit}` to the remote shell, and an Ansible
    /// one templating `{vars}`, each parameter declared with `rule`.
    fn forwarding(rule: &str, accepted: bool) -> String {
        let accepted = if accepted {
            "forwarded_values = \"accepted\"\n"
        } else {
            ""
        };
        format!(
            "schema = 2\n[[project]]\nname = \"lab\"\npath = \"Lab\"\nactions = [\"status\", \"deploy\"]\n[[action]]\nname = \"status\"\nclass = \"free\"\nprogram = \"C:/Windows/System32/OpenSSH/ssh.exe\"\nargs = [\"web01\", \"systemctl\", \"status\", \"{{unit}}\"]\n{accepted}[action.params.unit]\n{rule}\n[[action]]\nname = \"deploy\"\nclass = \"human\"\nrunner = \"wsl\"\nprogram = \"/opt/venv/bin/ansible-playbook\"\nargs = [\"-e\", \"{{vars}}\", \"site.yml\"]\n{accepted}[action.params.vars]\n{rule}\n"
        )
    }

    /// `text` as a TOML basic string.
    fn toml_string(text: &str) -> String {
        let escaped = text
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t");
        format!("\"{escaped}\"")
    }

    /// AC 9 of story 2.14 (D-173): a pattern admitting any character of the
    /// probe constant, on a value ssh or Ansible reads, drops the action
    /// with `ForwardedPattern`; one character at a time, from the constant.
    #[test]
    fn a_permissive_pattern_on_a_forwarded_value_is_an_issue() {
        for c in FORWARDED_PROBE_CHARS.chars() {
            let pattern = format!("[a-z0-9{}]+", regex::escape(&c.to_string()));
            let rule = format!("pattern = {}", toml_string(&pattern));
            let catalogue = validated(&forwarding(&rule, false));
            let flagged: Vec<&str> = catalogue
                .issues
                .iter()
                .filter(|i| i.kind == IssueKind::ForwardedPattern)
                .map(|i| i.name.as_str())
                .collect();
            assert_eq!(flagged, ["status", "deploy"], "{c:?}: {catalogue:#?}");
            assert!(catalogue.actions.is_empty(), "{c:?}: both dropped");
            let compiled = params::compile_pattern(&pattern).unwrap();
            assert_eq!(admitted_forwarded_char(&compiled), Some(c), "{pattern}");
        }
        for broad in [
            ".+",
            ".*",
            "\\S+",
            "[^/]+",
            "lab-[a-z ]+",
            ".+\\.service",
            "web[0-9]{2}( .*)?",
            // Story 2.16 AC 6 (D-175): patterns that need a value longer
            // than the fillers, found by the padded probe.
            ".{5,}",
            "\\S{6,}",
            "[a-z]{4}(;[a-z]+)?",
            "[a-z.@-]{5,}(;[a-z ]+)?",
            "[0-9]{8}( .*)?",
            "lab-[a-z]{6,}(;.*)?",
            ".{200,}",
        ] {
            let compiled = params::compile_pattern(broad).unwrap();
            assert!(admitted_forwarded_char(&compiled).is_some(), "{broad}");
        }
        for closed in [
            "[a-z0-9-]+\\.service",
            "lab-[a-z0-9-]{1,30}",
            "[A-Za-z0-9_.:/=,-]+",
            "[a-z]{200,256}",
            "[0-9]{8}",
        ] {
            let compiled = params::compile_pattern(closed).unwrap();
            assert_eq!(admitted_forwarded_char(&compiled), None, "{closed}");
            let catalogue = validated(&forwarding(
                &format!("pattern = {}", toml_string(closed)),
                false,
            ));
            assert!(
                catalogue.issues.is_empty(),
                "{closed}: {:#?}",
                catalogue.issues
            );
        }
    }

    /// AC 9 of story 2.14: a `values` list is closed whatever it holds; the
    /// notice of D-172 still says the program reads the value.
    #[test]
    fn values_on_a_forwarded_value_is_fine() {
        let values: Vec<String> = FORWARDED_PROBE_CHARS
            .chars()
            .map(|c| toml_string(&format!("a{c}b")))
            .collect();
        let catalogue = validated(&forwarding(
            &format!("values = [{}]", values.join(", ")),
            false,
        ));
        assert!(
            catalogue
                .issues
                .iter()
                .all(|i| i.kind != IssueKind::ForwardedPattern),
            "{:#?}",
            catalogue.issues
        );
        assert!(
            catalogue.actions.contains_key("status") && catalogue.actions.contains_key("deploy")
        );
        let relay = load(
            &forwarding("values = [\"nginx.service\"]", false),
            "C:/path/to/officina.toml",
        )
        .unwrap();
        assert_eq!(notices(&relay.canonical).len(), 2);
    }

    /// AC 9 of story 2.14: `forwarded_values = "accepted"` keeps the action
    /// with its pattern; the key is canonical, so accepting changes the
    /// hash, and the notices stay.
    #[test]
    fn forwarded_values_accepted_keeps_the_action() {
        let path = "C:/path/to/officina.toml";
        let accepted = load(&forwarding("pattern = \".+\"", true), path).unwrap();
        assert!(
            accepted.catalogue.issues.is_empty(),
            "{:#?}",
            accepted.catalogue.issues
        );
        for name in ["status", "deploy"] {
            assert_eq!(
                accepted.canonical.actions[name].forwarded_values,
                Some(ForwardedValues::Accepted)
            );
        }
        assert_eq!(notices(&accepted.canonical).len(), 2);
        let refused = load(&forwarding("pattern = \".+\"", false), path).unwrap();
        assert!(refused.canonical.actions.is_empty());
        let closed = load(&forwarding("values = [\"x\"]", false), path).unwrap();
        let closed_accepted = load(&forwarding("values = [\"x\"]", true), path).unwrap();
        assert_ne!(closed.hash, closed_accepted.hash, "the key is canonical");
        let rebuilt =
            action_from_canonical("deploy", &accepted.canonical.actions["deploy"]).unwrap();
        assert_eq!(
            rebuilt.forwarded_values,
            Some(ForwardedValues::Accepted),
            "it rebuilds"
        );
    }

    #[test]
    fn every_issue_kind_has_a_case() {
        for kind in IssueKind::ALL {
            with_issue(&failing_fixture(*kind), *kind);
        }
    }

    /// Each name rule is said one way, from the constant beside its
    /// predicate, and its length from [`MAX_NAME_CHARS`].
    #[test]
    fn every_name_rule_message_comes_from_its_constant() {
        for (kind, rule) in [
            (IssueKind::Name, NAME_RULE),
            (IssueKind::CredentialName, NAME_RULE),
            (IssueKind::ParamName, PARAM_NAME_RULE),
        ] {
            let catalogue = with_issue(&failing_fixture(kind), kind);
            let message = &issue_of(&catalogue, kind).message;
            assert!(message.ends_with(rule), "{kind:?}: {message}");
        }
        for rule in [NAME_RULE, PARAM_NAME_RULE] {
            assert!(
                rule.ends_with(&format!("at most {MAX_NAME_CHARS} characters")),
                "{rule}"
            );
        }
    }

    #[test]
    fn the_base_fixture_is_valid() {
        let catalogue = validated(&floor_with(&[VALID_PROBE]));
        assert!(catalogue.issues.is_empty(), "{:#?}", catalogue.issues);
        assert!(catalogue.actions.contains_key("probe"));
    }

    fn dropped(kind: IssueKind) {
        let catalogue = with_issue(&failing_fixture(kind), kind);
        assert!(
            !catalogue.actions.contains_key("probe"),
            "{kind:?} must drop the action"
        );
    }

    #[test]
    fn malformed_is_an_issue() {
        dropped(IssueKind::Malformed);
        let missing = validated(&bare(&["name = \"probe\"\nclass = \"free\"\n"]));
        assert!(
            issue_of(&missing, IssueKind::Malformed)
                .message
                .contains("program")
        );
        let not_a_table = validated("schema = 2\naction = [1]\n");
        assert_eq!(issue_of(&not_a_table, IssueKind::Malformed).name, UNNAMED);
    }

    #[test]
    fn http_action_parses() {
        let catalogue = validated(&with_credentials(&[VALID_HEALTH], &[CREDENTIAL]));
        assert!(catalogue.issues.is_empty(), "{:#?}", catalogue.issues);
        let action = &catalogue.actions["probe"];
        let http = action.http.as_ref().expect("an HTTP action");
        assert_eq!(http.method, HttpMethod::Get);
        assert_eq!(http.url.host(), "monitoring.example.com");
        assert_eq!(http.credential.as_deref(), Some("monitoring"));
        assert_eq!(
            (action.program.as_str(), action.args.len(), action.env.len()),
            ("", 0, 0)
        );
        let canonical = canonical(&catalogue, "C:/path/to/officina.toml");
        let http = canonical.actions["probe"].http.as_ref().unwrap();
        assert_eq!(http.url, "https://monitoring.example.com/api/{host}");
        assert_eq!(canonical.format, CANONICAL_FORMAT);

        let head = VALID_HEALTH
            .replace("\"GET\"", "\"HEAD\"")
            .replace("credential = \"monitoring\"\n", "")
            .replace("https://", "http://");
        let plain = validated(&with_credentials(&[&head], &[]));
        assert!(plain.issues.is_empty(), "{:#?}", plain.issues);
        assert_eq!(
            plain.actions["probe"].http.as_ref().unwrap().method,
            HttpMethod::Head,
            "plain http without a credential is allowed"
        );

        for (broken, kind) in [
            (
                VALID_HEALTH.replace("\"GET\"", "\"POST\""),
                IssueKind::Malformed,
            ),
            (
                VALID_HEALTH.replace("kind = \"http\"", "kind = \"process\""),
                IssueKind::Malformed,
            ),
            (
                VALID_HEALTH.replace("url = ", "location = "),
                IssueKind::Malformed,
            ),
            (
                VALID_HEALTH.replace("{host}", "{other}"),
                IssueKind::UndeclaredPlaceholder,
            ),
        ] {
            with_issue(&with_credentials(&[&broken], &[CREDENTIAL]), kind);
        }
    }

    #[test]
    fn http_action_with_program_fields_is_an_issue() {
        dropped(IssueKind::HttpProgramFields);
        for key in PROCESS_KEYS {
            let value = match *key {
                "env" => "{ A = \"b\" }",
                "args" => "[\"x\"]",
                _ => "\"x\"",
            };
            let action = VALID_HEALTH.replace(
                "class = \"free\"\n",
                &format!("class = \"free\"\n{key} = {value}\n"),
            );
            let catalogue = with_issue(
                &with_credentials(&[&action], &[CREDENTIAL]),
                IssueKind::HttpProgramFields,
            );
            assert!(
                issue_of(&catalogue, IssueKind::HttpProgramFields)
                    .message
                    .contains(&format!("`{key}`")),
                "{key}"
            );
        }
    }

    #[test]
    fn credential_entry_parses() {
        let credential = CREDENTIAL.replace(
            "[\"monitoring.example.com\"]",
            "[\"Monitoring.Example.com\", \"10.0.0.10\", \"monitoring.example.com\", \"[::1]\"]",
        );
        let catalogue = validated(&with_credentials(&[VALID_HEALTH], &[&credential]));
        assert!(catalogue.issues.is_empty(), "{:#?}", catalogue.issues);
        assert_eq!(
            catalogue.credentials["monitoring"],
            Credential {
                name: "monitoring".to_owned(),
                hosts: vec![
                    "10.0.0.10".to_owned(),
                    "[::1]".to_owned(),
                    "monitoring.example.com".to_owned()
                ],
                header: "Authorization".to_owned(),
                scheme: "Bearer".to_owned(),
            }
        );
        let canonical = canonical(&catalogue, "C:/path/to/officina.toml");
        assert_eq!(
            canonical.credentials["monitoring"].hosts,
            catalogue.credentials["monitoring"].hosts
        );
        assert_eq!(
            credentials_from_canonical(&canonical),
            catalogue.credentials
        );
        let line = line_of_text(
            &with_credentials(
                &[VALID_HEALTH],
                &[&credential.replace("Bearer", "Bad scheme")],
            ),
            "[[credential]]",
        );
        let broken = validated(&with_credentials(
            &[VALID_HEALTH],
            &[&credential.replace("Bearer", "Bad scheme")],
        ));
        assert_eq!(issue_of(&broken, IssueKind::CredentialScheme).line, line);
        for bad in ["", "example.com:443", "*.example.com", "a..b", "a b"] {
            assert!(host_entry(bad).is_none(), "{bad:?}");
        }
        let empty = CREDENTIAL.replace("[\"monitoring.example.com\"]", "[]");
        with_issue(
            &with_credentials(&[VALID_HEALTH], &[&empty]),
            IssueKind::CredentialHosts,
        );
    }

    #[test]
    fn unknown_credential_is_an_issue() {
        dropped(IssueKind::UnknownCredential);
        let dropped_credential = validated(&failing_fixture(IssueKind::CredentialHeader));
        assert!(
            dropped_credential
                .issues
                .iter()
                .any(|i| i.kind == IssueKind::UnknownCredential && i.name == "probe"),
            "a credential dropped as invalid is unknown to its actions"
        );
    }

    #[test]
    fn credential_host_not_bound_is_an_issue() {
        dropped(IssueKind::CredentialHostNotBound);
        let subdomain =
            VALID_HEALTH.replace("monitoring.example.com", "api.monitoring.example.com");
        with_issue(
            &with_credentials(&[&subdomain], &[CREDENTIAL]),
            IssueKind::CredentialHostNotBound,
        );
        let other_case =
            VALID_HEALTH.replace("monitoring.example.com", "MONITORING.example.com:8443");
        let bound = validated(&with_credentials(&[&other_case], &[CREDENTIAL]));
        assert!(
            bound.issues.is_empty(),
            "case and port do not matter: {:#?}",
            bound.issues
        );
    }

    #[test]
    fn credential_over_plain_http_is_an_issue() {
        dropped(IssueKind::CredentialOverHttp);
    }

    #[test]
    fn forbidden_or_malformed_header_is_an_issue() {
        dropped(IssueKind::CredentialHeader);
        let forbidden = FORBIDDEN_CREDENTIAL_HEADERS.iter().flat_map(|header| {
            [
                (*header).to_owned(),
                header.to_ascii_uppercase(),
                header
                    .split('-')
                    .map(|word| {
                        let mut chars = word.chars();
                        chars.next().map_or(String::new(), |first| {
                            first.to_ascii_uppercase().to_string() + chars.as_str()
                        })
                    })
                    .collect::<Vec<_>>()
                    .join("-"),
            ]
        });
        let malformed = ["", "X Api-Key", "X-Api-Key:", "Ä", "X\\tY"].map(str::to_owned);
        for header in forbidden.chain(malformed) {
            let credential = CREDENTIAL.replace("Authorization", &header);
            with_issue(
                &with_credentials(&[VALID_HEALTH], &[&credential]),
                IssueKind::CredentialHeader,
            );
        }
        let fine = CREDENTIAL.replace("Authorization", "X-Api-Key");
        assert!(
            validated(&with_credentials(&[VALID_HEALTH], &[&fine]))
                .issues
                .is_empty()
        );
    }

    #[test]
    fn malformed_scheme_is_an_issue() {
        dropped(IssueKind::CredentialScheme);
        for scheme in ["", "Bearer token", "Bea:rer", "Bearer\\r\\nX-Injected: 1"] {
            let credential = CREDENTIAL.replace("\"Bearer\"", &format!("\"{scheme}\""));
            with_issue(
                &with_credentials(&[VALID_HEALTH], &[&credential]),
                IssueKind::CredentialScheme,
            );
        }
    }

    #[test]
    fn url_userinfo_and_url_rules_are_issues() {
        dropped(IssueKind::UrlUserinfo);
        dropped(IssueKind::Url);
        let catalogue = validated(&failing_fixture(IssueKind::UrlUserinfo));
        assert!(
            issue_of(&catalogue, IssueKind::UrlUserinfo)
                .message
                .contains("user")
        );
    }

    #[test]
    fn duplicate_credentials_drop_every_copy() {
        let catalogue = with_issue(
            &failing_fixture(IssueKind::DuplicateCredential),
            IssueKind::DuplicateCredential,
        );
        assert!(catalogue.credentials.is_empty());
    }

    #[test]
    fn diff_reports_credential_changes() {
        let before = canonical_of(&with_credentials(&[VALID_HEALTH], &[CREDENTIAL]));
        let changed = CREDENTIAL
            .replace(
                "[\"monitoring.example.com\"]",
                "[\"monitoring.example.com\", \"mon2.example.com\"]",
            )
            .replace("Bearer", "Token");
        let added = CREDENTIAL.replace("\"monitoring\"", "\"backup\"");
        let after = canonical_of(&with_credentials(&[VALID_HEALTH], &[&changed, &added]));
        let d = diff(&before, &after);
        assert_eq!(d.credentials.added, ["backup"]);
        let entry = &d.credentials.changed[0];
        assert_eq!(entry.name, "monitoring");
        let fields: Vec<&str> = entry.fields.iter().map(|f| f.field.as_str()).collect();
        assert_eq!(fields, ["hosts", "scheme"]);
        assert!(!d.is_empty());

        let removed = diff(
            &after,
            &canonical_of(&with_credentials(&[VALID_HEALTH], &[&changed])),
        );
        assert_eq!(removed.credentials.removed, ["backup"]);
        let url = diff(
            &before,
            &canonical_of(&with_credentials(
                &[&VALID_HEALTH.replace("/api/", "/v2/")],
                &[CREDENTIAL],
            )),
        );
        assert_eq!(url.actions.changed[0].fields[0].field, "http");
    }

    #[test]
    fn name_rule() {
        dropped(IssueKind::Name);
        for valid in ["a", "check-lab-2", &"a".repeat(MAX_NAME_CHARS)] {
            assert!(is_valid_action_name(valid), "{valid}");
        }
        for invalid in [
            "",
            "2a",
            "-a",
            "a_b",
            "a.b",
            "é",
            &"a".repeat(MAX_NAME_CHARS + 1),
        ] {
            assert!(!is_valid_action_name(invalid), "{invalid}");
        }
    }

    #[test]
    fn duplicate_names_drop_every_copy() {
        let catalogue = with_issue(
            &failing_fixture(IssueKind::DuplicateName),
            IssueKind::DuplicateName,
        );
        assert!(catalogue.actions.is_empty());
        let duplicates = catalogue
            .issues
            .iter()
            .filter(|i| i.kind == IssueKind::DuplicateName)
            .count();
        assert_eq!(duplicates, 2);
        let one_invalid = validated(&floor_with(&[
            VALID_PROBE,
            "name = \"probe\"\nclass = \"free\"\n",
        ]));
        assert!(
            one_invalid.actions.is_empty(),
            "an invalid copy still makes the name ambiguous"
        );
    }

    #[test]
    fn program_must_be_absolute_for_its_runner() {
        dropped(IssueKind::ProgramNotAbsolute);
        let posix_on_native =
            floor_with(&[&VALID_PROBE.replace("C:/path/to/tool.exe", "/usr/bin/tool")]);
        with_issue(&posix_on_native, IssueKind::ProgramNotAbsolute);
        let windows_on_wsl = floor_with(&[
            &VALID_PROBE.replace("class = \"free\"\n", "class = \"free\"\nrunner = \"wsl\"\n")
        ]);
        with_issue(&windows_on_wsl, IssueKind::ProgramNotAbsolute);
    }

    #[test]
    fn program_holds_no_placeholder() {
        dropped(IssueKind::ProgramPlaceholder);
    }

    #[test]
    fn batch_programs_are_refused_on_native() {
        dropped(IssueKind::BatchProgram);
        for program in ["C:/x/run.bat", "C:/x/run.Bat. ", "C:/x/run.cmd"] {
            assert!(is_batch_program(program), "{program}");
        }
        assert!(!is_batch_program("C:/x/batch.exe"));
    }

    #[test]
    fn distro_rule() {
        dropped(IssueKind::Distro);
        for valid in ["Ubuntu", "Ubuntu-24.04", "kali_linux", "9distro"] {
            assert!(is_valid_distro(valid), "{valid}");
        }
        for invalid in [
            "",
            "-d",
            ".hidden",
            "Ubuntu 24",
            "ubuntu;id",
            &"a".repeat(65),
        ] {
            assert!(!is_valid_distro(invalid), "{invalid}");
        }
    }

    #[test]
    fn distro_needs_wsl() {
        dropped(IssueKind::DistroWithoutWsl);
    }

    #[test]
    fn env_key_rule() {
        dropped(IssueKind::EnvKey);
        for valid in [
            "PATH",
            "_x",
            "ANSIBLE_CONFIG",
            &"A".repeat(MAX_ENV_KEY_CHARS),
        ] {
            assert!(is_valid_env_key(valid), "{valid}");
        }
        for invalid in [
            "",
            "1A",
            "A-B",
            "A=B",
            "A B",
            &"A".repeat(MAX_ENV_KEY_CHARS + 1),
        ] {
            assert!(!is_valid_env_key(invalid), "{invalid}");
        }
    }

    #[test]
    fn env_values_are_literals() {
        dropped(IssueKind::EnvValuePlaceholder);
    }

    #[test]
    fn braces_delimit_one_placeholder() {
        dropped(IssueKind::Braces);
        type Cause = fn(String) -> BraceError;
        let cases: [(&str, Cause); 8] = [
            ("{a", BraceError::Unclosed),
            ("a}", BraceError::StrayClose),
            ("}a{", BraceError::StrayClose),
            ("{a}}", BraceError::Unbalanced),
            ("{{a}}", BraceError::Nested),
            ("{a}{b}", BraceError::SecondPlaceholder),
            ("x{a}y{b}z", BraceError::SecondPlaceholder),
            ("{a}b}", BraceError::Unbalanced),
        ];
        for (broken, cause) in cases {
            assert_eq!(
                ArgTemplate::parse(broken),
                Err(cause(broken.to_owned())),
                "{broken}"
            );
        }
        assert_eq!(
            BraceError::StrayClose("a}".to_owned()).to_string(),
            "`}` closes no placeholder in \"a}\""
        );
        assert_eq!(
            ArgTemplate::parse("--limit={group}:x").unwrap(),
            ArgTemplate::Placeholder {
                prefix: "--limit=".into(),
                param: "group".into(),
                suffix: ":x".into(),
            }
        );
        for item in ["plain", "{a}", "pre{a}", "{a}post", "p{a}s"] {
            assert_eq!(ArgTemplate::parse(item).unwrap().source(), item);
        }
    }

    #[test]
    fn placeholders_name_declared_parameters() {
        dropped(IssueKind::UndeclaredPlaceholder);
    }

    #[test]
    fn every_parameter_is_used() {
        dropped(IssueKind::UnusedParameter);
    }

    #[test]
    fn parameter_name_rule() {
        dropped(IssueKind::ParamName);
        assert!(is_valid_param_name("play_book-2"));
        assert!(!is_valid_param_name("a=b") && !is_valid_param_name(""));
    }

    #[test]
    fn values_or_pattern_exactly_one() {
        dropped(IssueKind::ValuesOrPattern);
        let neither = floor_with(&[&VALID_PROBE.replace("values = [\"web01\", \"db01\"]\n", "")]);
        with_issue(&neither, IssueKind::ValuesOrPattern);
    }

    #[test]
    fn values_pass_the_value_rules_for_the_runner() {
        dropped(IssueKind::InvalidValue);
        let native_switch = floor_with(&[&VALID_PROBE.replace("\"db01\"", "\"/S\"")]);
        with_issue(&native_switch, IssueKind::InvalidValue);
        let on_wsl = validated(&floor_with(&[&VALID_PROBE
            .replace("\"db01\"", "\"/srv/db01\"")
            .replace(
                "program = \"C:/path/to/tool.exe\"",
                "runner = \"wsl\"\nprogram = \"/usr/bin/tool\"",
            )]));
        assert!(on_wsl.issues.is_empty(), "{:#?}", on_wsl.issues);
    }

    #[test]
    fn pattern_too_long_is_an_issue() {
        dropped(IssueKind::PatternTooLong);
    }

    #[test]
    fn pattern_too_complex_is_an_issue() {
        dropped(IssueKind::PatternTooComplex);
    }

    #[test]
    fn pattern_that_does_not_compile_is_an_issue() {
        dropped(IssueKind::PatternInvalid);
    }

    const CHECK: &str = "name = \"check\"\nclass = \"free\"\nprogram = \"C:/path/to/tool.exe\"\nargs = [\"--dry-run\", \"{host}\"]\n[action.params.host]\nvalues = [\"web01\"]\n";

    fn apply(check: &str) -> String {
        format!(
            "name = \"apply\"\nclass = \"after-check\"\ncheck = \"{check}\"\nprogram = \"C:/path/to/tool.exe\"\nargs = [\"{{host}}\"]\n[action.params.host]\nvalues = [\"web01\"]\n"
        )
    }

    #[test]
    fn check_names_a_valid_free_action_with_the_same_parameters() {
        dropped(IssueKind::Check);
        let good = validated(&bare(&[CHECK, &apply("check")]));
        assert!(good.issues.is_empty(), "{:#?}", good.issues);
        assert_eq!(good.actions["apply"].check.as_deref(), Some("check"));

        let unknown = validated(&bare(&[CHECK, &apply("nope")]));
        let dropped_target = validated(&bare(&[
            &CHECK.replace("\"free\"", "\"auto\""),
            &apply("check"),
        ]));
        let not_free = validated(&bare(&[
            &CHECK.replace("\"free\"", "\"human\""),
            &apply("check"),
        ]));
        let other_params = validated(&bare(&[&CHECK.replace("host", "group"), &apply("check")]));
        let check_on_free = validated(&bare(&[&probe_top("check = \"probe\"\n")]));
        for catalogue in [
            unknown,
            dropped_target,
            not_free,
            other_params,
            check_on_free,
        ] {
            assert!(
                catalogue.issues.iter().any(|i| i.kind == IssueKind::Check),
                "{:#?}",
                catalogue.issues
            );
            assert!(!catalogue.actions.contains_key("apply"));
        }
    }

    /// Every action of a catalogue converts back from its canonical form to
    /// the same canonical form, so a command rebuilt from the approved
    /// snapshot is the one the floor's action builds.
    #[test]
    fn canonical_actions_convert_back_to_the_same_action() {
        let human = apply("check").replace("\"after-check\"", "\"human\"");
        let content = bare(&[
            CHECK,
            &human,
            &auto_apply().replace("\"apply\"", "\"apply-auto\"").replace(
                "class = \"auto\"\n",
                "class = \"auto\"\nauto_max_per_hour = 12\n",
            ),
            &probe_top(
                "runner = \"wsl\"\ndistro = \"Debian\"\nenv = { MODE = \"x\" }\ntimeout_s = 30\n",
            )
            .replace("C:/path/to/tool.exe", "/usr/bin/tool")
            .replace("\"probe\"", "\"remote\""),
            VALID_PROBE,
            &VALID_HEALTH.replace("\"probe\"", "\"health\""),
            &VALID_HEALTH
                .replace("\"probe\"", "\"ping\"")
                .replace("credential = \"monitoring\"\n", "")
                .replace("\"GET\"", "\"HEAD\""),
        ]) + "\n[[credential]]\n"
            + CREDENTIAL;
        let catalogue = canonical(&validated(&content), "C:/path/to/officina.toml");
        assert!(
            catalogue.actions["health"].http.is_some() && catalogue.actions["ping"].http.is_some()
        );
        assert!(
            catalogue.actions.len() >= 7,
            "{:?}",
            catalogue.actions.keys()
        );
        assert_eq!(catalogue.actions["apply-auto"].auto_max_per_hour, Some(12));
        for (name, action) in &catalogue.actions {
            let back = action_from_canonical(name, action).unwrap();
            assert_eq!(&CanonicalAction::from(&back), action, "{name}");
        }
    }

    /// D-170: a `human` action may name its dry run, under the same target
    /// rule as an `after-check` action.
    #[test]
    fn human_action_may_name_a_check() {
        let human = apply("check").replace("\"after-check\"", "\"human\"");
        let good = validated(&bare(&[CHECK, &human]));
        assert!(good.issues.is_empty(), "{:#?}", good.issues);
        assert_eq!(good.actions["apply"].check.as_deref(), Some("check"));
        let canonical = canonical(&good, "C:/path/to/officina.toml");
        assert_eq!(canonical.actions["apply"].check.as_deref(), Some("check"));
        let without = validated(&bare(&[&human.replace("check = \"check\"\n", "")]));
        assert!(without.issues.is_empty(), "a human action needs no check");

        let other_params = validated(&bare(&[&CHECK.replace("host", "group"), &human]));
        assert!(
            other_params
                .issues
                .iter()
                .any(|i| i.kind == IssueKind::Check)
        );
        assert!(!other_params.actions.contains_key("apply"));
    }

    #[test]
    fn free_action_naming_a_check_is_an_issue() {
        let free = apply("check").replace("\"after-check\"", "\"free\"");
        let catalogue = validated(&bare(&[CHECK, &free]));
        let issue = catalogue
            .issues
            .iter()
            .find(|i| i.kind == IssueKind::Check)
            .expect("an issue");
        assert_eq!(issue.name, "apply");
        assert!(!catalogue.actions.contains_key("apply"));
    }

    #[test]
    fn timeout_range() {
        dropped(IssueKind::Timeout);
        for out in [TIMEOUT_RANGE_S.start() - 1, TIMEOUT_RANGE_S.end() + 1] {
            let content = floor_with(&[&probe_top(&format!("timeout_s = {out}\n"))]);
            with_issue(&content, IssueKind::Timeout);
        }
        let longest = validated(&floor_with(&[&probe_top("timeout_s = 3600\n")]));
        assert_eq!(longest.actions["probe"].timeout_s, 3600);
    }

    #[test]
    fn max_output_range() {
        dropped(IssueKind::MaxOutput);
        let below = format!("max_output_kib = {}\n", MAX_OUTPUT_RANGE_KIB.start() - 1);
        with_issue(&floor_with(&[&probe_top(&below)]), IssueKind::MaxOutput);
    }

    #[test]
    fn guarded_program_entries_follow_d_3() {
        let catalogue = with_issue(
            &failing_fixture(IssueKind::GuardedProgram),
            IssueKind::GuardedProgram,
        );
        assert_eq!(catalogue.guarded_programs, ["ssh"]);
        assert!(catalogue.actions.contains_key("probe"));
        assert_eq!(
            issue_of(&catalogue, IssueKind::GuardedProgram).line,
            Some(2)
        );
        for invalid in ["", "*", "a*b", "**"] {
            assert!(guarded_program_problem(invalid).is_some(), "{invalid:?}");
        }
        assert!(guarded_program_problem("ansible*").is_none());
    }

    #[test]
    fn duplicate_projects_leave_the_catalogue() {
        let catalogue = with_issue(
            &failing_fixture(IssueKind::DuplicateProject),
            IssueKind::DuplicateProject,
        );
        assert!(catalogue.projects.is_empty());
        assert!(catalogue.actions.contains_key("probe"));
    }

    #[test]
    fn project_keeps_valid_actions_when_one_is_dropped() {
        let content = floor_with(&[
            VALID_PROBE,
            &VALID_PROBE
                .replace("\"probe\"", "\"broken\"")
                .replace("C:/", ""),
        ])
        .replace("[\"probe\"]", "[\"probe\", \"broken\", \"missing\"]");
        let catalogue = validated(&content);
        let project = &catalogue.projects[0];
        assert_eq!(project.actions, ["probe"]);
        let unknown: Vec<&str> = catalogue
            .issues
            .iter()
            .filter(|i| i.kind == IssueKind::UnknownProjectAction)
            .map(|i| i.message.as_str())
            .collect();
        assert_eq!(unknown.len(), 2, "{unknown:?}");
        assert!(unknown[0].contains("broken") && unknown[1].contains("missing"));
        let project_line = line_of_text(&content, "[[project]]");
        assert!(
            catalogue
                .issues
                .iter()
                .filter(|i| i.kind == IssueKind::UnknownProjectAction)
                .all(|i| i.name == "lab" && i.line == project_line)
        );
    }

    #[test]
    fn malformed_project_invalidates_the_floor() {
        for broken in [
            "schema = 2\n[[project]]\nname = \"lab\"\npath = \"Lab\"\nautonomy = \"maybe\"\n",
            "schema = 2\n[[project]]\nname = \"lab\"\npath = \"Lab\"\nactions = \"probe\"\n",
            "schema = 2\n[[project]]\nname = \"lab\"\npath = \"Lab\"\nunknown = 1\n",
            "schema = 2\n[[project]]\nname = \"lab\"\n",
        ] {
            assert!(
                matches!(parse_floor(broken), Err(FloorError::Invalid(_))),
                "{broken}"
            );
        }
    }

    #[test]
    fn malformed_action_never_invalidates_the_floor() {
        let broken_actions = [
            "name = 7\n",
            "class = \"free\"\n",
            "name = \"probe\"\nclass = \"sometimes\"\nprogram = 1\n",
            "name = \"probe\"\nclass = \"free\"\nprogram = \"C:/x.exe\"\nargs = \"--all\"\n",
            "name = \"probe\"\nclass = \"free\"\nprogram = \"C:/x.exe\"\nparams = { host = { values = 1 } }\n",
            "name = \"probe\"\nclass = \"free\"\nprogram = \"C:/x.exe\"\ntimeout_s = \"long\"\n",
        ];
        for action in broken_actions {
            let content = floor_with(&[action]);
            let floor = parse_floor(&content).expect("phase 1 ignores action content");
            let catalogue = validate(&floor, &content);
            assert!(catalogue.actions.is_empty(), "{action}");
            issue_of(&catalogue, IssueKind::Malformed);
            assert_eq!(catalogue.projects.len(), 1);
        }
    }

    #[test]
    fn issue_line_points_at_the_action() {
        let content = floor_with(&[
            VALID_PROBE,
            &VALID_PROBE
                .replace("\"probe\"", "\"second\"")
                .replace("C:/", ""),
            &VALID_PROBE.replace("\"probe\"", "\"third\""),
        ]);
        let header_lines: Vec<u32> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| *line == "[[action]]")
            .map(|(index, _)| u32::try_from(index + 1).unwrap())
            .collect();
        assert_eq!(header_lines.len(), 3);
        let catalogue = validated(&content);
        assert_eq!(catalogue.issues.len(), 1, "{:#?}", catalogue.issues);
        let issue = &catalogue.issues[0];
        assert_eq!(
            (issue.name.as_str(), issue.kind),
            ("second", IssueKind::ProgramNotAbsolute)
        );
        assert_eq!(issue.line, Some(header_lines[1]));
        assert!(catalogue.actions.contains_key("probe") && catalogue.actions.contains_key("third"));
    }

    #[test]
    fn defaults_are_explicit_after_validation() {
        let catalogue = validated(&floor_with(&[VALID_PROBE]));
        let action = &catalogue.actions["probe"];
        assert_eq!(action.runner, Runner::Native);
        assert_eq!(action.timeout_s, DEFAULT_TIMEOUT_S);
        assert_eq!(action.max_output_kib, DEFAULT_MAX_OUTPUT_KIB);
        assert_eq!(action.description, "");
        let expected: Vec<String> = {
            let mut list: Vec<String> = DEFAULT_GUARDED_PROGRAMS
                .iter()
                .map(|p| (*p).to_owned())
                .collect();
            list.sort();
            list
        };
        assert_eq!(catalogue.guarded_programs, expected);
        assert_eq!(catalogue.projects[0].autonomy, Autonomy::Guarded);
        let canonical = serde_json::to_value(CanonicalAction::from(action)).unwrap();
        assert_eq!(canonical["runner"], "native");
        assert_eq!(canonical["timeout_s"], DEFAULT_TIMEOUT_S);
        assert_eq!(canonical["max_output_kib"], DEFAULT_MAX_OUTPUT_KIB);
        assert!(canonical["distro"].is_null() && canonical["check"].is_null());
    }

    #[test]
    fn floor_list_replaces_the_default_lowercase_and_sorted() {
        let floor =
            parse_floor("schema = 2\nguarded_programs = [\"SSH\", \"ansible*\", \"ssh\"]\n")
                .unwrap();
        assert_eq!(guarded_programs(&floor), ["ansible*", "ssh"]);
        let empty = parse_floor("schema = 2\nguarded_programs = []\n").unwrap();
        assert!(guarded_programs(&empty).is_empty());
    }

    #[test]
    fn schema_1_floor_gives_free_projects_and_no_action() {
        let catalogue = validated("schema = 1\n[[project]]\nname = \"a\"\npath = \"a\"\n");
        assert_eq!(catalogue.projects[0].autonomy, Autonomy::Free);
        assert!(catalogue.actions.is_empty() && catalogue.issues.is_empty());
    }

    fn canonical_of(content: &str) -> CanonicalCatalogue {
        canonical(&validated(content), "C:/Work/officina.toml")
    }

    #[test]
    fn canonical_resolves_paths_and_sorts() {
        let content = floor_with(&[
            &VALID_PROBE.replace("[\"web01\", \"db01\"]", "[\"web01\", \"db01\", \"web01\"]")
        ]);
        let canonical = canonical_of(&content);
        assert_eq!(canonical.floor_path, "c:/work/officina.toml");
        assert_eq!(canonical.projects["lab"].path, "c:/work/lab");
        assert_eq!(
            canonical.actions["probe"].params["host"],
            CanonicalParam::Values(vec!["db01".into(), "web01".into()])
        );
        assert_eq!(canonical.actions["probe"].args, ["--host", "{host}"]);
        assert_eq!(hash(&canonical).len(), 64);
        assert!(
            hash(&canonical)
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
    }

    #[test]
    fn identical_catalogues_have_an_empty_diff() {
        let canonical = canonical_of(&floor_with(&[VALID_PROBE]));
        assert!(diff(&canonical, &canonical.clone()).is_empty());
    }

    #[test]
    fn diff_reports_added_and_removed() {
        let before = canonical_of(&floor_with(&[VALID_PROBE]));
        let after = canonical_of(
            &floor_with(&[&VALID_PROBE.replace("\"probe\"", "\"status\"")])
                .replace("name = \"lab\"", "name = \"ops\"")
                .replace("[\"probe\"]", "[\"status\"]"),
        );
        let d = diff(&before, &after);
        assert_eq!(d.actions.added, ["status"]);
        assert_eq!(d.actions.removed, ["probe"]);
        assert_eq!(d.projects.added, ["ops"]);
        assert_eq!(d.projects.removed, ["lab"]);
        assert!(d.actions.changed.is_empty() && d.projects.changed.is_empty());
        assert!(!d.is_empty());
    }

    #[test]
    fn diff_reports_changed_fields() {
        let before = canonical_of(&floor_with(&[VALID_PROBE]));
        let after = canonical_of(
            &floor_with(&[&probe_top(
                "timeout_s = 60\ndescription = \"Probe a host\"\n",
            )])
            .replace("path = \"Lab\"", "path = \"Lab2\"\nautonomy = \"free\""),
        );
        let d = diff(&before, &after);
        let action = &d.actions.changed[0];
        assert_eq!(action.name, "probe");
        let fields: Vec<&str> = action.fields.iter().map(|f| f.field.as_str()).collect();
        assert_eq!(fields, ["description", "timeout_s"]);
        assert_eq!(action.fields[1].before, DEFAULT_TIMEOUT_S);
        assert_eq!(action.fields[1].after, 60);
        let project = &d.projects.changed[0];
        let fields: Vec<&str> = project.fields.iter().map(|f| f.field.as_str()).collect();
        assert_eq!(fields, ["autonomy", "path"]);
        assert_eq!(project.fields[0].before, "guarded");
    }

    #[test]
    fn diff_reports_guarded_programs() {
        let before = canonical_of(&floor_with(&[VALID_PROBE]));
        let after = canonical_of(&floor_with(&[VALID_PROBE]).replacen(
            "schema = 2\n",
            "schema = 2\nguarded_programs = [\"ssh\", \"terraform\"]\n",
            1,
        ));
        let d = diff(&before, &after);
        assert_eq!(d.guarded_programs.added, ["terraform"]);
        let mut removed: Vec<String> = DEFAULT_GUARDED_PROGRAMS
            .iter()
            .filter(|p| **p != "ssh")
            .map(|p| (*p).to_owned())
            .collect();
        removed.sort();
        assert_eq!(d.guarded_programs.removed, removed);
        assert!(d.actions.is_empty() && d.projects.is_empty());
    }

    #[test]
    fn diff_covers_every_canonical_field() {
        let canonical = canonical_of(&with_templates(&[VALID_TEMPLATE]));
        assert!(!canonical.templates.is_empty());
        let canonical_keys: BTreeSet<String> = serde_json::to_value(&canonical)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let diff_keys: BTreeSet<String> = serde_json::to_value(CatalogueDiff::default())
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(canonical_keys, diff_keys);
        let mut moved = canonical.clone();
        moved.floor_path = "d:/other/officina.toml".into();
        moved.format += 1;
        moved.auto_review.model = Some(ModelAlias::Opus);
        moved.templates.get_mut("bump").unwrap().model = None;
        let d = diff(&canonical, &moved);
        assert_eq!(d.floor_path.unwrap().after, "d:/other/officina.toml");
        assert_eq!(d.format.unwrap().before, CANONICAL_FORMAT);
        assert_eq!(d.auto_review.unwrap().after, "opus");
        assert_eq!(d.templates.changed[0].fields[0].field, "model");
    }

    /// A floor whose one action runs `program` with `args`, its parameters
    /// `host` and `extra` taking a closed value each.
    fn notice_floor(program: &str, args: &str) -> String {
        floor_with(&[&format!(
            "name = \"probe\"\nclass = \"free\"\nprogram = \"{program}\"\nargs = {args}\n[action.params.host]\nvalues = [\"web01\"]\n[action.params.extra]\nvalues = [\"x\"]\n"
        )])
    }

    fn notices_of(content: &str) -> Vec<(NoticeKind, String, u32)> {
        notices(&canonical_of(content))
            .into_iter()
            .map(|n| (n.kind, n.param, n.item))
            .collect()
    }

    /// A floor raising exactly one notice of `kind`; exhaustive, so a new
    /// kind does not compile without its case.
    fn notice_fixture(kind: NoticeKind) -> String {
        match kind {
            NoticeKind::RemoteCommandValue => notice_floor(
                "C:/Windows/System32/OpenSSH/ssh.exe",
                "[\"{host}\", \"systemctl\", \"status\", \"{extra}\"]",
            ),
            NoticeKind::TemplatedValue => notice_floor(
                "C:/path/to/ansible-playbook.exe",
                "[\"--limit\", \"{host}\", \"-e\", \"{extra}\"]",
            ),
        }
    }

    #[test]
    fn every_notice_kind_has_a_case() {
        for kind in NoticeKind::ALL {
            let found = notices_of(&notice_fixture(*kind));
            assert_eq!(found.len(), 1, "{kind:?}: {found:?}");
            assert_eq!(found[0].0, *kind);
            assert!(kind.explanation().contains(kind.program()));
        }
    }

    #[test]
    fn ssh_placeholder_after_the_destination_is_a_remote_command_value() {
        assert_eq!(
            notices_of(&notice_fixture(NoticeKind::RemoteCommandValue)),
            [(NoticeKind::RemoteCommandValue, "extra".to_owned(), 3)]
        );
        let with_options = notice_floor(
            "C:/x/SSH.EXE",
            "[\"-p\", \"22\", \"-o\", \"BatchMode=yes\", \"web01\", \"echo {extra}\", \"{host}\"]",
        );
        let mut found = notices_of(&with_options);
        found.sort_by_key(|n| n.2);
        assert_eq!(
            found,
            [
                (NoticeKind::RemoteCommandValue, "extra".to_owned(), 5),
                (NoticeKind::RemoteCommandValue, "host".to_owned(), 6)
            ],
            "an option's value is not the destination"
        );
        let after_double_dash = notice_floor(
            "C:/x/ssh.exe",
            "[\"--\", \"web01\", \"{host}\", \"{extra}\"]",
        );
        assert_eq!(notices_of(&after_double_dash).len(), 2);
    }

    #[test]
    fn ssh_placeholder_as_the_destination_is_no_notice() {
        for args in [
            "[\"{host}\", \"uptime\", \"--\", \"x{extra}\"]",
            "[\"-l\", \"{extra}\", \"{host}\"]",
        ] {
            let found = notices_of(&notice_floor("C:/x/ssh.exe", args));
            let at_destination = found
                .iter()
                .filter(|n| n.1 == "host" || (args.contains("-l") && n.1 == "extra"))
                .count();
            assert_eq!(at_destination, 0, "{args}: {found:?}");
        }
        assert!(
            notices_of(&notice_floor(
                "C:/x/scp.exe",
                "[\"{host}\", \"a\", \"{extra}\"]"
            ))
            .is_empty(),
            "only ssh"
        );
    }

    #[test]
    fn ansible_extra_vars_placeholder_is_a_templated_value() {
        assert_eq!(
            notices_of(&notice_fixture(NoticeKind::TemplatedValue)),
            [(NoticeKind::TemplatedValue, "extra".to_owned(), 3)]
        );
        for (args, item) in [
            ("[\"--extra-vars\", \"{extra}\", \"{host}\"]", 1),
            ("[\"--extra-vars=target={extra}\", \"{host}\"]", 0),
        ] {
            assert_eq!(
                notices_of(&notice_floor("C:/x/ansible-playbook.exe", args)),
                [(NoticeKind::TemplatedValue, "extra".to_owned(), item)],
                "{args}"
            );
        }
    }

    #[test]
    fn ansible_args_placeholder_is_a_templated_value() {
        for args in [
            "[\"{host}\", \"-a\", \"{extra}\"]",
            "[\"{host}\", \"--args\", \"{extra}\"]",
            "[\"{host}\", \"--args=cmd={extra}\"]",
        ] {
            let found = notices_of(&notice_floor("C:/path/to/ANSIBLE.exe", args));
            assert_eq!(found.len(), 1, "{args}: {found:?}");
            assert_eq!(
                (found[0].0, found[0].1.as_str()),
                (NoticeKind::TemplatedValue, "extra")
            );
        }
    }

    #[test]
    fn notices_keep_the_action() {
        for kind in NoticeKind::ALL {
            let catalogue = validated(&notice_fixture(*kind));
            assert!(catalogue.issues.is_empty(), "{:#?}", catalogue.issues);
            assert!(catalogue.actions.contains_key("probe"), "{kind:?}");
        }
    }

    /// D-466: the view keys authorise nothing, so adding or changing them
    /// leaves the canonical catalogue, and an approval, as they were.
    #[test]
    fn priority_and_alert_keys_do_not_change_the_canonical_hash() {
        let content = floor_with(&[VALID_PROBE]);
        let plain = load(&content, "C:/Work/officina.toml").unwrap();
        let with_view_keys = content.replacen(
            "[[project]]\n",
            "[alert]\nafter_minutes = 3\n\n[[project]]\npriority = \"high\"\n",
            1,
        );
        assert_ne!(with_view_keys, content, "the keys were added");
        let viewed = load(&with_view_keys, "C:/Work/officina.toml").unwrap();
        assert_eq!(
            crate::floor::FloorFile::priority_of(&viewed.floor.projects[0]),
            crate::floor::Priority::High
        );
        assert_eq!(viewed.floor.alert_after_minutes(), 3);
        assert_eq!(viewed.canonical, plain.canonical);
        assert_eq!(viewed.hash, plain.hash);
    }

    #[test]
    fn notices_do_not_change_the_hash() {
        let content = notice_fixture(NoticeKind::RemoteCommandValue);
        let relay = load(&content, "C:/Work/officina.toml").unwrap();
        assert!(!notices(&relay.canonical).is_empty());
        assert_eq!(relay.hash, hash(&relay.canonical));
        let serialized = serde_json::to_string(&relay.canonical).unwrap();
        for kind in NoticeKind::ALL {
            assert!(
                !serialized.contains(kind.code()),
                "the canonical form carries a notice"
            );
        }
    }

    // ---- Story 5.4: templates, the new tables and `auto` ----

    /// The issues of `table`, as `(name, kind)`.
    fn issues_of(catalogue: &Catalogue, table: IssueTable) -> Vec<(String, IssueKind)> {
        catalogue
            .issues
            .iter()
            .filter(|i| i.table == table)
            .map(|i| (i.name.clone(), i.kind))
            .collect()
    }

    #[test]
    fn the_base_template_is_valid() {
        let catalogue = validated(&with_templates(&[VALID_TEMPLATE]));
        assert!(catalogue.issues.is_empty(), "{:#?}", catalogue.issues);
        let template = &catalogue.templates["bump"];
        assert_eq!(template.projects, ["lab"]);
        assert_eq!(template.params["level"], ["minor", "patch"], "sorted");
        assert_eq!(template.params["package"], ["@scope/pkg", "serde_json"]);
        assert_eq!(template.model, Some(ModelAlias::Sonnet));
        assert_eq!(catalogue.orchestrator_model, None);
        assert_eq!(catalogue.auto_review_model, Some(DEFAULT_REVIEW_MODEL));
    }

    /// Every template rule drops the template with its issue, which names
    /// its table and line, and leaves the actions and the floor as they
    /// are; an unknown project only leaves the template.
    #[test]
    fn every_template_issue_names_its_table_and_line() {
        for kind in IssueKind::ALL {
            let content = failing_fixture(*kind);
            if !content.contains("[[template]]") {
                continue;
            }
            let catalogue = with_issue(&content, *kind);
            let issue = issue_of(&catalogue, *kind);
            assert_eq!(issue.table, IssueTable::Template, "{kind:?}");
            assert_eq!(issue.name, "bump", "{kind:?}");
            assert_eq!(
                issue.line,
                line_of_text(&content, "[[template]]"),
                "{kind:?}"
            );
            assert_eq!(
                catalogue.templates.contains_key("bump"),
                *kind == IssueKind::UnknownTemplateProject,
                "{kind:?}"
            );
            assert!(catalogue.actions.contains_key("probe"), "{kind:?}");
        }
    }

    #[test]
    fn a_malformed_template_never_invalidates_the_floor() {
        for broken in [
            "name = 7\n",
            "name = \"bump\"\n",
            &template_with("model = \"sonnet\"\n", "model = \"sonnet\"\nunknown = 1\n"),
            &template_with("projects = [\"lab\"]", "projects = \"lab\""),
            &template_with(
                "values = [\"patch\", \"minor\"]",
                "values = [\"patch\"]\nextra = 1",
            ),
        ] {
            let content = with_templates(&[broken]);
            let floor = parse_floor(&content).expect("phase 1 keeps templates raw");
            let catalogue = validate(&floor, &content);
            assert!(catalogue.templates.is_empty(), "{broken}");
            assert_eq!(
                issue_of(&catalogue, IssueKind::Malformed).table,
                IssueTable::Template,
                "{broken}"
            );
            assert!(catalogue.actions.contains_key("probe"));
            assert_eq!(catalogue.projects.len(), 1);
        }
    }

    #[test]
    fn template_names_are_separate_from_action_names() {
        let same_as_action = template_with("name = \"bump\"", "name = \"probe\"");
        let catalogue = validated(&with_templates(&[&same_as_action]));
        assert!(catalogue.issues.is_empty(), "{:#?}", catalogue.issues);
        assert!(
            catalogue.templates.contains_key("probe") && catalogue.actions.contains_key("probe")
        );

        let twice = validated(&with_templates(&[VALID_TEMPLATE, VALID_TEMPLATE]));
        assert!(twice.templates.is_empty(), "both copies are dropped");
        assert_eq!(
            issues_of(&twice, IssueTable::Template),
            [
                ("bump".to_owned(), IssueKind::DuplicateName),
                ("bump".to_owned(), IssueKind::DuplicateName)
            ]
        );
        assert!(twice.actions.contains_key("probe"));

        let bad_name = validated(&with_templates(&[&template_with("\"bump\"", "\"Bump\"")]));
        assert_eq!(
            issues_of(&bad_name, IssueTable::Template),
            [("Bump".to_owned(), IssueKind::Name)]
        );
        assert!(
            issue_of(&bad_name, IssueKind::Name)
                .message
                .ends_with(NAME_RULE)
        );
    }

    #[test]
    fn an_unknown_project_leaves_the_template() {
        let catalogue = with_issue(
            &failing_fixture(IssueKind::UnknownTemplateProject),
            IssueKind::UnknownTemplateProject,
        );
        assert_eq!(catalogue.templates["bump"].projects, ["lab"]);
        assert!(
            issue_of(&catalogue, IssueKind::UnknownTemplateProject)
                .message
                .contains("missing")
        );
        // A project dropped as a duplicate is unknown to templates too.
        let duplicated = with_templates(&[VALID_TEMPLATE]).replacen(
            "\n[[action]]",
            &format!("{PROJECT}\n[[action]]"),
            1,
        );
        let catalogue = validated(&duplicated);
        assert!(catalogue.templates.is_empty());
        issue_of(&catalogue, IssueKind::TemplateProjects);
    }

    #[test]
    fn a_template_left_with_no_project_is_dropped() {
        let only_unknown =
            template_with("projects = [\"lab\"]", "projects = [\"missing\", \"gone\"]");
        let catalogue = validated(&with_templates(&[&only_unknown]));
        assert!(catalogue.templates.is_empty());
        let kinds: Vec<IssueKind> = issues_of(&catalogue, IssueTable::Template)
            .into_iter()
            .map(|(_, kind)| kind)
            .collect();
        assert_eq!(
            kinds,
            [
                IssueKind::UnknownTemplateProject,
                IssueKind::UnknownTemplateProject,
                IssueKind::TemplateProjects
            ]
        );
        let empty = validated(&failing_fixture(IssueKind::TemplateProjects));
        assert!(empty.templates.is_empty());
    }

    #[test]
    fn a_prompt_starting_with_a_placeholder_is_refused() {
        for prompt in ["{package} and {level}", "  \n{package} {level}"] {
            let template = template_with(
                "\"Bump {package} to its latest {level} release.\"",
                &toml_string(prompt),
            );
            let catalogue = with_issue(
                &with_templates(&[&template]),
                IssueKind::TemplatePromptStart,
            );
            assert!(catalogue.templates.is_empty(), "{prompt:?}");
        }
        for prompt in ["", " \n\t "] {
            let template = template_with(
                "\"Bump {package} to its latest {level} release.\"",
                &toml_string(prompt),
            );
            let catalogue = with_issue(
                &with_templates(&[&template]),
                IssueKind::TemplatePromptStart,
            );
            assert!(
                issue_of(&catalogue, IssueKind::TemplatePromptStart)
                    .message
                    .contains("only whitespace"),
                "{prompt:?}"
            );
        }
    }

    #[test]
    fn a_leading_slash_bang_or_dash_after_whitespace_is_refused() {
        for start in FORBIDDEN_PROMPT_STARTS {
            for lead in ["", "   ", "\n\t "] {
                let prompt = format!("{lead}{start}x {{package}} {{level}}");
                let template = template_with(
                    "\"Bump {package} to its latest {level} release.\"",
                    &toml_string(&prompt),
                );
                let catalogue = with_issue(
                    &with_templates(&[&template]),
                    IssueKind::TemplatePromptStart,
                );
                assert!(catalogue.templates.is_empty(), "{prompt:?}");
            }
            let inside = format!("Run x{start}y {{package}} {{level}}");
            let template = template_with(
                "\"Bump {package} to its latest {level} release.\"",
                &toml_string(&inside),
            );
            let fine = validated(&with_templates(&[&template]));
            assert!(fine.issues.is_empty(), "{inside:?}: {:#?}", fine.issues);
        }
        let windows_lines = template_with(
            "\"Bump {package} to its latest {level} release.\"",
            &toml_string("Bump {package}\r\nto {level}.\tDone."),
        );
        let fine = validated(&with_templates(&[&windows_lines]));
        assert!(fine.issues.is_empty(), "{:#?}", fine.issues);
    }

    #[test]
    fn the_longest_rendering_is_checked() {
        // Rendered with the longest values, the prompt is exactly the limit.
        let rendered_base = "Bump serde_json patch.".chars().count();
        let at_limit = format!(
            "Bump {{package}} {{level}}.{}",
            "a".repeat(MAX_PROMPT_CHARS - rendered_base)
        );
        let template = template_with(
            "\"Bump {package} to its latest {level} release.\"",
            &toml_string(&at_limit),
        );
        let fits = validated(&with_templates(&[&template]));
        assert!(fits.issues.is_empty(), "{:#?}", fits.issues);
        let params = &fits.templates["bump"].params;
        assert_eq!(worst_case_chars(&at_limit, params), Some(MAX_PROMPT_CHARS));

        let longer_value = template.replace("\"serde_json\"", "\"serde_jsonx\"");
        let over = with_issue(
            &with_templates(&[&longer_value]),
            IssueKind::TemplatePromptTooLong,
        );
        assert!(over.templates.is_empty());
        assert!(
            issue_of(&over, IssueKind::TemplatePromptTooLong)
                .message
                .contains(&(MAX_PROMPT_CHARS + 1).to_string())
        );
    }

    #[test]
    fn pattern_is_refused_for_templates() {
        let catalogue = with_issue(
            &failing_fixture(IssueKind::TemplatePattern),
            IssueKind::TemplatePattern,
        );
        assert!(catalogue.templates.is_empty());
        let both = template_with(
            "values = [\"patch\", \"minor\"]",
            "values = [\"patch\"]\npattern = \"[a-z]+\"",
        );
        let catalogue = with_issue(&with_templates(&[&both]), IssueKind::TemplatePattern);
        assert!(catalogue.templates.is_empty());
    }

    /// Template values follow `params::template_value`: a scoped package is
    /// one, a space is not; the other value rules hold.
    #[test]
    fn template_values_follow_the_template_value_rule() {
        for (value, valid) in [
            ("@scope/pkg", true),
            ("/srv/data", true),
            ("two words", false),
            ("-x", false),
            ("..", false),
        ] {
            let template = template_with("\"serde_json\"", &toml_string(value));
            let catalogue = validated(&with_templates(&[&template]));
            assert_eq!(catalogue.templates.contains_key("bump"), valid, "{value}");
            if !valid {
                issue_of(&catalogue, IssueKind::InvalidValue);
            }
        }
        for (from, to, kind) in [
            ("{level}", "{other}", IssueKind::UndeclaredPlaceholder),
            (" {level}", "", IssueKind::UnusedParameter),
            ("{level}", "{level", IssueKind::Braces),
            (
                "[template.params.level]",
                "[template.params.Level]",
                IssueKind::ParamName,
            ),
        ] {
            let template = template_with(from, to);
            let catalogue = with_issue(&with_templates(&[&template]), kind);
            assert_eq!(issue_of(&catalogue, kind).table, IssueTable::Template);
            assert!(catalogue.templates.is_empty(), "{kind:?}");
        }
    }

    #[test]
    fn auto_is_a_valid_class() {
        let catalogue = validated(&bare(&[CHECK, &auto_apply()]));
        assert!(catalogue.issues.is_empty(), "{:#?}", catalogue.issues);
        let action = &catalogue.actions["apply"];
        assert_eq!(action.class, ActionClass::Auto);
        assert_eq!(action.check.as_deref(), Some("check"));
        assert_eq!(action.auto_max_per_hour, Some(DEFAULT_AUTO_MAX_PER_HOUR));
        let canonical = canonical(&catalogue, "C:/path/to/officina.toml");
        assert_eq!(
            canonical.actions["apply"].auto_max_per_hour,
            Some(DEFAULT_AUTO_MAX_PER_HOUR),
            "the default is explicit"
        );
        assert_eq!(canonical.actions["check"].auto_max_per_hour, None);
        let value = serde_json::to_value(&canonical.actions["check"]).unwrap();
        assert!(
            value.get("auto_max_per_hour").is_none(),
            "absent off `auto`"
        );
    }

    #[test]
    fn auto_without_a_check_is_an_issue() {
        let catalogue = with_issue(
            &failing_fixture(IssueKind::AutoWithoutCheck),
            IssueKind::AutoWithoutCheck,
        );
        assert!(!catalogue.actions.contains_key("apply"), "dropped");
        assert!(catalogue.actions.contains_key("check"));
        let issue = issue_of(&catalogue, IssueKind::AutoWithoutCheck);
        assert_eq!(
            (issue.table, issue.name.as_str()),
            (IssueTable::Action, "apply")
        );
        let content = failing_fixture(IssueKind::AutoWithoutCheck);
        let lines: Vec<u32> = content
            .lines()
            .enumerate()
            .filter(|(_, line)| *line == "[[action]]")
            .map(|(index, _)| u32::try_from(index + 1).unwrap())
            .collect();
        assert_eq!(issue.line, Some(lines[1]));
    }

    #[test]
    fn auto_check_names_a_valid_free_action() {
        for broken in [
            bare(&[CHECK, &auto_apply().replace("\"check\"\n", "\"nope\"\n")]),
            bare(&[&CHECK.replace("\"free\"", "\"human\""), &auto_apply()]),
            bare(&[&CHECK.replace("host", "group"), &auto_apply()]),
        ] {
            let catalogue = with_issue(&broken, IssueKind::Check);
            assert!(!catalogue.actions.contains_key("apply"), "{broken}");
        }
    }

    #[test]
    fn auto_max_per_hour_range_and_class() {
        for (cap, valid) in [(0, false), (1, true), (60, true), (61, false), (-1, false)] {
            let action = auto_apply().replace(
                "class = \"auto\"\n",
                &format!("class = \"auto\"\nauto_max_per_hour = {cap}\n"),
            );
            let catalogue = validated(&bare(&[CHECK, &action]));
            assert_eq!(catalogue.actions.contains_key("apply"), valid, "{cap}");
            if valid {
                assert_eq!(
                    catalogue.actions["apply"].auto_max_per_hour,
                    Some(u32::try_from(cap).unwrap())
                );
            } else {
                issue_of(&catalogue, IssueKind::AutoMaxPerHour);
            }
        }
        for class in ["free", "after-check", "human"] {
            let action = auto_apply().replace(
                "class = \"auto\"\n",
                &format!("class = \"{class}\"\nauto_max_per_hour = 6\n"),
            );
            let action = if class == "free" {
                action.replace("check = \"check\"\n", "")
            } else {
                action
            };
            let catalogue = with_issue(&bare(&[CHECK, &action]), IssueKind::AutoMaxPerHour);
            assert!(!catalogue.actions.contains_key("apply"), "{class}");
        }
    }

    #[test]
    fn auto_on_an_http_action_is_an_issue() {
        let catalogue = with_issue(
            &failing_fixture(IssueKind::AutoOnHttp),
            IssueKind::AutoOnHttp,
        );
        assert!(catalogue.actions.is_empty());
        let human = with_credentials(
            &[&VALID_HEALTH.replace("\"free\"", "\"human\"")],
            &[CREDENTIAL],
        );
        assert!(validated(&human).issues.is_empty(), "HTTP stays on `human`");
    }

    #[test]
    fn a_failing_orchestrator_table_is_an_issue() {
        let floor =
            |table: &str| format!("{}\n[orchestrator]\n{table}", floor_with(&[VALID_PROBE]));
        let valid = validated(&floor("model = \"opus\"\n"));
        assert!(valid.issues.is_empty(), "{:#?}", valid.issues);
        assert_eq!(valid.orchestrator_model, Some(ModelAlias::Opus));
        assert_eq!(validated(&floor("")).orchestrator_model, None);
        for (table, kind) in [
            ("model = \"gpt\"\n", IssueKind::Model),
            ("model = 3\n", IssueKind::Malformed),
            ("modle = \"opus\"\n", IssueKind::Malformed),
        ] {
            let content = floor(table);
            let catalogue = with_issue(&content, kind);
            let issue = issue_of(&catalogue, kind);
            assert_eq!(
                (issue.table, issue.name.as_str()),
                (IssueTable::Orchestrator, "orchestrator")
            );
            assert_eq!(
                issue.line,
                line_of_text(&content, "[orchestrator]"),
                "{table}"
            );
            assert_eq!(catalogue.orchestrator_model, None, "no model flag");
            assert!(catalogue.actions.contains_key("probe"));
        }
    }

    #[test]
    fn a_failing_auto_review_table_is_an_issue() {
        let floor = |table: &str| format!("{}\n[auto_review]\n{table}", floor_with(&[VALID_PROBE]));
        let valid = load(&floor("model = \"sonnet\"\n"), "C:/path/to/officina.toml").unwrap();
        assert!(valid.catalogue.issues.is_empty());
        assert_eq!(valid.canonical.auto_review.model, Some(ModelAlias::Sonnet));
        let empty = load(&floor(""), "C:/path/to/officina.toml").unwrap();
        let absent = load(&floor_with(&[VALID_PROBE]), "C:/path/to/officina.toml").unwrap();
        assert_eq!(
            absent.canonical.auto_review.model,
            Some(DEFAULT_REVIEW_MODEL),
            "explicit when absent"
        );
        assert_eq!(empty.hash, absent.hash, "an empty table is the default");
        for (table, kind) in [
            ("model = \"gpt\"\n", IssueKind::Model),
            ("model = [\"haiku\"]\n", IssueKind::Malformed),
        ] {
            let content = floor(table);
            let relay = load(&content, "C:/path/to/officina.toml").unwrap();
            let issue = issue_of(&relay.catalogue, kind);
            assert_eq!(
                (issue.table, issue.name.as_str()),
                (IssueTable::AutoReview, "auto_review")
            );
            assert_eq!(issue.line, line_of_text(&content, "[auto_review]"));
            assert_eq!(
                relay.canonical.auto_review.model, None,
                "recorded as failing"
            );
            assert_ne!(relay.hash, absent.hash);
        }
    }

    #[test]
    fn orchestrator_table_is_not_canonical() {
        let plain = load(
            &with_templates(&[VALID_TEMPLATE]),
            "C:/path/to/officina.toml",
        )
        .unwrap();
        for table in ["model = \"opus\"\n", "model = \"gpt\"\n", ""] {
            let content = format!(
                "{}\n[orchestrator]\n{table}",
                with_templates(&[VALID_TEMPLATE])
            );
            let with_table = load(&content, "C:/path/to/officina.toml").unwrap();
            assert_eq!(with_table.canonical, plain.canonical, "{table}");
            assert_eq!(with_table.hash, plain.hash, "{table}");
        }
        let serialized = serde_json::to_value(&plain.canonical).unwrap();
        assert!(serialized.get("orchestrator").is_none());
    }

    #[test]
    fn templates_are_canonical_and_change_the_hash() {
        let path = "C:/path/to/officina.toml";
        let without = load(&floor_with(&[VALID_PROBE]), path).unwrap();
        let with = load(&with_templates(&[VALID_TEMPLATE]), path).unwrap();
        assert_ne!(without.hash, with.hash);
        let template = &with.canonical.templates["bump"];
        assert_eq!(
            template.prompt,
            "Bump {package} to its latest {level} release."
        );
        assert_eq!(template.params["level"], ["minor", "patch"]);
        let reordered = template_with(
            "[\"patch\", \"minor\"]",
            "[\"minor\", \"patch\", \"minor\"]",
        );
        let same = load(&with_templates(&[&reordered]), path).unwrap();
        assert_eq!(same.hash, with.hash, "values are sorted and deduplicated");
        let invalid = load(&failing_fixture(IssueKind::TemplatePattern), path).unwrap();
        assert_eq!(
            invalid.hash, without.hash,
            "a dropped template is not canonical"
        );
    }

    /// The previous format's approval, as `main` wrote it for the golden
    /// floor before this format (D-131): it parses, and against the same
    /// floor today its diff shows the format change and the templates added.
    #[test]
    fn diff_reports_the_format_change() {
        let previous: ApprovedCatalogue = serde_json::from_str(include_str!(
            "../../tests/data/relay/approved-previous-format.json"
        ))
        .expect("an approval in the previous format parses");
        assert_eq!(previous.canonical.format, CANONICAL_FORMAT - 1);
        assert!(previous.canonical.templates.is_empty());
        assert_eq!(
            previous.canonical.auto_review,
            CanonicalAutoReview::default()
        );
        assert_ne!(
            hash(&previous.canonical),
            previous.hash,
            "its stored hash is its own"
        );
        let current = load(
            include_str!("../../tests/data/relay/golden-floor.toml"),
            "C:/path/to/work/officina.toml",
        )
        .unwrap();
        let d = diff(&previous.canonical, &current.canonical);
        let format = d.format.as_ref().expect("the format changed");
        assert_eq!(
            (format.before.clone(), format.after.clone()),
            (
                serde_json::json!(CANONICAL_FORMAT - 1),
                serde_json::json!(CANONICAL_FORMAT)
            )
        );
        let added: Vec<&String> = current.canonical.templates.keys().collect();
        assert!(!added.is_empty());
        assert_eq!(d.templates.added.iter().collect::<Vec<_>>(), added);
        assert!(d.floor_path.is_none() && d.projects.removed.is_empty());
    }

    #[test]
    fn diff_reports_templates_added_removed_and_changed() {
        let before = canonical_of(&with_templates(&[VALID_TEMPLATE]));
        let other = template_with("name = \"bump\"", "name = \"audit\"");
        let changed = template_with("model = \"sonnet\"", "model = \"opus\"")
            .replace("\"patch\", \"minor\"", "\"patch\", \"minor\", \"major\"");
        let after = canonical_of(&with_templates(&[&changed, &other]));
        let d = diff(&before, &after);
        assert_eq!(d.templates.added, ["audit"]);
        let entry = &d.templates.changed[0];
        assert_eq!(entry.name, "bump");
        let fields: Vec<&str> = entry.fields.iter().map(|f| f.field.as_str()).collect();
        assert_eq!(fields, ["model", "params"]);
        assert!(!d.is_empty() && d.actions.is_empty());
        let removed = diff(&after, &before);
        assert_eq!(removed.templates.removed, ["audit"]);
    }

    #[test]
    fn diff_reports_the_review_model() {
        let before = canonical_of(&floor_with(&[VALID_PROBE]));
        let after = canonical_of(&format!(
            "{}\n[auto_review]\nmodel = \"sonnet\"\n",
            floor_with(&[VALID_PROBE])
        ));
        let d = diff(&before, &after);
        assert!(!d.is_empty());
        let change = d.auto_review.expect("the review model changed");
        assert_eq!(change.field, "model");
        assert_eq!(
            (change.before, change.after),
            (serde_json::json!("haiku"), serde_json::json!("sonnet"))
        );
    }

    #[test]
    fn known_projects_lists_every_declared_project() {
        let content = format!(
            "{}[[project]]\nname = \"free-one\"\npath = \"D:/Other\"\nautonomy = \"free\"\n",
            floor_with(&[VALID_PROBE])
        );
        let floor = parse_floor(&content).unwrap();
        let known = KnownProjects::from_floor(&floor, r"C:\Work\officina.toml", Some("ab".into()));
        assert_eq!(known.project_paths, ["c:/work/lab", "d:/other"]);
        let text = serde_json::to_string(&known).unwrap();
        assert_eq!(parse_known_projects(&text), Some(known));
        assert_eq!(
            parse_known_projects(&text.replace("\"schema\":1", "\"schema\":2")),
            None
        );
    }
}
