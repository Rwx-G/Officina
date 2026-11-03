//! Following a run, its limits and reconciliation end to end (story 6.12
//! AC 4 to 7; AD-109, D-216, D-223, D-230): a card launched through the
//! window's functions on the fake engine, whose followers replay the lines
//! the sidecar gives each role, then a second window's reconciliation
//! after the first one's followers are dropped.

mod common;

use std::time::{Duration, Instant};

use officina_core::relay::journal::JournalEntry;
use officina_core::workshop::gateway::{GatewayLine, GatewayRefusal};
use officina_core::workshop::launch::WORKER_DISALLOWED_TOOLS;
use officina_core::workshop::lifecycle::{ReconcileAction, StopReason};
use officina_core::workshop::names::LOGIN_ROLE;
use officina_core::workshop::output::{MAX_ALLOWED_HOSTS, OTHER_HOSTS, OUTPUT_CAP_BYTES, Pipe};
use officina_core::workshop::run::{FinishCause, Recorded, RunState, WorkshopOutcome};
use officina_core::workshop::stream::MAX_LINE_BYTES;
use officina_core::workshop::vector::ContainerRole;
use officina_host::workshop::follow::{FollowContext, reconcile_now};
use officina_host::workshop::launch::WorkshopRuntime;

use common::workshop::SeededStore;
use common::workshop_home::{Home, Options, SEEDED};

const WAIT: Duration = Duration::from_secs(30);

fn until(what: &str, mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < WAIT, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn init(tools: &[&str]) -> String {
    serde_json::json!({"type": "system", "subtype": "init", "model": "claude-test", "tools": tools, "mcp_servers": []}).to_string()
}

fn said(text: &str) -> String {
    serde_json::json!({"type": "assistant", "message": {"content": [{"type": "text", "text": text}]}}).to_string()
}

fn result() -> String {
    serde_json::json!({"type": "result", "subtype": "success", "is_error": false, "total_cost_usd": 0.02, "num_turns": 2, "duration_ms": 900, "session_id": "s"}).to_string()
}

fn allowed(host: &str) -> String {
    GatewayLine::Allowed {
        host: host.to_owned(),
        port: 443,
        ech: false,
    }
    .to_line()
}

fn refused(host: &str) -> String {
    GatewayLine::refused(host, 443, GatewayRefusal::NotAllowed, None).to_line()
}

/// The sidecar's `follow` key: each role's stdout lines.
fn follow(worker: &[String], gateway: &[String]) -> serde_json::Value {
    serde_json::json!({
        "worker": {"stdout": worker.join("\n")},
        "gateway": {"stdout": gateway.join("\n")},
    })
}

/// A home whose card is launched with the fake engine's followers set to
/// `follow` first.
fn launched(follow: serde_json::Value, cut_first: bool) -> Option<(Home, WorkshopRuntime, String)> {
    let home = Home::start(&Options::default())?;
    home.set_engine("follow", follow);
    home.set_engine("follow_cut_first", cut_first.into());
    let runtime = home.seeded();
    let card = home.issue(&runtime);
    home.launch(&card, &runtime);
    Some((home, runtime, card.id))
}

fn context(home: &Home, runtime: WorkshopRuntime) -> FollowContext {
    FollowContext {
        runtime,
        collector: home.sender().clone(),
        home: home.env.officina_home(),
        base_env: vec![(
            "SYSTEMROOT".to_owned(),
            std::env::var("SYSTEMROOT").unwrap_or_default(),
        )],
    }
}

/// Changes container `name` of the fake engine's state; removes it when
/// `state` is `None`.
fn set_container(home: &Home, name: &str, state: Option<(&str, i64)>) {
    let path = home.engine.dir.path().join("fake-docker-state.json");
    let mut all: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let containers = all["containers"].as_array_mut().unwrap();
    match state {
        Some((text, code)) => {
            for c in containers.iter_mut().filter(|c| c["name"] == name) {
                c["state"] = text.into();
                c["exit_code"] = code.into();
            }
        }
        None => containers.retain(|c| c["name"] != name),
    }
    std::fs::write(&path, all.to_string()).unwrap();
}

fn worker(run_id: &str) -> String {
    format!("officina-worker-{run_id}")
}

fn gateway(run_id: &str) -> String {
    format!("officina-gateway-{run_id}")
}

/// The first window closes: its followers stay, but hold no run.
fn close_window(runtime: &WorkshopRuntime) {
    runtime.followed.lock().unwrap().clear();
}

fn refusals(home: &Home, run_id: &str) -> Vec<String> {
    home.env
        .journal()
        .into_iter()
        .filter_map(|line| match line.entry {
            JournalEntry::GatewayRefused(r) if r.run_id.as_deref() == Some(run_id) => Some(r.host),
            _ => None,
        })
        .collect()
}

fn journal_text(home: &Home) -> String {
    home.env
        .journal()
        .into_iter()
        .map(|line| line.to_line())
        .collect()
}

fn follows(home: &Home, name: &str) -> u64 {
    let path = home.engine.dir.path().join("fake-docker-follows.json");
    std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|v| v[name].as_u64())
        .unwrap_or(0)
}

