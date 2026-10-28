//! The gate's decision (AD-26 steps 2 to 5, D-12 to D-17, D-173): in a
//! guarded repository, deny a `Bash` command that would start a guarded
//! program, hide its program, redirect Officina's home or environment or
//! touch Officina's own files; otherwise, and always outside a guarded
//! repository, decide nothing. Once the relay guards a project, deny a file
//! tool's write under the Officina home, from any project.
//! [`containment`] combines the floor, the approved snapshot and the
//! known-projects record into what is guarded, so that no file a session can
//! write lifts containment alone. The host reads those files and the payload
//! and journals each deny; this module only decides, from text.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::catalogue::{
    CanonicalCatalogue, DEFAULT_GUARDED_PROGRAMS, KnownProjects,
    guarded_programs as floor_guarded_programs,
};
use super::classes;
use super::journal::GateFault;
use super::shell::{self, Assignment, Lexed, Origin, Program};
use super::{HOME_DIR_NAME, JOURNAL_DIR, RELAY_DIR};
use crate::floor::{Autonomy, FloorFile, floor_dir_of, resolve_project_path};
use crate::quota::normalise_path;

/// Officina's own program, whose environment D-12 protects.
pub const OFFICINA_PROGRAM: &str = "officina";

/// The prefix of Officina's environment variables: any mention is denied
/// (AD-26 step 4), whatever the case, since Windows reads environment names
/// without case.
pub const OFFICINA_VARIABLE_PREFIX: &str = "OFFICINA_";

/// Variables that move Officina's home or floor when assigned in the same
/// command as an `officina` call (D-12). A trailing `*` is a prefix; names
/// compare without case, like Windows environment names.
pub const HOME_REDIRECTION_NAMES: &[&str] = &[
    "USERPROFILE",
    "HOME",
    "LOCALAPPDATA",
    "APPDATA",
    "OFFICINA_*",
];

/// Variables that decide which program `officina` and the programs it
/// starts run, or what reaches WSL, when assigned in the same command as an
/// `officina` call (D-173): belt and braces to `officina run` taking them
/// from the registry and resolving `git` there. A trailing `*` is a prefix;
/// names compare without case.
pub const OFFICINA_ENVIRONMENT_NAMES: &[&str] = &["PATH", "PATHEXT", "WSLENV", "ComSpec", "GIT_*"];

/// Longest deny reason, in bytes (AD-26).
pub const MAX_REASON_BYTES: usize = 2048;

/// The command every reason points to (D-17).
pub const LIST_COMMAND: &str = "officina run --list";

/// Longest project name or matched word quoted in a reason, in characters,
/// so the pointer to [`LIST_COMMAND`] always fits.
pub const MAX_QUOTED_CHARS: usize = 64;

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why the gate denied a command. Also the `expect` vocabulary of
    /// `tests/vectors/gate.json` and the journal's (story 2.4). When several
    /// apply, the first in this order is reported.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum DenyKind {
        /// The lexer cannot read the command.
        Unparsable,
        /// A guarded program in command position, or named in a scanned
        /// interpreter's text.
        GuardedProgram,
        /// PowerShell is given an encoded command, which cannot be scanned.
        EncodedCommand,
        /// A program is computed when the command runs (D-10).
        Dynamic,
        /// `officina` runs with its home or floor variables reassigned (D-12).
        HomeRedirection,
        /// `officina` runs with a variable of [`OFFICINA_ENVIRONMENT_NAMES`]
        /// reassigned (D-173).
        OfficinaEnvironment,
        /// The text names an `OFFICINA_` variable.
        OfficinaVariable,
        /// The text names Officina's relay or journal directory (D-13).
        ProtectedDir,
        /// A PowerShell tool call in a guarded project: Officina reads Bash
        /// commands only (D-171).
        PowershellInGuardedProject,
        /// A file tool writing under the Officina home while the relay
        /// guards a project (D-173).
        RelayStateWrite,
    }
}

closed_enum! {
    /// How the gate treats the calls of one shell tool (D-171).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ToolRule {
        /// Its `tool_input.command` is lexed and decided by [`decide`]; a
        /// call without one gets no decision.
        Lexed,
        /// Refused whole in a guarded project: its language is one Officina
        /// does not read, and the session keeps a lexed tool for commands.
        DeniedInGuardedProject,
    }
}

/// Every Claude Code tool that runs a shell command, each with its rule
/// (D-171); any other tool gets no decision. A tool whose language Officina
/// cannot read is refused whole in a guarded project rather than let
/// through: Claude Code turns the PowerShell tool on by default on Windows.
/// Doctor builds the `PreToolUse` matcher from this list.
pub const SHELL_TOOLS: &[(&str, ToolRule)] = &[
    ("Bash", ToolRule::Lexed),
    ("PowerShell", ToolRule::DeniedInGuardedProject),
    ("Monitor", ToolRule::Lexed),
];

/// Every Claude Code tool that writes a file, with the `tool_input` field
/// naming it (D-173): once the relay guards a project, the gate denies any
/// of them aimed under the Officina home, whose relay state and journal only
/// Officina writes. Doctor builds the `PreToolUse` matcher from this list
/// and [`SHELL_TOOLS`].
pub const FILE_TOOLS: &[(&str, &str)] = &[
    ("Write", "file_path"),
    ("Edit", "file_path"),
    ("MultiEdit", "file_path"),
    ("NotebookEdit", "notebook_path"),
];

/// The `tool_input` field naming the file `tool` writes, for a tool of
/// [`FILE_TOOLS`] named exactly as Claude Code names it.
pub fn file_tool_field(tool: &str) -> Option<&'static str> {
    FILE_TOOLS
        .iter()
        .find(|(name, _)| *name == tool)
        .map(|(_, field)| *field)
}

/// Every tool the gate decides: [`SHELL_TOOLS`], then [`FILE_TOOLS`].
pub fn gated_tools() -> Vec<&'static str> {
    SHELL_TOOLS
        .iter()
        .map(|(name, _)| *name)
        .chain(FILE_TOOLS.iter().map(|(name, _)| *name))
        .collect()
}

/// The rule of `tool`, named exactly as Claude Code names it; `None` for a
/// tool that runs no shell command.
pub fn tool_rule(tool: &str) -> Option<ToolRule> {
    SHELL_TOOLS
        .iter()
        .find(|(name, _)| *name == tool)
        .map(|(_, rule)| *rule)
}

/// The `PreToolUse` matcher naming every tool of [`gated_tools`].
///
/// ```
/// use officina_core::relay::gate::{FILE_TOOLS, SHELL_TOOLS, gated_tools_matcher};
/// let matcher = gated_tools_matcher();
/// assert_eq!(matcher.split('|').count(), SHELL_TOOLS.len() + FILE_TOOLS.len());
/// ```
pub fn gated_tools_matcher() -> String {
    gated_tools().join("|")
}

