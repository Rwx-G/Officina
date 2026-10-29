//! The floor file `officina.toml` (FR-9, AD-8, AD-22): the projects of one
//! workspace, optionally the Claude config dir of its account, and from
//! schema 2 the relay's catalogue.
//!
//! [`parse_floor`] is phase 1 of AD-23: projects are parsed strictly, as in
//! v0.1, while actions, credentials, templates and the `[orchestrator]` and
//! `[auto_review]` tables stay raw spanned values that phase 2
//! (`relay::catalogue::validate`) converts one by one, so a broken one can
//! never invalidate the floor the board uses (AD-75).

use serde::{Deserialize, Serialize};

/// Latest version of the floor file schema.
pub const FLOOR_SCHEMA: u32 = 2;

/// The v0.1 schema, still loaded: every project is `free` (AD-22).
pub const FLOOR_SCHEMA_V1: u32 = 1;

/// Keys only a schema 2 floor may carry. A dotted key names a key of every
/// `[[project]]` table. Asserted against [`FloorFile`] by a test, so a new
/// relay key cannot be forgotten here (D-7).
pub const RELAY_KEYS: [&str; 8] = [
    "guarded_programs",
    "action",
    "credential",
    "template",
    "orchestrator",
    "auto_review",
    "project.autonomy",
    "project.actions",
];

/// Keys valid in every schema that only change what the window shows:
/// they authorise nothing, so they stay out of the relay's canonical
/// catalogue and its approval (AD-168, D-466). A dotted key names a key of
/// every `[[project]]` table. Asserted against [`FloorFile`] by a test.
pub const VIEW_KEYS: [&str; 2] = ["alert", "project.priority"];

/// The escalation threshold when the floor sets none or an invalid one
/// (AD-168).
pub const ALERT_AFTER_MINUTES_DEFAULT: u32 = 10;

/// The thresholds `[alert] after_minutes` accepts, a day at most.
pub const ALERT_AFTER_MINUTES_RANGE: std::ops::RangeInclusive<u32> = 1..=1440;

/// Project colours of the framing's palette. The frontend maps every variant
/// to a CSS variable through an exhaustive record, so a new variant here fails
/// the frontend type check until it has a colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum ProjectColor {
    Green,
    Blue,
    Terracotta,
    Plum,
}

impl ProjectColor {
    /// Every variant, in palette order. Used to assign colours to projects
    /// derived from `cwd`, so the same folder name always gets the same colour.
    pub const ALL: [ProjectColor; 4] = [Self::Green, Self::Blue, Self::Terracotta, Self::Plum];
}

closed_enum! {
    #[serde(rename_all = "lowercase")]
    /// How much a project's waits matter (FR-10, AD-168): a waiting session
    /// of a `high` project is in alert at once.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum Priority {
        /// The default.
        #[default]
        Normal,
        /// Waits escalate to an alert at once.
        High,
    }
}

closed_enum! {
    #[serde(rename_all = "lowercase")]
    /// What the relay's gate does in a project (FR-1).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum Autonomy {
        /// Officina does not interfere.
        Free,
        /// Guarded programs are denied; the session uses `officina run`.
        Guarded,
    }
}

/// One declared project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectEntry {
    pub name: String,
    /// Relative to the floor file's directory, or absolute.
    pub path: String,
    #[serde(default)]
    pub color: Option<ProjectColor>,
    /// As written; read it through [`FloorFile::autonomy_of`], which applies
    /// the schema's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autonomy: Option<Autonomy>,
    /// Names of catalogue actions this project may run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<String>>,
    /// As written, with its position, so a bad value is a view issue with
    /// its line rather than a refused floor (AD-168); read it through
    /// [`FloorFile::priority_of`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<toml::Spanned<toml::Value>>,
}

/// The `[alert]` table (AD-168).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlertTable {
    /// As written, with its position; read it through
    /// [`FloorFile::alert_after_minutes`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_minutes: Option<toml::Spanned<toml::Value>>,
}

/// A view key whose value is not one it accepts: the default stays in force
/// and doctor prints the key and its line (AD-168).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewIssue {
    /// The key as the floor writes it, e.g. `priority` or `after_minutes`.
    pub key: &'static str,
    /// 1-based line of the value.
    pub line: u32,
    pub message: String,
}

