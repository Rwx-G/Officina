//! A workshop card and its launch end to end (story 6.11 AC 1 to 5 and 9;
//! AD-105, AD-107, D-206, D-214, D-239, D-611): the window's collector on
//! a home whose floor declares the workshop, a project in a real git
//! repository (D-23), the images built and a battery passed through the
//! fake engine (D-223), a seeded credential from a test store (D-86), and
//! the card issued and launched through the functions the window's
//! commands call. Nothing is created before the click; after it, one
//! gateway and one worker, the credential's name in the worker's `create`
//! child alone, its value nowhere.

mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use officina_core::orders::card::{LaunchAnswer, LaunchRefusal};
use officina_core::registry::types::LaunchOutcome;
use officina_core::relay::journal::JournalEntry;
use officina_core::workshop::run::{LaunchStep, RemovalCause, RunState, StepOutcome};

use common::workshop_home::{Home, Options, SEEDED, changes, proxy_mode};

/// AD-107, FR-6: issuing a card reads; only the click changes the engine,
/// and then once per role.
#[test]
fn zero_creates_and_starts_before_the_click_and_one_of_each_after() {
    let Some(home) = Home::start(&Options::default()) else {
        return;
    };
    let before = home.engine.log().len();
    let runtime = home.seeded();
    let card = home.issue(&runtime);
    let facts = card.workshop.clone().expect("a workshop card");
    assert!(facts.readiness.is_empty(), "{:?}", facts.readiness);
    assert_eq!(changes(&home, before), 0, "nothing before the click");
    let at_click = home.engine.log().len();
    assert_eq!(
        home.launch(&card, &runtime),
        LaunchAnswer::Launched(LaunchOutcome::Started)
    );
    let calls = home.calls(at_click);
    let created: Vec<&Vec<String>> = calls.iter().filter(|a| a[0] == "create").collect();
    assert_eq!(created.len(), 2, "one gateway and one worker: {created:?}");
    assert!(created[0][2].starts_with("officina-gateway-"));
    assert!(created[1][2].starts_with("officina-worker-"));
    assert_eq!(home.count(at_click, &["start"]), 2);
    assert_eq!(home.count(at_click, &["network", "create"]), 2);
    assert_eq!(home.count(at_click, &["volume", "create"]), 1);
    assert_eq!(home.count(at_click, &["cp"]), 1);
    let run = home.run(&card.id);
    assert_eq!(run.state, RunState::Running);
    // needles_are_kept_for_a_running_run (AD-112, D-230): in the window's
    // memory while it follows the run.
    let followed = runtime.followed.lock().unwrap();
    assert!(!followed[&card.id].needles().unwrap().needles().is_empty());
    assert_eq!(
        Some(run.plan_sha256.clone()),
        facts.plan_sha256,
        "the plan shown is the one run"
    );
    assert_eq!(run.base_branch, "main");
}

/// AD-105, D-206: the credential's name is in the worker's `create`
/// child's environment and nowhere else; its value in no argument, log,
/// journal line or registry file.
#[test]
fn the_credential_reaches_only_the_worker_create() {
    let Some(home) = Home::start(&Options::default()) else {
        return;
    };
    let runtime = home.seeded();
    let card = home.issue(&runtime);
    let at_click = home.engine.log().len();
    home.launch(&card, &runtime);
    let log = home.engine.log();
    for line in &log[at_click..] {
        let argv = line["argv"].to_string();
        let names = line["env_names"].to_string();
        let worker_create = argv.contains("\"create\"") && argv.contains("officina-worker-");
        assert_eq!(
            names.contains("CLAUDE_CODE_OAUTH_TOKEN"),
            worker_create,
            "{argv}"
        );
        if worker_create {
            assert!(
                argv.contains("\"--env\",\"CLAUDE_CODE_OAUTH_TOKEN\""),
                "{argv}"
            );
        }
    }
    let mut haystacks =
        vec![std::fs::read(home.engine.dir.path().join("fake-docker.log")).unwrap()];
    for entry in walk(&home.env.officina_home()) {
        haystacks.push(std::fs::read(entry).unwrap_or_default());
    }
    for bytes in haystacks {
        assert!(
            !String::from_utf8_lossy(&bytes).contains(SEEDED),
            "the value reached a file"
        );
    }
}

fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else {
            found.push(path);
        }
    }
    found
}

/// D-604: the target read is the floor's own.
#[test]
fn two_floors_read_two_targets() {
    use officina_host::relay::credential::{CredentialStore, Secret, StoreError};
    #[derive(Debug, Default)]
    struct Recording(std::sync::Mutex<Vec<String>>);
    impl CredentialStore for Recording {
        fn read(&self, name: &str) -> Result<Option<Secret>, StoreError> {
            self.0.lock().unwrap().push(name.to_owned());
            Ok(Some(Secret::new(SEEDED.to_owned())))
        }
    }
    let Some(home) = Home::start(&Options::default()) else {
        return;
    };
    let recording = Arc::new(Recording::default());
    let mut runtime = home.seeded();
    runtime.store = Arc::clone(&recording) as Arc<dyn CredentialStore + Send + Sync>;
    let card = home.issue(&runtime);
    home.launch(&card, &runtime);
    let floor_path = home.env.officina_home().join("floor.toml");
    let key = officina_core::workshop::names::floor_key(&floor_path.to_string_lossy());
    let names = recording.0.lock().unwrap().clone();
    assert!(!names.is_empty());
    for name in &names {
        assert_eq!(
            format!("officina:{name}"),
            officina_core::workshop::names::credential_target(&key)
        );
    }
    assert_ne!(
        officina_core::workshop::names::credential_target(&key),
        officina_core::workshop::names::credential_target(
            &officina_core::workshop::names::floor_key("C:/path/to/other/floor.toml")
        )
    );
}

/// A `login` home whose engine holds the floor's login volume.
fn login_home() -> Option<Home> {
    let home = Home::start(&Options {
        auth: "login",
        ..Options::default()
    })?;
    home.seed_login_volume();
    Some(home)
}

/// The argument vectors of the `create` calls naming `prefix`.
fn creates(home: &Home, from: usize, prefix: &str) -> Vec<Vec<String>> {
    home.calls(from)
        .into_iter()
        .filter(|argv| {
            argv.first().map(String::as_str) == Some("create")
                && argv.iter().any(|a| a.starts_with(prefix))
        })
        .collect()
}

/// Story 6.26 AC 2 (D-606): a `login` worker mounts the floor's login
/// read-only, and the launch starts it.
#[test]
fn a_login_launch_mounts_the_login_read_only() {
    let Some(home) = login_home() else {
        return;
    };
    let key = home.floor_key();
    let runtime = home.seeded();
    let card = home.issue(&runtime);
    assert_eq!(card.workshop.clone().unwrap().readiness, vec![]);
    let at_click = home.engine.log().len();
    assert_eq!(
        home.launch(&card, &runtime),
        LaunchAnswer::Launched(LaunchOutcome::Started)
    );
    let worker = creates(&home, at_click, "officina-worker-");
    assert_eq!(worker.len(), 1);
    let mount = format!(
        "type=volume,source={},target={},readonly",
        officina_core::workshop::names::login_volume(&key).as_str(),
        officina_core::workshop::vector::LOGIN_DIR
    );
    assert!(worker[0].contains(&mount), "{:?}", worker[0]);
}

/// Story 6.26 AC 2 (D-603): a `login` worker carries no credential name,
/// in its arguments or in its `create` child's environment, and nothing is
/// read from the Credential Manager.
#[test]
fn a_login_launch_sets_no_credential_name() {
    let Some(home) = login_home() else {
        return;
    };
    let runtime = home.seeded();
    let card = home.issue(&runtime);
    let at_click = home.engine.log().len();
    home.launch(&card, &runtime);
    let names: Vec<&str> = officina_core::workshop::floor::CredentialKind::ALL
        .iter()
        .filter_map(|kind| officina_core::workshop::vector::credential_name(*kind))
        .collect();
    assert!(!names.is_empty());
    for line in &home.engine.log()[at_click..] {
        let text = line.to_string();
        for name in &names {
            assert!(!text.contains(name), "{name} in {text}");
        }
        assert!(!text.contains(common::workshop_home::SEEDED));
    }
}

