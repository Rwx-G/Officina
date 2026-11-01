//! The window's authority over the catalogue and the stop flag (AD-21,
//! AD-24, AD-34, D-8, D-15, D-26). Built only with `window-authority`, like
//! the window.

mod common;

use std::collections::BTreeMap;
use std::path::Path;

use common::TestHome;
use officina_core::ipc::IpcErrorCode;
use officina_core::quota::normalise_path;
use officina_core::registry::types::{AuthorReason, ReviewOutcome};
use officina_core::relay::StopFlag;
use officina_core::relay::approvals::{
    ApprovalBy, ApprovalRequest, DecideRefusal, Decision, FileKind, KeyInputs, PENDING_MS,
    file_name, request_id, request_key,
};
use officina_core::relay::auto::{ReviewDecision, ReviewRecord};
use officina_core::relay::catalogue::DEFAULT_AUTO_MAX_PER_HOUR;
use officina_core::relay::journal::{ApprovalEvent, CatalogueApproved, JournalEntry};
use officina_host::files::now_ms;
use officina_host::relay::approvals;
use officina_host::relay::authority::{
    AUTHORITY_MARKER, AuthorityError, approve_catalogue, decide_approval, record_review_approval,
    set_relay_stopped,
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
        caller: Vec::new(),
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
    decide_approval(&relay, &id, true, None).unwrap();
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
            by: Some(ApprovalBy::Author),
            reason: None,
        })]
    );

    let (other, other_request) = write_request(&relay, "db01", 30_000);
    decide_approval(&relay, &other, false, None).unwrap();
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
            by: Some(ApprovalBy::Author),
            reason: None,
        })
    );
}

// ---- story 5.12: `auto` requests, reasons, one decision (AD-86, D-157) ----

const AUTO_FLOOR: &str = "schema = 2\n\n[[project]]\nname = \"lab\"\npath = \"Lab\"\nactions = [\"restart\", \"check\", \"apply\"]\n\n[[action]]\nname = \"restart\"\nclass = \"human\"\nprogram = \"C:/path/to/tool.exe\"\nargs = [\"{service}\"]\n\n[action.params.service]\nvalues = [\"web01\", \"db01\"]\n\n[[action]]\nname = \"check\"\nclass = \"free\"\nprogram = \"C:/path/to/tool.exe\"\nargs = [\"--check\", \"{service}\"]\n\n[action.params.service]\nvalues = [\"web01\", \"db01\"]\n\n[[action]]\nname = \"apply\"\nclass = \"auto\"\ncheck = \"check\"\nprogram = \"C:/path/to/tool.exe\"\nargs = [\"--apply\", \"{service}\"]\n\n[action.params.service]\nvalues = [\"web01\", \"db01\"]\n";

/// An approved relay with a `human` action and an `auto` one.
fn auto_relay(home: &TestHome) -> RelayHome {
    let relay = home.relay(None);
    home.write(&relay.default_floor(), AUTO_FLOOR);
    approve_catalogue(&relay, &displayed(&relay.default_floor())).unwrap();
    relay
}

/// Writes the request `officina run apply service=<service>` would write
/// after its check, and returns its id.
fn write_auto_request(relay: &RelayHome, service: &str) -> String {
    let hash = relay.read_approved().approved().unwrap().hash.clone();
    let params = BTreeMap::from([("service".to_owned(), service.to_owned())]);
    let key = request_key(&KeyInputs {
        project: "lab",
        action: "apply",
        params: &params,
        catalogue_hash: &hash,
        check_run_id: Some("19a2b3c4d5e-1f"),
        tree: None,
    });
    let created_at_ms = now_ms() - 1_000;
    let id = request_id(&key, created_at_ms);
    let request = ApprovalRequest {
        request_key: key,
        created_at_ms,
        project: "lab".to_owned(),
        action: "apply".to_owned(),
        params,
        argv: vec![
            "C:/path/to/tool.exe".to_owned(),
            "--apply".to_owned(),
            service.to_owned(),
        ],
        http: None,
        catalogue_hash: hash,
        session_id: None,
        check_run_id: Some("19a2b3c4d5e-1f".to_owned()),
        tree: None,
        caller: Vec::new(),
    };
    approvals::create_request(relay, &id, &request).unwrap();
    id
}