/// AD-109: a follower that ends while its container runs reads again from
/// the start, and the facts come out as one reading gives them.
#[test]
fn facts_rebuild_identically_after_a_restart() {
    let worker_lines = vec![init(&["Bash", "Edit"]), said("done"), result()];
    let gateway_lines = vec![allowed("index.crates.io"), refused("off.example.com")];
    let Some((home, runtime, id)) = launched(follow(&worker_lines, &gateway_lines), true) else {
        return;
    };
    until("a second follow", || {
        follows(&home, &worker(&id)) >= 2 && follows(&home, &gateway(&id)) >= 2
    });
    let mut once = officina_core::workshop::output::RunOutput::default();
    for line in &worker_lines {
        let read = officina_core::workshop::output::LineRead::Line(line.clone());
        once.observe(ContainerRole::Worker, Pipe::Stdout, &read);
    }
    for line in &gateway_lines {
        let read = officina_core::workshop::output::LineRead::Line(line.clone());
        once.observe(ContainerRole::Gateway, Pipe::Stdout, &read);
    }
    until("the facts", || {
        runtime.followed.lock().unwrap()[&id].output == once
    });
}

/// D-216: a line over 1 MiB is skipped and counted; the next one folds.
#[test]
fn an_oversized_line_is_skipped_and_counted() {
    let long = format!(
        "{{\"type\":\"x\",\"pad\":\"{}\"}}",
        "y".repeat(MAX_LINE_BYTES)
    );
    let Some((_home, runtime, id)) = launched(follow(&[long, result()], &[]), false) else {
        return;
    };
    until("the result", || {
        runtime.followed.lock().unwrap()[&id]
            .output
            .stream
            .result
            .is_some()
    });
    let followed = runtime.followed.lock().unwrap();
    assert_eq!(followed[&id].output.oversized, 1);
    assert_eq!(followed[&id].output.stream.skipped, 1);
}

/// D-216: a leading `*.` entry admits unbounded names; the counts keep a
/// bounded number of them.
#[test]
fn a_flood_of_allowed_wildcard_hosts_keeps_256_entries() {
    let flood: Vec<String> = (0..MAX_ALLOWED_HOSTS + 44)
        .map(|n| allowed(&format!("h{n}.example.com")))
        .collect();
    let Some((_home, runtime, id)) = launched(follow(&[], &flood), false) else {
        return;
    };
    until("the flood", || {
        runtime.followed.lock().unwrap()[&id]
            .output
            .gateway
            .allowed
            .get(OTHER_HOSTS)
            == Some(&44)
    });
    assert_eq!(
        runtime.followed.lock().unwrap()[&id]
            .output
            .gateway
            .allowed
            .len(),
        MAX_ALLOWED_HOSTS + 1
    );
}

/// AD-120: each refusal is journaled once, whatever the replays.
#[test]
fn each_refusal_is_journaled_once() {
    let gateway_lines = vec![refused("a.example.com"), refused("b.example.com")];
    let Some((home, runtime, id)) = launched(follow(&[], &gateway_lines), true) else {
        return;
    };
    until("a second follow", || follows(&home, &gateway(&id)) >= 2);
    until("two refusals", || refusals(&home, &id).len() >= 2);
    // A second window reads both again from the start.
    close_window(&runtime);
    let again = home.seeded();
    reconcile_now(&context(&home, again.clone()));
    until("the second window's fold", || {
        again
            .followed
            .lock()
            .unwrap()
            .get(&id)
            .is_some_and(|f| f.output.gateway.refused == 2)
    });
    home.settle();
    assert_eq!(refusals(&home, &id), ["a.example.com", "b.example.com"]);
    assert_eq!(home.run(&id).facts.refusals_journaled, 2);
}

