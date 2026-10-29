//! `auto_review` rows (story 5.13 AC 6, AD-82, AD-88, D-142, D-143): one
//! row per request with its baseline and reason, the author's decision
//! copied onto it, an expiry left as no decision; driven through the
//! window's review tick without a `review` thread, so every request
//! escalates before any call.

mod common;

use std::collections::BTreeMap;
use std::path::Path;

use common::TestHome;
use officina_core::registry::types::{AuthorDecision, AuthorReason, ReviewMode, ReviewOutcome};
use officina_core::relay::approvals::{
    ApprovalRequest, KeyInputs, PENDING_MS, request_id, request_key,
};
use officina_core::relay::auto::{EscalationCause, ReviewDecision};
use officina_core::relay::journal::JournalEntry;
use officina_host::files::now_ms;
use officina_host::registry::review::NewReview;
use officina_host::registry::store::open_reader;
use officina_host::registry::window::WindowRegistry;
use officina_host::relay::approvals;
use officina_host::relay::authority::{approve_catalogue, decide_approval};
use officina_host::relay::home::{RelayHome, load_relay_floor};
use officina_host::relay::journal::read_recent;
use officina_host::relay::review::{ReviewEnv, Reviews};
use officina_host::spawn::BaseEnv;

const FLOOR: &str = "schema = 2\n\n[[project]]\nname = \"lab\"\npath = \"Lab\"\nactions = [\"check\", \"apply\"]\n\n[[action]]\nname = \"check\"\nclass = \"free\"\nprogram = \"C:/path/to/tool.exe\"\nargs = [\"--check\", \"{group}\"]\n\n[action.params.group]\nvalues = [\"lab-web\", \"lab-db\"]\n\n[[action]]\nname = \"apply\"\nclass = \"auto\"\ncheck = \"check\"\nprogram = \"C:/path/to/tool.exe\"\nargs = [\"--apply\", \"{group}\"]\n\n[action.params.group]\nvalues = [\"lab-web\", \"lab-db\"]\n";

/// An approved relay with one `auto` action.
fn relay(home: &TestHome) -> RelayHome {
    let relay = home.relay(None);
    home.write(&relay.default_floor(), FLOOR);
    let hash = load_relay_floor(&relay.default_floor()).unwrap().hash;
    approve_catalogue(&relay, &hash).unwrap();
    relay
}

/// The request `officina run apply group=<group>` writes after its check,
/// created `ago` milliseconds ago.
fn write_request(relay: &RelayHome, group: &str, ago: i64) -> String {
    let hash = relay.read_approved().approved().unwrap().hash.clone();
    let params = BTreeMap::from([("group".to_owned(), group.to_owned())]);
    let key = request_key(&KeyInputs {
        project: "lab",
        action: "apply",
        params: &params,
        catalogue_hash: &hash,
        check_run_id: Some("19a2b3c4d5e-1f"),
        tree: None,
    });
    let created_at_ms = now_ms() - ago;
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
            group.to_owned(),
        ],
        http: None,
        catalogue_hash: hash,
        session_id: Some("00000000-0000-0000-0000-000000000001".to_owned()),
        check_run_id: Some("19a2b3c4d5e-1f".to_owned()),
        tree: None,
    };
    approvals::create_request(relay, &id, &request).unwrap();
    id
}

/// One review tick of the window, with no `claude` and no thread.
fn tick(reviews: &mut Reviews, relay: &RelayHome, registry: &mut WindowRegistry) -> bool {
    let base_env = BaseEnv::new(Vec::new());
    let env = ReviewEnv {
        relay,
        officina_home: relay.officina_home(),
        claude: None,
        config_dir: None,
        base_env: &base_env,
        no_excerpts: false,
        five_hour_used: None,
        board: None,
        timeout: officina_host::relay::review::review_timeout(),
    };
    reviews.tick(&env, registry, now_ms())
}

fn open(relay: &RelayHome) -> WindowRegistry {
    WindowRegistry::open(relay.officina_home(), &[], now_ms())
}

fn rows(home: &Path) -> Vec<officina_host::registry::review::ReviewRow> {
    open_reader(home, &[]).unwrap().review_rows(0).unwrap()
}

