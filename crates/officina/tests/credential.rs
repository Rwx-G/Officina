//! The credential end to end (story 6.14 AC 3; AD-125, PRD section 7
//! Credential, D-230, review F2, F14): a run through the fake engine with
//! a seeded credential, whose streams carry the value in a forged refused
//! host, in a wildcard-allowed host and in a mid-run report tail, and
//! whose engine `inspect` holds it in `Config.Env`. No form of the value
//! reaches a file under the test home, the journal, the registry, a log,
//! an answer of the window's functions or the fake engine's call log,
//! while the run is live and after, a restart and a quarantine included.

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use officina_core::workshop::floor::CredentialKind;
use officina_core::workshop::gateway::{GatewayLine, GatewayRefusal};
use officina_core::workshop::scan::{Needles, find, needles};
use officina_core::workshop::tar::{
    PACKAGE_BUNDLE, PACKAGE_DIR, PACKAGE_MANIFEST, PACKAGE_TIP, stream_of,
};
use officina_core::workshop::vector::credential_name;
use officina_host::workshop::follow::{FollowContext, reconcile_now};
use officina_host::workshop::launch::WorkshopRuntime;

use common::git_support::run_git;
use common::workshop_home::{Home, Options, SEEDED};

const WAIT: Duration = Duration::from_secs(60);

fn until(what: &str, mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < WAIT, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn base_env() -> Vec<(String, String)> {
    vec![(
        "SYSTEMROOT".to_owned(),
        std::env::var("SYSTEMROOT").unwrap_or_default(),
    )]
}

fn said(text: &str) -> String {
    serde_json::json!({"type": "assistant", "message": {"content": [{"type": "text", "text": text}]}}).to_string()
}

fn result() -> String {
    serde_json::json!({"type": "result", "subtype": "success", "is_error": false, "total_cost_usd": 0.02, "num_turns": 2, "duration_ms": 900, "session_id": "s"}).to_string()
}

/// The hostile run: the value mid-run in the report, in a forged refused
/// host and in a host the gateway allowed through a wildcard.
fn hostile() -> Option<(Home, WorkshopRuntime, String)> {
    let home = Home::start(&Options::default())?;
    let gateway = [
        GatewayLine::refused(
            &format!("{SEEDED}.example.com"),
            443,
            GatewayRefusal::NotAllowed,
            None,
        )
        .to_line(),
        GatewayLine::Allowed {
            host: format!("{SEEDED}.cdn.example.com"),
            port: 443,
            ech: false,
        }
        .to_line(),
    ];
    let worker = [said(&format!("mid-run, the token is {SEEDED}")), result()];
    home.set_engine(
        "follow",
        serde_json::json!({"worker": {"stdout": worker.join("\n")}, "gateway": {"stdout": gateway.join("\n")}}),
    );
    let runtime = home.seeded();
    let card = home.issue(&runtime);
    let answers = vec![
        serde_json::to_string(&card).unwrap(),
        serde_json::to_string(&home.launch(&card, &runtime)).unwrap(),
    ];
    for answer in &answers {
        assert_nowhere(&needles(SEEDED.as_bytes()), "an answer", answer.as_bytes());
    }
    seed_inspect(&home, &card.id);
    Some((home, runtime, card.id))
}

/// The worker's `Config.Env` holds the value, as a real engine's does
/// (review F14): a formatted `inspect` must never return it.
fn seed_inspect(home: &Home, run_id: &str) {
    let path = home.engine.dir.path().join("fake-docker-state.json");
    let mut all: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let worker = format!("officina-worker-{run_id}");
    let name = credential_name(CredentialKind::OauthToken).unwrap();
    for c in all["containers"].as_array_mut().unwrap() {
        if c["name"] == worker.as_str() {
            c["env"] = serde_json::json!([format!("{name}={SEEDED}")]);
        }
    }
    std::fs::write(&path, all.to_string()).unwrap();
}

fn assert_nowhere(found: &Needles, what: &str, bytes: &[u8]) {
    assert!(find(bytes, found).is_empty(), "the value in {what}");
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut all = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            all.extend(files_under(&path));
        } else {
            all.push(path);
        }
    }
    all
}

