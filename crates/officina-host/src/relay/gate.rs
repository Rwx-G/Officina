//! The gate in the hook (AD-26, AD-28, D-13 to D-17, D-171, D-173): reads a
//! `PreToolUse` payload of a shell or file tool, the relay floor (phase 1
//! only), the approved snapshot and, when one of them fails, the
//! known-projects record; decides by the tool's rule; journals each deny and
//! each fault; and returns the JSON Claude Code honours in every mode (E12,
//! E15). It spawns nothing and compiles no pattern.
//!
//! Before the pure gate sees them, the payload `cwd`, `CLAUDE_PROJECT_DIR`
//! and a file tool's target are resolved to the final path of an opened
//! handle (D-173): an 8.3 short name, a junction, a symbolic link or a
//! `subst` drive leading into a guarded project, or into the Officina home,
//! reads as the folder it leads to. A path that cannot be opened keeps its
//! normalised string; the folders are looked up as given and as resolved.
//! The guarded projects' roots are resolved the same way, once per set of
//! roots ([`final_roots`]), so a floor that writes a root through an alias
//! (a temp folder under an 8.3 user name) matches a folder given in full.
//!
//! It is containment, not the authority: an internal fault gives no
//! decision (fail open, AD-28), while an input it cannot read makes it
//! stricter (D-15). `officina run` fails closed on its own.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use officina_core::events::excerpt;
use officina_core::floor::{parse_floor, requests_no_excerpts};
use officina_core::quota::normalise_path;
use officina_core::relay::gate::{
    Deny, FloorInput, Offer, SnapshotInput, ToolRule, containment, decide_call, decide_file_write,
    deny_reason, file_tool_field, normalise_cwd, protected_dirs, tool_rule,
};
use officina_core::relay::journal::{
    COMMAND_EXCERPT_CHARS, GateDenied, GateError, GateFault, JournalEntry,
};
use officina_core::relay::{HOME_DIR_NAME, sha256_hex};

use serde::{Deserialize, Serialize};

use super::home::{ApprovedState, RelayHome};
use super::journal;
use crate::files::atomic_write;
use crate::paths::{excerpts_disabled_by_env, user_home};

/// The hook event the gate answers (AD-26).
pub const GATE_EVENT: &str = "PreToolUse";

/// Claude Code's project root in the hook's environment (E15): it stays the
/// session's root when the shell's `cwd` moves.
pub const ENV_PROJECT_DIR: &str = "CLAUDE_PROJECT_DIR";

/// What the gate reads from the hook's own environment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GateEnv {
    /// `CLAUDE_PROJECT_DIR`.
    pub project_dir: Option<String>,
    /// `OFFICINA_NO_EXCERPTS`.
    pub excerpts_off: bool,
}

impl GateEnv {
    /// The hook's environment.
    pub fn from_env() -> GateEnv {
        GateEnv {
            project_dir: std::env::var(ENV_PROJECT_DIR)
                .ok()
                .filter(|v| !v.is_empty()),
            excerpts_off: excerpts_disabled_by_env(),
        }
    }
}

/// A gated tool's call as the payload carries it (E15, D-171, D-173).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    /// The tool, one of `SHELL_TOOLS` or `FILE_TOOLS`.
    pub tool: String,
    /// A shell tool's `tool_input.command`; a lexed tool's call always has
    /// one.
    pub command: Option<String>,
    /// The file a file tool writes, as its `tool_input` names it.
    pub target: Option<String>,
    /// The shell's working directory, which follows `cd` (E15).
    pub cwd: Option<String>,
    pub session_id: Option<String>,
}

impl ToolCall {
    /// The call, when the payload is a gated tool's (AD-26 step 1, D-171,
    /// D-173): a lexed tool's needs a command, a tool refused whole does
    /// not, a file tool's needs the file it writes. Anything else gets no
    /// decision.
    pub fn from_payload(payload: &serde_json::Value) -> Option<ToolCall> {
        let text = |value: Option<&serde_json::Value>| {
            value.and_then(serde_json::Value::as_str).map(str::to_owned)
        };
        let tool = text(payload.get("tool_name"))?;
        let input = payload.get("tool_input");
        let (command, target) = match file_tool_field(&tool) {
            Some(field) => (None, Some(text(input.and_then(|i| i.get(field)))?)),
            None => {
                let command = text(input.and_then(|i| i.get("command")));
                match tool_rule(&tool)? {
                    ToolRule::Lexed if command.is_none() => return None,
                    ToolRule::Lexed | ToolRule::DeniedInGuardedProject => {}
                }
                (command, None)
            }
        };
        Some(ToolCall {
            tool,
            command,
            target,
            cwd: text(payload.get("cwd")),
            session_id: text(payload.get("session_id")),
        })
    }

