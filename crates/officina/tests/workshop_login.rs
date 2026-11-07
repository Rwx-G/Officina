//! The login kind and the credential state end to end (story 6.26 AC 1, 3
//! to 5; D-605, D-607, D-608, D-1135, D-1240): Log in for the workshop
//! and Check login through the functions the window's commands call, on
//! the fake engine (D-223), and the state the journal folds, moved by a
//! run ending `auth_failed`, the probe and a replaced token.

mod common;

use std::sync::Arc;

use officina_core::orders::card::{LaunchAnswer, LaunchRefusal};
use officina_core::relay::journal::JournalEntry;
use officina_core::workshop::credential::{
    CheckLoginAnswer, CredentialCause, CredentialSignal, LoginRefusal, PrepareLoginAnswer,
};
use officina_core::workshop::floor::CredentialKind;
use officina_core::workshop::launch::{CredentialState, login_command, render_one_line};
use officina_core::workshop::run::Recorded;
use officina_host::relay::credential::{CredentialStore, Secret, StoreError};
use officina_host::workshop::launch::CredentialStateSource;
use officina_host::workshop::login::{check_login, prepare_login};

use common::package_run::{self, result, said, until};
use common::workshop::SeededStore;
use common::workshop_home::{Home, Options, SEEDED, changes};

fn login_options() -> Options {
    Options {
        auth: "login",
        ..Options::default()
    }
}

/// The engine calls since `from` that name the floor's login volume.
fn touching_the_volume(home: &Home, from: usize) -> Vec<Vec<String>> {
    let volume = officina_core::workshop::names::login_volume(&home.floor_key());
    home.calls(from)
        .into_iter()
        .filter(|argv| argv.iter().any(|a| a.contains(volume.as_str())))
        .collect()
}

/// How many auth probes the engine created since `from`.
fn probes(home: &Home, from: usize) -> usize {
    home.calls(from)
        .iter()
        .filter(|argv| {
            argv.first().map(String::as_str) == Some("create")
                && argv.iter().any(|a| a == "officina.role=auth_probe")
        })
        .count()
}

/// AC 1: the first click creates the login volume with its labels; a
/// second finds it and creates nothing.
#[test]
fn prepare_login_creates_the_labelled_volume_once() {
    let Some(home) = Home::start(&login_options()) else {
        return;
    };
    let runtime = home.seeded();
    let reader = home.engine_reader(home.journal_states(SeededStore(None)));
    let at = home.engine.log().len();
    assert!(matches!(
        prepare_login(&runtime.thread, &reader),
        PrepareLoginAnswer::Ready(_)
    ));
    assert!(matches!(
        prepare_login(&runtime.thread, &reader),
        PrepareLoginAnswer::Ready(_)
    ));
    let creates: Vec<Vec<String>> = home
        .calls(at)
        .into_iter()
        .filter(|argv| argv.starts_with(&["volume".to_owned(), "create".to_owned()]))
        .collect();
    assert_eq!(creates.len(), 1, "{creates:?}");
    let key = home.floor_key();
    assert_eq!(
        creates[0].last().map(String::as_str),
        Some(officina_core::workshop::names::login_volume(&key).as_str())
    );
    for (name, value) in officina_core::workshop::launch::login_labels(&key) {
        assert!(creates[0].contains(&format!("{name}={value}")), "{name}");
    }
}

/// AC 1: the command answered is the golden one of the vector file, for
/// this floor's key.
#[test]
fn prepare_login_returns_the_golden_command() {
    let Some(home) = Home::start(&login_options()) else {
        return;
    };
    let runtime = home.seeded();
    let reader = home.engine_reader(home.journal_states(SeededStore(None)));
    let PrepareLoginAnswer::Ready(command) = prepare_login(&runtime.thread, &reader) else {
        panic!("not ready");
    };
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/vectors/workshop.json")).unwrap();
    let golden: Vec<String> = vectors["vectors"]["login_command"]["direct"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            a.as_str()
                .unwrap()
                .replace("0123456789abcdef", &home.floor_key())
        })
        .collect();
    assert_eq!(command, render_one_line(&golden));
    assert_eq!(
        command,
        render_one_line(&login_command(
            &home.floor_key(),
            officina_core::workshop::image::WORKSHOP_BASE,
            &std::collections::BTreeMap::new()
        ))
    );
    assert!(!command.contains('\n'));
}

