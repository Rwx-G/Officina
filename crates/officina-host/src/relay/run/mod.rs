//! `officina run` (AD-29 to AD-33, D-170): reads what the checks need,
//! refuses or starts the approved argument vector with a timeout and an
//! output budget, and journals the run. Runs of an action naming a check and
//! of a check record the tree they start on, and a check keeps its output
//! for the approval card; an `after-check` action runs only after its check;
//! a `human` action waits for the author's approval, made after its dry run
//! when it names one. It is the relay's authority over what runs and fails
//! closed, unlike the gate (AD-28): every doubt is a refusal.
//!
//! Exit codes a shell already reads: the program's own, full Windows value
//! included; 126 refused; 124 timed out; 127 not started; 64 usage. A
//! program can exit 126 or 124 itself: the stderr line and the journal tell
//! them apart. Officina itself never exits 2.
//!
//! One module per concern: [`args`], the request and its wait (`request`),
//! the use of an approval (`consume`), a process's start (`spawn`) and its
//! output (`output`), an HTTP request (`send`), the journal lines of a run
//! (`journaling`) and the kept output of a check run (`kept`).

mod args;
mod consume;
mod journaling;
mod kept;
mod output;
mod request;
mod send;
mod spawn;

#[cfg(test)]
mod fixture;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use officina_core::relay::catalogue::ActionClass;
use officina_core::relay::journal::{JournalEntry, Refused};
use officina_core::relay::run::{
    FloorState, Refusal, RunInputs, RunPlan, SnapshotState, check_step, list_text, plan,
};

pub use args::{RunArgs, UsageError, parse_args};
pub use officina_core::relay::run::{EXIT_NOT_STARTED, EXIT_REFUSED, EXIT_TIMEOUT};
pub use spawn::command_for;

use super::credential::{CredentialStore, Secret, SystemCredentialStore};
use super::home::{ApprovedState, RelayHome, load_relay_floor, system_dir};
use super::journal;
use super::tree::{self, TreeError};
use crate::files::now_ms;
use journaling::{Started, start_line};
use request::{Bound, approval};

/// The session that called, as Claude Code's Bash tool sets it (E13).
pub const ENV_SESSION_ID: &str = "CLAUDE_CODE_SESSION_ID";

/// How long the output pipes may stay open once the program has exited: a
/// process it left running holds them, and is ended with the job when
/// `officina run` exits.
pub const OUTPUT_GRACE: Duration = Duration::from_millis(500);

/// `CLAUDE_CODE_SESSION_ID`, absent outside Claude Code (AD-29).
pub fn session_id_from_env() -> Option<String> {
    std::env::var(ENV_SESSION_ID).ok().filter(|v| !v.is_empty())
}

/// Reads what the checks need and hands it to `f`.
fn with_inputs<R>(home: &RelayHome, cwd: &str, f: impl FnOnce(&RunInputs<'_>) -> R) -> R {
    let stopped = home.relay_stopped();
    let record = home.read_record();
    let approved = home.read_approved();
    let floor_path = home.relay_floor(&approved);
    let floor_text = floor_path.to_string_lossy();
    let loaded = load_relay_floor(&floor_path);
    let floor_error = loaded.as_ref().err().map(ToString::to_string);
    let snapshot = match &approved {
        ApprovedState::Missing => SnapshotState::Missing,
        ApprovedState::Unreadable(why) => SnapshotState::Unreadable(why),
        ApprovedState::Valid(approved) => SnapshotState::Valid(approved),
    };
    let floor = match &loaded {
        Ok(relay) => FloorState::Valid {
            relay,
            path: &floor_text,
        },
        Err(_) => FloorState::Unreadable(floor_error.as_deref().unwrap_or_default()),
    };
    // Empty off Windows, where the `wsl` runner has nothing to start.
    let system_dir = system_dir()
        .map(|dir| dir.to_string_lossy().into_owned())
        .unwrap_or_default();
    f(&RunInputs {
        stopped,
        snapshot,
        record: &record,
        floor,
        cwd,
        system_dir: &system_dir,
    })
}

/// `officina run --list` (D-21).
pub fn list(home: &RelayHome, cwd: &str) -> String {
    with_inputs(home, cwd, list_text)
}

/// `officina run <action> name=value ...`: checks, then runs. Returns the
/// exit code.
///
/// After the checks of [`plan`], a run that binds the tree takes its
/// fingerprint in the project folder (D-22), and an action naming a check
/// reads two days of journal for it (AD-33, D-170). A `human` action then
/// waits up to `wait` for the author's approval, the last step before
/// anything is journaled as started.
pub fn run(
    home: &RelayHome,
    cwd: &str,
    action: &str,
    params: &BTreeMap<String, String>,
    session_id: Option<String>,
    wait: Duration,
) -> i32 {
    run_with(
        home,
        cwd,
        &Call {
            action,
            params,
            session_id,
            wait,
        },
        &SystemCredentialStore,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    )
}

/// One `officina run` call: the action, its parameters, the session and how
/// long a `human` action waits.
pub struct Call<'a> {
    pub action: &'a str,
    pub params: &'a BTreeMap<String, String>,
    pub session_id: Option<String>,
    pub wait: Duration,
}

