//! A run's life (AD-109, AD-116): the edges between its states, the cause
//! a finished run is given, and what reconciliation does with the rows
//! and the containers the engine lists. Pure: the host reads the engine
//! and the registry, core decides.

use serde::{Deserialize, Serialize};

use super::run::{FinishCause, RunState};
use super::vector::ContainerRole;

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What moves a run from one state to the next (AD-109, AD-116).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum RunEvent {
        /// The launch's starts succeeded (AD-107 step 4).
        Started,
        /// A launch step failed, or a `launching` row outlived 2 minutes.
        LaunchFailed,
        /// The window stopped the worker: a limit, a stream stop, the
        /// deadline (AD-109).
        StopAsked,
        /// The worker's container exited.
        Exited,
        /// Stop all kept the run's containers (AD-116).
        StopAll,
        /// Resume chose to collect a kept run.
        Resumed,
        /// Resume chose to remove a kept run, or it expired (AD-116).
        Removed,
        /// Collection produced a package to review (AD-110).
        Packaged,
        /// Collection quarantined the package (AD-110).
        Quarantined,
        /// Collection found nothing past the base.
        NothingChanged,
        /// Collection could not produce what it reads.
        CollectionFailed,
        /// The engine answered and lacks the run's container (principle 3).
        Lost,
        /// The author imported the package (AD-114).
        Imported,
        /// The author discarded the package (AD-114).
        Discarded,
    }
}

/// Every allowed edge (AD-109): from a state, on an event, to a state. A
/// pair absent here is refused.
pub const EDGES: &[(RunState, RunEvent, RunState)] = &[
    (RunState::Launching, RunEvent::Started, RunState::Running),
    (
        RunState::Launching,
        RunEvent::LaunchFailed,
        RunState::Failed,
    ),
    (RunState::Running, RunEvent::StopAsked, RunState::Stopping),
    (RunState::Running, RunEvent::Exited, RunState::Collecting),
    (RunState::Running, RunEvent::StopAll, RunState::Stopped),
    (RunState::Running, RunEvent::Lost, RunState::Unknown),
    (RunState::Stopping, RunEvent::Exited, RunState::Collecting),
    (RunState::Stopping, RunEvent::StopAll, RunState::Stopped),
    (RunState::Stopping, RunEvent::Lost, RunState::Unknown),
    (RunState::Collecting, RunEvent::Packaged, RunState::Review),
    (
        RunState::Collecting,
        RunEvent::Quarantined,
        RunState::Quarantined,
    ),
    (
        RunState::Collecting,
        RunEvent::NothingChanged,
        RunState::NoChange,
    ),
    (
        RunState::Collecting,
        RunEvent::CollectionFailed,
        RunState::Failed,
    ),
    (RunState::Collecting, RunEvent::Lost, RunState::Unknown),
    (RunState::Stopped, RunEvent::Resumed, RunState::Collecting),
    (RunState::Stopped, RunEvent::Removed, RunState::Discarded),
    (RunState::Stopped, RunEvent::Lost, RunState::Unknown),
    (RunState::Review, RunEvent::Imported, RunState::Imported),
    (RunState::Review, RunEvent::Discarded, RunState::Discarded),
    (
        RunState::Quarantined,
        RunEvent::Discarded,
        RunState::Discarded,
    ),
];

/// A transition no edge allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoSuchEdge {
    pub from: RunState,
    pub event: RunEvent,
}

/// The state `event` moves a run in `from` to, or why it cannot (AD-109).
///
/// ```
/// use officina_core::workshop::lifecycle::{RunEvent, transition};
/// use officina_core::workshop::run::RunState;
/// assert_eq!(transition(RunState::Launching, RunEvent::Started), Ok(RunState::Running));
/// assert!(transition(RunState::Imported, RunEvent::Started).is_err());
/// ```
pub fn transition(from: RunState, event: RunEvent) -> Result<RunState, NoSuchEdge> {
    EDGES
        .iter()
        .find(|(f, e, _)| *f == from && *e == event)
        .map(|(_, _, to)| *to)
        .ok_or(NoSuchEdge { from, event })
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Why the window stopped a worker (AD-109, AD-116).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum StopReason {
        Deadline,
        OutputCap,
        UnexpectedTool,
        HookFailed,
        AuthFailed,
        StopAll,
    }
}

/// The `result` subtype Claude Code ends a run with when its budget is
/// spent (the gate 6 entry).
pub const BUDGET_SUBTYPE: &str = "error_max_budget_usd";