/// AC 1: Officina never reads, copies out or shows what is in the login
/// volume: the only calls naming it are its listing's creation, and the
/// probe's read-only mount.
#[test]
fn nothing_reads_the_login_volume() {
    let Some(home) = Home::start(&login_options()) else {
        return;
    };
    let runtime = home.seeded();
    let reader = home.engine_reader(home.journal_states(SeededStore(None)));
    let at = home.engine.log().len();
    prepare_login(&runtime.thread, &reader);
    check_login(&runtime.thread, &reader, true);
    for argv in touching_the_volume(&home, at) {
        let joined = argv.join(" ");
        assert!(argv.first().map(String::as_str) != Some("cp"), "{joined}");
        assert!(argv.first().map(String::as_str) != Some("run"), "{joined}");
        let volume_create = argv.starts_with(&["volume".to_owned(), "create".to_owned()]);
        let read_only_mount = argv.first().map(String::as_str) == Some("create")
            && argv.iter().any(|a| a.ends_with(",readonly"));
        assert!(volume_create || read_only_mount, "{joined}");
    }
}

/// AC 1, AC 5: both refuse while the workshop's stop flag is set, and
/// change nothing.
#[test]
fn prepare_login_refuses_while_stopped() {
    let Some(home) = Home::start(&login_options()) else {
        return;
    };
    let runtime = home.seeded();
    let reader = home.engine_reader(home.journal_states(SeededStore(None)));
    let stopped = officina_host::workshop::stopped_file(&home.env.officina_home());
    std::fs::write(&stopped, b"").unwrap();
    let at = home.engine.log().len();
    assert_eq!(
        prepare_login(&runtime.thread, &reader),
        PrepareLoginAnswer::Refused(LoginRefusal::WorkshopStopped)
    );
    assert_eq!(changes(&home, at), 0);
}

/// AC 5: Check login refuses while stopped, and runs no probe.
#[test]
fn check_login_refuses_while_stopped() {
    let Some(home) = Home::start(&login_options()) else {
        return;
    };
    home.seed_login_volume();
    let runtime = home.seeded();
    let reader = home.engine_reader(home.journal_states(SeededStore(None)));
    std::fs::write(
        officina_host::workshop::stopped_file(&home.env.officina_home()),
        b"",
    )
    .unwrap();
    let at = home.engine.log().len();
    assert_eq!(
        check_login(&runtime.thread, &reader, true),
        CheckLoginAnswer::Refused(LoginRefusal::WorkshopStopped)
    );
    assert_eq!(probes(&home, at), 0);
}

/// AC 5: the probe's answer carries `logged_in` and `auth_method` only;
/// whatever else its line held is not read.
#[test]
fn check_login_reads_only_logged_in_and_method() {
    let Some(home) = Home::start(&login_options()) else {
        return;
    };
    home.seed_login_volume();
    let runtime = home.seeded();
    let reader = home.engine_reader(home.journal_states(SeededStore(None)));
    let CheckLoginAnswer::Checked(check) = check_login(&runtime.thread, &reader, true) else {
        panic!("not checked");
    };
    assert!(check.logged_in);
    assert_eq!(check.auth_method.as_deref(), Some("claude.ai"));
    assert!(!check.unreadable);
    assert_eq!(check.state, CredentialState::Unchecked);
    let answer = serde_json::to_value(CheckLoginAnswer::Checked(check)).unwrap();
    let fields: Vec<&String> = answer["detail"].as_object().unwrap().keys().collect();
    assert_eq!(fields, ["auth_method", "logged_in", "state", "unreadable"]);
}

