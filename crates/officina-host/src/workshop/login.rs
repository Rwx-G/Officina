//! The login kind on the host (D-605, D-608, D-609, story 6.26): the login
//! volume and the command the author runs himself, the auth probe, and the
//! scanner of a login run. Officina never reads, copies out or displays
//! anything in the login volume: the probe and the scanner are containers
//! of the base image that print one answer each, never a value.

use std::collections::BTreeMap;
use std::path::PathBuf;

use officina_core::workshop::credential::{
    AuthStatusLine, CheckLoginAnswer, CredentialSignal, LoginCheck, LoginRefusal,
    PrepareLoginAnswer,
};
use officina_core::workshop::engine::{EngineCall, parse_image_inspect, parse_object_rows};
use officina_core::workshop::floor::CredentialKind;
use officina_core::workshop::image::WORKSHOP_BASE;
use officina_core::workshop::launch::{
    login_command, login_labels, login_volume_listed, render_one_line,
};
use officina_core::workshop::names::{ObjectName, ROLE_LABEL, base_labels, login_volume};
use officina_core::workshop::scan::{ScannerAnswer, scanner_answer};
use officina_core::workshop::vector::{self, ContainerRole};

use crate::workshop::engine::{EngineOutcome, ResolvedDocker, execute};
use crate::workshop::view::{EngineReader, login_target};

/// The engine and the floor the login's containers run with.
#[derive(Debug, Clone)]
pub struct LoginEngine {
    pub docker: ResolvedDocker,
    pub base_env: Vec<(String, String)>,
    pub home: PathBuf,
    pub floor_key: String,
}

/// Why a login step did not run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoginFailure {
    /// Refused while the workshop's stop flag is set (AD-116).
    #[error("workshop_stopped")]
    Stopped,
    /// The base image is not built: the probe and the scanner run it.
    #[error("the base image {WORKSHOP_BASE} is not built")]
    NoBaseImage,
    #[error("docker: {0}")]
    Engine(String),
}

/// What the auth probe answered (D-608, D-1135): a line that does not
/// read is a failed probe with detail `unreadable`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeAnswer {
    pub logged_in: bool,
    pub auth_method: Option<String>,
    pub unreadable: bool,
}

impl LoginEngine {
    fn call(
        &self,
        call: &EngineCall,
        stdin: Option<Vec<u8>>,
    ) -> Result<EngineOutcome, LoginFailure> {
        execute(&self.docker, call, &self.base_env, &self.home, stdin)
            .map_err(|e| LoginFailure::Engine(e.to_string()))
    }

    fn refuse_while_stopped(&self) -> Result<(), LoginFailure> {
        if crate::workshop::workshop_stopped(&self.home) {
            Err(LoginFailure::Stopped)
        } else {
            Ok(())
        }
    }

    /// The base image's id, which the probe and the scanner run by id.
    fn base_image_id(&self) -> Result<String, LoginFailure> {
        let tag = ObjectName::parse(WORKSHOP_BASE).ok_or(LoginFailure::NoBaseImage)?;
        let out = self.call(&EngineCall::image_inspect(&tag), None)?;
        if !out.succeeded() {
            return Err(LoginFailure::NoBaseImage);
        }
        parse_image_inspect(&String::from_utf8_lossy(&out.stdout))
            .map(|facts| facts.id)
            .ok_or(LoginFailure::NoBaseImage)
    }

    /// Whether the floor's login volume exists with its labels.
    pub fn volume_present(&self) -> Result<bool, LoginFailure> {
        let out = self.call(&EngineCall::volume_ls(), None)?;
        if !out.succeeded() {
            return Err(LoginFailure::Engine(out.stderr.trim().to_owned()));
        }
        Ok(login_volume_listed(
            &parse_object_rows(&String::from_utf8_lossy(&out.stdout)),
            &self.floor_key,
        ))
    }

    /// Log in for the workshop (D-605): creates the floor's login volume
    /// with its labels when it is absent, and returns the command the
    /// author runs himself, on one line, routed as `routes` say for the
    /// login's host. Refused while the workshop is stopped.
    pub fn prepare(&self, routes: &BTreeMap<String, String>) -> Result<String, LoginFailure> {
        self.refuse_while_stopped()?;
        if !self.volume_present()? {
            let created = self.call(
                &EngineCall::volume_create(
                    &login_volume(&self.floor_key),
                    &login_labels(&self.floor_key),
                ),
                None,
            )?;
            if !created.succeeded() {
                return Err(LoginFailure::Engine(created.stderr.trim().to_owned()));
            }
        }
        Ok(render_one_line(&login_command(
            &self.floor_key,
            WORKSHOP_BASE,
            routes,
        )))
    }

