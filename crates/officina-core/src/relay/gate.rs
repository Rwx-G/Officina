//! The gate's decision (AD-26 steps 2 to 5, D-12, D-13, D-17): in a guarded
//! repository, deny a `Bash` command that would start a guarded program,
//! hide its program, redirect Officina's home or touch Officina's own files;
//! otherwise, and always outside a guarded repository, decide nothing. The
//! host reads the payload, the floor and the snapshot and journals each deny
//! (story 2.4); this module only decides, from text.

use serde::{Deserialize, Serialize};

use super::shell::{self, Assignment, Lexed, Origin, Program};
use super::{HOME_DIR_NAME, JOURNAL_DIR, RELAY_DIR};
use crate::floor::Autonomy;
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

/// Longest deny reason, in bytes (AD-26).
pub const MAX_REASON_BYTES: usize = 2048;

/// The command every reason points to (D-17).
pub const LIST_COMMAND: &str = "officina run --list";

/// Longest project name or matched word quoted in a reason, in characters,
/// so the pointer to [`LIST_COMMAND`] always fits.
pub const MAX_QUOTED_CHARS: usize = 64;

closed_enum! {
    /// Why the gate denied a command. Also the `expect` vocabulary of
    /// `tests/vectors/gate.json` and the journal's (story 2.4). When several
    /// apply, the first in this order is reported.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
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
        /// The text names an `OFFICINA_` variable.
        OfficinaVariable,
        /// The text names Officina's relay or journal directory (D-13).
        ProtectedDir,
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
    if let Some(matched) = lexed
        .programs
        .iter()
        .find_map(|program| guarded_match(program, guarded_programs))
    {
        return Deny::new(DenyKind::GuardedProgram, Some(matched));
    }
    if lexed.encoded_command {
        return Deny::new(DenyKind::EncodedCommand, None);
    }
    if !lexed.dynamic.is_empty() {
        return Deny::new(DenyKind::Dynamic, None);
    }
    if let Some(deny) = home_redirection(&lexed) {
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
        let entry = shell::fold(entry);
        if let Some(stem) = entry.strip_suffix(".exe") {
            let written = shell::written_name(&program.written) == entry;
            let system = name == stem
                && program.origin == Origin::Command
                && shell::under_windows_dir(&program.written);
            (written || system).then_some(entry)
        } else {
            shell::name_matches(&entry, &name).then(|| name.clone())
        }
    })
}

fn home_redirection(lexed: &Lexed) -> Option<Deny> {
    let officina = lexed
        .programs
        .iter()
        .any(|p| shell::program_name(&p.written) == OFFICINA_PROGRAM);
    if !officina {
        return None;
    }
    let home_name = |name: &str| {
        HOME_REDIRECTION_NAMES
            .iter()
            .any(|entry| shell::name_matches(entry, name))
    };
    for assignment in &lexed.assignments {
        match assignment {
            Assignment::Name(name) if home_name(name) => {
                return Deny::new(DenyKind::HomeRedirection, Some(name.clone()));
            }
            Assignment::Computed => return Deny::new(DenyKind::HomeRedirection, None),
            Assignment::Name(_) => {}
        }
    }
    // A scanned interpreter can set a variable in its own syntax
    // (`$env:HOME`, `set HOME=`, `os.environ['HOME']`): the name is enough.
    lexed
        .programs
        .iter()
        .find(|p| p.origin == Origin::Scanned && home_name(&p.written))
        .and_then(|p| Deny::new(DenyKind::HomeRedirection, Some(p.written.clone())))
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
/// collapsed, `/./` segments dropped.
fn path_text(text: &str) -> String {
    let folded: Vec<char> = shell::fold(text)
        .chars()
        .map(|c| if c == '\\' { '/' } else { c })
        .collect();
    let mut out = String::with_capacity(folded.len());
    let mut i = 0;
    while i < folded.len() {
        let c = folded[i];
        i += 1;
        if c != '/' {
            out.push(c);
            continue;
        }
        loop {
            match (folded.get(i), folded.get(i + 1)) {
                (Some('/'), _) => i += 1,
                (Some('.'), Some('/')) => i += 2,
                _ => break,
            }
        }
        out.push('/');
    }
    out
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
}

/// The reason Claude Code shows the session (AD-26, D-17): what was denied
/// and where, the pointer to [`LIST_COMMAND`], then the actions to use
/// instead as `officina run <name> <param>=<...>`, unavailable classes
/// marked; at most [`MAX_REASON_BYTES`], cut on a character boundary.
pub fn deny_reason(deny: &Deny, project: &str, offer: Offer<'_>) -> String {
    let mut text = format!(
        "Officina refused this command in the guarded project {}: {}. Run `{LIST_COMMAND}` to see the actions you may use and their allowed values.",
        quoted(project),
        why(deny)
    );
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
    }
    cut(text, MAX_REASON_BYTES)
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
        DenyKind::OfficinaVariable => format!(
            "the command names an `{OFFICINA_VARIABLE_PREFIX}` variable, one of Officina's own settings"
        ),
        DenyKind::ProtectedDir => {
            "the command names Officina's relay or journal directory, which only the Officina window writes".to_owned()
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
}
