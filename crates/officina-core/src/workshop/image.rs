//! Images (AD-99): the Officina base images a project Dockerfile may
//! descend from, and the one Dockerfile parser, [`stages`], which phase 2
//! (story 6.2) and Build images (story 6.8) both use.

use serde::{Deserialize, Serialize};

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What an image is (AD-99, D-212): the base, the browser base (Slice
    /// B), the gateway, or a project's.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ImageKind {
        Workshop,
        Browser,
        Gateway,
        Project,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// How a build ended (D-212): a project image that does not descend
    /// from a base, or replaces its programs or reserved variables, is
    /// `not_from_base` (D-233).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum BuildOutcome {
        Built,
        Failed,
        NotFromBase,
        Stopped,
    }
}

/// The image every workshop project builds on (AD-99).
pub const WORKSHOP_BASE: &str = "officina-workshop:local";
/// The browser base, Slice B (AD-99, FR-16).
pub const BROWSER_BASE: &str = "officina-workshop-browser:local";
/// The images a project Dockerfile's final stage may descend from.
pub const PROJECT_BASES: &[&str] = &[WORKSHOP_BASE, BROWSER_BASE];

/// One `FROM` of a Dockerfile: the image it names, as written, and its
/// alias, lowercase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage {
    pub image: String,
    pub alias: Option<String>,
}

/// Why a Dockerfile's stages cannot be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StageError {
    #[error("the Dockerfile has no FROM line")]
    NoFrom,
    #[error("line {0}: a FROM line names no image")]
    EmptyFrom(usize),
    #[error("line {0}: an instruction appears before the first FROM")]
    BeforeFrom(usize),
}

/// The `FROM` lines of `dockerfile`, in order. Comments, blank lines,
/// backslash continuations and heredoc bodies (`<<WORD` to a line holding
/// `WORD`) are skipped, so a heredoc line spelled like a `FROM` is never
/// read as one; `ARG` may precede the first `FROM`, nothing else may.
///
/// ```
/// use officina_core::workshop::image::stages;
/// let found = stages("FROM rust:1 AS build\nRUN cargo build\nFROM officina-workshop:local\n").unwrap();
/// assert_eq!(found.len(), 2);
/// assert_eq!(found[0].alias.as_deref(), Some("build"));
/// ```
pub fn stages(dockerfile: &str) -> Result<Vec<Stage>, StageError> {
    let mut found = Vec::new();
    let mut heredoc: Option<String> = None;
    let mut instruction = String::new();
    let mut first_line = 0;
    for (index, raw) in dockerfile.lines().enumerate() {
        let number = index + 1;
        let line = raw.trim_end_matches('\r');
        if let Some(end) = &heredoc {
            if line.trim() == end {
                heredoc = None;
            }
            continue;
        }
        let trimmed = line.trim_start();
        if instruction.is_empty() && (trimmed.is_empty() || trimmed.starts_with('#')) {
            continue;
        }
        if instruction.is_empty() {
            first_line = number;
        }
        if let Some(body) = trimmed.strip_suffix('\\') {
            instruction.push_str(body);
            instruction.push(' ');
            continue;
        }
        instruction.push_str(trimmed);
        let whole = std::mem::take(&mut instruction);
        heredoc = heredoc_end(&whole);
        let mut words = whole.split_whitespace();
        let keyword = words.next().unwrap_or_default().to_ascii_uppercase();
        if keyword == "FROM" {
            let rest: Vec<&str> = words.filter(|w| !w.starts_with("--")).collect();
            let image = rest
                .first()
                .ok_or(StageError::EmptyFrom(first_line))?
                .to_string();
            let alias = match rest.get(1) {
                Some(word) if word.eq_ignore_ascii_case("as") => {
                    rest.get(2).map(|a| a.to_ascii_lowercase())
                }
                _ => None,
            };
            found.push(Stage { image, alias });
        } else if found.is_empty() && keyword != "ARG" {
            return Err(StageError::BeforeFrom(first_line));
        }
    }
    if found.is_empty() {
        return Err(StageError::NoFrom);
    }
    Ok(found)
}

