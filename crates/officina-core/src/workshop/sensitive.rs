//! Sensitive groups (AD-113, FR-11, D-246): the paths of a package that
//! configure a tool, a build, CI, the author's editor or Claude Code, and
//! the modes that make a file run. They direct the author's attention
//! before Import; they are not the boundary. Each group is approved
//! against the SHA-256 of the exact hunks shown (AD-24's pattern), and a
//! `file` group is keyed by an index, so no worker text becomes a key.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::floor::ReviewScope;
use crate::relay::catalogue::CanonicalCatalogue;

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// A family of sensitive changes (AD-113, D-246). Declaration order
    /// is the order the review shows them in.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum SensitiveGroup {
        ClaudeConfig,
        Git,
        Ci,
        BuildScripts,
        ToolchainConfig,
        EditorShell,
        /// The floor file, when it lies inside the project.
        Officina,
        /// A new symbolic link, an executable bit added, or a path segment
        /// shaped as an 8.3 short name.
        FileModes,
        /// A blob git reports as binary.
        Binary,
        /// The project's own `sensitive_paths`.
        Project,
        /// One changed path under `review = "all"`.
        File,
    }
}

/// Each pattern group with its patterns: seeded once from FR-11's list as
/// D-246 extends it, and from then on the only copy (AD-113). A pattern
/// without `/` matches a path's last segment; one with `/` any trailing
/// run of segments; `**` spans segments, `*` and `?` stay within one;
/// case-insensitive.
pub const SENSITIVE_PATTERNS: &[(SensitiveGroup, &[&str])] = &[
    (
        SensitiveGroup::ClaudeConfig,
        &[
            ".claude/**",
            "CLAUDE.md",
            "CLAUDE.local.md",
            "AGENTS.md",
            ".mcp.json",
        ],
    ),
    (SensitiveGroup::Git, &[".git*"]),
    (
        SensitiveGroup::Ci,
        &[
            ".github/**",
            ".gitlab-ci.yml",
            ".gitlab/**",
            "azure-pipelines.yml",
            "Jenkinsfile",
        ],
    ),
    (
        SensitiveGroup::BuildScripts,
        &[
            "build.rs",
            "package.json",
            "pnpm-workspace.yaml",
            ".npmrc",
            ".pnpmfile.cjs",
            ".yarnrc.yml",
            "pyproject.toml",
            "setup.py",
            "setup.cfg",
            "Makefile",
            "justfile",
            "Dockerfile*",
        ],
    ),
    (
        SensitiveGroup::ToolchainConfig,
        &[
            ".cargo/config.toml",
            ".cargo/config",
            "Cargo.toml",
            "rust-toolchain.toml",
            "rust-toolchain",
            "conftest.py",
            "tox.ini",
            "noxfile.py",
            "requirements*.txt",
            "ansible.cfg",
            "library/**",
            "filter_plugins/**",
            "callback_plugins/**",
        ],
    ),
    (
        SensitiveGroup::EditorShell,
        &[
            ".vscode/**",
            ".envrc",
            ".pre-commit-config.yaml",
            ".husky/**",
            ".devcontainer/**",
            "*.code-workspace",
            ".idea/**",
        ],
    ),
];

/// The most bytes of hunks a group may show and still be approved in the
/// panel (AD-113, FR-11); a package with a larger group can only be
/// discarded.
pub const MAX_GROUP_HUNK_BYTES: usize = 1024 * 1024;

const SYMLINK_MODE: &str = "120000";
const EXECUTABLE_MODE: &str = "100755";
const GITLINK_MODE: &str = "160000";

/// The anchored, case-insensitive expression of a glob `pattern`.
pub fn pattern_regex(pattern: &str) -> String {
    let spans_segments = pattern.contains('/');
    let chars: Vec<char> = pattern.chars().collect();
    let mut body = String::new();
    let mut at = 0;
    while at < chars.len() {
        match chars[at] {
            '*' if chars.get(at + 1) == Some(&'*') => {
                at += 1;
                if !spans_segments {
                    body.push_str("[^/]*");
                } else if chars.get(at + 1) == Some(&'/') {
                    at += 1;
                    body.push_str("(?:.*/)?");
                } else {
                    body.push_str(".*");
                }
            }
            '*' => body.push_str("[^/]*"),
            '?' => body.push_str("[^/]"),
            other => body.push_str(&regex::escape(&other.to_string())),
        }
        at += 1;
    }
    format!("(?i)^(?:.*/)?{body}$")
}

