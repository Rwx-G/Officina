//! Stop all, the stop flags, Resume and kept containers (story 6.17;
//! AD-116, AD-121, D-213, D-215, D-241): the window's functions on the
//! fake engine (D-223) with its stop and start latencies, and
//! `officina stop --all` as a process. No real engine, no real token.

mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use officina_core::orders::card::{LaunchAnswer, LaunchRefusal};
use officina_core::registry::types::LaunchOutcome;
use officina_core::relay::journal::{JournalEntry, WorkshopStopped};
use officina_core::relay::{StopBy, StopFlag};
use officina_core::workshop::battery::{BatteryAnswer, BatteryRefusal};
use officina_core::workshop::image::{BuildAnswer, BuildRefusal};
use officina_core::workshop::review::DiscardReason;
use officina_core::workshop::run::{
    KEPT_TTL_MS, KeptChoice, RemovalCause, RemoveRunAnswer, RemoveRunRefusal, ResumeChoice,
    RunState, STOP_BUDGET_MS, StepOutcome, StopScope, WorkshopOutcome,
};
use officina_host::workshop::follow::FollowContext;
use officina_host::workshop::resume::resume_workshop;
use officina_host::workshop::stop::{expire_kept, remove_run, stop_workshop};

use common::package_run::{Run, home, launch, until};
use common::workshop::FakeEngine;
use common::workshop_home::{Home, Options};
use common::{OFFICINA, TestEnv};

/// The fake engine's containers as its state holds them.
fn containers(engine: &FakeEngine) -> Vec<serde_json::Value> {
    let state: serde_json::Value = serde_json::from_slice(
        &std::fs::read(engine.dir.path().join("fake-docker-state.json")).unwrap(),
    )
    .unwrap();
    state["containers"].as_array().cloned().unwrap_or_default()
}

fn running(engine: &FakeEngine) -> Vec<String> {
    containers(engine)
        .iter()
        .filter(|c| c["state"] == "running")
        .map(|c| c["name"].as_str().unwrap_or_default().to_owned())
        .collect()
}

fn names_of(engine: &FakeEngine, run_id: &str) -> Vec<String> {
    containers(engine)
        .iter()
        .filter(|c| c["labels"]["officina.run"] == run_id)
        .map(|c| c["name"].as_str().unwrap_or_default().to_owned())
        .collect()
}

fn flag(path: &Path) -> Option<StopFlag> {
    StopFlag::parse(&std::fs::read(path).ok()?)
}

fn workshop_flag(home: &Home) -> std::path::PathBuf {
    officina_host::workshop::stopped_file(&home.env.officina_home())
}

fn journal(home: &Home) -> Vec<JournalEntry> {
    home.env.journal().into_iter().map(|l| l.entry).collect()
}

fn stopped_lines(entries: &[JournalEntry]) -> Vec<WorkshopStopped> {
    entries
        .iter()
        .filter_map(|e| match e {
            JournalEntry::WorkshopStopped(line) => Some(line.clone()),
            _ => None,
        })
        .collect()
}

/// A home whose worker runs until the test exits it.
fn a_home() -> Option<Home> {
    home(&Options::default(), &[])
}

/// A launched run, still running.
fn a_running(home: &Home) -> Run<'_> {
    let run = launch(home, home.seeded());
    assert_eq!(run.row().state, RunState::Running);
    run
}

/// Stop all from the window, on the run's runtime.
fn stop(run: &Run<'_>) -> officina_core::workshop::run::StopAnswer {
    let answer = stop_workshop(&run.context(), None).expect("an answer");
    run.home.settle();
    answer
}

fn removed_causes(entries: &[JournalEntry], run_id: &str) -> Vec<RemovalCause> {
    entries
        .iter()
        .filter_map(|e| match e {
            JournalEntry::WorkshopRemoved(line) if line.run_id == run_id => Some(line.cause),
            _ => None,
        })
        .collect()
}

fn finished_outcomes(entries: &[JournalEntry], run_id: &str) -> Vec<WorkshopOutcome> {
    entries
        .iter()
        .filter_map(|e| match e {
            JournalEntry::WorkshopFinished(line) if line.run_id == run_id => Some(line.outcome),
            _ => None,
        })
        .collect()
}