    /// What a gate line quotes: the command, or the file written.
    fn quoted(&self) -> Option<&str> {
        self.command.as_deref().or(self.target.as_deref())
    }
}

/// A file tool's target as an absolute path: a relative one is taken from
/// the call's `cwd`, then made absolute as Windows would (`..` applied).
pub fn absolute_target(target: &str, cwd: Option<&str>) -> PathBuf {
    let path = Path::new(target);
    let joined = match cwd {
        Some(cwd) if path.is_relative() => Path::new(cwd).join(path),
        _ => path.to_path_buf(),
    };
    std::path::absolute(&joined).unwrap_or(joined)
}

/// The final path of a folder the payload names, by an opened handle
/// (D-173), or `None` when it cannot be opened. Git Bash's `/c/...` and the
/// other spellings the pure gate reads are taken as Windows paths first.
pub fn final_dir(dir: &str) -> Option<String> {
    std::fs::canonicalize(normalise_cwd(dir))
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
}

/// `relay/final-roots.json`: the guarded roots it was written for, by
/// hash, and the final path of each one that opened.
#[derive(Debug, Serialize, Deserialize)]
struct FinalRoots {
    key: String,
    finals: BTreeMap<String, String>,
}

/// The final path of each guarded root by an opened handle (D-173), taken
/// from `relay/final-roots.json` when it was written for the same roots,
/// else resolved and written there: the handles are opened once per set of
/// roots, not on every call (AD-28 budget), and a root on a share that does
/// not answer delays one call, not each. A root that cannot be opened has
/// no entry and keeps its written form. The file is as writable as every
/// relay file (AD-25), so the gate adds what it holds to guarded projects
/// only ([`officina_core::relay::gate::Containment::add_final_roots`]).
pub fn final_roots(home: &RelayHome, roots: &BTreeSet<&str>) -> BTreeMap<String, String> {
    if roots.is_empty() {
        return BTreeMap::new();
    }
    let key = sha256_hex(
        roots
            .iter()
            .copied()
            .collect::<Vec<_>>()
            .join(
                "
",
            )
            .as_bytes(),
    );
    let file = home.final_roots_file();
    if let Some(cached) = fs::read(&file)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<FinalRoots>(&bytes).ok())
        .filter(|cached| cached.key == key)
    {
        return cached.finals;
    }
    let finals: BTreeMap<String, String> = roots
        .iter()
        .filter_map(|root| Some(((*root).to_owned(), final_dir(root)?)))
        .collect();
    if let Ok(bytes) = serde_json::to_vec(&FinalRoots {
        key,
        finals: finals.clone(),
    }) {
        let _ = atomic_write(&file, &bytes);
    }
    finals
}

/// `path` with its deepest folder that opens resolved by handle and the
/// rest joined as written (D-173): what a file a tool is about to create
/// resolves to. The path itself when no part of it opens.
pub fn final_target(path: &Path) -> String {
    let mut rest = Vec::new();
    let mut current = path;
    loop {
        if let Ok(mut found) = std::fs::canonicalize(current) {
            found.extend(rest.iter().rev());
            return found.to_string_lossy().into_owned();
        }
        match (current.parent(), current.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name);
                current = parent;
            }
            _ => return path.to_string_lossy().into_owned(),
        }
    }
}

/// A deny and what the session is told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateDeny {
    pub project: String,
    pub deny: Deny,
    pub reason: String,
}

/// What the gate concluded for one call, before journaling.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Evaluation {
    pub deny: Option<GateDeny>,
    /// The guarded project the call lies in, if any.
    pub project: Option<String>,
    /// Each input the gate could not read, with what went wrong.
    pub faults: Vec<(GateFault, String)>,
    /// The floor asks for no excerpts.
    pub floor_no_excerpts: bool,
}