fn refusal_of(result: Result<(), AuthorityError>) -> DecideRefusal {
    match result {
        Err(AuthorityError::Decide(refusal)) => refusal,
        other => panic!("{other:?}"),
    }
}

/// Story 5.12 AC 5 (FR-9, AD-86): rejecting an `auto` request needs a
/// reason, which reaches the rejection file and its journal line; nothing is
/// written without one.
#[test]
fn rejecting_auto_requires_a_reason() {
    let home = TestHome::new();
    let relay = auto_relay(&home);
    let id = write_auto_request(&relay, "web01");
    assert!(matches!(
        refusal_of(decide_approval(&relay, &id, false, None)),
        DecideRefusal::Reason { .. }
    ));
    assert!(decision_file(&relay, &id, FileKind::Rejected).is_none());
    assert!(
        !relay
            .approvals_dir()
            .join(file_name(&id, FileKind::Decided))
            .exists(),
        "a refused decision claims nothing"
    );
    decide_approval(&relay, &id, false, Some(AuthorReason::ShouldNotRun)).unwrap();
    let rejected = decision_file(&relay, &id, FileKind::Rejected).unwrap();
    assert_eq!(rejected.reason, Some(AuthorReason::ShouldNotRun));
    assert_eq!(rejected.by, Some(ApprovalBy::Author));
    match decision_lines(&relay).as_slice() {
        [JournalEntry::ApprovalRejected(event)] => {
            assert_eq!(event.reason, Some(AuthorReason::ShouldNotRun));
            assert_eq!(event.by, Some(ApprovalBy::Author));
        }
        other => panic!("{other:?}"),
    }
}

/// Story 5.12 AC 5: a `human` rejection and any approval take no reason.
#[test]
fn a_human_rejection_takes_no_reason() {
    let home = TestHome::new();
    let relay = auto_relay(&home);
    let (human, _) = write_request(&relay, "web01", 1_000);
    assert!(matches!(
        refusal_of(decide_approval(
            &relay,
            &human,
            false,
            Some(AuthorReason::NotNow)
        )),
        DecideRefusal::Reason { .. }
    ));
    let auto = write_auto_request(&relay, "web01");
    assert!(matches!(
        refusal_of(decide_approval(
            &relay,
            &auto,
            true,
            Some(AuthorReason::NotNow)
        )),
        DecideRefusal::Reason { .. }
    ));
    decide_approval(&relay, &human, false, None).unwrap();
    assert_eq!(
        decision_file(&relay, &human, FileKind::Rejected)
            .unwrap()
            .reason,
        None
    );
}

/// Story 5.12 AC 2 and AC 5 (AD-86): the author's approval says so, in its
/// file and its journal line.
#[test]
fn an_author_approval_says_by_author() {
    let home = TestHome::new();
    let relay = auto_relay(&home);
    let id = write_auto_request(&relay, "db01");
    decide_approval(&relay, &id, true, None).unwrap();
    let approved = decision_file(&relay, &id, FileKind::Approved).unwrap();
    assert_eq!(
        (approved.by, approved.reason),
        (Some(ApprovalBy::Author), None)
    );
    match decision_lines(&relay).as_slice() {
        [JournalEntry::ApprovalGiven(event)] => assert_eq!(event.by, Some(ApprovalBy::Author)),
        other => panic!("{other:?}"),
    }
}