/// D-230: a token in a refused host is replaced before the journal.
#[test]
fn a_token_in_a_refused_host_is_replaced_before_the_journal() {
    let host = format!("{SEEDED}.example.com");
    let Some((home, _runtime, id)) = launched(follow(&[], &[refused(&host)]), false) else {
        return;
    };
    until("the refusal", || refusals(&home, &id).len() == 1);
    let journal = journal_text(&home);
    assert!(!journal.contains(SEEDED));
    assert!(journal.contains(officina_core::workshop::scan::REPLACEMENT));
}

/// D-230: a token in a wildcard-allowed host leaves memory replaced.
#[test]
fn a_token_in_a_wildcard_allowed_host_is_replaced_before_the_view() {
    let host = format!("{SEEDED}.example.com");
    let Some((_home, runtime, id)) = launched(follow(&[], &[allowed(&host)]), false) else {
        return;
    };
    until("the host", || {
        !runtime.followed.lock().unwrap()[&id]
            .output
            .gateway
            .allowed
            .is_empty()
    });
    let texts = runtime.followed.lock().unwrap()[&id].texts().unwrap();
    let shown = serde_json::to_string(&texts).unwrap();
    assert!(!shown.contains(SEEDED), "{shown}");
    assert!(shown.contains(officina_core::workshop::scan::REPLACEMENT));
}

/// D-230: a token in the report tail mid-run leaves memory replaced, and
/// is stored replaced once the worker exits.
#[test]
fn a_token_in_a_mid_run_report_tail_is_replaced_before_the_view() {
    let lines = vec![said(&format!("the token is {SEEDED}"))];
    let Some((home, runtime, id)) = launched(follow(&lines, &[]), false) else {
        return;
    };
    until("the tail", || {
        runtime.followed.lock().unwrap()[&id]
            .output
            .stream
            .assistant_tail
            .is_some()
    });
    let texts = runtime.followed.lock().unwrap()[&id].texts().unwrap();
    assert!(!serde_json::to_string(&texts).unwrap().contains(SEEDED));
    set_container(&home, &worker(&id), Some(("exited", 0)));
    until("collected", || {
        matches!(home.run(&id).state, RunState::Collecting | RunState::Failed)
    });
    let stored = serde_json::to_string(&home.run(&id).texts).unwrap();
    assert!(!stored.contains(SEEDED), "{stored}");
    assert!(stored.contains(officina_core::workshop::scan::REPLACEMENT));
}

/// D-230: a window that cannot read the credential at reconciliation
/// holds the run's hosts until collection's scan.
#[test]
fn without_a_credential_hosts_wait_for_collection() {
    let host = format!("{SEEDED}.example.com");
    let Some((home, runtime, id)) = launched(follow(&[], &[refused(&host)]), false) else {
        return;
    };
    until("the first window's refusal", || {
        refusals(&home, &id).len() == 1
    });
    close_window(&runtime);
    // The registry says the refusal is journaled; a second window with no
    // credential journals nothing and holds what it reads past it.
    let blind = home.runtime(SeededStore(None));
    home.set_engine(
        "follow",
        follow(&[], &[refused(&host), refused(&format!("x.{host}"))]),
    );
    reconcile_now(&context(&home, blind.clone()));
    until("the held refusal", || {
        blind
            .followed
            .lock()
            .unwrap()
            .get(&id)
            .is_some_and(|f| f.held.len() == 1)
    });
    let followed = blind.followed.lock().unwrap();
    assert!(followed[&id].needles().is_none());
    assert!(followed[&id].texts().is_none());
    drop(followed);
    home.settle();
    assert_eq!(refusals(&home, &id).len(), 1);
    assert!(!journal_text(&home).contains(SEEDED));
}

