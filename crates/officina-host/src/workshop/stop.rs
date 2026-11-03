//! Stop all (AD-116, D-241): the flags, one `ps` and one `stop` through the
//! engine's read half, and, in the window, the latch, the interrupts, the
//! acknowledgement and the second pass; kept containers removed at 7 days
//! and by `remove_run`. `officina stop --all` shares the flags and the pass,
//! with no authority.

use std::path::Path;

use officina_core::relay::StopBy;
use officina_core::relay::journal::{JournalEntry, WorkshopStopped};
use officina_core::workshop::engine::{EngineCall, parse_ps};
use officina_core::workshop::run::{StopScope, stop_targets};

use crate::relay::home::{RelayHome, StopFlags, write_both_flags};
use crate::workshop::engine::{ResolvedDocker, execute};

/// One `ps` and one `stop --time 3` naming every running managed
/// container of the floor whose key is given, or of every floor (AD-116):
/// how many it stopped, or `None` when the engine gave no answer.
pub fn stop_pass(
    docker: &ResolvedDocker,
    base: &[(String, String)],
    home: &Path,
    floor_key: Option<&str>,
) -> Option<u32> {
    let listed = execute(docker, &EngineCall::ps(), base, home, None)
        .ok()
        .filter(crate::workshop::engine::EngineOutcome::succeeded)?;
    let targets = stop_targets(
        &parse_ps(&String::from_utf8_lossy(&listed.stdout)),
        floor_key,
    );
    if targets.is_empty() {
        return Some(0);
    }
    execute(docker, &EngineCall::stop(&targets), base, home, None)
        .ok()
        .filter(crate::workshop::engine::EngineOutcome::succeeded)
        .map(|_| u32::try_from(targets.len()).unwrap_or(u32::MAX))
}

/// What `officina stop --all` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliStop {
    /// The flags this call wrote; a present one was left.
    pub written: StopFlags,
    /// The containers stopped, or `None` when the engine could not be
    /// reached: containers may still run.
    pub containers: Option<u32>,
    /// False when the journal line could not be written.
    pub journaled: bool,
}

/// `officina stop --all` (AD-116, D-215): both flags written when missing
/// and never removed, then every `officina.managed` running container of
/// the engine stopped, of any floor, since the CLI cannot know the
/// window's (D-30) and stopping more is safe; one journal line.
pub fn stop_from_cli(
    relay: &RelayHome,
    docker: Option<&ResolvedDocker>,
    base: &[(String, String)],
    now_ms: i64,
) -> std::io::Result<CliStop> {
    let written = write_both_flags(relay, StopBy::Cli, now_ms)?;
    let home = relay.officina_home();
    let _ = std::fs::create_dir_all(home);
    let containers = docker.and_then(|docker| stop_pass(docker, base, home, None));
    let line = JournalEntry::WorkshopStopped(WorkshopStopped {
        scope: StopScope::All,
        by: StopBy::Cli,
        containers: containers.unwrap_or(0),
    });
    let journaled = crate::relay::journal::record(relay, &[line], now_ms).is_ok();
    Ok(CliStop {
        written,
        containers,
        journaled,
    })
}

#[cfg(feature = "window-authority")]
pub use window::*;

#[cfg(feature = "window-authority")]
mod window {
    use std::time::{Duration, Instant};

    use officina_core::ipc::{IpcError, IpcErrorCode};
    use officina_core::relay::StopBy;
    use officina_core::relay::journal::{
        JournalEntry, ReviewDiscarded, WorkshopRemoved, WorkshopStopped,
    };
    use officina_core::workshop::engine::{EngineCall, parse_object_rows, parse_ps};
    use officina_core::workshop::lifecycle::{RunEvent, StopReason};
    use officina_core::workshop::names::RunId;
    use officina_core::workshop::review::DiscardReason;
    use officina_core::workshop::run::{
        FinishCause, Recorded, RemovalCause, RemoveRunAnswer, RemoveRunRefusal, RunState,
        StopAnswer, StopScope, WorkshopOutcome, kept_expired, run_objects,
    };

    use super::stop_pass;
    use crate::collector::Collector;
    use crate::registry::window::WindowRegistry;
    use crate::relay::home::{RelayHome, write_stop_flag};
    use crate::watch::{AUTHOR_WAIT, AuthorAnswer, AuthorCommand, ask};
    use crate::workshop::engine::{EngineOutcome, execute};
    use crate::workshop::follow::{FollowContext, RunSummary, finished_line};
    use crate::workshop::threads::JobThread;