// AC 2: Stop all in the window.

/// AD-116, FR-14: every container of the run stops within the budget,
/// the run is `stopped` with its time, both flags are written by the
/// window, and the containers are kept.
#[test]
fn stop_all_ends_every_run_within_10_s() {
    let Some(home) = a_home() else {
        return;
    };
    let run = a_running(&home);
    assert_eq!(
        names_of(&home.engine, &run.id).len(),
        2,
        "gateway and worker"
    );
    let answer = stop(&run);
    assert!(answer.within_budget(), "{answer:?}");
    assert!(answer.engine_reached);
    assert_eq!(answer.containers, 2, "{answer:?}");
    assert!(
        running(&home.engine).is_empty(),
        "{:?}",
        running(&home.engine)
    );
    assert_eq!(names_of(&home.engine, &run.id).len(), 2, "containers kept");
    let row = run.row();
    assert_eq!(row.state, RunState::Stopped);
    assert!(row.stopped_at_ms.is_some());
    for path in [workshop_flag(&home), home.env.relay_home().stopped_file()] {
        assert_eq!(
            flag(&path).and_then(|f| f.by),
            Some(StopBy::Window),
            "{path:?}"
        );
    }
    let entries = journal(&home);
    assert!(entries.contains(&JournalEntry::RelayStopped));
    assert_eq!(
        stopped_lines(&entries),
        [WorkshopStopped {
            scope: StopScope::Floor,
            by: StopBy::Window,
            containers: 2
        }]
    );
    assert_eq!(answer.runs, 1);
}

/// The control of the budget check (AD-116): an engine whose `stop` takes
/// longer than 10 s fails it, so the assertion above can fail.
#[test]
fn a_slow_engine_past_10_s_fails_the_budget_check() {
    let Some(home) = a_home() else {
        return;
    };
    let run = a_running(&home);
    home.set_engine("stop_latency_ms", (STOP_BUDGET_MS + 500).into());
    let answer = stop(&run);
    assert!(!answer.within_budget(), "{answer:?}");
    assert!(answer.elapsed_ms > STOP_BUDGET_MS);
}

/// AD-116: the flags are files, so a window started again finds them and
/// launches nothing, its latch never set.
#[test]
fn flags_survive_a_restart() {
    let Some(home) = a_home() else {
        return;
    };
    let run = a_running(&home);
    stop(&run);
    let flags = home.env.relay_home().stop_flags();
    assert!(flags.relay && flags.workshop);
    // A fresh runtime: a new thread `workshop`, no latch.
    let restarted = home.seeded();
    assert!(!restarted.thread.latched());
    let card = home.issue(&restarted);
    let answer = home.launch(&card, &restarted);
    assert_eq!(
        answer,
        LaunchAnswer::Refused(LaunchRefusal::WorkshopStopped),
        "{answer:?}"
    );
}

/// D-241, review F20: a launch whose `start` the engine accepted but whose
/// client had not returned when Stop all came is stopped: its step is
/// `stopped`, its objects removed, and no container runs after Stop all.
#[test]
fn a_launch_racing_stop_all_is_stopped() {
    let Some(home) = a_home() else {
        return;
    };
    home.set_engine("start_latency_ms", 3_000.into());
    let runtime = home.seeded();
    let card = home.issue(&runtime);
    let context = FollowContext {
        runtime: runtime.clone(),
        collector: home.sender().clone(),
        home: home.env.officina_home(),
        base_env: common::package_run::base_env(),
    };
    let from = home.engine.log().len();
    let (launched, answer) = std::thread::scope(|scope| {
        let launching = scope.spawn(|| home.launch(&card, &runtime));
        until("the gateway's start", || {
            home.calls(from).iter().any(|argv| argv[0] == "start")
        });
        let answer = stop_workshop(&context, None).expect("an answer");
        (launching.join().unwrap(), answer)
    });
    home.settle();
    assert!(answer.within_budget(), "{answer:?}");
    assert_eq!(launched, LaunchAnswer::Launched(LaunchOutcome::Failed));
    assert!(
        running(&home.engine).is_empty(),
        "{:?}",
        running(&home.engine)
    );
    assert_eq!(home.count(from, &["start"]), 1, "the worker never started");
    let started = journal(&home)
        .into_iter()
        .find_map(|e| match e {
            JournalEntry::WorkshopStarted(line) if line.run_id == card.id => Some(line),
            _ => None,
        })
        .expect("workshop_started");
    let last = started.steps.last().unwrap();
    assert_eq!(last.outcome, StepOutcome::Stopped, "{last:?}");
    assert_eq!(last.detail.as_deref(), Some("workshop_stopped"));
    assert_eq!(home.run(&card.id).state, RunState::Failed);
    assert!(
        names_of(&home.engine, &card.id).is_empty(),
        "objects removed"
    );
}

