//! The window's authority over the catalogue and the stop flag (AD-21,
//! AD-24, AD-34, D-8, D-15, D-26). Built only with `window-authority`, like
//! the window.

mod common;

use std::collections::BTreeMap;
use std::path::Path;

use common::TestHome;
use officina_core::ipc::IpcErrorCode;
use officina_core::quota::normalise_path;
use officina_core::relay::StopFlag;
use officina_core::relay::approvals::{
    ApprovalRequest, DecideRefusal, Decision, FileKind, KeyInputs, PENDING_MS, file_name,
    request_id, request_key,
};
use officina_core::relay::journal::{ApprovalEvent, CatalogueApproved, JournalEntry};
use officina_host::files::now_ms;
use officina_host::relay::approvals;
use officina_host::relay::authority::{
    AUTHORITY_MARKER, AuthorityError, approve_catalogue, decide_approval, set_relay_stopped,
};
use officina_host::relay::home::{ApprovedState, RelayHome, load_relay_floor};
use officina_host::relay::journal::{JournalLock, read_recent};

const FLOOR: &str = "schema = 2\n\n[[project]]\nname = \"lab\"\npath = \"Lab\"\nactions = [\"probe\"]\n\n[[project]]\nname = \"notes\"\npath = \"Notes\"\nautonomy = \"free\"\n\n[[action]]\nname = \"probe\"\nclass = \"free\"\nprogram = \"C:/path/to/tool.exe\"\nargs = [\"{host}\"]\n\n[action.params.host]\nvalues = [\"web01\"]\n";

/// The hash the window would display for the floor at `path`.
fn displayed(path: &Path) -> String {
    load_relay_floor(path).unwrap().hash
}

#[test]
fn approves_the_displayed_hash() {
    let home = TestHome::new();
    let relay = home.relay(None);
    home.write(&relay.default_floor(), FLOOR);
    let displayed = displayed(&relay.default_floor());

    let approved = approve_catalogue(&relay, &displayed).unwrap();
    assert_eq!(approved.hash, displayed);
    assert!(approved.approved_at_ms > 0);
    assert_eq!(
        relay.read_approved(),
        ApprovedState::Valid(approved.clone())
    );
    assert!(approved.canonical.actions.contains_key("probe"));
    assert!(
        !relay.relay_dir().read_dir().unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")),
        "atomic writes leave no temp file"
    );
}

#[test]
fn edited_floor_between_display_and_click_is_not_approved() {
    let home = TestHome::new();
    let relay = home.relay(None);
    home.write(&relay.default_floor(), FLOOR);
    let displayed = displayed(&relay.default_floor());
    home.write(
        &relay.default_floor(),
        &FLOOR.replace("C:/path/to/tool.exe", "C:/Windows/System32/cmd.exe"),
    );

    let error = approve_catalogue(&relay, &displayed).unwrap_err();
    assert!(matches!(error, AuthorityError::CatalogueChanged { .. }));
    assert!(error.to_string().starts_with(AUTHORITY_MARKER));
    assert_eq!(relay.read_approved(), ApprovedState::Missing);
    assert!(!relay.known_projects_file().exists());
}

#[test]
fn invalid_actions_do_not_block_approval() {
    let home = TestHome::new();
    let relay = home.relay(None);
    let with_broken = format!(
        "{FLOOR}\n[[action]]\nname = \"broken\"\nclass = \"free\"\nprogram = \"relative.exe\"\n"
    );
    home.write(&relay.default_floor(), &with_broken);
    let loaded = load_relay_floor(&relay.default_floor()).unwrap();
    assert!(!loaded.catalogue.issues.is_empty());

    let approved = approve_catalogue(&relay, &loaded.hash).unwrap();
    assert!(!approved.canonical.actions.contains_key("broken"));
    home.write(&relay.default_floor(), FLOOR);
    assert_eq!(
        displayed(&relay.default_floor()),
        approved.hash,
        "an invalid action is no part of the approved meaning"
    );
}

