//! The escape battery end to end with the fake engine (story 6.10 AC 3 and
//! 5, AD-117, D-205, D-206, D-215, D-235, D-603): the decoy and its probe,
//! both phases' gateways and battery workers created from core's vectors,
//! labelled and removed afterwards; the judgement; and, through the
//! window's collector, one `workshop_battery` row and its journal lines.

mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use officina_core::relay::journal::JournalEntry;
use officina_core::workshop::battery::{
    BatteryAnswer, BatteryOutcome, BatterySpec, DecoyAddresses, attempts_for,
};
use officina_core::workshop::engine::EngineCall;
use officina_core::workshop::floor::CredentialKind;
use officina_core::workshop::image::{BuildAnswer, WORKSHOP_BASE};
use officina_core::workshop::names::{BatteryNames, RunId, login_volume};
use officina_core::workshop::vector::{LOGIN_DIR, credential_name};
use officina_host::collector::{Collector, LoadedFloor};
use officina_host::paths::Paths;
use officina_host::spawn::BaseEnv;
use officina_host::watch::{AUTHOR_WAIT, AuthorCommand, WatchHandle, ask, spawn};
use officina_host::workshop::battery::{
    BatteryContext, BatteryEngine, BatteryJob, BatteryReport, run, run_battery,
};
use officina_host::workshop::build::{BuildContext, build_images};
use officina_host::workshop::proxy::WinHttpReader;
use officina_host::workshop::threads::{BUILD_THREAD, JobThread};

use common::TestEnv;
use common::workshop::FakeEngine;

const BATTERY_ID: &str = "fedcba9876543210fedcba9876543210";
const FLOOR_KEY: &str = "0123456789abcdef";
/// What the window's environment holds under the credential's name: never
/// what a battery passes (D-206).
const WINDOW_TOKEN: &str = "a-value-from-the-window-environment";

fn base_env() -> Vec<(String, String)> {
    vec![
        (
            "SYSTEMROOT".to_owned(),
            std::env::var("SYSTEMROOT").unwrap_or_default(),
        ),
        (
            "CLAUDE_CODE_OAUTH_TOKEN".to_owned(),
            WINDOW_TOKEN.to_owned(),
        ),
    ]
}