/// The gate's decision for a file tool's write (D-173): denied when the
/// relay guards a project (`relay_guards`) and `target` lies in the Officina
/// home `home`, the folder itself or anything under it, whatever project the
/// call comes from. The host resolves both paths by handle first (8.3 names,
/// junctions, links, `subst`), so they compare as written here.
///
/// ```
/// use officina_core::relay::gate::{DenyKind, decide_file_write};
/// let home = "C:/Users/user01/.officina";
/// let deny = decide_file_write(r"C:\Users\user01\.officina\relay\stopped", home, true).unwrap();
/// assert_eq!(deny.kind, DenyKind::RelayStateWrite);
/// assert_eq!(decide_file_write("C:/Users/user01/notes.md", home, true), None);
/// assert_eq!(decide_file_write("C:/Users/user01/.officina/x", home, false), None);
/// ```
pub fn decide_file_write(target: &str, home: &str, relay_guards: bool) -> Option<Deny> {
    if !relay_guards {
        return None;
    }
    let target = file_path_text(target);
    let home = file_path_text(home);
    let home = home.trim_end_matches('/');
    let inside = !home.is_empty()
        && (target == home || target.starts_with(home) && target[home.len()..].starts_with('/'));
    inside.then_some(Deny {
        kind: DenyKind::RelayStateWrite,
        matched: None,
    })
}

/// A file path folded for [`decide_file_write`]: [`normalise_cwd`]'s form,
/// with the device namespace's UNC spelling (`\\?\UNC\host\share`) as
/// the plain one (`//host/share`), repeated slashes and `.` segments
/// dropped, `..` applied, and each segment's trailing dots and spaces
/// dropped: what Windows does to a path before it opens the file.
fn file_path_text(path: &str) -> String {
    let slashed = path.replace('\\', "/");
    let unc = ["//?/unc/", "//./unc/"].iter().find_map(|prefix| {
        slashed
            .get(..prefix.len())
            .filter(|head| head.eq_ignore_ascii_case(prefix))
            .map(|_| format!("//{}", &slashed[prefix.len()..]))
    });
    let normal = normalise_cwd(unc.as_deref().unwrap_or(&slashed));
    let (root, rest) = match normal.strip_prefix("//") {
        Some(rest) => ("//", rest),
        None => ("", normal.as_str()),
    };
    let mut segments: Vec<&str> = Vec::new();
    for segment in rest.split('/') {
        let segment = if segment.chars().all(|c| c == '.') {
            segment
        } else {
            segment.trim_end_matches(['.', ' '])
        };
        match segment {
            "" | "." => {}
            // Never above the drive or the host.
            ".." => {
                if segments.len() > 1 {
                    segments.pop();
                }
            }
            other => segments.push(other),
        }
    }
    format!("{root}{}", segments.join("/"))
}

/// The gate's decision for one tool call (AD-26, D-171): the rule of `tool`
/// applied to its `command`. A tool that runs no shell command, and a lexed
/// tool's call without a command, get no decision.
///
/// ```
/// use officina_core::relay::gate::{DenyKind, decide_call};
/// let list = vec!["ssh".to_owned()];
/// assert_eq!(decide_call("Read", Some("ssh host01"), &list, &[], true), None);
/// let deny = decide_call("PowerShell", Some("Get-ChildItem"), &list, &[], true).unwrap();
/// assert_eq!(deny.kind, DenyKind::PowershellInGuardedProject);
/// assert_eq!(decide_call("PowerShell", Some("Get-ChildItem"), &list, &[], false), None);
/// ```
pub fn decide_call(
    tool: &str,
    command: Option<&str>,
    guarded_programs: &[String],
    protected_dirs: &[String],
    cwd_guarded: bool,
) -> Option<Deny> {
    match tool_rule(tool)? {
        ToolRule::Lexed => decide(command?, guarded_programs, protected_dirs, cwd_guarded),
        ToolRule::DeniedInGuardedProject if cwd_guarded => {
            Deny::new(DenyKind::PowershellInGuardedProject, None)
        }
        ToolRule::DeniedInGuardedProject => None,
    }
}

/// A deny: its kind and, for a guarded program, the program; for a home
/// redirection, the variable when its name is known.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deny {
    pub kind: DenyKind,
    pub matched: Option<String>,
}

impl Deny {
    fn new(kind: DenyKind, matched: Option<String>) -> Option<Deny> {
        Some(Deny { kind, matched })
    }
}

/// The gate's decision for one `Bash` command (AD-26 steps 3 to 5): `None`
/// is no decision. Outside a guarded repository it is always `None`, before
/// any work. `guarded_programs` is the effective list (folded entries, a
/// trailing `*` a prefix, an entry ending in `.exe` matched on the written
/// form, D-11); `protected_dirs` comes from [`protected_dirs`].
///
/// ```
/// use officina_core::relay::gate::{DenyKind, decide};
/// let list = vec!["ssh".to_owned()];
/// let deny = decide("cd lab && \"ss\"h host01", &list, &[], true).unwrap();
/// assert_eq!(deny.kind, DenyKind::GuardedProgram);
/// assert_eq!(deny.matched.as_deref(), Some("ssh"));
/// assert_eq!(decide("ssh host01", &list, &[], false), None);
/// assert_eq!(decide("officina run check group=lab-web | head", &list, &[], true), None);
/// ```
pub fn decide(
    command: &str,
    guarded_programs: &[String],
    protected_dirs: &[String],
    cwd_guarded: bool,
) -> Option<Deny> {
    if !cwd_guarded {
        return None;
    }
    let Ok(lexed) = shell::lex(command) else {
        return Deny::new(DenyKind::Unparsable, None);
    };
    // Folded once here, not once per program the line starts.
    let folded: Vec<String> = guarded_programs.iter().map(|e| shell::fold(e)).collect();
    if let Some(matched) = lexed
        .programs
        .iter()
        .find_map(|program| guarded_match(program, &folded))
    {
        return Deny::new(DenyKind::GuardedProgram, Some(matched));
    }
    if lexed.encoded_command {
        return Deny::new(DenyKind::EncodedCommand, None);
    }
    if !lexed.dynamic.is_empty() {
        return Deny::new(DenyKind::Dynamic, None);
    }
    if let Some(deny) = reassigned(&lexed) {
        return Some(deny);
    }
    let prefix = shell::fold(OFFICINA_VARIABLE_PREFIX);
    if shell::fold(command).contains(&prefix) || shell::fold(&lexed.values).contains(&prefix) {
        return Deny::new(DenyKind::OfficinaVariable, None);
    }
    if names_protected_dir(command, protected_dirs)
        || names_protected_dir(&lexed.values, protected_dirs)
    {
        return Deny::new(DenyKind::ProtectedDir, None);
    }
    None
}

/// The guarded entry a program matches, as the deny reports it: the program
/// name for a plain or prefix entry, the entry for a written-form one.
///
/// An entry ending in `.exe` (D-11's `bash.exe`) matches the word as written,
/// before `.exe` is stripped: the explicit `.exe` spelling, or the bare name
/// under a `windows` directory (`C:/Windows/System32/bash`). A bare `bash`
/// stays Git Bash's shell, which the lexer reads through.
pub fn guarded_match(program: &Program, guarded_programs: &[String]) -> Option<String> {
    let name = shell::program_name(&program.written);
    guarded_programs.iter().find_map(|entry| {
        // An entry already folded (as `decide` passes them) is used as is.
        let folded = entry.is_ascii() && !entry.bytes().any(|b| b.is_ascii_uppercase());
        let entry = if folded {
            std::borrow::Cow::Borrowed(entry.as_str())
        } else {
            std::borrow::Cow::Owned(shell::fold(entry))
        };
        if let Some(stem) = entry.strip_suffix(".exe") {
            let written = shell::written_name(&program.written) == entry;
            let system = name == stem
                && program.origin == Origin::Command
                && shell::under_windows_dir(&program.written);
            (written || system).then(|| entry.into_owned())
        } else {
            shell::name_matches(&entry, &name).then(|| name.clone())
        }
    })
}