#[test]
fn approval_hashes_the_relay_floor_not_another() {
    let home = TestHome::new();
    let window_floor = home.root.join("window").join("officina.toml");
    let relay_floor = home.root.join("relay").join("officina.toml");
    home.write(&window_floor, FLOOR);
    home.write(
        &relay_floor,
        &FLOOR.replace("values = [\"web01\"]", "values = [\"web01\", \"db01\"]"),
    );
    let relay = home.relay(Some(relay_floor.clone()));

    let error = approve_catalogue(&relay, &displayed(&window_floor)).unwrap_err();
    assert!(matches!(error, AuthorityError::CatalogueChanged { .. }));
    assert_eq!(relay.read_approved(), ApprovedState::Missing);

    let approved = approve_catalogue(&relay, &displayed(&relay_floor)).unwrap();
    assert_eq!(
        approved.canonical.floor_path,
        normalise_path(&relay_floor.to_string_lossy())
    );

    // Without the override, the floor recorded in the approval is the relay's.
    let after = home.relay(None);
    assert_eq!(
        normalise_path(&after.relay_floor(&after.read_approved()).to_string_lossy()),
        normalise_path(&relay_floor.to_string_lossy())
    );
    assert!(approve_catalogue(&after, &approved.hash).is_ok());
}

#[test]
fn approved_floor_is_read_back_from_the_path_as_given() {
    let home = TestHome::new();
    let floor = home.root.join("Mixed").join("Case").join("Officina.toml");
    home.write(&floor, FLOOR);
    let relay = home.relay(Some(floor.clone()));
    let approved = approve_catalogue(&relay, &displayed(&floor)).unwrap();
    assert_eq!(
        approved.floor_file.as_deref(),
        Some(floor.to_string_lossy().as_ref())
    );

    let after = home.relay(None);
    let resolved = after.relay_floor(&after.read_approved());
    assert_eq!(
        resolved.as_os_str().as_encoded_bytes(),
        floor.as_os_str().as_encoded_bytes(),
        "byte-equal to the path read at approval, never the normalised one"
    );
    assert!(approve_catalogue(&after, &approved.hash).is_ok());
}

#[test]
fn approval_records_known_projects() {
    let home = TestHome::new();
    let relay = home.relay(None);
    home.write(&relay.default_floor(), FLOOR);
    let approved = approve_catalogue(&relay, &displayed(&relay.default_floor())).unwrap();

    let known = relay.read_known_projects().valid().cloned().unwrap();
    assert_eq!(known.approved_hash.as_deref(), Some(approved.hash.as_str()));
    let floor_dir = normalise_path(&relay.officina_home().to_string_lossy());
    assert_eq!(
        known.project_paths,
        [format!("{floor_dir}/lab"), format!("{floor_dir}/notes")],
        "every declared project, free ones included"
    );
}

fn approvals_journaled(relay: &RelayHome) -> Vec<CatalogueApproved> {
    let (lines, corrupt) = read_recent(relay, now_ms());
    assert_eq!(corrupt, 0);
    lines
        .into_iter()
        .map(|line| match line.entry {
            JournalEntry::CatalogueApproved(approved) => approved,
            other => panic!("unexpected line {other:?}"),
        })
        .collect()
}

#[test]
fn approval_is_journaled_once_with_previous_hash() {
    let home = TestHome::new();
    let relay = home.relay(None);
    home.write(&relay.default_floor(), FLOOR);
    let first = approve_catalogue(&relay, &displayed(&relay.default_floor())).unwrap();
    assert_eq!(
        approvals_journaled(&relay),
        [CatalogueApproved {
            hash: first.hash.clone(),
            previous_hash: None
        }]
    );

    home.write(
        &relay.default_floor(),
        &FLOOR.replace("values = [\"web01\"]", "values = [\"web01\", \"db01\"]"),
    );
    let second = approve_catalogue(&relay, &displayed(&relay.default_floor())).unwrap();
    let journaled = approvals_journaled(&relay);
    assert_eq!(journaled.len(), 2, "one line per approval");
    assert_eq!(
        journaled[1],
        CatalogueApproved {
            hash: second.hash,
            previous_hash: Some(first.hash)
        }
    );

    let refused = approve_catalogue(&relay, "stale");
    assert!(refused.is_err());
    assert_eq!(
        approvals_journaled(&relay).len(),
        2,
        "a refused approval writes no line"
    );
}