/// AC 5 (D-1135): a line that does not read is a failed probe: the login
/// reads `missing`, journaled with cause `probe_failed`.
#[test]
fn an_unreadable_probe_line_fails_closed() {
    let Some(home) = Home::start(&login_options()) else {
        return;
    };
    home.seed_login_volume();
    home.set_engine(
        "auth_probe_line",
        r#"{"logged_in":true,"auth_method":"claude.ai","email":"user01@example.com"}"#.into(),
    );
    let runtime = home.seeded();
    let reader = home.engine_reader(home.journal_states(SeededStore(None)));
    let CheckLoginAnswer::Checked(check) = check_login(&runtime.thread, &reader, true) else {
        panic!("not checked");
    };
    assert!(check.unreadable);
    assert!(!check.logged_in);
    assert_eq!(check.auth_method, None);
    assert_eq!(check.state, CredentialState::Missing);
    let journal = home.env.journal();
    assert!(journal.iter().any(|line| matches!(&line.entry,
        JournalEntry::WorkshopCredential(c) if c.cause == CredentialCause::ProbeFailed
            && c.state == CredentialState::Missing)));
    assert!(
        !journal
            .iter()
            .any(|line| line.to_line().contains("example.com")),
        "the probe's extra field reached the journal"
    );
}

/// A worker that fails on its credential: a `result` with status 401.
fn auth_failure() -> Vec<String> {
    vec![
        said("starting"),
        serde_json::json!({"type": "result", "subtype": "error_during_execution", "is_error": true, "api_error_status": 401, "session_id": "s"}).to_string(),
    ]
}

/// AC 3: a run of a `login` floor finishing `auth_failed` sets
/// `needs_login`, journaled; the next launch is refused
/// `credential_expired`, which the restarted window still reads.
#[test]
fn an_auth_failed_run_sets_needs_login() {
    let Some(home) = package_run::home(&login_options(), &auth_failure()) else {
        return;
    };
    home.seed_login_volume();
    let states = home.journal_states(SeededStore(None));
    let mut runtime = home.seeded();
    runtime.states = Arc::clone(&states) as Arc<dyn CredentialStateSource + Send + Sync>;
    let run = package_run::launch(&home, runtime);
    run.exit(1);
    let row = run.collected();
    assert_eq!(
        row.cause,
        Some(Recorded::Known(
            officina_core::workshop::run::FinishCause::AuthFailed
        ))
    );
    assert_eq!(
        states.state(&home.floor_key(), CredentialKind::Login),
        CredentialState::NeedsLogin
    );
    assert!(home.env.journal().iter().any(|line| matches!(&line.entry,
        JournalEntry::WorkshopCredential(c) if c.cause == CredentialCause::RunAuthFailed
            && c.state == CredentialState::NeedsLogin)));
}

/// AC 3: the state survives a restart: a new window folds the journal to
/// the same state, and the launch is refused `credential_expired`.
#[test]
fn the_state_survives_a_restart() {
    let Some(home) = Home::start(&login_options()) else {
        return;
    };
    home.seed_login_volume();
    home.journal_states(SeededStore(None)).signal(
        &home.floor_key(),
        CredentialKind::Login,
        CredentialSignal::RunAuthFailed,
    );
    let restarted = home.journal_states(SeededStore(None));
    assert_eq!(
        restarted.state(&home.floor_key(), CredentialKind::Login),
        CredentialState::NeedsLogin
    );
}

/// AC 3: an expired state refuses the launch `credential_expired`, and the
/// click creates nothing.
#[test]
fn an_expired_state_refuses_the_launch() {
    let Some(home) = Home::start(&login_options()) else {
        return;
    };
    home.seed_login_volume();
    let states = home.journal_states(SeededStore(None));
    states.signal(
        &home.floor_key(),
        CredentialKind::Login,
        CredentialSignal::RunAuthFailed,
    );
    let mut runtime = home.seeded();
    runtime.states = states as Arc<dyn CredentialStateSource + Send + Sync>;
    let card = home.issue(&runtime);
    let at = home.engine.log().len();
    assert_eq!(
        home.launch(&card, &runtime),
        LaunchAnswer::Refused(LaunchRefusal::CredentialExpired)
    );
    assert_eq!(changes(&home, at), 0);
}

