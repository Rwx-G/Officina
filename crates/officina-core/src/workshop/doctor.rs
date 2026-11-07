//! Doctor's workshop section (AD-123, D-228): every finding is a variant of
//! [`WorkshopCheck`], with its level and title, and the section prints in
//! the enum's declaration order (story 6.24 orders the whole section). The
//! host gathers the facts; the rules that read them live here.

use serde::{Deserialize, Serialize};

use super::engine::{ObjectRow, PsRow};
use super::names::{LOGIN_ROLE, ROLE_LABEL, RUN_LABEL};

/// How much a finding weighs in doctor (D-228).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckLevel {
    /// The workshop cannot run until it is fixed.
    Fail,
    /// It runs, but something is degraded or exposed.
    Warn,
    /// A fact worth reading, nothing to fix.
    Inform,
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// One finding of doctor's workshop section. The declaration order is
    /// the order the section prints in.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum WorkshopCheck {
        /// `docker.exe` resolved, with its source.
        DockerResolved,
        /// The `docker` found is not a native `.exe` (AD-51).
        DockerShim,
        /// No `docker` was found.
        DockerMissing,
        /// The workshop's `git` resolved, with its source.
        GitResolved,
        /// The `git` found is not a native `.exe`.
        GitShim,
        /// No `git` was found.
        GitMissing,
        /// Variables that steer which engine or binary is used are set.
        OverridesSet,
        /// The Docker CLI's `config.json` has a `proxies` entry, which the
        /// CLI adds to every container it creates (D-206).
        DockerCliProxies,
        /// The Docker CLI's `config.json` names a current context.
        DockerCliContext,
        /// The Docker CLI's `config.json` adds plugin folders, whose
        /// programs every `docker` call may start.
        DockerCliPluginDirs,
        /// The Docker CLI's `config.json` exists and is not JSON.
        DockerCliConfigUnreadable,
        /// `DOCKER_HOST` names a TCP engine without TLS (review F40).
        PlainTcpDockerHost,
        /// A Linux engine answered, with its key.
        EngineReady,
        /// The engine does not run.
        EngineNotRunning,
        /// The engine runs Windows containers.
        EngineWindowsContainers,
        /// `docker version` gave no answer.
        EngineUnreachable,
        /// `officina-gateway` and `officina-entry` found (AD-100).
        LinuxPrograms,
        /// Either Linux program is missing: Build images is refused.
        LinuxProgramsMissing,
        /// An image is present and matches what Officina would build now.
        ImageMatches,
        /// An image is not built yet.
        ImageAbsent,
        /// An image differs: the label or the measurement named (AD-99,
        /// D-233); a launch on it is refused `image_mismatch`.
        ImageMismatch,
        /// The base image's Claude Code differs from the host's (AD-99:
        /// not compared at launch).
        ClaudeVersionDiffers,
        /// The CA label found on the base image (D-208).
        CaLabel,
        /// `ca_file` cannot be read, or no longer hashes to the approved
        /// digest.
        CaUnreadable,
        /// The base image's CA label is not the floor's CA file (D-208).
        CaLabelMismatch,
        /// `max_workers` times `memory_mib` exceeds the engine's memory.
        WorkersOverMemory,
        /// Networks labelled by Officina that no container of theirs uses.
        OrphanNetworks,
        /// Volumes labelled by Officina that no container of theirs uses.
        OrphanVolumes,
        /// Both stop flags, each for what it stops (AD-116, P-50).
        StopFlags,
        /// Runs Stop all kept, with their age: their containers hold the
        /// credential in their engine configuration until removed at 7
        /// days or by Resume (AD-116).
        KeptContainers,
        /// The Officina home lies in a sync client's folder: base bundles
        /// of up to 1 GiB and the scratch repository would sync (D-228,
        /// review F44).
        HomeSynced,
        /// The proxy setting, its source and each entry's route (D-611).
        ProxyRoutes,
        /// An entry's route could not be decided (D-611).
        ProxyRefused,
        /// The reader cannot evaluate a PAC file or automatic detection
        /// (a reader without WinHTTP, D-1136): routes are assumed direct, or refused for a PAC URL.
        ProxyNotEvaluated,
        /// `api.anthropic.com` goes through a proxy and no `ca_file` is set
        /// (D-612): if it inspects TLS, its root must be exported.
        ProxyCaNote,
        /// With `upstream_proxy` set: whether the upstream answered the
        /// newest run's gateway at its start, or unknown before any run
        /// (AD-123 as amended by amendment 14).
        UpstreamState,
        /// A project's hook and MCP programs all resolved in its image
        /// (AD-118).
        HookProgramsFound,
        /// A `PreToolUse` guard's program is missing from the image: a
        /// launch of the project is refused `hook_program_missing`.
        HookProgramMissing,
        /// A program of another event or of an MCP server is missing, or a
        /// command is `dynamic`: warned on the card.
        HookProgramWarning,
        /// No inventory is recorded for the project's image: the next
        /// launch runs it.
        InventoryAbsent,
        /// A mounted settings file holds an `env` block or `apiKeyHelper`,
        /// which the worker can read (names only).
        SettingsExposed,
        /// A mounted hook script has CRLF line endings (review F12).
        HookScriptCrlf,
        /// An image's last escape battery: its outcome and age (AD-117).
        BatteryLast,
        /// No battery valid for what would run now: `battery_stale`, named
        /// with the key that moved (AD-117, D-232); a launch is refused.
        BatteryStale,
        /// The engine's clock against the window's at the newest battery,
        /// a diagnostic only (D-242).
        BatteryClockSkew,
        /// The floor's credential is stored (never read for display), or
        /// the floor's kind holds none on the host (AD-105, D-604).
        CredentialPresent,
        /// The floor's credential is absent: the floor's `cmdkey` line,
        /// also written to a file (D-38, D-617); a launch is refused.
        CredentialAbsent,
        /// The floor's credential state, when it calls for nothing
        /// (D-607, story 6.26).
        CredentialState,
        /// The floor's credential needs the author: a login missing or
        /// needed, or a token to renew, with the one action that fixes it;
        /// nothing retries (D-607); a launch is refused.
        CredentialAction,
        /// A Claude Code managed-settings file or folder is present on
        /// this machine (presence only): workers do not apply it (D-614).
        ManagedSettingsPresent,
        /// A mount is a folder, git-ignored, holding no tracked file.
        MountReady,
        /// A mount is ignored only by a rule outside the tracked files
        /// (`.git/info/exclude`, `core.excludesFile`), named (FR-2).
        MountIgnoredUntracked,
        /// A mount's folder is missing (FR-2).
        MountMissing,
        /// A mount is not git-ignored (FR-2).
        MountNotIgnored,
        /// A mount holds files the base tracks (D-231, review F3).
        MountTracked,
        /// A mount's path holds a character the engine cannot carry, or
        /// goes through a reparse point (D-236).
        MountPathUnsupported,
        /// A mount holds Cloud Files placeholders, which the engine cannot
        /// read offline (R55), with their count.
        MountPlaceholders,
    }
}