#[test]
fn a_locked_journal_approves_nothing() {
    let home = TestHome::new();
    let relay = home.relay(None);
    home.write(&relay.default_floor(), FLOOR);
    let held = JournalLock::acquire(&relay).unwrap();
    let error = approve_catalogue(&relay, &displayed(&relay.default_floor())).unwrap_err();
    assert!(matches!(error, AuthorityError::Journal(_)), "{error}");
    assert!(error.to_string().starts_with(AUTHORITY_MARKER));
    assert_eq!(relay.read_approved(), ApprovedState::Missing);
    assert!(!relay.known_projects_file().exists());
    drop(held);
}

#[test]
fn a_missing_floor_is_an_error_that_writes_nothing() {
    let home = TestHome::new();
    let relay = home.relay(None);
    let error = approve_catalogue(&relay, "00").unwrap_err();
    assert!(matches!(error, AuthorityError::Floor(_)));
    assert!(error.to_string().starts_with(AUTHORITY_MARKER));
    assert!(!relay.relay_dir().exists());
}

fn stop_lines(relay: &RelayHome) -> Vec<JournalEntry> {
    let (lines, corrupt) = read_recent(relay, now_ms());
    assert_eq!(corrupt, 0);
    lines
        .into_iter()
        .map(|line| line.entry)
        .filter(|entry| {
            matches!(
                entry,
                JournalEntry::RelayStopped | JournalEntry::RelayResumed
            )
        })
        .collect()
}

#[test]
fn stop_writes_the_flag_and_journals() {
    let home = TestHome::new();
    let relay = home.relay(None);
    let before = now_ms();
    assert!(set_relay_stopped(&relay, true).unwrap());
    assert!(relay.relay_stopped());
    let flag = StopFlag::parse(&std::fs::read(relay.stopped_file()).unwrap()).unwrap();
    assert!(flag.since_ms >= before && flag.since_ms <= now_ms());
    assert_eq!(stop_lines(&relay), [JournalEntry::RelayStopped]);
    assert!(
        !relay.relay_dir().read_dir().unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")),
        "an atomic write leaves no temp file"
    );
}

#[test]
fn resume_deletes_the_flag_and_journals() {
    let home = TestHome::new();
    let relay = home.relay(None);
    set_relay_stopped(&relay, true).unwrap();
    assert!(set_relay_stopped(&relay, false).unwrap());
    assert!(!relay.stopped_file().exists() && !relay.relay_stopped());
    assert_eq!(
        stop_lines(&relay),
        [JournalEntry::RelayStopped, JournalEntry::RelayResumed]
    );
    assert!(
        !set_relay_stopped(&relay, false).unwrap(),
        "resuming a running relay changes nothing"
    );
    assert_eq!(stop_lines(&relay).len(), 2);
}

#[test]
fn stopping_twice_journals_once() {
    let home = TestHome::new();
    let relay = home.relay(None);
    let first = std::thread::scope(|scope| {
        let calls: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| set_relay_stopped(&relay, true).unwrap()))
            .collect();
        calls
            .into_iter()
            .map(|c| c.join().unwrap())
            .collect::<Vec<bool>>()
    });
    assert_eq!(
        first.iter().filter(|changed| **changed).count(),
        1,
        "{first:?}"
    );
    let since = std::fs::read(relay.stopped_file()).unwrap();
    assert!(!set_relay_stopped(&relay, true).unwrap());
    assert_eq!(
        std::fs::read(relay.stopped_file()).unwrap(),
        since,
        "the time it was stopped stays"
    );
    assert_eq!(stop_lines(&relay), [JournalEntry::RelayStopped]);
}

