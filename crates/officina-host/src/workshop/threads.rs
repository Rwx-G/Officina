//! The window's workshop threads (AD-98 as AD-3 amends it, D-239): the
//! collector never runs `docker` or the workshop's `git`. Thread `workshop`
//! runs short engine and git jobs one at a time; thread `workshop-build`
//! runs builds, inventories and batteries. Each job's outcome returns to the
//! collector as an [`AuthorRequest`] the collector never drops (D-135's
//! outcome pattern).

use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};

use crate::watch::{AUTHOR_WAIT, AuthorCommand, AuthorRequest, CollectorMsg};
use crate::workshop::engine::{ResolveError, ResolvedDocker, read_engine_state};

type Job = Box<dyn FnOnce() + Send>;

/// One named thread that runs its jobs one at a time, in the order given.
#[derive(Debug, Clone)]
pub struct JobThread {
    jobs: Sender<Job>,
}

impl JobThread {
    /// Starts the thread named `name`; it ends once the handle is dropped
    /// and its queued jobs ran.
    pub fn spawn(name: &str) -> std::io::Result<JobThread> {
        let (jobs, inbox) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                while let Ok(job) = inbox.recv() {
                    job();
                }
            })?;
        Ok(JobThread { jobs })
    }

    /// Queues `job`; false when the thread is gone.
    pub fn submit(&self, job: impl FnOnce() + Send + 'static) -> bool {
        self.jobs.send(Box::new(job)).is_ok()
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
}
