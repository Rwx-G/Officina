//! The window's authority over the catalogue and the stop flag (AD-21,
//! AD-24, AD-34, D-8, D-15, D-26). Built only with `window-authority`, like
//! the window.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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

struct Home {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl Home {
    fn new() -> Home {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        Home { _dir: dir, root }
    }

    fn relay(&self, floor_override: Option<PathBuf>) -> RelayHome {
        RelayHome::new(self.root.join("officina-home"), floor_override)
    }

    fn write(&self, path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    /// The hash the window would display for the floor at `path`.
    fn displayed(&self, path: &Path) -> String {
        load_relay_floor(path).unwrap().hash
    }
}

#[test]
fn approves_the_displayed_hash() {
    let home = Home::new();
    let relay = home.relay(None);
    home.write(&relay.default_floor(), FLOOR);
    let displayed = home.displayed(&relay.default_floor());

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
    let home = Home::new();
    let relay = home.relay(None);
    home.write(&relay.default_floor(), FLOOR);
    let displayed = home.displayed(&relay.default_floor());
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
    let home = Home::new();
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
        home.displayed(&relay.default_floor()),
        approved.hash,
        "an invalid action is no part of the approved meaning"
    );
}

#[test]
fn approval_hashes_the_relay_floor_not_another() {
    let home = Home::new();
    let window_floor = home.root.join("window").join("officina.toml");
    let relay_floor = home.root.join("relay").join("officina.toml");
    home.write(&window_floor, FLOOR);
    home.write(
        &relay_floor,
        &FLOOR.replace("values = [\"web01\"]", "values = [\"web01\", \"db01\"]"),
    );
    let relay = home.relay(Some(relay_floor.clone()));

    let error = approve_catalogue(&relay, &home.displayed(&window_floor)).unwrap_err();
    assert!(matches!(error, AuthorityError::CatalogueChanged { .. }));
    assert_eq!(relay.read_approved(), ApprovedState::Missing);

    let approved = approve_catalogue(&relay, &home.displayed(&relay_floor)).unwrap();
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
    let home = Home::new();
    let floor = home.root.join("Mixed").join("Case").join("Officina.toml");
    home.write(&floor, FLOOR);
    let relay = home.relay(Some(floor.clone()));
    let approved = approve_catalogue(&relay, &home.displayed(&floor)).unwrap();
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
    let home = Home::new();
    let relay = home.relay(None);
    home.write(&relay.default_floor(), FLOOR);
    let approved = approve_catalogue(&relay, &home.displayed(&relay.default_floor())).unwrap();

    let known = relay.read_known_projects().unwrap();
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
    let home = Home::new();
    let relay = home.relay(None);
    home.write(&relay.default_floor(), FLOOR);
    let first = approve_catalogue(&relay, &home.displayed(&relay.default_floor())).unwrap();
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
    let second = approve_catalogue(&relay, &home.displayed(&relay.default_floor())).unwrap();
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
    let home = Home::new();
    let relay = home.relay(None);
    home.write(&relay.default_floor(), FLOOR);
    let held = JournalLock::acquire(&relay).unwrap();
    let error = approve_catalogue(&relay, &home.displayed(&relay.default_floor())).unwrap_err();
    assert!(matches!(error, AuthorityError::Journal(_)), "{error}");
    assert!(error.to_string().starts_with(AUTHORITY_MARKER));
    assert_eq!(relay.read_approved(), ApprovedState::Missing);
    assert!(!relay.known_projects_file().exists());
    drop(held);
}

#[test]
fn a_missing_floor_is_an_error_that_writes_nothing() {
    let home = Home::new();
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
    let home = Home::new();
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
    let home = Home::new();
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
    let home = Home::new();
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
    let home = Home::new();
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
fn human_relay(home: &Home) -> RelayHome {
    let relay = home.relay(None);
    home.write(&relay.default_floor(), HUMAN_FLOOR);
    approve_catalogue(&relay, &home.displayed(&relay.default_floor())).unwrap();
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
    let home = Home::new();
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
        })
    );
}

#[test]
fn decide_refuses_unknown_decided_expired_and_stale_hash() {
    let home = Home::new();
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
    approve_catalogue(&relay, &home.displayed(&relay.default_floor())).unwrap();
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
    let home = Home::new();
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