/// Story 5.12 AC 5 (D-157): a decision marker created first, as a review
/// approving does, refuses the author `already_decided` before anything is
/// written; so does a second click after a decision.
#[test]
fn a_second_decision_is_refused() {
    let home = TestHome::new();
    let relay = auto_relay(&home);
    let id = write_auto_request(&relay, "web01");
    assert!(
        approvals::claim_decision(&relay, &id).unwrap(),
        "the review first"
    );
    assert!(matches!(
        refusal_of(decide_approval(&relay, &id, true, None)),
        DecideRefusal::Decided(_)
    ));
    assert!(decision_file(&relay, &id, FileKind::Approved).is_none());
    assert!(decision_lines(&relay).is_empty());

    let other = write_auto_request(&relay, "db01");
    decide_approval(&relay, &other, true, None).unwrap();
    assert!(matches!(
        refusal_of(decide_approval(
            &relay,
            &other,
            false,
            Some(AuthorReason::NotNow)
        )),
        DecideRefusal::Decided(_)
    ));
    assert!(decision_file(&relay, &other, FileKind::Rejected).is_none());
}

/// Story 5.12 AC 5 (D-157): two decisions at once on one request write one
/// decision file and one journal line.
#[test]
fn two_concurrent_decisions_write_one() {
    let home = TestHome::new();
    let relay = auto_relay(&home);
    let id = write_auto_request(&relay, "web01");
    let results: Vec<bool> = std::thread::scope(|scope| {
        let approve = scope.spawn(|| decide_approval(&relay, &id, true, None).is_ok());
        let reject =
            scope.spawn(|| decide_approval(&relay, &id, false, Some(AuthorReason::NotNow)).is_ok());
        vec![approve.join().unwrap(), reject.join().unwrap()]
    });
    assert_eq!(results.iter().filter(|ok| **ok).count(), 1, "{results:?}");
    let written = [FileKind::Approved, FileKind::Rejected]
        .into_iter()
        .filter(|kind| decision_file(&relay, &id, *kind).is_some())
        .count();
    assert_eq!(written, 1);
    assert_eq!(decision_lines(&relay).len(), 1);
}

/// Story 5.12 AC 5 (D-157): while a live review decides a request, the
/// author cannot; once the review lets go, he can.
#[test]
fn a_reviewing_request_cannot_be_decided() {
    let home = TestHome::new();
    let relay = auto_relay(&home);
    let id = write_auto_request(&relay, "web01");
    approvals::set_reviewing(&relay, &id, true).unwrap();
    assert!(matches!(
        refusal_of(decide_approval(&relay, &id, true, None)),
        DecideRefusal::Reviewing(_)
    ));
    assert!(decision_file(&relay, &id, FileKind::Approved).is_none());
    approvals::set_reviewing(&relay, &id, false).unwrap();
    decide_approval(&relay, &id, true, None).unwrap();
}

// ---- story 5.14: approvals by a review or the rule (AD-86, D-141) ----

/// `n` review approvals of `lab`'s `apply` in the last hour.
fn approvals_this_hour(n: usize) -> Vec<ReviewRecord> {
    (0..n)
        .map(|_| ReviewRecord {
            project: "lab".into(),
            action: "apply".into(),
            requested_at_ms: now_ms() - 60_000,
            outcome: ReviewOutcome::Answered,
            decision: Some(ReviewDecision::Approve),
            call_started_at_ms: Some(now_ms() - 60_000),
        })
        .collect()
}