/// The word ending a heredoc an instruction opens (`<<EOF`, `<<-EOF`,
/// `<<"EOF"`), if it opens one.
fn heredoc_end(instruction: &str) -> Option<String> {
    let start = instruction.find("<<")?;
    let rest = instruction[start + 2..].trim_start_matches('-');
    let word: String = rest
        .trim_start_matches(['"', '\''])
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    (!word.is_empty()).then_some(word)
}

/// The Officina base the final stage descends from, following stage aliases
/// back to an image; `None` when it reaches any other image, or names a
/// variable (`FROM ${BASE}`), which phase 2 cannot resolve. Other stages may
/// name any image (a builder stage pulling a toolchain).
///
/// ```
/// use officina_core::workshop::image::{final_base, stages, WORKSHOP_BASE};
/// let found = stages("FROM officina-workshop:local AS base\nFROM base\n").unwrap();
/// assert_eq!(final_base(&found), Some(WORKSHOP_BASE));
/// ```
pub fn final_base(found: &[Stage]) -> Option<&'static str> {
    let mut index = found.len().checked_sub(1)?;
    loop {
        let image = found[index].image.as_str();
        if let Some(base) = PROJECT_BASES.iter().find(|b| **b == image) {
            return Some(base);
        }
        let alias = image.to_ascii_lowercase();
        index = found[..index]
            .iter()
            .rposition(|stage| stage.alias.as_deref() == Some(alias.as_str()))?;
    }
}

/// `docker/workshop.Dockerfile`, embedded: the build writes it into the
/// base's context, and its SHA-256 is a label (AD-99).
pub const WORKSHOP_DOCKERFILE: &str = include_str!("../../../../docker/workshop.Dockerfile");
/// `docker/gateway.Dockerfile`, embedded.
pub const GATEWAY_DOCKERFILE: &str = include_str!("../../../../docker/gateway.Dockerfile");
/// The gateway image's tag (AD-99).
pub const GATEWAY_IMAGE: &str = "officina-gateway:local";

/// The labels Officina sets at build (AD-99): which base, the version and
/// the SHA-256 of what went into the image.
pub const LABEL_BASE: &str = "officina.base";
pub const LABEL_VERSION: &str = "officina.version";
pub const LABEL_DOCKERFILE: &str = "officina.dockerfile.sha256";
pub const LABEL_ENTRY: &str = "officina.entry.sha256";
pub const LABEL_GATEWAY: &str = "officina.gateway.sha256";
pub const LABEL_CLAUDE_VERSION: &str = "officina.claude.version";
pub const LABEL_CA: &str = "officina.ca.sha256";
pub const LABEL_PROJECT: &str = "officina.project";
pub const LABEL_PROJECT_DOCKERFILE: &str = "officina.project.dockerfile.sha256";
pub const LABEL_PROJECT_CONTEXT: &str = "officina.project.context.sha256";
pub const LABEL_BASE_ID: &str = "officina.base.id";
/// `officina.ca.sha256` without a CA file.
pub const NO_CA: &str = "none";

/// The labels a check never compares: Claude Code updates itself on the
/// host, and comparing its version would force a rebuild each time
/// (AD-99); the OCI labels describe the build, not what is in it.
pub fn is_compared(label: &str) -> bool {
    label.starts_with("officina.") && label != LABEL_CLAUDE_VERSION
}

/// What a project image adds to the labels (AD-99).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectLabels {
    pub floor_key: String,
    pub project: String,
    pub dockerfile_sha256: String,
    pub context_sha256: String,
    /// The base image's id at build time.
    pub base_id: String,
}