impl WorkshopCheck {
    /// The finding's level.
    pub const fn level(self) -> CheckLevel {
        match self {
            WorkshopCheck::DockerShim
            | WorkshopCheck::DockerMissing
            | WorkshopCheck::GitShim
            | WorkshopCheck::GitMissing
            | WorkshopCheck::EngineWindowsContainers
            | WorkshopCheck::LinuxProgramsMissing
            | WorkshopCheck::ImageMismatch
            | WorkshopCheck::CaUnreadable
            | WorkshopCheck::CaLabelMismatch
            | WorkshopCheck::HookProgramMissing
            | WorkshopCheck::CredentialAbsent
            | WorkshopCheck::CredentialAction
            | WorkshopCheck::MountMissing
            | WorkshopCheck::MountNotIgnored
            | WorkshopCheck::MountTracked
            | WorkshopCheck::MountPathUnsupported => CheckLevel::Fail,
            WorkshopCheck::DockerCliProxies
            | WorkshopCheck::DockerCliPluginDirs
            | WorkshopCheck::DockerCliConfigUnreadable
            | WorkshopCheck::PlainTcpDockerHost
            | WorkshopCheck::EngineNotRunning
            | WorkshopCheck::EngineUnreachable
            | WorkshopCheck::OrphanNetworks
            | WorkshopCheck::OrphanVolumes
            | WorkshopCheck::KeptContainers
            | WorkshopCheck::HomeSynced
            | WorkshopCheck::ProxyNotEvaluated
            | WorkshopCheck::ImageAbsent
            | WorkshopCheck::ClaudeVersionDiffers
            | WorkshopCheck::WorkersOverMemory
            | WorkshopCheck::HookProgramWarning
            | WorkshopCheck::InventoryAbsent
            | WorkshopCheck::SettingsExposed
            | WorkshopCheck::HookScriptCrlf
            | WorkshopCheck::BatteryStale
            | WorkshopCheck::ManagedSettingsPresent
            | WorkshopCheck::MountPlaceholders => CheckLevel::Warn,
            WorkshopCheck::ProxyRefused => CheckLevel::Fail,
            WorkshopCheck::ProxyRoutes
            | WorkshopCheck::ProxyCaNote
            | WorkshopCheck::UpstreamState
            | WorkshopCheck::StopFlags => CheckLevel::Inform,
            WorkshopCheck::DockerResolved
            | WorkshopCheck::GitResolved
            | WorkshopCheck::OverridesSet
            | WorkshopCheck::DockerCliContext
            | WorkshopCheck::EngineReady
            | WorkshopCheck::LinuxPrograms
            | WorkshopCheck::ImageMatches
            | WorkshopCheck::CaLabel
            | WorkshopCheck::HookProgramsFound
            | WorkshopCheck::BatteryLast
            | WorkshopCheck::BatteryClockSkew
            | WorkshopCheck::CredentialPresent
            | WorkshopCheck::CredentialState
            | WorkshopCheck::MountReady
            | WorkshopCheck::MountIgnoredUntracked => CheckLevel::Inform,
        }
    }