/// [`run`] with the credential store and the streams given, for tests: a
/// fake store, a captured response on `out`, Officina's own lines (a
/// refusal, a timeout, a status that did not succeed) on `err`. A process
/// action's output goes to this process's stdout and stderr, where the
/// session reads it.
pub fn run_with(
    home: &RelayHome,
    cwd: &str,
    call: &Call<'_>,
    store: &dyn CredentialStore,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let ctx = Context {
        home,
        cwd,
        action: call.action,
        params: call.params,
        session_id: call.session_id.as_deref(),
        store,
    };
    let plan = match with_inputs(home, cwd, |inputs| plan(inputs, call.action, call.params)) {
        Ok(plan) => plan,
        Err(refusal) => return ctx.refuse(err, &refusal),
    };
    let tree = if plan.binds_tree {
        match tree::fingerprint(Path::new(&plan.project_dir)) {
            Ok(found) => found,
            Err(e) => return ctx.refuse(err, &tree_refusal(&plan.project, &e)),
        }
    } else {
        None
    };
    let mut check_run_id = None;
    if plan.check.is_some() {
        let now = now_ms();
        let (lines, _) = journal::read_recent(home, now);
        match check_step(&plan, call.params, &lines, tree.as_ref(), now) {
            Ok(found) => check_run_id = found,
            Err(refusal) => return ctx.refuse(err, &refusal),
        }
    }
    // `auto` never passes validation (v0.5); were it to, it would still wait
    // for the author.
    if matches!(plan.class, ActionClass::Human | ActionClass::Auto) {
        let bound = Bound {
            check_run_id: check_run_id.as_deref(),
            tree: tree.as_ref(),
        };
        return match approval(&ctx, err, &plan, bound, call.wait) {
            Ok(approved) => execute(
                &ctx,
                &approved.plan,
                tree,
                Some(approved.id),
                approved.secret,
                out,
                err,
            ),
            Err(code) => code,
        };
    }
    // After every check, just before the request (D-29).
    let secret = match read_secret(&plan, store) {
        Ok(secret) => secret,
        Err(refusal) => return ctx.refuse(err, &refusal),
    };
    execute(&ctx, &plan, tree, None, secret, out, err)
}

/// One call as the steps after its plan see it.
struct Context<'a> {
    home: &'a RelayHome,
    cwd: &'a str,
    action: &'a str,
    params: &'a BTreeMap<String, String>,
    session_id: Option<&'a str>,
    store: &'a dyn CredentialStore,
}

impl Context<'_> {
    /// One line on `err` and one `refused` line (PRD amendment 11). A
    /// journal that cannot be written does not add a second line: the
    /// refusal stands.
    fn refuse(&self, err: &mut dyn Write, refusal: &Refusal) -> i32 {
        let _ = writeln!(
            err,
            "officina: refused: {} ({})",
            refusal.detail,
            refusal.reason.code()
        );
        let line = JournalEntry::Refused(Refused {
            project: refusal.project.clone(),
            session_id: self.session_id.map(str::to_owned),
            action: self.action.to_owned(),
            params: Some(self.params.clone()),
            reason: refusal.reason,
            detail: refusal.detail.clone(),
        });
        let _ = journal::append(self.home, &[line], now_ms());
        EXIT_REFUSED
    }
}

