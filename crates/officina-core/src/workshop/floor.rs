//! The workshop's floor keys (FR-2 as amended, AD-101): the `[workshop]`,
//! `[project.workshop]` and `[template.workshop]` tables, their phase 2
//! rules, the files phase 2 reads through the host ([`CatalogueFiles`]), and
//! their canonical forms. Each rule's refusal is a [`Problem`] whose
//! [`IssueKind`] has a failing fixture in `relay::catalogue`'s tests (D-132);
//! `relay::catalogue::validate` turns them into issues with their table and
//! line.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use super::image::{PROJECT_BASES, final_base, stages};
use crate::floor::{FloorFile, floor_dir_of, is_absolute, join_project_path};
use crate::relay::catalogue::{IssueKind, is_valid_action_name};

/// Workers at once, at most (FR-2).
pub const MAX_WORKERS_RANGE: RangeInclusive<i64> = 1..=4;
/// `max_workers` when the table does not set it.
pub const DEFAULT_MAX_WORKERS: u32 = 2;
/// CPUs per worker when the table does not set it.
pub const DEFAULT_CPUS: u32 = 2;
/// Memory per worker, in MiB, when the table does not set it.
pub const DEFAULT_MEMORY_MIB: u32 = 4096;
/// The largest CA file read (FR-2).
pub const MAX_CA_BYTES: usize = 64 * 1024;
/// The largest project Dockerfile read (AD-101).
pub const MAX_DOCKERFILE_BYTES: usize = 64 * 1024;
/// Regular files in a Dockerfile's folder, the build context, at most.
pub const MAX_CONTEXT_FILES: usize = 32;
/// Bytes in a Dockerfile's folder, at most.
pub const MAX_CONTEXT_BYTES: usize = 1024 * 1024;
/// Read-only mounts per project, at most (FR-2).
pub const MAX_MOUNTS: usize = 8;
/// Allowed hosts per project, at most (FR-2).
pub const MAX_HOSTS: usize = 32;
/// `sensitive_paths` entries, at most (FR-11).
pub const MAX_SENSITIVE_PATHS: usize = 32;
/// The longest `sensitive_paths` pattern.
pub const MAX_PATTERN_CHARS: usize = 256;
/// Base branches a template offers, at most (FR-2).
pub const MAX_BASES: usize = 8;
/// The longest branch name accepted in `base`.
pub const MAX_BRANCH_CHARS: usize = 200;
/// The longest `memory_name`.
pub const MAX_MEMORY_NAME_CHARS: usize = 255;
/// Accepted `max_minutes` (FR-2).
pub const MAX_MINUTES_RANGE: RangeInclusive<i64> = 5..=240;
/// `max_minutes` when the template does not set it (FR-2).
pub const DEFAULT_MAX_MINUTES: u32 = 60;

closed_enum! {
    #[serde(rename_all = "kebab-case")]
    /// How a worker authenticates (FR-5 as D-603 amends it): a
    /// `setup-token` token through `CLAUDE_CODE_OAUTH_TOKEN` (the default,
    /// recommended for unattended workers), a `/login` made once inside the
    /// workshop, or an API key.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum CredentialKind {
        #[default]
        OauthToken,
        Login,
        ApiKey,
    }
}

closed_enum! {
    #[serde(rename_all = "lowercase")]
    /// What the review panel groups (FR-11): sensitive groups, or every
    /// changed path as its own group.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ReviewScope {
        #[default]
        Groups,
        All,
    }
}

/// `upstream_proxy` (D-611): Windows' own proxy (the default), a direct
/// connection, or an explicit `host:port`. The canonical form holds this
/// setting, never what it resolves to on a given network.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub enum UpstreamProxy {
    #[default]
    System,
    Direct,
    Explicit(String),
}

/// The words `upstream_proxy` accepts beside `host:port`.
pub const UPSTREAM_SYSTEM: &str = "system";
pub const UPSTREAM_DIRECT: &str = "none";

impl UpstreamProxy {
    /// The setting as the floor writes it.
    pub fn setting(&self) -> &str {
        match self {
            UpstreamProxy::System => UPSTREAM_SYSTEM,
            UpstreamProxy::Direct => UPSTREAM_DIRECT,
            UpstreamProxy::Explicit(target) => target,
        }
    }

    /// The setting a floor's text names, or why it names none.
    ///
    /// ```
    /// use officina_core::workshop::floor::UpstreamProxy;
    /// assert_eq!(UpstreamProxy::parse("system"), Ok(UpstreamProxy::System));
    /// assert!(UpstreamProxy::parse("proxy.example.com:8080").is_ok());
    /// assert!(UpstreamProxy::parse("http://proxy.example.com:8080").is_err());
    /// ```
    pub fn parse(text: &str) -> Result<UpstreamProxy, String> {
        match text {
            UPSTREAM_SYSTEM => return Ok(UpstreamProxy::System),
            UPSTREAM_DIRECT => return Ok(UpstreamProxy::Direct),
            _ => {}
        }
        if text.contains("://") {
            return Err("no scheme: write host:port".to_owned());
        }
        if text.contains('@') {
            return Err("no credentials: an authenticating proxy is outside v0.6".to_owned());
        }
        let Some((host, port)) = text.rsplit_once(':') else {
            return Err("host:port, \"system\" or \"none\"".to_owned());
        };
        if !port.parse::<u16>().is_ok_and(|p| p > 0) {
            return Err(format!("the port {port:?} is not 1 to 65535"));
        }
        if !(is_ipv4(host) || is_dns_name(host)) {
            return Err(format!(
                "{host:?} is neither a DNS name nor an IPv4 address"
            ));
        }
        Ok(UpstreamProxy::Explicit(text.to_owned()))
    }
}

impl Serialize for UpstreamProxy {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.setting())
    }
}

impl<'de> Deserialize<'de> for UpstreamProxy {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        UpstreamProxy::parse(&text).map_err(serde::de::Error::custom)
    }
}

/// A refused value of a workshop table, before it gets its table and line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub kind: IssueKind,
    pub message: String,
}

fn problem(kind: IssueKind, message: impl Into<String>) -> Problem {
    Problem {
        kind,
        message: message.into(),
    }
}