/// AD-116: while the workshop's flag is set nothing launches, builds,
/// runs a battery or collects.
#[test]
fn nothing_launches_builds_or_collects_while_set() {
    let Some(home) = a_home() else {
        return;
    };
    let run = a_running(&home);
    stop(&run);
    let from = home.engine.log().len();
    assert_eq!(
        home.build_answer(),
        BuildAnswer::Refused(BuildRefusal::WorkshopStopped)
    );
    assert_eq!(
        home.battery_answer(),
        BatteryAnswer::Refused(BatteryRefusal::WorkshopStopped)
    );
    let card = home.issue(&run.runtime);
    assert_eq!(
        home.launch(&card, &run.runtime),
        LaunchAnswer::Refused(LaunchRefusal::WorkshopStopped)
    );
    std::thread::sleep(Duration::from_millis(1500));
    home.settle();
    assert_eq!(run.row().state, RunState::Stopped, "not collected");
    let packer = format!("officina-pack-{}", run.id);
    for argv in home.calls(from) {
        assert!(
            !matches!(argv[0].as_str(), "create" | "start" | "build" | "cp"),
            "{argv:?}"
        );
        assert!(!argv.iter().any(|a| a == packer.as_str()), "{argv:?}");
    }
}

/// AD-116: a follower keeps reading while the workshop is stopped: the
/// worker's exit is seen and recorded.
#[test]
fn followers_keep_reading_while_stopped() {
    let Some(home) = a_home() else {
        return;
    };
    let run = a_running(&home);
    officina_host::relay::home::write_stop_flag(&workshop_flag(&home), StopBy::Window, 1).unwrap();
    run.exit(3);
    until("the exit read", || run.row().state == RunState::Collecting);
    let row = run.row();
    assert_eq!(row.exit_code, Some(3));
    assert!(
        run.runtime.followed.lock().unwrap().contains_key(&run.id),
        "still followed"
    );
}

/// AD-116: Import and Discard start nothing, so both stay possible while
/// the workshop is stopped.
#[test]
fn import_and_discard_stay_possible_while_stopped() {
    for import in [true, false] {
        let Some(home) = a_home() else {
            return;
        };
        let run = launch(&home, home.seeded());
        let clone = run.clone();
        run.commit(&clone, "src/lib.rs", "pub fn bumped() {}\n");
        run.serve(&run.packed_branch(&clone), 0);
        run.exit(0);
        assert_eq!(run.collected().state, RunState::Review);
        stop(&run);
        assert!(home.env.relay_home().stop_flags().workshop);
        if import {
            run.approve_all();
            let answer = run.import();
            assert!(
                matches!(
                    answer,
                    officina_core::workshop::review::ImportAnswer::Imported(_)
                ),
                "{answer:?}"
            );
            assert_eq!(run.row().state, RunState::Imported);
        } else {
            run.discard(DiscardReason::NotNeeded);
            assert_eq!(run.row().state, RunState::Discarded);
        }
    }
}

