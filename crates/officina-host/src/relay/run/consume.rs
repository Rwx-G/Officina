//! Using an approval (AD-33 step 5, D-170, D-173): the last step before
//! the start line, every check made again before the claim.

use std::path::Path;

use officina_core::relay::run::{Refusal, RunPlan, plan};

use super::request::Bound;
use super::{Context, read_secret, tree_refusal, with_inputs};
use crate::relay::approvals;
use crate::relay::credential::Secret;
use crate::relay::tree;

/// How using an approval went.
pub(super) enum Consumed {
    /// Used: the plan checked again just before, and its credential.
    Done(Box<RunPlan>, Option<Secret>),
    /// Another call used it first.
    Gone,
    /// Refused before using it: the approval stays as it was.
    Refused(Refusal),
}

/// Uses approval `id`, last before the spawn (AD-33): an approval the
/// window's record names (D-173), the checks of [`plan`] again, the
/// approval made under the same catalogue, the files still the ones the
/// request was bound to, its dry run's when it names a check (D-170,
/// D-173), and an HTTP action's credential read (D-29); then the claim by
/// exclusive create. Anything refused leaves the approval unconsumed.
pub(super) fn consume(
    ctx: &Context<'_>,
    planned: &RunPlan,
    bound: Bound<'_>,
    id: &str,
) -> Consumed {
    // An approval the window's record does not name was written by another
    // process (D-173). Without a record, a window that has not run this
    // version yet, the approval file stands alone, as before.
    if let Some(record) = ctx.home.read_record()
        && !record.decided.contains_key(id)
    {
        return Consumed::Refused(Refusal::approval_not_given(&planned.project, id));
    }
    let fresh = match with_inputs(ctx.home, ctx.cwd, |inputs| {
        plan(inputs, &planned.action, ctx.params)
    }) {
        Ok(fresh) => fresh,
        Err(refusal) => return Consumed::Refused(refusal),
    };
    if fresh.catalogue_hash != planned.catalogue_hash {
        return Consumed::Refused(Refusal::catalogue_changed_since_request(&fresh.project));
    }
    if fresh.binds_tree {
        match tree::fingerprint(Path::new(&fresh.project_dir)) {
            Err(e) => return Consumed::Refused(tree_refusal(&fresh.project, &e)),
            Ok(current) if current.as_ref() != bound.tree => {
                return Consumed::Refused(match &fresh.check {
                    Some(check) => Refusal::check_not_satisfied(&fresh.project, check),
                    None => Refusal::tree_changed(&fresh.project),
                });
            }
            Ok(_) => {}
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

    /// An approval made under another catalogue is refused before the
    /// claim, and nothing is consumed.
    #[test]
    fn an_approval_under_another_catalogue_is_refused_unconsumed() {
        let fixture = Fixture::human();
        let mut planned = fixture.plan();
        planned.catalogue_hash = "0".repeat(64);
        let bound = Bound {
            check_run_id: None,
            tree: None,
        };
        match consume(&fixture.context(), &planned, bound, "0123456789abcdef-1") {
            Consumed::Refused(refusal) => {
                assert_eq!(refusal.reason, RefusalReason::CatalogueChanged);
            }
            Consumed::Done(..) | Consumed::Gone => panic!("not refused"),
        }
        assert!(approvals::read_files(&fixture.home).consumed.is_empty());
    }
}