    /// Runs `role`'s container on the login, attached, removed before (a
    /// crash's leftover) and after; its stdout.
    fn run_reader(
        &self,
        role: ContainerRole,
        stdin: Option<Vec<u8>>,
    ) -> Result<String, LoginFailure> {
        self.refuse_while_stopped()?;
        let image_id = self.base_image_id()?;
        let name = ObjectName::parse(&format!(
            "officina-{}-{}",
            role.code().replace('_', "-"),
            self.floor_key
        ))
        .ok_or_else(|| LoginFailure::Engine("the container's name".to_owned()))?;
        let mut labels = base_labels(&self.floor_key, crate::workshop::build::VERSION);
        labels.insert(ROLE_LABEL.to_owned(), role.code().to_owned());
        let container = match role {
            ContainerRole::Scanner => vector::scanner(&name, &labels, &image_id, &self.floor_key),
            _ => vector::auth_probe(&name, &labels, &image_id, &self.floor_key),
        };
        let remove = || {
            let _ = self.call(&EngineCall::rm(std::slice::from_ref(&name)), None);
        };
        remove();
        let created = self.call(&EngineCall::create(&container), None)?;
        if !created.succeeded() {
            remove();
            return Err(LoginFailure::Engine(format!(
                "create: {}",
                created.stderr.trim()
            )));
        }
        let with_stdin = stdin.is_some();
        let started = self.call(
            &EngineCall::start_attached(&name, role, 0, with_stdin),
            stdin,
        );
        remove();
        let started = started?;
        if !started.succeeded() || started.stdout_over {
            return Err(LoginFailure::Engine(format!(
                "start: exit {:?}: {}",
                started.code,
                started.stderr.trim()
            )));
        }
        Ok(String::from_utf8_lossy(&started.stdout).into_owned())
    }

    /// The auth probe (D-608): `officina-entry auth-status` on the login,
    /// no network, 30 s; only `logged_in` and `auth_method` are read, and
    /// a line that does not read fails closed.
    pub fn probe(&self) -> Result<ProbeAnswer, LoginFailure> {
        let output = self.run_reader(ContainerRole::AuthProbe, None)?;
        Ok(match AuthStatusLine::parse(&output) {
            Some(line) => ProbeAnswer {
                logged_in: line.logged_in,
                auth_method: line.auth_method,
                unreadable: false,
            },
            None => ProbeAnswer {
                logged_in: false,
                auth_method: None,
                unreadable: true,
            },
        })
    }

    /// The scan of a login run (D-609): `records` framed on the scanner's
    /// stdin (`<index> <length>` then the bytes), its answer read back;
    /// `None` when it could not scan, so nothing scanned is released.
    pub fn scan(&self, records: &[&[u8]]) -> Option<ScannerAnswer> {
        let mut stdin = Vec::new();
        for (index, record) in records.iter().enumerate() {
            stdin.extend_from_slice(format!("{index} {}\n", record.len()).as_bytes());
            stdin.extend_from_slice(record);
        }
        let output = self.run_reader(ContainerRole::Scanner, Some(stdin)).ok()?;
        scanner_answer(&output, records.len())
    }
}

/// A login run's [`officina_core::workshop::scan::ScanSource`]: the
/// scanner, and whether the stored login gave it no needle (the run's
/// facts say so, D-609).
pub struct ScannerSource<'e> {
    pub engine: &'e LoginEngine,
    pub no_needles: bool,
}

impl officina_core::workshop::scan::ScanSource for ScannerSource<'_> {
    fn ranges(&mut self, records: &[&[u8]]) -> Option<Vec<Vec<std::ops::Range<usize>>>> {
        match self.engine.scan(records)? {
            ScannerAnswer::NoNeedles => {
                self.no_needles = true;
                Some(vec![Vec::new(); records.len()])
            }
            ScannerAnswer::Ranges(ranges) => Some(ranges),
        }
    }
}