/// No form of the value in any file under the test home (the registry,
/// its WAL, the journal, the logs, the run folders), in the review view,
/// or in the fake engine's call log, whose lines name the credential on
/// the worker's `create` only (D-223).
fn nowhere(home: &Home, runtime: &WorkshopRuntime, run_id: &str) {
    let found = needles(SEEDED.as_bytes());
    for file in files_under(home.env.root.path()) {
        if let Ok(bytes) = std::fs::read(&file) {
            assert_nowhere(&found, &file.display().to_string(), &bytes);
        }
    }
    if let Ok(view) = officina_host::workshop::review::open_review(
        home.sender(),
        runtime,
        &home.env.officina_home(),
        &base_env(),
        run_id,
    ) {
        assert_nowhere(
            &found,
            "the review view",
            serde_json::to_string(&view).unwrap().as_bytes(),
        );
    }
    let row = format!("{:?}", home.run(run_id));
    assert_nowhere(&found, "the run's row", row.as_bytes());
    let name = credential_name(CredentialKind::OauthToken).unwrap();
    for line in home.engine.log() {
        let text = line.to_string();
        assert_nowhere(&found, "the engine log", text.as_bytes());
        let argv = line["argv"].to_string();
        let worker_create = argv.contains("\"create\"") && argv.contains("officina-worker-");
        assert_eq!(
            line["env_names"].to_string().contains(name),
            worker_create,
            "{argv}"
        );
    }
}

fn context(home: &Home, runtime: WorkshopRuntime) -> FollowContext {
    FollowContext {
        runtime,
        collector: home.sender().clone(),
        home: home.env.officina_home(),
        base_env: base_env(),
    }
}

fn git_text(home: &Home, dir: &Path, args: &[&str]) -> String {
    let out = run_git(&home.git, dir, args);
    assert!(out.status.success(), "{args:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// The fake engine's packer copy streams a package of the run's branch
/// with `file` committed holding `text`; then the worker exits 0.
fn finish(home: &Home, run_id: &str, file: &str, text: &str) {
    let row = home.run(run_id);
    // The clone is the worker's, not Officina's: out of the searched home.
    let clone = home
        .engine
        .dir
        .path()
        .join(format!("clone-{}", &run_id[..8]));
    let project = home.project();
    git_text(
        home,
        home.env.root.path(),
        &[
            "clone",
            "-q",
            &project.to_string_lossy(),
            &clone.to_string_lossy(),
        ],
    );
    git_text(home, &clone, &["checkout", "-q", "-b", &row.branch]);
    std::fs::create_dir_all(clone.join(file).parent().unwrap()).unwrap();
    std::fs::write(clone.join(file), text).unwrap();
    git_text(home, &clone, &["add", "-f", "--", file]);
    git_text(home, &clone, &["commit", "-q", "-m", "change"]);
    let tip = git_text(home, &clone, &["rev-parse", "HEAD"]);
    let bundle = clone.join("work.bundle.tmp");
    git_text(
        home,
        &clone,
        &[
            "bundle",
            "create",
            "-q",
            &bundle.to_string_lossy(),
            &format!("refs/heads/{}", row.branch),
            &format!("^{}", row.base_commit),
        ],
    );
    let bytes = std::fs::read(&bundle).unwrap();
    std::fs::remove_file(&bundle).unwrap();
    let tip_line = format!("{tip}\n");
    let tar = stream_of(&[
        (PACKAGE_DIR, b"", b'5'),
        (PACKAGE_TIP, tip_line.as_bytes(), b'0'),
        (PACKAGE_MANIFEST, b"{\"steps\":[]}", b'0'),
        (PACKAGE_BUNDLE, &bytes, b'0'),
    ]);
    let path = home
        .engine
        .dir
        .path()
        .join(format!("package-{}.tar", &run_id[..8]));
    std::fs::write(&path, tar).unwrap();
    home.set_engine("package", path.to_string_lossy().into_owned().into());
    home.set_engine("package_extra_bytes", 0.into());
    let state = home.engine.dir.path().join("fake-docker-state.json");
    let mut all: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&state).unwrap()).unwrap();
    let worker = format!("officina-worker-{run_id}");
    for c in all["containers"].as_array_mut().unwrap() {
        if c["name"] == worker.as_str() {
            c["state"] = "exited".into();
            c["exit_code"] = 0.into();
        }
    }
    std::fs::write(&state, all.to_string()).unwrap();
    until("the run's outcome", || home.run(run_id).outcome.is_some());
    home.settle();
}

/// The run's output read: the report tail and both gateway lines folded.
fn followed_whole(runtime: &WorkshopRuntime, run_id: &str) {
    until("the run's output", || {
        runtime
            .followed
            .lock()
            .unwrap()
            .get(run_id)
            .is_some_and(|f| {
                f.output.gateway.refused == 1
                    && !f.output.gateway.allowed.is_empty()
                    && f.output.stream.result.is_some()
            })
    });
}