/// The gateway's exit code when its refusal limit was reached (AD-103).
pub const GATEWAY_LIMIT_EXIT: i64 = 3;

/// What a run's cause is decided from (AD-109).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FinishInput {
    /// The engine no longer knows the worker's container.
    pub lost: bool,
    pub stop: Option<StopReason>,
    pub oom_killed: bool,
    pub exit_code: Option<i64>,
    pub gateway_exit: Option<i64>,
    /// The last `result`'s subtype, display only except for the budget.
    pub result_subtype: Option<String>,
}

/// A finished run's cause (AD-109): the engine losing it, the window's own
/// stop reason, the engine's memory kill, the gateway's limit, the budget,
/// then the exit code. A forged `result` can only name the budget, which
/// stops nothing more than an exit already did.
pub fn finish_cause(input: &FinishInput) -> FinishCause {
    if input.lost {
        return FinishCause::EngineLost;
    }
    if let Some(stop) = input.stop {
        return match stop {
            StopReason::Deadline => FinishCause::Deadline,
            StopReason::OutputCap => FinishCause::OutputCap,
            StopReason::UnexpectedTool => FinishCause::UnexpectedTool,
            StopReason::HookFailed => FinishCause::HookFailed,
            StopReason::AuthFailed => FinishCause::AuthFailed,
            StopReason::StopAll => FinishCause::Stopped,
        };
    }
    if input.oom_killed {
        return FinishCause::Memory;
    }
    if input.gateway_exit == Some(GATEWAY_LIMIT_EXIT) {
        return FinishCause::GatewayLimit;
    }
    if input.exit_code != Some(0) && input.result_subtype.as_deref() == Some(BUDGET_SUBTYPE) {
        return FinishCause::Budget;
    }
    match input.exit_code {
        Some(0) => FinishCause::Exited,
        _ => FinishCause::Failed,
    }
}

/// The causes PRD section 7's Environment failures counts (AD-109, D-217):
/// the gateway's limit and a hook that could not run.
pub const ENVIRONMENT_FAILURE_CAUSES: &[FinishCause] =
    &[FinishCause::GatewayLimit, FinishCause::HookFailed];

/// True when a run counts as an environment failure (D-217): a cause of
/// [`ENVIRONMENT_FAILURE_CAUSES`], or a `failed` outcome with at least one
/// gateway refusal; a `no_change` run never does.
pub fn is_environment_failure(
    cause: Option<FinishCause>,
    outcome: Option<super::run::WorkshopOutcome>,
    refused: u32,
) -> bool {
    use super::run::WorkshopOutcome;
    if outcome == Some(WorkshopOutcome::NoChange) {
        return false;
    }
    cause.is_some_and(|c| ENVIRONMENT_FAILURE_CAUSES.contains(&c))
        || (outcome == Some(WorkshopOutcome::Failed) && refused > 0)
}

/// How long a `launching` row may wait before reconciliation fails it
/// (AD-81's rule as AD-109 applies it).
pub const LAUNCHING_GRACE_MS: i64 = 2 * 60 * 1000;

/// How long past its deadline a worker runs before the window stops it
/// (AD-109).
pub const DEADLINE_GRACE_MS: i64 = 60 * 1000;

/// A run row as reconciliation reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowSeen {
    pub id: String,
    pub state: RunState,
    pub started_at_ms: i64,
    /// Relative to `started_at_ms`.
    pub deadline_ms: i64,
    /// A package collection left on the host.
    pub has_package: bool,
}

/// A labelled container the engine listed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerSeen {
    pub name: String,
    /// `officina.run`, when the container carries it.
    pub run_id: Option<String>,
    /// `officina.role`'s text: a role, or `login`.
    pub role: String,
    pub running: bool,
}

/// What reconciliation does (AD-109).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ReconcileAction {
    /// Follow the run's containers.
    Follow { run_id: String },
    /// Follow, and stop the worker: it is past its deadline plus the grace.
    StopPastDeadline { run_id: String },
    /// The worker exited: follow to the end, then collect.
    Collect { run_id: String },
    /// A launch that never came back: `failed`, its objects removed.
    FailLaunch { run_id: String },
    /// The engine lacks the container and no package exists: `unknown`.
    Lost { run_id: String },
    /// A run container of this floor with no row: listed, never adopted.
    Orphan { name: String },
}

