//! The relay's part of `officina doctor` (FR-14, D-30 to D-33): the relay
//! floor and its catalogue, the approval state, the projects and their
//! actions, whether each action's program resolves, the warnings of D-32,
//! and per guarded project the `CLAUDE.md` paragraph that tells its sessions
//! how to use `officina run`, printed for the author to paste.
//!
//! It reads, probes and prints. It writes nothing, neither under the Claude
//! config dir nor in a project folder: the tree fingerprint runs `git`
//! without optional locks (D-22), and the WSL probe only tests paths.
//!
//! Levels (D-31): an action whose program does not resolve fails, and so
//! does a relay floor the relay cannot read while it is in use, as the
//! board's floor check does; catalogue issues, a pending change and every
//! warning of D-32 warn. The gate's registration is the hooks check of
//! [`crate::doctor`] (D-18). A workstation that does not use the relay gets
//! no relay line at all.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use officina_core::floor::{Autonomy, floor_dir_of, join_project_path};
use officina_core::quota::normalise_path;
use officina_core::relay::approvals::DEFAULT_WAIT_S;
use officina_core::relay::argv::{WSL_ENV_PROGRAM, WSL_EXE, WSL_REMOVED_ENV, WSL_TIMEOUT_PROGRAM};
use officina_core::relay::catalogue::{
    ActionClass, CanonicalAction, CanonicalCatalogue, CatalogueDiff, RelayFloor, Runner,
    SectionDiff, diff, relay_in_use,
};
use officina_core::relay::classes::{self, CHECK_VALID_MS, WAIT_RANGE_S};
use officina_core::relay::gate::{LIST_COMMAND, SHELL_TOOLS, ToolRule};
use officina_core::relay::journal::JournalEntry;
use officina_core::relay::run::{BASH_TOOL_MAX_TIMEOUT_MS, one_line};
use officina_core::relay::view::GATE_ERRORS_WINDOW_MS;

use super::home::{ApprovedState, RelayHome, load_relay_floor};
use super::{journal, tree};
use crate::doctor::{Check, Level, check};

/// The program the WSL probe runs inside the distro, by absolute path like
/// the tools it probes (D-6): `test -x <path>` exits 0 when the path is an
/// executable file, 1 otherwise.
pub const WSL_TEST_PROGRAM: &str = "/usr/bin/test";

/// Deadline of one WSL probe call (AD-6 style): long enough for a distro's
/// cold start, short enough that doctor never hangs.
pub const WSL_PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// What the relay part of doctor reads.
#[derive(Debug, Clone, Copy)]
pub struct RelayDoctor<'a> {
    pub home: &'a RelayHome,
    /// The floor doctor itself resolved (AD-7): `--floor`, `OFFICINA_FLOOR`,
    /// or the home's `floor.toml` when it exists.
    pub doctor_floor: Option<&'a Path>,
    /// The Windows system directory, where `wsl.exe` is taken from (AD-30);
    /// `None` off Windows or when it cannot be read.
    pub system_dir: Option<&'a Path>,
    pub now_ms: i64,
}

/// How one probe call ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeRun {
    /// The process exited, with its code when it has one.
    Exited(Option<i32>),
    TimedOut,
    NotStarted(String),
}

/// What probing one path in a distro told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolProbe {
    Executable,
    /// Missing, or not an executable file.
    Missing,
    /// The probe itself did not answer: the distro does not start, `wsl.exe`
    /// failed or timed out. Why, in words.
    Failed(String),
}

/// The command probing `path` in `distro` (the default one when `None`):
/// `wsl.exe [-d <distro>] --exec /usr/bin/test -x <path>`, never through a
/// shell, stdio null, no console window, the environment `officina run`
/// gives `wsl.exe` (D-6).
pub fn wsl_probe_command(wsl: &Path, distro: Option<&str>, path: &str) -> Command {
    let mut command = Command::new(wsl);
    if let Some(distro) = distro {
        command.args(["-d", distro]);
    }
    command
        .args(["--exec", WSL_TEST_PROGRAM, "-x", path])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for key in WSL_REMOVED_ENV {
        command.env_remove(key);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// Runs `command` to its end or `timeout`, whichever comes first; a process
/// still running at the deadline is killed.
pub fn run_with_deadline(command: &mut Command, timeout: Duration) -> ProbeRun {
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => return ProbeRun::NotStarted(e.to_string()),
    };
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return ProbeRun::Exited(status.code()),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return ProbeRun::TimedOut;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return ProbeRun::NotStarted(e.to_string());
            }
        }
    }
}

/// Probes each of `paths` in `distro` through `wsl` (the system directory's
/// `wsl.exe`, or whatever a test points it at), one call per path.
pub fn probe_wsl(wsl: &Path, distro: Option<&str>, paths: &[&str]) -> Vec<(String, ToolProbe)> {
    probe_wsl_with(wsl, distro, paths, |command| {
        run_with_deadline(command, WSL_PROBE_TIMEOUT)
    })
}