#[test]
fn the_value_is_nowhere_while_the_run_is_live() {
    let Some((home, runtime, id)) = hostile() else {
        return;
    };
    followed_whole(&runtime, &id);
    home.settle();
    nowhere(&home, &runtime, &id);
}

#[test]
fn the_value_is_nowhere_after_a_full_run() {
    let Some((home, runtime, id)) = hostile() else {
        return;
    };
    followed_whole(&runtime, &id);
    finish(&home, &id, "src/lib.rs", "pub fn bumped() {}\n");
    nowhere(&home, &runtime, &id);
    assert!(
        home.run(&id)
            .facts
            .credential_locations
            .is_some_and(|n| n >= 3)
    );
}

#[test]
fn the_value_is_nowhere_after_a_quarantine() {
    let Some((home, runtime, id)) = hostile() else {
        return;
    };
    followed_whole(&runtime, &id);
    finish(&home, &id, "leak.txt", SEEDED);
    let row = home.run(&id);
    assert_eq!(row.quarantine_cause.as_deref(), Some("credential_found"));
    nowhere(&home, &runtime, &id);
}

#[test]
fn the_value_is_nowhere_after_a_restart() {
    let Some((home, runtime, id)) = hostile() else {
        return;
    };
    followed_whole(&runtime, &id);
    // The window closes; a second one reconciles and follows again from
    // a fresh credential read (D-230).
    runtime.followed.lock().unwrap().clear();
    let again = home.seeded();
    reconcile_now(&context(&home, again.clone()));
    followed_whole(&again, &id);
    home.settle();
    nowhere(&home, &again, &id);
    finish(&home, &id, "src/lib.rs", "pub fn bumped() {}\n");
    nowhere(&home, &again, &id);
}

/// Story 6.26 AC 6 (D-609): a `login` run, whose credential the host
/// never holds. The value is planted in the fake engine's stored login
/// (the scanner's needle) and in the run's stream and gateway lines: after
/// collection it is in no file under the test home, no journal line, no
/// registry row, no review view, and never in the engine's call log; no
/// call carries a credential name.
#[test]
fn the_value_is_nowhere_after_a_login_run() {
    let Some(home) = Home::start(&Options {
        auth: "login",
        ..Options::default()
    }) else {
        return;
    };
    home.seed_login_volume();
    home.set_engine("scanner_needle", SEEDED.into());
    let gateway = [GatewayLine::refused(
        &format!("{SEEDED}.example.com"),
        443,
        GatewayRefusal::NotAllowed,
        None,
    )
    .to_line()];
    let worker = [said(&format!("mid-run, the token is {SEEDED}")), result()];
    home.set_engine(
        "follow",
        serde_json::json!({"worker": {"stdout": worker.join("\n")}, "gateway": {"stdout": gateway.join("\n")}}),
    );
    let runtime = home.runtime(common::workshop::SeededStore(None));
    let card = home.issue(&runtime);
    home.launch(&card, &runtime);
    until("the run's output", || {
        runtime
            .followed
            .lock()
            .unwrap()
            .get(&card.id)
            .is_some_and(|f| f.output.gateway.refused == 1 && f.output.stream.result.is_some())
    });
    finish(&home, &card.id, "src/lib.rs", "pub fn bumped() {}\n");
    let found = needles(SEEDED.as_bytes());
    for file in files_under(home.env.root.path()) {
        if let Ok(bytes) = std::fs::read(&file) {
            assert_nowhere(&found, &file.display().to_string(), &bytes);
        }
    }
    if let Ok(view) = officina_host::workshop::review::open_review(
        home.sender(),
        &runtime,
        &home.env.officina_home(),
        &base_env(),
        &card.id,
    ) {
        assert_nowhere(
            &found,
            "the review view",
            serde_json::to_string(&view).unwrap().as_bytes(),
        );
    }
    let row = home.run(&card.id);
    assert_nowhere(&found, "the run's row", format!("{row:?}").as_bytes());
    let names: Vec<&str> = CredentialKind::ALL
        .iter()
        .filter_map(|kind| credential_name(*kind))
        .collect();
    for line in home.engine.log() {
        let text = line.to_string();
        assert_nowhere(&found, "the engine log", text.as_bytes());
        for name in &names {
            assert!(!text.contains(name), "{name} in {text}");
        }
    }
    assert!(
        row.facts.credential_locations.is_some_and(|n| n >= 2),
        "{:?}",
        row.facts
    );
}