/// Story 6.26 AC 2 (D-603, D-621): without its login volume a `login`
/// floor is `credential_missing`, and the click creates nothing.
#[test]
fn a_login_launch_without_its_volume_is_credential_missing() {
    let Some(home) = Home::start(&Options {
        auth: "login",
        ..Options::default()
    }) else {
        return;
    };
    let runtime = home.seeded();
    let card = home.issue(&runtime);
    let readiness = card.workshop.clone().unwrap().readiness;
    assert!(
        readiness
            .iter()
            .any(|r| r.refusal == LaunchRefusal::CredentialMissing && r.detail.is_none()),
        "{readiness:?}"
    );
    let at_click = home.engine.log().len();
    assert_eq!(
        home.launch(&card, &runtime),
        LaunchAnswer::Refused(LaunchRefusal::CredentialMissing)
    );
    assert_eq!(changes(&home, at_click), 0);
}

/// Story 6.26 AC 2 (D-606): a `login` run's gateway allows the token
/// refresh hosts, each from core's list.
#[test]
fn a_login_run_allows_the_refresh_host() {
    let Some(home) = login_home() else {
        return;
    };
    let runtime = home.seeded();
    let card = home.issue(&runtime);
    let at_click = home.engine.log().len();
    home.launch(&card, &runtime);
    let gateway = creates(&home, at_click, "officina-gateway-");
    assert_eq!(gateway.len(), 1);
    for host in officina_core::workshop::launch::LOGIN_HOSTS {
        assert!(
            gateway[0]
                .windows(2)
                .any(|pair| pair[0] == "--allow" && pair[1] == *host),
            "{host} not allowed: {:?}",
            gateway[0]
        );
    }
}

/// AD-107 steps: a failed worker `create` ends the launch `failed`,
/// removes what was created and records the step.
#[test]
fn a_failed_step_removes_what_was_created() {
    let Some(home) = Home::start(&Options::default()) else {
        return;
    };
    home.set_engine(
        "fail_when",
        serde_json::json!({"prefix": "create --name officina-worker-"}),
    );
    let runtime = home.seeded();
    let card = home.issue(&runtime);
    let at_click = home.engine.log().len();
    assert_eq!(
        home.launch(&card, &runtime),
        LaunchAnswer::Launched(LaunchOutcome::Failed)
    );
    assert_eq!(home.count(at_click, &["start"]), 0);
    assert_eq!(home.count(at_click, &["rm"]), 1);
    assert_eq!(home.count(at_click, &["network", "rm"]), 1);
    assert_eq!(home.count(at_click, &["volume", "rm"]), 1);
    let run = home.run(&card.id);
    assert_eq!(run.state, RunState::Failed);
    let journal = home.env.journal();
    let started = journal
        .iter()
        .find_map(|l| match &l.entry {
            JournalEntry::WorkshopStarted(s) => Some(s.clone()),
            _ => None,
        })
        .expect("a workshop_started line");
    let last = started.steps.last().unwrap();
    assert_eq!(
        (last.step, last.outcome),
        (LaunchStep::Worker, StepOutcome::Failed)
    );
    assert!(journal.iter().any(|l| matches!(&l.entry,
        JournalEntry::WorkshopRemoved(r) if r.cause == RemovalCause::LaunchFailed && r.run_id == card.id)));
    assert!(
        !officina_host::workshop::launch::run_dir(&home.env.officina_home(), &card.id).exists(),
        "the run's folder goes too"
    );
}