/// [`probe_wsl`] with `run` starting each command. Once a call fails (the
/// distro does not start, a timeout), the remaining paths are reported with
/// the same failure instead of being probed: one dead distro costs one
/// deadline, not one per path.
pub fn probe_wsl_with(
    wsl: &Path,
    distro: Option<&str>,
    paths: &[&str],
    mut run: impl FnMut(&mut Command) -> ProbeRun,
) -> Vec<(String, ToolProbe)> {
    let mut failed: Option<String> = None;
    paths
        .iter()
        .map(|path| {
            let probe = match &failed {
                Some(why) => ToolProbe::Failed(why.clone()),
                None => match run(&mut wsl_probe_command(wsl, distro, path)) {
                    ProbeRun::Exited(Some(0)) => ToolProbe::Executable,
                    ProbeRun::Exited(Some(1)) => ToolProbe::Missing,
                    other => {
                        let why = match other {
                            ProbeRun::Exited(Some(code)) => {
                                format!("wsl.exe exited {code}; is the distro installed?")
                            }
                            ProbeRun::Exited(None) => "wsl.exe ended without a code".to_owned(),
                            ProbeRun::TimedOut => format!(
                                "wsl.exe did not answer within {} s",
                                WSL_PROBE_TIMEOUT.as_secs()
                            ),
                            ProbeRun::NotStarted(e) => format!("wsl.exe could not start: {e}"),
                        };
                        failed = Some(why.clone());
                        ToolProbe::Failed(why)
                    }
                },
            };
            ((*path).to_owned(), probe)
        })
        .collect()
}

/// The relay floor as doctor found it.
enum FloorLoad {
    Missing,
    Broken(String),
    Valid(Box<RelayFloor>),
}

/// The relay's checks, in output order: floor, floor mismatch, catalogue and
/// issues, approval, projects, programs, the warnings of D-32, then one
/// `CLAUDE.md` paragraph per guarded project. Empty when the relay is not in
/// use: nothing approved, no known-projects record, and no floor asking
/// anything of the relay.
pub fn checks(inputs: &RelayDoctor<'_>) -> Vec<Check> {
    checks_with(inputs, &mut probe_wsl)
}

type WslProbe<'p> = dyn FnMut(&Path, Option<&str>, &[&str]) -> Vec<(String, ToolProbe)> + 'p;

fn checks_with(inputs: &RelayDoctor<'_>, probe: &mut WslProbe<'_>) -> Vec<Check> {
    let home = inputs.home;
    let approved = home.read_approved();
    let record = home.read_known_projects();
    let floor_path = home.relay_floor(&approved);
    let load = if floor_path.exists() {
        match load_relay_floor(&floor_path) {
            Ok(relay) => FloorLoad::Valid(Box::new(relay)),
            Err(e) => FloorLoad::Broken(e.to_string()),
        }
    } else {
        FloorLoad::Missing
    };
    let in_use = !matches!(approved, ApprovedState::Missing)
        || record.is_some()
        || matches!(&load, FloorLoad::Valid(relay) if relay_in_use(&relay.floor));
    if !in_use {
        return Vec::new();
    }
    let recorded_approval = record.is_some_and(|r| r.approved_hash.is_some());

    let mut checks = vec![floor_check(&floor_path, &load)];
    checks.extend(relative_floor_override(home));
    checks.extend(floor_mismatch(inputs, &floor_path));
    let current = match &load {
        FloorLoad::Valid(relay) => Some(relay.as_ref()),
        FloorLoad::Missing | FloorLoad::Broken(_) => None,
    };
    if let Some(relay) = current {
        checks.extend(catalogue_checks(relay));
    }
    checks.push(approval_check(
        home,
        &approved,
        recorded_approval,
        current,
        inputs.now_ms,
    ));
    if let Some(relay) = current {
        let dirs = project_dirs(relay, &floor_path);
        checks.extend(project_checks(&relay.canonical, &dirs));
        checks.extend(program_checks(
            &relay.canonical.actions,
            inputs.system_dir,
            probe,
        ));
        checks.extend(timeout_checks(&relay.canonical.actions));
        checks.extend(check_listing_checks(&relay.canonical));
        checks.extend(tree_checks(&relay.canonical, &dirs));
    }
    checks.push(journal_check(home, inputs.now_ms));
    let snapshot = approved.approved().map(|a| &a.canonical);
    checks.extend(paragraph_checks(current.map(|r| &r.canonical), snapshot));
    checks
}

fn floor_check(path: &Path, load: &FloorLoad) -> Check {
    const TITLE: &str = "relay floor";
    let shown = path.display();
    match load {
        FloorLoad::Valid(_) => check(Level::Ok, TITLE, shown.to_string()),
        FloorLoad::Broken(error) => check(
            Level::Fail,
            TITLE,
            format!(
                "{error}; until it reads again `officina run` refuses every action and the gate guards every project it knows"
            ),
        ),
        FloorLoad::Missing => check(
            Level::Fail,
            TITLE,
            format!(
                "{shown} does not exist; until it does `officina run` refuses every action and the gate guards every project it knows"
            ),
        ),
    }
}

/// Two paths name the same file: compared absolute, normalised (lowercase,
/// forward slashes, as Windows compares them).
fn same_path(a: &Path, b: &Path) -> bool {
    let normal = |path: &Path| {
        let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        normalise_path(&absolute.to_string_lossy())
    };
    normal(a) == normal(b)
}

/// A relative `OFFICINA_FLOOR` resolves against each process's working
/// directory: the window, the gate (in the session's folder) and `officina
/// run` would read different floors, and the window would approve one the
/// others never check (story 2.2).
fn relative_floor_override(home: &RelayHome) -> Option<Check> {
    let path = home.floor_override().filter(|path| !path.is_absolute())?;
    Some(check(
        Level::Warn,
        "relay floor override",
        format!(
            "OFFICINA_FLOOR is the relative path {}: each process resolves it from its own folder, so the window, the gate and `officina run` can read different floors; set an absolute path",
            path.display()
        ),
    ))
}