/// One entry of `git diff --raw -z -M`: its status letter, both modes and
/// object ids, and its path (both sides of a rename or copy). Paths are
/// worker text: matched and displayed, never passed to git (D-219).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedFile {
    pub status: String,
    #[serde(default)]
    pub old_mode: String,
    #[serde(default)]
    pub new_mode: String,
    #[serde(default)]
    pub old_id: String,
    #[serde(default)]
    pub new_id: String,
    pub paths: Vec<String>,
    /// `diff --numstat` shows `-`: git reads the blob as binary.
    #[serde(default)]
    pub binary: bool,
}

/// `id` when it names an object, `None` for git's all-zero absent side.
fn blob_side(id: &str) -> Option<&str> {
    let absent = id.bytes().all(|b| b == b'0');
    (!absent && super::package::object_id(id).is_some()).then_some(id)
}

impl ChangedFile {
    /// The blob id each side of the change is diffed from: `None` for a
    /// side that is absent (an added or deleted file) or not a blob (a
    /// gitlink), which the host replaces by the empty blob or skips.
    pub fn blob_ids(&self) -> Option<(Option<&str>, Option<&str>)> {
        if self.old_mode == GITLINK_MODE || self.new_mode == GITLINK_MODE {
            return None;
        }
        let (old, new) = (blob_side(&self.old_id), blob_side(&self.new_id));
        (old != new).then_some((old, new))
    }

    /// The line each file's hunks start with: what `diff --raw` said of it,
    /// so an approval also binds to the modes, the ids and the paths.
    pub fn hunk_header(&self) -> Vec<u8> {
        let mut line = format!(
            "{} {} {} {} {}",
            self.status, self.old_mode, self.new_mode, self.old_id, self.new_id
        );
        for path in &self.paths {
            line.push('\t');
            line.push_str(path);
        }
        line.push('\n');
        line.into_bytes()
    }
}

/// Every entry of `git diff --raw -z` output, in git's order.
pub fn parse_raw(out: &[u8]) -> Vec<ChangedFile> {
    let mut files = Vec::new();
    let mut fields = out.split(|b| *b == 0).filter(|f| !f.is_empty());
    while let Some(meta) = fields.next() {
        let meta = String::from_utf8_lossy(meta);
        let parts: Vec<&str> = meta.trim_start_matches(':').split(' ').collect();
        let [old_mode, new_mode, old_id, new_id, status] = parts.as_slice() else {
            continue;
        };
        let status: String = status.chars().take(1).collect();
        let count = if matches!(status.as_str(), "R" | "C") {
            2
        } else {
            1
        };
        let paths: Vec<String> = fields
            .by_ref()
            .take(count)
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .collect();
        files.push(ChangedFile {
            status,
            old_mode: (*old_mode).to_owned(),
            new_mode: (*new_mode).to_owned(),
            old_id: (*old_id).to_owned(),
            new_id: (*new_id).to_owned(),
            paths,
            binary: false,
        });
    }
    files
}