#[test]
fn a_locked_journal_stops_nothing() {
    let home = TestHome::new();
    let relay = home.relay(None);
    let held = JournalLock::acquire(&relay).unwrap();
    let error = set_relay_stopped(&relay, true).unwrap_err();
    assert!(matches!(error, AuthorityError::Journal(_)), "{error}");
    assert!(error.to_string().starts_with(AUTHORITY_MARKER));
    assert!(!relay.relay_stopped());
    drop(held);
}

// ---- story 2.8: approving or rejecting a request (AD-33, AD-35, D-27) ----

const HUMAN_FLOOR: &str = "schema = 2\n\n[[project]]\nname = \"lab\"\npath = \"Lab\"\nactions = [\"restart\"]\n\n[[action]]\nname = \"restart\"\nclass = \"human\"\nprogram = \"C:/path/to/tool.exe\"\nargs = [\"{service}\"]\n\n[action.params.service]\nvalues = [\"web01\", \"db01\"]\n";

/// An approved relay with one `human` action.
fn human_relay(home: &TestHome) -> RelayHome {
    let relay = home.relay(None);
    home.write(&relay.default_floor(), HUMAN_FLOOR);
    approve_catalogue(&relay, &displayed(&relay.default_floor())).unwrap();
    relay
}

/// Writes the request `officina run restart service=<service>` would write,
/// created `ago` milliseconds ago, and returns its id and request.
fn write_request(relay: &RelayHome, service: &str, ago: i64) -> (String, ApprovalRequest) {
    let hash = relay.read_approved().approved().unwrap().hash.clone();
    let params = BTreeMap::from([("service".to_owned(), service.to_owned())]);
    let key = request_key(&KeyInputs {
        project: "lab",
        action: "restart",
        params: &params,
        catalogue_hash: &hash,
        check_run_id: None,
        tree: None,
    });
    let created_at_ms = now_ms() - ago;
    let id = request_id(&key, created_at_ms);
    let request = ApprovalRequest {
        request_key: key,
        created_at_ms,
        project: "lab".to_owned(),
        action: "restart".to_owned(),
        params,
        argv: vec!["C:/path/to/tool.exe".to_owned(), service.to_owned()],
        http: None,
        catalogue_hash: hash,
        session_id: None,
        check_run_id: None,
        tree: None,
    };
    approvals::create_request(relay, &id, &request).unwrap();
    (id, request)
}

fn decision_lines(relay: &RelayHome) -> Vec<JournalEntry> {
    let (lines, corrupt) = read_recent(relay, now_ms());
    assert_eq!(corrupt, 0);
    lines
        .into_iter()
        .map(|line| line.entry)
        .filter(|entry| {
            matches!(
                entry,
                JournalEntry::ApprovalGiven(_) | JournalEntry::ApprovalRejected(_)
            )
        })
        .collect()
}

fn decision_file(relay: &RelayHome, id: &str, kind: FileKind) -> Option<Decision> {
    let bytes = std::fs::read(relay.approvals_dir().join(file_name(id, kind))).ok()?;
    Some(serde_json::from_slice(&bytes).unwrap())
}

#[test]
fn decide_approves_a_pending_request() {
    let home = TestHome::new();
    let relay = human_relay(&home);
    let (id, request) = write_request(&relay, "web01", 60_000);
    decide_approval(&relay, &id, true).unwrap();
    let approved = decision_file(&relay, &id, FileKind::Approved).expect("approved.json");
    assert_eq!(approved.request_key, request.request_key);
    assert!((now_ms() - approved.decided_at_ms).abs() < 60_000);
    assert_eq!(
        decision_lines(&relay),
        [JournalEntry::ApprovalGiven(ApprovalEvent {
            approval_id: id.clone(),
            request_key: request.request_key.clone(),
            project: Some(request.project.clone()),
            action: Some(request.action.clone()),
            dry_run: None,
        })]
    );

    let (other, other_request) = write_request(&relay, "db01", 30_000);
    decide_approval(&relay, &other, false).unwrap();
    assert!(decision_file(&relay, &other, FileKind::Rejected).is_some());
    assert!(decision_file(&relay, &other, FileKind::Approved).is_none());
    assert_eq!(
        decision_lines(&relay)[1],
        JournalEntry::ApprovalRejected(ApprovalEvent {
            approval_id: other,
            request_key: other_request.request_key,
            project: Some(other_request.project),
            action: Some(other_request.action),
            dry_run: None,
        })
    );
}