/// AD-109: the window stops the worker past its deadline plus the grace,
/// at window start too; the cause is the deadline.
#[test]
fn the_deadline_stops_the_worker() {
    let Some((home, runtime, id)) = launched(follow(&[], &[]), false) else {
        return;
    };
    close_window(&runtime);
    let mut later = home.seeded();
    later.now_ms = || common::now_ms() + 3 * 60 * 60 * 1000;
    let actions = reconcile_now(&context(&home, later));
    assert!(actions.contains(&ReconcileAction::StopPastDeadline { run_id: id.clone() }));
    until("the cause", || {
        home.run(&id).cause == Some(Recorded::Known(FinishCause::Deadline))
    });
    let run = home.run(&id);
    assert!(matches!(run.state, RunState::Collecting | RunState::Failed));
    assert_eq!(run.facts.stop_reason, Some(StopReason::Deadline));
    assert!(home.count(0, &["stop"]) >= 1);
}

/// AD-109: 64 MiB of output stops the worker.
#[test]
fn the_output_cap_stops_the_worker() {
    let Some(home) = Home::start(&Options::default()) else {
        return;
    };
    home.set_engine(
        "follow",
        serde_json::json!({"worker": {"bulk_bytes": OUTPUT_CAP_BYTES + 128 * 1024}}),
    );
    let runtime = home.seeded();
    let card = home.issue(&runtime);
    home.launch(&card, &runtime);
    until("the cause", || {
        home.run(&card.id).cause == Some(Recorded::Known(FinishCause::OutputCap))
    });
    // a_limit_sets_no_flag (FR-14): stopping one run sets no stop flag.
    assert!(!officina_host::workshop::workshop_stopped(
        &home.env.officina_home()
    ));
}

/// AD-109, R50: a disallowed tool in the first init stops the worker.
#[test]
fn a_stream_stop_stops_the_worker() {
    let lines = vec![init(WORKER_DISALLOWED_TOOLS), result()];
    let Some((home, _runtime, id)) = launched(follow(&lines, &[]), false) else {
        return;
    };
    until("the cause", || {
        home.run(&id).cause == Some(Recorded::Known(FinishCause::UnexpectedTool))
    });
    assert_eq!(
        home.run(&id).facts.stop_reason,
        Some(StopReason::UnexpectedTool)
    );
}

/// FR-14: no limit sets the workshop's or the relay's stop flag.
#[test]
fn a_limit_sets_no_flag() {
    let lines = vec![init(WORKER_DISALLOWED_TOOLS)];
    let Some((home, _runtime, id)) = launched(follow(&lines, &[]), false) else {
        return;
    };
    until("the stop", || home.run(&id).facts.stop_reason.is_some());
    assert!(!officina_host::workshop::workshop_stopped(
        &home.env.officina_home()
    ));
    assert!(!home.env.relay_home().relay_stopped());
}

/// AD-109: a second window finds the live run and follows it.
#[test]
fn a_window_restart_finds_and_follows_a_live_run() {
    let lines = vec![init(&["Bash"]), said("working")];
    let Some((home, runtime, id)) = launched(follow(&lines, &[]), false) else {
        return;
    };
    close_window(&runtime);
    let again = home.seeded();
    let actions = reconcile_now(&context(&home, again.clone()));
    assert!(actions.contains(&ReconcileAction::Follow { run_id: id.clone() }));
    until("the fold", || {
        again
            .followed
            .lock()
            .unwrap()
            .get(&id)
            .is_some_and(|f| f.output.stream.assistant_tail.is_some())
    });
    assert!(again.followed.lock().unwrap()[&id].needles().is_some());
    assert_eq!(home.run(&id).state, RunState::Running);
}

/// AD-109: an engine that does not answer changes no row.
#[test]
fn an_engine_down_at_start_changes_no_row() {
    let Some((home, runtime, id)) = launched(follow(&[], &[]), false) else {
        return;
    };
    close_window(&runtime);
    home.set_engine("version", "down".into());
    set_container(&home, &worker(&id), None);
    let actions = reconcile_now(&context(&home, home.seeded()));
    assert!(actions.is_empty());
    home.settle();
    assert_eq!(home.run(&id).state, RunState::Running);
}