/// Runs `job` on thread `workshop` and waits for its answer; `None` when
/// the thread is gone.
fn on_thread<T: Send + 'static>(
    thread: &crate::workshop::threads::JobThread,
    job: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (sender, receiver) = std::sync::mpsc::channel();
    if !thread.submit(move || {
        let _ = sender.send(job());
    }) {
        return None;
    }
    receiver.recv().ok()
}

/// The floor's login engine, or why the login steps cannot run.
fn login_engine(
    reader: &EngineReader,
) -> Result<(LoginEngine, BTreeMap<String, String>), LoginRefusal> {
    let (floor_key, kind, routes) = login_target(reader).ok_or(LoginRefusal::NoFloor)?;
    if kind != CredentialKind::Login {
        return Err(LoginRefusal::NotLoginFloor);
    }
    let docker = reader
        .docker
        .as_ref()
        .map_err(|_| LoginRefusal::EngineUnavailable)?
        .clone();
    let engine = LoginEngine {
        docker,
        base_env: reader.env.clone(),
        home: reader.relay.officina_home().to_path_buf(),
        floor_key,
    };
    if engine.refuse_while_stopped().is_err() {
        return Err(LoginRefusal::WorkshopStopped);
    }
    Ok((engine, routes))
}

fn refusal_of(failure: &LoginFailure) -> LoginRefusal {
    match failure {
        LoginFailure::Stopped => LoginRefusal::WorkshopStopped,
        LoginFailure::NoBaseImage => LoginRefusal::BaseImageMissing,
        LoginFailure::Engine(_) => LoginRefusal::EngineUnavailable,
    }
}

/// Log in for the workshop (D-605, pointer only): on thread `workshop`,
/// refused while stopped; creates the login volume when absent and
/// answers the command the author runs himself.
pub fn prepare_login(
    thread: &crate::workshop::threads::JobThread,
    reader: &EngineReader,
) -> PrepareLoginAnswer {
    let reader = reader.clone();
    on_thread(thread, move || match login_engine(&reader) {
        Ok((engine, routes)) => match engine.prepare(&routes) {
            Ok(command) => PrepareLoginAnswer::Ready(command),
            Err(failure) => PrepareLoginAnswer::Refused(refusal_of(&failure)),
        },
        Err(refusal) => PrepareLoginAnswer::Refused(refusal),
    })
    .unwrap_or(PrepareLoginAnswer::Refused(LoginRefusal::EngineUnavailable))
}

/// The auth probe on thread `workshop` (D-608, D-1135): from Check login
/// (`by_click`, pointer only) or once at the window's start; refused while
/// stopped. Its answer moves the floor's state (D-1240).
pub fn check_login(
    thread: &crate::workshop::threads::JobThread,
    reader: &EngineReader,
    by_click: bool,
) -> CheckLoginAnswer {
    let reader = reader.clone();
    on_thread(thread, move || probe_now(&reader, by_click))
        .unwrap_or(CheckLoginAnswer::Refused(LoginRefusal::EngineUnavailable))
}

/// Runs the probe on the calling thread and moves the state on its answer.
fn probe_now(reader: &EngineReader, by_click: bool) -> CheckLoginAnswer {
    let (engine, _) = match login_engine(reader) {
        Ok(found) => found,
        Err(refusal) => return CheckLoginAnswer::Refused(refusal),
    };
    match engine.probe() {
        Ok(answer) => {
            let state = reader.states.signal(
                &engine.floor_key,
                CredentialKind::Login,
                CredentialSignal::Probe {
                    logged_in: answer.logged_in,
                    by_click,
                },
            );
            CheckLoginAnswer::Checked(LoginCheck {
                logged_in: answer.logged_in,
                auth_method: answer.auth_method,
                unreadable: answer.unreadable,
                state,
            })
        }
        Err(failure) => CheckLoginAnswer::Refused(refusal_of(&failure)),
    }
}

/// The window's one probe at start for a `login` floor (D-608): queued on
/// thread `workshop` without waiting; a token floor, a stopped workshop or
/// an engine that does not answer runs nothing.
pub fn probe_at_start(thread: &crate::workshop::threads::JobThread, reader: &EngineReader) {
    let reader = reader.clone();
    thread.submit(move || {
        let _ = probe_now(&reader, false);
    });
}
