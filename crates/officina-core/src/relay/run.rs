//! `officina run` as pure functions (AD-29, AD-33, D-21, D-170): the checks
//! in their documented order, failing closed, and the text of
//! `officina run --list`. The host reads the stop flag, the files, the
//! working directory and the system directory, then, for an action naming a
//! check, the journal and the tree for [`check_step`]; for a `human` action
//! it goes through `relay::approvals`; it journals and spawns what [`plan`]
//! returns.

use std::collections::BTreeMap;

use super::approvals;
use super::argv::{self, ArgvRefusal, Launch};
use super::catalogue::{
    ActionClass, ApprovedCatalogue, CanonicalAction, CanonicalParam, CanonicalProject, RelayFloor,
};
use super::classes::{self, CHECK_VALID_MS};
use super::gate::{LIST_COMMAND, normalise_cwd};
use super::http::{self, HttpPlan};
use super::journal::{JournalLine, RefusalReason, TreeFingerprint};
use crate::floor::{floor_dir_of, join_project_path};

/// Exit code of a refusal (AD-29): a shell's "found but not executable".
pub const EXIT_REFUSED: i32 = 126;
/// Exit code when Officina ended the action at its timeout.
pub const EXIT_TIMEOUT: i32 = 124;
/// Exit code when the program could not start.
pub const EXIT_NOT_STARTED: i32 = 127;
/// Exit code of a usage error.
pub const EXIT_USAGE: i32 = 64;

/// The refusals of the file binding (D-170, D-173): no `git` to list a work
/// tree, a tree past the fingerprint's limits, a file it cannot read, and
/// the files changed since the dry run or the request. Each leaves an
/// approval unconsumed; the README names this constant beside the binding.
pub const TREE_BINDING_REFUSALS: &[RefusalReason] = &[
    RefusalReason::GitUnavailable,
    RefusalReason::TreeTooLarge,
    RefusalReason::TreeUnreadable,
    RefusalReason::CheckNotSatisfied,
    RefusalReason::TreeChanged,
];

/// The longest timeout Claude Code's Bash tool accepts (AD-31): a relay call
/// still running then is killed by Claude Code, and its run reads abandoned.
/// The `CLAUDE.md` paragraph doctor prints tells sessions to pass it.
pub const BASH_TOOL_MAX_TIMEOUT_MS: u64 = 600_000;

/// `relay/approved.json` as `officina run` reads it.
#[derive(Debug, Clone, Copy)]
pub enum SnapshotState<'a> {
    Missing,
    /// Why it cannot be read.
    Unreadable(&'a str),
    Valid(&'a ApprovedCatalogue),
}

/// The relay floor as `officina run` reads it: both phases.
#[derive(Debug, Clone, Copy)]
pub enum FloorState<'a> {
    /// Why it cannot be read or fails phase 1.
    Unreadable(&'a str),
    /// `path` is the floor's path as given, for its relative project paths.
    Valid {
        relay: &'a RelayFloor,
        path: &'a str,
    },
}

/// What `officina run` knows before checking anything.
#[derive(Debug, Clone, Copy)]
pub struct RunInputs<'a> {
    /// `relay/stopped` (AD-34).
    pub stopped: bool,
    pub snapshot: SnapshotState<'a>,
    pub floor: FloorState<'a>,
    /// The process's working directory.
    pub cwd: &'a str,
    /// The Windows system directory, where `wsl.exe` is taken from (AD-30).
    pub system_dir: &'a str,
}

/// Why `officina run` refused, as the stderr line and the `refused` journal
/// line carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub reason: RefusalReason,
    /// One sentence for the session, never echoing a refused value.
    pub detail: String,
    /// The project, once known.
    pub project: Option<String>,
}

/// What passed every check: the exact launch and what the journal records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunPlan {
    pub project: String,
    pub action: String,
    pub class: ActionClass,
    /// What runs: a process or an HTTP request.
    pub target: Target,
    /// The project folder as the floor declares it (D-6): a process's
    /// working directory, and the tree a check binds.
    pub project_dir: String,
    /// The approved hash, equal to the current one.
    pub catalogue_hash: String,
    pub timeout_s: u32,
    pub max_output_kib: u32,
    /// The check it needs: an `after-check` action's (AD-33), or a `human`
    /// action's dry run (D-170).
    pub check: Option<String>,
    /// True for an action naming a check, any action named as a check, and
    /// every `human` action: their runs record the tree, and a `human`
    /// action's request is bound to it (D-22, D-173).
    pub binds_tree: bool,
    /// True for an action named as a check: its run keeps its output for the
    /// approval card (D-170).
    pub keeps_output: bool,
}