/// What the labels of an image are made from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelInputs {
    pub kind: ImageKind,
    pub version: String,
    pub revision: String,
    pub created: String,
    pub entry_sha256: String,
    pub gateway_sha256: String,
    pub claude_version: String,
    pub ca_sha256: Option<String>,
    pub project: Option<ProjectLabels>,
}

/// The `officina.base` value of each Officina image kind; a project image
/// inherits its base's.
pub const fn base_label(kind: ImageKind) -> Option<&'static str> {
    match kind {
        ImageKind::Workshop => Some("workshop"),
        ImageKind::Browser => Some("browser"),
        ImageKind::Gateway => Some("gateway"),
        ImageKind::Project => None,
    }
}

/// Every label of an image of `inputs` (AD-99): the OCI set, then
/// Officina's.
pub fn labels(inputs: &LabelInputs) -> std::collections::BTreeMap<String, String> {
    let dockerfile = match inputs.kind {
        ImageKind::Gateway => GATEWAY_DOCKERFILE,
        ImageKind::Workshop | ImageKind::Browser | ImageKind::Project => WORKSHOP_DOCKERFILE,
    };
    let title = match inputs.kind {
        ImageKind::Workshop => "officina-workshop",
        ImageKind::Browser => "officina-workshop-browser",
        ImageKind::Gateway => "officina-gateway",
        ImageKind::Project => "officina-project",
    };
    let mut labels: std::collections::BTreeMap<String, String> = [
        ("org.opencontainers.image.title", title.to_owned()),
        (
            "org.opencontainers.image.description",
            "Officina's isolated workshop".to_owned(),
        ),
        (
            "org.opencontainers.image.source",
            "https://github.com/Rwx-G/Officina".to_owned(),
        ),
        ("org.opencontainers.image.version", inputs.version.clone()),
        ("org.opencontainers.image.revision", inputs.revision.clone()),
        ("org.opencontainers.image.licenses", "Apache-2.0".to_owned()),
        ("org.opencontainers.image.created", inputs.created.clone()),
        (LABEL_VERSION, inputs.version.clone()),
        (LABEL_ENTRY, inputs.entry_sha256.clone()),
        (LABEL_GATEWAY, inputs.gateway_sha256.clone()),
        (LABEL_CLAUDE_VERSION, inputs.claude_version.clone()),
        (
            LABEL_CA,
            inputs.ca_sha256.clone().unwrap_or_else(|| NO_CA.to_owned()),
        ),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    if let Some(base) = base_label(inputs.kind) {
        labels.insert(LABEL_BASE.to_owned(), base.to_owned());
        labels.insert(
            LABEL_DOCKERFILE.to_owned(),
            crate::sha256_hex(dockerfile.as_bytes()),
        );
    }
    if let Some(project) = &inputs.project {
        labels.insert(
            super::names::FLOOR_LABEL.to_owned(),
            project.floor_key.clone(),
        );
        labels.insert(LABEL_PROJECT.to_owned(), project.project.clone());
        labels.insert(
            LABEL_PROJECT_DOCKERFILE.to_owned(),
            project.dockerfile_sha256.clone(),
        );
        labels.insert(
            LABEL_PROJECT_CONTEXT.to_owned(),
            project.context_sha256.clone(),
        );
        labels.insert(LABEL_BASE_ID.to_owned(), project.base_id.clone());
    }
    labels
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why an image is not the one the running Officina would build now
    /// (AD-99's `image_mismatch`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum ImageMismatch {
        /// The engine holds no such image.
        Absent,
        /// Officina has no record of building it, or its build failed.
        NotRecorded,
        /// A compared label differs.
        LabelDiffers,
        /// A project image was built on another base image.
        BaseIdDiffers,
        /// The image's measured programs are not the base's.
        ProgramsDiffer,
    }
}

/// A mismatch and the label it is about, when one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    pub cause: ImageMismatch,
    pub label: Option<String>,
}