/// Content of `officina.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FloorFile {
    pub schema: u32,
    #[serde(default)]
    pub name: Option<String>,
    /// Absolute Claude config dir of this floor's account.
    #[serde(default)]
    pub config_dir: Option<String>,
    /// Drop prompt and assistant excerpts from the board (FR-3).
    #[serde(default)]
    pub no_excerpts: bool,
    /// Programs the gate denies in guarded projects; replaces the default
    /// list when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guarded_programs: Option<Vec<String>>,
    #[serde(default, rename = "project")]
    pub projects: Vec<ProjectEntry>,
    /// Raw `[[action]]` tables with their position in the file. Phase 1 never
    /// interprets them (AD-23).
    #[serde(default, rename = "action", skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<toml::Spanned<toml::Value>>,
    /// Raw `[[credential]]` tables (AD-36), converted by phase 2 like the
    /// actions.
    #[serde(default, rename = "credential", skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<toml::Spanned<toml::Value>>,
    /// Raw `[[template]]` tables (AD-75), converted by phase 2 one by one.
    #[serde(default, rename = "template", skip_serializing_if = "Vec::is_empty")]
    pub templates: Vec<toml::Spanned<toml::Value>>,
    /// The raw `[orchestrator]` table (AD-75), validated by phase 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orchestrator: Option<toml::Spanned<toml::Value>>,
    /// The raw `[auto_review]` table (AD-75), validated by phase 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_review: Option<toml::Spanned<toml::Value>>,
    /// The escalation threshold of the alert (AD-168).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert: Option<AlertTable>,
}

/// The priority a value names, if it names one.
fn priority_value(value: &toml::Value) -> Option<Priority> {
    value.as_str().and_then(Priority::from_code)
}

/// The threshold a value names, if it is an integer in
/// [`ALERT_AFTER_MINUTES_RANGE`].
fn after_minutes_value(value: &toml::Value) -> Option<u32> {
    value
        .as_integer()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| ALERT_AFTER_MINUTES_RANGE.contains(n))
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

impl FloorFile {
    /// The autonomy of `project` under this floor's schema. A schema 1 floor
    /// predates the relay, so its projects are `free`: upgrading Officina
    /// must not start denying commands in repositories the author never
    /// opted in (AD-22). In schema 2 a project without the key is `guarded`.
    pub fn autonomy_of(&self, project: &ProjectEntry) -> Autonomy {
        if self.schema == FLOOR_SCHEMA_V1 {
            return Autonomy::Free;
        }
        project.autonomy.unwrap_or(Autonomy::Guarded)
    }

    /// The priority of `project`: [`Priority::Normal`] when the key is
    /// absent or its value is not one [`Priority`] names, which
    /// [`FloorFile::view_issues`] reports.
    pub fn priority_of(project: &ProjectEntry) -> Priority {
        project
            .priority
            .as_ref()
            .and_then(|v| priority_value(v.get_ref()))
            .unwrap_or_default()
    }

    /// The escalation threshold in minutes: [`ALERT_AFTER_MINUTES_DEFAULT`]
    /// when `[alert] after_minutes` is absent or invalid, which
    /// [`FloorFile::view_issues`] reports.
    pub fn alert_after_minutes(&self) -> u32 {
        self.alert
            .as_ref()
            .and_then(|a| a.after_minutes.as_ref())
            .and_then(|v| after_minutes_value(v.get_ref()))
            .unwrap_or(ALERT_AFTER_MINUTES_DEFAULT)
    }

    /// The view keys whose value is refused, with their line in `content`,
    /// the text this floor was parsed from (AD-168).
    pub fn view_issues(&self, content: &str) -> Vec<ViewIssue> {
        let priorities = self.projects.iter().filter_map(|p| {
            let value = p.priority.as_ref()?;
            priority_value(value.get_ref())
                .is_none()
                .then(|| ViewIssue {
                    key: "priority",
                    line: line_at(content, value.span().start),
                    message: format!(
                        "project `{}`: priority is one of {}; it stays {}",
                        p.name,
                        Priority::ALL
                            .iter()
                            .map(|v| format!("\"{}\"", v.code()))
                            .collect::<Vec<_>>()
                            .join(", "),
                        Priority::default().code()
                    ),
                })
        });
        let threshold = self
            .alert
            .as_ref()
            .and_then(|a| a.after_minutes.as_ref())
            .filter(|v| after_minutes_value(v.get_ref()).is_none())
            .map(|value| ViewIssue {
                key: "after_minutes",
                line: line_at(content, value.span().start),
                message: format!(
                    "[alert] after_minutes is a whole number of minutes from {} to {}; {} applies",
                    ALERT_AFTER_MINUTES_RANGE.start(),
                    ALERT_AFTER_MINUTES_RANGE.end(),
                    ALERT_AFTER_MINUTES_DEFAULT
                ),
            });
        priorities.chain(threshold).collect()
    }
}