/// What a plan runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A program, as an argument vector (AD-30).
    Process(Launch),
    /// An HTTP request (AD-36).
    Http(HttpPlan),
}

impl RunPlan {
    /// The program, then its arguments: what the journal records as `argv`;
    /// empty for an HTTP action, which records its request instead.
    pub fn argv(&self) -> Vec<String> {
        match &self.target {
            Target::Process(launch) => std::iter::once(launch.program.clone())
                .chain(launch.args.iter().cloned())
                .collect(),
            Target::Http(_) => Vec::new(),
        }
    }

    /// The process to start, for a process action.
    pub fn launch(&self) -> Option<&Launch> {
        match &self.target {
            Target::Process(launch) => Some(launch),
            Target::Http(_) => None,
        }
    }

    /// The request to send, for an HTTP action.
    pub fn http(&self) -> Option<&HttpPlan> {
        match &self.target {
            Target::Http(plan) => Some(plan),
            Target::Process(_) => None,
        }
    }
}

fn refuse<T>(reason: RefusalReason, detail: String, project: Option<&str>) -> Result<T, Refusal> {
    Err(Refusal::new(reason, &detail, project))
}

impl Refusal {
    fn new(reason: RefusalReason, detail: &str, project: Option<&str>) -> Refusal {
        Refusal {
            reason,
            detail: one_line(detail),
            project: project.map(str::to_owned),
        }
    }

    /// The relay is stopped: at the first step, or during a wait (AD-34).
    pub fn stopped() -> Refusal {
        Refusal::new(
            RefusalReason::Stopped,
            "the relay is stopped in the Officina window; the author resumes it there",
            None,
        )
    }

    /// The author rejected the request in the last 15 minutes (AD-33).
    pub fn approval_rejected(project: &str) -> Refusal {
        Refusal::new(
            RefusalReason::ApprovalRejected,
            "rejected in Officina; do not retry",
            Some(project),
        )
    }

    /// No decision within `--wait` (AD-33).
    pub fn approval_pending(project: &str) -> Refusal {
        Refusal::new(
            RefusalReason::ApprovalPending,
            "approval still pending; run the same command again to keep waiting",
            Some(project),
        )
    }

    /// The author approved another catalogue since the request was made: its
    /// approval no longer applies (AD-24, AD-33).
    pub fn catalogue_changed_since_request(project: &str) -> Refusal {
        Refusal::new(
            RefusalReason::CatalogueChanged,
            "the catalogue changed since this approval was requested; run the same command again for a new request",
            Some(project),
        )
    }

    /// An approval file for request `id` that the window's record of its
    /// decisions does not name: another process wrote it (D-173).
    pub fn approval_not_given(project: &str, id: &str) -> Refusal {
        Refusal::new(
            RefusalReason::ApprovalNotGiven,
            &format!(
                "the approval {id} was not given in the Officina window, so it is not used; the author decides the request there"
            ),
            Some(project),
        )
    }

    /// The request could not be written.
    pub fn approval_unavailable(project: &str, why: &str) -> Refusal {
        Refusal::new(
            RefusalReason::ApprovalUnavailable,
            &format!("the approval request cannot be written ({why})"),
            Some(project),
        )
    }

    /// The credential an HTTP action names is not in the Credential
    /// Manager: nothing was sent, and an approval stays (D-29).
    pub fn credential_missing(project: &str, name: &str) -> Refusal {
        Refusal::new(
            RefusalReason::CredentialMissing,
            &format!(
                "the credential {name} is not in the Windows Credential Manager; the author stores it with the line `officina doctor` prints"
            ),
            Some(project),
        )
    }

    /// The credential cannot be read; `why` never holds the secret.
    pub fn credential_unreadable(project: &str, name: &str, why: &str) -> Refusal {
        Refusal::new(
            RefusalReason::CredentialUnreadable,
            &format!("the credential {name} cannot be read ({why})"),
            Some(project),
        )
    }