/// A file the host read for phase 2, bounded at its limit plus one byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileContent {
    Bytes(Vec<u8>),
    /// Over its limit: nothing was kept.
    TooLarge,
    /// The read failed, with the reason.
    Unreadable(String),
}

/// One regular file of a Dockerfile's folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Why a Dockerfile's folder cannot be a build context (AD-101).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextProblem {
    TooManyFiles,
    TooLarge,
    Subfolder(String),
    Symlink(String),
    Unreadable(String),
}

/// A project's Dockerfile and its folder, as the host read them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerfileFiles {
    pub dockerfile: FileContent,
    pub context: Result<Vec<ContextFile>, ContextProblem>,
}

/// What the host read for phase 2 (AD-101): the CA file and each workshop
/// project's Dockerfile and folder. `read` is false for `officina run` and
/// the gate, which never read them (D-244): the file rules are skipped and
/// only the relay-section hash of such a load means anything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogueFiles {
    pub read: bool,
    pub ca_file: Option<FileContent>,
    /// By project name.
    pub dockerfiles: BTreeMap<String, DockerfileFiles>,
}

impl CatalogueFiles {
    /// The files of a load that reads none: `officina run`'s.
    pub fn not_read() -> CatalogueFiles {
        CatalogueFiles::default()
    }
}

/// One Dockerfile the host is asked to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerfileRequest {
    pub project: String,
    /// The project folder joined with the Dockerfile's relative path, in
    /// the floor's spelling.
    pub path: String,
}

/// The files phase 2 needs from a floor: decided on the raw tables, so the
/// host reads only what a syntactically valid key names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileRequests {
    pub ca_file: Option<String>,
    pub dockerfiles: Vec<DockerfileRequest>,
}

/// The files `floor`, read from `floor_path`, asks phase 2 to read.
pub fn file_requests(floor: &FloorFile, floor_path: &str) -> FileRequests {
    let ca_file = floor
        .workshop
        .as_ref()
        .and_then(|table| table.get_ref().get("ca_file"))
        .and_then(toml::Value::as_str)
        .filter(|path| is_absolute(path))
        .map(str::to_owned);
    let floor_dir = floor_dir_of(floor_path);
    let dockerfiles = floor
        .projects
        .iter()
        .filter_map(|project| {
            let relative = project
                .workshop
                .as_ref()?
                .get_ref()
                .get("dockerfile")?
                .as_str()?;
            relative_path_problem(relative)
                .is_none()
                .then(|| DockerfileRequest {
                    project: project.name.clone(),
                    path: format!(
                        "{}/{}",
                        join_project_path(floor_dir, &project.path).trim_end_matches(['\\', '/']),
                        relative.replace('\\', "/")
                    ),
                })
        })
        .collect();
    FileRequests {
        ca_file,
        dockerfiles,
    }
}

/// A validated `[workshop]` table, every default applied (AD-101).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkshopTable {
    pub auth: CredentialKind,
    pub max_workers: u32,
    pub cpus: u32,
    pub memory_mib: u32,
    pub upstream_proxy: UpstreamProxy,
    pub ca_file: Option<String>,
    /// The CA file's SHA-256, when it was read.
    pub ca_sha256: Option<String>,
}

/// A validated `[project.workshop]` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectWorkshop {
    /// Sorted, `/`-separated.
    pub mounts: Vec<String>,
    /// Lowercase, sorted, deduplicated.
    pub hosts: Vec<String>,
    pub dockerfile: Option<DockerfileFacts>,
    pub review: ReviewScope,
    /// Sorted, deduplicated.
    pub sensitive_paths: Vec<String>,
    pub memory_name: Option<String>,
}

/// A project's Dockerfile: its path and, when the host read it, the
/// SHA-256 of the file and of its folder (over the sorted file names,
/// lengths and bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerfileFacts {
    pub path: String,
    pub sha256: Option<String>,
    pub context_sha256: Option<String>,
}

