//! The catalogue (FR-1, FR-2, FR-4, AD-22 to AD-24): phase 2 of the floor
//! parse, the canonical form the author approves, its hash and its diff.
//!
//! Phase 2 converts each raw `[[action]]` alone and validates it; a failure
//! becomes an [`ActionIssue`] and drops that action, never the floor. Issues
//! go to the relay view and doctor, never to the board's `floor_error`.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use super::params::{self, Param, ParamRule, PatternError};
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

/// Longest action or parameter name.
pub const MAX_NAME_CHARS: usize = 32;
/// Longest `env` key (D-1).
pub const MAX_ENV_KEY_CHARS: usize = 64;
/// Longest `distro` (D-4).
pub const MAX_DISTRO_CHARS: usize = 64;

/// Version of the canonical form. Bumping it invalidates every approval on
/// purpose (AD-24).
pub const CANONICAL_FORMAT: u32 = 1;

/// Version of `relay/known-projects.json` (D-15).
pub const KNOWN_PROJECTS_SCHEMA: u32 = 1;

/// The name an issue carries when the action has no readable name.
pub const UNNAMED: &str = "(unnamed)";

closed_enum! {
    /// Where an action runs (AD-22, AD-30).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[serde(rename_all = "lowercase")]
    pub enum Runner {
        /// The program itself, a Windows absolute path.
        #[default]
        Native,
        /// `wsl.exe --exec`, the program a POSIX absolute path.
        Wsl,
    }
}

/// How an action is allowed (FR-8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "kebab-case")]
pub enum ActionClass {
    Free,
    AfterCheck,
    Human,
    /// Reserved for the orchestrator (v0.5): always an issue.
    Auto,
}

closed_enum! {
    /// One rule of AD-23 or of D-1 to D-5. Every variant has a failing
    /// fixture in this module's tests.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    #[serde(rename_all = "snake_case")]
    pub enum IssueKind {
        /// The table does not convert: unknown key, wrong type, missing key.
        Malformed,
        /// `kind = "http"`: not in this build until AD-36.
        HttpNotInThisBuild,
        /// `class = "auto"`: reserved for v0.5.
        AutoReserved,
        /// The name is not `[a-z][a-z0-9-]` of at most 32 characters.
        Name,
        /// Two actions share a name: both are dropped.
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
        /// An unbalanced brace or a second placeholder in an `args` item (D-2).
        Braces,
        /// A placeholder names no declared parameter.
        UndeclaredPlaceholder,
        /// A declared parameter no `args` item uses.
        UnusedParameter,
        /// A parameter name is not `[a-z][a-z0-9_-]` of at most 32 characters.
        ParamName,
        /// A parameter declares both or neither of `values` and `pattern`.
        ValuesOrPattern,
        /// A `values` entry breaks a value rule for the action's runner.
        InvalidValue,
        /// A `pattern` over the length limit.
        PatternTooLong,
        /// A `pattern` over the compiled size limit.
        PatternTooComplex,
        /// A `pattern` that does not compile.
        PatternInvalid,
        /// `check` is missing on an `after-check` action, set on another
        /// class, or does not name a valid `free` action with the same
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
    }
}

/// A problem found in phase 2. `name` is the action's, or for a project or
/// `guarded_programs` issue the project's name or the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ActionIssue {
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