    /// The project is a git work tree and no `git` is installed where Git
    /// for Windows records it, so its files cannot be bound (D-173).
    pub fn git_unavailable(project: &str, why: &str) -> Refusal {
        Refusal::new(
            RefusalReason::GitUnavailable,
            &format!(
                "the project is a git work tree and {why}, so its files cannot be bound; the author checks it with `officina doctor`"
            ),
            Some(project),
        )
    }

    /// The work tree is past a limit of the fingerprint (D-173).
    pub fn tree_too_large(project: &str, why: &str) -> Refusal {
        Refusal::new(
            RefusalReason::TreeTooLarge,
            &format!("the project's work tree is too large to bind: {why}"),
            Some(project),
        )
    }

    /// A file of the work tree cannot be read (D-173).
    pub fn tree_unreadable(project: &str, why: &str) -> Refusal {
        Refusal::new(
            RefusalReason::TreeUnreadable,
            &format!(
                "a file of the project's work tree cannot be read, so its content cannot be bound: {why}"
            ),
            Some(project),
        )
    }

    /// The tracked and unignored files changed since the request of a
    /// `human` action naming no check; the approval stays for a call on the
    /// files it was asked for (D-173).
    pub fn tree_changed(project: &str) -> Refusal {
        Refusal::new(
            RefusalReason::TreeChanged,
            "the project's tracked and unignored files changed since this approval was requested; run the same command again for a new request",
            Some(project),
        )
    }

    /// The code changed since the dry run the approval was given for; the
    /// approval stays for a later call on the same code (D-170).
    pub fn check_not_satisfied(project: &str, check: &str) -> Refusal {
        Refusal::new(
            RefusalReason::CheckNotSatisfied,
            &format!(
                "the code changed since the `{check}` dry run this approval was given for; run `officina run {check}` again for a new request"
            ),
            Some(project),
        )
    }
}