/// True when `home` is the default `~/.officina`, whose tails D-13 also
/// protects.
fn is_default_home(home: &RelayHome) -> bool {
    user_home().is_some_and(|user| {
        normalise_path(&user.join(HOME_DIR_NAME).to_string_lossy())
            == normalise_path(&home.officina_home().to_string_lossy())
    })
}

/// Reads the gate's inputs and decides for `call` (AD-26 steps 2 to 5,
/// D-14 to D-16, D-173). `project_dir` is `CLAUDE_PROJECT_DIR`. A file
/// tool's write is decided from any project once the relay guards one.
pub fn evaluate(home: &RelayHome, call: &ToolCall, project_dir: Option<&str>) -> Evaluation {
    let approved = home.read_approved();
    let floor_path = home.relay_floor(&approved);
    let floor_path_text = floor_path.to_string_lossy();
    let floor_text = fs::read_to_string(&floor_path);
    let parsed = floor_text.as_ref().map(|text| parse_floor(text));
    let floor = match &parsed {
        Ok(Ok(floor)) => FloorInput::Valid {
            floor,
            path: &floor_path_text,
        },
        Ok(Err(_)) => FloorInput::Invalid,
        Err(e) if e.kind() == io::ErrorKind::NotFound => FloorInput::Missing,
        Err(_) => FloorInput::Unreadable,
    };
    let snapshot = match &approved {
        ApprovedState::Missing => SnapshotInput::Missing,
        ApprovedState::Unreadable(_) => SnapshotInput::Unreadable,
        ApprovedState::Valid(approved) => SnapshotInput::Valid(&approved.canonical),
    };
    // The record is read only when the floor or the snapshot failed or is
    // missing, so the usual call reads two files (AD-28).
    let complete =
        matches!(snapshot, SnapshotInput::Valid(_)) && matches!(floor, FloorInput::Valid { .. });
    let known = if complete {
        None
    } else {
        home.read_known_projects()
    };

    let mut contained = containment(floor, snapshot, known.as_ref());
    let finals = final_roots(home, &contained.guarded_roots());
    contained.add_final_roots(&finals);
    let detail_of = |fault: GateFault| -> String {
        let floor_shown = floor_path.display();
        match (fault, &parsed, &approved) {
            (GateFault::FloorInvalid, Ok(Err(e)), _) => format!("{floor_shown}: {e}"),
            (GateFault::FloorUnreadable, Err(e), _) => format!("cannot read {floor_shown}: {e}"),
            (GateFault::FloorMissing, _, _) => format!("{floor_shown} is missing"),
            (GateFault::ApprovedUnreadable, _, ApprovedState::Unreadable(e)) => {
                format!("{}: {e}", home.approved_file().display())
            }
            (GateFault::ApprovedMissing, _, _) => format!(
                "{} is missing while {} records an approved catalogue",
                home.approved_file().display(),
                home.known_projects_file().display()
            ),
            (GateFault::KnownProjectsUnreadable, _, _) => format!(
                "the floor and the approved catalogue failed and {} cannot be read: nothing is guarded",
                home.known_projects_file().display()
            ),
            (other, _, _) => format!("{other:?}"),
        }
    };
    let faults = contained
        .faults
        .iter()
        .map(|fault| (*fault, detail_of(*fault)))
        .collect();
    // A floor phase 1 refuses can still ask for no excerpts, as for the
    // event buffer.
    let floor_no_excerpts = match &parsed {
        Ok(Ok(floor)) => floor.no_excerpts,
        _ => floor_text.as_deref().is_ok_and(requests_no_excerpts),
    };
    // As given first, so D-16's order holds, then as resolved by handle.
    let given: Vec<&str> = project_dir.into_iter().chain(call.cwd.as_deref()).collect();
    let resolved: Vec<String> = given.iter().filter_map(|dir| final_dir(dir)).collect();
    let dirs: Vec<&str> = given
        .iter()
        .copied()
        .chain(resolved.iter().map(String::as_str))
        .collect();
    if let Some(target) = &call.target {
        let project = contained.guarded_project(&dirs);
        let name = project.map(|p| p.name.clone()).unwrap_or_default();
        let deny = decide_file_write(
            &final_target(&absolute_target(target, call.cwd.as_deref())),
            &final_target(home.officina_home()),
            contained.guards_any(),
        )
        .map(|deny| GateDeny {
            reason: deny_reason(
                &deny,
                &name,
                project.map_or(Offer::Approved(&[]), |p| contained.offer_for(&p.name)),
            ),
            project: name.clone(),
            deny,
        });
        return Evaluation {
            deny,
            project: project.map(|p| p.name.clone()),
            faults,
            floor_no_excerpts,
        };
    }
    let Some(project) = contained.guarded_project(&dirs) else {
        return Evaluation {
            faults,
            floor_no_excerpts,
            ..Evaluation::default()
        };
    };
    let protected = protected_dirs(
        &home.officina_home().to_string_lossy(),
        is_default_home(home),
    );
    let deny = decide_call(
        &call.tool,
        call.command.as_deref(),
        &contained.guarded_programs,
        &protected,
        true,
    )
    .map(|deny| GateDeny {
        reason: deny_reason(&deny, &project.name, contained.offer_for(&project.name)),
        project: project.name.clone(),
        deny,
    });
    Evaluation {
        deny,
        project: Some(project.name.clone()),
        faults,
        floor_no_excerpts,
    }
}

