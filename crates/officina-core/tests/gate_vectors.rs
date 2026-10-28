//! Replays the hand-written gate vector set (AD-26, AD-27, D-10 to D-13,
//! D-171) through `gate::decide_call`, refuses a set that no longer
//! exercises every lexer constant, shell tool, default guarded program,
//! home-redirection name and deny kind, and holds the honest limits to "no
//! decision".

use std::collections::HashSet;

use officina_core::floor::parse_floor;
use officina_core::relay::catalogue::{DEFAULT_GUARDED_PROGRAMS, guarded_programs};
use officina_core::relay::gate::{
    DenyKind, HOME_REDIRECTION_NAMES, SHELL_TOOLS, decide_call, guarded_match, protected_dirs,
};
use officina_core::relay::shell::{
    self, Builtin, CMD_COMMAND_SWITCHES, CMD_DELAYED_EXPANSION_SWITCH, COMMAND_QUERY_OPTIONS,
    ENV_SPLIT_OPTIONS, FIND_EXEC_ACTIONS, FIND_EXEC_TERMINATORS, LAUNCHERS,
    POWERSHELL_INTERPRETERS, REDIRECTIONS, RESERVED_WORDS, SCRIPT_INTERPRETERS, SEPARATORS,
    SHELL_INTERPRETERS, SHELL_VALUE_OPTIONS, STDIN_PATHS, Seen, WATCH_EXEC_OPTIONS, WRAPPERS,
    XARGS_REPLACE_OPTIONS, encoded_command_spellings,
};
use serde::Deserialize;

const VECTORS: &str = include_str!("../../../tests/vectors/gate.json");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VectorSet {
    schema: u32,
    /// The Officina home the protected directories derive from (D-13).
    home: String,
    home_is_default: bool,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    /// The tool that runs the command; the Bash tool when absent (D-171).
    #[serde(default)]
    tool: Option<String>,
    command: String,
    /// The effective list; the default one when absent.
    #[serde(default)]
    guarded_programs: Option<Vec<String>>,
    cwd_guarded: bool,
    /// `"none"` or a deny kind.
    expect: String,
    #[serde(default)]
    matched: Option<String>,
    /// What a string match cannot catch (AD-27): held at no decision so an
    /// improvement shows up as a vector change.
    #[serde(default)]
    limit: bool,
}

impl Case {
    fn expected(&self) -> Option<DenyKind> {
        (self.expect != "none").then(|| {
            serde_json::from_value(serde_json::Value::String(self.expect.clone()))
                .unwrap_or_else(|e| panic!("case {}: expect {:?}: {e}", self.name, self.expect))
        })
    }

    fn tool(&self) -> &str {
        self.tool.as_deref().unwrap_or("Bash")
    }

    fn uses_default_list(&self) -> bool {
        self.guarded_programs.is_none()
    }
}

fn load() -> VectorSet {
    let set: VectorSet = serde_json::from_str(VECTORS).expect("vector set parses");
    assert_eq!(set.schema, 1);
    set
}

/// The default list as the gate receives it: through the floor's phase 1.
fn default_list() -> Vec<String> {
    guarded_programs(&parse_floor("schema = 2\n").unwrap())
}