/// `text` on one line: a refusal is one stderr line (AD-29), and a quoted
/// error (a TOML parse error, a file name) can span several.
pub fn one_line(text: &str) -> String {
    text.split(['\r', '\n'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The approved projects `cwd` lies in, by longest path prefix on segment
/// boundaries, free and guarded alike (D-21); several only when two
/// projects share a path. Sorted by name.
pub fn projects_at<'c>(
    cwd: &str,
    approved: &'c ApprovedCatalogue,
) -> Vec<(&'c str, &'c CanonicalProject)> {
    let cwd = normalise_cwd(cwd);
    let containing: Vec<(usize, &str, &CanonicalProject)> = approved
        .canonical
        .projects
        .iter()
        .filter_map(|(name, project)| {
            let root = normalise_cwd(&project.path);
            let inside = !root.is_empty()
                && (cwd == root || cwd.starts_with(&root) && cwd[root.len()..].starts_with('/'));
            inside.then_some((root.len(), name.as_str(), project))
        })
        .collect();
    let Some(longest) = containing.iter().map(|(len, _, _)| *len).max() else {
        return Vec::new();
    };
    containing
        .into_iter()
        .filter(|(len, _, _)| *len == longest)
        .map(|(_, name, project)| (name, project))
        .collect()
}

/// The checks of `officina run <action> name=value ...` in AD-29's order,
/// failing closed; the first that fails is the refusal:
///
/// 1. the relay is not stopped (AD-34);
/// 2. a catalogue is approved, the floor reads, and its hash equals the
///    approved one;
/// 3. the working directory lies in an approved project, `free` or
///    `guarded` (D-21);
/// 4. that project lists the action;
/// 5. every declared parameter is given, none other, each value accepted
///    (AD-30, D-5);
/// 6. an action naming a check then needs [`check_step`], which reads the
///    journal and the tree; a `human` action then needs the author's
///    approval (`relay::approvals`).
///
/// The launch is built from the current floor's validated action, which
/// the equal hash proves is the approved one, with the project folder as
/// the floor declares it (D-6).
pub fn plan(
    inputs: &RunInputs<'_>,
    action: &str,
    params: &BTreeMap<String, String>,
) -> Result<RunPlan, Refusal> {
    if inputs.stopped {
        return Err(Refusal::stopped());
    }
    let approved = match inputs.snapshot {
        SnapshotState::Missing => {
            return refuse(
                RefusalReason::NotApproved,
                "no relay catalogue is approved yet; the author approves it in the Officina window"
                    .to_owned(),
                None,
            );
        }
        SnapshotState::Unreadable(why) => {
            return refuse(
                RefusalReason::ApprovedUnreadable,
                format!(
                    "the approved catalogue cannot be read ({why}); the author approves it again in the Officina window"
                ),
                None,
            );
        }
        SnapshotState::Valid(approved) => approved,
    };
    let (relay, floor_path) = match inputs.floor {
        FloorState::Unreadable(why) => {
            return refuse(
                RefusalReason::FloorUnreadable,
                format!("the relay floor cannot be read ({why})"),
                None,
            );
        }
        FloorState::Valid { relay, path } => (relay, path),
    };
    if relay.hash != approved.hash {
        return refuse(
            RefusalReason::CatalogueChanged,
            "catalogue changed, approval pending in Officina".to_owned(),
            None,
        );
    }

    let candidates = projects_at(inputs.cwd, approved);
    let Some((first, _)) = candidates.first() else {
        return refuse(
            RefusalReason::OutsideProjects,
            format!(
                "the folder {} lies in no project of the approved catalogue",
                inputs.cwd
            ),
            None,
        );
    };
    let Some((project, _)) = candidates
        .iter()
        .find(|(_, p)| p.actions.iter().any(|a| a == action))
    else {
        return refuse(
            RefusalReason::ActionNotAllowed,
            format!(
                "the project {first} has no action {action}; run `{LIST_COMMAND}` to see its actions"
            ),
            Some(first),
        );
    };
    let Some(validated) = relay.catalogue.actions.get(action) else {
        return refuse(
            RefusalReason::ActionNotAllowed,
            format!("the action {action} is not in the catalogue"),
            Some(project),
        );
    };

    let project_dir = relay
        .floor
        .projects
        .iter()
        .find(|p| p.name == *project)
        .map(|p| join_project_path(floor_dir_of(floor_path), &p.path))
        .unwrap_or_else(|| approved.canonical.projects[*project].path.clone());
    let target = match http::build(validated, params, &relay.catalogue.credentials) {
        Some(request) => request.map(Target::Http),
        None => {
            argv::build(validated, params, &project_dir, inputs.system_dir).map(Target::Process)
        }
    };
    let target = target.map_err(|refusal| {
        let reason = match refusal {
            ArgvRefusal::MissingParameter { .. } => RefusalReason::MissingParameter,
            ArgvRefusal::UnknownParameter { .. } => RefusalReason::UnknownParameter,
            ArgvRefusal::Value { .. } => RefusalReason::InvalidValue,
        };
        Refusal {
            reason,
            detail: one_line(&format!(
                "{refusal}; run `{LIST_COMMAND}` to see what it accepts"
            )),
            project: Some((*project).to_owned()),
        }
    })?;
    let names_a_check = validated.check.is_some();
    let waits_for_the_author = matches!(validated.class, ActionClass::Human | ActionClass::Auto);
    let is_a_check = relay
        .catalogue
        .actions
        .values()
        .any(|a| a.check.as_deref() == Some(action));
    Ok(RunPlan {
        project: (*project).to_owned(),
        action: action.to_owned(),
        class: validated.class,
        target,
        project_dir,
        catalogue_hash: approved.hash.clone(),
        timeout_s: validated.timeout_s,
        max_output_kib: validated.max_output_kib,
        check: validated.check.clone(),
        binds_tree: names_a_check || is_a_check || waits_for_the_author,
        keeps_output: is_a_check,
    })
}

/// The check half of the class step (AD-29 step 6, AD-33, D-170): an action
/// naming a check (an `after-check` action, or a `human` action's dry run)
/// goes on only when [`classes::satisfying_check`] finds that check, run
/// with the same `params` under the same catalogue on the same `tree`, in
/// `lines`, and returns its run id; otherwise it is refused with
/// [`RefusalReason::CheckRequired`]. A plan without a check passes with no
/// run id.
pub fn check_step(
    plan: &RunPlan,
    params: &BTreeMap<String, String>,
    lines: &[JournalLine],
    tree: Option<&TreeFingerprint>,
    now_ms: i64,
) -> Result<Option<String>, Refusal> {
    let Some(check) = &plan.check else {
        return Ok(None);
    };
    if let Some(found) = classes::satisfying_check(
        lines,
        &plan.project,
        check,
        params,
        &plan.catalogue_hash,
        tree,
        now_ms,
    ) {
        return Ok(Some(found.run_id.clone()));
    }
    let minutes = CHECK_VALID_MS / 60_000;
    let detail = if plan.class == ActionClass::Human {
        format!(
            "run `{check}` first: an approval of {} is requested only after its dry run {check} succeeded with the same parameters on the same code in the last {minutes} minutes",
            plan.action
        )
    } else {
        format!(
            "run `{check}` first: {} runs only after {check} succeeded with the same parameters on the same code in the last {minutes} minutes",
            plan.action
        )
    };
    refuse(RefusalReason::CheckRequired, detail, Some(&plan.project))
}

fn action_lines(text: &mut String, name: &str, action: &CanonicalAction) {
    text.push_str("  officina run ");
    text.push_str(name);
    for param in action.params.keys() {
        text.push_str(&format!(" {param}=<...>"));
    }
    if !classes::is_available(action.class) {
        text.push_str(&format!(
            "  (class {}, not available in this build)",
            classes::class_name(action.class)
        ));
    }
    text.push('\n');
    if !action.description.is_empty() {
        text.push_str(&format!("      {}\n", action.description));
    }
    let minutes = CHECK_VALID_MS / 60_000;
    match (action.class, &action.check) {
        (ActionClass::AfterCheck, Some(check)) => text.push_str(&format!(
            "      runs only after `officina run {check}` succeeded with the same parameters on the same code in the last {minutes} minutes\n"
        )),
        (ActionClass::Human, check) => {
            if let Some(check) = check {
                text.push_str(&format!(
                    "      asks for approval only after its dry run `officina run {check}` succeeded with the same parameters on the same code in the last {minutes} minutes\n"
                ));
            }
            text.push_str(&format!(
                "      runs once per approval the author gives in the Officina window; the call waits for it up to --wait <s> (default {}, at most {})\n",
                approvals::DEFAULT_WAIT_S,
                classes::WAIT_RANGE_S.end()
            ));
        }
        _ => {}
    }
    for (param, rule) in &action.params {
        match rule {
            CanonicalParam::Values(values) => {
                text.push_str(&format!("      {param}: one of {}\n", values.join(", ")));
            }
            CanonicalParam::Pattern(pattern) => {
                text.push_str(&format!(
                    "      {param}: the whole value matches {pattern}\n"
                ));
            }
        }
    }
}

/// The text of `officina run --list` (D-21): the approved actions of the
/// project the working directory lies in, with their parameters and what
/// each accepts, unavailable classes marked (D-17); a stopped relay, a
/// pending catalogue change or an unreadable file stated first. It always
/// exits 0, whatever it says.
pub fn list_text(inputs: &RunInputs<'_>) -> String {
    let approved = match inputs.snapshot {
        SnapshotState::Missing => {
            return "No relay catalogue is approved yet: the author approves it in the Officina window, and `officina run` refuses every action until then.\n".to_owned();
        }
        SnapshotState::Unreadable(why) => {
            return format!(
                "The approved catalogue cannot be read ({why}): `officina run` refuses every action until the author approves it again in the Officina window.\n"
            );
        }
        SnapshotState::Valid(approved) => approved,
    };
    let mut text = String::new();
    if inputs.stopped {
        text.push_str("The relay is stopped in the Officina window: `officina run` refuses every action until the author resumes it.\n");
    }
    match inputs.floor {
        FloorState::Unreadable(why) => text.push_str(&format!(
            "The relay floor cannot be read ({why}): `officina run` refuses every action until it can.\n"
        )),
        FloorState::Valid { relay, .. } if relay.hash != approved.hash => text.push_str(
            "A catalogue change awaits approval in the Officina window: `officina run` refuses every action until it is approved.\n",
        ),
        FloorState::Valid { .. } => {}
    }
    let projects = projects_at(inputs.cwd, approved);
    if projects.is_empty() {
        text.push_str(&format!(
            "The folder {} lies in no project of the approved catalogue: there is nothing to run here.\n",
            inputs.cwd
        ));
        return text;
    }
    for (name, project) in projects {
        let autonomy = serde_json::to_value(project.autonomy)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        text.push_str(&format!("Project {name} ({autonomy}), approved actions:\n"));
        let actions: Vec<(&String, &CanonicalAction)> = project
            .actions
            .iter()
            .filter_map(|a| Some((a, approved.canonical.actions.get(a)?)))
            .collect();
        if actions.is_empty() {
            text.push_str("  (none)\n");
        }
        for (action_name, action) in actions {
            action_lines(&mut text, action_name, action);
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::catalogue::load;

    const FLOOR_PATH: &str = "C:/path/to/officina.toml";
    const FLOOR: &str = "schema = 2\n[[project]]\nname = \"lab\"\npath = \"Lab\"\nactions = [\"probe\", \"apply\"]\n[[project]]\nname = \"notes\"\npath = \"Notes\"\nautonomy = \"free\"\nactions = [\"probe\"]\n[[action]]\nname = \"probe\"\ndescription = \"Reads one host\"\nclass = \"free\"\nprogram = \"C:/path/to/tool.exe\"\nargs = [\"--host\", \"{host}\"]\n[action.params.host]\nvalues = [\"web01\", \"db01\"]\n[[action]]\nname = \"apply\"\nclass = \"human\"\nprogram = \"C:/path/to/tool.exe\"\nargs = [\"{group}\"]\n[action.params.group]\npattern = \"lab-[a-z]+\"\n";

    fn approved_of(relay: &RelayFloor) -> ApprovedCatalogue {
        ApprovedCatalogue {
            approved_at_ms: 1,
            hash: relay.hash.clone(),
            floor_file: Some(FLOOR_PATH.to_owned()),
            canonical: relay.canonical.clone(),
        }
    }

    fn params(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn inputs<'a>(
        stopped: bool,
        snapshot: SnapshotState<'a>,
        floor: FloorState<'a>,
        cwd: &'a str,
    ) -> RunInputs<'a> {
        RunInputs {
            stopped,
            snapshot,
            floor,
            cwd,
            system_dir: r"C:\Windows\System32",
        }
    }

    #[test]
    fn checks_fail_in_the_documented_order() {
        let current = load(FLOOR, FLOOR_PATH).unwrap();
        let approved = approved_of(&current);
        let edited = load(&FLOOR.replace("web01", "web02"), FLOOR_PATH).unwrap();
        let floor = FloorState::Valid {
            relay: &current,
            path: FLOOR_PATH,
        };
        let valid = SnapshotState::Valid(&approved);
        let lab = "C:/path/to/Lab/roles";
        let reason = |i: RunInputs<'_>, action: &str, p: &[(&str, &str)]| {
            plan(&i, action, &params(p)).map(|_| ()).unwrap_err().reason
        };
        let bad = [("host", "evil")];
        assert_eq!(
            reason(
                inputs(
                    true,
                    SnapshotState::Missing,
                    FloorState::Unreadable("x"),
                    "C:/x"
                ),
                "apply",
                &bad
            ),
            RefusalReason::Stopped
        );
        assert_eq!(
            reason(
                inputs(
                    false,
                    SnapshotState::Missing,
                    FloorState::Unreadable("x"),
                    "C:/x"
                ),
                "apply",
                &bad
            ),
            RefusalReason::NotApproved
        );
        assert_eq!(
            reason(
                inputs(false, SnapshotState::Unreadable("corrupt"), floor, "C:/x"),
                "apply",
                &bad
            ),
            RefusalReason::ApprovedUnreadable
        );
        assert_eq!(
            reason(
                inputs(false, valid, FloorState::Unreadable("gone"), "C:/x"),
                "apply",
                &bad
            ),
            RefusalReason::FloorUnreadable
        );
        let changed = FloorState::Valid {
            relay: &edited,
            path: FLOOR_PATH,
        };
        assert_eq!(
            reason(inputs(false, valid, changed, "C:/x"), "apply", &bad),
            RefusalReason::CatalogueChanged
        );
        assert_eq!(
            reason(inputs(false, valid, floor, "C:/x"), "apply", &bad),
            RefusalReason::OutsideProjects
        );
        assert_eq!(
            reason(
                inputs(false, valid, floor, "C:/path/to/Notes"),
                "apply",
                &bad
            ),
            RefusalReason::ActionNotAllowed
        );
        assert_eq!(
            reason(inputs(false, valid, floor, lab), "probe", &[]),
            RefusalReason::MissingParameter
        );
        assert_eq!(
            reason(
                inputs(false, valid, floor, lab),
                "probe",
                &[("host", "web01"), ("x", "1")]
            ),
            RefusalReason::UnknownParameter
        );
        assert_eq!(
            reason(inputs(false, valid, floor, lab), "probe", &bad),
            RefusalReason::InvalidValue
        );
        let human = plan(
            &inputs(false, valid, floor, lab),
            "apply",
            &params(&[("group", "lab-web")]),
        )
        .unwrap();
        assert_eq!(
            human.class,
            ActionClass::Human,
            "a human action plans (D-20 gone)"
        );
        assert!(
            human.binds_tree && human.check.is_none(),
            "a human action binds its tree, check or not (D-173)"
        );

        let run = plan(
            &inputs(false, valid, floor, lab),
            "probe",
            &params(&[("host", "web01")]),
        )
        .unwrap();
        assert_eq!(run.project, "lab");
        assert_eq!(run.argv(), ["C:/path/to/tool.exe", "--host", "web01"]);
        assert_eq!(
            run.launch().unwrap().cwd,
            "C:/path/to/Lab",
            "the folder as declared (D-6)"
        );
        assert_eq!(run.catalogue_hash, approved.hash);
        let free = plan(
            &inputs(false, valid, floor, "C:/path/to/Notes"),
            "probe",
            &params(&[("host", "db01")]),
        )
        .unwrap();
        assert_eq!(
            free.project, "notes",
            "a free project listing the action is served (D-21)"
        );
    }

    /// The binding's constructors give exactly the reasons the constant
    /// lists, each once.
    #[test]
    fn tree_binding_refusals_are_the_bindings() {
        let made = [
            Refusal::git_unavailable("lab", "no git").reason,
            Refusal::tree_too_large("lab", "big").reason,
            Refusal::tree_unreadable("lab", "locked").reason,
            Refusal::check_not_satisfied("lab", "verify").reason,
            Refusal::tree_changed("lab").reason,
        ];
        assert_eq!(made, TREE_BINDING_REFUSALS);
        for reason in TREE_BINDING_REFUSALS {
            assert!(RefusalReason::ALL.contains(reason));
        }
    }

    #[test]
    fn a_refusal_never_echoes_the_value() {
        let current = load(FLOOR, FLOOR_PATH).unwrap();
        let approved = approved_of(&current);
        let floor = FloorState::Valid {
            relay: &current,
            path: FLOOR_PATH,
        };
        let refusal = plan(
            &inputs(
                false,
                SnapshotState::Valid(&approved),
                floor,
                "C:/path/to/Lab",
            ),
            "probe",
            &params(&[("host", "web01; rm -rf /")]),
        )
        .unwrap_err();
        assert!(!refusal.detail.contains("rm -rf"), "{}", refusal.detail);
        assert_eq!(refusal.project.as_deref(), Some("lab"));
    }

    #[test]
    fn list_states_what_blocks_and_explains_human_actions() {
        let current = load(FLOOR, FLOOR_PATH).unwrap();
        let approved = approved_of(&current);
        let floor = FloorState::Valid {
            relay: &current,
            path: FLOOR_PATH,
        };
        let text = list_text(&inputs(
            false,
            SnapshotState::Valid(&approved),
            floor,
            "C:/path/to/Lab",
        ));
        assert!(text.starts_with("Project lab (guarded)"), "{text}");
        assert!(text.contains("officina run probe host=<...>\n"), "{text}");
        assert!(text.contains("host: one of db01, web01"), "{text}");
        assert!(
            text.contains("group: the whole value matches lab-[a-z]+"),
            "{text}"
        );
        assert!(
            text.contains(
                "officina run apply group=<...>
"
            ),
            "{text}"
        );
        assert!(!text.contains("not available"), "{text}");
        assert!(
            text.contains("runs once per approval the author gives in the Officina window"),
            "{text}"
        );
        assert!(text.contains("Reads one host"), "{text}");

        let edited = load(&FLOOR.replace("web01", "web02"), FLOOR_PATH).unwrap();
        let pending = list_text(&inputs(
            true,
            SnapshotState::Valid(&approved),
            FloorState::Valid {
                relay: &edited,
                path: FLOOR_PATH,
            },
            "C:/path/to/Lab",
        ));
        assert!(
            pending.contains("stopped") && pending.contains("awaits approval"),
            "{pending}"
        );
        assert!(
            pending.contains("officina run probe"),
            "the approved actions are still listed: {pending}"
        );

        let outside = list_text(&inputs(
            false,
            SnapshotState::Valid(&approved),
            floor,
            "D:/elsewhere",
        ));
        assert!(outside.contains("lies in no project"), "{outside}");
        assert!(
            list_text(&inputs(false, SnapshotState::Missing, floor, "C:/x"))
                .contains("No relay catalogue is approved")
        );
    }

    const CHECKED_FLOOR: &str = "schema = 2
[[project]]
name = \"lab\"
path = \"Lab\"
actions = [\"verify\", \"apply\", \"probe\"]
[[action]]
name = \"verify\"
class = \"free\"
program = \"C:/path/to/tool.exe\"
args = [\"--check\", \"{group}\"]
[action.params.group]
pattern = \"lab-[a-z]+\"
[[action]]
name = \"apply\"
class = \"after-check\"
check = \"verify\"
program = \"C:/path/to/tool.exe\"
args = [\"{group}\"]
[action.params.group]
pattern = \"lab-[a-z]+\"
[[action]]
name = \"probe\"
class = \"free\"
program = \"C:/path/to/tool.exe\"
";

    #[test]
    fn after_check_needs_its_check_and_binds_the_tree() {
        use crate::relay::journal::{
            JournalEntry, RunFinished, RunOutcome, RunStarted, StreamSummary,
        };
        let current = load(CHECKED_FLOOR, FLOOR_PATH).unwrap();
        assert!(
            current.catalogue.issues.is_empty(),
            "{:?}",
            current.catalogue.issues
        );
        let approved = approved_of(&current);
        let floor = FloorState::Valid {
            relay: &current,
            path: FLOOR_PATH,
        };
        let ready = inputs(
            false,
            SnapshotState::Valid(&approved),
            floor,
            "C:/path/to/Lab",
        );
        let group = params(&[("group", "lab-web")]);
        let verify = plan(&ready, "verify", &group).unwrap();
        let apply = plan(&ready, "apply", &group).unwrap();
        let probe = plan(&ready, "probe", &params(&[])).unwrap();
        assert_eq!((verify.check.as_deref(), verify.binds_tree), (None, true));
        assert_eq!(
            (apply.check.as_deref(), apply.binds_tree),
            (Some("verify"), true)
        );
        assert_eq!((probe.check.as_deref(), probe.binds_tree), (None, false));
        let listed = list_text(&ready);
        assert!(
            listed.contains("  officina run apply group=<...>\n      runs only after `officina run verify` succeeded with the same parameters"),
            "{listed}"
        );

        let now = 1_793_102_400_000;
        let tree = TreeFingerprint {
            sha256: crate::relay::sha256_hex(b"tree"),
            files: 0,
            git: None,
        };
        let refusal = check_step(&apply, &group, &[], Some(&tree), now).unwrap_err();
        assert_eq!(refusal.reason, RefusalReason::CheckRequired);
        assert!(
            refusal.detail.starts_with("run `verify` first"),
            "{}",
            refusal.detail
        );
        assert_eq!(refusal.project.as_deref(), Some("lab"));

        let stream = StreamSummary {
            bytes: 0,
            sha256: crate::relay::sha256_hex(b""),
        };
        let lines = [
            JournalLine::new(
                now - 60_000,
                JournalEntry::RunStarted(RunStarted {
                    run_id: "r1".to_owned(),
                    project: "lab".to_owned(),
                    session_id: None,
                    action: "verify".to_owned(),
                    class: verify.class,
                    params: group.clone(),
                    argv: verify.argv(),
                    method: None,
                    url: None,
                    credential: None,
                    catalogue_hash: verify.catalogue_hash.clone(),
                    approval_id: None,
                    tree: Some(tree.clone()),
                    timeout_s: Some(verify.timeout_s),
                }),
            ),
            JournalLine::new(
                now - 30_000,
                JournalEntry::RunFinished(RunFinished {
                    run_id: "r1".to_owned(),
                    outcome: RunOutcome::Exited,
                    exit_code: Some(0),
                    duration_ms: 30_000,
                    stdout: Some(stream.clone()),
                    stderr: Some(stream),
                    dropped_bytes: 0,
                    status: None,
                    body_bytes: None,
                    body_sha256: None,
                    detail: None,
                }),
            ),
        ];
        assert_eq!(
            check_step(&apply, &group, &lines, Some(&tree), now),
            Ok(Some("r1".to_owned())),
            "the satisfying check's run id"
        );
        assert_eq!(
            check_step(&verify, &group, &[], None, now),
            Ok(None),
            "a plan without a check passes"
        );
    }
}