/// Marks each of `files` that `git diff --numstat -z -M` output reports
/// as binary (`-` for its counts), matched by its paths.
pub fn mark_binary(files: &mut [ChangedFile], numstat: &[u8]) {
    let mut fields = numstat.split(|b| *b == 0);
    let mut binary: Vec<Vec<String>> = Vec::new();
    while let Some(field) = fields.next() {
        if field.is_empty() {
            continue;
        }
        let text = String::from_utf8_lossy(field);
        let mut parts = text.splitn(3, '\t');
        let (Some(added), Some(_deleted), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let paths: Vec<String> = if path.is_empty() {
            fields
                .by_ref()
                .take(2)
                .map(|p| String::from_utf8_lossy(p).into_owned())
                .collect()
        } else {
            vec![path.to_owned()]
        };
        if added == "-" {
            binary.push(paths);
        }
    }
    for file in files {
        file.binary = binary.contains(&file.paths);
    }
}

/// `files` in the package's order: sorted by their paths, so a `file`
/// group's index names the same file on every computation.
pub fn sort_files(files: &mut [ChangedFile]) {
    files.sort_by(|a, b| a.paths.cmp(&b.paths));
}

/// What a project's approval says about its packages' groups.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SensitiveRules {
    pub review: ReviewScope,
    /// The project's `sensitive_paths`.
    pub project_patterns: Vec<String>,
    /// The floor file's path relative to the project, when it lies inside.
    pub floor_in_project: Option<String>,
}

impl SensitiveRules {
    /// The rules of `project` as `approved` holds them: its review scope
    /// and `sensitive_paths`, and the floor file when it lies inside the
    /// project's folder. A project the snapshot lacks gets the defaults.
    pub fn of_approved(approved: &CanonicalCatalogue, project: &str) -> SensitiveRules {
        let Some(entry) = approved.projects.get(project) else {
            return SensitiveRules::default();
        };
        let workshop = entry.workshop.as_ref();
        SensitiveRules {
            review: workshop.map(|w| w.review).unwrap_or_default(),
            project_patterns: workshop
                .map(|w| w.sensitive_paths.clone())
                .unwrap_or_default(),
            floor_in_project: inside(&approved.floor_path, &entry.path),
        }
    }
}

/// `path` relative to `dir`, both normalised, when it lies inside.
fn inside(path: &str, dir: &str) -> Option<String> {
    let path = crate::quota::normalise_path(path);
    let dir = crate::quota::normalise_path(dir);
    path.strip_prefix(&format!("{dir}/"))
        .filter(|rest| !rest.is_empty())
        .map(str::to_owned)
}

/// One group of a package: its key (the group's code, or `file:<index>`
/// for a `file` group), its family and the indices of its files in the
/// package's sorted list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PackageGroup {
    pub key: String,
    pub group: SensitiveGroup,
    pub files: Vec<u32>,
}

/// The key of the `file` group of the file at `index`.
pub fn file_key(index: usize) -> String {
    format!("{}:{index}", SensitiveGroup::File.code())
}

fn has_alias_segment(path: &str) -> bool {
    path.split('/').any(|segment| {
        segment
            .as_bytes()
            .windows(2)
            .any(|w| w[0] == b'~' && w[1].is_ascii_digit())
    })
}

fn compiled(patterns: &[&str]) -> regex::RegexSet {
    // Every expression is built by `pattern_regex`, whose escaping makes it
    // valid; an empty set matches nothing.
    regex::RegexSet::new(patterns.iter().map(|p| pattern_regex(p)))
        .unwrap_or_else(|_| regex::RegexSet::empty())
}

/// The families `file` falls in under `rules`, besides `file` groups.
pub fn families(file: &ChangedFile, rules: &SensitiveRules) -> Vec<SensitiveGroup> {
    let mut found = Vec::new();
    for (group, patterns) in SENSITIVE_PATTERNS {
        let set = compiled(patterns);
        if file.paths.iter().any(|p| set.is_match(p)) {
            found.push(*group);
        }
    }
    let floor = rules.floor_in_project.as_deref();
    if file
        .paths
        .iter()
        .any(|p| floor.is_some_and(|f| p.eq_ignore_ascii_case(f)))
    {
        found.push(SensitiveGroup::Officina);
    }
    let symlink_added = file.new_mode == SYMLINK_MODE && file.old_mode != SYMLINK_MODE;
    let executable_added = file.new_mode == EXECUTABLE_MODE && file.old_mode != EXECUTABLE_MODE;
    if symlink_added || executable_added || file.paths.iter().any(|p| has_alias_segment(p)) {
        found.push(SensitiveGroup::FileModes);
    }
    if file.old_mode == GITLINK_MODE || file.new_mode == GITLINK_MODE {
        found.push(SensitiveGroup::Git);
    }
    if file.binary {
        found.push(SensitiveGroup::Binary);
    }
    let project: Vec<&str> = rules.project_patterns.iter().map(String::as_str).collect();
    let set = compiled(&project);
    if file.paths.iter().any(|p| set.is_match(p)) {
        found.push(SensitiveGroup::Project);
    }
    found.sort();
    found.dedup();
    found
}