/// AD-107: the stop flag set between steps ends the launch before any
/// start, and what was created is removed.
#[test]
fn the_stop_flag_between_steps_ends_the_launch() {
    let Some(home) = Home::start(&Options::default()) else {
        return;
    };
    let stopped = officina_host::workshop::stopped_file(&home.env.officina_home());
    home.set_engine(
        "touch_when",
        serde_json::json!({"prefix": "volume create", "path": stopped}),
    );
    let runtime = home.seeded();
    let card = home.issue(&runtime);
    let at_click = home.engine.log().len();
    assert_eq!(
        home.launch(&card, &runtime),
        LaunchAnswer::Launched(LaunchOutcome::Failed)
    );
    assert_eq!(home.count(at_click, &["start"]), 0, "nothing starts");
    assert_eq!(home.count(at_click, &["rm"]), 1);
    let journal = home.env.journal();
    let started = journal
        .iter()
        .find_map(|l| match &l.entry {
            JournalEntry::WorkshopStarted(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(started.steps.last().unwrap().outcome, StepOutcome::Stopped);
}

/// D-214: a launch writes `workshop_started`, never `work_order_launched`.
#[test]
fn a_launch_writes_workshop_started_only() {
    let Some(home) = Home::start(&Options::default()) else {
        return;
    };
    let runtime = home.seeded();
    let card = home.issue(&runtime);
    home.launch(&card, &runtime);
    let journal = home.env.journal();
    assert_eq!(
        journal
            .iter()
            .filter(|l| matches!(&l.entry, JournalEntry::WorkshopStarted(s) if s.run_id == card.id))
            .count(),
        1
    );
    assert!(
        !journal
            .iter()
            .any(|l| matches!(&l.entry, JournalEntry::WorkOrderLaunched(_)))
    );
    let started = journal
        .iter()
        .find_map(|l| match &l.entry {
            JournalEntry::WorkshopStarted(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(started.outcome, LaunchOutcome::Started);
    assert!(
        !started.battery_id.is_empty(),
        "the battery the check relied on"
    );
    assert!(started.steps.iter().all(|s| s.outcome == StepOutcome::Ok));
}

/// FR-4: two clicks on one card launch it once.
#[test]
fn a_double_click_launches_once() {
    let Some(home) = Home::start(&Options::default()) else {
        return;
    };
    let runtime = home.seeded();
    let card = home.issue(&runtime);
    let at_click = home.engine.log().len();
    let first = home.launch(&card, &runtime);
    let second = home.launch(&card, &runtime);
    assert_eq!(first, LaunchAnswer::Launched(LaunchOutcome::Started));
    assert_eq!(second, LaunchAnswer::Refused(LaunchRefusal::Consumed));
    assert_eq!(home.count(at_click, &["create"]), 2);
}

/// D-611: the routes resolved at issue are the gateway's `--route`
/// arguments; a network change before the click is `plan_changed`; a PAC
/// that cannot be evaluated refuses `proxy_unresolved`.
#[test]
fn routes_follow_the_network_from_issue_to_launch() {
    let Some(home) = Home::start(&Options {
        upstream_proxy: "system",
        ..Options::default()
    }) else {
        return;
    };
    home.set_proxy(proxy_mode::STATIC);
    let runtime = home.seeded();
    let card = home.issue(&runtime);
    let at_click = home.engine.log().len();
    assert_eq!(
        home.launch(&card, &runtime),
        LaunchAnswer::Launched(LaunchOutcome::Started)
    );
    let gateway_create = home
        .calls(at_click)
        .into_iter()
        .find(|a| a[0] == "create" && a[2].starts_with("officina-gateway-"))
        .unwrap();
    assert!(
        gateway_create
            .iter()
            .any(|a| a.contains("proxy.example.com:8080")),
        "routes_resolved_at_issue_reach_the_gateway_args: {gateway_create:?}"
    );
    let moved = home.issue(&runtime);
    home.set_proxy(proxy_mode::OTHER_STATIC);
    assert_eq!(
        home.launch(&moved, &runtime),
        LaunchAnswer::Refused(LaunchRefusal::PlanChanged),
        "a_network_change_between_issue_and_launch_is_plan_changed"
    );
    home.set_proxy(proxy_mode::PAC_FAILS);
    let pac = home.issue(&runtime);
    assert!(
        pac.workshop
            .as_ref()
            .unwrap()
            .readiness
            .iter()
            .any(|r| r.refusal == LaunchRefusal::ProxyUnresolved)
    );
    assert_eq!(
        home.launch(&pac, &runtime),
        LaunchAnswer::Refused(LaunchRefusal::ProxyUnresolved),
        "an_unresolvable_pac_refuses_the_launch"
    );
}

/// D-80, D-163: a launch leaves the project's references, index and work
/// tree as they were, though a tracked file's time moved first.
#[test]
fn a_launch_leaves_the_project_repository_unchanged() {
    let Some(home) = Home::start(&Options::default()) else {
        return;
    };
    let readme = home.project().join("README.md");
    let file = std::fs::File::options().write(true).open(&readme).unwrap();
    file.set_modified(std::time::SystemTime::now() + Duration::from_secs(5))
        .unwrap();
    drop(file);
    let snapshot = || {
        let git = home.project().join(".git");
        let mut files: Vec<(PathBuf, Vec<u8>)> = walk(&git)
            .into_iter()
            .filter(|p| !p.starts_with(git.join("objects")))
            .map(|p| {
                let bytes = std::fs::read(&p).unwrap_or_default();
                (p, bytes)
            })
            .collect();
        files.push((readme.clone(), std::fs::read(&readme).unwrap()));
        files.sort();
        files
    };
    let before = snapshot();
    let config_before = common::fingerprint(&home.env.claude_config());
    let runtime = home.seeded();
    let card = home.issue(&runtime);
    assert_eq!(
        home.launch(&card, &runtime),
        LaunchAnswer::Launched(LaunchOutcome::Started)
    );
    assert_eq!(snapshot(), before);
    assert_eq!(
        common::fingerprint(&home.env.claude_config()),
        config_before,
        "no write under the Claude config dir (D-80)"
    );
}

/// AD-3, D-239: the collector's own share of a workshop card, its issue
/// and its grant, reaches no engine: the facts and the checks run on
/// thread `workshop`, a proxy resolution included.
#[test]
fn the_collector_never_spawns_docker_for_a_workshop_card() {
    use officina_host::watch::{AUTHOR_WAIT, AuthorCommand, ask};
    let Some(home) = Home::start(&Options {
        upstream_proxy: "system",
        ..Options::default()
    }) else {
        return;
    };
    home.set_proxy(proxy_mode::STATIC);
    let runtime = home.seeded();
    let card = home.issue(&runtime);
    let before = home.engine.log().len();
    let request = officina_core::orders::card::CardRequest {
        template: "bump".to_owned(),
        project: "api".to_owned(),
        values: vec![("package".to_owned(), "serde_json".to_owned())],
        subject: None,
    };
    let step = ask(
        home.sender(),
        AuthorCommand::IssueCard(request),
        AUTHOR_WAIT,
    )
    .unwrap()
    .into_issue_step()
    .unwrap();
    assert!(
        matches!(step, officina_host::orders::IssueStep::Workshop(_)),
        "a ticket, its facts not read yet: {step:?}"
    );
    let grant = ask(
        home.sender(),
        AuthorCommand::LaunchCard {
            card_id: card.id.clone(),
            prompt_sha256: card.prompt_sha256.clone(),
            plan_sha256: card.workshop.as_ref().and_then(|w| w.plan_sha256.clone()),
        },
        AUTHOR_WAIT,
    )
    .unwrap()
    .into_launch()
    .unwrap();
    assert!(matches!(
        grant,
        officina_host::orders::LaunchGrant::Workshop(_)
    ));
    assert_eq!(home.engine.log().len(), before, "the collector ran docker");
}