impl ArgTemplate {
    /// Parses one `args` item. Braces delimit placeholders only (D-2).
    ///
    /// ```
    /// use officina_core::relay::catalogue::ArgTemplate;
    /// assert_eq!(
    ///     ArgTemplate::parse("--limit={group}").unwrap().param(),
    ///     Some("group"),
    /// );
    /// assert!(ArgTemplate::parse("{a}{b}").is_err());
    /// ```
    pub fn parse(item: &str) -> Result<ArgTemplate, String> {
        let Some(open) = item.find(['{', '}']) else {
            return Ok(ArgTemplate::Literal(item.to_owned()));
        };
        if item[open..].starts_with('}') {
            return Err(format!("`}}` closes no placeholder in {item:?}"));
        }
        let inner = &item[open + 1..];
        let Some(close) = inner.find(['{', '}']) else {
            return Err(format!("unbalanced `{{` in {item:?}"));
        };
        if inner[close..].starts_with('{') {
            return Err(format!("`{{` inside a placeholder in {item:?}"));
        }
        let suffix = &inner[close + 1..];
        if let Some(next) = suffix.find(['{', '}']) {
            let rest = &suffix[next..];
            return Err(if rest.starts_with('{') && rest.contains('}') {
                format!("a second placeholder in {item:?}: one argument holds at most one")
            } else {
                format!("unbalanced brace in {item:?}")
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

/// A validated action, every default explicit.
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
    pub params: BTreeMap<String, Param>,
    pub check: Option<String>,
    pub timeout_s: u32,
    pub max_output_kib: u32,
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
#[derive(Debug, Clone, Default)]
pub struct Catalogue {
    /// Defaults applied, lowercase, sorted, deduplicated.
    pub guarded_programs: Vec<String>,
    /// Sorted by name.
    pub projects: Vec<CatalogueProject>,
    pub actions: BTreeMap<String, Action>,
    /// In file order; issues without a line last.
    pub issues: Vec<ActionIssue>,
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
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawParam {
    #[serde(default)]
    values: Option<Vec<String>>,
    #[serde(default)]
    pattern: Option<String>,
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
/// every `Bash` call (phase 1, AD-28).
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

/// `[a-z]` then up to 31 of `[a-z0-9]` and `extra`.
fn is_lower_name(name: &str, extra: &[char]) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && name.chars().count() <= MAX_NAME_CHARS
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || extra.contains(&c))
}

/// An action name: `^[a-z][a-z0-9-]{0,31}$` (AD-23).
pub fn is_valid_action_name(name: &str) -> bool {
    is_lower_name(name, &['-'])
}

/// A parameter name: `^[a-z][a-z0-9_-]{0,31}$`, so it can be passed as
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

/// Converts and validates one action alone: every rule that needs no other
/// action. The issues carry no line; [`validate`] adds it.
///
/// ```
/// let value: toml::Value = toml::from_str(
///     "name = \"probe\"\nclass = \"free\"\nprogram = \"C:/path/to/tool.exe\"\n",
/// ).unwrap();
/// let action = officina_core::relay::catalogue::action_from_value(&value).unwrap();
/// assert_eq!(action.timeout_s, officina_core::relay::catalogue::DEFAULT_TIMEOUT_S);
/// ```
pub fn action_from_value(value: &toml::Value) -> Result<Action, Vec<ActionIssue>> {
    let name = value
        .get("name")
        .and_then(toml::Value::as_str)
        .unwrap_or(UNNAMED)
        .to_owned();
    let issue = |kind: IssueKind, message: String| ActionIssue {
        name: name.clone(),
        line: None,
        kind,
        message,
    };
    if value.get("kind").and_then(toml::Value::as_str) == Some("http") {
        return Err(vec![issue(
            IssueKind::HttpNotInThisBuild,
            "HTTP actions are not in this build".to_owned(),
        )]);
    }
    let raw: RawAction = value.clone().try_into().map_err(|e: toml::de::Error| {
        vec![issue(IssueKind::Malformed, e.message().trim().to_owned())]
    })?;
    let mut issues = Vec::new();
    let mut report = |kind: IssueKind, message: String| issues.push(issue(kind, message));

    if !is_valid_action_name(&raw.name) {
        report(
            IssueKind::Name,
            format!(
                "the name must be a lowercase letter followed by lowercase letters, digits or `-`, at most {MAX_NAME_CHARS} characters"
            ),
        );
    }
    if raw.class == ActionClass::Auto {
        report(
            IssueKind::AutoReserved,
            "class `auto` is reserved for the orchestrator (v0.5)".to_owned(),
        );
    }

    if raw.program.contains(['{', '}']) {
        report(
            IssueKind::ProgramPlaceholder,
            "`program` is a fixed path: it cannot hold a placeholder".to_owned(),
        );
    }
    let absolute = match raw.runner {
        Runner::Native => is_windows_absolute(&raw.program),
        Runner::Wsl => raw.program.starts_with('/'),
    };
    if !absolute {
        report(
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
        report(
            IssueKind::BatchProgram,
            "a `.bat` or `.cmd` program is re-parsed by cmd.exe: run its interpreter's real program instead".to_owned(),
        );
    }

    if let Some(distro) = &raw.distro {
        if raw.runner != Runner::Wsl {
            report(
                IssueKind::DistroWithoutWsl,
                "`distro` applies to `runner = \"wsl\"` only".to_owned(),
            );
        }
        if !is_valid_distro(distro) {
            report(
                IssueKind::Distro,
                format!(
                    "`distro` must start with a letter or digit, then letters, digits, `.`, `_` or `-`, at most {MAX_DISTRO_CHARS} characters"
                ),
            );
        }
    }

    for (key, value) in &raw.env {
        if !is_valid_env_key(key) {
            report(
                IssueKind::EnvKey,
                format!(
                    "`env` key {key:?} must be a letter or `_`, then letters, digits or `_`, at most {MAX_ENV_KEY_CHARS} characters"
                ),
            );
        }
        if value.contains(['{', '}']) {
            report(
                IssueKind::EnvValuePlaceholder,
                format!(
                    "`env` value of {key} holds a brace: env values are literals, never placeholders"
                ),
            );
        }
    }

    let mut args = Vec::new();
    for item in &raw.args {
        match ArgTemplate::parse(item) {
            Ok(template) => args.push(template),
            Err(message) => report(IssueKind::Braces, message),
        }
    }

    let mut params = BTreeMap::new();
    for (param_name, raw_param) in &raw.params {
        if !is_valid_param_name(param_name) {
            report(
                IssueKind::ParamName,
                format!(
                    "parameter {param_name:?} must be a lowercase letter followed by lowercase letters, digits, `_` or `-`, at most {MAX_NAME_CHARS} characters"
                ),
            );
        }
        let rule = match (&raw_param.values, &raw_param.pattern) {
            (Some(values), None) => {
                for value in values {
                    if let Err(refusal) = params::check_value(raw.runner, value) {
                        report(
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
                    report(kind, format!("parameter {param_name}: {error}"));
                    None
                }
            },
            _ => {
                report(
                    IssueKind::ValuesOrPattern,
                    format!("parameter {param_name} declares exactly one of `values` or `pattern`"),
                );
                None
            }
        };
        if let Some(rule) = rule {
            params.insert(param_name.clone(), Param { rule });
        }
    }

    let used: BTreeSet<&str> = args.iter().filter_map(ArgTemplate::param).collect();
    for placeholder in &used {
        if !raw.params.contains_key(*placeholder) {
            report(
                IssueKind::UndeclaredPlaceholder,
                format!("placeholder {{{placeholder}}} names no declared parameter"),
            );
        }
    }
    for param_name in raw.params.keys() {
        if !used.contains(param_name.as_str()) {
            report(
                IssueKind::UnusedParameter,
                format!("parameter {param_name} is used by no `args` item"),
            );
        }
    }

    let timeout_s = raw.timeout_s.unwrap_or(i64::from(DEFAULT_TIMEOUT_S));
    if !TIMEOUT_RANGE_S.contains(&timeout_s) {
        report(
            IssueKind::Timeout,
            format!(
                "`timeout_s` must be {} to {}",
                TIMEOUT_RANGE_S.start(),
                TIMEOUT_RANGE_S.end()
            ),
        );
    }
    let max_output_kib = raw
        .max_output_kib
        .unwrap_or(i64::from(DEFAULT_MAX_OUTPUT_KIB));
    if !MAX_OUTPUT_RANGE_KIB.contains(&max_output_kib) {
        report(
            IssueKind::MaxOutput,
            format!(
                "`max_output_kib` must be {} to {}",
                MAX_OUTPUT_RANGE_KIB.start(),
                MAX_OUTPUT_RANGE_KIB.end()
            ),
        );
    }

    // An `after-check` action needs its check; a `human` action may name one,
    // its dry run (D-170); a `free` action runs without any, so naming one
    // would promise a binding it never has.
    match (raw.class, &raw.check) {
        (ActionClass::AfterCheck, None) => report(
            IssueKind::Check,
            "an `after-check` action names its check action in `check`".to_owned(),
        ),
        (ActionClass::AfterCheck | ActionClass::Human, _) | (_, None) => {}
        (_, Some(_)) => report(
            IssueKind::Check,
            "only an `after-check` or `human` action names a `check`".to_owned(),
        ),
    }

    if !issues.is_empty() {
        return Err(issues);
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
        params,
        check: raw.check,
        timeout_s: u32::try_from(timeout_s).unwrap_or(DEFAULT_TIMEOUT_S),
        max_output_kib: u32::try_from(max_output_kib).unwrap_or(DEFAULT_MAX_OUTPUT_KIB),
    })
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

/// Phase 2 of AD-23: converts and validates each action alone, then the
/// rules across actions (unique names, `check` targets), then the projects'
/// action lists. `content` is the text `floor` was parsed from, for lines.
pub fn validate(floor: &FloorFile, content: &str) -> Catalogue {
    let positions: Positions = toml::from_str(content).unwrap_or_default();
    let line_of = |spanned: Option<&toml::Spanned<toml::Value>>| {
        spanned.map(|s| line_at(content, s.span().start))
    };
    let mut issues = Vec::new();

    for (index, entry) in floor.guarded_programs.iter().flatten().enumerate() {
        if let Some(problem) = guarded_program_problem(entry) {
            issues.push(ActionIssue {
                name: "guarded_programs".to_owned(),
                line: line_of(positions.guarded_programs.get(index)),
                kind: IssueKind::GuardedProgram,
                message: format!("entry {entry:?} is ignored: {problem}"),
            });
        }
    }

    let mut name_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for raw in &floor.actions {
        if let Some(name) = raw.get_ref().get("name").and_then(toml::Value::as_str) {
            *name_counts.entry(name).or_default() += 1;
        }
    }
    let mut candidates: BTreeMap<String, (Action, u32)> = BTreeMap::new();
    for raw in &floor.actions {
        let line = line_at(content, raw.span().start);
        match action_from_value(raw.get_ref()) {
            Ok(action) if name_counts.get(action.name.as_str()) > Some(&1) => {
                issues.push(ActionIssue {
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

    let mut actions = BTreeMap::new();
    for (name, (action, line)) in &candidates {
        if let Some(problem) = check_target_problem(action, &candidates) {
            issues.push(ActionIssue {
                name: name.clone(),
                line: Some(*line),
                kind: IssueKind::Check,
                message: problem,
            });
        } else {
            actions.insert(name.clone(), action.clone());
        }
    }

    let mut project_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for project in &floor.projects {
        *project_counts.entry(project.name.as_str()).or_default() += 1;
    }
    let mut projects = Vec::new();
    for (index, project) in floor.projects.iter().enumerate() {
        let line = line_of(positions.project.get(index));
        if project_counts.get(project.name.as_str()) > Some(&1) {
            issues.push(ActionIssue {
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
    issues.sort_by_key(|issue| issue.line.unwrap_or(u32::MAX));

    Catalogue {
        guarded_programs: guarded_programs(floor),
        projects,
        actions,
        issues,
    }
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

/// The canonical form of one valid action, every default explicit.
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
    pub params: BTreeMap<String, CanonicalParam>,
    pub check: Option<String>,
    pub timeout_s: u32,
    pub max_output_kib: u32,
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
/// Colours, the floor's `name`, `config_dir`, `no_excerpts`, comments and
/// invalid actions are not part of it.
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
    }
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
    fields.retain(|_, value| !value.is_null());
    fields.insert("name".to_owned(), name.into());
    let value = toml::Value::try_from(serde_json::Value::Object(fields)).map_err(|e| {
        vec![ActionIssue {
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
            IssueKind::HttpNotInThisBuild => floor_with(&[
                "name = \"probe\"\nkind = \"http\"\nurl = \"https://example.com/status\"\n",
            ]),
            IssueKind::AutoReserved => floor_with(&[&replaced("\"free\"", "\"auto\"")]),
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

    #[test]
    fn every_issue_kind_has_a_case() {
        for kind in IssueKind::ALL {
            with_issue(&failing_fixture(*kind), *kind);
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
    fn http_is_not_in_this_build() {
        let catalogue = validated(&failing_fixture(IssueKind::HttpNotInThisBuild));
        assert!(
            issue_of(&catalogue, IssueKind::HttpNotInThisBuild)
                .message
                .contains("not in this build")
        );
    }

    #[test]
    fn auto_is_reserved() {
        dropped(IssueKind::AutoReserved);
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
        for broken in [
            "{a",
            "a}",
            "}a{",
            "{a}}",
            "{{a}}",
            "{a}{b}",
            "x{a}y{b}z",
            "{a}b}",
        ] {
            assert!(ArgTemplate::parse(broken).is_err(), "{broken}");
        }
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
            &probe_top(
                "runner = \"wsl\"\ndistro = \"Debian\"\nenv = { MODE = \"x\" }\ntimeout_s = 30\n",
            )
            .replace("C:/path/to/tool.exe", "/usr/bin/tool")
            .replace("\"probe\"", "\"remote\""),
            VALID_PROBE,
        ]);
        let catalogue = canonical(&validated(&content), "C:/path/to/officina.toml");
        assert!(
            catalogue.actions.len() >= 4,
            "{:?}",
            catalogue.actions.keys()
        );
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
        let canonical = canonical_of(&floor_with(&[VALID_PROBE]));
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
        let d = diff(&canonical, &moved);
        assert_eq!(d.floor_path.unwrap().after, "d:/other/officina.toml");
        assert_eq!(d.format.unwrap().before, CANONICAL_FORMAT);
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