/// A variable of `names` assigned in `lexed`, as the deny of `kind`
/// reports it: by name, or a computed name, which could be any. A scanned
/// interpreter can set a variable in its own syntax (`$env:HOME`, `set
/// HOME=`, `os.environ['HOME']`): the name is enough.
fn assigned(lexed: &Lexed, names: &[&str], kind: DenyKind) -> Option<Deny> {
    let listed = |name: &str| names.iter().any(|entry| shell::name_matches(entry, name));
    for assignment in &lexed.assignments {
        match assignment {
            Assignment::Name(name) if listed(name) => return Deny::new(kind, Some(name.clone())),
            Assignment::Computed => return Deny::new(kind, None),
            Assignment::Name(_) => {}
        }
    }
    lexed
        .programs
        .iter()
        .find(|p| p.origin == Origin::Scanned && listed(&p.written))
        .and_then(|p| Deny::new(kind, Some(p.written.clone())))
}

/// D-12 and D-173: in a command calling `officina`, a home or floor
/// variable reassigned, then one of its environment's.
fn reassigned(lexed: &Lexed) -> Option<Deny> {
    // The name is derived only for words that can hold it: folding every
    // program of a long line would cost the gate its budget (AD-28).
    let officina = lexed.programs.iter().any(|p| {
        p.written.len() >= OFFICINA_PROGRAM.len()
            && shell::program_name(&p.written) == OFFICINA_PROGRAM
    });
    if !officina {
        return None;
    }
    assigned(lexed, HOME_REDIRECTION_NAMES, DenyKind::HomeRedirection).or_else(|| {
        assigned(
            lexed,
            OFFICINA_ENVIRONMENT_NAMES,
            DenyKind::OfficinaEnvironment,
        )
    })
}

/// The protected directories of a home (D-13): `<home>/relay` and
/// `<home>/journal`, normalised, plus the `.officina/relay` and
/// `.officina/journal` tails when the home is the default one. A bare
/// `relay/` is never protected.
pub fn protected_dirs(home: &str, default_home: bool) -> Vec<String> {
    let home = path_text(home);
    let home = home.trim_end_matches('/');
    let mut dirs: Vec<String> = [RELAY_DIR, JOURNAL_DIR]
        .iter()
        .map(|dir| format!("{home}/{dir}"))
        .collect();
    if default_home {
        dirs.extend(
            [RELAY_DIR, JOURNAL_DIR]
                .iter()
                .map(|dir| format!("{HOME_DIR_NAME}/{dir}")),
        );
    }
    dirs
}

/// True when `text` names a protected directory with either slash, any case,
/// repeated or `.` segments, and for a drive path also its Git Bash
/// (`/c/...`), Cygwin (`/cygdrive/c/...`) and administrative share
/// (`c$/...`) spellings. The name must end at a path boundary, and a tail
/// must also start at one.
fn names_protected_dir(text: &str, protected_dirs: &[String]) -> bool {
    if protected_dirs.is_empty() {
        return false;
    }
    let haystack = path_text(text);
    protected_dirs.iter().any(|dir| {
        let dir = path_text(dir);
        let absolute = drive_letter(&dir).is_some() || dir.starts_with('/');
        spellings(&dir)
            .iter()
            .any(|needle| contains_bounded(&haystack, needle, !absolute))
    })
}

fn drive_letter(path: &str) -> Option<char> {
    let mut chars = path.chars();
    let letter = chars.next().filter(char::is_ascii_alphabetic)?;
    (chars.next() == Some(':') && matches!(chars.next(), Some('/') | None)).then_some(letter)
}

fn spellings(dir: &str) -> Vec<String> {
    let mut all = vec![dir.to_owned()];
    if let Some(letter) = drive_letter(dir) {
        let rest = &dir[2..];
        all.push(format!("/{letter}{rest}"));
        all.push(format!("/cygdrive/{letter}{rest}"));
        all.push(format!("{letter}${rest}"));
    }
    all
}

fn path_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.')
}

fn contains_bounded(haystack: &str, needle: &str, check_start: bool) -> bool {
    haystack.match_indices(needle).any(|(i, _)| {
        let after = haystack[i + needle.len()..].chars().next();
        let before = haystack[..i].chars().next_back();
        after.is_none_or(|c| !path_char(c))
            && (!check_start || before.is_none_or(|c| c == '/' || !path_char(c)))
    })
}

/// Text folded for path comparison: case folded, `\` as `/`, runs of `/`
/// collapsed, `/./` segments dropped. Works on bytes: `/`, `\` and `.` are
/// ASCII, never part of a multi-byte character, so dropping them keeps the
/// text valid UTF-8.
fn path_text(text: &str) -> String {
    let folded = shell::fold(text).into_bytes();
    let slash = |byte: Option<&u8>| matches!(byte, Some(b'/' | b'\\'));
    let mut out = Vec::with_capacity(folded.len());
    let mut i = 0;
    while let Some(&byte) = folded.get(i) {
        i += 1;
        if byte != b'/' && byte != b'\\' {
            out.push(byte);
            continue;
        }
        loop {
            if slash(folded.get(i)) {
                i += 1;
            } else if folded.get(i) == Some(&b'.') && slash(folded.get(i + 1)) {
                i += 2;
            } else {
                break;
            }
        }
        out.push(b'/');
    }
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// One form for every spelling of a working directory (AD-26 step 2):
/// `C:\x`, `C:/x` and Git Bash's `/c/x` all give `c:/x`.
pub fn normalise_cwd(path: &str) -> String {
    let slashed = path.replace('\\', "/");
    let slashed = slashed
        .strip_prefix("//?/")
        .or_else(|| slashed.strip_prefix("//./"))
        .unwrap_or(&slashed);
    let bytes = slashed.as_bytes();
    let git_bash = |skip: usize| {
        let letter = bytes.get(skip).filter(|b| b.is_ascii_alphabetic())?;
        matches!(bytes.get(skip + 1), Some(b'/') | None).then(|| {
            format!(
                "{}:/{}",
                char::from(*letter),
                &slashed[(skip + 2).min(slashed.len())..]
            )
        })
    };
    let drive_form = if slashed.starts_with("/cygdrive/") {
        git_bash("/cygdrive/".len())
    } else if slashed.starts_with('/') {
        git_bash(1)
    } else {
        None
    };
    normalise_path(drive_form.as_deref().unwrap_or(slashed))
}

/// A project as the gate sees it: its path and its autonomy, from one
/// source (the current floor or the approved snapshot).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRoot {
    pub name: String,
    pub path: String,
    pub autonomy: Autonomy,
}