/// Why a floor file was refused. The message carries the toml error, line
/// numbers included, so doctor can point at the problem.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FloorError {
    #[error("invalid floor file: {0}")]
    Invalid(String),
    #[error("unsupported floor schema {0}, expected {FLOOR_SCHEMA_V1} or {FLOOR_SCHEMA}")]
    Schema(u32),
    #[error("`{0}` is a relay key: it needs `schema = 2`")]
    RelayKeyNeedsSchema2(&'static str),
    #[error("config_dir must be an absolute path: {0}")]
    RelativeConfigDir(String),
}

/// Parses and validates a floor file (phase 1 of AD-23).
///
/// ```
/// let floor = officina_core::floor::parse_floor(
///     "schema = 1\n[[project]]\nname = \"api\"\npath = \"Api\"\ncolor = \"blue\"\n",
/// ).unwrap();
/// assert_eq!(floor.projects[0].name, "api");
/// assert_eq!(
///     floor.autonomy_of(&floor.projects[0]),
///     officina_core::floor::Autonomy::Free,
/// );
/// ```
pub fn parse_floor(content: &str) -> Result<FloorFile, FloorError> {
    let floor: FloorFile =
        toml::from_str(content).map_err(|e| FloorError::Invalid(e.to_string()))?;
    if floor.schema != FLOOR_SCHEMA_V1 && floor.schema != FLOOR_SCHEMA {
        return Err(FloorError::Schema(floor.schema));
    }
    if floor.schema == FLOOR_SCHEMA_V1
        && let Some(key) = first_relay_key(&floor)
    {
        return Err(FloorError::RelayKeyNeedsSchema2(key));
    }
    if let Some(dir) = &floor.config_dir
        && !is_absolute(dir)
    {
        return Err(FloorError::RelativeConfigDir(dir.clone()));
    }
    Ok(floor)
}

/// The first relay key the floor carries, found in its serialized form so the
/// check follows [`RELAY_KEYS`] rather than a second list of fields. Absent
/// relay fields are skipped on serialization, so a key is present exactly
/// when the file wrote it.
fn first_relay_key(floor: &FloorFile) -> Option<&'static str> {
    let value = serde_json::to_value(floor).ok()?;
    RELAY_KEYS
        .into_iter()
        .find(|key| match key.split_once('.') {
            None => value.get(key).is_some(),
            Some((table, field)) => value
                .get(table)
                .and_then(serde_json::Value::as_array)
                .is_some_and(|items| items.iter().any(|item| item.get(field).is_some())),
        })
}

/// True when a floor file's content sets `no_excerpts = true`. Only that key
/// is read, on purpose: a floor that [`parse_floor`] refuses for an unrelated
/// mistake (an unknown key, another schema) still keeps the author's text out
/// of the event buffer. Content that is not TOML, or a value that is not a
/// boolean, asks for nothing.
///
/// ```
/// use officina_core::floor::requests_no_excerpts;
/// assert!(requests_no_excerpts("schema = 1\nno_excerpts = true\n"));
/// assert!(!requests_no_excerpts("schema = 1\n"));
/// ```
pub fn requests_no_excerpts(content: &str) -> bool {
    #[derive(Deserialize)]
    struct ExcerptSwitch {
        #[serde(default)]
        no_excerpts: bool,
    }
    toml::from_str::<ExcerptSwitch>(content).is_ok_and(|switch| switch.no_excerpts)
}

/// Absolute on Windows (`C:\x`, `\\server\x`) or POSIX (`/x`). Decided on the
/// text so core stays free of the platform's path rules.
pub fn is_absolute(path: &str) -> bool {
    path.starts_with('/') || is_windows_absolute(path)
}