    /// The line's title: findings about one thing share it.
    pub const fn title(self) -> &'static str {
        match self {
            WorkshopCheck::DockerResolved
            | WorkshopCheck::DockerShim
            | WorkshopCheck::DockerMissing => "workshop docker",
            WorkshopCheck::GitResolved | WorkshopCheck::GitShim | WorkshopCheck::GitMissing => {
                "workshop git"
            }
            WorkshopCheck::OverridesSet => "workshop overrides",
            WorkshopCheck::DockerCliProxies
            | WorkshopCheck::DockerCliContext
            | WorkshopCheck::DockerCliPluginDirs
            | WorkshopCheck::DockerCliConfigUnreadable => "workshop docker config",
            WorkshopCheck::PlainTcpDockerHost => "workshop docker host",
            WorkshopCheck::EngineReady
            | WorkshopCheck::EngineNotRunning
            | WorkshopCheck::EngineWindowsContainers
            | WorkshopCheck::EngineUnreachable => "workshop engine",
            WorkshopCheck::LinuxPrograms | WorkshopCheck::LinuxProgramsMissing => {
                "workshop linux programs"
            }
            WorkshopCheck::ImageMatches
            | WorkshopCheck::ImageAbsent
            | WorkshopCheck::ImageMismatch => "workshop image",
            WorkshopCheck::ClaudeVersionDiffers => "workshop claude version",
            WorkshopCheck::CaLabel
            | WorkshopCheck::CaUnreadable
            | WorkshopCheck::CaLabelMismatch => "workshop ca",
            WorkshopCheck::WorkersOverMemory => "workshop memory",
            WorkshopCheck::OrphanNetworks | WorkshopCheck::OrphanVolumes => "workshop orphans",
            WorkshopCheck::StopFlags => "workshop stop flags",
            WorkshopCheck::KeptContainers => "workshop kept containers",
            WorkshopCheck::HomeSynced => "workshop home",
            WorkshopCheck::ProxyRoutes
            | WorkshopCheck::ProxyRefused
            | WorkshopCheck::ProxyNotEvaluated
            | WorkshopCheck::ProxyCaNote => "workshop proxy",
            WorkshopCheck::UpstreamState => "workshop upstream",
            WorkshopCheck::HookProgramsFound
            | WorkshopCheck::HookProgramMissing
            | WorkshopCheck::HookProgramWarning
            | WorkshopCheck::InventoryAbsent => "workshop hook programs",
            WorkshopCheck::SettingsExposed | WorkshopCheck::HookScriptCrlf => {
                "workshop project settings"
            }
            WorkshopCheck::BatteryLast | WorkshopCheck::BatteryStale => "workshop battery",
            WorkshopCheck::BatteryClockSkew => "workshop clock",
            WorkshopCheck::CredentialPresent
            | WorkshopCheck::CredentialAbsent
            | WorkshopCheck::CredentialState
            | WorkshopCheck::CredentialAction => "workshop credential",
            WorkshopCheck::ManagedSettingsPresent => "workshop managed settings",
            WorkshopCheck::MountReady
            | WorkshopCheck::MountIgnoredUntracked
            | WorkshopCheck::MountMissing
            | WorkshopCheck::MountNotIgnored
            | WorkshopCheck::MountTracked
            | WorkshopCheck::MountPathUnsupported
            | WorkshopCheck::MountPlaceholders => "workshop mounts",
        }
    }

    /// Where the finding sits in the section.
    pub fn order(self) -> usize {
        WorkshopCheck::ALL
            .iter()
            .position(|check| *check == self)
            .unwrap_or(WorkshopCheck::ALL.len())
    }
}

/// One image's batteries as doctor reads them (AD-117, AD-123): what a
/// battery would key on now, when the engine answered, and the recorded
/// batteries, newest first, with the newest one's clock skew.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatteryFacts {
    pub image: String,
    pub current: Option<super::battery::ValidityKey>,
    pub runs: Vec<super::battery::BatteryRun>,
    pub newest_skew_ms: Option<i64>,
}

/// An age in the largest whole unit: minutes, hours or days.
fn age(ms: i64) -> String {
    let minutes = ms.max(0) / 60_000;
    match minutes {
        m if m < 60 => format!("{m} min"),
        m if m < 48 * 60 => format!("{} h", m / 60),
        m => format!("{} days", m / (24 * 60)),
    }
}