/// The project `cwd` lies in, by longest path prefix on segment boundaries,
/// when that project is guarded (AD-26 steps 2 and 3). Two projects of the
/// same path: guarded if either is. Combining the floor's and the snapshot's
/// sets is the caller's (D-14 to D-16).
pub fn guarded_project<'p>(cwd: &str, projects: &'p [ProjectRoot]) -> Option<&'p ProjectRoot> {
    let cwd = normalise_cwd(cwd);
    let containing: Vec<(usize, &ProjectRoot)> = projects
        .iter()
        .filter_map(|project| {
            let root = normalise_cwd(&project.path);
            let inside = !root.is_empty()
                && (cwd == root || cwd.starts_with(&root) && cwd[root.len()..].starts_with('/'));
            inside.then_some((root.len(), project))
        })
        .collect();
    let longest = containing.iter().map(|(len, _)| *len).max()?;
    containing
        .into_iter()
        .filter(|(len, _)| *len == longest)
        .map(|(_, project)| project)
        .find(|project| project.autonomy == Autonomy::Guarded)
}

/// An action the deny reason offers (D-17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferedAction {
    pub name: String,
    /// Parameter names, in order.
    pub params: Vec<String>,
    /// False when the action's class is not available in this build.
    pub available: bool,
}

/// What the reason can offer (D-17).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offer<'a> {
    /// The approved snapshot's actions for the project.
    Approved(&'a [OfferedAction]),
    /// No approved snapshot: the action names the floor's project entry
    /// declares.
    AwaitingApproval(&'a [String]),
    /// Neither the floor nor the snapshot names the project: it is guarded
    /// from the known-projects record alone (D-15).
    Unreadable,
}

/// The reason Claude Code shows the session (AD-26, D-17): what was denied
/// and where, the pointer to [`LIST_COMMAND`], then the actions to use
/// instead as `officina run <name> <param>=<...>`, unavailable classes
/// marked; at most [`MAX_REASON_BYTES`], cut on a character boundary.
///
/// A file write (D-173) is refused from any project: its reason names no
/// project and offers no action when `project` is empty.
pub fn deny_reason(deny: &Deny, project: &str, offer: Offer<'_>) -> String {
    let mut text = if deny.kind == DenyKind::RelayStateWrite {
        format!(
            "Officina refused this file write: {}. Run `{LIST_COMMAND}` to see the actions you may use and their allowed values.",
            why(deny)
        )
    } else {
        format!(
            "Officina refused this command in the guarded project {}: {}. Run `{LIST_COMMAND}` to see the actions you may use and their allowed values.",
            quoted(project),
            why(deny)
        )
    };
    if deny.kind == DenyKind::RelayStateWrite && project.is_empty() {
        return cut(text, MAX_REASON_BYTES);
    }
    match offer {
        Offer::Approved([]) => text.push_str("\nThis project has no relay action."),
        Offer::Approved(actions) => {
            text.push_str("\nActions of this project:");
            for action in actions {
                text.push_str("\n  officina run ");
                text.push_str(&action.name);
                for param in &action.params {
                    text.push(' ');
                    text.push_str(param);
                    text.push_str("=<...>");
                }
                if !action.available {
                    text.push_str("  (class not available in this build)");
                }
            }
        }
        Offer::AwaitingApproval([]) => text.push_str(
            "\nThe catalogue awaits approval in the Officina window, and the floor declares no action for this project.",
        ),
        Offer::AwaitingApproval(names) => {
            text.push_str(
                "\nThe catalogue awaits approval in the Officina window; the floor declares these actions: ",
            );
            text.push_str(&names.join(", "));
            text.push('.');
        }
        Offer::Unreadable => text.push_str(
            "\nOfficina cannot read its floor or its approved catalogue, so every project it knows is guarded; the author can check the setup with `officina doctor`.",
        ),
    }
    cut(text, MAX_REASON_BYTES)
}

/// The relay floor as the gate reads it: phase 1 only (AD-23, AD-28).
#[derive(Debug, Clone, Copy)]
pub enum FloorInput<'a> {
    Missing,
    Unreadable,
    Invalid,
    /// `path` is the floor's path as given, for its relative project paths.
    Valid {
        floor: &'a FloorFile,
        path: &'a str,
    },
}

/// `relay/approved.json` as the gate reads it.
#[derive(Debug, Clone, Copy)]
pub enum SnapshotInput<'a> {
    Missing,
    Unreadable,
    Valid(&'a CanonicalCatalogue),
}

/// One source's projects, looked up on their own so a nested project in one
/// source cannot hide a guarded parent in another.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Source {
    projects: Vec<ProjectRoot>,
}

/// What is guarded for one `Bash` call, from every source the gate could
/// read (D-14, D-15): the floor's and the snapshot's projects are looked up
/// separately and a call is guarded if either says so, the guarded programs
/// are the union of both lists, and a source that cannot be read makes the
/// gate stricter, never looser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Containment {
    /// Looked up in order: snapshot, floor, known projects. The first source
    /// that guards the call names the project.
    sources: Vec<Source>,
    /// The effective list [`decide`] takes: sorted, deduplicated.
    pub guarded_programs: Vec<String>,
    /// What could not be read, each one a `gate_error` line.
    pub faults: Vec<GateFault>,
    approved_offers: BTreeMap<String, Vec<OfferedAction>>,
    floor_actions: BTreeMap<String, Vec<String>>,
}