/// The refusal for a work tree that cannot be bound (D-173).
fn tree_refusal(project: &str, error: &TreeError) -> Refusal {
    let why = error.to_string();
    match error {
        TreeError::GitUnavailable(_) => Refusal::git_unavailable(project, &why),
        TreeError::TooLarge(_) => Refusal::tree_too_large(project, &why),
        TreeError::Unreadable(_) => Refusal::tree_unreadable(project, &why),
    }
}

/// The secret of the credential an HTTP action's plan carries, read from
/// `store`; `None` for a plan without one. A missing or unreadable
/// credential is a refusal, before any connection (D-29).
fn read_secret(plan: &RunPlan, store: &dyn CredentialStore) -> Result<Option<Secret>, Refusal> {
    let Some(binding) = plan.http().and_then(|http| http.credential.as_ref()) else {
        return Ok(None);
    };
    match store.read(&binding.name) {
        Ok(Some(secret)) => Ok(Some(secret)),
        Ok(None) => Err(Refusal::credential_missing(&plan.project, &binding.name)),
        Err(e) => Err(Refusal::credential_unreadable(
            &plan.project,
            &binding.name,
            &e.to_string(),
        )),
    }
}

/// Journals `run_started`, then starts the process or sends the request.
fn execute(
    ctx: &Context<'_>,
    plan: &RunPlan,
    tree: Option<officina_core::relay::journal::TreeFingerprint>,
    approval_id: Option<String>,
    secret: Option<Secret>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let run_id = officina_core::relay::approvals::run_id(now_ms(), std::process::id());
    let start = start_line(
        &run_id,
        plan,
        Started {
            params: ctx.params,
            session_id: ctx.session_id,
            tree,
            approval_id,
        },
    );
    if let Err(e) = journal::append(ctx.home, &[start], now_ms()) {
        let _ = writeln!(
            err,
            "officina: refused: {e}, so nothing ran (journal_unavailable)"
        );
        return EXIT_REFUSED;
    }
    match (plan.launch(), plan.http()) {
        (Some(launch), _) => spawn::spawn(ctx.home, plan, launch, &run_id, err),
        (None, Some(request)) => {
            send::send_request(ctx.home, plan, request, &run_id, secret, out, err)
        }
        (None, None) => EXIT_NOT_STARTED,
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::Fixture;
    use super::*;
    use officina_core::relay::journal::RefusalReason;

    /// A refusal is one line on the given stream and one journal line, in
    /// process: nothing reaches this process's stderr.
    #[test]
    fn a_refusal_is_said_on_err_and_journaled() {
        let fixture = Fixture::human();
        std::fs::remove_file(fixture.home.approved_file()).unwrap();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let call = Call {
            action: "restart",
            params: &fixture.params,
            session_id: Some("00000000-0000-0000-0000-000000000001".to_owned()),
            wait: Duration::from_secs(1),
        };
        let code = run_with(
            &fixture.home,
            &fixture.cwd(),
            &call,
            &SystemCredentialStore,
            &mut out,
            &mut err,
        );
        assert_eq!(code, EXIT_REFUSED);
        assert!(out.is_empty());
        let said = String::from_utf8_lossy(&err);
        assert_eq!(said.lines().count(), 1, "{said}");
        assert!(
            said.trim_end()
                .ends_with(&format!("({})", RefusalReason::NotApproved.code())),
            "{said}"
        );
        match fixture.entries().as_slice() {
            [JournalEntry::Refused(refused)] => {
                assert_eq!(refused.reason, RefusalReason::NotApproved);
                assert_eq!(refused.session_id, call.session_id);
            }
            other => panic!("{other:?}"),
        }
    }
}
