//! The floor file `officina.toml` (FR-9, AD-8): the projects of one workspace
//! and, optionally, the Claude config dir of its account.

use serde::{Deserialize, Serialize};

/// Version of the floor file schema.
pub const FLOOR_SCHEMA: u32 = 1;

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
}

impl ProjectColor {
    /// Every variant, in palette order. Used to assign colours to projects
    /// derived from `cwd`, so the same folder name always gets the same colour.
    pub const ALL: [ProjectColor; 3] = [Self::Green, Self::Blue, Self::Terracotta];
}

/// One declared project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectEntry {
    pub name: String,
    /// Relative to the floor file's directory, or absolute.
    pub path: String,
    #[serde(default)]
    pub color: Option<ProjectColor>,
}

/// Content of `officina.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default, rename = "project")]
    pub projects: Vec<ProjectEntry>,
}

/// Why a floor file was refused. The message carries the toml error, line
/// numbers included, so doctor can point at the problem.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FloorError {
    #[error("invalid floor file: {0}")]
    Invalid(String),
    #[error("unsupported floor schema {0}, expected {FLOOR_SCHEMA}")]
    Schema(u32),
    #[error("config_dir must be an absolute path: {0}")]
    RelativeConfigDir(String),
}

/// Parses and validates a floor file.
///
/// ```
/// let floor = officina_core::floor::parse_floor(
///     "schema = 1\n[[project]]\nname = \"api\"\npath = \"Api\"\ncolor = \"blue\"\n",
/// ).unwrap();
/// assert_eq!(floor.projects[0].name, "api");
/// ```
pub fn parse_floor(content: &str) -> Result<FloorFile, FloorError> {
    let floor: FloorFile =
        toml::from_str(content).map_err(|e| FloorError::Invalid(e.to_string()))?;
    if floor.schema != FLOOR_SCHEMA {
        return Err(FloorError::Schema(floor.schema));
    }
    if let Some(dir) = &floor.config_dir
        && !is_absolute(dir)
    {
        return Err(FloorError::RelativeConfigDir(dir.clone()));
    }
    Ok(floor)
}

/// Absolute on Windows (`C:\x`, `\\server\x`) or POSIX (`/x`). Decided on the
/// text so core stays free of the platform's path rules.
pub fn is_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || path.starts_with("\\\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/'))
}

/// Joins a project path to the floor directory unless it is absolute, and
/// normalises the result for comparison with session `cwd` values.
pub fn resolve_project_path(floor_dir: &str, project_path: &str) -> String {
    let joined = if is_absolute(project_path) {
        project_path.to_owned()
    } else {
        format!(
            "{}/{}",
            floor_dir.trim_end_matches(['\\', '/']),
            project_path
        )
    };
    crate::quota::normalise_path(&joined)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn errors_are_typed_and_located() {
        let unknown_key = parse_floor("schema = 1\nfoo = 2\n").unwrap_err();
        assert!(matches!(unknown_key, FloorError::Invalid(ref m) if m.contains("foo")));
        let bad_color =
            parse_floor("schema = 1\n[[project]]\nname=\"a\"\npath=\"a\"\ncolor=\"pink\"\n")
                .unwrap_err();
        assert!(matches!(bad_color, FloorError::Invalid(ref m) if m.contains("line")));
        assert_eq!(
            parse_floor("schema = 2\n").unwrap_err(),
            FloorError::Schema(2)
        );
        assert_eq!(
            parse_floor("schema = 1\nconfig_dir = \"rel/dir\"\n").unwrap_err(),
            FloorError::RelativeConfigDir("rel/dir".into())
        );
    }

    #[test]
    fn absolute_paths_and_resolution() {
        assert!(is_absolute(r"C:\x") && is_absolute("c:/x") && is_absolute(r"\\srv\share"));
        assert!(is_absolute("/home/x") && !is_absolute("x/y") && !is_absolute("C:x"));
        assert_eq!(resolve_project_path(r"C:\Work\", "Api"), "c:/work/api");
        assert_eq!(resolve_project_path(r"C:\Work", r"D:\Other"), "d:/other");
    }
}