    /// How long Stop all waits for thread `workshop`'s acknowledgement,
    /// leaving the second pass the rest of the budget (D-241).
    const ACK_WAIT: Duration = Duration::from_secs(4);

    /// The stopped runs with their stop times, and the floor's key.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct KeptRuns {
        pub runs: Vec<(String, Option<i64>)>,
        pub floor_key: Option<String>,
    }

    fn internal(e: impl std::fmt::Display) -> IpcError {
        IpcError::internal(e.to_string())
    }

    fn kept(context: &FollowContext) -> Result<KeptRuns, IpcError> {
        match ask(&context.collector, AuthorCommand::KeptRuns, AUTHOR_WAIT)? {
            AuthorAnswer::KeptRuns(kept) => Ok(*kept),
            other => Err(internal(format!("unexpected answer: {other:?}"))),
        }
    }

    /// Stop all from the window (AD-116, D-241), on its command's own
    /// thread, never behind a job: both flags, the latch, the run stops
    /// marked, the interrupts of thread `workshop` and of the build thread,
    /// one `ps` and `stop` of the floor's running containers, the
    /// acknowledgement, a second pass for a `start` the daemon accepted
    /// before the latch, then the live runs `stopped`. Containers are kept;
    /// the author's own sessions are untouched.
    pub fn stop_workshop(
        context: &FollowContext,
        build: Option<&JobThread>,
    ) -> Result<StopAnswer, IpcError> {
        let started = Instant::now();
        let now = (context.runtime.now_ms)();
        let relay = RelayHome::new(&context.home, None);
        // The relay's own toggle writes its record, flag and line; a flag
        // it could not write is written here, a stop never waiting on it.
        if crate::relay::authority::set_relay_stopped(&relay, true).is_err() {
            let _ = write_stop_flag(&relay.stopped_file(), StopBy::Window, now);
        }
        write_stop_flag(
            &crate::workshop::stopped_file(&context.home),
            StopBy::Window,
            now,
        )
        .map_err(internal)?;
        context.runtime.thread.set_latch(true);
        if let Ok(mut followed) = context.runtime.followed.lock() {
            for entry in followed.values_mut() {
                entry.stop.get_or_insert(StopReason::StopAll);
            }
        }
        context.runtime.thread.interrupt();
        if let Some(build) = build {
            build.interrupt();
        }
        let floor_key = kept(context)?.floor_key;
        let pass = || match (context.runtime.docker.as_ref(), floor_key.as_deref()) {
            (Some(docker), Some(key)) => {
                stop_pass(docker, &context.base_env, &context.home, Some(key))
            }
            // No approved floor: no run of this window can live.
            (Some(_), None) => Some(0),
            (None, _) => None,
        };
        let first = pass();
        let _ = context.runtime.thread.acknowledged(started + ACK_WAIT);
        let second = pass();
        let containers = first.unwrap_or(0).saturating_add(second.unwrap_or(0));
        let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let runs = match ask(
            &context.collector,
            AuthorCommand::WorkshopStopAll { containers },
            AUTHOR_WAIT,
        )? {
            AuthorAnswer::Stopped(runs) => runs,
            other => return Err(internal(format!("unexpected answer: {other:?}"))),
        };
        Ok(StopAnswer {
            containers,
            runs,
            engine_reached: first.is_some() && second.is_some(),
            elapsed_ms,
        })
    }

    /// The collector's half of Stop all (AD-116): every running, stopping
    /// or collecting run moved to `stopped` at `now_ms` with its stop time,
    /// a run without a cause given `stopped`; a launching run ends with its
    /// launch, which the latch stops. Journals `workshop_stopped`.
    pub(crate) fn stop_all_on_collector(
        collector: &Collector,
        registry: &mut WindowRegistry,
        containers: u32,
        now_ms: i64,
    ) -> Result<AuthorAnswer, IpcError> {
        let live = registry.workshop_runs_in(&[
            RunState::Running,
            RunState::Stopping,
            RunState::Collecting,
        ])?;
        let mut runs = 0u32;
        for row in live {
            let moved = registry.move_workshop_run(&row.id, RunEvent::StopAll, now_ms, |row| {
                row.stopped_at_ms = Some(now_ms);
                if row.cause.is_none() {
                    row.cause = Some(Recorded::Known(FinishCause::Stopped));
                }
            })?;
            runs += u32::from(moved.is_some());
        }
        let line = JournalEntry::WorkshopStopped(WorkshopStopped {
            scope: StopScope::Floor,
            by: StopBy::Window,
            containers,
        });
        crate::relay::journal::record(collector.relay(), &[line], now_ms)
            .map_err(|e| IpcError::new(IpcErrorCode::JournalUnavailable, e.to_string()))?;
        Ok(AuthorAnswer::Stopped(runs))
    }

    /// The stopped runs and the floor's key, read on the collector.
    pub(crate) fn kept_on_collector(
        collector: &Collector,
        registry: &mut WindowRegistry,
    ) -> Result<AuthorAnswer, IpcError> {
        let runs = registry
            .workshop_runs_in(&[RunState::Stopped])?
            .into_iter()
            .map(|row| (row.id, row.stopped_at_ms))
            .collect();
        let relay = collector.relay();
        let floor_key =
            crate::relay::home::load_relay_floor(&relay.relay_floor(&relay.read_approved()))
                .ok()
                .map(|floor| officina_core::workshop::names::floor_key(&floor.floor_path));
        Ok(AuthorAnswer::KeptRuns(Box::new(KeptRuns {
            runs,
            floor_key,
        })))
    }

    /// The collector's half of a kept run's removal (AD-116): `discarded`
    /// with reason `not_collected` and outcome `not_collected`, journaled
    /// `workshop_removed` with `cause`, `workshop_finished` and
    /// `review_discarded`. A run no longer kept changes nothing.
    pub(crate) fn remove_kept_on_collector(
        collector: &Collector,
        registry: &mut WindowRegistry,
        ids: &[String],
        cause: RemovalCause,
        now_ms: i64,
    ) -> Result<AuthorAnswer, IpcError> {
        let mut lines = Vec::new();
        for id in ids {
            let moved = registry.move_workshop_run(id, RunEvent::Removed, now_ms, |row| {
                row.discard_reason = Some(DiscardReason::NotCollected);
                row.outcome = Some(WorkshopOutcome::NotCollected);
            })?;
            let Some(row) = moved else {
                // An orphan has no kept row: its removal is journaled alone.
                if cause == RemovalCause::Orphan {
                    lines.push(JournalEntry::WorkshopRemoved(WorkshopRemoved {
                        run_id: id.clone(),
                        cause,
                    }));
                }
                continue;
            };
            let finish = match row.cause {
                Some(Recorded::Known(cause)) => cause,
                _ => FinishCause::Stopped,
            };
            lines.push(JournalEntry::WorkshopRemoved(WorkshopRemoved {
                run_id: id.clone(),
                cause,
            }));
            lines.push(finished_line(
                &row,
                finish,
                WorkshopOutcome::NotCollected,
                &RunSummary::default(),
                now_ms,
            ));
            lines.push(JournalEntry::ReviewDiscarded(ReviewDiscarded {
                run_id: id.clone(),
                reason: DiscardReason::NotCollected,
            }));
        }
        if !lines.is_empty() {
            crate::relay::journal::record(collector.relay(), &lines, now_ms)
                .map_err(|e| IpcError::new(IpcErrorCode::JournalUnavailable, e.to_string()))?;
        }
        Ok(AuthorAnswer::Done { created: None })
    }

    fn listing(context: &FollowContext, call: &EngineCall) -> Option<String> {
        let docker = context.runtime.docker.as_ref()?;
        execute(docker, call, &context.base_env, &context.home, None)
            .ok()
            .filter(EngineOutcome::succeeded)
            .map(|outcome| String::from_utf8_lossy(&outcome.stdout).into_owned())
    }

    /// Removes the engine objects labelled with `run_id` and `floor_key`
    /// (AD-121, D-215), and nothing else: how many, or `None` when the
    /// engine gave no listing or refused a removal.
    pub fn remove_run_objects(
        context: &FollowContext,
        run_id: &str,
        floor_key: &str,
    ) -> Option<u32> {
        let containers = parse_ps(&listing(context, &EngineCall::ps())?);
        let networks = parse_object_rows(&listing(context, &EngineCall::network_ls())?);
        let volumes = parse_object_rows(&listing(context, &EngineCall::volume_ls())?);
        let objects = run_objects(&containers, &networks, &volumes, run_id, floor_key);
        let docker = context.runtime.docker.as_ref()?;
        let mut calls = Vec::new();
        if !objects.containers.is_empty() {
            calls.push(EngineCall::rm(&objects.containers));
        }
        if !objects.networks.is_empty() {
            calls.push(EngineCall::network_rm(&objects.networks));
        }
        if !objects.volumes.is_empty() {
            calls.push(EngineCall::volume_rm(&objects.volumes));
        }
        for call in &calls {
            execute(docker, call, &context.base_env, &context.home, None)
                .ok()
                .filter(EngineOutcome::succeeded)?;
        }
        let count = objects.containers.len() + objects.networks.len() + objects.volumes.len();
        Some(u32::try_from(count).unwrap_or(u32::MAX))
    }

    /// Removes the kept runs past 7 days (AD-116, D-213), on the calling
    /// thread, even while stopped: removal only reduces. A run whose
    /// objects the engine could not remove is left `stopped` for the next
    /// pass. The ids removed.
    pub fn expire_kept(context: &FollowContext) -> Vec<String> {
        let Ok(kept) = kept(context) else {
            return Vec::new();
        };
        let Some(floor_key) = kept.floor_key else {
            return Vec::new();
        };
        let removed: Vec<String> = kept_expired(&kept.runs, (context.runtime.now_ms)())
            .into_iter()
            .filter(|id| remove_run_objects(context, id, &floor_key).is_some())
            .collect();
        if !removed.is_empty() {
            let _ = ask(
                &context.collector,
                AuthorCommand::RemoveKept {
                    ids: removed.clone(),
                    cause: RemovalCause::KeptExpired,
                },
                AUTHOR_WAIT,
            );
        }
        removed
    }

    /// Queues [`expire_kept`] on thread `workshop`: what the hourly pass
    /// runs (AD-116).
    pub fn expire_kept_on_thread(context: &FollowContext) {
        let job = context.clone();
        context.runtime.thread.submit(move || {
            expire_kept(&job);
        });
    }

    /// `remove_run(run_id)` (AD-121, D-215) on thread `workshop`: a kept
    /// run, or an orphan (no run of the registry still holds it), its
    /// objects labelled with the run and the window's floor removed. A
    /// kept run becomes `discarded`, reason `not_collected`.
    pub fn remove_run(context: &FollowContext, run_id: &str) -> Result<RemoveRunAnswer, IpcError> {
        let refused = |why| Ok(RemoveRunAnswer::Refused(why));
        if RunId::parse(run_id).is_none() {
            return refused(RemoveRunRefusal::MalformedId);
        }
        let kept_runs = kept(context)?;
        let Some(floor_key) = kept_runs.floor_key.clone() else {
            return refused(RemoveRunRefusal::NoFloor);
        };
        let is_kept = kept_runs.runs.iter().any(|(id, _)| id == run_id);
        if !is_kept {
            let row = match ask(
                &context.collector,
                AuthorCommand::WorkshopRun(run_id.to_owned()),
                AUTHOR_WAIT,
            )? {
                AuthorAnswer::WorkshopRun(row) => row,
                other => return Err(internal(format!("unexpected answer: {other:?}"))),
            };
            if row.is_some_and(|row| !row.state.is_terminal()) {
                return refused(RemoveRunRefusal::NotRemovable);
            }
        }
        let (done, answer) = std::sync::mpsc::channel();
        let job = context.clone();
        let id = run_id.to_owned();
        let submitted = context.runtime.thread.submit(move || {
            let _ = done.send(remove_run_objects(&job, &id, &floor_key));
        });
        if !submitted {
            return Err(internal("thread workshop has stopped"));
        }
        let Some(count) = answer
            .recv()
            .map_err(|_| internal("thread workshop has stopped"))?
        else {
            return refused(RemoveRunRefusal::EngineUnavailable);
        };
        let cause = if is_kept {
            RemovalCause::Resume
        } else {
            RemovalCause::Orphan
        };
        if is_kept || count > 0 {
            ask(
                &context.collector,
                AuthorCommand::RemoveKept {
                    ids: vec![run_id.to_owned()],
                    cause,
                },
                AUTHOR_WAIT,
            )?;
        }
        Ok(RemoveRunAnswer::Removed(count))
    }
}