/// Doctor's stop lines (AD-116, AD-123): both flags, each for what it
/// stops, and each kept run with the age of its stop and the time left
/// before its containers go.
pub fn stop_findings(
    relay_stopped: bool,
    workshop_stopped: bool,
    kept: &[(String, Option<i64>)],
    now_ms: i64,
) -> Vec<Finding> {
    let state = |stopped: bool| if stopped { "stopped" } else { "running" };
    let mut found = vec![Finding::new(
        WorkshopCheck::StopFlags,
        format!(
            "relay {}, workshop {}{}",
            state(relay_stopped),
            state(workshop_stopped),
            if workshop_stopped {
                "; resume from the window"
            } else {
                ""
            }
        ),
    )];
    for (id, stopped_at) in kept {
        let shown = id.get(..8).unwrap_or(id);
        let detail = match stopped_at {
            Some(at) => format!(
                "run {shown}: stopped {} ago; its containers hold the credential until removed, at the latest in {}",
                age(now_ms - at),
                age(at + super::run::KEPT_TTL_MS - now_ms)
            ),
            None => format!("run {shown}: stopped, time unknown"),
        };
        found.push(Finding::new(WorkshopCheck::KeptContainers, detail));
    }
    found
}

/// Doctor's battery lines (AD-117, AD-123, D-232, D-242): each image's
/// last outcome and age, `battery_stale` with the keys that moved since
/// the newest passing battery, and the engine's clock skew at the newest
/// battery of all.
pub fn battery_findings(images: &[BatteryFacts], now_ms: i64) -> Vec<Finding> {
    use super::battery::{BatteryOutcome, is_valid};
    let mut found = Vec::new();
    for facts in images {
        let newest = facts.runs.first();
        match newest {
            None => found.push(Finding::new(
                WorkshopCheck::BatteryStale,
                format!("{}: battery_stale: no escape battery has run", facts.image),
            )),
            Some(run) => found.push(Finding::new(
                WorkshopCheck::BatteryLast,
                format!(
                    "{}: {} {} ago",
                    facts.image,
                    run.outcome.map_or("unfinished", BatteryOutcome::code),
                    run.finished_at_ms
                        .map_or_else(|| "?".to_owned(), |at| age(now_ms - at))
                ),
            )),
        }
        let Some(current) = &facts.current else {
            continue;
        };
        if newest.is_none() || is_valid(&facts.runs, current, now_ms).is_some() {
            continue;
        }
        let passing = facts
            .runs
            .iter()
            .find(|run| run.outcome == Some(BatteryOutcome::Passed));
        let why = match passing {
            None => "no battery passed".to_owned(),
            Some(run) => {
                let moved = run.key.moved(current);
                if moved.is_empty() {
                    "the last pass is older than 30 days".to_owned()
                } else {
                    format!("since the last pass, the {} moved", moved.join(", "))
                }
            }
        };
        found.push(Finding::new(
            WorkshopCheck::BatteryStale,
            format!("{}: battery_stale: {why}", facts.image),
        ));
    }
    let skew = images
        .iter()
        .filter_map(|f| Some((f.runs.first()?.finished_at_ms?, f.newest_skew_ms?)))
        .max_by_key(|(at, _)| *at);
    if let Some((_, skew)) = skew {
        found.push(Finding::new(
            WorkshopCheck::BatteryClockSkew,
            format!("the engine's clock was {skew:+} ms from the window's at the last battery"),
        ));
    }
    found
}

/// What doctor read about one project's hooks (AD-123, story 6.9 AC 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookFacts {
    pub project: String,
    /// The inventory recorded for the project's image, if any.
    pub stored: Option<super::inventory::StoredInventory>,
    /// Each mounted settings file with the names it exposes.
    pub exposed: Vec<(String, Vec<String>)>,
    /// The mounted hook scripts with CRLF line endings.
    pub crlf: Vec<String>,
}

fn describe(command: &super::inventory::InventoryCommand) -> String {
    let what = match (&command.event, &command.server) {
        (Some(event), _) => format!("{event} hook"),
        (None, Some(server)) => format!("MCP server {server}"),
        (None, None) => "command".to_owned(),
    };
    format!("{what} `{}`", command.command)
}

