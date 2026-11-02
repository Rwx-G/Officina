//! The workshop's test doubles (D-223): the fake engine and the logging
//! git, each a link to the built binary in a folder of its own beside it,
//! with its sidecar, state and log.

use std::path::{Path, PathBuf};

use std::sync::Arc;

use officina_host::relay::credential::{CredentialStore, Secret, StoreError};
use officina_host::workshop::engine::{BinarySource, ResolvedDocker};
use officina_host::workshop::launch::{UncheckedStates, WorkshopRuntime};
use officina_host::workshop::proxy::WinHttpReader;
use officina_host::workshop::threads::{JobThread, WORKSHOP_THREAD};

/// A credential store holding one seeded value under every name, or none
/// (D-86's pattern): never the system's.
#[derive(Debug, Clone, Default)]
pub struct SeededStore(pub Option<String>);

impl CredentialStore for SeededStore {
    fn read(&self, _name: &str) -> Result<Option<Secret>, StoreError> {
        Ok(self.0.clone().map(Secret::new))
    }
}

/// The runtime of a window whose engine and `git` are absent: a host
/// card's tests never reach it.
pub fn host_runtime() -> WorkshopRuntime {
    runtime(
        None,
        None,
        SeededStore(None),
        std::path::PathBuf::new(),
        None,
    )
}

/// A workshop runtime on its own thread `workshop`, with these sources.
pub fn runtime(
    docker: Option<ResolvedDocker>,
    git: Option<PathBuf>,
    store: SeededStore,
    exe_dir: PathBuf,
    linux_override: Option<std::ffi::OsString>,
) -> WorkshopRuntime {
    WorkshopRuntime {
        thread: JobThread::spawn(WORKSHOP_THREAD).unwrap(),
        docker,
        git,
        exe_dir,
        linux_override,
        store: Arc::new(store),
        states: Arc::new(UncheckedStates),
        reader: Arc::new(WinHttpReader),
        followed: Arc::default(),
        now_ms: super::now_ms,
    }
}

pub const FAKE_DOCKER: &str = env!("CARGO_BIN_EXE_officina-fake-docker");
pub const GIT_LOGGER: &str = env!("CARGO_BIN_EXE_officina-git-logger");

/// A folder beside `binary` holding a link to it. Beside the built binary,
/// so the hard link works: a copy written by this process stays open for
/// writing in children other test threads fork, and Linux then refuses to
/// run it ("Text file busy", D-884).
fn linked(binary: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir_in(Path::new(binary).parent().unwrap()).unwrap();
    let exe = dir.path().join(Path::new(binary).file_name().unwrap());
    if std::fs::hard_link(binary, &exe).is_err() {
        std::fs::copy(binary, &exe).unwrap();
    }
    (dir, exe)
}

fn json_lines(path: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
        .collect()
}

/// The fake engine, configured by `sidecar`, its state `state`.
pub struct FakeEngine {
    pub dir: tempfile::TempDir,
    pub exe: PathBuf,
}

impl FakeEngine {
    pub fn new(sidecar: &serde_json::Value, state: &serde_json::Value) -> FakeEngine {
        let (dir, exe) = linked(FAKE_DOCKER);
        std::fs::write(dir.path().join("fake-docker.json"), sidecar.to_string()).unwrap();
        std::fs::write(dir.path().join("fake-docker-state.json"), state.to_string()).unwrap();
        FakeEngine { dir, exe }
    }

    pub fn docker(&self) -> ResolvedDocker {
        ResolvedDocker::at(self.exe.clone(), BinarySource::Override)
    }

    /// Every call so far, `{"argv": [...], "env_names": [...]}` each.
    pub fn log(&self) -> Vec<serde_json::Value> {
        json_lines(&self.dir.path().join("fake-docker.log"))
    }
}

/// The logging git, running `git` after logging each argument vector.
pub struct GitLogger {
    pub dir: tempfile::TempDir,
    pub exe: PathBuf,
}