#[test]
fn every_case_matches() {
    let set = load();
    let dirs = protected_dirs(&set.home, set.home_is_default);
    let default = default_list();
    let mut names = HashSet::new();
    let mut failures = Vec::new();
    for case in &set.cases {
        assert!(
            names.insert(&case.name),
            "duplicate case name {}",
            case.name
        );
        let list = case.guarded_programs.as_ref().unwrap_or(&default);
        let decision = decide_call(
            case.tool(),
            Some(&case.command),
            list,
            &dirs,
            case.cwd_guarded,
        );
        let kind = decision.as_ref().map(|d| d.kind);
        let matched_ok = case
            .matched
            .as_ref()
            .is_none_or(|m| decision.as_ref().and_then(|d| d.matched.as_ref()) == Some(m));
        if kind != case.expected() || !matched_ok {
            failures.push(format!(
                "{}: expected {} {:?}, got {decision:?}",
                case.name, case.expect, case.matched
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} failing cases:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn limits_are_flagged() {
    let set = load();
    let limits: Vec<&Case> = set.cases.iter().filter(|c| c.limit).collect();
    assert!(!limits.is_empty(), "the honest limits of AD-27 have cases");
    for case in limits {
        assert_eq!(
            case.expect, "none",
            "limit case {} must expect no decision",
            case.name
        );
    }
    assert!(
        set.cases
            .iter()
            .any(|c| c.limit && !c.cwd_guarded && c.command.starts_with("cd ")),
        "a cd into a guarded project from a free folder is a limit case (AC 10)"
    );
}

#[test]
fn every_constant_appears_in_a_case() {
    let set = load();
    let mut seen: HashSet<Seen> = HashSet::new();
    for case in &set.cases {
        if let Ok(lexed) = shell::lex(&case.command) {
            seen.extend(lexed.trace);
        }
    }
    let mut missing: Vec<String> = Vec::new();
    let mut need = |entry: Seen| {
        if !seen.contains(&entry) {
            missing.push(format!("{entry:?}"));
        }
    };
    for s in SEPARATORS {
        need(Seen::Separator(s));
    }
    for w in RESERVED_WORDS {
        need(Seen::Reserved(w));
    }
    for r in REDIRECTIONS {
        need(Seen::Redirection(r));
    }
    for wrapper in WRAPPERS {
        need(Seen::Wrapper(wrapper.name));
        for option in wrapper.value_options {
            need(Seen::WrapperOption {
                wrapper: wrapper.name,
                option,
            });
        }
    }
    ENV_SPLIT_OPTIONS
        .iter()
        .for_each(|o| need(Seen::EnvSplit(o)));
    COMMAND_QUERY_OPTIONS
        .iter()
        .for_each(|o| need(Seen::CommandQuery(o)));
    XARGS_REPLACE_OPTIONS
        .iter()
        .for_each(|o| need(Seen::XargsReplace(o)));
    WATCH_EXEC_OPTIONS
        .iter()
        .for_each(|o| need(Seen::WatchExec(o)));
    FIND_EXEC_ACTIONS
        .iter()
        .for_each(|a| need(Seen::FindAction(a)));
    FIND_EXEC_TERMINATORS
        .iter()
        .for_each(|t| need(Seen::FindTerminator(t)));
    SHELL_INTERPRETERS.iter().for_each(|s| need(Seen::Shell(s)));
    SHELL_VALUE_OPTIONS
        .iter()
        .for_each(|o| need(Seen::ShellOption(o)));
    need(Seen::ShellCommandLetter);
    need(Seen::ShellStdinLetter);
    for interpreter in SCRIPT_INTERPRETERS {
        need(Seen::Script(interpreter.name));
        for flag in interpreter
            .program_flags
            .iter()
            .chain(interpreter.value_options)
        {
            need(Seen::ScriptFlag {
                interpreter: interpreter.name,
                flag,
            });
        }
    }
    STDIN_PATHS.iter().for_each(|p| need(Seen::StdinPath(p)));
    need(Seen::Cmd);
    for switch in CMD_COMMAND_SWITCHES
        .iter()
        .chain([&CMD_DELAYED_EXPANSION_SWITCH])
    {
        need(Seen::CmdSwitch(switch));
    }
    POWERSHELL_INTERPRETERS
        .iter()
        .for_each(|p| need(Seen::PowerShell(p)));
    encoded_command_spellings().for_each(|s| need(Seen::EncodedSpelling(s)));
    LAUNCHERS.iter().for_each(|l| need(Seen::Launcher(l.name)));
    Builtin::ALL.iter().for_each(|b| need(Seen::Builtin(*b)));
    assert!(
        missing.is_empty(),
        "lexer constants no case exercises:\n{}",
        missing.join("\n")
    );

    for (tool, _) in SHELL_TOOLS {
        assert!(
            set.cases.iter().any(|c| c.tool() == *tool),
            "no case runs through the {tool} tool"
        );
    }
    let expected: Vec<(&Case, Option<DenyKind>)> =
        set.cases.iter().map(|c| (c, c.expected())).collect();
    for kind in DenyKind::ALL {
        assert!(
            expected.iter().any(|(_, k)| *k == Some(*kind)),
            "no case expects {kind:?}"
        );
    }
    let default = default_list();
    for entry in DEFAULT_GUARDED_PROGRAMS {
        let covered = expected.iter().any(|(case, kind)| {
            *kind == Some(DenyKind::GuardedProgram)
                && case.uses_default_list()
                && case.cwd_guarded
                && case.matched.as_deref().is_some_and(|m| {
                    let one = [(*entry).to_owned()];
                    let program = shell::Program {
                        written: m.to_owned(),
                        origin: shell::Origin::Command,
                    };
                    guarded_match(&program, &one).as_deref() == Some(m)
                        && guarded_match(&program, &default).is_some()
                })
        });
        assert!(
            covered,
            "no case denies the default guarded program {entry}"
        );
    }
    for name in HOME_REDIRECTION_NAMES {
        let covered = expected.iter().any(|(case, kind)| {
            *kind == Some(DenyKind::HomeRedirection)
                && case
                    .matched
                    .as_deref()
                    .is_some_and(|m| shell::name_matches(name, m))
        });
        assert!(covered, "no case denies a home redirection through {name}");
    }
}