#[test]
fn decide_refuses_unknown_decided_expired_and_stale_hash() {
    let home = TestHome::new();
    let relay = human_relay(&home);
    let refused = |id: &str| match decide_approval(&relay, id, true) {
        Err(AuthorityError::Decide(refusal)) => refusal,
        other => panic!("{id}: {other:?}"),
    };

    assert!(matches!(
        refused("0123456789abcdef-1"),
        DecideRefusal::Unknown(_)
    ));

    let (decided, _) = write_request(&relay, "web01", 60_000);
    decide_approval(&relay, &decided, false).unwrap();
    assert!(matches!(refused(&decided), DecideRefusal::Decided(_)));

    let (expired, _) = write_request(&relay, "db01", PENDING_MS + 60_000);
    assert!(matches!(refused(&expired), DecideRefusal::Expired(_)));

    let (stale, _) = write_request(&relay, "web01", 1_000);
    home.write(
        &relay.default_floor(),
        &HUMAN_FLOOR.replace("\"web01\", \"db01\"", "\"web01\", \"db01\", \"cache01\""),
    );
    approve_catalogue(&relay, &displayed(&relay.default_floor())).unwrap();
    let error = decide_approval(&relay, &stale, true).unwrap_err();
    assert!(error.to_string().starts_with(AUTHORITY_MARKER));
    assert!(
        matches!(&error, AuthorityError::Decide(DecideRefusal::Invalid { why, .. }) if why.contains("another catalogue")),
        "{error}"
    );
    assert_eq!(
        decision_lines(&relay).len(),
        1,
        "only the one decision taken leaves a line"
    );
    for id in [&expired, &stale] {
        assert!(decision_file(&relay, id, FileKind::Approved).is_none());
    }
}

#[test]
fn decide_refuses_a_forged_request() {
    let home = TestHome::new();
    let relay = human_relay(&home);
    let (real, request) = write_request(&relay, "db01", 60_000);
    // A harmless stored command under the key of another parameter set.
    let mut forged = request;
    forged
        .params
        .insert("service".to_owned(), "web01".to_owned());
    forged.argv = vec!["C:/Windows/System32/whoami.exe".to_owned()];
    forged.created_at_ms += 1;
    let id = request_id(&forged.request_key, forged.created_at_ms);
    approvals::create_request(&relay, &id, &forged).unwrap();
    let error = decide_approval(&relay, &id, true).unwrap_err();
    assert!(
        matches!(&error, AuthorityError::Decide(DecideRefusal::Invalid { why, .. }) if why.contains("key")),
        "{error}"
    );
    assert!(decision_file(&relay, &id, FileKind::Approved).is_none());
    decide_approval(&relay, &real, true).unwrap();
}

/// The anomalies the relay view and doctor read now (D-173).
fn anomalies_now(relay: &RelayHome) -> Vec<officina_core::relay::record::Anomaly> {
    let (lines, _) = read_recent(relay, now_ms());
    officina_host::relay::view::read_anomalies(
        relay,
        &lines,
        relay.read_approved().approved().map(|a| a.hash.as_str()),
        relay.relay_stopped(),
    )
}