/// The findings of one project's hooks (AD-118, AD-123): the recorded
/// inventory judged as a launch judges it, a guard's missing program a
/// failure, other misses and `dynamic` commands warnings; then the
/// exposed settings names and the CRLF scripts.
pub fn hook_findings(facts: &HookFacts) -> Vec<Finding> {
    use super::inventory::Resolution;
    let project = &facts.project;
    let mut found = Vec::new();
    match &facts.stored {
        None => found.push(Finding::new(
            WorkshopCheck::InventoryAbsent,
            format!("{project}: no inventory recorded for its image; Build images or the next launch runs it"),
        )),
        Some(stored) => {
            let judged = stored.consequences();
            let line = |index: usize| {
                let command = stored.commands.get(index).map(describe).unwrap_or_default();
                let result = stored.results.iter().find(|r| r.index == index);
                let how = match result.map(|r| (r.resolution, r.detail.as_deref())) {
                    Some((Resolution::Dynamic, _)) => "dynamic".to_owned(),
                    Some((_, Some(detail))) => format!("missing ({detail})"),
                    _ => "missing".to_owned(),
                };
                format!("{project}: {command}: {how}")
            };
            for index in &judged.refused {
                found.push(Finding::new(WorkshopCheck::HookProgramMissing, line(*index)));
            }
            for index in &judged.warnings {
                found.push(Finding::new(WorkshopCheck::HookProgramWarning, line(*index)));
            }
            if judged.refused.is_empty() && judged.warnings.is_empty() {
                found.push(Finding::new(
                    WorkshopCheck::HookProgramsFound,
                    format!(
                        "{project}: {} command(s) resolved in its image",
                        stored.commands.len()
                    ),
                ));
            }
        }
    }
    for (file, names) in &facts.exposed {
        if !names.is_empty() {
            found.push(Finding::new(
                WorkshopCheck::SettingsExposed,
                format!(
                    "{project}: {file} sets {} which the worker can read",
                    names.join(", ")
                ),
            ));
        }
    }
    for script in &facts.crlf {
        found.push(Finding::new(
            WorkshopCheck::HookScriptCrlf,
            format!("{project}: {script} has CRLF line endings; it runs on Linux in the workshop"),
        ));
    }
    found
}

/// One finding and what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub check: WorkshopCheck,
    pub detail: String,
}

impl Finding {
    pub fn new(check: WorkshopCheck, detail: impl Into<String>) -> Finding {
        Finding {
            check,
            detail: detail.into(),
        }
    }
}

/// The floor's credential line (AD-105, D-604, D-617): its kind, and
/// whether it is stored; for a token kind not stored, the floor's
/// `cmdkey` line, which the file `cmdkey_file` also holds. A `login`
/// floor's credential is its login volume, never a host value (D-603).
pub fn credential_finding(
    kind: super::floor::CredentialKind,
    present: bool,
    floor_key: &str,
    cmdkey_file: &str,
) -> Finding {
    use super::floor::CredentialKind;
    match (kind, present) {
        (CredentialKind::Login, _) => Finding::new(
            WorkshopCheck::CredentialPresent,
            "login: the worker's login is a volume; the host holds no value",
        ),
        (_, true) => Finding::new(
            WorkshopCheck::CredentialPresent,
            format!("{}: stored, never shown", kind.code()),
        ),
        (_, false) => Finding::new(
            WorkshopCheck::CredentialAbsent,
            format!(
                "{}: not stored; run `{}` (also in {cmdkey_file}); Officina never asks for it",
                kind.code(),
                super::names::cmdkey_line(floor_key)
            ),
        ),
    }
}

/// The floor's credential state line (D-607, story 6.26): `state`, as the
/// Workshop view shows it, and `action`, the one the view gives for it.
/// A token floor's `missing` is [`credential_finding`]'s line already, so
/// it gives none here. A `login` floor's command is also in `login_file`.
pub fn credential_state_finding(
    kind: super::floor::CredentialKind,
    state: super::launch::CredentialState,
    action: Option<&super::view::CredentialAction>,
    login_file: &str,
) -> Option<Finding> {
    use super::floor::CredentialKind;
    use super::launch::CredentialState;
    use super::view::CredentialAction;
    if kind != CredentialKind::Login && state == CredentialState::Missing {
        return None;
    }
    Some(match action {
        None => Finding::new(
            WorkshopCheck::CredentialState,
            format!("{}: {}", kind.code(), state.code()),
        ),
        Some(CredentialAction::Login { command }) => Finding::new(
            WorkshopCheck::CredentialAction,
            format!(
                "{}: {}: log in for the workshop from PowerShell or cmd, not Git Bash: `{command}` (also in {login_file}); open the URL it prints on this machine and paste the code; nothing retries",
                kind.code(),
                state.code()
            ),
        ),
        Some(CredentialAction::SetupToken { step, cmdkey_line }) => Finding::new(
            WorkshopCheck::CredentialAction,
            format!(
                "{}: {}: run `{step}`, then `{cmdkey_line}` and paste the new token; nothing retries",
                kind.code(),
                state.code()
            ),
        ),
    })
}