/// What the running Officina expects of an image (AD-99, D-233).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expected {
    /// The labels it would set now; only [`is_compared`] ones count.
    pub labels: std::collections::BTreeMap<String, String>,
    /// For a project image, the current base image's id.
    pub base_id: Option<String>,
    /// The base's measured programs, for a project or browser image.
    pub base_programs_sha256: Option<String>,
}

/// What the registry recorded of the image's last build (D-233).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    pub image_id: Option<String>,
    pub programs_sha256: Option<String>,
    pub built: bool,
}

/// The image check at launch (AD-99, D-207, D-233): the image exists, its
/// compared labels equal what Officina would build now, a project image's
/// base id is the current base's, and the programs measured at its build
/// are the base's. A label is a claim; the measurement is the check.
/// Returns the id a `create` names.
pub fn check(
    expected: &Expected,
    inspected: Option<&super::engine::ImageFacts>,
    recorded: Option<&Recorded>,
) -> Result<String, Mismatch> {
    let mismatch = |cause, label: Option<&str>| Mismatch {
        cause,
        label: label.map(str::to_owned),
    };
    let image = inspected.ok_or_else(|| mismatch(ImageMismatch::Absent, None))?;
    let recorded = recorded
        .filter(|r| r.built && r.image_id.as_deref() == Some(image.id.as_str()))
        .ok_or_else(|| mismatch(ImageMismatch::NotRecorded, None))?;
    for (label, value) in expected.labels.iter().filter(|(l, _)| is_compared(l)) {
        if label == LABEL_BASE_ID {
            continue;
        }
        if image.labels.get(label) != Some(value) {
            return Err(mismatch(ImageMismatch::LabelDiffers, Some(label)));
        }
    }
    if let Some(base_id) = &expected.base_id
        && image.labels.get(LABEL_BASE_ID) != Some(base_id)
    {
        return Err(mismatch(ImageMismatch::BaseIdDiffers, Some(LABEL_BASE_ID)));
    }
    // The gateway image holds no measured program (D-233): it is built
    // `FROM scratch` with the gateway alone, whose digest is a label.
    let measured =
        expected.labels.get(LABEL_BASE).map(String::as_str) != Some(ImageKind::Gateway.code());
    match (&expected.base_programs_sha256, &recorded.programs_sha256) {
        (_, None) if !measured => {}
        (_, None) => return Err(mismatch(ImageMismatch::ProgramsDiffer, None)),
        (Some(base), Some(own)) if base != own => {
            return Err(mismatch(ImageMismatch::ProgramsDiffer, None));
        }
        _ => {}
    }
    Ok(image.id.clone())
}

/// The host the native installer downloads Claude Code from
/// (`network-config`, the gate 1 entry): a build takes this entry's route
/// as its proxy build arguments (D-611).
pub const INSTALLER_HOST: &str = "downloads.claude.ai";
/// Where the base puts the entry point (AD-99, AD-100).
pub const ENTRY_PATH: &str = "/opt/officina/bin/officina-entry";
/// Where the base puts Claude Code (AD-99).
pub const CLAUDE_PATH: &str = "/opt/officina/bin/claude";
/// The programs measured in every base, browser and project image, in the
/// order [`programs_sha256`] takes their digests (D-233).
pub const MEASURED_PROGRAMS: [&str; 2] = [ENTRY_PATH, CLAUDE_PATH];
/// The build argument carrying the host's Claude Code version (AD-99).
pub const CLAUDE_VERSION_ARG: &str = "CLAUDE_CODE_VERSION";
/// Docker's predefined proxy build arguments a build passes when the
/// installer's host goes through a proxy (D-611); the engine keeps them out
/// of the image's history.
pub const PROXY_BUILD_ARGS: [&str; 4] = ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"];
/// The predefined argument naming what bypasses the proxy.
pub const NO_PROXY_BUILD_ARG: &str = "NO_PROXY";