/// D-30: doctor cannot see the window's `--floor`; it compares the floor it
/// resolved itself (AD-7) with the relay floor (AD-24). Without a floor of
/// its own, doctor would read the home's `floor.toml`.
fn floor_mismatch(inputs: &RelayDoctor<'_>, relay_floor: &Path) -> Option<Check> {
    let default = inputs.home.default_floor();
    let doctor_floor = inputs.doctor_floor.unwrap_or(&default);
    (!same_path(doctor_floor, relay_floor)).then(|| {
        check(
            Level::Warn,
            "floor mismatch",
            format!(
                "doctor and a board started like it read {}, but the relay reads {} (OFFICINA_FLOOR, else the approved floor, else floor.toml in the Officina home): the window approves that one, and `officina run` and the gate check it",
                doctor_floor.display(),
                relay_floor.display()
            ),
        )
    })
}

fn catalogue_checks(relay: &RelayFloor) -> Vec<Check> {
    let catalogue = &relay.canonical;
    let source = if relay.floor.guarded_programs.is_some() {
        "the floor's list"
    } else {
        "the default list"
    };
    let mut checks = vec![check(
        Level::Ok,
        "relay catalogue",
        format!(
            "{} valid actions, {} projects; guarded programs ({source}): {}",
            catalogue.actions.len(),
            catalogue.projects.len(),
            catalogue.guarded_programs.join(", ")
        ),
    )];
    checks.extend(relay.catalogue.issues.iter().map(|issue| {
        let at = issue
            .line
            .map_or_else(String::new, |line| format!("line {line}, "));
        let kind = serde_json::to_value(issue.kind)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        check(
            Level::Warn,
            "relay issue",
            format!("{at}{}: {} ({kind})", issue.name, issue.message),
        )
    }));
    checks
}

fn section_parts(label: &str, section: &SectionDiff, parts: &mut Vec<String>) {
    if !section.added.is_empty() {
        parts.push(format!("{label} added: {}", section.added.join(", ")));
    }
    if !section.removed.is_empty() {
        parts.push(format!("{label} removed: {}", section.removed.join(", ")));
    }
    if !section.changed.is_empty() {
        let changed: Vec<String> = section
            .changed
            .iter()
            .map(|entry| {
                let fields: Vec<&str> = entry.fields.iter().map(|f| f.field.as_str()).collect();
                format!("{} ({})", entry.name, fields.join(", "))
            })
            .collect();
        parts.push(format!("{label} changed: {}", changed.join(", ")));
    }
}

/// What a pending change does, in one line; the window shows it in full.
fn change_summary(change: &CatalogueDiff) -> String {
    let mut parts = Vec::new();
    section_parts("actions", &change.actions, &mut parts);
    section_parts("projects", &change.projects, &mut parts);
    if !change.guarded_programs.added.is_empty() {
        parts.push(format!(
            "guarded programs added: {}",
            change.guarded_programs.added.join(", ")
        ));
    }
    if !change.guarded_programs.removed.is_empty() {
        parts.push(format!(
            "guarded programs removed: {}",
            change.guarded_programs.removed.join(", ")
        ));
    }
    if change.floor_path.is_some() {
        parts.push("the floor's path changed".to_owned());
    }
    if change.format.is_some() {
        parts.push("Officina's catalogue format changed".to_owned());
    }
    parts.join("; ")
}

fn ago(ms: i64) -> String {
    let minutes = ms.max(0) / 60_000;
    match minutes {
        0 => "under a minute".to_owned(),
        1..=119 => format!("{minutes} min"),
        _ if minutes < 48 * 60 => format!("{} h", minutes / 60),
        _ => format!("{} days", minutes / (24 * 60)),
    }
}

fn approval_check(
    home: &RelayHome,
    approved: &ApprovedState,
    recorded_approval: bool,
    current: Option<&RelayFloor>,
    now_ms: i64,
) -> Check {
    const TITLE: &str = "relay approval";
    const REFUSES: &str = "`officina run` refuses every action until then";
    match approved {
        ApprovedState::Valid(approved) => match current {
            Some(relay) if relay.hash != approved.hash => check(
                Level::Warn,
                TITLE,
                format!(
                    "a catalogue change awaits approval in the window ({}); {REFUSES}",
                    change_summary(&diff(&approved.canonical, &relay.canonical))
                ),
            ),
            _ => check(
                Level::Ok,
                TITLE,
                format!(
                    "catalogue {} approved {} ago",
                    approved.hash.get(..12).unwrap_or(&approved.hash),
                    ago(now_ms - approved.approved_at_ms)
                ),
            ),
        },
        ApprovedState::Unreadable(why) => check(
            Level::Warn,
            TITLE,
            format!(
                "the approved catalogue cannot be read ({why}): approve it again in the window; meanwhile the gate guards every project of the floor and {REFUSES}"
            ),
        ),
        ApprovedState::Missing if recorded_approval => check(
            Level::Warn,
            TITLE,
            format!(
                "{} is gone while {} records an approval: approve the catalogue again in the window; meanwhile the gate guards every project of the floor and {REFUSES}",
                home.approved_file().display(),
                home.known_projects_file().display()
            ),
        ),
        ApprovedState::Missing => check(
            Level::Warn,
            TITLE,
            format!("nothing approved yet: approve the catalogue in the window; {REFUSES}"),
        ),
    }
}

