//! `officina run` as pure functions (AD-29, D-20, D-21): the checks in their
//! documented order, failing closed, and the text of `officina run --list`.
//! The host reads the stop flag, the files, the working directory and the
//! system directory, then journals and spawns what [`plan`] returns.

use std::collections::BTreeMap;

use super::argv::{self, ArgvRefusal, Launch};
use super::catalogue::{
    ActionClass, ApprovedCatalogue, CanonicalAction, CanonicalParam, CanonicalProject, RelayFloor,
};
use super::classes;
use super::gate::{LIST_COMMAND, normalise_cwd};
use super::journal::RefusalReason;
use crate::floor::{floor_dir_of, join_project_path};

/// Exit code of a refusal (AD-29): a shell's "found but not executable".
pub const EXIT_REFUSED: i32 = 126;
/// Exit code when Officina ended the action at its timeout.
pub const EXIT_TIMEOUT: i32 = 124;
/// Exit code when the program could not start.
pub const EXIT_NOT_STARTED: i32 = 127;
/// Exit code of a usage error.
pub const EXIT_USAGE: i32 = 64;

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
    pub launch: Launch,
    /// The approved hash, equal to the current one.
    pub catalogue_hash: String,
    pub timeout_s: u32,
    pub max_output_kib: u32,
}

impl RunPlan {
    /// The program, then its arguments: what the journal records as `argv`.
    pub fn argv(&self) -> Vec<String> {
        std::iter::once(self.launch.program.clone())
            .chain(self.launch.args.iter().cloned())
            .collect()
    }
}

fn refuse<T>(reason: RefusalReason, detail: String, project: Option<&str>) -> Result<T, Refusal> {
    Err(Refusal {
        reason,
        detail: one_line(&detail),
        project: project.map(str::to_owned),
    })
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
/// 6. the action's class runs in this build (D-20).
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
        return refuse(
            RefusalReason::Stopped,
            "the relay is stopped in the Officina window; the author resumes it there".to_owned(),
            None,
        );
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
    let launch =
        argv::build(validated, params, &project_dir, inputs.system_dir).map_err(|refusal| {
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
    if let Err(reason) = classes::decide(validated.class) {
        return refuse(
            reason,
            format!(
                "the action {action} is of class {}, which this build does not run yet",
                classes::class_name(validated.class)
            ),
            Some(project),
        );
    }
    Ok(RunPlan {
        project: (*project).to_owned(),
        action: action.to_owned(),
        class: validated.class,
        launch,
        catalogue_hash: approved.hash.clone(),
        timeout_s: validated.timeout_s,
        max_output_kib: validated.max_output_kib,
    })
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
        assert_eq!(
            reason(
                inputs(false, valid, floor, lab),
                "apply",
                &[("group", "lab-web")]
            ),
            RefusalReason::ClassNotAvailable
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
            run.launch.cwd, "C:/path/to/Lab",
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
    fn list_states_what_blocks_and_marks_unavailable_classes() {
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
                "officina run apply group=<...>  (class human, not available in this build)"
            ),
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
}