/// A validated `[template.workshop]` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateWorkshop {
    /// In the floor's order; the first is the default (FR-2).
    pub base: Vec<String>,
    pub max_minutes: u32,
    pub max_usd_cents: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkshop {
    #[serde(default)]
    auth: Option<String>,
    #[serde(default)]
    max_workers: Option<i64>,
    #[serde(default)]
    cpus: Option<i64>,
    #[serde(default)]
    memory_mib: Option<i64>,
    #[serde(default)]
    upstream_proxy: Option<String>,
    #[serde(default)]
    ca_file: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProjectWorkshop {
    #[serde(default)]
    mounts: Vec<String>,
    #[serde(default)]
    hosts: Vec<String>,
    #[serde(default)]
    dockerfile: Option<String>,
    #[serde(default)]
    review: Option<String>,
    #[serde(default)]
    sensitive_paths: Vec<String>,
    #[serde(default)]
    memory_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTemplateWorkshop {
    base: Vec<String>,
    #[serde(default)]
    max_minutes: Option<i64>,
    #[serde(default)]
    max_usd: Option<toml::Value>,
}

fn parse<T: serde::de::DeserializeOwned>(value: &toml::Value) -> Result<T, Vec<Problem>> {
    value.clone().try_into().map_err(|e: toml::de::Error| {
        vec![problem(IssueKind::Malformed, e.message().trim().to_owned())]
    })
}

/// A positive count within `u32`, or the problem naming `key`.
fn positive(key: &str, value: i64, kind: IssueKind) -> Result<u32, Problem> {
    u32::try_from(value)
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| problem(kind, format!("`{key}` is a positive whole number")))
}

/// Phase 2 of `[workshop]` (FR-2, D-603, D-611).
pub fn workshop_table(
    value: &toml::Value,
    files: &CatalogueFiles,
) -> Result<WorkshopTable, Vec<Problem>> {
    let raw: RawWorkshop = parse(value)?;
    let mut problems = Vec::new();
    let auth = match raw.auth.as_deref() {
        None => CredentialKind::default(),
        Some(text) => CredentialKind::from_code(text).unwrap_or_else(|| {
            let kinds: Vec<&str> = CredentialKind::ALL.iter().map(|k| k.code()).collect();
            problems.push(problem(
                IssueKind::WorkshopAuth,
                format!("`auth` is one of {}, not {text:?}", kinds.join(", ")),
            ));
            CredentialKind::default()
        }),
    };
    let max_workers = match raw.max_workers {
        None => DEFAULT_MAX_WORKERS,
        Some(n) if MAX_WORKERS_RANGE.contains(&n) => {
            u32::try_from(n).unwrap_or(DEFAULT_MAX_WORKERS)
        }
        Some(_) => {
            problems.push(problem(
                IssueKind::WorkshopMaxWorkers,
                format!(
                    "`max_workers` is {} to {}",
                    MAX_WORKERS_RANGE.start(),
                    MAX_WORKERS_RANGE.end()
                ),
            ));
            DEFAULT_MAX_WORKERS
        }
    };
    let mut limit = |key: &str, value: Option<i64>, default: u32| match value {
        None => default,
        Some(n) => positive(key, n, IssueKind::WorkshopLimits).unwrap_or_else(|p| {
            problems.push(p);
            default
        }),
    };
    let cpus = limit("cpus", raw.cpus, DEFAULT_CPUS);
    let memory_mib = limit("memory_mib", raw.memory_mib, DEFAULT_MEMORY_MIB);
    let upstream_proxy = match raw.upstream_proxy.as_deref() {
        None => UpstreamProxy::default(),
        Some(text) => UpstreamProxy::parse(text).unwrap_or_else(|why| {
            problems.push(problem(
                IssueKind::UpstreamProxy,
                format!("`upstream_proxy` {text:?}: {why}"),
            ));
            UpstreamProxy::default()
        }),
    };
    let mut ca_sha256 = None;
    if let Some(path) = &raw.ca_file {
        match ca_file_problem(path, files) {
            Ok(digest) => ca_sha256 = digest,
            Err(why) => problems.push(problem(IssueKind::CaFile, format!("`ca_file`: {why}"))),
        }
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    Ok(WorkshopTable {
        auth,
        max_workers,
        cpus,
        memory_mib,
        upstream_proxy,
        ca_file: raw.ca_file,
        ca_sha256,
    })
}

/// The CA file's SHA-256 (`None` when the files were not read), or why the
/// file is refused: an absolute path to PEM `CERTIFICATE` blocks with
/// valid base64 and nothing else, at most [`MAX_CA_BYTES`].
fn ca_file_problem(path: &str, files: &CatalogueFiles) -> Result<Option<String>, String> {
    if !is_absolute(path) {
        return Err(format!("{path:?} is not an absolute path"));
    }
    if !files.read {
        return Ok(None);
    }
    match &files.ca_file {
        None => Err("the file was not read".to_owned()),
        Some(FileContent::TooLarge) => Err(format!("the file is over {MAX_CA_BYTES} bytes")),
        Some(FileContent::Unreadable(why)) => Err(format!("the file cannot be read ({why})")),
        Some(FileContent::Bytes(bytes)) => {
            let text = std::str::from_utf8(bytes).map_err(|_| "the file is not text".to_owned())?;
            pem_certificates_only(text)?;
            Ok(Some(crate::sha256_hex(bytes)))
        }
    }
}

const PEM_BEGIN: &str = "-----BEGIN CERTIFICATE-----";
const PEM_END: &str = "-----END CERTIFICATE-----";

/// `Ok` when `text` holds one or more PEM `CERTIFICATE` blocks with valid
/// base64 bodies and only whitespace around them.
fn pem_certificates_only(text: &str) -> Result<(), String> {
    let mut blocks = 0;
    let mut body: Option<String> = None;
    for line in text.lines().map(str::trim) {
        match (&mut body, line) {
            (None, "") => {}
            (None, PEM_BEGIN) => body = Some(String::new()),
            (None, _) => return Err("it holds text outside CERTIFICATE blocks".to_owned()),
            (Some(collected), PEM_END) => {
                if !is_base64(collected) {
                    return Err("a CERTIFICATE block's body is not valid base64".to_owned());
                }
                blocks += 1;
                body = None;
            }
            (Some(collected), line) => collected.push_str(line),
        }
    }
    if body.is_some() {
        return Err("a CERTIFICATE block is not closed".to_owned());
    }
    if blocks == 0 {
        return Err("it holds no CERTIFICATE block".to_owned());
    }
    Ok(())
}

fn is_base64(text: &str) -> bool {
    let data = text.trim_end_matches('=');
    let padding = text.len() - data.len();
    !text.is_empty()
        && text.len().is_multiple_of(4)
        && padding <= 2
        && data
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/')
}

/// Phase 2 of a project's `[project.workshop]` (FR-2, D-231, D-236, D-248).
pub fn project_workshop(
    project: &str,
    value: &toml::Value,
    files: &CatalogueFiles,
) -> Result<ProjectWorkshop, Vec<Problem>> {
    let raw: RawProjectWorkshop = parse(value)?;
    let mut problems = Vec::new();
    if !is_valid_action_name(project) || project.ends_with('-') {
        problems.push(problem(
            IssueKind::WorkshopProjectName,
            format!(
                "a workshop project's name follows the action name rule and does not end in `-`, since it names /work/{project} and an image"
            ),
        ));
    }
    let mounts = check_mounts(&raw.mounts, &mut problems);
    let hosts = check_hosts(&raw.hosts, &mut problems);
    let dockerfile = raw
        .dockerfile
        .as_deref()
        .and_then(|path| check_dockerfile(project, path, files, &mut problems));
    let review = match raw.review.as_deref() {
        None => ReviewScope::default(),
        Some(text) => ReviewScope::from_code(text).unwrap_or_else(|| {
            problems.push(problem(
                IssueKind::WorkshopReview,
                format!("`review` is `groups` or `all`, not {text:?}"),
            ));
            ReviewScope::default()
        }),
    };
    let sensitive_paths = check_sensitive_paths(&raw.sensitive_paths, &mut problems);
    if let Some(name) = &raw.memory_name
        && let Some(why) = memory_name_problem(name)
    {
        problems.push(problem(
            IssueKind::MemoryName,
            format!("`memory_name`: {why}"),
        ));
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    Ok(ProjectWorkshop {
        mounts,
        hosts,
        dockerfile,
        review,
        sensitive_paths,
        memory_name: raw.memory_name,
    })
}

/// Why a relative path inside a project is refused, if it is: absolute,
/// empty, or holding an empty, `.` or `..` segment.
fn relative_path_problem(path: &str) -> Option<String> {
    if path.is_empty() {
        return Some("the path is empty".to_owned());
    }
    if is_absolute(path) || path.starts_with('\\') || path.contains(':') {
        return Some(format!("{path:?} is not relative to the project folder"));
    }
    let bad = path
        .split(['/', '\\'])
        .find(|segment| segment.is_empty() || *segment == "." || *segment == "..");
    bad.map(|segment| format!("{path:?} holds the segment {segment:?}"))
}

/// The mounts, `/`-separated and sorted, each refused one said (FR-2,
/// D-231): relative, no `.` or `..` segment, at most [`MAX_MOUNTS`], no
/// duplicate ignoring case, not `.git` or under it.
fn check_mounts(raw: &[String], problems: &mut Vec<Problem>) -> Vec<String> {
    if raw.len() > MAX_MOUNTS {
        problems.push(problem(
            IssueKind::WorkshopMounts,
            format!("`mounts` lists at most {MAX_MOUNTS} folders"),
        ));
    }
    let mut seen = BTreeSet::new();
    let mut mounts = Vec::new();
    for mount in raw {
        if let Some(why) = relative_path_problem(mount) {
            problems.push(problem(IssueKind::WorkshopMounts, format!("mount {why}")));
            continue;
        }
        let normal = mount.replace('\\', "/");
        let folded = normal.to_lowercase();
        if folded == ".git" || folded.starts_with(".git/") {
            problems.push(problem(
                IssueKind::WorkshopMounts,
                format!("mount {mount:?} is the repository's .git folder or under it"),
            ));
            continue;
        }
        if !seen.insert(folded) {
            problems.push(problem(
                IssueKind::WorkshopMounts,
                format!("mount {mount:?} is listed twice, ignoring case"),
            ));
            continue;
        }
        mounts.push(normal);
    }
    mounts.sort();
    mounts
}

/// A lowercase DNS name: labels of letters, digits and `-`, not starting or
/// ending with `-`, at most 63 characters each and 253 in all, the last
/// holding a letter so no numeric spelling of an address passes.
pub fn is_dns_name(name: &str) -> bool {
    let labels: Vec<&str> = name.split('.').collect();
    name.len() <= 253
        && labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
        && labels
            .last()
            .is_some_and(|last| last.bytes().any(|b| b.is_ascii_lowercase()))
}

fn is_ipv4(text: &str) -> bool {
    text.parse::<std::net::Ipv4Addr>().is_ok()
}

/// The hosts, sorted and deduplicated (FR-2, FR-4): lowercase DNS names, at
/// most [`MAX_HOSTS`], a leading `*.` only on a name of three labels or
/// more.
fn check_hosts(raw: &[String], problems: &mut Vec<Problem>) -> Vec<String> {
    if raw.len() > MAX_HOSTS {
        problems.push(problem(
            IssueKind::WorkshopHosts,
            format!("`hosts` lists at most {MAX_HOSTS} names"),
        ));
    }
    let mut hosts = BTreeSet::new();
    for host in raw {
        let (wildcard, name) = match host.strip_prefix("*.") {
            Some(rest) => (true, rest),
            None => (false, host.as_str()),
        };
        let why = if !is_dns_name(name) {
            Some(
                "is not a lowercase DNS name whose last label holds a letter (no IP literal, no port)",
            )
        } else if wildcard && name.split('.').count() < 2 {
            Some("takes a leading `*.` only on a name of three labels or more")
        } else {
            None
        };
        match why {
            Some(why) => problems.push(problem(
                IssueKind::WorkshopHosts,
                format!("host {host:?} {why}"),
            )),
            None => {
                hosts.insert(host.clone());
            }
        }
    }
    hosts.into_iter().collect()
}

/// A project's Dockerfile (AD-99, AD-101): inside the project, at most
/// [`MAX_DOCKERFILE_BYTES`], its folder a bounded build context, its final
/// stage descending from an Officina base. The file rules apply only when
/// the host read the files.
fn check_dockerfile(
    project: &str,
    path: &str,
    files: &CatalogueFiles,
    problems: &mut Vec<Problem>,
) -> Option<DockerfileFacts> {
    if let Some(why) = relative_path_problem(path) {
        problems.push(problem(
            IssueKind::DockerfilePath,
            format!("`dockerfile` {why}"),
        ));
        return None;
    }
    let path = path.replace('\\', "/");
    if !files.read {
        return Some(DockerfileFacts {
            path,
            sha256: None,
            context_sha256: None,
        });
    }
    let Some(read) = files.dockerfiles.get(project) else {
        problems.push(problem(
            IssueKind::DockerfileFile,
            "the Dockerfile was not read",
        ));
        return None;
    };
    let bytes = match &read.dockerfile {
        FileContent::Bytes(bytes) => bytes,
        FileContent::TooLarge => {
            problems.push(problem(
                IssueKind::DockerfileFile,
                format!("the Dockerfile is over {MAX_DOCKERFILE_BYTES} bytes"),
            ));
            return None;
        }
        FileContent::Unreadable(why) => {
            problems.push(problem(
                IssueKind::DockerfileFile,
                format!("the Dockerfile cannot be read ({why})"),
            ));
            return None;
        }
    };
    let context = match &read.context {
        Ok(context) => context,
        Err(why) => {
            let message = match why {
                ContextProblem::TooManyFiles => {
                    format!("its folder holds more than {MAX_CONTEXT_FILES} files")
                }
                ContextProblem::TooLarge => {
                    format!("its folder holds more than {MAX_CONTEXT_BYTES} bytes")
                }
                ContextProblem::Subfolder(name) => {
                    format!("its folder holds the subfolder {name:?}")
                }
                ContextProblem::Symlink(name) => format!("its folder holds the link {name:?}"),
                ContextProblem::Unreadable(why) => format!("its folder cannot be read ({why})"),
            };
            problems.push(problem(
                IssueKind::DockerfileContext,
                format!("the Dockerfile's folder is its build context: {message}"),
            ));
            return None;
        }
    };
    let text = String::from_utf8_lossy(bytes);
    let descends = stages(&text).ok().and_then(|found| final_base(&found));
    if descends.is_none() {
        problems.push(problem(
            IssueKind::DockerfileBase,
            format!(
                "the Dockerfile's final stage descends, through stage aliases, from {}",
                PROJECT_BASES.join(" or ")
            ),
        ));
        return None;
    }
    Some(DockerfileFacts {
        path,
        sha256: Some(crate::sha256_hex(bytes)),
        context_sha256: Some(context_sha256(context)),
    })
}

/// SHA-256 over the context's files sorted by name: each name, its length
/// and its bytes, so a renamed, resized or edited file moves it.
pub fn context_sha256(files: &[ContextFile]) -> String {
    let mut sorted: Vec<&ContextFile> = files.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    let mut buffer = Vec::new();
    for file in sorted {
        buffer.extend_from_slice(file.name.as_bytes());
        buffer.push(0);
        buffer.extend_from_slice(&(file.bytes.len() as u64).to_be_bytes());
        buffer.extend_from_slice(&file.bytes);
    }
    crate::sha256_hex(&buffer)
}

fn check_sensitive_paths(raw: &[String], problems: &mut Vec<Problem>) -> Vec<String> {
    if raw.len() > MAX_SENSITIVE_PATHS {
        problems.push(problem(
            IssueKind::SensitivePaths,
            format!("`sensitive_paths` lists at most {MAX_SENSITIVE_PATHS} patterns"),
        ));
    }
    let mut kept = BTreeSet::new();
    for pattern in raw {
        let bad = pattern.is_empty()
            || pattern.chars().count() > MAX_PATTERN_CHARS
            || pattern.starts_with('/')
            || pattern.contains('\\')
            || pattern.chars().any(char::is_control);
        if bad {
            problems.push(problem(
                IssueKind::SensitivePaths,
                format!(
                    "pattern {pattern:?} is a relative glob with `/` separators, at most {MAX_PATTERN_CHARS} characters, no control character"
                ),
            ));
        } else {
            kept.insert(pattern.clone());
        }
    }
    kept.into_iter().collect()
}

/// Why `memory_name` is refused, if it is (D-236): one path segment, not
/// `.` or `..`, no `\`, `/` or `:`.
fn memory_name_problem(name: &str) -> Option<&'static str> {
    if name.is_empty() || name.chars().count() > MAX_MEMORY_NAME_CHARS {
        Some("one folder name, not empty and not too long")
    } else if name == "." || name == ".." {
        Some("`.` and `..` name no folder of its own")
    } else if name.contains(['\\', '/', ':']) || name.chars().any(char::is_control) {
        Some("one folder name, without `\\`, `/`, `:` or a control character")
    } else {
        None
    }
}

/// Phase 2 of a template's `[template.workshop]` (FR-2, D-203).
pub fn template_workshop(value: &toml::Value) -> Result<TemplateWorkshop, Vec<Problem>> {
    let raw: RawTemplateWorkshop = parse(value)?;
    let mut problems = Vec::new();
    if raw.base.is_empty() || raw.base.len() > MAX_BASES {
        problems.push(problem(
            IssueKind::WorkshopBase,
            format!("`base` lists 1 to {MAX_BASES} branches; the first is the default"),
        ));
    }
    let mut seen = BTreeSet::new();
    for branch in &raw.base {
        if !is_valid_branch_name(branch) {
            problems.push(problem(
                IssueKind::WorkshopBase,
                format!("{branch:?} is not a branch name git accepts"),
            ));
        } else if !seen.insert(branch.as_str()) {
            problems.push(problem(
                IssueKind::WorkshopBase,
                format!("{branch:?} is listed twice"),
            ));
        }
    }
    let max_minutes = match raw.max_minutes {
        None => DEFAULT_MAX_MINUTES,
        Some(n) if MAX_MINUTES_RANGE.contains(&n) => {
            u32::try_from(n).unwrap_or(DEFAULT_MAX_MINUTES)
        }
        Some(_) => {
            problems.push(problem(
                IssueKind::WorkshopMaxMinutes,
                format!(
                    "`max_minutes` is {} to {}",
                    MAX_MINUTES_RANGE.start(),
                    MAX_MINUTES_RANGE.end()
                ),
            ));
            DEFAULT_MAX_MINUTES
        }
    };
    let max_usd_cents = raw.max_usd.as_ref().and_then(|value| {
        usd_cents(value)
            .map_err(|why| problems.push(problem(IssueKind::WorkshopMaxUsd, why)))
            .ok()
    });
    if !problems.is_empty() {
        return Err(problems);
    }
    Ok(TemplateWorkshop {
        base: raw.base,
        max_minutes,
        max_usd_cents,
    })
}

/// `max_usd` in cents: a positive amount with at most two decimals.
fn usd_cents(value: &toml::Value) -> Result<u32, String> {
    let refused = || "`max_usd` is a positive amount with at most two decimals".to_owned();
    let cents = match value {
        toml::Value::Integer(n) => n.checked_mul(100).ok_or_else(refused)?,
        toml::Value::Float(f) => {
            let scaled = f * 100.0;
            let rounded = scaled.round();
            if !scaled.is_finite() || (scaled - rounded).abs() > 1e-6 {
                return Err(refused());
            }
            // A finite whole number; `u32::try_from` below bounds it.
            rounded as i64
        }
        _ => return Err(refused()),
    };
    u32::try_from(cents)
        .ok()
        .filter(|c| *c > 0)
        .ok_or_else(refused)
}

/// A branch name git's `check-ref-format --branch` accepts, checked in core
/// (FR-2): no control character, space or any of `~^:?*[\`; no `..` or
/// `@{`; no segment that starts with `.` or ends with `.lock`; not starting
/// with `-` or `/`, not ending with `/` or `.`, no empty segment; not `@`.
///
/// ```
/// use officina_core::workshop::floor::is_valid_branch_name;
/// assert!(is_valid_branch_name("main"));
/// assert!(is_valid_branch_name("release/2.1"));
/// assert!(!is_valid_branch_name("feat..x"));
/// ```
pub fn is_valid_branch_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= MAX_BRANCH_CHARS
        && name != "@"
        && !name.starts_with('-')
        && !name.starts_with('/')
        && !name.ends_with('/')
        && !name.ends_with('.')
        && !name.contains("..")
        && !name.contains("@{")
        && !name
            .chars()
            .any(|c| c.is_control() || c == ' ' || "~^:?*[\\".contains(c))
        && name.split('/').all(|segment| {
            !segment.is_empty() && !segment.starts_with('.') && !segment.ends_with(".lock")
        })
}

/// The canonical form of `[workshop]` (AD-101): the setting of
/// `upstream_proxy`, never a route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CanonicalWorkshop {
    pub auth: CredentialKind,
    pub max_workers: u32,
    pub cpus: u32,
    pub memory_mib: u32,
    pub upstream_proxy: String,
    pub ca_file: Option<String>,
    pub ca_sha256: Option<String>,
}