/// What a passing battery prints, derived from core (D-223): each phase's
/// controls `ok` and attempts `refused` (or the outcome `outcomes` names),
/// each gateway's refusals in its phase's order, and the decoy's
/// addresses and `listening` line; `edits` is merged over it.
fn sidecar(kind: CredentialKind, edits: &serde_json::Value) -> serde_json::Value {
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
            let attempts: Vec<_> = attempts_for(kind, phase)
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
        DecoyLine::Addresses(DecoyAddresses {
            own: vec!["172.18.0.2".to_owned()],
            ..DecoyAddresses::default()
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

/// A battery run straight on the fake engine, outside the window.
struct Bench {
    home: tempfile::TempDir,
    engine: FakeEngine,
    kind: CredentialKind,
}

impl Bench {
    fn new(edits: &serde_json::Value) -> Bench {
        Bench::of(CredentialKind::OauthToken, edits)
    }

    fn of(kind: CredentialKind, edits: &serde_json::Value) -> Bench {
        Bench {
            home: tempfile::tempdir().unwrap(),
            engine: FakeEngine::new(&sidecar(kind, edits), &serde_json::json!({})),
            kind,
        }
    }

    fn job(kind: CredentialKind) -> BatteryJob {
        BatteryJob {
            id: RunId::parse(BATTERY_ID).unwrap(),
            image: WORKSHOP_BASE.to_owned(),
            spec: BatterySpec {
                kind,
                cpus: 2,
                memory_mib: 4096,
                floor_key: FLOOR_KEY.to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
                image_id: format!("sha256:{}", "a".repeat(64)),
                image_env: vec!["PATH".to_owned()],
            },
            programs_sha256: "p".repeat(64),
            gateway_image_id: format!("sha256:{}", "b".repeat(64)),
        }
    }

    fn run(&self) -> BatteryReport {
        let engine = BatteryEngine {
            docker: self.engine.docker(),
            base_env: base_env(),
            home: self.home.path().to_path_buf(),
        };
        run(
            &engine,
            &Bench::job(self.kind),
            &officina_host::files::now_ms,
        )
    }

    fn calls(&self) -> Vec<(Vec<String>, Vec<String>)> {
        let strings = |v: &serde_json::Value| -> Vec<String> {
            v.as_array()
                .unwrap()
                .iter()
                .map(|a| a.as_str().unwrap().to_owned())
                .collect()
        };
        self.engine
            .log()
            .iter()
            .map(|line| (strings(&line["argv"]), strings(&line["env_names"])))
            .collect()
    }

    fn creates_of(&self, role: &str) -> Vec<(Vec<String>, Vec<String>)> {
        let label = format!("officina.role={role}");
        self.calls()
            .into_iter()
            .filter(|(argv, _)| argv[0] == "create" && argv.contains(&label))
            .collect()
    }

    fn state(&self) -> serde_json::Value {
        serde_json::from_slice(
            &std::fs::read(self.engine.dir.path().join("fake-docker-state.json")).unwrap(),
        )
        .unwrap()
    }
}

#[test]
fn a_battery_creates_and_removes_its_containers() {
    let bench = Bench::new(&serde_json::json!({}));
    let report = bench.run();
    assert_eq!(
        report.record.outcome,
        Some(BatteryOutcome::Passed),
        "{:?}",
        report.record.cause
    );
    for (role, count) in [("decoy", 2), ("battery_gateway", 2), ("battery", 2)] {
        assert_eq!(bench.creates_of(role).len(), count, "{role}");
    }
    for (argv, _) in bench.calls().iter().filter(|(a, _)| a[0] == "create") {
        assert!(argv.contains(&"officina.managed=1".to_owned()), "{argv:?}");
        assert!(
            argv.contains(&format!("officina.run={BATTERY_ID}")),
            "{argv:?}"
        );
    }
    let state = bench.state();
    for key in ["containers", "networks", "volumes"] {
        assert_eq!(
            state[key].as_array().map_or(0, Vec::len),
            0,
            "{key} left: {}",
            state[key]
        );
    }
    let names = BatteryNames::of(&RunId::parse(BATTERY_ID).unwrap());
    let internal = bench
        .calls()
        .into_iter()
        .find(|(argv, _)| {
            argv.starts_with(&["network".to_owned(), "create".to_owned()])
                && argv.last() == Some(&names.run.internal.as_str().to_owned())
        })
        .expect("the internal network");
    assert!(internal.0.contains(&"--internal".to_owned()));
}

#[test]
fn the_battery_worker_vector_is_the_worker_vector_normalised() {
    let bench = Bench::new(&serde_json::json!({}));
    bench.run();
    let job = Bench::job(CredentialKind::OauthToken);
    let mount = officina_host::workshop::battery_dir(bench.home.path()).join("mount");
    let decoy = DecoyAddresses {
        own: vec!["172.18.0.2".to_owned()],
        ..DecoyAddresses::default()
    };
    let creates = bench.creates_of("battery");
    for (phase, (argv, _)) in [1u8, 2].into_iter().zip(&creates) {
        let planned = job
            .spec
            .worker(&job.id, phase, &mount.to_string_lossy(), &decoy);
        assert_eq!(
            argv.as_slice(),
            EngineCall::create(&planned.vector).args(),
            "phase {phase}: the host runs core's vector untouched"
        );
        assert_eq!(
            officina_core::workshop::vector::normalised(&planned.vector),
            officina_core::workshop::vector::normalised(&creates_vector(&job, 1, &mount, &decoy)),
            "both phases normalise to one vector"
        );
    }
}

fn creates_vector(
    job: &BatteryJob,
    phase: u8,
    mount: &std::path::Path,
    decoy: &DecoyAddresses,
) -> officina_core::workshop::vector::ContainerVector {
    job.spec
        .worker(&job.id, phase, &mount.to_string_lossy(), decoy)
        .vector
}

#[test]
fn the_dummy_credential_is_the_only_credential() {
    let bench = Bench::new(&serde_json::json!({}));
    bench.run();
    let name = credential_name(CredentialKind::OauthToken)
        .unwrap()
        .to_owned();
    for (argv, env_names) in bench.calls() {
        let battery_create =
            argv[0] == "create" && argv.contains(&"officina.role=battery".to_owned());
        assert_eq!(
            env_names.contains(&name),
            battery_create,
            "{argv:?} carries the credential's name"
        );
    }
    let seen = bench.state()["credentials"].as_array().unwrap().clone();
    assert_eq!(seen.len(), 2, "one per battery worker");
    for credential in seen {
        assert_eq!(credential["dummy"], true, "{credential}");
    }
}

#[test]
fn a_login_floor_battery_mounts_its_own_empty_volume_never_the_login() {
    let bench = Bench::of(CredentialKind::Login, &serde_json::json!({}));
    let report = bench.run();
    assert_eq!(report.record.outcome, Some(BatteryOutcome::Passed));
    let names = BatteryNames::of(&RunId::parse(BATTERY_ID).unwrap());
    let own = format!(
        "type=volume,source={},target={LOGIN_DIR},readonly",
        names.login_volume.as_str()
    );
    let real = login_volume(FLOOR_KEY);
    for (argv, env_names) in bench.creates_of("battery") {
        assert!(argv.contains(&own), "{argv:?}");
        assert!(
            !argv.iter().any(|a| a.contains(real.as_str())),
            "the floor's login is never mounted: {argv:?}"
        );
        assert!(
            !env_names
                .iter()
                .any(|n| n == "CLAUDE_CODE_OAUTH_TOKEN" || n == "ANTHROPIC_API_KEY")
        );
    }
    let volume_creates: Vec<Vec<String>> = bench
        .calls()
        .into_iter()
        .map(|(argv, _)| argv)
        .filter(|argv| argv.starts_with(&["volume".to_owned(), "create".to_owned()]))
        .collect();
    assert!(
        volume_creates
            .iter()
            .any(|argv| argv.last() == Some(&names.login_volume.as_str().to_owned()))
    );
    assert!(
        report
            .attempts
            .iter()
            .any(|a| a.attempt == "write_login_mount"),
        "the files family writes in the login mount"
    );
}

#[test]
fn the_decoy_control_needs_a_second_container() {
    let bench = Bench::new(&serde_json::json!({ "decoy_probe_exit": 1 }));
    let report = bench.run();
    assert_eq!(report.record.outcome, Some(BatteryOutcome::Failed));
    let probes: Vec<_> = bench
        .creates_of("decoy")
        .into_iter()
        .filter(|(argv, _)| argv.contains(&"--probe".to_owned()))
        .collect();
    assert_eq!(probes.len(), 1, "the probe is a container of its own");
    // A fact holds or not whatever the decoy did; an attempt judged from a
    // failure needs the control (D-235).
    let from_failure = |code: &str| {
        officina_core::workshop::battery::attempt_named(code)
            .is_some_and(|a| a.judged == officina_core::workshop::battery::Judged::Failure)
    };
    let judged: Vec<_> = report
        .attempts
        .iter()
        .filter(|a| a.family.code() == "host" && from_failure(&a.attempt))
        .collect();
    assert!(!judged.is_empty());
    for verdict in judged {
        assert_ne!(
            verdict.outcome.code(),
            "refused",
            "{} passed without the decoy's control",
            verdict.attempt
        );
    }
    let silent = Bench::new(&serde_json::json!({ "decoy_silent": true }));
    let report = silent.run();
    assert_eq!(report.record.outcome, Some(BatteryOutcome::Failed));
}

#[test]
fn nothing_starts_while_stopped() {
    let bench = Bench::new(&serde_json::json!({}));
    let stopped = officina_host::workshop::stopped_file(bench.home.path());
    std::fs::create_dir_all(stopped.parent().unwrap()).unwrap();
    std::fs::write(&stopped, "").unwrap();
    let report = bench.run();
    assert_eq!(report.record.outcome, Some(BatteryOutcome::Stopped));
    assert!(
        bench.calls().is_empty(),
        "no engine call: {:?}",
        bench.calls()
    );
}

#[test]
fn an_escape_or_a_wrong_gateway_exit_fails() {
    let escaped = Bench::new(&serde_json::json!({ "outcomes": { "tcp_public": "escaped" } }));
    let report = escaped.run();
    assert_eq!(report.record.outcome, Some(BatteryOutcome::Failed));
    assert_eq!(report.record.totals.escaped, 1);
    let open = Bench::new(&serde_json::json!({ "gateway_b_exit": 0 }));
    let report = open.run();
    assert_eq!(report.record.outcome, Some(BatteryOutcome::Failed));
    assert!(report.record.cause.unwrap().contains("gateway B"));
}

/// The window's collector over a floor whose images Build images built on
/// the fake engine.
struct WindowHome {
    env: TestEnv,
    handle: Option<WatchHandle>,
    engine: FakeEngine,
    exe_dir: PathBuf,
    thread: JobThread,
    busy: Arc<AtomicBool>,
}

impl WindowHome {
    fn start() -> WindowHome {
        let env = TestEnv::new();
        env.write_floor(
            "schema = 2\n[workshop]\nauth = \"oauth-token\"\nupstream_proxy = \"none\"\n",
        );
        env.approve();
        env.write_agents("[]");
        let claude_sidecar = serde_json::json!({
            "version": "2.1.316 (Claude Code)",
            "fixture": env.fixture().to_string_lossy(),
            "log": env.fake_log().to_string_lossy(),
        });
        let claude = common::fake_claude_in(&env.root.path().join("bin"), &claude_sidecar);
        let paths = Paths {
            officina_home: env.officina_home(),
            claude_config_dir: env.claude_config(),
            claude_config_override: None,
            floor_file: None,
        };
        let collector = Collector::from_parts(paths, LoadedFloor::default(), Some(claude))
            .with_base_env(BaseEnv::new(base_env()));
        let (board_sink, emitted) = common::sink(|_| {});
        let handle = spawn(collector, board_sink).unwrap();
        emitted
            .boards
            .recv_timeout(Duration::from_secs(10))
            .expect("first board");
        let exe_dir = env.root.path().join("exe");
        let linux = exe_dir.join("linux");
        std::fs::create_dir_all(&linux).unwrap();
        std::fs::write(linux.join("officina-entry"), b"entry").unwrap();
        std::fs::write(linux.join("officina-gateway"), b"gateway").unwrap();
        WindowHome {
            env,
            handle: Some(handle),
            engine: FakeEngine::new(
                &sidecar(CredentialKind::OauthToken, &serde_json::json!({})),
                &serde_json::json!({}),
            ),
            exe_dir,
            thread: JobThread::spawn(BUILD_THREAD).unwrap(),
            busy: Arc::new(AtomicBool::new(false)),
        }
    }

    fn sender(&self) -> &std::sync::mpsc::Sender<officina_host::watch::CollectorMsg> {
        &self.handle.as_ref().unwrap().sender
    }

    fn settle(&self) {
        ask(self.sender(), AuthorCommand::ListDiscarded, AUTHOR_WAIT).expect("an answer");
    }

    fn build(&self) {
        for _ in 0..100 {
            let (done, finished) = std::sync::mpsc::channel();
            let answer = build_images(
                self.sender(),
                &self.thread,
                BuildContext {
                    exe_dir: Some(self.exe_dir.clone()),
                    linux_override: None,
                    docker: Ok(self.engine.docker()),
                    reader: Arc::new(WinHttpReader),
                    busy: Arc::clone(&self.busy),
                    now_ms: officina_host::files::now_ms,
                },
                Box::new(move || {
                    let _ = done.send(());
                }),
            )
            .expect("an answer");
            if answer == BuildAnswer::Queued {
                finished.recv_timeout(Duration::from_secs(60)).unwrap();
                self.settle();
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("Build images never ran");
    }

    fn battery(&self, only: Option<&str>) -> BatteryAnswer {
        let (done, finished) = std::sync::mpsc::channel();
        let answer = run_battery(
            self.sender(),
            &self.thread,
            BatteryContext {
                docker: Ok(self.engine.docker()),
                busy: Arc::clone(&self.busy),
                now_ms: officina_host::files::now_ms,
            },
            only.map(str::to_owned),
            Box::new(move || {
                let _ = done.send(());
            }),
        )
        .expect("an answer");
        if matches!(answer, BatteryAnswer::Queued(_)) {
            finished.recv_timeout(Duration::from_secs(60)).unwrap();
        }
        self.settle();
        answer
    }
}

impl Drop for WindowHome {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle
                .sender
                .send(officina_host::watch::CollectorMsg::Shutdown);
            let _ = handle.thread.join();
        }
    }
}

#[test]
fn one_row_and_its_journal_lines_per_battery() {
    let home = WindowHome::start();
    home.build();
    let answer = home.battery(None);
    assert_eq!(
        answer,
        BatteryAnswer::Queued(vec![WORKSHOP_BASE.to_owned()])
    );
    let reader = officina_host::registry::store::open_reader(&home.env.officina_home(), &[])
        .expect("a registry");
    let rows = reader.workshop_batteries(WORKSHOP_BASE).expect("a read");
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.outcome, Some(BatteryOutcome::Passed), "{:?}", row.cause);
    assert_eq!(row.id.len(), 32);
    assert_eq!(row.key.officina_version, env!("CARGO_PKG_VERSION"));
    assert!(row.clock_skew_ms.is_some(), "the skew is measured");
    let journal = home.env.journal();
    let count = |pick: &dyn Fn(&JournalEntry) -> bool| {
        journal.iter().filter(|line| pick(&line.entry)).count()
    };
    let attempts = (1..=2)
        .map(|phase| attempts_for(CredentialKind::OauthToken, phase).count())
        .sum::<usize>();
    assert_eq!(
        count(&|e| matches!(e, JournalEntry::EscapeAttempt(a) if a.battery_id == row.id)),
        attempts
    );
    assert_eq!(
        count(&|e| matches!(e, JournalEntry::EscapeBattery(b) if b.battery_id == row.id)),
        1
    );
    let expected_refusals = (1..=2)
        .flat_map(|phase| attempts_for(CredentialKind::OauthToken, phase))
        .filter(|a| a.expected.is_some_and(|c| c.code() != "limit"))
        .count();
    assert_eq!(
        count(
            &|e| matches!(e, JournalEntry::GatewayRefused(r) if r.battery_id.as_deref() == Some(row.id.as_str()))
        ),
        expected_refusals
    );
    assert_eq!(
        home.battery(None),
        BatteryAnswer::Refused(officina_core::workshop::battery::BatteryRefusal::NothingStale),
        "a valid battery is not run again"
    );
    assert_eq!(
        home.battery(Some(WORKSHOP_BASE)),
        BatteryAnswer::Queued(vec![WORKSHOP_BASE.to_owned()]),
        "one image on demand"
    );
}