/// An image's `programs_sha256` (D-233): one digest over each measured
/// program's path and SHA-256, in [`MEASURED_PROGRAMS`]' order, so a
/// replaced entry point or Claude Code moves it.
///
/// ```
/// use officina_core::workshop::image::programs_sha256;
/// assert_ne!(programs_sha256(&["a".into(), "b".into()]), programs_sha256(&["b".into(), "a".into()]));
/// ```
pub fn programs_sha256(digests: &[String; 2]) -> String {
    let text: String = MEASURED_PROGRAMS
        .iter()
        .zip(digests)
        .map(|(path, digest)| format!("{path} {digest}\n"))
        .collect();
    crate::sha256_hex(text.as_bytes())
}

/// The build arguments of an Officina image (AD-99, D-611): the host's
/// Claude Code version, and Docker's predefined proxy arguments when the
/// installer's host has a proxy route.
pub fn build_args(
    claude_version: &str,
    proxy: Option<&str>,
) -> std::collections::BTreeMap<String, String> {
    let mut args = std::collections::BTreeMap::from([(
        CLAUDE_VERSION_ARG.to_owned(),
        claude_version.to_owned(),
    )]);
    if let Some(proxy) = proxy {
        let url = format!("http://{proxy}");
        for name in PROXY_BUILD_ARGS {
            args.insert(name.to_owned(), url.clone());
        }
        args.insert(
            NO_PROXY_BUILD_ARG.to_owned(),
            super::vector::NO_PROXY.to_owned(),
        );
    }
    args
}

/// The prefixes no image `ENV` name may start with (D-233, review F13):
/// Claude Code reads its configuration and credentials under them.
pub const RESERVED_ENV_PREFIXES: &[&str] = &["ANTHROPIC_", "CLAUDE_CODE_", "CLAUDE_CONFIG_"];

/// The first of `env_names` an image may not set (D-233, review F13): a
/// name the worker's vector sets (derived from the builders), either
/// credential name, or a reserved prefix, compared ignoring ASCII case.
/// The image's `ENV` applies under the vector's names, so such a name
/// would add a second credential or redirect the API.
pub fn reserved_env(env_names: &[String]) -> Option<String> {
    let mut reserved = super::vector::reserved_env_names();
    for kind in super::floor::CredentialKind::ALL {
        if let Some(name) = super::vector::credential_name(*kind) {
            reserved.push(name.to_owned());
        }
    }
    env_names
        .iter()
        .find(|name| {
            let upper = name.to_ascii_uppercase();
            reserved.iter().any(|r| r.eq_ignore_ascii_case(name))
                || RESERVED_ENV_PREFIXES.iter().any(|p| upper.starts_with(p))
        })
        .cloned()
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why a built image is refused `not_from_base` and removed (AD-99,
    /// D-233). No `BuildOutcome` variant is added for them.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum NotFromBase {
        /// The project Dockerfile's final stage does not descend from an
        /// Officina base (checked before the build).
        Stages,
        /// The built image does not carry the base's `officina.base`.
        NoBaseLabel,
        /// Its measured programs are not the base's.
        ProgramsReplaced,
        /// Its `ENV` sets a reserved name.
        ReservedEnv,
    }
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why Build images did not start (AD-99, AD-100).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum BuildRefusal {
        /// `officina-gateway` or `officina-entry` is not in `<exe
        /// dir>/linux/` or `OFFICINA_LINUX_BIN_DIR` (AD-100).
        LinuxProgramsMissing,
        /// The workshop's stop flag is set (AD-116).
        WorkshopStopped,
        /// The floor declares no valid `[workshop]`.
        WorkshopOff,
        /// No catalogue was approved, or the window's record does not vouch
        /// for it (D-175).
        NotApproved,
        /// The floor or a workshop file changed since its approval: a
        /// Dockerfile, its folder or the CA file (D-244).
        CatalogueChanged,
        /// The host's Claude Code version is not known, so the base would
        /// install a version nobody chose.
        ClaudeVersionUnknown,
        /// The engine is not a reachable Linux engine (AD-98).
        EngineUnavailable,
        /// The engine has no `docker`-driver builder for its context (the
        /// gate 1 entry, review F41).
        BuilderMissing,
        /// The installer host's route could not be decided (D-611).
        ProxyRefused,
        /// A build context could not be written under the home.
        ContextUnwritable,
        /// A build is already running.
        Busy,
    }
}

