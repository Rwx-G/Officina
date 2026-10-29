//! Using an approval (AD-33 step 5, D-170, D-173): the last step before
//! the start line, every check made again before the claim.

use std::path::Path;

use officina_core::relay::catalogue::ActionClass;
use officina_core::relay::record::{RecordState, Verdict};
use officina_core::relay::run::{Refusal, RunPlan, auto_check_at_consumption, plan};

use super::request::Bound;
use super::{Context, read_secret, tree_refusal, with_inputs};
use crate::files::now_ms;
use crate::relay::approvals;
use crate::relay::credential::Secret;
use crate::relay::{journal, tree};

/// How using an approval went.
pub(super) enum Consumed {
    /// Used: the plan checked again just before, and its credential.
    Done(Box<RunPlan>, Option<Secret>),
    /// Another call used it first.
    Gone,
    /// Refused before using it: the approval stays as it was.
    Refused(Refusal),
}

/// Uses approval `id` for the request `key`, last before the spawn
/// (AD-33): the checks of [`plan`] again, the window's record and catalogue
/// included, an approval the record holds as approved for this request key
/// (D-173, D-175), the approval made under the same catalogue, the files
/// still the ones the request was bound to, its dry run's when it names a
/// check (D-170, D-173), for an `auto` action its check satisfied again on
/// those files at this moment, whoever approved (D-151), and an HTTP
/// action's credential read (D-29); then
/// the claim by exclusive create. Anything refused leaves the approval
/// unconsumed.
pub(super) fn consume(
    ctx: &Context<'_>,
    planned: &RunPlan,
    bound: Bound<'_>,
    id: &str,
    key: &str,
) -> Consumed {
    let fresh = match with_inputs(ctx.home, ctx.cwd, |inputs| {
        plan(inputs, &planned.action, ctx.params)
    }) {
        Ok(fresh) => fresh,
        Err(refusal) => return Consumed::Refused(refusal),
    };
    // An approval the window's record does not hold as approved for this
    // very request was written by another process (D-173, D-175). `plan`
    // refused a broken record; one broken since is refused the same way.
    match ctx.home.read_record() {
        RecordState::Valid(record) => {
            let given = record.decided.get(id).is_some_and(|recorded| {
                recorded.verdict == Verdict::Approved && recorded.request_key == key
            });
            if !given {
                return Consumed::Refused(Refusal::approval_not_given(&fresh.project, id));
            }
        }
        broken => {
            let why = broken.broken(true).unwrap_or_default();
            return Consumed::Refused(Refusal::decision_record_unreadable(&why));
        }
    }
    if fresh.catalogue_hash != planned.catalogue_hash {
        return Consumed::Refused(Refusal::catalogue_changed_since_request(&fresh.project));
    }
    let mut current = None;
    if fresh.binds_tree {
        match tree::fingerprint(Path::new(&fresh.project_dir)) {
            Err(e) => return Consumed::Refused(tree_refusal(&fresh.project, &e)),
            Ok(found) if found.as_ref() != bound.tree => {
                return Consumed::Refused(match &fresh.check {
                    Some(check) => Refusal::check_not_satisfied(&fresh.project, check),
                    None => Refusal::tree_changed(&fresh.project),
                });
            }
            Ok(found) => current = found,
        }
    }
    if fresh.class == ActionClass::Auto {
        let now = now_ms();
        let (lines, _) = journal::read_recent(ctx.home, now);
        if let Err(refusal) =
            auto_check_at_consumption(&fresh, ctx.params, &lines, current.as_ref(), now)
        {
            return Consumed::Refused(refusal);
        }
    }
    let secret = match read_secret(&fresh, ctx.store) {
        Ok(secret) => secret,
        Err(refusal) => return Consumed::Refused(refusal),
    };
    match approvals::consume(ctx.home, id) {
        Ok(()) => Consumed::Done(Box::new(fresh), secret),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Consumed::Gone,
        Err(e) => Consumed::Refused(Refusal::approval_unavailable(
            &fresh.project,
            &e.to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixture::Fixture;
    use super::*;
    use officina_core::relay::journal::RefusalReason;

    const ID: &str = "0123456789abcdef-1";
    const KEY: &str = "ab";
    const UNBOUND: Bound<'static> = Bound {
        check_run_id: None,
        tree: None,
    };

    fn refused(fixture: &Fixture, planned: &RunPlan) -> RefusalReason {
        match consume(&fixture.context(), planned, UNBOUND, ID, KEY) {
            Consumed::Refused(refusal) => refusal.reason,
            Consumed::Done(..) | Consumed::Gone => panic!("not refused"),
        }
    }

    /// An approval made under another catalogue is refused before the
    /// claim, and nothing is consumed.
    #[test]
    fn an_approval_under_another_catalogue_is_refused_unconsumed() {
        let fixture = Fixture::human();
        fixture.record(ID, Verdict::Approved, KEY);
        let mut planned = fixture.plan();
        planned.catalogue_hash = "0".repeat(64);
        assert_eq!(refused(&fixture, &planned), RefusalReason::CatalogueChanged);
        assert!(approvals::read_files(&fixture.home).consumed.is_empty());
    }

    /// D-175: an approval the window's record does not hold, holds as
    /// rejected, or holds for another request key is not used; nor is any
    /// while the record is missing or unreadable.
    #[test]
    fn an_approval_the_record_does_not_give_is_refused_unconsumed() {
        let fixture = Fixture::human();
        let planned = fixture.plan();
        assert_eq!(refused(&fixture, &planned), RefusalReason::ApprovalNotGiven);
        fixture.record(ID, Verdict::Rejected, KEY);
        assert_eq!(refused(&fixture, &planned), RefusalReason::ApprovalNotGiven);
        fixture.record(ID, Verdict::Approved, "another request");
        assert_eq!(refused(&fixture, &planned), RefusalReason::ApprovalNotGiven);
        std::fs::write(fixture.home.decisions_file(), b"{ not json").unwrap();
        assert_eq!(
            refused(&fixture, &planned),
            RefusalReason::DecisionRecordUnreadable
        );
        std::fs::remove_file(fixture.home.decisions_file()).unwrap();
        assert_eq!(
            refused(&fixture, &planned),
            RefusalReason::DecisionRecordUnreadable
        );
        assert!(approvals::read_files(&fixture.home).consumed.is_empty());
    }
}
