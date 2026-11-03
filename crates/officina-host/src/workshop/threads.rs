//! The window's workshop threads (AD-98 as AD-3 amends it, D-239): the
//! collector never runs `docker` or the workshop's `git`. Thread `workshop`
//! runs short engine and git jobs one at a time; thread `workshop-build`
//! runs builds, inventories and batteries. Each job's outcome returns to the
//! collector as an [`AuthorRequest`] the collector never drops (D-135's
//! outcome pattern).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

use crate::process::{Interrupts, watch_interrupts};
use crate::watch::{AUTHOR_WAIT, AuthorCommand, AuthorRequest, CollectorMsg};
use crate::workshop::engine::{ResolveError, ResolvedDocker, read_engine_state};

type Job = Box<dyn FnOnce() + Send>;

/// What Stop all holds over one job thread (AD-116, D-241): the
/// interrupts that end its running engine call, and the latch its
/// launches read right before each `start`, under a gate Stop all takes to
/// know no `start` that read the latch open is still in flight.
#[derive(Debug, Default)]
struct Control {
    interrupts: Arc<Interrupts>,
    latched: AtomicBool,
    start_gate: Mutex<()>,
}

/// One named thread that runs its jobs one at a time, in the order given.
#[derive(Debug, Clone)]
pub struct JobThread {
    jobs: Sender<Job>,
    control: Arc<Control>,
}

/// Held across one `start` (D-241): while a launch holds it, Stop all's
/// acknowledgement waits.
pub struct StartGate<'a>(#[expect(dead_code, reason = "held for its lock")] MutexGuard<'a, ()>);

impl JobThread {
    /// Starts the thread named `name`; it ends once the handle is dropped
    /// and its queued jobs ran. Its engine calls end when Stop all fires
    /// its interrupts.
    pub fn spawn(name: &str) -> std::io::Result<JobThread> {
        let (jobs, inbox) = mpsc::channel::<Job>();
        let control = Arc::new(Control::default());
        let interrupts = Arc::clone(&control.interrupts);
        std::thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                watch_interrupts(interrupts);
                while let Ok(job) = inbox.recv() {
                    job();
                }
            })?;
        Ok(JobThread { jobs, control })
    }

    /// Queues `job`; false when the thread is gone.
    pub fn submit(&self, job: impl FnOnce() + Send + 'static) -> bool {
        self.jobs.send(Box::new(job)).is_ok()
    }

    /// Ends the engine call the thread is running now, with its tree
    /// (AD-116); the job's next calls run.
    pub fn interrupt(&self) {
        self.control.interrupts.fire();
    }

    /// Sets or clears the latch the thread's launches read before each
    /// `start` (D-241).
    pub fn set_latch(&self, latched: bool) {
        self.control.latched.store(latched, Ordering::SeqCst);
    }

    pub fn latched(&self) -> bool {
        self.control.latched.load(Ordering::SeqCst)
    }

    /// The gate a launch holds from its read of the latch to the end of
    /// its `start` (D-241).
    pub fn start_gate(&self) -> StartGate<'_> {
        StartGate(
            self.control
                .start_gate
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    /// Waits until no `start` that read the latch open is in flight, or
    /// `deadline` passes: true for the acknowledgement (D-241).
    pub fn acknowledged(&self, deadline: Instant) -> bool {
        loop {
            match self.control.start_gate.try_lock() {
                Ok(_) | Err(TryLockError::Poisoned(_)) => return true,
                Err(TryLockError::WouldBlock) if Instant::now() >= deadline => return false,
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
    }
}

/// The two workshop threads of the window.
#[derive(Debug)]
pub struct WorkshopThreads {
    /// Short engine and git jobs: launch steps, stopping one run,
    /// collection, removal, reconciliation.
    pub workshop: JobThread,
    /// Builds, inventories and batteries.
    pub build: JobThread,
}

/// The name of the thread of short engine and git jobs.
pub const WORKSHOP_THREAD: &str = "workshop";
/// The name of the thread of builds, inventories and batteries.
pub const BUILD_THREAD: &str = "workshop-build";

impl WorkshopThreads {
    pub fn spawn() -> std::io::Result<WorkshopThreads> {
        Ok(WorkshopThreads {
            workshop: JobThread::spawn(WORKSHOP_THREAD)?,
            build: JobThread::spawn(BUILD_THREAD)?,
        })
    }

    /// Queues the window start's engine read on thread `workshop`: its
    /// state reaches the collector as [`AuthorCommand::EngineState`], a
    /// request the collector never drops.
    pub fn read_engine_at_start(
        &self,
        docker: Result<ResolvedDocker, ResolveError>,
        base: Vec<(String, String)>,
        home: PathBuf,
        collector: Sender<CollectorMsg>,
    ) -> bool {
        self.workshop.submit(move || {
            // `docker.exe` runs in the home (AD-98), which the collector may
            // not have created yet at window start.
            let _ = std::fs::create_dir_all(&home);
            let state = read_engine_state(docker.as_ref(), &base, &home);
            let (request, _answer) =
                AuthorRequest::new(AuthorCommand::EngineState(Box::new(state)), AUTHOR_WAIT);
            let _ = collector.send(CollectorMsg::Author(request));
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jobs_run_in_order_on_the_named_thread() {
        let thread = JobThread::spawn(WORKSHOP_THREAD).unwrap();
        let (tx, rx) = mpsc::channel();
        for i in 0..3 {
            let tx = tx.clone();
            assert!(thread.submit(move || {
                let name = std::thread::current().name().map(str::to_owned);
                tx.send((i, name)).unwrap();
            }));
        }
        let got: Vec<(i32, Option<String>)> = (0..3).map(|_| rx.recv().unwrap()).collect();
        for (i, (n, name)) in got.into_iter().enumerate() {
            assert_eq!(n, i32::try_from(i).unwrap());
            assert_eq!(name.as_deref(), Some(WORKSHOP_THREAD));
        }
    }

    /// D-241: the acknowledgement waits for a `start` in flight under the
    /// gate, and gives up at its deadline.
    #[test]
    fn the_acknowledgement_waits_for_a_start_in_flight() {
        let thread = JobThread::spawn(WORKSHOP_THREAD).unwrap();
        assert!(thread.acknowledged(Instant::now()));
        let (holding, held) = mpsc::channel();
        let (release, released) = mpsc::channel::<()>();
        let job = thread.clone();
        thread.submit(move || {
            let _gate = job.start_gate();
            holding.send(()).unwrap();
            let _ = released.recv();
        });
        held.recv().unwrap();
        assert!(!thread.acknowledged(Instant::now() + Duration::from_millis(50)));
        release.send(()).unwrap();
        assert!(thread.acknowledged(Instant::now() + Duration::from_secs(10)));
        thread.set_latch(true);
        assert!(thread.latched());
    }
}
