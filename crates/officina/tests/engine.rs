//! The engine driver against the fake engine (story 6.3, D-223, D-616):
//! the environment `docker.exe` gets, deadlines, caps, and the fake's own
//! contract (log format, modes, state, the formatted `inspect`).

use std::path::{Path, PathBuf};

use officina_core::spawn::{CONTAINED_ENV_SYSTEM, NO_CURRENT_DIR_VAR, SESSION_ENV_REMOVED};
use officina_core::workshop::engine::{
    EngineCall, EngineCallKind, EngineInfo, EngineUnavailable, STDOUT_CAP, parse_inspect,
    parse_object_rows, parse_ps,
};
use officina_core::workshop::names::ObjectName;
use officina_host::workshop::engine::{BinarySource, EngineError, ResolvedDocker, execute};

const FAKE_DOCKER: &str = env!("CARGO_BIN_EXE_officina-fake-docker");
const GIT_LOGGER: &str = env!("CARGO_BIN_EXE_officina-git-logger");

/// The seeded credential value a container's environment holds in the fake
/// engine's state; no formatted call may print it.
const SEEDED: &str = "seeded-credential-value-7Q";

struct FakeEngine {
    dir: tempfile::TempDir,
    exe: PathBuf,
}

impl FakeEngine {
    fn new(sidecar: serde_json::Value) -> FakeEngine {
        let dir = tempfile::tempdir().unwrap();
        let name = Path::new(FAKE_DOCKER).file_name().unwrap();
        let exe = dir.path().join(name);
        if std::fs::hard_link(FAKE_DOCKER, &exe).is_err() {
            std::fs::copy(FAKE_DOCKER, &exe).unwrap();
        }
        std::fs::write(dir.path().join("fake-docker.json"), sidecar.to_string()).unwrap();
        let state = serde_json::json!({
            "containers": [{
                "id": "af9c6af2673a",
                "name": "officina-worker-0123456789abcdef0123456789abcdef",
                "state": "exited",
                "exit_code": 0,
                "oom_killed": false,
                "labels": {"officina.managed": "1", "officina.role": "worker"},
                "env": [format!("CLAUDE_CODE_OAUTH_TOKEN={SEEDED}"), "HOME=/work/.officina/home"],
            }],
            "networks": [{"name": "officina-int-0123456789abcdef0123456789abcdef", "labels": {"officina.managed": "1"}}],
            "volumes": [{"name": "officina-vol-0123456789abcdef0123456789abcdef", "labels": {"officina.managed": "1"}}],
        });
        std::fs::write(dir.path().join("fake-docker-state.json"), state.to_string()).unwrap();
        FakeEngine { dir, exe }
    }

    fn docker(&self) -> ResolvedDocker {
        ResolvedDocker::at(self.exe.clone(), BinarySource::Override)
    }

    fn run(
        &self,
        call: &EngineCall,
        base: &[(String, String)],
    ) -> Result<officina_host::workshop::engine::EngineOutcome, EngineError> {
        execute(&self.docker(), call, base, self.dir.path(), None)
    }