/// How `build_images` was answered: queued on thread `workshop-build`,
/// whose outcomes reach the registry and the journal per image, or refused
/// before any build starts, with why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "outcome", content = "detail", rename_all = "snake_case")]
pub enum BuildAnswer {
    Queued,
    Refused(BuildRefusal),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(kind: ImageKind) -> LabelInputs {
        LabelInputs {
            kind,
            version: "0.1.0".into(),
            revision: "abc".into(),
            created: "2026-11-01T00:00:00Z".into(),
            entry_sha256: "e".repeat(64),
            gateway_sha256: "g".repeat(64),
            claude_version: "2.1.316".into(),
            ca_sha256: None,
            project: None,
        }
    }

    #[test]
    fn embedded_dockerfiles_hash_to_their_labels() {
        let base = labels(&inputs(ImageKind::Workshop));
        assert_eq!(
            base[LABEL_DOCKERFILE],
            crate::sha256_hex(WORKSHOP_DOCKERFILE.as_bytes())
        );
        assert_eq!(base[LABEL_BASE], "workshop");
        assert_eq!(base[LABEL_CA], NO_CA);
        let gateway = labels(&inputs(ImageKind::Gateway));
        assert_eq!(
            gateway[LABEL_DOCKERFILE],
            crate::sha256_hex(GATEWAY_DOCKERFILE.as_bytes())
        );
        assert!(
            stages(WORKSHOP_DOCKERFILE).unwrap()[0]
                .image
                .starts_with("debian:trixie-slim@sha256:")
        );
        assert_eq!(stages(GATEWAY_DOCKERFILE).unwrap()[0].image, "scratch");
    }

    /// A context file keeps the mode the host wrote it with, and COPY keeps
    /// that mode: on Linux the program lands without its executable bit and
    /// the entry point cannot start (the CI engine's gateway, D-1071).
    #[test]
    fn each_entry_point_is_copied_with_its_own_mode() {
        for dockerfile in [WORKSHOP_DOCKERFILE, GATEWAY_DOCKERFILE] {
            let entry = dockerfile
                .lines()
                .find_map(|l| l.strip_prefix("ENTRYPOINT [\"")?.split('"').next())
                .expect("an entry point");
            let copy = dockerfile
                .lines()
                .find(|l| l.starts_with("COPY ") && l.split_whitespace().last() == Some(entry))
                .expect("the entry point's COPY");
            assert!(copy.contains(" --chmod=0755 "), "{copy}");
        }
    }

    #[test]
    fn the_ca_step_precedes_every_https_download() {
        let lines: Vec<&str> = WORKSHOP_DOCKERFILE.lines().collect();
        let ca = lines
            .iter()
            .position(|l| l.contains("update-ca-certificates"))
            .expect("the CA step");
        let first_https = lines
            .iter()
            .position(|l| l.contains("https://"))
            .expect("a download");
        assert!(
            ca < first_https,
            "the CA step at {ca}, a download at {first_https}"
        );
        let apt = lines
            .iter()
            .position(|l| l.contains("apt-get install"))
            .unwrap();
        assert!(
            apt < ca,
            "the Debian packages come first, over Debian's mirror"
        );
    }

    fn inspected(
        labels: &std::collections::BTreeMap<String, String>,
    ) -> crate::workshop::engine::ImageFacts {
        crate::workshop::engine::ImageFacts {
            id: "sha256:aa".into(),
            labels: labels.clone(),
            env_names: vec!["PATH".into()],
        }
    }