/// Absolute for Windows: a drive letter with a separator, or a UNC path.
/// `/x` is not: Windows resolves it against the current drive.
pub fn is_windows_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with("\\\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/'))
}

/// Joins a project path to the floor directory unless it is absolute, keeping
/// the spelling the floor declares. Programs receive this form as their
/// working directory (D-6); comparisons use [`resolve_project_path`].
pub fn join_project_path(floor_dir: &str, project_path: &str) -> String {
    if is_absolute(project_path) {
        project_path.to_owned()
    } else {
        format!(
            "{}/{}",
            floor_dir.trim_end_matches(['\\', '/']),
            project_path
        )
    }
}

/// Joins a project path to the floor directory unless it is absolute, and
/// normalises the result for comparison with session `cwd` values.
pub fn resolve_project_path(floor_dir: &str, project_path: &str) -> String {
    crate::quota::normalise_path(&join_project_path(floor_dir, project_path))
}

/// The directory part of a floor path, decided on the text (either
/// separator), for resolving relative project paths.
pub fn floor_dir_of(floor_path: &str) -> &str {
    floor_path
        .rfind(['\\', '/'])
        .map_or("", |index| &floor_path[..index])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// A schema 1 floor written with every v0.1 key. The v0.1 schema is
    /// frozen, so this text is a fact, not a copy of the type.
    const V0_1_FLOOR: &str = r#"
schema = 1
name = "work"
config_dir = "C:/path/to/claude-config"
no_excerpts = true

[[project]]
name = "backend"
path = "Backend"
color = "green"
"#;

    #[test]
    fn full_floor_parses() {
        let floor = parse_floor(
            r#"
schema = 1
name = "work"
config_dir = "C:/path/to/claude-config"
no_excerpts = true

[[project]]
name = "backend"
path = "Backend"
color = "green"

[[project]]
name = "scripts"
path = "D:/abs/scripts"
"#,
        )
        .unwrap();
        assert_eq!(floor.projects.len(), 2);
        assert_eq!(floor.projects[1].color, None);
        assert!(floor.no_excerpts);
    }

    #[test]
    fn schema_2_example_parses() {
        let floor = parse_floor(
            r#"
schema = 2
guarded_programs = ["ansible*", "ssh", "scp", "sftp", "rsync", "wsl"]

[[project]]
name = "supervision"
path = "Supervision"
autonomy = "guarded"
actions = ["check", "apply"]

[[action]]
name = "check"
description = "Dry run of a playbook on one lab group"
class = "free"
runner = "wsl"
distro = "Ubuntu"
program = "/home/user01/venv/bin/ansible-playbook"
env = { ANSIBLE_CONFIG = "/home/user01/ansible.cfg" }
args = ["--check", "--diff", "-i", "inventory/lab", "--limit", "{group}", "playbooks/{playbook}"]
timeout_s = 600
max_output_kib = 128

[action.params.playbook]
values = ["alerting.yml", "node-exporter.yml"]

[action.params.group]
pattern = "lab-[a-z0-9-]{1,30}"

[[action]]
name = "apply"
class = "after-check"
check = "check"
program = "C:/path/to/tool.exe"
"#,
        )
        .unwrap();
        assert_eq!(floor.schema, FLOOR_SCHEMA);
        assert_eq!(
            floor.guarded_programs.as_deref().map(<[String]>::len),
            Some(6)
        );
        let project = &floor.projects[0];
        assert_eq!(floor.autonomy_of(project), Autonomy::Guarded);
        assert_eq!(
            project.actions.as_deref(),
            Some(&["check".to_owned(), "apply".to_owned()][..])
        );
        assert_eq!(floor.actions.len(), 2);
        let check = floor.actions[0].get_ref();
        assert_eq!(check["runner"].as_str(), Some("wsl"));
        assert_eq!(
            check["params"]["group"]["pattern"].as_str(),
            Some("lab-[a-z0-9-]{1,30}")
        );
    }

    #[test]
    fn schema_2_project_defaults_to_guarded() {
        let floor = parse_floor(
            "schema = 2\n[[project]]\nname = \"a\"\npath = \"a\"\n[[project]]\nname = \"b\"\npath = \"b\"\nautonomy = \"free\"\n",
        )
        .unwrap();
        assert_eq!(floor.autonomy_of(&floor.projects[0]), Autonomy::Guarded);
        assert_eq!(floor.autonomy_of(&floor.projects[1]), Autonomy::Free);
    }

    #[test]
    fn schema_1_projects_are_free() {
        let floor = parse_floor(V0_1_FLOOR).unwrap();
        assert!(
            floor
                .projects
                .iter()
                .all(|p| floor.autonomy_of(p) == Autonomy::Free)
        );
    }

    /// A schema 1 floor carrying `key`, with a value of the right type so
    /// the refusal can only come from the schema rule.
    fn schema_1_with(key: &str) -> String {
        let project = "[[project]]\nname = \"a\"\npath = \"a\"\n";
        match key.split_once('.') {
            Some(("project", "autonomy")) => format!("schema = 1\n{project}autonomy = \"free\"\n"),
            Some(("project", "actions")) => format!("schema = 1\n{project}actions = [\"x\"]\n"),
            Some((table, field)) => panic!("no sample for the relay key {table}.{field}"),
            None if ["action", "credential", "template"].contains(&key) => {
                format!("schema = 1\n{project}[[{key}]]\nname = \"x\"\n")
            }
            None if ["orchestrator", "auto_review"].contains(&key) => {
                format!("schema = 1\n{project}[{key}]\nmodel = \"x\"\n")
            }
            None => format!("schema = 1\n{key} = [\"x\"]\n{project}"),
        }
    }

    /// Story 5.4 AC 1: a template is a schema 2 key, and phase 1 keeps it
    /// raw, so a schema 2 floor with a broken one still parses.
    #[test]
    fn a_schema_1_floor_with_a_template_names_schema_2() {
        let error = parse_floor(&schema_1_with("template")).unwrap_err();
        assert_eq!(error, FloorError::RelayKeyNeedsSchema2("template"));
        assert!(error.to_string().contains("schema = 2"));
        let broken = "schema = 2\n[[project]]\nname = \"a\"\npath = \"a\"\n[[template]]\nname = 7\nunknown = true\n[orchestrator]\nmodel = 3\n[auto_review]\nwhatever = \"x\"\n";
        let floor = parse_floor(broken).expect("phase 1 keeps the new tables raw");
        assert_eq!(floor.templates.len(), 1);
        assert!(floor.orchestrator.is_some() && floor.auto_review.is_some());
        assert_eq!(floor.projects.len(), 1);
    }

    #[test]
    fn schema_1_with_each_relay_key_names_schema_2() {
        for key in RELAY_KEYS {
            let error = parse_floor(&schema_1_with(key)).unwrap_err();
            assert_eq!(error, FloorError::RelayKeyNeedsSchema2(key), "{key}");
            assert!(error.to_string().contains("schema = 2"), "{key}");
            let as_schema_2 = schema_1_with(key).replacen("schema = 1", "schema = 2", 1);
            assert!(parse_floor(&as_schema_2).is_ok(), "{key}");
        }
    }

    #[test]
    fn unsupported_schema_is_refused() {
        for schema in [0, 3, 99] {
            let error = parse_floor(&format!("schema = {schema}\n")).unwrap_err();
            assert_eq!(error, FloorError::Schema(schema));
            assert!(error.to_string().contains("expected 1 or 2"));
        }
    }

    /// Top-level keys, plus `project.<key>` for the keys of project tables.
    fn serialized_keys(floor: &FloorFile) -> BTreeSet<String> {
        let value = serde_json::to_value(floor).unwrap();
        let mut keys = BTreeSet::new();
        for (key, item) in value.as_object().unwrap() {
            keys.insert(key.clone());
            for table in item.as_array().into_iter().flatten() {
                for field in table.as_object().into_iter().flat_map(|t| t.keys()) {
                    keys.insert(format!("{key}.{field}"));
                }
            }
        }
        keys
    }

    #[test]
    fn relay_keys_constant_matches_the_type() {
        // Struct literals: a field added to either type fails to compile here
        // until it is populated, and then shows up in the key comparison.
        // An action's or a credential's own keys belong to phase 2, not to
        // this type, hence the empty tables.
        let action = toml::Value::Table(toml::Table::new());
        let full = FloorFile {
            schema: FLOOR_SCHEMA,
            name: Some("work".to_owned()),
            config_dir: Some("C:/path/to/claude-config".to_owned()),
            no_excerpts: true,
            guarded_programs: Some(vec!["ssh".to_owned()]),
            projects: vec![ProjectEntry {
                name: "a".to_owned(),
                path: "a".to_owned(),
                color: Some(ProjectColor::Blue),
                autonomy: Some(Autonomy::Guarded),
                actions: Some(vec!["x".to_owned()]),
                priority: Some(toml::Spanned::new(0..0, toml::Value::from("high"))),
            }],
            actions: vec![toml::Spanned::new(0..0, action.clone())],
            credentials: vec![toml::Spanned::new(0..0, action.clone())],
            templates: vec![toml::Spanned::new(0..0, action.clone())],
            orchestrator: Some(toml::Spanned::new(0..0, action.clone())),
            auto_review: Some(toml::Spanned::new(0..0, action)),
            alert: Some(AlertTable {
                after_minutes: Some(toml::Spanned::new(0..0, toml::Value::from(10))),
            }),
        };
        let v0_1 = serialized_keys(&parse_floor(V0_1_FLOOR).unwrap());
        let view: BTreeSet<String> = VIEW_KEYS.iter().map(|k| (*k).to_owned()).collect();
        let added: BTreeSet<String> = serialized_keys(&full).difference(&v0_1).cloned().collect();
        let relay: BTreeSet<String> = added.difference(&view).cloned().collect();
        let constant: BTreeSet<String> = RELAY_KEYS.iter().map(|k| (*k).to_owned()).collect();
        assert_eq!(relay, constant);
        assert!(
            view.is_subset(&added),
            "every view key is a field of the type"
        );
    }

    /// A floor of `schema` with one project carrying `priority` (raw TOML)
    /// and, when given, an `[alert]` table holding `after_minutes`.
    fn view_floor(schema: u32, priority: Option<&str>, after: Option<&str>) -> String {
        let mut text = format!("schema = {schema}\n[[project]]\nname = \"api\"\npath = \"api\"\n");
        if let Some(value) = priority {
            text.push_str(&format!("priority = {value}\n"));
        }
        if let Some(value) = after {
            text.push_str(&format!("[alert]\nafter_minutes = {value}\n"));
        }
        text
    }

    #[test]
    fn priority_defaults_to_normal() {
        let floor = parse_floor(&view_floor(1, None, None)).unwrap();
        assert_eq!(FloorFile::priority_of(&floor.projects[0]), Priority::Normal);
        assert!(floor.view_issues(&view_floor(1, None, None)).is_empty());
    }

    #[test]
    fn priority_high_is_read() {
        for schema in [FLOOR_SCHEMA_V1, FLOOR_SCHEMA] {
            for priority in Priority::ALL {
                let text = view_floor(schema, Some(&format!("\"{}\"", priority.code())), None);
                let floor = parse_floor(&text).unwrap();
                assert_eq!(FloorFile::priority_of(&floor.projects[0]), *priority);
                assert!(floor.view_issues(&text).is_empty(), "{text}");
            }
        }
    }

    #[test]
    fn an_unknown_priority_is_an_issue_and_stays_normal() {
        for bad in ["\"urgent\"", "\"HIGH\"", "3", "true"] {
            let text = view_floor(FLOOR_SCHEMA, Some(bad), None);
            let floor = parse_floor(&text).unwrap();
            assert_eq!(
                FloorFile::priority_of(&floor.projects[0]),
                Priority::Normal,
                "{bad}"
            );
            let issues = floor.view_issues(&text);
            assert_eq!(issues.len(), 1, "{bad}");
            assert_eq!((issues[0].key, issues[0].line), ("priority", 5), "{bad}");
            assert!(issues[0].message.contains("api"), "{}", issues[0].message);
        }
    }

    #[test]
    fn alert_threshold_defaults_to_ten_minutes() {
        let floor = parse_floor(&view_floor(1, None, None)).unwrap();
        assert_eq!(floor.alert_after_minutes(), ALERT_AFTER_MINUTES_DEFAULT);
        let empty_table = "schema = 1\n[alert]\n";
        assert_eq!(
            parse_floor(empty_table).unwrap().alert_after_minutes(),
            ALERT_AFTER_MINUTES_DEFAULT
        );
        for minutes in [
            *ALERT_AFTER_MINUTES_RANGE.start(),
            25,
            *ALERT_AFTER_MINUTES_RANGE.end(),
        ] {
            let text = view_floor(FLOOR_SCHEMA, None, Some(&minutes.to_string()));
            let floor = parse_floor(&text).unwrap();
            assert_eq!(floor.alert_after_minutes(), minutes);
            assert!(floor.view_issues(&text).is_empty());
        }
    }

    #[test]
    fn alert_threshold_out_of_range_is_an_issue() {
        let below = ALERT_AFTER_MINUTES_RANGE.start() - 1;
        let above = ALERT_AFTER_MINUTES_RANGE.end() + 1;
        for bad in [
            below.to_string(),
            above.to_string(),
            "-5".to_owned(),
            "2.5".to_owned(),
            "\"10\"".to_owned(),
        ] {
            let text = view_floor(FLOOR_SCHEMA, None, Some(&bad));
            let floor = parse_floor(&text).unwrap();
            assert_eq!(
                floor.alert_after_minutes(),
                ALERT_AFTER_MINUTES_DEFAULT,
                "{bad}"
            );
            let issues = floor.view_issues(&text);
            assert_eq!(issues.len(), 1, "{bad}");
            assert_eq!(
                (issues[0].key, issues[0].line),
                ("after_minutes", 6),
                "{bad}"
            );
        }
        assert!(
            parse_floor("schema = 1\n[alert]\nafter = 3\n").is_err(),
            "an unknown key in [alert]"
        );
    }

    #[test]
    fn plum_is_a_project_colour() {
        assert!(ProjectColor::ALL.contains(&ProjectColor::Plum));
        let floor =
            parse_floor("schema = 1\n[[project]]\nname = \"a\"\npath = \"a\"\ncolor = \"plum\"\n")
                .unwrap();
        assert_eq!(floor.projects[0].color, Some(ProjectColor::Plum));
    }

    #[test]
    fn errors_are_typed_and_located() {
        let unknown_key = parse_floor("schema = 1\nfoo = 2\n").unwrap_err();
        assert!(matches!(unknown_key, FloorError::Invalid(ref m) if m.contains("foo")));
        let bad_color =
            parse_floor("schema = 1\n[[project]]\nname=\"a\"\npath=\"a\"\ncolor=\"pink\"\n")
                .unwrap_err();
        assert!(matches!(bad_color, FloorError::Invalid(ref m) if m.contains("line")));
        assert_eq!(
            parse_floor("schema = 1\nconfig_dir = \"rel/dir\"\n").unwrap_err(),
            FloorError::RelativeConfigDir("rel/dir".into())
        );
    }

    #[test]
    fn no_excerpts_is_read_even_from_a_refused_floor() {
        let refused = "schema = 2\nno_excerpts = true\nunknown_key = 1\n";
        assert!(parse_floor(refused).is_err());
        assert!(requests_no_excerpts(refused));
        assert!(requests_no_excerpts(
            "no_excerpts = true\n[[project]]\nname = \"a\"\npath = \"a\"\n"
        ));
        for asks_nothing in [
            "",
            "schema = 1\nno_excerpts = false\n",
            "no_excerpts = \"yes\"\n",
            "no_excerpts = true\n[[[",
            "[section]\nno_excerpts = true\n",
        ] {
            assert!(!requests_no_excerpts(asks_nothing), "{asks_nothing}");
        }
    }

    #[test]
    fn absolute_paths_and_resolution() {
        assert!(is_absolute(r"C:\x") && is_absolute("c:/x") && is_absolute(r"\\srv\share"));
        assert!(is_absolute("/home/x") && !is_absolute("x/y") && !is_absolute("C:x"));
        assert!(is_windows_absolute("c:/x") && !is_windows_absolute("/home/x"));
        assert_eq!(resolve_project_path(r"C:\Work\", "Api"), "c:/work/api");
        assert_eq!(resolve_project_path(r"C:\Work", r"D:\Other"), "d:/other");
        assert_eq!(join_project_path(r"C:\Work\", "Api"), r"C:\Work/Api");
        assert_eq!(floor_dir_of(r"C:\Work\officina.toml"), r"C:\Work");
        assert_eq!(floor_dir_of("C:/Work/sub/officina.toml"), "C:/Work/sub");
        assert_eq!(floor_dir_of("officina.toml"), "");
    }
}
