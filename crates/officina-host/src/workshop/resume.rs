//! Resume (AD-116, FR-14, D-28): from the window only, on a pointer click,
//! since resuming starts collections and only the window holds the
//! authority (AD-21). The dialog lists the kept runs, each to collect (the
//! default) or remove, and offers to resume the relay too.

use officina_core::ipc::{IpcError, IpcErrorCode};
use officina_core::relay::journal::{JournalEntry, WorkshopResumed};
use officina_core::workshop::lifecycle::RunEvent;
use officina_core::workshop::run::{
    KeptChoice, RemovalCause, ResumeAnswer, ResumeChoice, RunState, resume_plan,
};

use crate::collector::Collector;
use crate::registry::window::WindowRegistry;
use crate::watch::{AUTHOR_WAIT, AuthorAnswer, AuthorCommand, ask};
use crate::workshop::follow::FollowContext;

/// What the collector decided at Resume: each kept run's choice, the
/// floor's key the removals act on, and whether the relay resumed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumePlan {
    pub runs: Vec<(String, KeptChoice)>,
    pub floor_key: Option<String>,
    pub relay_resumed: bool,
}

impl ResumePlan {
    fn count(&self, choice: KeptChoice) -> u32 {
        let n = self.runs.iter().filter(|(_, c)| *c == choice).count();
        u32::try_from(n).unwrap_or(u32::MAX)
    }
}

/// The collector's half of Resume (AD-116): the workshop's flag deleted,
/// the relay's too when asked (through the relay's own toggle, which
/// journals `relay_resumed`), `workshop_resumed` journaled with the
/// counts, and each run to collect moved to `collecting`.
pub(crate) fn resume_on_collector(
    collector: &Collector,
    registry: &mut WindowRegistry,
    choices: &[ResumeChoice],
    resume_relay: bool,
    now_ms: i64,
) -> Result<AuthorAnswer, IpcError> {
    let kept: Vec<String> = registry
        .workshop_runs_in(&[RunState::Stopped])?
        .into_iter()
        .map(|row| row.id)
        .collect();
    let runs = resume_plan(&kept, choices);
    let relay = collector.relay();
    let flag = crate::workshop::stopped_file(relay.officina_home());
    match std::fs::remove_file(&flag) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(IpcError::internal(format!("{}: {e}", flag.display())));
        }
    }
    let relay_resumed = resume_relay
        && crate::relay::authority::set_relay_stopped(relay, false).is_ok()
        && !relay.relay_stopped();
    let floor_key =
        crate::relay::home::load_relay_floor(&relay.relay_floor(&relay.read_approved()))
            .ok()
            .map(|floor| officina_core::workshop::names::floor_key(&floor.floor_path));
    let plan = ResumePlan {
        runs,
        floor_key,
        relay_resumed,
    };
    let line = JournalEntry::WorkshopResumed(WorkshopResumed {
        collected: plan.count(KeptChoice::Collect),
        removed: plan.count(KeptChoice::Remove),
        relay_resumed,
    });
    crate::relay::journal::record(relay, &[line], now_ms)
        .map_err(|e| IpcError::new(IpcErrorCode::JournalUnavailable, e.to_string()))?;
    for (id, choice) in &plan.runs {
        if *choice == KeptChoice::Collect {
            registry.move_workshop_run(id, RunEvent::Resumed, now_ms, |_| {})?;
        }
    }
    Ok(AuthorAnswer::Resumed(Box::new(plan)))
}

/// `resume_workshop(choices, resume_relay)` (AD-116): the collector's half,
/// the latch opened, then each kept run collected (story 6.13, the packer
/// emptying `out/` first) or its objects removed on thread `workshop`
/// (`discarded`, reason `not_collected`, `workshop_removed` cause
/// `resume`). A removal the engine refuses leaves the run `stopped`.
pub fn resume_workshop(
    context: &FollowContext,
    choices: &[ResumeChoice],
    resume_relay: bool,
) -> Result<ResumeAnswer, IpcError> {
    let plan = match ask(
        &context.collector,
        AuthorCommand::ResumeWorkshop {
            choices: choices.to_vec(),
            resume_relay,
        },
        AUTHOR_WAIT,
    )? {
        AuthorAnswer::Resumed(plan) => *plan,
        other => {
            return Err(IpcError::internal(format!("unexpected answer: {other:?}")));
        }
    };
    context.runtime.thread.set_latch(false);
    let mut removals = Vec::new();
    for (id, choice) in &plan.runs {
        match choice {
            KeptChoice::Collect => crate::workshop::collect::collect_on_thread(context, id),
            KeptChoice::Remove => removals.push(id.clone()),
        }
    }
    if let (false, Some(floor_key)) = (removals.is_empty(), plan.floor_key.clone()) {
        let job = context.clone();
        context.runtime.thread.submit(move || {
            let removed: Vec<String> = removals
                .into_iter()
                .filter(|id| {
                    crate::workshop::stop::remove_run_objects(&job, id, &floor_key).is_some()
                })
                .collect();
            if !removed.is_empty() {
                let _ = ask(
                    &job.collector,
                    AuthorCommand::RemoveKept {
                        ids: removed,
                        cause: RemovalCause::Resume,
                    },
                    AUTHOR_WAIT,
                );
            }
        });
    }
    Ok(ResumeAnswer {
        collected: plan.count(KeptChoice::Collect),
        removed: plan.count(KeptChoice::Remove),
        relay_resumed: plan.relay_resumed,
    })
}