/// AD-116: a run Stop all caught while collecting becomes `stopped` and
/// is collected again at Resume, `out/` emptied first.
#[test]
fn a_collecting_run_is_collected_again_at_resume() {
    let Some(home) = a_home() else {
        return;
    };
    let run = a_running(&home);
    let clone = run.clone();
    run.commit(&clone, "src/lib.rs", "pub fn bumped() {}\n");
    run.serve(&run.packed_branch(&clone), 0);
    // The flag before the exit holds the run in `collecting`, as a stop
    // in the middle of its collection would.
    officina_host::relay::home::write_stop_flag(&workshop_flag(&home), StopBy::Window, 1).unwrap();
    run.exit(0);
    until("collecting", || run.row().state == RunState::Collecting);
    let out = run.run_dir().join("out");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("partial.bin"), b"left by the stopped collection").unwrap();
    stop(&run);
    assert_eq!(run.row().state, RunState::Stopped);
    let answer = resume_workshop(&run.context(), &[], true).expect("an answer");
    assert_eq!(answer.collected, 1);
    let row = run.collected();
    assert_eq!(row.state, RunState::Review);
    let left: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(left, ["work.bundle"]);
}

// AC 4: Resume.

/// AD-116: Resume collects each kept run unless told otherwise, deletes
/// both flags and journals `workshop_resumed`.
#[test]
fn resume_collects_by_default() {
    let Some(home) = a_home() else {
        return;
    };
    let run = a_running(&home);
    let clone = run.clone();
    run.commit(&clone, "src/lib.rs", "pub fn bumped() {}\n");
    run.serve(&run.packed_branch(&clone), 0);
    stop(&run);
    let answer = resume_workshop(&run.context(), &[], true).expect("an answer");
    assert_eq!((answer.collected, answer.removed), (1, 0));
    assert!(answer.relay_resumed);
    assert!(!run.runtime.thread.latched());
    let flags = home.env.relay_home().stop_flags();
    assert!(!flags.relay && !flags.workshop);
    assert_eq!(run.collected().state, RunState::Review);
    let entries = journal(&home);
    assert!(entries.iter().any(|e| matches!(
        e,
        JournalEntry::WorkshopResumed(line) if line.collected == 1 && line.relay_resumed
    )));
    assert!(entries.contains(&JournalEntry::RelayResumed));
}

/// AD-116: a kept run the author chose to remove loses its objects and
/// becomes `discarded`, reason and outcome `not_collected`.
#[test]
fn resume_removes_what_the_author_chose() {
    let Some(home) = a_home() else {
        return;
    };
    let run = a_running(&home);
    stop(&run);
    let choice = ResumeChoice {
        run_id: run.id.clone(),
        choice: KeptChoice::Remove,
    };
    let answer = resume_workshop(&run.context(), &[choice], true).expect("an answer");
    assert_eq!((answer.collected, answer.removed), (0, 1));
    until("the removal", || run.row().state == RunState::Discarded);
    home.settle();
    let row = run.row();
    assert_eq!(row.discard_reason, Some(DiscardReason::NotCollected));
    assert_eq!(row.outcome, Some(WorkshopOutcome::NotCollected));
    assert!(row.removed_at_ms.is_some());
    assert!(names_of(&home.engine, &run.id).is_empty());
    let entries = journal(&home);
    assert_eq!(removed_causes(&entries, &run.id), [RemovalCause::Resume]);
    assert_eq!(
        finished_outcomes(&entries, &run.id),
        [WorkshopOutcome::NotCollected]
    );
}

/// AD-116, AD-34: Resume can leave the relay stopped; its own toggle
/// stays.
#[test]
fn resume_can_leave_the_relay_stopped() {
    let Some(home) = a_home() else {
        return;
    };
    let run = a_running(&home);
    stop(&run);
    let answer = resume_workshop(&run.context(), &[], false).expect("an answer");
    assert!(!answer.relay_resumed);
    let flags = home.env.relay_home().stop_flags();
    assert!(flags.relay, "the relay stays stopped");
    assert!(!flags.workshop);
}

// AC 5: kept containers.

fn eight_days_on() -> i64 {
    officina_host::files::now_ms() + KEPT_TTL_MS + 60 * 60 * 1000
}