/// Combines the gate's inputs into what is guarded (AD-26 step 2, D-14,
/// D-15):
///
/// - a valid snapshot contributes its projects, each with its approved
///   autonomy, and its guarded programs;
/// - a valid floor contributes its projects and its list (defaults
///   applied); when the snapshot is broken (unreadable, or missing while the
///   known-projects record names an approved hash) every floor project is
///   guarded and the default list is added;
/// - when the floor fails while the relay is in use (a snapshot exists or
///   the known-projects record does), every known project path is guarded
///   with the default list.
///
/// With nothing configured (no floor, no snapshot, no record) nothing is
/// guarded and nothing is a fault: the relay is simply not in use.
pub fn containment(
    floor: FloorInput<'_>,
    snapshot: SnapshotInput<'_>,
    known: Option<&KnownProjects>,
) -> Containment {
    let mut faults = Vec::new();
    let recorded_approval = known.is_some_and(|k| k.approved_hash.is_some());
    let snapshot_broken = match snapshot {
        SnapshotInput::Unreadable => {
            faults.push(GateFault::ApprovedUnreadable);
            true
        }
        SnapshotInput::Missing if recorded_approval => {
            faults.push(GateFault::ApprovedMissing);
            true
        }
        SnapshotInput::Missing | SnapshotInput::Valid(_) => false,
    };
    let relay_in_use = !matches!(snapshot, SnapshotInput::Missing) || known.is_some();
    let floor_failed = match floor {
        FloorInput::Valid { .. } => None,
        FloorInput::Missing => Some(GateFault::FloorMissing),
        FloorInput::Unreadable => Some(GateFault::FloorUnreadable),
        FloorInput::Invalid => Some(GateFault::FloorInvalid),
    }
    .filter(|_| relay_in_use);
    faults.extend(floor_failed);

    let mut sources = Vec::new();
    let mut lists: Vec<String> = Vec::new();
    let mut approved_offers = BTreeMap::new();
    let mut floor_actions = BTreeMap::new();

    if let SnapshotInput::Valid(canonical) = snapshot {
        sources.push(Source {
            projects: canonical
                .projects
                .iter()
                .map(|(name, project)| ProjectRoot {
                    name: name.clone(),
                    path: project.path.clone(),
                    autonomy: project.autonomy,
                })
                .collect(),
        });
        lists.extend(canonical.guarded_programs.iter().cloned());
        for (name, project) in &canonical.projects {
            let offered = project
                .actions
                .iter()
                .filter_map(|action| {
                    let canonical_action = canonical.actions.get(action)?;
                    Some(OfferedAction {
                        name: action.clone(),
                        params: canonical_action.params.keys().cloned().collect(),
                        available: classes::is_available(canonical_action.class),
                    })
                })
                .collect();
            approved_offers.insert(name.clone(), offered);
        }
    }
    if let FloorInput::Valid { floor, path } = floor {
        let floor_dir = floor_dir_of(path);
        sources.push(Source {
            projects: floor
                .projects
                .iter()
                .map(|project| ProjectRoot {
                    name: project.name.clone(),
                    path: resolve_project_path(floor_dir, &project.path),
                    autonomy: if snapshot_broken {
                        Autonomy::Guarded
                    } else {
                        floor.autonomy_of(project)
                    },
                })
                .collect(),
        });
        lists.extend(floor_guarded_programs(floor));
        for project in &floor.projects {
            floor_actions
                .entry(project.name.clone())
                .or_insert_with(|| project.actions.clone().unwrap_or_default());
        }
    }
    if floor_failed.is_some() {
        match known {
            Some(known) => sources.push(Source {
                projects: known
                    .project_paths
                    .iter()
                    .map(|path| ProjectRoot {
                        name: path.clone(),
                        path: path.clone(),
                        autonomy: Autonomy::Guarded,
                    })
                    .collect(),
            }),
            None if snapshot_broken => faults.push(GateFault::KnownProjectsUnreadable),
            None => {}
        }
    }
    if snapshot_broken || floor_failed.is_some() {
        lists.extend(DEFAULT_GUARDED_PROGRAMS.iter().map(|p| (*p).to_owned()));
    }
    lists.sort();
    lists.dedup();
    Containment {
        sources,
        guarded_programs: lists,
        faults,
        approved_offers,
        floor_actions,
    }
}

impl Containment {
    /// True when any source guards a project: what makes file writes under
    /// the Officina home denied, from any project (D-173).
    pub fn guards_any(&self) -> bool {
        self.sources.iter().any(|source| {
            source
                .projects
                .iter()
                .any(|project| project.autonomy == Autonomy::Guarded)
        })
    }

    /// The roots of the guarded projects of every source, as written, each
    /// once: what the host resolves to final paths (D-173).
    pub fn guarded_roots(&self) -> BTreeSet<&str> {
        self.sources
            .iter()
            .flat_map(|source| &source.projects)
            .filter(|project| project.autonomy == Autonomy::Guarded)
            .map(|project| project.path.as_str())
            .collect()
    }

    /// Looks each guarded project up at the final path of its root too
    /// (D-173): `finals` maps a root as written to the path an opened handle
    /// gives, so a root written through an 8.3 name, a junction or a `subst`
    /// drive matches a folder resolved the same way. Guarded projects only:
    /// a guarded root added anywhere can only make the gate stricter, so a
    /// `finals` read from a file a session can write cannot loosen it. A
    /// root without a final path keeps its written form alone.
    pub fn add_final_roots(&mut self, finals: &BTreeMap<String, String>) {
        for source in &mut self.sources {
            let resolved: Vec<ProjectRoot> = source
                .projects
                .iter()
                .filter(|project| project.autonomy == Autonomy::Guarded)
                .filter_map(|project| {
                    let path = finals.get(&project.path)?;
                    (normalise_cwd(path) != normalise_cwd(&project.path)).then(|| ProjectRoot {
                        path: path.clone(),
                        ..project.clone()
                    })
                })
                .collect();
            source.projects.extend(resolved);
        }
    }

    /// The guarded project of the first directory any source guards (D-16:
    /// the session's project root or the call's `cwd`), sources in order.
    pub fn guarded_project(&self, dirs: &[&str]) -> Option<&ProjectRoot> {
        self.sources.iter().find_map(|source| {
            dirs.iter()
                .find_map(|dir| guarded_project(dir, &source.projects))
        })
    }

    /// What the deny reason offers for `project` (D-17): its approved
    /// actions, else the floor entry's action names, else nothing readable.
    pub fn offer_for(&self, project: &str) -> Offer<'_> {
        if let Some(actions) = self.approved_offers.get(project) {
            Offer::Approved(actions)
        } else if let Some(names) = self.floor_actions.get(project) {
            Offer::AwaitingApproval(names)
        } else {
            Offer::Unreadable
        }
    }
}

fn why(deny: &Deny) -> String {
    let matched = deny.matched.as_deref().map(quoted);
    match deny.kind {
        DenyKind::Unparsable => format!(
            "the command cannot be read (an unclosed quote or substitution, more than {} KiB, or text nested more than {} levels deep)",
            shell::MAX_INPUT_BYTES / 1024,
            shell::MAX_DEPTH
        ),
        DenyKind::GuardedProgram => format!(
            "`{}` is a guarded program",
            matched.unwrap_or_default()
        ),
        DenyKind::EncodedCommand => {
            "PowerShell's encoded command cannot be checked".to_owned()
        }
        DenyKind::Dynamic => "a program name is computed when the command runs (an expansion, substitution, glob, brace, ANSI-C quoting, or a program read from a pipe); write the program literally".to_owned(),
        DenyKind::HomeRedirection => format!(
            "`{OFFICINA_PROGRAM}` would run with {} reassigned, which can point it at another home or floor",
            matched.map_or_else(|| "a computed variable".to_owned(), |m| format!("`{m}`"))
        ),
        DenyKind::OfficinaEnvironment => format!(
            "`{OFFICINA_PROGRAM}` would run with {} reassigned, which decides which programs it and its actions start; `{OFFICINA_PROGRAM} run` takes them from the registry",
            matched.map_or_else(|| "a computed variable".to_owned(), |m| format!("`{m}`"))
        ),
        DenyKind::OfficinaVariable => format!(
            "the command names an `{OFFICINA_VARIABLE_PREFIX}` variable, one of Officina's own settings"
        ),
        DenyKind::ProtectedDir => {
            "the command names Officina's relay or journal directory, which only the Officina window writes".to_owned()
        }
        DenyKind::PowershellInGuardedProject => {
            "Officina reads Bash commands only, so the PowerShell tool is refused in a guarded project; run the command with the Bash tool".to_owned()
        }
        DenyKind::RelayStateWrite => {
            "the file lies in Officina's home, whose relay state and journal only Officina writes; the author edits Officina's files outside a session".to_owned()
        }
    }
}

fn quoted(text: &str) -> String {
    let mut chars = text.chars();
    let mut out: String = chars.by_ref().take(MAX_QUOTED_CHARS).collect();
    if chars.next().is_some() {
        out.push_str("...");
    }
    out
}