/// Each floor project's folder as the floor declares it (D-6), by name.
fn project_dirs(relay: &RelayFloor, floor_path: &Path) -> BTreeMap<String, String> {
    let floor_text = floor_path.to_string_lossy();
    relay
        .floor
        .projects
        .iter()
        .map(|p| {
            (
                p.name.clone(),
                join_project_path(floor_dir_of(&floor_text), &p.path),
            )
        })
        .collect()
}

fn autonomy_name(autonomy: Autonomy) -> &'static str {
    match autonomy {
        Autonomy::Free => "free",
        Autonomy::Guarded => "guarded",
    }
}

/// The guarded projects, and the free ones `officina run` serves (D-21),
/// with their valid actions.
fn project_checks(catalogue: &CanonicalCatalogue, dirs: &BTreeMap<String, String>) -> Vec<Check> {
    catalogue
        .projects
        .iter()
        .filter(|(_, p)| p.autonomy == Autonomy::Guarded || !p.actions.is_empty())
        .map(|(name, project)| {
            let actions = if project.actions.is_empty() {
                "no action".to_owned()
            } else {
                format!("actions {}", project.actions.join(", "))
            };
            let dir = dirs.get(name).unwrap_or(&project.path);
            check(
                Level::Ok,
                &format!("relay project {name}"),
                format!("{}, {actions} ({dir})", autonomy_name(project.autonomy)),
            )
        })
        .collect()
}

fn where_in(distro: Option<&str>) -> String {
    distro.map_or_else(
        || "the default WSL distro".to_owned(),
        |d| format!("WSL distro {d}"),
    )
}

fn tool_problem(path: &str, probe: Option<&ToolProbe>) -> Option<String> {
    match probe {
        Some(ToolProbe::Executable) => None,
        Some(ToolProbe::Missing) => Some(format!("{path} is missing or not executable")),
        Some(ToolProbe::Failed(why)) => Some(format!("{path} could not be checked: {why}")),
        None => Some(format!("{path} was not checked")),
    }
}

/// AC 2 of story 2.10 (FR-14, R14): a `native` program is an existing file;
/// for `wsl` actions the system directory's `wsl.exe` exists, and in each
/// distro `/usr/bin/env`, `/usr/bin/timeout` (D-6) and the programs are
/// executable. A program that does not resolve fails (D-31).
fn program_checks(
    actions: &BTreeMap<String, CanonicalAction>,
    system_dir: Option<&Path>,
    probe: &mut WslProbe<'_>,
) -> Vec<Check> {
    let mut checks = Vec::new();
    let mut by_distro: BTreeMap<Option<&str>, Vec<(&str, &str)>> = BTreeMap::new();
    for (name, action) in actions {
        match action.runner {
            Runner::Native => {
                let title = format!("relay program {name}");
                checks.push(if Path::new(&action.program).is_file() {
                    check(Level::Ok, &title, action.program.clone())
                } else {
                    check(
                        Level::Fail,
                        &title,
                        format!("{} is not a file: the action cannot start", action.program),
                    )
                });
            }
            Runner::Wsl => by_distro
                .entry(action.distro.as_deref())
                .or_default()
                .push((name.as_str(), action.program.as_str())),
        }
    }
    if by_distro.is_empty() {
        return checks;
    }
    let wsl = system_dir.map(|dir| dir.join(WSL_EXE));
    let Some(wsl) = wsl.as_ref().filter(|path| path.is_file()) else {
        let names: Vec<&str> = by_distro.values().flatten().map(|(n, _)| *n).collect();
        let missing = wsl.as_ref().map_or_else(
            || format!("no system directory to take {WSL_EXE} from"),
            |path| format!("{} does not exist", path.display()),
        );
        checks.push(check(
            Level::Fail,
            "relay wsl",
            format!(
                "{missing}: the wsl actions {} cannot start",
                names.join(", ")
            ),
        ));
        return checks;
    };
    for (distro, list) in by_distro {
        let mut paths = vec![WSL_ENV_PROGRAM, WSL_TIMEOUT_PROGRAM];
        for (_, program) in &list {
            if !paths.contains(program) {
                paths.push(*program);
            }
        }
        let found: BTreeMap<String, ToolProbe> = probe(wsl, distro, &paths).into_iter().collect();
        let place = where_in(distro);
        let title = format!("relay wsl {}", distro.unwrap_or("default"));
        let problems: Vec<String> = [WSL_ENV_PROGRAM, WSL_TIMEOUT_PROGRAM]
            .into_iter()
            .filter_map(|tool| tool_problem(tool, found.get(tool)))
            .collect();
        checks.push(if problems.is_empty() {
            check(
                Level::Ok,
                &title,
                format!("{WSL_ENV_PROGRAM} and {WSL_TIMEOUT_PROGRAM} are executable in {place}"),
            )
        } else {
            check(
                Level::Fail,
                &title,
                format!(
                    "{} in {place}: every wsl action runs through them",
                    problems.join("; ")
                ),
            )
        });
        for (name, program) in list {
            let title = format!("relay program {name}");
            checks.push(match tool_problem(program, found.get(program)) {
                None => check(
                    Level::Ok,
                    &title,
                    format!("{program} is executable in {place}"),
                ),
                Some(problem) => check(
                    Level::Fail,
                    &title,
                    format!("{problem} in {place}: the action cannot start"),
                ),
            });
        }
    }
    checks
}