    fn recorded(programs: Option<&str>) -> Recorded {
        Recorded {
            image_id: Some("sha256:aa".into()),
            programs_sha256: programs.map(str::to_owned),
            built: true,
        }
    }

    #[test]
    fn the_claude_version_is_not_compared() {
        let built = labels(&inputs(ImageKind::Workshop));
        let mut now = inputs(ImageKind::Workshop);
        now.claude_version = "2.1.400".into();
        now.created = "2026-12-01T00:00:00Z".into();
        let expected = Expected {
            labels: labels(&now),
            base_id: None,
            base_programs_sha256: None,
        };
        assert_eq!(
            check(
                &expected,
                Some(&inspected(&built)),
                Some(&recorded(Some("p")))
            ),
            Ok("sha256:aa".into())
        );
    }

    #[test]
    fn a_label_without_a_matching_measurement_mismatches() {
        let mut project = inputs(ImageKind::Project);
        project.project = Some(ProjectLabels {
            floor_key: "0123456789abcdef".into(),
            project: "api".into(),
            dockerfile_sha256: "d".into(),
            context_sha256: "c".into(),
            base_id: "sha256:base".into(),
        });
        let built = labels(&project);
        let expected = Expected {
            labels: built.clone(),
            base_id: Some("sha256:base".into()),
            base_programs_sha256: Some("base-programs".into()),
        };
        assert_eq!(
            check(
                &expected,
                Some(&inspected(&built)),
                Some(&recorded(Some("replaced")))
            )
            .unwrap_err()
            .cause,
            ImageMismatch::ProgramsDiffer
        );
        assert_eq!(
            check(
                &expected,
                Some(&inspected(&built)),
                Some(&recorded(Some("base-programs")))
            ),
            Ok("sha256:aa".into())
        );
        let other_base = Expected {
            base_id: Some("sha256:new-base".into()),
            ..expected.clone()
        };
        assert_eq!(
            check(
                &other_base,
                Some(&inspected(&built)),
                Some(&recorded(Some("base-programs")))
            )
            .unwrap_err()
            .cause,
            ImageMismatch::BaseIdDiffers
        );
        assert_eq!(
            check(&expected, None, None).unwrap_err().cause,
            ImageMismatch::Absent
        );
        assert_eq!(
            check(&expected, Some(&inspected(&built)), None)
                .unwrap_err()
                .cause,
            ImageMismatch::NotRecorded
        );
        let mut changed = built.clone();
        changed.insert(LABEL_ENTRY.into(), "other".into());
        let mismatch = check(
            &expected,
            Some(&inspected(&changed)),
            Some(&recorded(Some("base-programs"))),
        )
        .unwrap_err();
        assert_eq!(mismatch.cause, ImageMismatch::LabelDiffers);
        assert_eq!(mismatch.label.as_deref(), Some(LABEL_ENTRY));
    }

    #[test]
    fn the_final_stage_must_descend_from_a_base() {
        for base in PROJECT_BASES {
            let found = stages(&format!("FROM {base}\nRUN true\n")).unwrap();
            assert_eq!(final_base(&found), Some(*base));
        }
        let other = stages("FROM officina-workshop:local\nFROM debian:trixie-slim\n").unwrap();
        assert_eq!(final_base(&other), None);
        let variable = stages("ARG BASE=officina-workshop:local\nFROM ${BASE}\n").unwrap();
        assert_eq!(final_base(&variable), None, "a variable is not resolved");
        let tagged = stages("FROM officina-workshop:latest\n").unwrap();
        assert_eq!(final_base(&tagged), None);
    }