/// The JSON a deny prints on stdout (E15): Claude Code refuses the call in
/// every permission mode and shows the session `reason`.
pub fn deny_json(reason: &str) -> String {
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": GATE_EVENT,
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }
    })
    .to_string()
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic without a message".to_owned())
}

/// Runs `decide` for `call`, journals what it concluded, and returns the
/// deny JSON when there is one. A panic or an error inside `decide` gives no
/// decision and a `gate_error` line (AD-28); the command excerpt is then
/// left out, since the floor's `no_excerpts` is unknown. A journal that
/// stays locked loses the lines, never the decision (AD-32).
pub fn answer_with(
    home: &RelayHome,
    call: &ToolCall,
    env: &GateEnv,
    now_ms: i64,
    decide: impl FnOnce() -> io::Result<Evaluation>,
) -> Option<String> {
    let evaluation = match catch_unwind(AssertUnwindSafe(decide)) {
        Ok(Ok(evaluation)) => evaluation,
        Ok(Err(e)) => {
            journal_fault(home, call, GateFault::Io, e.to_string(), now_ms);
            return None;
        }
        Err(payload) => {
            journal_fault(home, call, GateFault::Panic, panic_text(&*payload), now_ms);
            return None;
        }
    };
    let command_excerpt = call
        .quoted()
        .filter(|_| !env.excerpts_off && !evaluation.floor_no_excerpts)
        .map(|command| excerpt(command, COMMAND_EXCERPT_CHARS));
    let mut lines = Vec::new();
    if let Some(deny) = &evaluation.deny {
        lines.push(JournalEntry::GateDenied(GateDenied {
            project: deny.project.clone(),
            session_id: call.session_id.clone(),
            deny: deny.deny.kind,
            matched: deny.deny.matched.clone(),
            command_excerpt: command_excerpt.clone(),
        }));
    }
    lines.extend(evaluation.faults.iter().map(|(cause, detail)| {
        JournalEntry::GateError(GateError {
            project: evaluation.project.clone(),
            session_id: call.session_id.clone(),
            cause: *cause,
            detail: detail.clone(),
            command_excerpt: command_excerpt.clone(),
        })
    }));
    let _ = journal::append(home, &lines, now_ms);
    evaluation.deny.map(|deny| deny_json(&deny.reason))
}

fn journal_fault(home: &RelayHome, call: &ToolCall, cause: GateFault, detail: String, now_ms: i64) {
    let line = JournalEntry::GateError(GateError {
        project: None,
        session_id: call.session_id.clone(),
        cause,
        detail,
        command_excerpt: None,
    });
    let _ = journal::append(home, &[line], now_ms);
}