/// Story 5.14 AC 3 (AD-86): a review's approval of a request it is
/// reviewing writes `by = "review"` and its row's id, goes into the
/// window's record, and journals `approval_given` with `by`; the rule's
/// says `rule`. A `human` request is never a review's to approve.
#[test]
fn a_review_approval_says_by_review_with_its_id() {
    let home = TestHome::new();
    let relay = auto_relay(&home);
    let id = write_auto_request(&relay, "web01");
    approvals::set_reviewing(&relay, &id, true).unwrap();
    record_review_approval(&relay, &id, false, 42, &[]).unwrap();
    let approved = decision_file(&relay, &id, FileKind::Approved).unwrap();
    assert_eq!(
        (approved.by, approved.review_id.as_deref(), approved.reason),
        (Some(ApprovalBy::Review), Some("42"), None)
    );
    assert!(
        relay
            .read_record()
            .valid()
            .is_some_and(|record| record.decided.contains_key(&id)),
        "the window's record holds it, so `officina run` may consume it"
    );
    let rule = write_auto_request(&relay, "db01");
    record_review_approval(&relay, &rule, true, 43, &[]).unwrap();
    assert_eq!(
        decision_file(&relay, &rule, FileKind::Approved).unwrap().by,
        Some(ApprovalBy::Rule)
    );
    match decision_lines(&relay).as_slice() {
        [
            JournalEntry::ApprovalGiven(review),
            JournalEntry::ApprovalGiven(by_rule),
        ] => {
            assert_eq!(review.by, Some(ApprovalBy::Review));
            assert_eq!(by_rule.by, Some(ApprovalBy::Rule));
        }
        other => panic!("{other:?}"),
    }
    let (human, _) = write_request(&relay, "web01", 1_000);
    assert!(matches!(
        refusal_of(record_review_approval(&relay, &human, false, 44, &[])),
        DecideRefusal::NotAuto(_)
    ));
}

/// Story 5.14 AC 3 (D-157, review F26): an author decision already made
/// wins: the review's approval is refused `already_decided` and writes
/// nothing.
#[test]
fn a_review_approval_loses_to_an_author_decision() {
    let home = TestHome::new();
    let relay = auto_relay(&home);
    let id = write_auto_request(&relay, "web01");
    decide_approval(&relay, &id, false, Some(AuthorReason::NotNow)).unwrap();
    assert!(matches!(
        refusal_of(record_review_approval(&relay, &id, false, 42, &[])),
        DecideRefusal::Decided(_)
    ));
    assert!(decision_file(&relay, &id, FileKind::Approved).is_none());
    assert_eq!(decision_lines(&relay).len(), 1);
    let claimed = write_auto_request(&relay, "db01");
    assert!(
        approvals::claim_decision(&relay, &claimed).unwrap(),
        "the author first"
    );
    assert!(matches!(
        refusal_of(record_review_approval(&relay, &claimed, false, 43, &[])),
        DecideRefusal::Decided(_)
    ));
}

/// Story 5.14 AC 3 (D-142, review F15): the hourly cap is checked once
/// more right before a review's approval: a burst of approvals cannot pass
/// it, the default cap of the action counting.
#[test]
fn a_burst_cannot_approve_past_the_hourly_cap() {
    let home = TestHome::new();
    let relay = auto_relay(&home);
    let cap = usize::try_from(DEFAULT_AUTO_MAX_PER_HOUR).unwrap();
    let id = write_auto_request(&relay, "web01");
    assert!(matches!(
        refusal_of(record_review_approval(
            &relay,
            &id,
            false,
            42,
            &approvals_this_hour(cap)
        )),
        DecideRefusal::HourlyCap(_)
    ));
    assert!(decision_file(&relay, &id, FileKind::Approved).is_none());
    assert!(
        !relay
            .approvals_dir()
            .join(file_name(&id, FileKind::Decided))
            .exists(),
        "a refused approval claims nothing"
    );
    record_review_approval(&relay, &id, false, 42, &approvals_this_hour(cap - 1)).unwrap();
}

/// Story 5.14 AC 6 (D-27, AD-95): a catalogue approved again while the
/// review ran refuses its approval: the request no longer rebuilds.
#[test]
fn a_catalogue_change_during_a_review_refuses_its_approval() {
    let home = TestHome::new();
    let relay = auto_relay(&home);
    let id = write_auto_request(&relay, "web01");
    home.write(
        &relay.default_floor(),
        &AUTO_FLOOR.replace("\"web01\", \"db01\"", "\"web01\", \"db01\", \"cache01\""),
    );
    approve_catalogue(&relay, &displayed(&relay.default_floor())).unwrap();
    assert!(matches!(
        refusal_of(record_review_approval(&relay, &id, false, 42, &[])),
        DecideRefusal::Invalid { .. }
    ));
    assert!(decision_file(&relay, &id, FileKind::Approved).is_none());
}