impl From<&WorkshopTable> for CanonicalWorkshop {
    fn from(table: &WorkshopTable) -> CanonicalWorkshop {
        CanonicalWorkshop {
            auth: table.auth,
            max_workers: table.max_workers,
            cpus: table.cpus,
            memory_mib: table.memory_mib,
            upstream_proxy: table.upstream_proxy.setting().to_owned(),
            ca_file: table.ca_file.clone(),
            ca_sha256: table.ca_sha256.clone(),
        }
    }
}

/// The canonical form of a project's `[project.workshop]` (AD-101).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CanonicalProjectWorkshop {
    pub mounts: Vec<String>,
    pub hosts: Vec<String>,
    pub dockerfile: Option<String>,
    pub dockerfile_sha256: Option<String>,
    pub context_sha256: Option<String>,
    pub review: ReviewScope,
    pub sensitive_paths: Vec<String>,
    pub memory_name: Option<String>,
}

impl From<&ProjectWorkshop> for CanonicalProjectWorkshop {
    fn from(project: &ProjectWorkshop) -> CanonicalProjectWorkshop {
        let dockerfile = project.dockerfile.as_ref();
        CanonicalProjectWorkshop {
            mounts: project.mounts.clone(),
            hosts: project.hosts.clone(),
            dockerfile: dockerfile.map(|d| d.path.clone()),
            dockerfile_sha256: dockerfile.and_then(|d| d.sha256.clone()),
            context_sha256: dockerfile.and_then(|d| d.context_sha256.clone()),
            review: project.review,
            sensitive_paths: project.sensitive_paths.clone(),
            memory_name: project.memory_name.clone(),
        }
    }
}