/// True for the roles a run owns (AD-109): a job's containers belong to
/// their job and the owner's login container to him (D-605).
pub fn is_run_role(role: &str) -> bool {
    [
        ContainerRole::Worker,
        ContainerRole::Gateway,
        ContainerRole::Packer,
    ]
    .iter()
    .any(|r| r.code() == role)
}

/// Reconciliation (AD-109, D-216) of the registry's `rows` against the
/// `containers` the engine listed at `now_ms`. Only runs not terminal and
/// not kept are judged; an engine that did not answer is not reconciled at
/// all (the caller's rule: no row changes until it answers).
pub fn reconcile(
    rows: &[RowSeen],
    containers: &[ContainerSeen],
    now_ms: i64,
) -> Vec<ReconcileAction> {
    let worker_of = |id: &str| {
        containers
            .iter()
            .find(|c| c.run_id.as_deref() == Some(id) && c.role == ContainerRole::Worker.code())
    };
    let mut actions = Vec::new();
    for row in rows {
        let id = row.id.clone();
        match row.state {
            RunState::Launching if now_ms - row.started_at_ms > LAUNCHING_GRACE_MS => {
                actions.push(ReconcileAction::FailLaunch { run_id: id });
            }
            RunState::Running | RunState::Stopping => match worker_of(&row.id) {
                Some(worker) if worker.running => {
                    if now_ms > row.started_at_ms + row.deadline_ms + DEADLINE_GRACE_MS {
                        actions.push(ReconcileAction::StopPastDeadline { run_id: id });
                    } else {
                        actions.push(ReconcileAction::Follow { run_id: id });
                    }
                }
                Some(_) => actions.push(ReconcileAction::Collect { run_id: id }),
                None if row.has_package => actions.push(ReconcileAction::Collect { run_id: id }),
                None => actions.push(ReconcileAction::Lost { run_id: id }),
            },
            RunState::Collecting => match worker_of(&row.id) {
                Some(_) => actions.push(ReconcileAction::Collect { run_id: id }),
                None if row.has_package => actions.push(ReconcileAction::Collect { run_id: id }),
                None => actions.push(ReconcileAction::Lost { run_id: id }),
            },
            _ => {}
        }
    }
    for container in containers {
        if !is_run_role(&container.role) {
            continue;
        }
        let owned = container
            .run_id
            .as_deref()
            .is_some_and(|id| rows.iter().any(|row| row.id == id));
        if !owned {
            actions.push(ReconcileAction::Orphan {
                name: container.name.clone(),
            });
        }
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AD-109, D-605: only a run's roles are judged; a job's container and
    /// the owner's login container with no row are never orphans.
    #[test]
    fn job_and_login_containers_are_never_orphans() {
        let roles = ContainerRole::ALL
            .iter()
            .map(|r| r.code())
            .chain([crate::workshop::names::LOGIN_ROLE]);
        for role in roles {
            let container = ContainerSeen {
                name: format!("officina-{role}-x"),
                run_id: Some("f".repeat(32)),
                role: role.to_owned(),
                running: true,
            };
            let orphaned = !reconcile(&[], &[container], 0).is_empty();
            assert_eq!(orphaned, is_run_role(role), "{role}");
        }
        assert!(is_run_role(ContainerRole::Worker.code()));
    }

    /// D-217: the causes, a failed outcome with a refusal, never
    /// `no_change`.
    #[test]
    fn environment_failures_follow_d217() {
        use crate::workshop::run::WorkshopOutcome;
        for cause in FinishCause::ALL {
            assert_eq!(
                is_environment_failure(Some(*cause), Some(WorkshopOutcome::Package), 0),
                ENVIRONMENT_FAILURE_CAUSES.contains(cause),
                "{cause:?}"
            );
        }
        assert!(is_environment_failure(
            Some(FinishCause::Exited),
            Some(WorkshopOutcome::Failed),
            1
        ));
        assert!(!is_environment_failure(
            Some(FinishCause::Exited),
            Some(WorkshopOutcome::Failed),
            0
        ));
        assert!(!is_environment_failure(
            Some(FinishCause::GatewayLimit),
            Some(WorkshopOutcome::NoChange),
            3
        ));
    }

    /// Principle 3: a run the engine lost is never finished, whatever its
    /// last exit code said.
    #[test]
    fn a_lost_run_is_never_finished() {
        let input = FinishInput {
            lost: true,
            exit_code: Some(0),
            ..FinishInput::default()
        };
        assert_eq!(finish_cause(&input), FinishCause::EngineLost);
    }
}
