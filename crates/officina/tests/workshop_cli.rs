//! `officina workshop --json` (story 6.18 AC 3; AD-121, D-47): the
//! Workshop view from the read-only registry and the fake engine's read
//! half (D-223), as a process. No real engine, no real token: the engine is
//! the fake one or a path that does not exist.

mod common;

use std::path::Path;

use officina_core::workshop::floor::CredentialKind;
use officina_core::workshop::image::{GATEWAY_IMAGE, WORKSHOP_BASE};
use officina_core::workshop::launch::CredentialState;
use officina_core::workshop::run::RunState;
use officina_core::workshop::view::{
    CredentialAction, EngineStatus, ImageState, WORKSHOP_VIEW_SCHEMA, WorkshopView,
};

use common::TestEnv;
use common::package_run::{home, launch};
use common::workshop_home::{Home, Options};

fn workshop_json(env: &TestEnv, docker: &Path, linux: Option<&Path>) -> std::process::Output {
    let mut command = env.officina(&["workshop", "--json"]);
    command.env("OFFICINA_DOCKER_BIN", docker);
    if let Some(linux) = linux {
        command.env(officina_host::workshop::ENV_LINUX_BIN_DIR, linux);
    }
    common::run_with(command, b"")
}

fn view_of(output: &std::process::Output) -> WorkshopView {
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    serde_json::from_slice(&output.stdout).expect("a Workshop view")
}

/// A home with one run still running.
fn a_home_with_a_run() -> Option<(Home, String)> {
    let home = home(&Options::default(), &[])?;
    let id = launch(&home, home.seeded()).id;
    Some((home, id))
}

/// AD-121: the view holds the engine, the images with their battery, the
/// live run, the limits, the credential's state and action and the
/// routes; exit 0.
#[test]
fn workshop_json_prints_the_view() {
    let Some((home, id)) = a_home_with_a_run() else {
        return;
    };
    let linux = home.exe_dir.join("linux");
    let view = view_of(&workshop_json(&home.env, &home.engine.exe, Some(&linux)));
    assert_eq!(view.schema, WORKSHOP_VIEW_SCHEMA);
    assert!(view.enabled);
    let engine = view.engine.as_ref().expect("the engine was read");
    assert_eq!(engine.status, EngineStatus::Ready, "{engine:?}");
    assert!(engine.key.is_some());
    let tags: Vec<&str> = view.images.iter().map(|i| i.tag.as_str()).collect();
    assert!(
        tags.contains(&WORKSHOP_BASE) && tags.contains(&GATEWAY_IMAGE),
        "{tags:?}"
    );
    let base = view.images.iter().find(|i| i.tag == WORKSHOP_BASE).unwrap();
    assert_eq!(base.state, ImageState::Ready, "{base:?}");
    let battery = base.battery.as_ref().expect("the base has a battery");
    assert_eq!(battery.valid, Some(true), "{battery:?}");
    let gateway = view.images.iter().find(|i| i.tag == GATEWAY_IMAGE).unwrap();
    assert!(gateway.battery.is_none());
    assert_eq!(
        view.runs
            .live
            .iter()
            .map(|r| (r.id.as_str(), r.state))
            .collect::<Vec<_>>(),
        [(id.as_str(), RunState::Running)]
    );
    assert_eq!(view.limits.as_ref().map(|l| l.live), Some(1));
    // The test floor's key names no Credential Manager entry: missing, and
    // the token floor's action.
    let credential = view.credential.as_ref().expect("a credential");
    assert_eq!(credential.kind, CredentialKind::OauthToken);
    assert_eq!(credential.state, CredentialState::Missing);
    assert!(matches!(
        credential.action,
        Some(CredentialAction::SetupToken { .. })
    ));
    let network = view.network.as_ref().expect("the routes");
    assert_eq!(network.setting, "none");
    assert!(
        network.routes.iter().all(|r| r.route == "direct"),
        "{network:?}"
    );
    assert!(!view.workshop_stopped && !view.relay_stopped);
}

/// D-47: a registry that cannot be read exits 3 with the reason, never 2;
/// a usage error exits 64.
#[test]
fn an_unreadable_registry_exits_3() {
    let env = TestEnv::new();
    let registry = env.officina_home().join("registry.db");
    std::fs::create_dir_all(&registry).unwrap();
    let missing = env.root.path().join("no-docker").join("docker.exe");
    let output = workshop_json(&env, &missing, None);
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("registry"), "{stderr}");
    let output = env.run(&["workshop"], b"");
    assert_eq!(output.status.code(), Some(64));
    let output = env.run(&["workshop", "--json", "--extra"], b"");
    assert_eq!(output.status.code(), Some(64));
}

/// AD-121: with the engine down the registry's part still prints: the
/// runs, the per-template table and the flags; the engine says why.
#[test]
fn the_engine_down_still_prints_the_registry_part() {
    let Some((home, id)) = a_home_with_a_run() else {
        return;
    };
    let missing = home.env.root.path().join("no-docker").join("docker.exe");
    let view = view_of(&workshop_json(&home.env, &missing, None));
    let engine = view.engine.as_ref().expect("the engine's state");
    assert_ne!(engine.status, EngineStatus::Ready, "{engine:?}");
    assert!(engine.key.is_none());
    assert_eq!(
        view.runs
            .live
            .iter()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>(),
        [id.as_str()]
    );
    assert!(view.runs.orphans.is_empty());
    assert!(view.enabled);
    assert!(view.images.iter().all(|i| i.state != ImageState::Ready));
}