/// A store whose entry was written at a fixed time, its value never read.
struct WrittenAt(i64);

impl CredentialStore for WrittenAt {
    fn read(&self, _name: &str) -> Result<Option<Secret>, StoreError> {
        Ok(Some(Secret::new(SEEDED.to_owned())))
    }
    fn last_written_ms(&self, _name: &str) -> Result<Option<i64>, StoreError> {
        Ok(Some(self.0))
    }
}

/// AC 3: a token written after `needs_new_credential` was set returns the
/// state to `unchecked`, journaled `credential_replaced`; one written
/// before leaves it.
#[test]
fn a_replaced_token_returns_to_unchecked() {
    let Some(home) = Home::start(&Options::default()) else {
        return;
    };
    let key = home.floor_key();
    let kind = CredentialKind::OauthToken;
    home.journal_states(SeededStore(None))
        .signal(&key, kind, CredentialSignal::RunAuthFailed);
    let load = |written: i64| {
        officina_host::workshop::credential::JournalStates::load(
            home.env.relay_home(),
            Arc::new(WrittenAt(written)),
            officina_host::workshop::credential::StatesRole::Window,
            common::now_ms,
        )
    };
    assert_eq!(
        load(0).state(&key, kind),
        CredentialState::NeedsNewCredential
    );
    assert_eq!(
        load(common::now_ms() + 60_000).state(&key, kind),
        CredentialState::Unchecked
    );
    assert!(home.env.journal().iter().any(|line| matches!(&line.entry,
        JournalEntry::WorkshopCredential(c) if c.cause == CredentialCause::CredentialReplaced)));
    assert_eq!(load(0).state(&key, kind), CredentialState::Unchecked);
}

/// AC 4: one probe per trigger: a Check login click, a run ending
/// `auth_failed`; never a second for one trigger.
#[test]
fn the_probe_runs_once_per_trigger() {
    let Some(home) = package_run::home(&login_options(), &auth_failure()) else {
        return;
    };
    home.seed_login_volume();
    let states = home.journal_states(SeededStore(None));
    let mut runtime = home.seeded();
    runtime.states = Arc::clone(&states) as Arc<dyn CredentialStateSource + Send + Sync>;
    let reader =
        home.engine_reader(Arc::clone(&states) as Arc<dyn CredentialStateSource + Send + Sync>);
    let at = home.engine.log().len();
    check_login(&runtime.thread, &reader, true);
    assert_eq!(probes(&home, at), 1, "one click, one probe");
    let at = home.engine.log().len();
    let run = package_run::launch(&home, runtime);
    run.exit(1);
    run.collected();
    until("the probe after the failed run", || probes(&home, at) >= 1);
    home.settle();
    assert_eq!(probes(&home, at), 1, "one failed run, one probe");
    // The probe answers logged in, as a revoked login does (D-1135): the
    // run's evidence stands.
    assert_eq!(
        states.state(&home.floor_key(), CredentialKind::Login),
        CredentialState::NeedsLogin
    );
}

/// AC 4: without a trigger nothing probes: a run that succeeds, then the
/// window's hourly pass (its only timer), create no probe.
#[test]
fn no_probe_runs_without_a_trigger() {
    let Some(home) = package_run::home(&login_options(), &[said("done"), result()]) else {
        return;
    };
    home.seed_login_volume();
    let states = home.journal_states(SeededStore(None));
    let mut runtime = home.seeded();
    runtime.states = states as Arc<dyn CredentialStateSource + Send + Sync>;
    let at = home.engine.log().len();
    let run = package_run::launch(&home, runtime);
    run.exit(0);
    run.collected();
    officina_host::workshop::stop::expire_kept_on_thread(&run.context());
    home.settle();
    assert_eq!(probes(&home, at), 0);
}