/// The groups of a package whose files are `files`, in the package's
/// sorted order (AD-113): each family with the files it holds, in
/// [`SensitiveGroup`]'s order; with `review = "all"` one `file` group per
/// file instead, since each file then gets its own approval.
pub fn groups(files: &[ChangedFile], rules: &SensitiveRules) -> Vec<PackageGroup> {
    if rules.review == ReviewScope::All {
        return (0..files.len())
            .map(|index| PackageGroup {
                key: file_key(index),
                group: SensitiveGroup::File,
                files: vec![u32::try_from(index).unwrap_or(u32::MAX)],
            })
            .collect();
    }
    let mut by_group: std::collections::BTreeMap<SensitiveGroup, Vec<u32>> =
        std::collections::BTreeMap::new();
    for (index, file) in files.iter().enumerate() {
        for group in families(file, rules) {
            by_group
                .entry(group)
                .or_default()
                .push(u32::try_from(index).unwrap_or(u32::MAX));
        }
    }
    by_group
        .into_iter()
        .map(|(group, files)| PackageGroup {
            key: group.code().to_owned(),
            group,
            files,
        })
        .collect()
}

/// The hex SHA-256 of a group's raw hunks, the value an approval binds to
/// (AD-113): never of their rendering (D-245).
pub fn hunks_sha256(hunks: &[u8]) -> String {
    Sha256::digest(hunks)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pattern_without_slash_matches_the_last_segment() {
        let set = compiled(&["Dockerfile*"]);
        assert!(set.is_match("Dockerfile"));
        assert!(set.is_match("ops/dockerfile.dev"));
        assert!(!set.is_match("Dockerfile.d/x"));
    }

    #[test]
    fn a_double_star_followed_by_a_slash_also_matches_no_segment() {
        let set = compiled(&["deploy/**/hosts"]);
        assert!(set.is_match("deploy/hosts"));
        assert!(set.is_match("x/deploy/a/b/hosts"));
        assert!(!set.is_match("deploy/hostsx"));
    }

    #[test]
    fn a_floor_outside_the_project_is_no_group() {
        assert_eq!(inside("c:/floor/officina.toml", "c:/work/api"), None);
        assert_eq!(
            inside("C:\\Work\\Api\\ops\\officina.toml", "c:/work/api"),
            Some("ops/officina.toml".to_owned())
        );
        assert_eq!(inside("c:/work/api2/officina.toml", "c:/work/api"), None);
    }

    #[test]
    fn a_mode_only_change_has_no_blob_to_diff() {
        let file = ChangedFile {
            status: "M".into(),
            old_mode: "100644".into(),
            new_mode: EXECUTABLE_MODE.into(),
            old_id: "a".repeat(40),
            new_id: "a".repeat(40),
            paths: vec!["run.sh".into()],
            binary: false,
        };
        assert_eq!(file.blob_ids(), None);
        let added = ChangedFile {
            old_mode: "000000".into(),
            old_id: "0".repeat(40),
            new_id: "b".repeat(40),
            ..file
        };
        assert_eq!(
            added.blob_ids(),
            Some((None, Some("b".repeat(40).as_str())))
        );
    }

    #[test]
    fn numstat_marks_binary_files_and_renames() {
        let mut files = parse_raw(
            b":100644 100644 aaa bbb R090\0old.bin\0new.bin\0:100644 100644 aaa bbb M\0x.rs\0",
        );
        mark_binary(&mut files, b"-\t-\t\0old.bin\0new.bin\x003\t1\tx.rs\0");
        assert!(files[0].binary);
        assert!(!files[1].binary);
        assert_eq!(files[0].paths, ["old.bin", "new.bin"]);
        assert_eq!(files[0].status, "R");
    }
}