/// The gate for one `PreToolUse` payload: `None` is no decision, which
/// prints nothing. Not a gated tool's call: no decision, no line.
pub fn answer(
    home: &RelayHome,
    payload: &serde_json::Value,
    env: &GateEnv,
    now_ms: i64,
) -> Option<String> {
    let call = ToolCall::from_payload(payload)?;
    answer_with(home, &call, env, now_ms, || {
        Ok(evaluate(home, &call, env.project_dir.as_deref()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use officina_core::relay::journal::JournalLine;

    fn call() -> ToolCall {
        ToolCall {
            tool: "Bash".to_owned(),
            command: Some("ssh host01".to_owned()),
            target: None,
            cwd: Some(r"C:\path\to\Lab".to_owned()),
            session_id: Some("00000000-0000-0000-0000-000000000001".to_owned()),
        }
    }

    fn gate_errors(home: &RelayHome) -> Vec<GateError> {
        let (lines, corrupt) = journal::read_recent(home, crate::files::now_ms());
        assert_eq!(corrupt, 0);
        lines
            .into_iter()
            .map(|line: JournalLine| match line.entry {
                JournalEntry::GateError(error) => error,
                other => panic!("unexpected line {other:?}"),
            })
            .collect()
    }

    /// `cmd.exe` from the system directory, running `line`; its stdout.
    #[cfg(windows)]
    fn cmd(line: &str) -> String {
        use std::os::windows::process::CommandExt;
        let system = crate::relay::home::system_dir().unwrap();
        let out = std::process::Command::new(system.join("cmd.exe"))
            .arg("/d")
            .arg("/c")
            .raw_arg(line)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// A relay home whose floor guards `project` as `lab`, with the default
    /// list (`ssh` among it), nothing approved yet.
    #[cfg(windows)]
    fn home_guarding(dir: &Path, project: &Path) -> RelayHome {
        let home = RelayHome::new(dir.join("officina-home"), None);
        fs::create_dir_all(home.officina_home()).unwrap();
        let path = project.to_string_lossy().replace('\\', "/");
        fs::write(
            home.default_floor(),
            format!("schema = 2\n[[project]]\nname = \"lab\"\npath = \"{path}\"\n"),
        )
        .unwrap();
        home
    }

    #[cfg(windows)]
    fn bash_in(cwd: &Path) -> ToolCall {
        ToolCall {
            cwd: Some(cwd.to_string_lossy().into_owned()),
            ..call()
        }
    }

    /// AC 8 of story 2.14 (D-173): a session entering a guarded project by
    /// its 8.3 short name is guarded; skipped where the volume makes no
    /// short names.
    #[cfg(windows)]
    #[test]
    fn short_name_resolves_to_the_guarded_project() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("Supervision Platform Project");
        fs::create_dir_all(&project).unwrap();
        let short = cmd(&format!(
            "for %I in (\"{}\") do @echo %~sI",
            project.display()
        ));
        if short.is_empty() || short.eq_ignore_ascii_case(&project.to_string_lossy()) {
            println!("skipped: this volume makes no 8.3 short names");
            return;
        }
        assert!(short.contains('~'), "{short}");
        let home = home_guarding(dir.path(), &project);
        let found = evaluate(&home, &bash_in(Path::new(&short)), None);
        let deny = found.deny.expect("the short name is the guarded project");
        assert_eq!(deny.project, "lab");
        let as_root = evaluate(&home, &bash_in(dir.path()), Some(&short));
        assert!(
            as_root.deny.is_some(),
            "CLAUDE_PROJECT_DIR by its short name"
        );
    }

    /// AC 8 of story 2.14 (D-173): a junction from a free folder into a
    /// guarded project is guarded, as `cwd` and as `CLAUDE_PROJECT_DIR`; a
    /// junction into the Officina home cannot carry a file write either.
    #[cfg(windows)]
    #[test]
    fn junction_into_a_guarded_project_is_guarded() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("Supervision");
        let free = dir.path().join("Notes");
        fs::create_dir_all(project.join("roles")).unwrap();
        fs::create_dir_all(&free).unwrap();
        let link = free.join("lab-link");
        cmd(&format!(
            "mklink /J \"{}\" \"{}\"",
            link.display(),
            project.display()
        ));
        assert!(link.join("roles").is_dir(), "the junction was made");
        let home = home_guarding(dir.path(), &project);
        let inside = link.join("roles");
        assert_eq!(
            evaluate(&home, &bash_in(&inside), None)
                .deny
                .map(|d| d.project),
            Some("lab".to_owned())
        );
        assert!(
            evaluate(&home, &bash_in(&free), Some(&link.to_string_lossy()))
                .deny
                .is_some()
        );
        assert!(
            evaluate(&home, &bash_in(&free), None).deny.is_none(),
            "the free folder itself stays free"
        );

        fs::create_dir_all(home.approvals_dir()).unwrap();
        let home_link = free.join("home-link");
        cmd(&format!(
            "mklink /J \"{}\" \"{}\"",
            home_link.display(),
            home.officina_home().display()
        ));
        let forged = home_link
            .join("relay")
            .join("approvals")
            .join("0123456789abcdef-1.approved.json");
        let write = ToolCall {
            tool: "Write".to_owned(),
            command: None,
            target: Some(forged.to_string_lossy().into_owned()),
            ..bash_in(&free)
        };
        let found = evaluate(&home, &write, None)
            .deny
            .expect("written through a junction");
        assert_eq!(
            found.deny.kind,
            officina_core::relay::gate::DenyKind::RelayStateWrite
        );
    }

    /// The hosted runner's failure of story 2.14 (its temp folder sits under
    /// an 8.3 user name): a floor naming the guarded project by the short
    /// form of a long path guards the folder given in long form, and the
    /// reverse; both sides of the match are final paths.
    #[cfg(windows)]
    #[test]
    fn a_short_root_matches_a_long_folder_and_the_reverse() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("Supervision Platform Project");
        fs::create_dir_all(project.join("roles")).unwrap();
        let short = cmd(&format!(
            "for %I in (\"{}\") do @echo %~sI",
            project.display()
        ));
        if !short.contains('~') {
            println!("skipped: this volume makes no 8.3 short names");
            return;
        }
        let long = fs::canonicalize(&project).unwrap();
        let long = long.to_string_lossy();
        let long = long.trim_start_matches(r"\\?\");
        assert!(!long.contains('~'), "{long}");

        let home = home_guarding(dir.path(), Path::new(&short));
        let found = evaluate(&home, &bash_in(&Path::new(long).join("roles")), None);
        assert_eq!(found.deny.map(|d| d.project), Some("lab".to_owned()));

        let home = home_guarding(dir.path(), Path::new(long));
        let found = evaluate(&home, &bash_in(&Path::new(&short).join("roles")), None);
        assert_eq!(found.deny.map(|d| d.project), Some("lab".to_owned()));
    }

    /// The roots are resolved once per set of roots: a second call reads
    /// `relay/final-roots.json` back, and another set resolves again.
    /// Windows only, like every resolution here: [`final_dir`] reads a folder
    /// as Windows names it, case folded.
    #[cfg(windows)]
    #[test]
    fn final_roots_are_resolved_once_per_set_of_roots() {
        let dir = tempfile::tempdir().unwrap();
        let home = RelayHome::new(dir.path().join("officina-home"), None);
        let lab = dir.path().join("Lab");
        let other = dir.path().join("Other");
        fs::create_dir_all(&lab).unwrap();
        fs::create_dir_all(&other).unwrap();
        let root = lab.to_string_lossy().into_owned();
        let resolved = fs::canonicalize(&lab)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let roots = BTreeSet::from([root.as_str()]);
        assert_eq!(final_roots(&home, &roots).get(&root), Some(&resolved));

        let file = home.final_roots_file();
        let mut cached: FinalRoots = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        cached
            .finals
            .insert(root.clone(), "C:/path/to/elsewhere".to_owned());
        fs::write(&file, serde_json::to_vec(&cached).unwrap()).unwrap();
        assert_eq!(
            final_roots(&home, &roots).get(&root).map(String::as_str),
            Some("C:/path/to/elsewhere"),
            "read back, not resolved again"
        );

        let other_root = other.to_string_lossy().into_owned();
        let both = BTreeSet::from([root.as_str(), other_root.as_str()]);
        assert_eq!(final_roots(&home, &both).get(&root), Some(&resolved));
    }

    #[test]
    fn a_target_that_does_not_exist_resolves_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("relay");
        fs::create_dir_all(&folder).unwrap();
        let resolved = final_target(&folder.join("new").join("file.json"));
        let expected = fs::canonicalize(&folder)
            .unwrap()
            .join("new")
            .join("file.json");
        assert_eq!(resolved, expected.to_string_lossy());
        assert_eq!(
            final_dir(&dir.path().join("absent").to_string_lossy()),
            None
        );
    }

    #[test]
    fn panic_gives_no_decision_and_records_gate_error() {
        let dir = tempfile::tempdir().unwrap();
        let home = RelayHome::new(dir.path(), None);
        let answer = answer_with(
            &home,
            &call(),
            &GateEnv::default(),
            crate::files::now_ms(),
            || -> io::Result<Evaluation> { panic!("the gate broke") },
        );
        assert_eq!(answer, None);
        let errors = gate_errors(&home);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].cause, GateFault::Panic);
        assert_eq!(errors[0].detail, "the gate broke");
        assert_eq!(errors[0].command_excerpt, None);
        assert_eq!(errors[0].session_id, call().session_id);
    }

    #[test]
    fn io_error_gives_no_decision() {
        let dir = tempfile::tempdir().unwrap();
        let home = RelayHome::new(dir.path(), None);
        let answer = answer_with(
            &home,
            &call(),
            &GateEnv::default(),
            crate::files::now_ms(),
            || Err(io::Error::other("disk gone")),
        );
        assert_eq!(answer, None);
        let errors = gate_errors(&home);
        assert_eq!(
            errors
                .iter()
                .map(|e| (e.cause, e.detail.as_str()))
                .collect::<Vec<_>>(),
            [(GateFault::Io, "disk gone")]
        );
    }

    #[test]
    fn an_unreachable_journal_does_not_change_the_answer() {
        let dir = tempfile::tempdir().unwrap();
        let blocked = dir.path().join("a-file");
        fs::write(&blocked, b"not a directory").unwrap();
        let home = RelayHome::new(&blocked, None);
        let deny = Evaluation {
            deny: Some(GateDeny {
                project: "lab".to_owned(),
                deny: Deny {
                    kind: officina_core::relay::gate::DenyKind::GuardedProgram,
                    matched: Some("ssh".to_owned()),
                },
                reason: "no".to_owned(),
            }),
            ..Evaluation::default()
        };
        let answer = answer_with(&home, &call(), &GateEnv::default(), 1, || Ok(deny));
        assert_eq!(answer, Some(deny_json("no")));
        let none = answer_with(&home, &call(), &GateEnv::default(), 1, || {
            panic!("and the journal is gone too")
        });
        assert_eq!(none, None);
    }

    #[test]
    fn only_gated_tool_calls_are_gated() {
        use officina_core::relay::gate::{FILE_TOOLS, SHELL_TOOLS};
        for (tool, field) in FILE_TOOLS {
            let with = serde_json::json!({"tool_name": tool, "tool_input": {*field: "C:/x/a.txt"}, "cwd": "C:/x"});
            let call = ToolCall::from_payload(&with).unwrap();
            assert_eq!(call.target.as_deref(), Some("C:/x/a.txt"), "{tool}");
            assert_eq!(call.command, None, "{tool}");
            let without = serde_json::json!({"tool_name": tool, "tool_input": {"content": "x"}});
            assert_eq!(
                ToolCall::from_payload(&without),
                None,
                "{tool}: no file, no decision"
            );
        }
        for (tool, rule) in SHELL_TOOLS {
            let with = serde_json::json!({"tool_name": tool, "tool_input": {"command": "ls"}, "cwd": "C:/x"});
            let call = ToolCall::from_payload(&with).unwrap();
            assert_eq!(call.command.as_deref(), Some("ls"), "{tool}");
            let without =
                serde_json::json!({"tool_name": tool, "tool_input": {"url": "wss://example.com"}});
            let gated = ToolCall::from_payload(&without).is_some();
            match rule {
                ToolRule::Lexed => assert!(!gated, "{tool}: no command, no decision"),
                ToolRule::DeniedInGuardedProject => assert!(gated, "{tool}: refused whole"),
            }
        }
        for other in [
            serde_json::json!({"tool_name": "Read", "tool_input": {"file_path": "x"}}),
            serde_json::json!({"tool_name": "Bash", "tool_input": {"command": 7}}),
            serde_json::json!({"tool_name": "bash", "tool_input": {"command": "ls"}}),
            serde_json::json!({"tool_input": {"command": "ls"}}),
        ] {
            assert_eq!(ToolCall::from_payload(&other), None, "{other}");
        }
    }
}
