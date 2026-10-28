//! The gate in the hook (AD-26, AD-28, D-13 to D-17, D-171): reads a
//! `PreToolUse` payload of a shell tool, the relay floor (phase 1 only), the
//! approved snapshot and, when one of them fails, the known-projects record;
//! decides by the tool's rule; journals each deny and each fault; and returns
//! the JSON Claude Code honours in every mode (E12, E15). It spawns nothing
//! and compiles no pattern.
//!
//! It is containment, not the authority: an internal fault gives no
//! decision (fail open, AD-28), while an input it cannot read makes it
//! stricter (D-15). `officina run` fails closed on its own.

use std::fs;
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};

use officina_core::events::excerpt;
use officina_core::floor::{parse_floor, requests_no_excerpts};
use officina_core::quota::normalise_path;
use officina_core::relay::HOME_DIR_NAME;
use officina_core::relay::gate::{
    Deny, FloorInput, SnapshotInput, ToolRule, containment, decide_call, deny_reason,
    protected_dirs, tool_rule,
};
use officina_core::relay::journal::{
    COMMAND_EXCERPT_CHARS, GateDenied, GateError, GateFault, JournalEntry,
};

use super::home::{ApprovedState, RelayHome};
use super::journal;
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

/// A shell tool's call as the payload carries it (E15, D-171).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellCall {
    /// The tool, one of `SHELL_TOOLS`.
    pub tool: String,
    /// `tool_input.command`; a lexed tool's call always has one.
    pub command: Option<String>,
    /// The shell's working directory, which follows `cd` (E15).
    pub cwd: Option<String>,
    pub session_id: Option<String>,
}

impl ShellCall {
    /// The call, when the payload is a shell tool's (AD-26 step 1, D-171):
    /// a lexed tool's needs a command, a tool refused whole does not.
    /// Anything else gets no decision.
    pub fn from_payload(payload: &serde_json::Value) -> Option<ShellCall> {
        let text = |value: Option<&serde_json::Value>| {
            value.and_then(serde_json::Value::as_str).map(str::to_owned)
        };
        let tool = text(payload.get("tool_name"))?;
        let command = text(payload.get("tool_input").and_then(|i| i.get("command")));
        match tool_rule(&tool)? {
            ToolRule::Lexed if command.is_none() => return None,
            ToolRule::Lexed | ToolRule::DeniedInGuardedProject => {}
        }
        Some(ShellCall {
            tool,
            command,
            cwd: text(payload.get("cwd")),
            session_id: text(payload.get("session_id")),
        })
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
/// D-14 to D-16). `project_dir` is `CLAUDE_PROJECT_DIR`.
pub fn evaluate(home: &RelayHome, call: &ShellCall, project_dir: Option<&str>) -> Evaluation {
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

    let contained = containment(floor, snapshot, known.as_ref());
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
    let dirs: Vec<&str> = project_dir.into_iter().chain(call.cwd.as_deref()).collect();
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
    call: &ShellCall,
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
        .command
        .as_deref()
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

fn journal_fault(
    home: &RelayHome,
    call: &ShellCall,
    cause: GateFault,
    detail: String,
    now_ms: i64,
) {
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
/// prints nothing. Not a shell tool's call: no decision, no line.
pub fn answer(
    home: &RelayHome,
    payload: &serde_json::Value,
    env: &GateEnv,
    now_ms: i64,
) -> Option<String> {
    let call = ShellCall::from_payload(payload)?;
    answer_with(home, &call, env, now_ms, || {
        Ok(evaluate(home, &call, env.project_dir.as_deref()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use officina_core::relay::journal::JournalLine;

    fn call() -> ShellCall {
        ShellCall {
            tool: "Bash".to_owned(),
            command: Some("ssh host01".to_owned()),
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
    fn only_shell_tool_calls_are_gated() {
        use officina_core::relay::gate::SHELL_TOOLS;
        for (tool, rule) in SHELL_TOOLS {
            let with = serde_json::json!({"tool_name": tool, "tool_input": {"command": "ls"}, "cwd": "C:/x"});
            let call = ShellCall::from_payload(&with).unwrap();
            assert_eq!(call.command.as_deref(), Some("ls"), "{tool}");
            let without =
                serde_json::json!({"tool_name": tool, "tool_input": {"url": "wss://example.com"}});
            let gated = ShellCall::from_payload(&without).is_some();
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
            assert_eq!(ShellCall::from_payload(&other), None, "{other}");
        }
    }
}