/// D-216: once the engine answers again, reconciliation runs, and a run
/// whose container is gone reads `unknown`.
#[test]
fn an_engine_back_reconciles() {
    let Some((home, runtime, id)) = launched(follow(&[], &[]), false) else {
        return;
    };
    close_window(&runtime);
    home.set_engine("version", "down".into());
    set_container(&home, &worker(&id), None);
    reconcile_now(&context(&home, home.seeded()));
    home.set_engine("version", "linux".into());
    until("unknown", || home.run(&id).state == RunState::Unknown);
}

/// Principle 3: a run the engine lost is `unknown`, never finished, and
/// its `workshop_finished` says so.
#[test]
fn a_lost_container_is_unknown_never_finished() {
    let Some((home, runtime, id)) = launched(follow(&[], &[]), false) else {
        return;
    };
    close_window(&runtime);
    set_container(&home, &worker(&id), None);
    let actions = reconcile_now(&context(&home, home.seeded()));
    assert!(actions.contains(&ReconcileAction::Lost { run_id: id.clone() }));
    until("unknown", || home.run(&id).state == RunState::Unknown);
    let run = home.run(&id);
    assert_eq!(run.outcome, Some(WorkshopOutcome::Unknown));
    assert_eq!(run.cause, Some(Recorded::Known(FinishCause::EngineLost)));
    let finished: Vec<_> = home
        .env
        .journal()
        .into_iter()
        .filter_map(|line| match line.entry {
            JournalEntry::WorkshopFinished(f) if f.run_id == id => Some(f),
            _ => None,
        })
        .collect();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].outcome, WorkshopOutcome::Unknown);
}

/// The floor's key, read from the worker's labels.
fn floor_of(home: &Home, id: &str) -> String {
    let state: serde_json::Value = serde_json::from_slice(
        &std::fs::read(home.engine.dir.path().join("fake-docker-state.json")).unwrap(),
    )
    .unwrap();
    state["containers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == worker(id))
        .unwrap()["labels"]["officina.floor"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn add_container(home: &Home, name: &str, floor: &str, role: &str) {
    let path = home.engine.dir.path().join("fake-docker-state.json");
    let mut all: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    all["containers"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "id": format!("c{name}"),
            "name": name,
            "state": "running",
            "labels": {
                "officina.managed": "1",
                "officina.floor": floor,
                "officina.role": role,
                "officina.run": "f".repeat(32),
            },
        }));
    std::fs::write(&path, all.to_string()).unwrap();
}

/// AD-109, D-215: a run container of this floor with no row is listed,
/// never adopted.
#[test]
fn an_orphan_is_listed_not_adopted() {
    let Some((home, runtime, id)) = launched(follow(&[], &[]), false) else {
        return;
    };
    close_window(&runtime);
    let floor = floor_of(&home, &id);
    let orphan = format!("officina-worker-{}", "f".repeat(32));
    add_container(&home, &orphan, &floor, ContainerRole::Worker.code());
    let again = home.seeded();
    let actions = reconcile_now(&context(&home, again.clone()));
    assert!(actions.contains(&ReconcileAction::Orphan { name: orphan }));
    assert!(!again.followed.lock().unwrap().contains_key(&"f".repeat(32)));
}

/// AD-109, D-605: a job's containers and the owner's login container are
/// never orphans.
#[test]
fn job_and_login_containers_are_never_orphans() {
    let Some((home, runtime, id)) = launched(follow(&[], &[]), false) else {
        return;
    };
    close_window(&runtime);
    let floor = floor_of(&home, &id);
    let roles: Vec<&str> = ContainerRole::ALL
        .iter()
        .map(|r| r.code())
        .chain([LOGIN_ROLE])
        .collect();
    for role in &roles {
        add_container(
            &home,
            &format!("officina-{role}-orphan-probe"),
            &floor,
            role,
        );
    }
    let actions = reconcile_now(&context(&home, home.seeded()));
    for role in roles {
        let listed = actions.contains(&ReconcileAction::Orphan {
            name: format!("officina-{role}-orphan-probe"),
        });
        assert_eq!(
            listed,
            officina_core::workshop::lifecycle::is_run_role(role),
            "{role}"
        );
    }
}