fn cut(mut text: String, max: usize) -> String {
    if text.len() > max {
        let mut end = max;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str, path: &str, autonomy: Autonomy) -> ProjectRoot {
        ProjectRoot {
            name: name.to_owned(),
            path: path.to_owned(),
            autonomy,
        }
    }

    #[test]
    fn cwd_forms_normalise_alike() {
        let forms = [
            r"C:\Work\Lab",
            "C:/Work/Lab",
            "c:/work/lab/",
            "/c/Work/Lab",
            "/cygdrive/c/Work/Lab",
            r"\\?\C:\Work\Lab",
        ];
        for form in forms {
            assert_eq!(normalise_cwd(form), "c:/work/lab", "{form}");
        }
        assert_eq!(normalise_cwd("/c"), normalise_cwd(r"C:\"));
        assert_eq!(normalise_cwd("/usr/local"), "/usr/local");
    }

    #[test]
    fn longest_prefix_wins() {
        let projects = [
            project("work", "C:/Work", Autonomy::Guarded),
            project("sandbox", r"C:\Work\Sandbox", Autonomy::Free),
            project("lab", "c:/work/lab", Autonomy::Guarded),
        ];
        assert_eq!(
            guarded_project(r"C:\Work\Lab\roles", &projects).map(|p| p.name.as_str()),
            Some("lab")
        );
        assert_eq!(guarded_project("/c/work/sandbox/x", &projects), None);
        assert_eq!(
            guarded_project("C:/Work/Sandbox2", &projects).map(|p| p.name.as_str()),
            Some("work"),
            "a prefix counts on segment boundaries only"
        );
        let twins = [
            project("free-twin", "C:/Same", Autonomy::Free),
            project("guarded-twin", "c:/same", Autonomy::Guarded),
        ];
        assert_eq!(
            guarded_project("C:/Same", &twins).map(|p| p.name.as_str()),
            Some("guarded-twin")
        );
    }

    #[test]
    fn outside_every_project_is_none() {
        let projects = [project("lab", "C:/Work/Lab", Autonomy::Guarded)];
        assert_eq!(guarded_project("C:/Work", &projects), None);
        assert_eq!(guarded_project("D:/Work/Lab", &projects), None);
        assert_eq!(guarded_project("C:/Work/Labs", &projects), None);
        assert_eq!(guarded_project("C:/Work/Lab", &[]), None);
    }

    fn deny(kind: DenyKind, matched: Option<&str>) -> Deny {
        Deny {
            kind,
            matched: matched.map(str::to_owned),
        }
    }

    fn action(name: &str, params: &[&str], available: bool) -> OfferedAction {
        OfferedAction {
            name: name.to_owned(),
            params: params.iter().map(|p| (*p).to_owned()).collect(),
            available,
        }
    }

    #[test]
    fn reason_lists_actions_and_the_list_command() {
        let actions = [
            action("check", &["group", "playbook"], true),
            action("status", &["host"], true),
        ];
        let reason = deny_reason(
            &deny(DenyKind::GuardedProgram, Some("ansible-playbook")),
            "supervision",
            Offer::Approved(&actions),
        );
        assert!(reason.contains("`ansible-playbook`"), "{reason}");
        assert!(reason.contains("supervision"), "{reason}");
        assert!(reason.contains(LIST_COMMAND), "{reason}");
        assert!(
            reason.contains("officina run check group=<...> playbook=<...>"),
            "{reason}"
        );
        assert!(
            reason.contains("officina run status host=<...>"),
            "{reason}"
        );

        let awaiting = deny_reason(
            &deny(DenyKind::Dynamic, None),
            "supervision",
            Offer::AwaitingApproval(&["check".to_owned(), "apply".to_owned()]),
        );
        assert!(awaiting.contains("awaits approval"), "{awaiting}");
        assert!(awaiting.contains("check, apply"), "{awaiting}");
        assert!(awaiting.contains(LIST_COMMAND), "{awaiting}");
        for kind in DenyKind::ALL {
            let reason = deny_reason(&deny(*kind, Some("x")), "p", Offer::Approved(&[]));
            assert!(reason.contains(LIST_COMMAND), "{kind:?}");
        }
    }

    #[test]
    fn reason_marks_unavailable_classes() {
        let actions = [
            action("check", &["group"], true),
            action("apply-sensitive", &["playbook"], false),
        ];
        let reason = deny_reason(
            &deny(DenyKind::GuardedProgram, Some("ssh")),
            "lab",
            Offer::Approved(&actions),
        );
        let lines: Vec<&str> = reason.lines().collect();
        let check = lines.iter().find(|l| l.contains("run check")).unwrap();
        let sensitive = lines
            .iter()
            .find(|l| l.contains("run apply-sensitive"))
            .unwrap();
        assert!(!check.contains("not available"), "{reason}");
        assert!(
            sensitive.contains("not available in this build"),
            "{reason}"
        );
    }

    #[test]
    fn reason_is_cut_at_2_kib_on_a_char_boundary() {
        let actions: Vec<OfferedAction> = (0..200)
            .map(|i| action(&format!("action-{i}"), &["parameter"], i % 2 == 0))
            .collect();
        let project = "é".repeat(500);
        for shift in 0..4 {
            let name = format!("{}{project}", "a".repeat(shift));
            let reason = deny_reason(
                &deny(DenyKind::GuardedProgram, Some("ssh")),
                &name,
                Offer::Approved(&actions),
            );
            assert!(reason.len() <= MAX_REASON_BYTES, "{}", reason.len());
            assert!(reason.len() > MAX_REASON_BYTES - 4, "{}", reason.len());
            assert!(reason.contains(LIST_COMMAND));
        }
        let multibyte: Vec<OfferedAction> = (0..400)
            .map(|i| action(&format!("é{i}"), &[], true))
            .collect();
        let reason = deny_reason(
            &deny(DenyKind::Dynamic, None),
            "p",
            Offer::Approved(&multibyte),
        );
        assert!(reason.len() <= MAX_REASON_BYTES);
        assert!(reason.len() > MAX_REASON_BYTES - 4);
    }

    #[test]
    fn gate_modules_import_no_regex_or_hash() {
        for (module, source) in [
            ("shell.rs", include_str!("shell.rs")),
            ("gate.rs", include_str!("gate.rs")),
        ] {
            let code = source.split("#[cfg(test)]").next().unwrap_or_default();
            for forbidden in [
                "regex", "Regex", "sha2", "Sha256", "sha256", "Digest", "fnv1a",
            ] {
                assert!(
                    !code.contains(forbidden),
                    "{module} mentions `{forbidden}` outside its tests (AD-28)"
                );
            }
        }
    }

    const FLOOR_PATH: &str = "C:/path/to/officina.toml";

    fn floor(text: &str) -> FloorFile {
        crate::floor::parse_floor(text).unwrap()
    }

    fn snapshot_of(text: &str) -> CanonicalCatalogue {
        crate::relay::catalogue::load(text, FLOOR_PATH)
            .unwrap()
            .canonical
    }

    const GUARDED_LAB: &str = "schema = 2\n[[project]]\nname = \"lab\"\npath = \"Lab\"\nactions = [\"probe\"]\n[[project]]\nname = \"notes\"\npath = \"Notes\"\nautonomy = \"free\"\n[[action]]\nname = \"probe\"\nclass = \"free\"\nprogram = \"C:/path/to/tool.exe\"\nargs = [\"{host}\"]\n[action.params.host]\nvalues = [\"web01\"]\n";

    fn known(paths: &[&str], approved: bool) -> KnownProjects {
        KnownProjects {
            schema: crate::relay::catalogue::KNOWN_PROJECTS_SCHEMA,
            approved_hash: approved.then(|| "00".to_owned()),
            project_paths: paths.iter().map(|p| (*p).to_owned()).collect(),
        }
    }

    fn name_for<'c>(c: &'c Containment, dirs: &[&str]) -> Option<&'c str> {
        c.guarded_project(dirs).map(|p| p.name.as_str())
    }

    #[test]
    fn nothing_configured_guards_nothing_and_is_no_fault() {
        let c = containment(FloorInput::Missing, SnapshotInput::Missing, None);
        assert_eq!(c.guarded_project(&["C:/path/to/Lab"]), None);
        assert!(c.faults.is_empty() && c.guarded_programs.is_empty());
        let broken = containment(FloorInput::Invalid, SnapshotInput::Missing, None);
        assert!(
            broken.faults.is_empty(),
            "a broken floor of a relay never used is the board's error, not the gate's"
        );
    }

    /// D-173: a guarded root written through an alias matches the folder it
    /// resolves to once its final path is added, and still matches as
    /// written; a free project's final path is never added, so a forged one
    /// cannot carve a free folder out of a guarded project.
    #[test]
    fn final_roots_add_to_guarded_projects_only() {
        let written = floor(
            "schema = 2\n[[project]]\nname = \"lab\"\npath = \"C:/Users/RUNNER~1/Lab\"\n[[project]]\nname = \"notes\"\npath = \"C:/Notes\"\nautonomy = \"free\"\n",
        );
        let mut c = containment(
            FloorInput::Valid {
                floor: &written,
                path: FLOOR_PATH,
            },
            SnapshotInput::Missing,
            None,
        );
        let roots: Vec<String> = c.guarded_roots().into_iter().map(str::to_owned).collect();
        assert_eq!(roots.len(), 1, "{roots:?}");
        let free_root = resolve_project_path(floor_dir_of(FLOOR_PATH), "C:/Notes");
        assert!(!roots.contains(&free_root));
        assert_eq!(name_for(&c, &["C:/Users/runneradmin/Lab/roles"]), None);
        let finals = BTreeMap::from([
            (roots[0].clone(), "C:/Users/runneradmin/Lab".to_owned()),
            (free_root, "C:/Users/runneradmin/Lab/roles".to_owned()),
        ]);
        c.add_final_roots(&finals);
        assert_eq!(
            name_for(&c, &["C:/Users/runneradmin/Lab/roles/x"]),
            Some("lab")
        );
        assert_eq!(name_for(&c, &[r"C:\Users\RUNNER~1\Lab"]), Some("lab"));
        assert_eq!(name_for(&c, &["C:/Notes"]), None);
    }

    #[test]
    fn floor_and_snapshot_are_looked_up_separately_and_united() {
        let approved = snapshot_of(GUARDED_LAB);
        // A session's edit: the list emptied, the project set free, and a
        // free project nested inside it.
        let loosened = floor(
            "schema = 2\nguarded_programs = []\n[[project]]\nname = \"lab\"\npath = \"Lab\"\nautonomy = \"free\"\n[[project]]\nname = \"inner\"\npath = \"Lab/inner\"\nautonomy = \"free\"\n",
        );
        let c = containment(
            FloorInput::Valid {
                floor: &loosened,
                path: FLOOR_PATH,
            },
            SnapshotInput::Valid(&approved),
            None,
        );
        assert_eq!(name_for(&c, &["C:/path/to/Lab/inner/x"]), Some("lab"));
        assert_eq!(name_for(&c, &[r"C:\path\to\Lab"]), Some("lab"));
        assert_eq!(name_for(&c, &["C:/path/to/Notes"]), None);
        assert_eq!(
            c.guarded_programs,
            {
                let mut all: Vec<String> = DEFAULT_GUARDED_PROGRAMS
                    .iter()
                    .map(|p| (*p).to_owned())
                    .collect();
                all.sort();
                all
            },
            "an emptied floor list keeps the approved one"
        );
        assert!(c.faults.is_empty());
        assert!(
            matches!(c.offer_for("lab"), Offer::Approved(actions) if actions[0].name == "probe" && actions[0].params == ["host"] && actions[0].available)
        );
        assert!(matches!(c.offer_for("inner"), Offer::AwaitingApproval([])));
    }

    #[test]
    fn broken_snapshot_guards_every_floor_project_with_the_defaults() {
        let current = floor(&GUARDED_LAB.replace(
            "[[project]]\nname = \"lab\"",
            "guarded_programs = [\"ssh\"]\n[[project]]\nname = \"lab\"",
        ));
        let input = FloorInput::Valid {
            floor: &current,
            path: FLOOR_PATH,
        };
        for (snapshot, record, fault) in [
            (
                SnapshotInput::Unreadable,
                None,
                GateFault::ApprovedUnreadable,
            ),
            (
                SnapshotInput::Missing,
                Some(known(&[], true)),
                GateFault::ApprovedMissing,
            ),
        ] {
            let c = containment(input, snapshot, record.as_ref());
            assert_eq!(c.faults, [fault]);
            assert_eq!(name_for(&c, &["C:/path/to/Notes"]), Some("notes"));
            assert!(c.guarded_programs.contains(&"ssh".to_owned()));
            assert!(c.guarded_programs.contains(&"wsl".to_owned()));
            assert!(
                matches!(c.offer_for("lab"), Offer::AwaitingApproval(names) if names == ["probe"])
            );
        }
        let never_approved = containment(input, SnapshotInput::Missing, Some(&known(&[], false)));
        assert!(never_approved.faults.is_empty());
        assert_eq!(name_for(&never_approved, &["C:/path/to/Notes"]), None);
        assert_eq!(never_approved.guarded_programs, ["ssh"]);
    }

    #[test]
    fn a_failed_floor_falls_back_to_the_known_projects() {
        let record = known(&["c:/path/to/notes"], true);
        let approved = snapshot_of(GUARDED_LAB);
        for (floor_input, fault) in [
            (FloorInput::Invalid, GateFault::FloorInvalid),
            (FloorInput::Unreadable, GateFault::FloorUnreadable),
            (FloorInput::Missing, GateFault::FloorMissing),
        ] {
            let both_broken = containment(floor_input, SnapshotInput::Unreadable, Some(&record));
            assert_eq!(both_broken.faults, [GateFault::ApprovedUnreadable, fault]);
            assert_eq!(
                name_for(&both_broken, &["/c/path/to/notes/sub"]),
                Some("c:/path/to/notes")
            );
            assert!(matches!(
                both_broken.offer_for("c:/path/to/notes"),
                Offer::Unreadable
            ));
            assert_eq!(name_for(&both_broken, &["C:/path/to/Other"]), None);

            let snapshot_holds =
                containment(floor_input, SnapshotInput::Valid(&approved), Some(&record));
            assert_eq!(snapshot_holds.faults, [fault]);
            assert_eq!(name_for(&snapshot_holds, &["C:/path/to/Lab"]), Some("lab"));
            assert_eq!(
                name_for(&snapshot_holds, &["C:/path/to/Notes"]),
                Some("c:/path/to/notes"),
                "a broken floor makes even a free known project guarded"
            );
        }
        let nothing = containment(FloorInput::Invalid, SnapshotInput::Unreadable, None);
        assert_eq!(
            nothing.faults,
            [
                GateFault::ApprovedUnreadable,
                GateFault::FloorInvalid,
                GateFault::KnownProjectsUnreadable
            ]
        );
        assert_eq!(nothing.guarded_project(&["C:/path/to/Lab"]), None);
    }

    #[test]
    fn either_directory_guards_the_call() {
        let current = floor(GUARDED_LAB);
        let c = containment(
            FloorInput::Valid {
                floor: &current,
                path: FLOOR_PATH,
            },
            SnapshotInput::Missing,
            None,
        );
        assert_eq!(name_for(&c, &["C:/path/to/Lab", "C:/path/to"]), Some("lab"));
        assert_eq!(
            name_for(&c, &["C:/path/to/Notes", "C:/path/to/Lab/x"]),
            Some("lab")
        );
        assert_eq!(name_for(&c, &["C:/path/to/Notes", "C:/path/to"]), None);
    }

    #[test]
    fn unreadable_offer_points_to_doctor() {
        let reason = deny_reason(
            &deny(DenyKind::GuardedProgram, Some("ssh")),
            "c:/path/to/lab",
            Offer::Unreadable,
        );
        assert!(reason.contains("officina doctor") && reason.contains(LIST_COMMAND));
    }

    #[test]
    fn protected_dirs_follow_the_home() {
        let default = protected_dirs(r"C:\Users\user01\.officina\", true);
        assert_eq!(
            default,
            [
                "c:/users/user01/.officina/relay",
                "c:/users/user01/.officina/journal",
                ".officina/relay",
                ".officina/journal"
            ]
        );
        let custom = protected_dirs("D:/OfficinaHome", false);
        assert_eq!(custom, ["d:/officinahome/relay", "d:/officinahome/journal"]);
        for text in [
            "cat D:/OfficinaHome/relay/approved.json",
            r"type D:\OFFICINAHOME\Relay\approved.json",
            "ls /d/officinahome//relay",
            "ls /cygdrive/d/officinahome/./journal",
            r"dir \\host01\d$\officinahome\relay",
        ] {
            assert!(names_protected_dir(text, &custom), "{text}");
        }
        for text in [
            "ls relay/",
            "cat D:/OfficinaHome/relay2",
            "cat notes.officina/relay",
        ] {
            assert!(!names_protected_dir(text, &custom), "{text}");
        }
        assert!(names_protected_dir("cat ~/.officina/relay/x", &default));
        assert!(!names_protected_dir("cat x.officina/relay", &default));
    }

    /// D-173: each file tool is named in the matcher with its field, and
    /// none of them is a shell tool.
    #[test]
    fn every_file_tool_is_gated_by_its_field() {
        let matcher = gated_tools_matcher();
        for (tool, field) in FILE_TOOLS {
            assert_eq!(file_tool_field(tool), Some(*field), "{tool}");
            assert_eq!(tool_rule(tool), None, "{tool}");
            assert!(matcher.split('|').any(|name| name == *tool), "{tool}");
        }
        assert_eq!(file_tool_field("Read"), None);
        assert_eq!(file_tool_field("write"), None, "named exactly");
    }

    /// D-173: every name of the environment constant, assigned next to an
    /// `officina` call in a guarded project, is denied by name.
    #[test]
    fn every_officina_environment_name_is_denied() {
        let list = vec!["ssh".to_owned()];
        for name in OFFICINA_ENVIRONMENT_NAMES {
            let variable = name.replace('*', "DIR");
            for command in [
                format!("{variable}=C:/shim officina run probe host=web01"),
                format!("export {variable}=C:/shim; officina run --list"),
            ] {
                let deny = decide(&command, &list, &[], true).unwrap();
                assert_eq!(deny.kind, DenyKind::OfficinaEnvironment, "{command}");
                assert_eq!(deny.matched.as_deref(), Some(variable.as_str()));
                assert_eq!(decide(&command, &list, &[], false), None);
            }
            let other = format!("{variable}=C:/shim make test");
            assert_eq!(decide(&other, &list, &[], true), None, "{other}");
        }
        let reason = deny_reason(
            &deny(DenyKind::OfficinaEnvironment, Some("PATH")),
            "lab",
            Offer::Approved(&[]),
        );
        assert!(reason.contains("`PATH`"), "{reason}");
    }

    /// D-173: a file write's reason names no project outside one, and still
    /// points at the list command.
    #[test]
    fn a_file_write_reason_names_no_project_outside_one() {
        let write = deny(DenyKind::RelayStateWrite, None);
        let reason = deny_reason(&write, "", Offer::Approved(&[]));
        assert!(
            reason.starts_with("Officina refused this file write"),
            "{reason}"
        );
        assert!(reason.contains(LIST_COMMAND), "{reason}");
        assert!(!reason.contains("guarded project"), "{reason}");
        assert!(!reason.contains("no relay action"), "{reason}");
    }

    /// D-171: every shell tool has a rule, applied by `decide_call`; the
    /// match is exhaustive, so a new rule does not compile without a case.
    #[test]
    fn every_shell_tool_has_a_rule() {
        let list = vec!["ssh".to_owned()];
        assert!(!SHELL_TOOLS.is_empty());
        for (tool, rule) in SHELL_TOOLS {
            assert_eq!(tool_rule(tool), Some(*rule), "{tool}");
            assert!(
                gated_tools_matcher().split('|').any(|name| name == *tool),
                "{tool} is in the matcher"
            );
            let guarded = decide_call(tool, Some("ssh host01"), &list, &[], true).map(|d| d.kind);
            let free = decide_call(tool, Some("ssh host01"), &list, &[], false);
            assert_eq!(free, None, "{tool}: never outside a guarded project");
            match rule {
                ToolRule::Lexed => {
                    assert_eq!(guarded, Some(DenyKind::GuardedProgram), "{tool}");
                    assert_eq!(
                        decide_call(tool, Some("ls -la"), &list, &[], true),
                        None,
                        "{tool}: lexed like Bash"
                    );
                    assert_eq!(
                        decide_call(tool, None, &list, &[], true),
                        None,
                        "{tool}: no command, no decision"
                    );
                }
                ToolRule::DeniedInGuardedProject => {
                    assert_eq!(
                        guarded,
                        Some(DenyKind::PowershellInGuardedProject),
                        "{tool}"
                    );
                    assert_eq!(
                        decide_call(tool, None, &list, &[], true).map(|d| d.kind),
                        Some(DenyKind::PowershellInGuardedProject),
                        "{tool}: refused whole"
                    );
                }
            }
        }
        for other in ["Read", "Write", "Edit", "bash", "powershell", ""] {
            assert_eq!(tool_rule(other), None, "{other}");
            assert_eq!(
                decide_call(other, Some("ssh host01"), &list, &[], true),
                None
            );
        }
        let reason = deny_reason(
            &deny(DenyKind::PowershellInGuardedProject, None),
            "lab",
            Offer::Approved(&[]),
        );
        assert!(reason.contains("lab"), "{reason}");
        assert!(reason.contains("Bash"), "{reason}");
        assert!(reason.contains(LIST_COMMAND), "{reason}");
    }
}