/// D-32, P-6: a run whose own deadline is at or past the Bash tool's cap can
/// be killed by Claude Code first, and then reads abandoned in the journal;
/// a `human` action can also wait up to the longest `--wait` before it
/// starts. Officina's own start adds to the action's time, so reaching the
/// cap counts.
fn timeout_checks(actions: &BTreeMap<String, CanonicalAction>) -> Vec<Check> {
    let cap_s = BASH_TOOL_MAX_TIMEOUT_MS / 1000;
    let max_wait = u64::from(*WAIT_RANGE_S.end());
    actions
        .iter()
        .filter_map(|(name, action)| {
            let timeout = u64::from(action.timeout_s);
            let waits = action.class == ActionClass::Human;
            let detail = if timeout >= cap_s {
                format!(
                    "timeout_s {timeout} reaches the Bash tool's {cap_s} s cap: Claude Code can kill the call first, and the run then reads abandoned in the journal"
                )
            } else if waits && timeout + max_wait >= cap_s {
                format!(
                    "timeout_s {timeout} plus a --wait of up to {max_wait} s reaches the Bash tool's {cap_s} s cap: a call that waits more than {} s for the approval can be killed by Claude Code, and the run then reads abandoned",
                    cap_s - timeout - 1
                )
            } else {
                return None;
            };
            Some(check(Level::Warn, &format!("relay timeout {name}"), detail))
        })
        .collect()
}

/// An action naming a check (an `after-check` action, or a `human` action's
/// dry run, D-170) goes on only after that check ran in the same project, so
/// a project that lists it without its check can never run it.
fn check_listing_checks(catalogue: &CanonicalCatalogue) -> Vec<Check> {
    let mut checks = Vec::new();
    for (project_name, project) in &catalogue.projects {
        for action_name in &project.actions {
            let Some(action) = catalogue.actions.get(action_name) else {
                continue;
            };
            if let Some(check_name) = &action.check
                && !project.actions.contains(check_name)
            {
                checks.push(check(
                    Level::Warn,
                    &format!("relay check {project_name}"),
                    format!(
                        "project {project_name} lists {action_name} but not its check {check_name}: {action_name} runs only after {check_name} ran in the same project, so it can never run there"
                    ),
                ));
            }
        }
    }
    checks
}

/// D-22, D-170: the projects with an action naming a check, and whether
/// their code binds through a git fingerprint.
fn tree_checks(catalogue: &CanonicalCatalogue, dirs: &BTreeMap<String, String>) -> Vec<Check> {
    catalogue
        .projects
        .iter()
        .filter(|(_, project)| {
            project.actions.iter().any(|a| {
                catalogue
                    .actions
                    .get(a)
                    .is_some_and(|action| action.check.is_some())
            })
        })
        .map(|(name, project)| {
            let dir = dirs.get(name).unwrap_or(&project.path);
            let title = format!("relay tree {name}");
            match tree::fingerprint(Path::new(dir)) {
                Some(found) if found.untracked_by_size > 0 => check(
                    Level::Ok,
                    &title,
                    format!(
                        "{dir} is a git work tree: its actions naming a check bind its code ({} files over 1 MiB by size only)",
                        found.untracked_by_size
                    ),
                ),
                Some(_) => check(
                    Level::Ok,
                    &title,
                    format!("{dir} is a git work tree: its actions naming a check bind its code"),
                ),
                None => check(
                    Level::Warn,
                    &title,
                    format!(
                        "{dir} is not in a git work tree, or git cannot run there: its actions naming a check bind their parameters only, not the code their check ran on"
                    ),
                ),
            }
        })
        .collect()
}

/// D-32, AD-28, AD-32: unreadable journal lines in the two days read, and
/// gate errors in the last 24 hours, by cause.
fn journal_check(home: &RelayHome, now_ms: i64) -> Check {
    const TITLE: &str = "relay journal";
    let (lines, corrupt) = journal::read_recent(home, now_ms);
    let since = now_ms - GATE_ERRORS_WINDOW_MS;
    let mut causes: BTreeMap<String, u32> = BTreeMap::new();
    let mut last_detail = None;
    for line in lines.iter().filter(|line| line.ts >= since) {
        if let JournalEntry::GateError(error) = &line.entry {
            let cause = serde_json::to_value(error.cause)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default();
            *causes.entry(cause).or_default() += 1;
            last_detail = Some(error.detail.clone());
        }
    }
    let errors: u32 = causes.values().sum();
    if errors == 0 && corrupt == 0 {
        return check(
            Level::Ok,
            TITLE,
            "no gate error in the last 24 h, no unreadable line",
        );
    }
    let mut parts = Vec::new();
    if errors > 0 {
        let by_cause: Vec<String> = causes.iter().map(|(c, n)| format!("{c} x{n}")).collect();
        parts.push(format!(
            "{errors} gate errors in the last 24 h ({}), the last: {}",
            by_cause.join(", "),
            last_detail.unwrap_or_default()
        ));
    }
    if corrupt > 0 {
        parts.push(format!(
            "{corrupt} unreadable lines in the journal files of today and yesterday (UTC)"
        ));
    }
    check(
        Level::Warn,
        TITLE,
        format!(
            "{}; the Relay view and {} show the journal",
            parts.join("; "),
            home.journal_dir().display()
        ),
    )
}