/// AD-116, D-213: a kept run's objects go 7 days after its stop, even
/// while stopped: `workshop_removed` cause `kept_expired`, `discarded`
/// reason `not_collected`; a younger one is left.
#[test]
fn kept_containers_are_removed_after_7_days() {
    let Some(home) = a_home() else {
        return;
    };
    let run = a_running(&home);
    stop(&run);
    assert!(expire_kept(&run.context()).is_empty(), "too young");
    assert_eq!(run.row().state, RunState::Stopped);
    let mut later = run.context();
    later.runtime.now_ms = eight_days_on;
    assert_eq!(expire_kept(&later), std::slice::from_ref(&run.id));
    home.settle();
    let row = run.row();
    assert_eq!(row.state, RunState::Discarded);
    assert_eq!(row.discard_reason, Some(DiscardReason::NotCollected));
    assert!(names_of(&home.engine, &run.id).is_empty());
    assert!(home.env.relay_home().stop_flags().workshop, "still stopped");
    assert_eq!(
        removed_causes(&journal(&home), &run.id),
        [RemovalCause::KeptExpired]
    );
}

/// D-213: an engine that does not answer leaves the removal for the next
/// pass and the row unchanged.
#[test]
fn an_engine_down_delays_the_removal() {
    let Some(home) = a_home() else {
        return;
    };
    let run = a_running(&home);
    stop(&run);
    let mut later = run.context();
    later.runtime.now_ms = eight_days_on;
    home.set_engine("fail_when", serde_json::json!({"prefix": "ps"}));
    assert!(expire_kept(&later).is_empty());
    home.settle();
    assert_eq!(run.row().state, RunState::Stopped);
    assert_eq!(names_of(&home.engine, &run.id).len(), 2);
    home.set_engine("fail_when", serde_json::Value::Null);
    assert_eq!(expire_kept(&later), std::slice::from_ref(&run.id));
}

// AC 6: remove_run.

/// D-215: an id that is not 32 lowercase hex characters is refused before
/// anything reads the engine.
#[test]
fn remove_run_refuses_a_malformed_id() {
    let Some(home) = a_home() else {
        return;
    };
    let run = a_running(&home);
    let from = home.engine.log().len();
    for id in [
        "",
        "--privileged",
        "0123456789ABCDEF0123456789ABCDEF",
        "0123456789abcdef0123456789abcde",
        "0123456789abcdef0123456789abcdef0",
    ] {
        assert_eq!(
            remove_run(&run.context(), id).unwrap(),
            RemoveRunAnswer::Refused(RemoveRunRefusal::MalformedId),
            "{id:?}"
        );
    }
    assert_eq!(home.engine.log().len(), from, "the engine was not asked");
    assert_eq!(
        remove_run(&run.context(), &run.id).unwrap(),
        RemoveRunAnswer::Refused(RemoveRunRefusal::NotRemovable),
        "a live run is not removable"
    );
}

/// D-215: `remove_run` acts on the objects labelled with the run and the
/// window's floor only; the same run id on another floor is left.
#[test]
fn remove_run_never_touches_another_floor() {
    let Some(home) = a_home() else {
        return;
    };
    let run = a_running(&home);
    stop(&run);
    let path = home.engine.dir.path().join("fake-docker-state.json");
    let mut state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let elsewhere = serde_json::json!({
        "id": "f".repeat(64),
        "name": format!("officina-worker-elsewhere-{}", &run.id[..8]),
        "state": "exited",
        "labels": {"officina.managed": "1", "officina.floor": "another-floor", "officina.run": run.id},
    });
    state["containers"].as_array_mut().unwrap().push(elsewhere);
    std::fs::write(&path, state.to_string()).unwrap();
    let answer = remove_run(&run.context(), &run.id).unwrap();
    assert!(
        matches!(answer, RemoveRunAnswer::Removed(n) if n >= 2),
        "{answer:?}"
    );
    home.settle();
    assert_eq!(
        names_of(&home.engine, &run.id),
        [format!("officina-worker-elsewhere-{}", &run.id[..8])]
    );
    assert_eq!(run.row().state, RunState::Discarded);
}

// AC 3: `officina stop --all`.