    fn log(&self) -> Vec<serde_json::Value> {
        std::fs::read_to_string(self.dir.path().join("fake-docker.log"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
            .collect()
    }
}

fn worker() -> ObjectName {
    ObjectName::parse("officina-worker-0123456789abcdef0123456789abcdef").unwrap()
}

/// One call of every read kind the fake answers, and a launch kind.
fn calls() -> Vec<EngineCall> {
    vec![
        EngineCall::version(),
        EngineCall::info(),
        EngineCall::ps(),
        EngineCall::inspect(&worker()),
        EngineCall::network_ls(),
        EngineCall::volume_ls(),
        EngineCall::stop(&[worker()]),
        EngineCall::start(&worker()),
        EngineCall::rm(&[worker()]),
    ]
}

#[test]
fn seeded_session_and_credential_names_never_reach_docker() {
    let engine = FakeEngine::new(serde_json::json!({}));
    let mut base: Vec<(String, String)> = SESSION_ENV_REMOVED
        .iter()
        .map(|name| ((*name).to_owned(), "x".to_owned()))
        .collect();
    let refused = [
        "OFFICINA_HOME",
        "Claude_Code_OAuth_Token",
        "ANTHROPIC_API_KEY",
        "HTTPS_PROXY",
        "UNRELATED_NAME",
    ];
    base.extend(
        refused
            .iter()
            .map(|name| ((*name).to_owned(), "x".to_owned())),
    );
    for (name, value) in std::env::vars() {
        if CONTAINED_ENV_SYSTEM
            .iter()
            .any(|kept| kept.eq_ignore_ascii_case(&name))
        {
            base.push((name, value));
        }
    }
    base.push(("DOCKER_CONTEXT".to_owned(), "desktop-linux".to_owned()));
    for call in calls() {
        let ran = engine.run(&call, &base).unwrap();
        assert!(ran.succeeded(), "{:?}: {}", call.kind(), ran.stderr);
    }
    let log = engine.log();
    assert_eq!(log.len(), calls().len());
    let present: Vec<&str> = base
        .iter()
        .filter(|(name, _)| {
            CONTAINED_ENV_SYSTEM
                .iter()
                .any(|kept| kept.eq_ignore_ascii_case(name))
        })
        .map(|(name, _)| name.as_str())
        .collect();
    assert!(!present.is_empty(), "the base holds system names");
    for line in log {
        let names: Vec<String> = line["env_names"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap().to_uppercase())
            .collect();
        for name in SESSION_ENV_REMOVED.iter().chain(refused.iter()) {
            assert!(
                !names.contains(&name.to_uppercase()),
                "{name} reached {line}"
            );
        }
        for name in &present {
            assert!(
                names.contains(&name.to_uppercase()),
                "{name} missing from {line}"
            );
        }
        assert!(names.contains(&"DOCKER_CONTEXT".to_owned()));
        assert!(names.contains(&NO_CURRENT_DIR_VAR.to_uppercase()));
    }
}

#[test]
fn a_deadline_kills_the_child() {
    let engine = FakeEngine::new(serde_json::json!({ "sleep_ms": 30_000 }));
    let started = std::time::Instant::now();
    let refused = engine.run(&EngineCall::version(), &[]).unwrap_err();
    assert!(
        matches!(
            refused,
            EngineError::Run(officina_host::process::RunError::Timeout(_))
        ),
        "{refused:?}"
    );
    let deadline = std::time::Duration::from_millis(EngineCallKind::Version.deadline_ms().unwrap());
    assert!(started.elapsed() < deadline + std::time::Duration::from_secs(10));
}

#[test]
fn stdout_is_capped() {
    let over = u64::try_from(STDOUT_CAP).unwrap() + 4096;
    let engine = FakeEngine::new(serde_json::json!({ "stdout_bytes": over }));
    let ran = engine.run(&EngineCall::info(), &[]).unwrap();
    assert!(ran.stdout_over);
    assert_eq!(ran.stdout.len(), STDOUT_CAP);
}

#[test]
fn the_log_is_one_json_object_per_call() {
    let engine = FakeEngine::new(serde_json::json!({}));
    engine.run(&EngineCall::ps(), &[]).unwrap();
    engine.run(&EngineCall::volume_ls(), &[]).unwrap();
    let log = engine.log();
    assert_eq!(log.len(), 2);
    for line in &log {
        let keys: Vec<&String> = line.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["argv", "env_names"]);
        let names: Vec<&str> = line["env_names"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap())
            .collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "names sorted");
        assert!(
            names.iter().all(|n| !n.contains('=')),
            "names only, never a value: {line}"
        );
    }
    assert_eq!(log[0]["argv"][0], "ps");
}

#[test]
fn version_modes_answer_as_recorded() {
    for (mode, expected) in [
        ("linux", Ok(())),
        ("windows", Err(EngineUnavailable::WindowsContainers)),
        ("down", Err(EngineUnavailable::NotRunning)),
    ] {
        let engine = FakeEngine::new(serde_json::json!({ "version": mode }));
        let ran = engine.run(&EngineCall::version(), &[]).unwrap();
        let parsed = EngineInfo::parse(&String::from_utf8_lossy(&ran.stdout)).map(|_| ());
        assert_eq!(parsed, expected, "{mode}");
    }
}

#[test]
fn state_survives_between_calls() {
    let engine = FakeEngine::new(serde_json::json!({}));
    for _ in 0..2 {
        let ps = engine.run(&EngineCall::ps(), &[]).unwrap();
        let rows = parse_ps(&String::from_utf8_lossy(&ps.stdout));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].labels["officina.role"], "worker");
    }
    let networks = engine.run(&EngineCall::network_ls(), &[]).unwrap();
    assert_eq!(
        parse_object_rows(&String::from_utf8_lossy(&networks.stdout)).len(),
        1
    );
}

#[test]
fn formatted_inspect_never_returns_the_seeded_env() {
    let engine = FakeEngine::new(serde_json::json!({}));
    let ran = engine.run(&EngineCall::inspect(&worker()), &[]).unwrap();
    let text = String::from_utf8_lossy(&ran.stdout).into_owned();
    assert!(!text.contains(SEEDED), "{text}");
    let state = parse_inspect(&text).expect("the formatted shape");
    assert_eq!(state.exit_code, Some(0));
    // The control: the same container inspected without the format shows
    // the seeded value, so the fake holds what the format hides.
    let full = std::process::Command::new(&engine.exe)
        .args(["inspect", worker().as_str()])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&full.stdout).contains(SEEDED));
}

/// D-223, review F38: the logging git passes its arguments through and
/// logs them, one JSON line each.
#[test]
fn the_logger_passes_through_and_logs_argv() {
    let Some(git) = officina_host::relay::tree::resolve_git() else {
        eprintln!("skipped: no git installed (D-23)");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join(Path::new(GIT_LOGGER).file_name().unwrap());
    if std::fs::hard_link(GIT_LOGGER, &exe).is_err() {
        std::fs::copy(GIT_LOGGER, &exe).unwrap();
    }
    std::fs::write(
        dir.path().join("git-logger.json"),
        serde_json::json!({ "git": git }).to_string(),
    )
    .unwrap();
    let out = std::process::Command::new(&exe)
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("git version"));
    let log = std::fs::read_to_string(dir.path().join("git-logger.log")).unwrap();
    let line: serde_json::Value = serde_json::from_str(log.trim()).unwrap();
    assert_eq!(line["argv"], serde_json::json!(["--version"]));
}