    #[test]
    fn an_alias_chain_is_followed() {
        let text = "FROM officina-workshop:local AS One\nFROM one AS two\nRUN x\nFROM TWO\n";
        assert_eq!(final_base(&stages(text).unwrap()), Some(WORKSHOP_BASE));
        let unknown = "FROM officina-workshop:local AS one\nFROM three\n";
        assert_eq!(final_base(&stages(unknown).unwrap()), None);
        let later = "FROM two AS one\nFROM officina-workshop:local AS two\nFROM one\n";
        assert_eq!(
            final_base(&stages(later).unwrap()),
            None,
            "an alias names an earlier stage only"
        );
    }

    #[test]
    fn a_builder_stage_may_name_any_image() {
        let text = "# syntax\nARG V=1\nFROM --platform=linux/amd64 rust:1-bookworm AS build\nRUN cargo build \\\n  --release\nFROM officina-workshop:local\nCOPY --from=build /x /y\n";
        let found = stages(text).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].image, "rust:1-bookworm");
        assert_eq!(final_base(&found), Some(WORKSHOP_BASE));
    }

    #[test]
    fn a_heredoc_line_is_never_a_stage() {
        let text = "FROM debian:trixie-slim\nRUN <<EOF\nFROM officina-workshop:local\nEOF\n";
        let found = stages(text).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(final_base(&found), None);
    }

    #[test]
    fn malformed_files_are_refused() {
        assert_eq!(stages("# only a comment\n"), Err(StageError::NoFrom));
        assert_eq!(stages("RUN x\nFROM a\n"), Err(StageError::BeforeFrom(1)));
        assert_eq!(stages("FROM\n"), Err(StageError::EmptyFrom(1)));
        assert_eq!(stages("FROM --platform=x\n"), Err(StageError::EmptyFrom(1)));
    }

    /// D-233, review F13: every name the worker's vector sets, both
    /// credential names and each reserved prefix are refused, in any case;
    /// the base's own `ENV` names pass.
    #[test]
    fn reserved_names_are_derived_from_the_builders() {
        let mut refused: Vec<String> = crate::workshop::vector::reserved_env_names();
        assert!(!refused.is_empty());
        for kind in crate::workshop::floor::CredentialKind::ALL {
            refused.extend(crate::workshop::vector::credential_name(*kind).map(str::to_owned));
        }
        refused.extend(RESERVED_ENV_PREFIXES.iter().map(|p| format!("{p}X")));
        for name in refused {
            for spelled in [name.clone(), name.to_ascii_lowercase()] {
                let names = vec!["PATH".to_owned(), spelled.clone()];
                assert_eq!(reserved_env(&names), Some(spelled), "{name}");
            }
        }
        let base: Vec<String> = [
            "PATH",
            "SSL_CERT_FILE",
            "REQUESTS_CA_BUNDLE",
            "NODE_EXTRA_CA_CERTS",
            "UV_NATIVE_TLS",
        ]
        .map(str::to_owned)
        .to_vec();
        assert_eq!(reserved_env(&base), None, "the base's own ENV passes");
    }

    #[test]
    fn a_proxy_route_becomes_the_predefined_build_args() {
        let direct = build_args("2.1.316", None);
        assert_eq!(direct.len(), 1);
        assert_eq!(direct[CLAUDE_VERSION_ARG], "2.1.316");
        let routed = build_args("2.1.316", Some("proxy.example.com:8080"));
        for name in PROXY_BUILD_ARGS {
            assert_eq!(routed[name], "http://proxy.example.com:8080");
        }
        assert_eq!(
            routed[NO_PROXY_BUILD_ARG],
            crate::workshop::vector::NO_PROXY
        );
    }

    #[test]
    fn the_programs_digest_moves_with_either_program() {
        let base = programs_sha256(&["e".repeat(64), "c".repeat(64)]);
        assert_ne!(base, programs_sha256(&["f".repeat(64), "c".repeat(64)]));
        assert_ne!(base, programs_sha256(&["e".repeat(64), "d".repeat(64)]));
        assert_eq!(base, programs_sha256(&["e".repeat(64), "c".repeat(64)]));
    }
}