fn managed(name: &str, floor: &str, state: &str) -> serde_json::Value {
    serde_json::json!({
        "id": format!("{name}-id"),
        "name": name,
        "state": state,
        "labels": {"officina.managed": "1", "officina.floor": floor, "officina.run": "0123456789abcdef0123456789abcdef"},
    })
}

fn cli_engine() -> FakeEngine {
    FakeEngine::new(
        &serde_json::json!({}),
        &serde_json::json!({"containers": [
            managed("officina-worker-one", "floor-one", "running"),
            managed("officina-worker-two", "floor-two", "running"),
            managed("officina-worker-old", "floor-one", "exited"),
            {"id": "x", "name": "someone-elses", "state": "running", "labels": {}},
        ]}),
    )
}

fn stop_all(env: &TestEnv, engine: &FakeEngine) -> std::process::Output {
    let mut command = env.officina(&["stop", "--all"]);
    command.env("OFFICINA_DOCKER_BIN", &engine.exe);
    common::run_with(command, b"")
}

/// AD-116, D-215: the CLI writes both flags, stops every managed running
/// container of every floor and nothing else, and journals one line.
#[test]
fn cli_stop_writes_both_flags_and_stops_every_floor() {
    let env = TestEnv::new();
    let engine = cli_engine();
    let output = stop_all(&env, &engine);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(running(&engine), ["someone-elses"]);
    let home = env.relay_home();
    for path in [
        officina_host::workshop::stopped_file(&env.officina_home()),
        home.stopped_file(),
    ] {
        assert_eq!(
            flag(&path).and_then(|f| f.by),
            Some(StopBy::Cli),
            "{path:?}"
        );
    }
    let entries: Vec<JournalEntry> = env.journal().into_iter().map(|l| l.entry).collect();
    assert_eq!(
        entries,
        [JournalEntry::WorkshopStopped(WorkshopStopped {
            scope: StopScope::All,
            by: StopBy::Cli,
            containers: 2
        })]
    );
    let stops: Vec<serde_json::Value> = engine
        .log()
        .into_iter()
        .filter(|l| l["argv"][0] == "stop")
        .collect();
    assert_eq!(stops.len(), 1, "one stop with every name: {stops:?}");
}

/// AD-116: with the engine down the flags are still written, the exit is
/// 1 and the message says containers may still run.
#[test]
fn cli_stop_with_the_engine_down_exits_1_with_flags_written() {
    let env = TestEnv::new();
    let engine = FakeEngine::new(
        &serde_json::json!({"fail_when": {"prefix": "ps"}}),
        &serde_json::json!({}),
    );
    let output = stop_all(&env, &engine);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("may still run"), "{stderr}");
    assert!(env.relay_home().stop_flags().relay);
    assert!(env.relay_home().stop_flags().workshop);
    // No engine at all reads the same.
    let other = TestEnv::new();
    let mut command = other.officina(&["stop", "--all"]);
    command.env("OFFICINA_DOCKER_BIN", other.root.path().join("missing.exe"));
    let output = common::run_with(command, b"");
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(other.relay_home().stop_flags().workshop);
    // Usage: 64, never 2.
    let output = env.run(&["stop"], b"");
    assert_eq!(output.status.code(), Some(64));
}

/// AD-116: the CLI never removes a flag nor replaces one the window
/// wrote.
#[test]
fn cli_stop_never_removes_a_flag() {
    let env = TestEnv::new();
    let engine = cli_engine();
    let workshop = officina_host::workshop::stopped_file(&env.officina_home());
    officina_host::relay::home::write_stop_flag(&workshop, StopBy::Window, 5).unwrap();
    let before = std::fs::read(&workshop).unwrap();
    let started = Instant::now();
    for _ in 0..2 {
        assert_eq!(stop_all(&env, &engine).status.code(), Some(0));
    }
    assert!(started.elapsed() < Duration::from_secs(60));
    assert_eq!(std::fs::read(&workshop).unwrap(), before);
    let relay = flag(&env.relay_home().stopped_file()).unwrap();
    assert_eq!(relay.by, Some(StopBy::Cli));
    assert!(Path::new(OFFICINA).exists());
}