#[test]
fn decide_refuses_unknown_decided_expired_and_stale_hash() {
    let home = TestHome::new();
    let relay = human_relay(&home);
    let refused = |id: &str| match decide_approval(&relay, id, true, None) {
        Err(AuthorityError::Decide(refusal)) => refusal,
        other => panic!("{id}: {other:?}"),
    };

    assert!(matches!(
        refused("0123456789abcdef-1"),
        DecideRefusal::Unknown(_)
    ));

    let (decided, _) = write_request(&relay, "web01", 60_000);
    decide_approval(&relay, &decided, false, None).unwrap();
    assert!(matches!(refused(&decided), DecideRefusal::Decided(_)));

    let (expired, _) = write_request(&relay, "db01", PENDING_MS + 60_000);
    assert!(matches!(refused(&expired), DecideRefusal::Expired(_)));

    let (stale, _) = write_request(&relay, "web01", 1_000);
    home.write(
        &relay.default_floor(),
        &HUMAN_FLOOR.replace("\"web01\", \"db01\"", "\"web01\", \"db01\", \"cache01\""),
    );
    approve_catalogue(&relay, &displayed(&relay.default_floor())).unwrap();
    let error = decide_approval(&relay, &stale, true, None).unwrap_err();
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
    let error = decide_approval(&relay, &id, true, None).unwrap_err();
    assert!(
        matches!(&error, AuthorityError::Decide(DecideRefusal::Invalid { why, .. }) if why.contains("key")),
        "{error}"
    );
    assert!(decision_file(&relay, &id, FileKind::Approved).is_none());
    decide_approval(&relay, &real, true, None).unwrap();
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
    decide_approval(&relay, &given, true, None).unwrap();
    assert!(
        anomalies_now(&relay).is_empty(),
        "the window's own approval"
    );

    let (forged_id, forged) = write_request(&relay, "db01", 30_000);
    std::fs::write(
        relay
            .approvals_dir()
            .join(file_name(&forged_id, FileKind::Approved)),
        serde_json::to_vec(&Decision::by_author(now_ms(), &forged.request_key, None)).unwrap(),
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
    decide_approval(&relay, &given, true, None).unwrap();
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
    let unknown = decide_approval(&relay, "0123456789abcdef-1", true, None).unwrap_err();
    assert_eq!(unknown.ipc().code, IpcErrorCode::Refused);
}

/// Story 5.4 AC 7 (AD-76's four facts, D-131): an `approved.json` written in
/// the previous canonical format parses with its stored hash; the gate keeps
/// applying that snapshot's guarded projects while the current floor, read
/// in the new format, differs; the pending diff shows the format change and
/// the templates added, if any; one approval by the author then holds the new
/// format.
#[test]
fn an_approval_in_the_previous_format_keeps_guarding_and_waits_for_one_approval() {
    use common::previous_format::{GOLDEN_FLOOR, PREVIOUS_APPROVAL, previous_approval_at};
    use officina_core::relay::catalogue::{CANONICAL_FORMAT, diff};
    use officina_core::relay::record::record_anomaly_key;
    use officina_host::relay::authority::acknowledge_anomaly;
    use officina_host::relay::gate::{ToolCall, evaluate};

    let home = TestHome::new();
    let relay = home.relay(None);
    // The supervision project, guarded by the previous approval, turned
    // free in the floor read today: only the snapshot still guards it.
    let current_floor = GOLDEN_FLOOR.replacen("autonomy = \"guarded\"", "autonomy = \"free\"", 1);
    assert_ne!(current_floor, GOLDEN_FLOOR);
    home.write(&relay.default_floor(), &current_floor);
    home.write(
        &relay.approved_file(),
        &previous_approval_at(&relay.default_floor()),
    );
    acknowledge_anomaly(&relay, &record_anomaly_key()).unwrap();
    let previous: serde_json::Value = serde_json::from_str(PREVIOUS_APPROVAL).unwrap();
    let on_disk: serde_json::Value =
        serde_json::from_slice(&std::fs::read(relay.approved_file()).unwrap()).unwrap();
    assert!(
        on_disk["canonical"].get("workshop").is_none() && on_disk.get("relay_hash").is_none(),
        "the file is in the previous format"
    );

    // It parses, and its stored hash is kept.
    let ApprovedState::Valid(approved) = relay.read_approved() else {
        panic!("the previous format reads");
    };
    assert_eq!(approved.hash, previous["hash"].as_str().unwrap());
    assert_eq!(approved.canonical.format, CANONICAL_FORMAT - 1);

    // Its hash differs from the floor's, read in the new format, so
    // `officina run` refuses as pending (CLI `run.rs`).
    let current = load_relay_floor(&relay.default_floor()).unwrap();
    assert_eq!(current.canonical.format, CANONICAL_FORMAT);
    assert_ne!(current.hash, approved.hash);

    // The gate keeps guarding the snapshot's project.
    let project = relay.officina_home().join("Supervision");
    std::fs::create_dir_all(&project).unwrap();
    let call = ToolCall {
        tool: "Bash".to_owned(),
        command: Some("ssh web01 uptime".to_owned()),
        target: None,
        cwd: Some(project.to_string_lossy().into_owned()),
        session_id: None,
    };
    let deny = evaluate(&relay, &call, None)
        .deny
        .expect("the snapshot's guarded project stays guarded");
    assert_eq!(deny.project, "supervision");

    // The pending diff shows the format change and the templates added:
    // here none, since this home holds neither the golden floor's CA file
    // nor its Dockerfile, so its workshop is off and its workshop template
    // dropped, with their issues.
    let pending = diff(&approved.canonical, &current.canonical);
    let format = pending.format.expect("the format changed");
    assert_eq!(format.before, CANONICAL_FORMAT - 1);
    assert_eq!(format.after, CANONICAL_FORMAT);
    let templates: Vec<&String> = current
        .canonical
        .templates
        .keys()
        .filter(|name| !approved.canonical.templates.contains_key(*name))
        .collect();
    assert_eq!(
        pending.templates.added.iter().collect::<Vec<_>>(),
        templates
    );
    assert!(current.canonical.workshop.is_none());
    assert!(!current.catalogue.issues.is_empty());

    // One approval of what the window shows; the gate then follows it.
    let renewed = approve_catalogue(&relay, &current.hash).unwrap();
    assert_eq!(renewed.canonical.format, CANONICAL_FORMAT);
    assert_eq!(renewed.hash, current.hash);
    assert!(
        evaluate(&relay, &call, None).deny.is_none(),
        "the project the author made free is free once approved"
    );
}

/// A routed `PermissionRequest` written as the hook writes it, waiting
/// `wait_ms` from `at_ms`.
fn prompt_request(
    relay: &RelayHome,
    at_ms: i64,
    wait_ms: i64,
) -> officina_core::relay::prompts::PromptRequest {
    let request = officina_core::relay::prompts::PromptRequest::from_payload(
        &serde_json::json!({
            "session_id": "0f1e2d3c-4b5a-4978-8796-a5b4c3d2e1f0",
            "tool_name": "Bash",
            "tool_input": {"command": "deploy --token sk-placeholder-0000"},
        }),
        None,
        7,
        at_ms,
        wait_ms,
    )
    .unwrap();
    std::fs::create_dir_all(relay.prompts_dir()).unwrap();
    std::fs::write(
        relay.prompts_dir().join(format!("{}.json", request.id)),
        serde_json::to_vec(&request).unwrap(),
    )
    .unwrap();
    request
}

fn prompt_lines(relay: &RelayHome) -> Vec<officina_core::relay::journal::PromptDecided> {
    read_recent(relay, now_ms())
        .0
        .into_iter()
        .filter_map(|line| match line.entry {
            JournalEntry::PromptDecided(decided) => Some(decided),
            _ => None,
        })
        .collect()
}

/// Story 5.22 AC 5 (AD-181, D-157's pattern): the first answer writes the
/// decision, bound to the request's session and input hash, and journals
/// `prompt_decided` without the input; a second answer, of either kind, is
/// refused `already_decided` and writes and journals nothing; an id of
/// another shape or without a request is refused.
#[test]
fn a_prompt_decision_is_written_once() {
    use officina_core::relay::prompts::{PromptAnswer, PromptBehavior, PromptDecision};
    use officina_host::relay::authority::decide_prompt;
    let home = TestHome::new();
    let relay = home.relay(None);
    let now = now_ms();
    let request = prompt_request(&relay, now, 60_000);
    decide_prompt(&relay, &request.id, PromptAnswer::Deny, now + 1).unwrap();
    let decided = relay
        .prompts_dir()
        .join(format!("{}.decided.json", request.id));
    let decision: PromptDecision =
        serde_json::from_slice(&std::fs::read(&decided).unwrap()).unwrap();
    assert_eq!(
        decision,
        PromptDecision::on(&request, PromptBehavior::Deny, now + 1)
    );
    for again in PromptAnswer::ALL {
        let refused = decide_prompt(&relay, &request.id, *again, now + 2).unwrap_err();
        assert!(
            matches!(refused, AuthorityError::Decide(DecideRefusal::Decided(_))),
            "{refused}"
        );
        assert!(refused.to_string().contains(AUTHORITY_MARKER));
    }
    assert!(
        !relay
            .prompts_dir()
            .join(format!("{}.declined", request.id))
            .exists()
    );
    let lines = prompt_lines(&relay);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].decision, PromptAnswer::Deny);
    assert_eq!(lines[0].input_sha256, request.input_sha256);
    let journal: String = std::fs::read_dir(relay.journal_dir())
        .unwrap()
        .flatten()
        .map(|e| std::fs::read_to_string(e.path()).unwrap_or_default())
        .collect();
    assert!(!journal.contains("sk-placeholder"), "never the input");
    for unknown in ["0a-1", "../approved", ""] {
        assert!(matches!(
            decide_prompt(&relay, unknown, PromptAnswer::Allow, now),
            Err(AuthorityError::Decide(DecideRefusal::UnknownPrompt(_)))
        ));
    }
}