/// AC 6: a row holds the request, the mode, the outcome, the cause, the
/// decision, the reason as untrusted text, the call's metrics and the
/// baseline, and reads back so.
#[test]
fn a_review_writes_one_row_with_its_baseline() {
    let home = TestHome::new();
    let relay = relay(&home);
    let mut registry = open(&relay);
    let id = registry
        .insert_review(&NewReview {
            approval_id: "0123456789abcdef-1".into(),
            request_key: "ab".into(),
            project: "lab".into(),
            action: "apply".into(),
            requested_at_ms: now_ms() - 1_000,
            mode: ReviewMode::Shadow,
            outcome: ReviewOutcome::Answered,
            cause: Some(EscalationCause::AutoApprovalOff),
            decision: Some(ReviewDecision::Approve),
            reason: Some("The dry run passed.".into()),
            rule_baseline: true,
            call_started_at_ms: Some(now_ms() - 900),
            duration_ms: Some(800),
            input_tokens: Some(4_500),
            output_tokens: Some(40),
            cost_microusd: Some(1_100),
        })
        .expect("written");
    drop(registry);
    let rows = rows(relay.officina_home());
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.id, id);
    assert!(row.rule_baseline);
    assert_eq!(row.decision, Some(ReviewDecision::Approve));
    assert_eq!(row.cause, Some(EscalationCause::AutoApprovalOff));
    assert_eq!(
        serde_json::to_value(&row.reason).unwrap(),
        "The dry run passed."
    );
    assert!(
        !format!("{row:?}").contains("dry run passed"),
        "the reason never shows in debug output"
    );
    assert_eq!(row.cost_microusd, Some(1_100));
}

/// AC 6 and 1: a request the precheck escalates gets one row, `no_call`
/// with its cause, and its `auto_escalated` line; a second tick adds none.
#[test]
fn an_escalated_request_gets_one_row_and_its_line() {
    let home = TestHome::new();
    let relay = relay(&home);
    let id = write_request(&relay, "lab-web", 1_000);
    let mut registry = open(&relay);
    let mut reviews = Reviews::new(None);
    assert!(tick(&mut reviews, &relay, &mut registry));
    assert!(!tick(&mut reviews, &relay, &mut registry));
    assert_eq!(
        reviews.escalations().get(&id),
        Some(&EscalationCause::ClaudeNotFound)
    );
    drop(registry);
    let found = rows(relay.officina_home());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].approval_id, id);
    assert_eq!(found[0].outcome, ReviewOutcome::NoCall);
    assert_eq!(found[0].cause, Some(EscalationCause::ClaudeNotFound));
    let (lines, _) = read_recent(&relay, now_ms());
    let escalated: Vec<&JournalEntry> = lines
        .iter()
        .map(|line| &line.entry)
        .filter(|entry| matches!(entry, JournalEntry::AutoEscalated(_)))
        .collect();
    assert_eq!(escalated.len(), 1);
    // A window started again does not review the request twice.
    let mut again = open(&relay);
    let mut restarted = Reviews::new(None);
    tick(&mut restarted, &relay, &mut again);
    drop(again);
    assert_eq!(rows(relay.officina_home()).len(), 1);
}

/// AC 6 (AD-88): the author's decision, approve or reject with its reason,
/// is copied onto the request's row with its time.
#[test]
fn the_author_decision_is_copied_onto_the_row() {
    let home = TestHome::new();
    let relay = relay(&home);
    let rejected = write_request(&relay, "lab-web", 1_000);
    let approved = write_request(&relay, "lab-db", 1_000);
    let mut registry = open(&relay);
    let mut reviews = Reviews::new(None);
    tick(&mut reviews, &relay, &mut registry);
    decide_approval(&relay, &rejected, false, Some(AuthorReason::ShouldNotRun)).unwrap();
    decide_approval(&relay, &approved, true, None).unwrap();
    tick(&mut reviews, &relay, &mut registry);
    drop(registry);
    let found = rows(relay.officina_home());
    let row = |id: &str| found.iter().find(|r| r.approval_id == id).unwrap();
    assert_eq!(
        (row(&rejected).author_decision, row(&rejected).author_reason),
        (
            Some(AuthorDecision::Rejected),
            Some(AuthorReason::ShouldNotRun)
        )
    );
    assert!(row(&rejected).decided_at_ms.is_some());
    assert_eq!(
        (row(&approved).author_decision, row(&approved).author_reason),
        (Some(AuthorDecision::Approved), None)
    );
}

/// AC 6 (AD-88): a request that expired unanswered is no decision.
#[test]
fn expiry_is_not_a_decision() {
    let home = TestHome::new();
    let relay = relay(&home);
    let id = write_request(&relay, "lab-web", 1_000);
    let mut registry = open(&relay);
    let mut reviews = Reviews::new(None);
    tick(&mut reviews, &relay, &mut registry);
    let expired = write_request(&relay, "lab-db", PENDING_MS + 60_000);
    tick(&mut reviews, &relay, &mut registry);
    drop(registry);
    let found = rows(relay.officina_home());
    assert_eq!(found.len(), 1, "an expired request is never prechecked");
    assert_eq!(found[0].approval_id, id);
    assert_eq!(found[0].author_decision, None);
    assert!(found.iter().all(|r| r.approval_id != expired));
}