/// AC 7 of story 2.14, the audits' forged approval: an approval file the
/// window did not write, for a request it never decided, is an anomaly
/// until the author acknowledges it; the window's own approval is none.
#[test]
fn a_foreign_approval_file_is_an_anomaly() {
    use officina_core::relay::record::AnomalyKind;
    use officina_host::relay::authority::acknowledge_anomaly;
    let home = TestHome::new();
    let relay = human_relay(&home);
    let (given, _) = write_request(&relay, "web01", 60_000);
    decide_approval(&relay, &given, true).unwrap();
    assert!(
        anomalies_now(&relay).is_empty(),
        "the window's own approval"
    );

    let (forged_id, forged) = write_request(&relay, "db01", 30_000);
    std::fs::write(
        relay
            .approvals_dir()
            .join(file_name(&forged_id, FileKind::Approved)),
        serde_json::to_vec(&Decision {
            decided_at_ms: now_ms(),
            request_key: forged.request_key,
        })
        .unwrap(),
    )
    .unwrap();
    let found = anomalies_now(&relay);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].kind, AnomalyKind::ForeignApproval);
    assert!(found[0].detail.contains(&forged_id));
    assert!(!found[0].acknowledged);

    acknowledge_anomaly(&relay, &found[0].key).unwrap();
    assert!(
        anomalies_now(&relay)[0].acknowledged,
        "listed, acknowledged"
    );
}

/// AC 7 of story 2.14: a run journaled with an approval the window never
/// gave is an anomaly; so are a catalogue the window did not approve and a
/// stop flag gone while the window holds the relay stopped.
#[test]
fn a_run_without_a_window_decision_is_an_anomaly() {
    use officina_core::relay::journal::RunStarted;
    use officina_core::relay::record::AnomalyKind;
    let home = TestHome::new();
    let relay = human_relay(&home);
    let start = |approval_id: &str| {
        JournalEntry::RunStarted(RunStarted {
            run_id: format!("run-{approval_id}"),
            project: "lab".to_owned(),
            session_id: None,
            action: "restart".to_owned(),
            class: officina_core::relay::catalogue::ActionClass::Human,
            params: BTreeMap::from([("service".to_owned(), "web01".to_owned())]),
            argv: vec!["C:/path/to/tool.exe".to_owned(), "web01".to_owned()],
            method: None,
            url: None,
            credential: None,
            catalogue_hash: "00".to_owned(),
            approval_id: Some(approval_id.to_owned()),
            tree: None,
            timeout_s: Some(60),
        })
    };
    let (given, _) = write_request(&relay, "web01", 60_000);
    decide_approval(&relay, &given, true).unwrap();
    officina_host::relay::journal::append(&relay, &[start(&given)], now_ms()).unwrap();
    assert!(
        anomalies_now(&relay).is_empty(),
        "a run the window approved"
    );

    officina_host::relay::journal::append(&relay, &[start("0123456789abcdef-9")], now_ms())
        .unwrap();
    let kinds = |found: &[officina_core::relay::record::Anomaly]| {
        found.iter().map(|a| a.kind).collect::<Vec<_>>()
    };
    assert_eq!(
        kinds(&anomalies_now(&relay)),
        [AnomalyKind::RunWithoutDecision]
    );

    set_relay_stopped(&relay, true).unwrap();
    std::fs::remove_file(relay.stopped_file()).unwrap();
    let mut forged = relay.read_approved().approved().unwrap().clone();
    forged.hash = "f".repeat(64);
    std::fs::write(relay.approved_file(), serde_json::to_vec(&forged).unwrap()).unwrap();
    let mut found = kinds(&anomalies_now(&relay));
    found.sort_by_key(|k| AnomalyKind::ALL.iter().position(|a| a == k));
    assert_eq!(
        found,
        [
            AnomalyKind::RunWithoutDecision,
            AnomalyKind::ForeignCatalogue,
            AnomalyKind::StopFlagRemoved
        ]
    );
}

/// Story 2.15 AC 7: an authority error reaches the window as one IPC error,
/// its kind as the code the window branches on, its sentence as the
/// message it shows.
#[test]
fn an_authority_error_reaches_the_window_with_its_code() {
    let home = TestHome::new();
    let relay = home.relay(None);
    home.write(&relay.default_floor(), FLOOR);
    let stale = approve_catalogue(&relay, &"0".repeat(64)).unwrap_err();
    let ipc = stale.ipc();
    assert_eq!(ipc.code, IpcErrorCode::CatalogueChanged);
    assert_eq!(ipc.message, stale.to_string());
    let unknown = decide_approval(&relay, "0123456789abcdef-1", true).unwrap_err();
    assert_eq!(unknown.ipc().code, IpcErrorCode::Refused);
}