/// Story 5.22 AC 5: a request past its deadline cannot be answered, by
/// any answer, and nothing is written or journaled.
#[test]
fn an_expired_prompt_cannot_be_decided() {
    use officina_core::relay::prompts::PromptAnswer;
    use officina_host::relay::authority::decide_prompt;
    let home = TestHome::new();
    let relay = home.relay(None);
    let now = now_ms();
    let request = prompt_request(&relay, now - 10_000, 5_000);
    for answer in PromptAnswer::ALL {
        assert!(matches!(
            decide_prompt(&relay, &request.id, *answer, now),
            Err(AuthorityError::Decide(DecideRefusal::Expired(_)))
        ));
    }
    let names: Vec<String> = std::fs::read_dir(relay.prompts_dir())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, [format!("{}.json", request.id)]);
    assert!(prompt_lines(&relay).is_empty());
}

/// Story 5.22 AC 5: a decline writes `<id>.declined` and no decision file,
/// journals `prompt_decided` with `decline`, and leaves nothing to answer.
#[test]
fn decline_writes_no_decision() {
    use officina_core::relay::prompts::PromptAnswer;
    use officina_host::relay::authority::decide_prompt;
    let home = TestHome::new();
    let relay = home.relay(None);
    let now = now_ms();
    let request = prompt_request(&relay, now, 60_000);
    decide_prompt(&relay, &request.id, PromptAnswer::Decline, now).unwrap();
    let dir = relay.prompts_dir();
    assert!(dir.join(format!("{}.declined", request.id)).exists());
    assert!(!dir.join(format!("{}.decided.json", request.id)).exists());
    assert!(matches!(
        decide_prompt(&relay, &request.id, PromptAnswer::Allow, now),
        Err(AuthorityError::Decide(DecideRefusal::Decided(_)))
    ));
    let lines = prompt_lines(&relay);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].decision, PromptAnswer::Decline);
}
