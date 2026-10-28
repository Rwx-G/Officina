//! A `human` action's request and its wait (AD-33 steps 1 to 4): the
//! request written with its journal line, then a stat every tick until the
//! author decides, the relay stops, or the wait runs out.

use std::io::Write;
use std::time::{Duration, Instant};

use officina_core::relay::approvals::{
    ApprovalRequest, FileKind, HttpRequestView, KeyInputs, NextStep, WAIT_TICK_MS, next_step,
    request_id, request_key,
};
use officina_core::relay::journal::{ApprovalEvent, JournalEntry, TreeFingerprint};
use officina_core::relay::run::{EXIT_REFUSED, Refusal, RunPlan};

use super::Context;
use super::consume::{Consumed, consume};
use crate::files::now_ms;
use crate::relay::approvals;
use crate::relay::credential::Secret;
use crate::relay::journal::JournalLock;

/// What a `human` action's request is bound to besides its parameters: the
/// dry run it was made after, and the files, the ones the dry run ran on
/// when there is one (D-170, D-173).
#[derive(Clone, Copy)]
pub(super) struct Bound<'a> {
    pub check_run_id: Option<&'a str>,
    pub tree: Option<&'a TreeFingerprint>,
}

/// An approval used: its id, the plan checked again just before, and the
/// credential read before the approval was consumed (D-29).
pub(super) struct Approved {
    pub id: String,
    pub plan: RunPlan,
    pub secret: Option<Secret>,
}

/// AD-33 steps 1 to 4 for a `human` action: refused after a recent
/// rejection; an approval for this request consumed and the plan returned
/// with its id; otherwise the pending request reused, or a new one written
/// and journaled, then waited for, a stat every [`WAIT_TICK_MS`] with the
/// stop flag read each tick, until a decision or `wait`. Errors are exit
/// codes, the refusal already said on `err` and journaled.
pub(super) fn approval(
    ctx: &Context<'_>,
    err: &mut dyn Write,
    plan: &RunPlan,
    bound: Bound<'_>,
    wait: Duration,
) -> Result<Approved, i32> {
    let key = request_key(&KeyInputs {
        project: &plan.project,
        action: &plan.action,
        params: ctx.params,
        catalogue_hash: &plan.catalogue_hash,
        check_run_id: bound.check_run_id,
        tree: bound.tree,
    });
    let deadline = Instant::now() + wait;
    loop {
        let files = approvals::read_files(ctx.home);
        let id = match next_step(&files, &key, now_ms()) {
            NextStep::Rejected { .. } => {
                return Err(ctx.refuse(err, &Refusal::approval_rejected(&plan.project)));
            }
            NextStep::Consume { id } => match consume(ctx, plan, bound, &id) {
                Consumed::Done(fresh, secret) => {
                    return Ok(Approved {
                        id,
                        plan: *fresh,
                        secret,
                    });
                }
                Consumed::Gone => continue,
                Consumed::Refused(refusal) => return Err(ctx.refuse(err, &refusal)),
            },
            NextStep::Wait { id } => id,
            NextStep::Create => create(ctx, err, plan, bound, &key)?,
        };
        loop {
            if ctx.home.relay_stopped() {
                return Err(ctx.refuse(err, &Refusal::stopped()));
            }
            match approvals::decision(ctx.home, &id) {
                Some(FileKind::Approved) => break,
                Some(FileKind::Rejected) => {
                    return Err(ctx.refuse(err, &Refusal::approval_rejected(&plan.project)));
                }
                _ => {}
            }
            if Instant::now() >= deadline {
                return Err(ctx.refuse(err, &Refusal::approval_pending(&plan.project)));
            }
            std::thread::sleep(Duration::from_millis(WAIT_TICK_MS));
        }
    }
}

/// Writes a new request and its `approval_requested` line under the journal
/// lock, so a request and its line go together (PRD amendment 11), unless a
/// concurrent call made the same request first. Returns its id.
fn create(
    ctx: &Context<'_>,
    err: &mut dyn Write,
    plan: &RunPlan,
    bound: Bound<'_>,
    key: &str,
) -> Result<String, i32> {
    let lock = match JournalLock::acquire(ctx.home) {
        Ok(lock) => lock,
        Err(e) => {
            let _ = writeln!(
                err,
                "officina: refused: {e}, so no approval was requested (journal_unavailable)"
            );
            return Err(EXIT_REFUSED);
        }
    };
    let now = now_ms();
    // Every request is written under the lock: a call that waited for it
    // while another made the same request reuses that one.
    if let NextStep::Wait { id } = next_step(&approvals::read_files(ctx.home), key, now) {
        return Ok(id);
    }
    let id = request_id(key, now);
    let request = ApprovalRequest {
        request_key: key.to_owned(),
        created_at_ms: now,
        project: plan.project.clone(),
        action: plan.action.clone(),
        params: ctx.params.clone(),
        argv: plan.argv(),
        http: plan.http().map(HttpRequestView::from),
        catalogue_hash: plan.catalogue_hash.clone(),
        session_id: ctx.session_id.map(str::to_owned),
        check_run_id: bound.check_run_id.map(str::to_owned),
        tree: bound.tree.cloned(),
    };
    if let Err(e) = approvals::create_request(ctx.home, &id, &request) {
        drop(lock);
        return Err(ctx.refuse(
            err,
            &Refusal::approval_unavailable(&plan.project, &e.to_string()),
        ));
    }
    let line = JournalEntry::ApprovalRequested(ApprovalEvent {
        approval_id: id.clone(),
        request_key: key.to_owned(),
        project: Some(plan.project.clone()),
        action: Some(plan.action.clone()),
        dry_run: Some(plan.check.is_some()),
    });
    if lock.append(&[line], now).is_err() {
        let _ = writeln!(
            err,
            "officina: the journal could not record approval request {id}"
        );
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::super::fixture::Fixture;
    use super::*;
    use officina_core::relay::journal::RefusalReason;

    /// A request is written with its journal line before the wait, and the
    /// wait reads the stop flag: a relay stopped meanwhile refuses `stopped`.
    #[test]
    fn a_request_is_journaled_and_a_stop_ends_the_wait() {
        let fixture = Fixture::human();
        let plan = fixture.plan();
        std::fs::write(fixture.home.stopped_file(), b"").unwrap();
        let mut err = Vec::new();
        let bound = Bound {
            check_run_id: None,
            tree: None,
        };
        let refused = approval(
            &fixture.context(),
            &mut err,
            &plan,
            bound,
            Duration::from_secs(5),
        );
        assert_eq!(refused.err(), Some(EXIT_REFUSED));
        let said = String::from_utf8_lossy(&err);
        assert_eq!(said.lines().count(), 1, "{said}");
        assert!(
            said.trim_end()
                .ends_with(&format!("({})", RefusalReason::Stopped.code())),
            "{said}"
        );
        let kinds: Vec<&str> = fixture
            .entries()
            .iter()
            .map(|entry| entry.kind().code())
            .collect();
        assert_eq!(kinds, ["approval_requested", "refused"]);
        assert_eq!(approvals::read_files(&fixture.home).requests.len(), 1);
    }
}