impl GitLogger {
    pub fn new(git: &Path) -> GitLogger {
        let (dir, exe) = linked(GIT_LOGGER);
        std::fs::write(
            dir.path().join("git-logger.json"),
            serde_json::json!({ "git": git }).to_string(),
        )
        .unwrap();
        GitLogger { dir, exe }
    }

    /// Every call so far, `{"argv": [...]}` each.
    pub fn log(&self) -> Vec<serde_json::Value> {
        json_lines(&self.dir.path().join("git-logger.log"))
    }
}

/// What a passing battery prints, derived from core (D-223): each phase's
/// controls `ok` and attempts `refused` (or the outcome `outcomes` names),
/// each gateway's refusals in its phase's order, and the decoy's
/// addresses and `listening` line; `edits` is merged over it.
pub fn battery_sidecar(
    kind: officina_core::workshop::floor::CredentialKind,
    edits: &serde_json::Value,
) -> serde_json::Value {
    use officina_core::workshop::battery::{
        AttemptLine, AttemptOutcome, BatteryFamily, BatteryLine, Control, ControlLine,
        ControlStatus, DECOY_PORT, DUMMY_CREDENTIAL, DecoyLine,
    };
    use officina_core::workshop::gateway::{GatewayLine, GatewayRefusal};
    let mut lines = serde_json::Map::new();
    let mut gateways = serde_json::Map::new();
    for (phase, key) in [(1u8, "a"), (2, "b")] {
        let mut text = String::new();
        let mut refusals = String::new();
        for family in BatteryFamily::ALL {
            let attempts: Vec<_> = officina_core::workshop::battery::attempts_for(kind, phase)
                .filter(|a| a.family == *family)
                .collect();
            if attempts.is_empty() {
                continue;
            }
            if let Some(control) = family.control().filter(|c| *c != Control::DecoyListening) {
                let status = if edits["failed_control"] == family.code() {
                    ControlStatus::Failed
                } else {
                    ControlStatus::Ok
                };
                let line = BatteryLine::Control(ControlLine {
                    family: *family,
                    control,
                    status,
                });
                text.push_str(&format!("{}\n", line.to_json()));
            }
            for attempt in attempts {
                let outcome = edits["outcomes"][attempt.code]
                    .as_str()
                    .and_then(AttemptOutcome::from_code)
                    .unwrap_or(AttemptOutcome::Refused);
                let line = BatteryLine::Attempt(AttemptLine {
                    family: attempt.family,
                    attempt: attempt.code.to_owned(),
                    expected: attempt.expected,
                    outcome,
                    detail: None,
                });
                text.push_str(&format!("{}\n", line.to_json()));
                if let Some(cause) = attempt.expected {
                    let gateway = if cause == GatewayRefusal::Limit {
                        GatewayLine::Limit {}
                    } else {
                        GatewayLine::refused("officina-off-list.test", 443, cause, None)
                    };
                    refusals.push_str(&format!("{}\n", gateway.to_line()));
                }
            }
        }
        lines.insert(phase.to_string(), text.into());
        gateways.insert(key.to_owned(), refusals.into());
    }
    let mut decoy = format!(
        "{}\n",
        DecoyLine::Addresses(officina_core::workshop::battery::DecoyAddresses {
            own: vec!["172.18.0.2".to_owned()],
            ..officina_core::workshop::battery::DecoyAddresses::default()
        })
        .to_json()
    );
    if edits["decoy_silent"] != true {
        decoy.push_str(&format!(
            "{}\n",
            DecoyLine::Listening { port: DECOY_PORT }.to_json()
        ));
    }
    let mut out = serde_json::json!({
        "battery_lines": lines,
        "gateway_logs": gateways,
        "decoy_logs": decoy,
        "dummy_credential": DUMMY_CREDENTIAL,
    });
    for (key, value) in edits.as_object().into_iter().flatten() {
        out[key] = value.clone();
    }
    out
}