/// A managed-settings file or folder found at `path` (D-614): presence
/// only, never its content; workers do not apply it.
pub fn managed_settings_finding(path: &str) -> Finding {
    Finding::new(
        WorkshopCheck::ManagedSettingsPresent,
        format!(
            "{path} is present (not read): workshop workers do not apply it; a company account's rules are its holder's to check"
        ),
    )
}

/// What doctor found of one mount (FR-2, D-231, D-236).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountFacts {
    pub project: String,
    pub path: String,
    pub state: crate::orders::workshop_card::MountState,
    /// The rule's source when only an untracked rule ignores it.
    pub untracked_rule: Option<String>,
    /// How many Cloud Files placeholders it holds (R55).
    pub placeholders: u32,
}

/// One mount's lines.
pub fn mount_findings(mount: &MountFacts) -> Vec<Finding> {
    use crate::orders::workshop_card::MountState;
    let name = format!("{}: {}", mount.project, mount.path);
    let mut found = vec![match mount.state {
        MountState::Ready => match &mount.untracked_rule {
            Some(source) => Finding::new(
                WorkshopCheck::MountIgnoredUntracked,
                format!("{name} is ignored only by {source}, which the repository does not carry"),
            ),
            None => Finding::new(WorkshopCheck::MountReady, format!("{name} is ignored")),
        },
        MountState::Missing => {
            Finding::new(WorkshopCheck::MountMissing, format!("{name} is missing"))
        }
        MountState::NotIgnored => Finding::new(
            WorkshopCheck::MountNotIgnored,
            format!("{name} is not git-ignored"),
        ),
        MountState::Tracked => Finding::new(
            WorkshopCheck::MountTracked,
            format!("{name} holds files the base tracks"),
        ),
        MountState::PathUnsupported => Finding::new(
            WorkshopCheck::MountPathUnsupported,
            format!("{name}: a comma, a quote or a control character the engine cannot carry"),
        ),
        MountState::ReparsePoint => Finding::new(
            WorkshopCheck::MountPathUnsupported,
            format!("{name} is reached through a reparse point"),
        ),
    }];
    if mount.placeholders > 0 {
        found.push(Finding::new(
            WorkshopCheck::MountPlaceholders,
            format!(
                "{name} holds {} Cloud Files placeholder(s): make them available offline",
                mount.placeholders
            ),
        ));
    }
    found
}

/// The findings in section order, a check's several lines kept in the order
/// they came.
pub fn in_order(mut findings: Vec<Finding>) -> Vec<Finding> {
    findings.sort_by_key(|finding| finding.check.order());
    findings
}

/// One image doctor compares (AD-99, AD-123): its tag, what the running
/// Officina expects of it, what the engine and the registry hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageInput {
    pub tag: String,
    pub expected: super::image::Expected,
    pub inspected: Option<super::engine::ImageFacts>,
    pub recorded: Option<super::image::Recorded>,
}

/// The image's line: matching, not built, or the label or measurement that
/// differs, by [`super::image::check`], the launch's own rule.
pub fn image_finding(input: &ImageInput) -> Finding {
    use super::image::{ImageMismatch, check};
    match check(
        &input.expected,
        input.inspected.as_ref(),
        input.recorded.as_ref(),
    ) {
        Ok(id) => Finding::new(WorkshopCheck::ImageMatches, format!("{} {id}", input.tag)),
        Err(mismatch) if mismatch.cause == ImageMismatch::Absent => Finding::new(
            WorkshopCheck::ImageAbsent,
            format!("{} is not built: Build images builds it", input.tag),
        ),
        Err(mismatch) => {
            let what = mismatch
                .label
                .map_or_else(String::new, |label| format!(" ({label})"));
            Finding::new(
                WorkshopCheck::ImageMismatch,
                format!(
                    "{}: {}{what}; Build images rebuilds it",
                    input.tag,
                    mismatch.cause.code()
                ),
            )
        }
    }
}

/// A warning when the base image's Claude Code (its label) is not the
/// host's version (AD-99: not compared at launch, since Claude Code updates
/// itself on the host).
pub fn claude_version_finding(host: Option<&str>, image: Option<&str>) -> Option<Finding> {
    let (host, image) = (host?, image?);
    (host != image).then(|| {
        Finding::new(
            WorkshopCheck::ClaudeVersionDiffers,
            format!(
                "the base image holds Claude Code {image}, the host {host}: Build images updates it"
            ),
        )
    })
}

/// What doctor read of `ca_file` (D-208, D-612).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaRead {
    /// No `ca_file` is set.
    None,
    /// Read: its SHA-256.
    Sha256(String),
    /// Not readable, with why.
    Unreadable(String),
}