/// The canonical form of a template's `[template.workshop]` (AD-101): the
/// base list in order, its first entry the default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct CanonicalTemplateWorkshop {
    pub base: Vec<String>,
    pub max_minutes: u32,
    pub max_usd_cents: Option<u32>,
}

impl From<&TemplateWorkshop> for CanonicalTemplateWorkshop {
    fn from(template: &TemplateWorkshop) -> CanonicalTemplateWorkshop {
        CanonicalTemplateWorkshop {
            base: template.base.clone(),
            max_minutes: template.max_minutes,
            max_usd_cents: template.max_usd_cents,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(text: &str) -> toml::Value {
        toml::from_str::<toml::Value>(text).expect("TOML")
    }

    fn kinds(problems: &[Problem]) -> Vec<IssueKind> {
        problems.iter().map(|p| p.kind).collect()
    }

    const PEM: &str = "-----BEGIN CERTIFICATE-----\nTUlJQmV4YW1wbGU=\n-----END CERTIFICATE-----\n";

    fn with_ca(content: FileContent) -> CatalogueFiles {
        CatalogueFiles {
            read: true,
            ca_file: Some(content),
            dockerfiles: BTreeMap::new(),
        }
    }

    #[test]
    fn defaults_are_explicit() {
        let workshop = workshop_table(&table(""), &CatalogueFiles::not_read()).unwrap();
        assert_eq!(workshop.max_workers, DEFAULT_MAX_WORKERS);
        assert_eq!(workshop.cpus, DEFAULT_CPUS);
        assert_eq!(workshop.memory_mib, DEFAULT_MEMORY_MIB);
        assert_eq!(workshop.ca_file, None);
        let canonical = serde_json::to_value(CanonicalWorkshop::from(&workshop)).unwrap();
        for key in [
            "auth",
            "max_workers",
            "cpus",
            "memory_mib",
            "upstream_proxy",
        ] {
            assert!(!canonical[key].is_null(), "{key} is written explicitly");
        }
        let project = project_workshop("api", &table(""), &CatalogueFiles::not_read()).unwrap();
        assert_eq!(project.review, ReviewScope::Groups);
        assert!(project.mounts.is_empty() && project.hosts.is_empty());
        let template = template_workshop(&table("base = [\"main\"]")).unwrap();
        assert_eq!(template.max_minutes, DEFAULT_MAX_MINUTES);
        assert_eq!(template.max_usd_cents, None);
    }

    #[test]
    fn auth_defaults_to_oauth_token_and_upstream_proxy_to_system() {
        let workshop = workshop_table(&table(""), &CatalogueFiles::not_read()).unwrap();
        assert_eq!(workshop.auth, CredentialKind::OauthToken);
        assert_eq!(workshop.auth.code(), "oauth-token");
        assert_eq!(workshop.upstream_proxy, UpstreamProxy::System);
        assert_eq!(workshop.upstream_proxy.setting(), UPSTREAM_SYSTEM);
    }

    #[test]
    fn every_credential_kind_parses() {
        for kind in CredentialKind::ALL {
            let text = format!("auth = \"{}\"", kind.code());
            let workshop = workshop_table(&table(&text), &CatalogueFiles::not_read()).unwrap();
            assert_eq!(workshop.auth, *kind);
            let json = serde_json::to_value(kind).unwrap();
            assert_eq!(json, serde_json::json!(kind.code()));
        }
        let wrong = workshop_table(&table("auth = \"password\""), &CatalogueFiles::not_read());
        assert_eq!(kinds(&wrong.unwrap_err()), [IssueKind::WorkshopAuth]);
    }

    #[test]
    fn an_upstream_proxy_with_a_scheme_or_credentials_is_an_issue() {
        for good in [
            "system",
            "none",
            "proxy.example.com:8080",
            "192.0.2.10:3128",
        ] {
            let text = format!("upstream_proxy = \"{good}\"");
            let workshop = workshop_table(&table(&text), &CatalogueFiles::not_read()).unwrap();
            assert_eq!(workshop.upstream_proxy.setting(), good);
        }
        for bad in [
            "http://proxy.example.com:8080",
            "user:secret@proxy.example.com:8080",
            "proxy.example.com",
            "proxy.example.com:0",
            "proxy.example.com:99999",
            "Proxy.Example.com:8080",
            "[2001:db8::1]:8080",
            "",
        ] {
            let text = format!("upstream_proxy = \"{bad}\"");
            let found = workshop_table(&table(&text), &CatalogueFiles::not_read());
            assert_eq!(
                kinds(&found.unwrap_err()),
                [IssueKind::UpstreamProxy],
                "{bad}"
            );
        }
    }

    #[test]
    fn the_ca_file_holds_certificates_only() {
        let path = "ca_file = \"C:/path/to/ca.pem\"";
        let ok = workshop_table(&table(path), &with_ca(FileContent::Bytes(PEM.into()))).unwrap();
        assert_eq!(ok.ca_sha256, Some(crate::sha256_hex(PEM.as_bytes())));
        for bad in [
            FileContent::TooLarge,
            FileContent::Unreadable("denied".to_owned()),
            FileContent::Bytes(b"not a certificate".to_vec()),
            FileContent::Bytes(PEM.replace("TUlJQmV4YW1wbGU=", "not base64!").into_bytes()),
            FileContent::Bytes(PEM.replace("CERTIFICATE", "PRIVATE KEY").into_bytes()),
            FileContent::Bytes(format!("{PEM}trailing").into_bytes()),
            FileContent::Bytes(PEM.replace("-----END CERTIFICATE-----\n", "").into_bytes()),
        ] {
            let found = workshop_table(&table(path), &with_ca(bad.clone()));
            assert_eq!(kinds(&found.unwrap_err()), [IssueKind::CaFile], "{bad:?}");
        }
        let relative = workshop_table(&table("ca_file = \"ca.pem\""), &CatalogueFiles::not_read());
        assert_eq!(kinds(&relative.unwrap_err()), [IssueKind::CaFile]);
        let skipped = workshop_table(&table(path), &CatalogueFiles::not_read()).unwrap();
        assert_eq!(skipped.ca_sha256, None, "officina run reads no file");
    }

    #[test]
    fn branch_names_follow_git_rules() {
        for good in ["main", "master", "release/2.1", "feat-x", "v1.2.3", "a@b"] {
            assert!(is_valid_branch_name(good), "{good}");
        }
        for bad in [
            "", "@", "-x", "/x", "x/", "x.", "a..b", "a@{1}", "a b", "a~1", "a^", "a:b", "a?",
            "a*", "a[b", "a\\b", ".hidden", "x/.y", "x.lock", "x/y.lock", "a//b", "a\u{7}",
        ] {
            assert!(!is_valid_branch_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_project_name_ending_in_a_dash_is_an_issue() {
        let found = project_workshop("api-", &table(""), &CatalogueFiles::not_read());
        assert_eq!(kinds(&found.unwrap_err()), [IssueKind::WorkshopProjectName]);
        let found = project_workshop("Api", &table(""), &CatalogueFiles::not_read());
        assert_eq!(kinds(&found.unwrap_err()), [IssueKind::WorkshopProjectName]);
        assert!(project_workshop("api-2", &table(""), &CatalogueFiles::not_read()).is_ok());
    }

    #[test]
    fn memory_name_refuses_dot_dotdot_backslash_and_colon() {
        for bad in [".", "..", "a\\b", "a/b", "C:x", ""] {
            let text = format!("memory_name = {bad:?}").replace("\\\\", "\\\\");
            let found = project_workshop("api", &table(&text), &CatalogueFiles::not_read());
            assert_eq!(
                kinds(&found.unwrap_err()),
                [IssueKind::MemoryName],
                "{bad:?}"
            );
        }
        let ok = project_workshop(
            "api",
            &table("memory_name = \"C--work-api\""),
            &CatalogueFiles::not_read(),
        );
        assert_eq!(ok.unwrap().memory_name.as_deref(), Some("C--work-api"));
    }

    #[test]
    fn mounts_differing_only_in_case_are_duplicates() {
        let found = project_workshop(
            "api",
            &table("mounts = [\".claude\", \".Claude\"]"),
            &CatalogueFiles::not_read(),
        );
        assert_eq!(kinds(&found.unwrap_err()), [IssueKind::WorkshopMounts]);
        for bad in [
            "..",
            "a/../b",
            "./a",
            "/abs",
            "C:/abs",
            ".git",
            ".GIT/hooks",
            "a//b",
            "",
        ] {
            let text = format!("mounts = [{bad:?}]");
            let found = project_workshop("api", &table(&text), &CatalogueFiles::not_read());
            assert_eq!(
                kinds(&found.unwrap_err()),
                [IssueKind::WorkshopMounts],
                "{bad}"
            );
        }
        let many: Vec<String> = (0..=MAX_MOUNTS).map(|i| format!("\"m{i}\"")).collect();
        let text = format!("mounts = [{}]", many.join(", "));
        let found = project_workshop("api", &table(&text), &CatalogueFiles::not_read());
        assert!(kinds(&found.unwrap_err()).contains(&IssueKind::WorkshopMounts));
        let ok = project_workshop(
            "api",
            &table("mounts = [\"_bmad\", \".claude\", \"docs\\\\notes\"]"),
            &CatalogueFiles::not_read(),
        )
        .unwrap();
        assert_eq!(ok.mounts, [".claude", "_bmad", "docs/notes"]);
    }

    #[test]
    fn hosts_are_lowercase_names_with_a_lettered_last_label() {
        let ok = project_workshop(
            "api",
            &table("hosts = [\"static.crates.io\", \"index.crates.io\", \"*.pkg.example.com\", \"index.crates.io\"]"),
            &CatalogueFiles::not_read(),
        )
        .unwrap();
        assert_eq!(
            ok.hosts,
            ["*.pkg.example.com", "index.crates.io", "static.crates.io"]
        );
        for bad in [
            "192.0.2.10",
            "1.2.3.4.5",
            "example.com:443",
            "Example.com",
            "*.com",
            "*.example",
            "localhost",
            "-a.example.com",
            "a..example.com",
            "2001:db8::1",
            "*",
        ] {
            let text = format!("hosts = [{bad:?}]");
            let found = project_workshop("api", &table(&text), &CatalogueFiles::not_read());
            assert_eq!(
                kinds(&found.unwrap_err()),
                [IssueKind::WorkshopHosts],
                "{bad}"
            );
        }
    }

    #[test]
    fn max_usd_is_stored_as_cents() {
        for (text, cents) in [("3.0", 300), ("3", 300), ("0.01", 1), ("12.5", 1250)] {
            let template =
                template_workshop(&table(&format!("base = [\"main\"]\nmax_usd = {text}"))).unwrap();
            assert_eq!(template.max_usd_cents, Some(cents), "{text}");
        }
        for bad in ["0.001", "0", "-1.0", "\"3\"", "1e40"] {
            let found = template_workshop(&table(&format!("base = [\"main\"]\nmax_usd = {bad}")));
            assert_eq!(
                kinds(&found.unwrap_err()),
                [IssueKind::WorkshopMaxUsd],
                "{bad}"
            );
        }
    }

    #[test]
    fn the_dockerfile_rules_need_the_files_read() {
        let files = |dockerfile: FileContent, context: Result<Vec<ContextFile>, ContextProblem>| {
            CatalogueFiles {
                read: true,
                ca_file: None,
                dockerfiles: BTreeMap::from([(
                    "api".to_owned(),
                    DockerfileFiles {
                        dockerfile,
                        context,
                    },
                )]),
            }
        };
        let good = b"FROM officina-workshop:local\nRUN true\n".to_vec();
        let context = vec![ContextFile {
            name: "Dockerfile".to_owned(),
            bytes: good.clone(),
        }];
        let text = table("dockerfile = \"tools/Dockerfile\"");
        let ok = project_workshop(
            "api",
            &text,
            &files(FileContent::Bytes(good.clone()), Ok(context.clone())),
        )
        .unwrap();
        let facts = ok.dockerfile.unwrap();
        assert_eq!(facts.sha256, Some(crate::sha256_hex(&good)));
        assert_eq!(facts.context_sha256, Some(context_sha256(&context)));
        let cases = [
            (
                FileContent::TooLarge,
                Ok(context.clone()),
                IssueKind::DockerfileFile,
            ),
            (
                FileContent::Unreadable("gone".into()),
                Ok(context.clone()),
                IssueKind::DockerfileFile,
            ),
            (
                FileContent::Bytes(good.clone()),
                Err(ContextProblem::TooManyFiles),
                IssueKind::DockerfileContext,
            ),
            (
                FileContent::Bytes(good.clone()),
                Err(ContextProblem::Subfolder("x".into())),
                IssueKind::DockerfileContext,
            ),
            (
                FileContent::Bytes(good.clone()),
                Err(ContextProblem::Symlink("l".into())),
                IssueKind::DockerfileContext,
            ),
            (
                FileContent::Bytes(b"FROM debian\n".to_vec()),
                Ok(context.clone()),
                IssueKind::DockerfileBase,
            ),
        ];
        for (dockerfile, context, kind) in cases {
            let found = project_workshop("api", &text, &files(dockerfile, context));
            assert_eq!(kinds(&found.unwrap_err()), [kind]);
        }
        for bad in [
            "../Dockerfile",
            "/abs/Dockerfile",
            "C:/Dockerfile",
            "a/./Dockerfile",
        ] {
            let found = project_workshop(
                "api",
                &table(&format!("dockerfile = {bad:?}")),
                &CatalogueFiles::not_read(),
            );
            assert_eq!(
                kinds(&found.unwrap_err()),
                [IssueKind::DockerfilePath],
                "{bad}"
            );
        }
        let unread = project_workshop("api", &text, &CatalogueFiles::not_read()).unwrap();
        assert_eq!(unread.dockerfile.unwrap().sha256, None);
    }

    #[test]
    fn a_context_hash_covers_names_lengths_and_bytes() {
        let file = |name: &str, bytes: &[u8]| ContextFile {
            name: name.to_owned(),
            bytes: bytes.to_vec(),
        };
        let base = context_sha256(&[file("a", b"x"), file("b", b"y")]);
        assert_eq!(base, context_sha256(&[file("b", b"y"), file("a", b"x")]));
        for changed in [
            vec![file("a", b"x"), file("c", b"y")],
            vec![file("a", b"x"), file("b", b"yy")],
            vec![file("a", b"xy"), file("b", b"")],
        ] {
            assert_ne!(context_sha256(&changed), base);
        }
    }

    #[test]
    fn file_requests_follow_valid_keys_only() {
        let floor = crate::floor::parse_floor(
            "schema = 2\n[workshop]\nca_file = \"C:/path/to/ca.pem\"\n[[project]]\nname = \"api\"\npath = \"Api\"\n[project.workshop]\ndockerfile = \"tools\\\\Dockerfile\"\n[[project]]\nname = \"web\"\npath = \"D:/web\"\n[project.workshop]\ndockerfile = \"../x\"\n",
        )
        .unwrap();
        let requests = file_requests(&floor, "C:/path/to/work/officina.toml");
        assert_eq!(requests.ca_file.as_deref(), Some("C:/path/to/ca.pem"));
        assert_eq!(
            requests.dockerfiles,
            [DockerfileRequest {
                project: "api".to_owned(),
                path: "C:/path/to/work/Api/tools/Dockerfile".to_owned(),
            }]
        );
    }
}