/// One action of the paragraph: its call with every parameter, then what
/// the session should know before calling it.
fn paragraph_action(name: &str, action: &CanonicalAction) -> String {
    let mut call = format!("officina run {name}");
    for param in action.params.keys() {
        call.push_str(&format!(" {param}=<...>"));
    }
    let mut notes = Vec::new();
    // One line, so a description written over several keeps the list a list.
    let description = one_line(&action.description);
    let description = description.trim_end_matches('.');
    if !description.is_empty() {
        notes.push(format!("{description}."));
    }
    let minutes = CHECK_VALID_MS / 60_000;
    match (action.class, &action.check) {
        (ActionClass::AfterCheck, Some(check)) => notes.push(format!(
            "Runs only after `officina run {check}` succeeded with the same values on the same code in the last {minutes} minutes."
        )),
        (ActionClass::Human, check) => {
            if let Some(check) = check {
                notes.push(format!(
                    "Run its dry run `officina run {check}` with the same values first: the approval request is made only after it succeeded on the same code in the last {minutes} minutes."
                ));
            }
            notes.push(format!(
                "Runs once the author approves it in the Officina window; the call waits for that up to `--wait <s>` ({DEFAULT_WAIT_S} by default, at most {}), and a call that timed out is repeated as is to keep waiting.",
                WAIT_RANGE_S.end()
            ));
        }
        _ => {}
    }
    if !classes::is_available(action.class) {
        notes.push(format!(
            "Not available in this build (class {}).",
            classes::class_name(action.class)
        ));
    }
    if notes.is_empty() {
        format!("- `{call}`\n")
    } else {
        format!("- `{call}`: {}\n", notes.join(" "))
    }
}

/// The `CLAUDE.md` paragraph for a guarded project (FR-14, AD-31, D-33):
/// use `officina run` for the actions the approved catalogue gives the
/// project, `officina run --list` to see them, the Bash tool's longest
/// timeout, and the refusal's reason before any retry. With nothing
/// approved for the project it says the catalogue awaits approval and
/// lists no action. Generated, so it follows the catalogue; doctor prints
/// it and never writes it.
pub fn claude_md_paragraph(project: &str, approved: Option<&CanonicalCatalogue>) -> String {
    let mut text = format!(
        "Officina guards this repository (relay project {project}): a Bash command that starts a guarded program directly is refused. Run the approved relay actions instead, from this repository, as `officina run <action> name=value ...`"
    );
    let entry = approved.and_then(|catalogue| Some((catalogue, catalogue.projects.get(project)?)));
    match entry {
        Some((catalogue, entry)) if !entry.actions.is_empty() => {
            text.push_str(":\n\n");
            for name in &entry.actions {
                if let Some(action) = catalogue.actions.get(name) {
                    text.push_str(&paragraph_action(name, action));
                }
            }
        }
        Some(_) => text.push_str(".\n\nThe approved catalogue gives this project no action.\n"),
        None => text.push_str(
            ".\n\nThe relay's catalogue awaits the author's approval in the Officina window: until then no action runs.\n",
        ),
    }
    text.push_str(&format!(
        "\n`{LIST_COMMAND}` shows the actions and the values each parameter accepts. Give every `officina run` call a Bash timeout of {BASH_TOOL_MAX_TIMEOUT_MS} ms. A refused call prints `officina: refused: <why>` on stderr and runs nothing: read the reason before retrying, and do not retry the same call until what the reason names has changed.\n"
    ));
    text.push_str(&tools_sentence());
    text
}

/// Which tools to run commands with in a guarded repository, from
/// `SHELL_TOOLS` (D-171): the ones the gate reads, and the ones it refuses.
fn tools_sentence() -> String {
    let with = |wanted: ToolRule| -> Vec<&str> {
        SHELL_TOOLS
            .iter()
            .filter(|(_, rule)| *rule == wanted)
            .map(|(name, _)| *name)
            .collect()
    };
    let refused = with(ToolRule::DeniedInGuardedProject);
    if refused.is_empty() {
        return String::new();
    }
    format!(
        "Run commands in this repository with the {} tool: Officina refuses the {} tool here.\n",
        with(ToolRule::Lexed).join(" or "),
        refused.join(" or ")
    )
}