/// The CA lines (D-208): the label found on the base image; a CA file that
/// cannot be read or no longer hashes to the approved digest; a base label
/// that is not the floor's CA.
pub fn ca_findings(
    read: &CaRead,
    approved: Option<&str>,
    base_label: Option<&str>,
) -> Vec<Finding> {
    let mut found = Vec::new();
    if let Some(label) = base_label {
        found.push(Finding::new(
            WorkshopCheck::CaLabel,
            format!("the base image's CA label is {label}"),
        ));
    }
    let floor = match read {
        CaRead::None => super::image::NO_CA.to_owned(),
        CaRead::Unreadable(why) => {
            found.push(Finding::new(
                WorkshopCheck::CaUnreadable,
                format!("ca_file: {why}"),
            ));
            return found;
        }
        CaRead::Sha256(sha) => {
            if approved.is_some_and(|a| a != sha) {
                found.push(Finding::new(
                    WorkshopCheck::CaUnreadable,
                    "ca_file changed since its approval: approve the catalogue again",
                ));
            }
            sha.clone()
        }
    };
    if let Some(label) = base_label.filter(|label| *label != floor) {
        found.push(Finding::new(
            WorkshopCheck::CaLabelMismatch,
            format!(
                "the base image was built with CA {label}, the floor's is {floor}: one CA per engine (D-208); Build images rebuilds it"
            ),
        ));
    }
    found
}

/// A warning when `max_workers` workers of `memory_mib` each need more
/// than the engine's memory.
pub fn memory_finding(max_workers: u32, memory_mib: u32, engine_bytes: u64) -> Option<Finding> {
    let needed = u64::from(max_workers) * u64::from(memory_mib) * 1024 * 1024;
    (needed > engine_bytes).then(|| {
        Finding::new(
            WorkshopCheck::WorkersOverMemory,
            format!(
                "{max_workers} workers of {memory_mib} MiB need {} MiB, the engine has {} MiB",
                needed / (1024 * 1024),
                engine_bytes / (1024 * 1024)
            ),
        )
    })
}

/// The value of `name` in `env`, names compared ignoring ASCII case as
/// Windows does.
fn value<'e>(env: &'e [(String, String)], name: &str) -> Option<&'e str> {
    env.iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
        .filter(|v| !v.is_empty())
}

/// The names of `names` that `env` sets to a non-empty value, in `names`'
/// order, as doctor names them (never their values).
pub fn overrides_set(env: &[(String, String)], names: &[&str]) -> Option<Finding> {
    let set: Vec<&str> = names
        .iter()
        .copied()
        .filter(|name| value(env, name).is_some())
        .collect();
    (!set.is_empty()).then(|| {
        Finding::new(
            WorkshopCheck::OverridesSet,
            format!("set: {}", set.join(", ")),
        )
    })
}

/// A `DOCKER_HOST` on `tcp://` with neither `DOCKER_TLS_VERIFY` nor
/// `DOCKER_TLS` set: the engine's API in clear, which any process that can
/// reach the port may drive (review F40).
pub fn plain_tcp_docker_host(env: &[(String, String)]) -> Option<Finding> {
    let host = value(env, "DOCKER_HOST")?;
    let tls = value(env, "DOCKER_TLS_VERIFY").is_some() || value(env, "DOCKER_TLS").is_some();
    (host.to_ascii_lowercase().starts_with("tcp://") && !tls).then(|| {
        Finding::new(
            WorkshopCheck::PlainTcpDockerHost,
            "DOCKER_HOST is tcp:// without TLS: the engine's API is reachable in clear",
        )
    })
}

/// What the Docker CLI's `config.json` holds that changes what a `docker`
/// call does (D-228): read, never written; names only, never a proxy's
/// value.
pub fn docker_cli_config(text: &str) -> Vec<Finding> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return vec![Finding::new(
            WorkshopCheck::DockerCliConfigUnreadable,
            "config.json is not JSON",
        )];
    };
    let mut findings = Vec::new();
    if value
        .get("proxies")
        .and_then(serde_json::Value::as_object)
        .is_some_and(|proxies| !proxies.is_empty())
    {
        findings.push(Finding::new(
            WorkshopCheck::DockerCliProxies,
            "config.json has a proxies entry: the CLI adds its variables to every container it creates; the escape battery checks that none reaches a worker",
        ));
    }
    if let Some(context) = value
        .get("currentContext")
        .and_then(serde_json::Value::as_str)
        .filter(|c| !c.is_empty())
    {
        findings.push(Finding::new(
            WorkshopCheck::DockerCliContext,
            format!("current context {context}"),
        ));
    }
    let dirs: Vec<&str> = value
        .get("cliPluginsExtraDirs")
        .and_then(serde_json::Value::as_array)
        .map(|dirs| dirs.iter().filter_map(serde_json::Value::as_str).collect())
        .unwrap_or_default();
    if !dirs.is_empty() {
        findings.push(Finding::new(
            WorkshopCheck::DockerCliPluginDirs,
            format!(
                "cliPluginsExtraDirs adds {}: their programs may start with any docker call",
                dirs.join(", ")
            ),
        ));
    }
    findings
}

