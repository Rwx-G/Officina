//! Replays the hand-written gate vector set (AD-26, AD-27, D-10 to D-13,
//! D-171, D-173) through `gate::decide_call`, or `gate::decide_file_write`
//! for a file tool, refuses a set that no longer exercises every shell and
//! file tool, default guarded program, home-redirection and environment
//! name and deny kind (the lexer constants are checked in `relay::shell`'s
//! unit tests), and holds the honest limits to "no decision".

use std::collections::HashSet;

use officina_core::floor::parse_floor;
use officina_core::relay::catalogue::{DEFAULT_GUARDED_PROGRAMS, guarded_programs};
use officina_core::relay::gate::{
    DenyKind, FILE_TOOLS, GateInputs, HOME_REDIRECTION_NAMES, OFFICINA_ENVIRONMENT_NAMES,
    SHELL_TOOLS, decide_call, decide_file_write, file_tool_field, guarded_match, protected_dirs,
};
use officina_core::relay::shell;
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
    /// What a shell tool runs; empty for a file tool.
    #[serde(default)]
    command: String,
    /// The file a file tool writes, resolved as the host would (D-173).
    #[serde(default)]
    target: Option<String>,
    /// The effective list; the default one when absent.
    #[serde(default)]
    guarded_programs: Option<Vec<String>>,
    /// For a shell tool, the call lies in a guarded project; for a file
    /// tool, the relay guards any project.
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
        let decision = match (file_tool_field(case.tool()), &case.target) {
            (Some(_), Some(target)) => decide_file_write(target, &set.home, case.cwd_guarded),
            (None, None) => decide_call(
                case.tool(),
                Some(&case.command),
                &GateInputs {
                    guarded_programs: list,
                    protected_dirs: &dirs,
                    cwd_guarded: case.cwd_guarded,
                },
            ),
            _ => panic!(
                "case {}: a target goes with a file tool, and only",
                case.name
            ),
        };
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

/// Every shell and file tool, deny kind, default guarded program and
/// home-redirection and environment name has a case. The lexer constants
/// are covered by `relay::shell::tests::every_constant_appears_in_a_vector_case`,
/// where the lexer records what it used.
#[test]
fn every_constant_appears_in_a_case() {
    let set = load();
    for tool in SHELL_TOOLS
        .iter()
        .map(|(tool, _)| tool)
        .chain(FILE_TOOLS.iter().map(|(tool, _)| tool))
    {
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
    for name in OFFICINA_ENVIRONMENT_NAMES {
        let covered = expected.iter().any(|(case, kind)| {
            *kind == Some(DenyKind::OfficinaEnvironment)
                && case
                    .matched
                    .as_deref()
                    .is_some_and(|m| shell::name_matches(name, m))
        });
        assert!(
            covered,
            "no case denies an officina environment through {name}"
        );
    }
}