/// One paragraph per project the floor or the approved snapshot guards,
/// each generated from the approved snapshot (D-33).
fn paragraph_checks(
    floor: Option<&CanonicalCatalogue>,
    snapshot: Option<&CanonicalCatalogue>,
) -> Vec<Check> {
    let guarded: BTreeSet<&String> = [floor, snapshot]
        .into_iter()
        .flatten()
        .flat_map(|catalogue| {
            catalogue
                .projects
                .iter()
                .filter(|(_, p)| p.autonomy == Autonomy::Guarded)
                .map(|(name, _)| name)
        })
        .collect();
    guarded
        .into_iter()
        .map(|name| Check {
            level: Level::Ok,
            title: format!("CLAUDE.md {name}"),
            detail: "paste this paragraph into the project's CLAUDE.md (Officina never writes it):"
                .to_owned(),
            paste: Some(claude_md_paragraph(name, snapshot)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::home::system_dir;
    use officina_core::relay::catalogue::load;
    use std::path::PathBuf;

    const FLOOR_PATH: &str = "C:/path/to/officina.toml";

    fn wsl_floor(distro: Option<&str>) -> RelayFloor {
        let distro = distro.map_or_else(String::new, |d| format!("distro = \"{d}\"\n"));
        load(
            &format!(
                "schema = 2\n[[project]]\nname = \"lab\"\npath = \"Lab\"\nactions = [\"check\"]\n[[action]]\nname = \"check\"\nclass = \"free\"\nrunner = \"wsl\"\n{distro}program = \"/opt/venv/bin/ansible-playbook\"\n"
            ),
            FLOOR_PATH,
        )
        .unwrap()
    }

    fn tools(program: &str) -> [String; 3] {
        [
            WSL_ENV_PROGRAM.to_owned(),
            WSL_TIMEOUT_PROGRAM.to_owned(),
            program.to_owned(),
        ]
    }

    fn summary(checks: &[Check]) -> Vec<(String, Level)> {
        checks.iter().map(|c| (c.title.clone(), c.level)).collect()
    }

    #[test]
    fn wsl_probe_uses_exec_and_the_distro() {
        let wsl = Path::new(r"C:\path\to\System32\wsl.exe");
        let paths = tools("/opt/venv/bin/ansible-playbook");
        let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
        for distro in [Some("Debian"), None] {
            let mut seen = Vec::new();
            let found = probe_wsl_with(wsl, distro, &paths, |command| {
                let args: Vec<String> = command
                    .get_args()
                    .map(|a| a.to_string_lossy().into_owned())
                    .collect();
                let envs: Vec<(String, bool)> = command
                    .get_envs()
                    .map(|(k, v)| (k.to_string_lossy().into_owned(), v.is_some()))
                    .collect();
                seen.push((PathBuf::from(command.get_program()), args, envs));
                ProbeRun::Exited(Some(0))
            });
            assert!(found.iter().all(|(_, p)| *p == ToolProbe::Executable));
            assert_eq!(seen.len(), paths.len(), "one call per path");
            for ((program, args, envs), path) in seen.iter().zip(&paths) {
                assert_eq!(program, wsl, "the wsl.exe it was given, never PATH's");
                let mut expected: Vec<String> = distro
                    .map(|d| vec!["-d".to_owned(), d.to_owned()])
                    .unwrap_or_default();
                expected.extend(
                    ["--exec", WSL_TEST_PROGRAM, "-x", path]
                        .iter()
                        .map(|a| (*a).to_owned()),
                );
                assert_eq!(args, &expected, "{distro:?}");
                for removed in WSL_REMOVED_ENV {
                    assert!(envs.contains(&((*removed).to_owned(), false)), "{envs:?}");
                }
            }
        }
    }

    #[test]
    fn wsl_probe_reports_each_missing_tool() {
        let relay = wsl_floor(Some("Debian"));
        let program = "/opt/venv/bin/ansible-playbook";
        let system = tempfile::tempdir().unwrap();
        std::fs::write(system.path().join(WSL_EXE), b"").unwrap();
        for missing in tools(program) {
            let mut probe = |wsl: &Path, distro: Option<&str>, paths: &[&str]| {
                probe_wsl_with(wsl, distro, paths, |command| {
                    let probed = command.get_args().last().unwrap().to_owned();
                    ProbeRun::Exited(Some(i32::from(probed == missing.as_str())))
                })
            };
            let checks = program_checks(&relay.canonical.actions, Some(system.path()), &mut probe);
            let failed: Vec<&Check> = checks.iter().filter(|c| c.level == Level::Fail).collect();
            assert_eq!(failed.len(), 1, "{missing}: {checks:?}");
            assert!(
                failed[0].detail.contains(&format!("{missing} is missing")),
                "{failed:?}"
            );
            assert!(failed[0].detail.contains("WSL distro Debian"), "{failed:?}");
        }

        let mut dead = |wsl: &Path, distro: Option<&str>, paths: &[&str]| {
            probe_wsl_with(wsl, distro, paths, |_| ProbeRun::Exited(Some(-1)))
        };
        let checks = program_checks(&relay.canonical.actions, Some(system.path()), &mut dead);
        assert_eq!(
            summary(&checks),
            [
                ("relay wsl Debian".to_owned(), Level::Fail),
                ("relay program check".to_owned(), Level::Fail),
            ]
        );
        assert!(checks[0].detail.contains("is the distro installed"));
    }

    #[test]
    fn a_failed_probe_stops_probing_the_distro() {
        let mut calls = 0;
        let found = probe_wsl_with(
            Path::new("wsl.exe"),
            None,
            &[WSL_ENV_PROGRAM, WSL_TIMEOUT_PROGRAM, "/bin/true"],
            |_| {
                calls += 1;
                ProbeRun::TimedOut
            },
        );
        assert_eq!(calls, 1);
        assert!(
            found.iter().all(
                |(_, p)| matches!(p, ToolProbe::Failed(why) if why.contains("did not answer"))
            )
        );
    }

    #[test]
    fn without_wsl_exe_every_wsl_action_fails() {
        let relay = wsl_floor(None);
        let empty = tempfile::tempdir().unwrap();
        let mut never = |_: &Path, _: Option<&str>, _: &[&str]| -> Vec<(String, ToolProbe)> {
            panic!("no probe without wsl.exe")
        };
        for system_dir in [Some(empty.path()), None] {
            let checks = program_checks(&relay.canonical.actions, system_dir, &mut never);
            assert_eq!(summary(&checks), [("relay wsl".to_owned(), Level::Fail)]);
            assert!(checks[0].detail.contains("check"), "{:?}", checks[0]);
        }
    }

    #[test]
    fn probe_runs_report_exit_codes_and_deadlines() {
        let (shell, flag) = if cfg!(windows) {
            ("cmd.exe", "/c")
        } else {
            ("sh", "-c")
        };
        let mut exits = Command::new(shell);
        exits.args([flag, "exit 3"]);
        assert_eq!(
            run_with_deadline(&mut exits, Duration::from_secs(10)),
            ProbeRun::Exited(Some(3))
        );
        let mut sleeps = Command::new(shell);
        sleeps.arg(flag).arg(if cfg!(windows) {
            "ping -n 6 127.0.0.1 >nul"
        } else {
            "sleep 5"
        });
        let started = Instant::now();
        assert_eq!(
            run_with_deadline(&mut sleeps, Duration::from_millis(200)),
            ProbeRun::TimedOut
        );
        assert!(started.elapsed() < Duration::from_secs(4));
        let mut absent = Command::new("officina-no-such-program-for-the-probe");
        assert!(matches!(
            run_with_deadline(&mut absent, Duration::from_secs(1)),
            ProbeRun::NotStarted(_)
        ));
    }

    /// The real probe on this machine's first distro, when one is installed:
    /// the tools of D-6 are found, a missing path is reported missing, and a
    /// distro that does not exist fails the probe.
    #[cfg(windows)]
    #[test]
    fn wsl_probe_on_an_installed_distro() {
        let Some(wsl) = system_dir().map(|dir| dir.join(WSL_EXE)) else {
            println!("skipped: no system directory");
            return;
        };
        let listed = Command::new(&wsl)
            .args(["-l", "-q"])
            .stdin(Stdio::null())
            .output();
        let distro = listed
            .ok()
            .filter(|out| out.status.success())
            .and_then(|out| {
                let units: Vec<u16> = out
                    .stdout
                    .chunks_exact(2)
                    .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                    .collect();
                String::from_utf16_lossy(&units)
                    .lines()
                    .map(|l| {
                        l.trim_matches(|c: char| c == '\0' || c.is_whitespace())
                            .to_owned()
                    })
                    .find(|name| !name.is_empty() && !name.starts_with("docker-desktop"))
            });
        let Some(distro) = distro else {
            println!("skipped: no WSL distro is installed");
            return;
        };
        let missing = "/nonexistent/officina-doctor-probe";
        let found = probe_wsl(
            &wsl,
            Some(&distro),
            &[WSL_ENV_PROGRAM, WSL_TIMEOUT_PROGRAM, missing],
        );
        assert_eq!(
            found,
            [
                (WSL_ENV_PROGRAM.to_owned(), ToolProbe::Executable),
                (WSL_TIMEOUT_PROGRAM.to_owned(), ToolProbe::Executable),
                (missing.to_owned(), ToolProbe::Missing),
            ]
        );
        let none = probe_wsl(&wsl, Some("officina-no-such-distro"), &[WSL_ENV_PROGRAM]);
        assert!(matches!(none[0].1, ToolProbe::Failed(_)), "{none:?}");
    }

    #[test]
    fn timeouts_reaching_the_bash_cap_warn() {
        let floor = |class: &str, timeout: Option<u32>| {
            let timeout = timeout.map_or_else(String::new, |t| format!("timeout_s = {t}\n"));
            load(
                &format!(
                    "schema = 2\n[[project]]\nname = \"lab\"\npath = \"Lab\"\nactions = [\"a\"]\n[[action]]\nname = \"a\"\nclass = \"{class}\"\nprogram = \"C:/path/to/tool.exe\"\n{timeout}"
                ),
                FLOOR_PATH,
            )
            .unwrap()
        };
        let cap_s = u32::try_from(BASH_TOOL_MAX_TIMEOUT_MS / 1000).unwrap();
        let max_wait = *WAIT_RANGE_S.end();
        for (class, timeout, warns) in [
            ("free", None, false),
            ("free", Some(cap_s - 1), false),
            ("free", Some(cap_s), true),
            ("human", Some(cap_s - max_wait - 1), false),
            ("human", Some(cap_s - max_wait), true),
            ("human", None, true),
        ] {
            let relay = floor(class, timeout);
            let checks = timeout_checks(&relay.canonical.actions);
            assert_eq!(checks.len(), usize::from(warns), "{class} {timeout:?}");
        }
    }

    #[test]
    fn paragraph_without_the_project_approved_lists_no_action() {
        let relay = wsl_floor(None);
        for approved in [None, Some(&relay.canonical)] {
            let project = if approved.is_some() { "other" } else { "lab" };
            let text = claude_md_paragraph(project, approved);
            assert!(text.contains("awaits the author's approval"), "{text}");
            assert!(!text.contains("officina run check"), "{text}");
            assert!(text.contains(LIST_COMMAND));
            assert!(text.contains(&format!("{BASH_TOOL_MAX_TIMEOUT_MS} ms")));
        }
        let text = claude_md_paragraph("lab", Some(&relay.canonical));
        assert!(text.contains("- `officina run check`\n"), "{text}");
    }

    /// Adversarial pass: a description written over several lines stays on
    /// its action's line, so the pasted list stays a list.
    #[test]
    fn a_multiline_description_stays_on_its_line() {
        let relay = load(
            "schema = 2\n[[project]]\nname = \"lab\"\npath = \"Lab\"\nactions = [\"a\", \"b\"]\n[[action]]\nname = \"a\"\ndescription = \"\"\"First line\n\n- not an action\"\"\"\nclass = \"free\"\nprogram = \"C:/path/to/tool.exe\"\n[[action]]\nname = \"b\"\nclass = \"free\"\nprogram = \"C:/path/to/tool.exe\"\n",
            FLOOR_PATH,
        )
        .unwrap();
        let text = claude_md_paragraph("lab", Some(&relay.canonical));
        let items: Vec<&str> = text.lines().filter(|l| l.starts_with("- ")).collect();
        assert_eq!(
            items,
            [
                "- `officina run a`: First line - not an action.",
                "- `officina run b`",
            ],
            "{text}"
        );
    }
}