/// How many of `rows` (networks or volumes Officina labelled) belong to no
/// container `containers` lists: a run's object whose run has no container
/// left, or an object of no run. The floor's login volume is the author's,
/// never an orphan (D-605).
pub fn orphans(containers: &[PsRow], rows: &[ObjectRow]) -> usize {
    rows.iter()
        .filter(|row| row.labels.get(ROLE_LABEL).map(String::as_str) != Some(LOGIN_ROLE))
        .filter(|row| match row.labels.get(RUN_LABEL) {
            Some(run) => !containers
                .iter()
                .any(|c| c.labels.get(RUN_LABEL) == Some(run)),
            None => true,
        })
        .count()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn env(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn every_check_has_a_title_and_a_place() {
        for (i, check) in WorkshopCheck::ALL.iter().enumerate() {
            assert!(check.title().starts_with("workshop "), "{check:?}");
            assert_eq!(check.order(), i);
            let _ = check.level();
        }
    }

    #[test]
    fn findings_sort_into_the_declared_order() {
        let mut findings: Vec<Finding> = WorkshopCheck::ALL
            .iter()
            .rev()
            .map(|c| Finding::new(*c, c.code()))
            .collect();
        findings = in_order(findings);
        let order: Vec<WorkshopCheck> = findings.iter().map(|f| f.check).collect();
        assert_eq!(order, WorkshopCheck::ALL);
    }

    #[test]
    fn overrides_are_named_never_valued() {
        let names = ["DOCKER_HOST", "OFFICINA_DOCKER_BIN", "DOCKER_CONTEXT"];
        let finding = overrides_set(
            &env(&[
                ("docker_host", "npipe:////./pipe/secret-pipe"),
                ("OFFICINA_DOCKER_BIN", "C:/x/docker.exe"),
                ("DOCKER_CONTEXT", ""),
            ]),
            &names,
        )
        .unwrap();
        assert_eq!(finding.detail, "set: DOCKER_HOST, OFFICINA_DOCKER_BIN");
        assert!(overrides_set(&env(&[("PATH", "x")]), &names).is_none());
    }

    #[test]
    fn a_plain_tcp_host_warns_unless_tls_is_on() {
        assert!(plain_tcp_docker_host(&env(&[("DOCKER_HOST", "tcp://192.0.2.10:2375")])).is_some());
        assert!(plain_tcp_docker_host(&env(&[("DOCKER_HOST", "TCP://192.0.2.10:2375")])).is_some());
        assert!(
            plain_tcp_docker_host(&env(&[
                ("DOCKER_HOST", "tcp://192.0.2.10:2376"),
                ("DOCKER_TLS_VERIFY", "1")
            ]))
            .is_none()
        );
        assert!(
            plain_tcp_docker_host(&env(&[("DOCKER_HOST", "npipe:////./pipe/docker_engine")]))
                .is_none()
        );
        assert!(plain_tcp_docker_host(&env(&[])).is_none());
    }

    #[test]
    fn docker_cli_config_entries_are_named() {
        let findings = docker_cli_config(
            r#"{"proxies":{"default":{"httpsProxy":"http://proxy.internal.example.org:3128"}},"currentContext":"desktop-linux","cliPluginsExtraDirs":["C:/plugins"]}"#,
        );
        let checks: Vec<WorkshopCheck> = findings.iter().map(|f| f.check).collect();
        assert_eq!(
            checks,
            [
                WorkshopCheck::DockerCliProxies,
                WorkshopCheck::DockerCliContext,
                WorkshopCheck::DockerCliPluginDirs
            ]
        );
        assert!(
            findings
                .iter()
                .all(|f| !f.detail.contains("proxy.internal"))
        );
        assert!(docker_cli_config(r#"{"auths":{}}"#).is_empty());
        assert!(docker_cli_config(r#"{"proxies":{}}"#).is_empty());
        assert_eq!(
            docker_cli_config("not json")[0].check,
            WorkshopCheck::DockerCliConfigUnreadable
        );
    }

    fn labels(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn orphans_are_objects_whose_run_has_no_container() {
        let containers = [PsRow {
            id: "a".into(),
            name: "officina-worker-live".into(),
            state: "running".into(),
            labels: labels(&[(RUN_LABEL, "live")]),
        }];
        let row = |name: &str, l: &[(&str, &str)]| ObjectRow {
            name: name.into(),
            labels: labels(l),
        };
        let rows = [
            row("officina-int-live", &[(RUN_LABEL, "live")]),
            row("officina-int-gone", &[(RUN_LABEL, "gone")]),
            row("officina-login-x", &[(ROLE_LABEL, LOGIN_ROLE)]),
            row("stray", &[]),
        ];
        assert_eq!(orphans(&containers, &rows), 2);
    }
}
