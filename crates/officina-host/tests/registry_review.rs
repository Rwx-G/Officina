//! `auto_review` rows (story 5.13 AC 6, AD-82, AD-88, D-142, D-143): one
//! row per request with its baseline and reason, the author's decision
//! copied onto it, an expiry left as no decision; driven through the
//! window's review tick without a `review` thread, so every request
//! escalates before any call, but for the pacing case, which runs one.
//! Story 5.14: the switches under the shadow
//! gate, the mark that takes approval back, and runs linked to their row.

mod common;

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use common::TestHome;
use officina_core::agents::parse_agents;
use officina_core::board::{Board, BoardInput, build_board};
use officina_core::registry::triage_call::CALL_SPACING_MS;
use officina_core::registry::types::{
    AuthorDecision, AuthorReason, ReviewMode, ReviewOutcome, Source,
};
use officina_core::relay::approvals::{
    ApprovalRequest, CallerProcess, KeyInputs, PENDING_MS, request_id, request_key,
};
use officina_core::relay::auto::{
    AutoMode, AutoOffReason, EscalationCause, ReviewDecision, STATS_WINDOW_MS,
};
use officina_core::relay::catalogue::ActionClass;
use officina_core::relay::journal::{AutoApprovalSet, JournalEntry, RunStarted};
use officina_host::files::now_ms;
use officina_host::registry::review::NewReview;
use officina_host::registry::store::open_reader;
use officina_host::registry::window::WindowRegistry;
use officina_host::relay::approvals;
use officina_host::relay::authority::{approve_catalogue, decide_approval};
use officina_host::relay::home::{RelayHome, load_relay_floor};
use officina_host::relay::journal::read_recent;
use officina_host::relay::review::link_runs;
use officina_host::relay::review::{ReviewEnv, ReviewWorker, Reviews};
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
/// created `ago` milliseconds ago, from [`SESSION`] through this process.
fn write_request(relay: &RelayHome, group: &str, ago: i64) -> String {
    write_request_as(
        relay,
        group,
        ago,
        SESSION,
        officina_host::process_tree::own_chain(),
    )
}

/// [`write_request`] claiming `session`, with `caller` as its caller chain.
fn write_request_as(
    relay: &RelayHome,
    group: &str,
    ago: i64,
    session: &str,
    caller: Vec<CallerProcess>,
) -> String {
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
        session_id: Some(session.to_owned()),
        check_run_id: Some("19a2b3c4d5e-1f".to_owned()),
        tree: None,
        caller,
    };
    approvals::create_request(relay, &id, &request).unwrap();
    id
}

/// The session the requests claim, unless a test says otherwise.
const SESSION: &str = "00000000-0000-0000-0000-000000000001";
/// A background session no work order started, run by the same process.
const BACKGROUND: &str = "00000000-0000-0000-0000-000000000002";
/// An interactive session whose process is this test process itself, the
/// caller, never its ancestor.
const ITSELF: &str = "00000000-0000-0000-0000-000000000003";

/// The board the window reviews with (D-960): [`SESSION`] live and
/// interactive, its process the test runner's parent, which the caller
/// chain of a request this process writes holds as its ancestor; and
/// [`BACKGROUND`] and [`ITSELF`], which never bind.
fn board() -> Board {
    let chain = officina_host::process_tree::own_chain();
    let (own, parent) = (chain[0].pid, chain[1].pid);
    let agents = format!(
        r#"[{{"pid":{parent},"sessionId":"{SESSION}","kind":"interactive","status":"busy","cwd":"C:/path/to/Lab","startedAt":1}},
            {{"pid":{parent},"id":"bg2","sessionId":"{BACKGROUND}","kind":"background","status":"busy","state":"working","cwd":"C:/path/to/Lab","startedAt":1}},
            {{"pid":{own},"sessionId":"{ITSELF}","kind":"interactive","status":"busy","cwd":"C:/path/to/Lab","startedAt":1}}]"#
    );
    build_board(
        &BoardInput {
            agents: Some(parse_agents(&agents).unwrap()),
            ..Default::default()
        },
        now_ms(),
    )
}

/// One review tick of the window, with no `claude` and no thread.
fn tick(reviews: &mut Reviews, relay: &RelayHome, registry: &mut WindowRegistry) -> bool {
    tick_at(reviews, relay, registry, None, now_ms())
}

/// One review tick of the window stamped `now`, with `claude`.
fn tick_at(
    reviews: &mut Reviews,
    relay: &RelayHome,
    registry: &mut WindowRegistry,
    claude: Option<&Path>,
    now: i64,
) -> bool {
    let base_env = BaseEnv::new(Vec::new());
    let board = board();
    let env = ReviewEnv {
        relay,
        officina_home: relay.officina_home(),
        claude,
        config_dir: None,
        base_env: &base_env,
        no_excerpts: false,
        five_hour_used: None,
        board: Some(&board),
        timeout: officina_host::relay::review::review_timeout(),
    };
    reviews.tick(&env, registry, now)
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
        reviews.escalations().get(&id).map(|e| (e.cause, e.mode)),
        Some((EscalationCause::ClaudeNotFound, ReviewMode::Shadow))
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

/// D-960: a request whose session claim does not bind to its caller is the
/// author's at once, `session_unbound` with its row, its line and no call;
/// the same request from the bound session is queued for its review.
#[test]
fn an_unbound_session_claim_escalates_before_any_call() {
    let home = TestHome::new();
    let relay = relay(&home);
    let own = officina_host::process_tree::own_chain();
    let unbound = [
        ("no caller chain", SESSION, Vec::new()),
        (
            "a session the board does not show",
            "s-not-listed",
            own.clone(),
        ),
        (
            "a background session no work order started",
            BACKGROUND,
            own.clone(),
        ),
        ("the caller itself as the session", ITSELF, own.clone()),
    ];
    let ids: Vec<(&str, String)> = unbound
        .into_iter()
        .zip(["lab-web", "lab-db"].iter().cycle())
        .enumerate()
        .map(|(at, ((name, session, caller), group))| {
            let ago = 10_000 + 1_000 * i64::try_from(at).unwrap();
            (name, write_request_as(&relay, group, ago, session, caller))
        })
        .collect();
    let bound = write_request(&relay, "lab-web", 1_000);
    let claude = relay.officina_home().join("missing").join("claude.exe");
    let mut registry = open(&relay);
    let mut reviews = Reviews::new(None);
    tick_at(&mut reviews, &relay, &mut registry, Some(&claude), now_ms());
    drop(registry);
    let found = rows(relay.officina_home());
    for (name, id) in &ids {
        let row = found.iter().find(|r| &r.approval_id == id);
        assert_eq!(
            row.map(|r| (r.outcome, r.cause)),
            Some((ReviewOutcome::NoCall, Some(EscalationCause::SessionUnbound))),
            "{name}"
        );
        assert_eq!(
            reviews.escalations().get(id).map(|e| e.cause),
            Some(EscalationCause::SessionUnbound),
            "{name}"
        );
    }
    assert!(
        found.iter().all(|r| r.approval_id != bound),
        "the bound request waits for its call"
    );
    assert_eq!(
        reviews.escalations().get(&bound).map(|e| e.cause),
        Some(EscalationCause::AutoApprovalOff)
    );
    let (lines, _) = read_recent(&relay, now_ms());
    let journaled = lines
        .iter()
        .filter(|line| {
            matches!(&line.entry, JournalEntry::AutoEscalated(a)
                if a.cause == EscalationCause::SessionUnbound)
        })
        .count();
    assert_eq!(journaled, ids.len());
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

/// Story 5.13 AC 4 (D-142): the spacing runs from the instant the last
/// call started, the one its row records, not from the stamp of the tick
/// that submitted it. A tick's stamp is taken before its reads and
/// prechecks, so under load it is older than the call it starts; a first
/// tick 3 s stale, then prompt ones, must not start the second call 2 s
/// after the first. The `claude` named does not exist: each call starts,
/// fails and records its start all the same.
#[test]
fn the_spacing_runs_from_the_call_start_not_the_tick_stamp() {
    const STALE_MS: i64 = 3_000;
    let home = TestHome::new();
    let relay = relay(&home);
    write_request(&relay, "lab-web", 10_000);
    write_request(&relay, "lab-db", 10_000);
    let claude = relay.officina_home().join("missing").join("claude.exe");
    let mut registry = open(&relay);
    let mut reviews = Reviews::new(Some(ReviewWorker::spawn().unwrap()));
    tick_at(
        &mut reviews,
        &relay,
        &mut registry,
        Some(&claude),
        now_ms() - STALE_MS,
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    let started = loop {
        tick_at(&mut reviews, &relay, &mut registry, Some(&claude), now_ms());
        let started: Vec<i64> = registry
            .review_rows(0)
            .iter()
            .filter_map(|row| row.call_started_at_ms)
            .collect();
        if started.len() == 2 {
            break started;
        }
        assert!(Instant::now() < deadline, "{started:?}");
        std::thread::sleep(Duration::from_millis(20));
    };
    let gap = (started[0] - started[1]).abs();
    assert!(gap >= CALL_SPACING_MS, "{gap} ms apart");
}

/// One row of `mode` for request `approval_id`, requested `ago` ms ago:
/// an answered `approve` whose baseline held.
fn review(approval_id: &str, mode: ReviewMode, ago: i64) -> NewReview {
    let at = now_ms() - ago;
    NewReview {
        approval_id: approval_id.into(),
        request_key: "ab".into(),
        project: "lab".into(),
        action: "apply".into(),
        requested_at_ms: at,
        mode,
        outcome: ReviewOutcome::Answered,
        cause: (mode == ReviewMode::Shadow).then_some(EscalationCause::AutoApprovalOff),
        decision: Some(ReviewDecision::Approve),
        reason: Some("The dry run passed.".into()),
        rule_baseline: true,
        call_started_at_ms: Some(at),
        duration_ms: Some(800),
        input_tokens: None,
        output_tokens: None,
        cost_microusd: None,
    }
}

/// `n` clean shadow cases: answered approve, the author approving too.
fn clean_cases(registry: &mut WindowRegistry, n: usize) {
    for i in 0..n {
        let id = registry
            .insert_review(&review(&format!("shadow-{i}"), ReviewMode::Shadow, 60_000))
            .unwrap();
        registry.copy_author_decision(id, AuthorDecision::Approved, None, now_ms() - 30_000);
    }
}

fn auto_lines(relay: &RelayHome) -> Vec<AutoApprovalSet> {
    read_recent(relay, now_ms())
        .0
        .into_iter()
        .filter_map(|line| match line.entry {
            JournalEntry::AutoApprovalSet(set) => Some(set),
            _ => None,
        })
        .collect()
}

/// Story 5.14 AC 2 (AD-88): turning auto approval on is refused while the
/// gate does not hold; 19 clean cases are not enough, 20 are.
#[test]
fn turning_on_is_refused_without_the_gate() {
    let home = TestHome::new();
    let relay = relay(&home);
    let mut registry = open(&relay);
    clean_cases(&mut registry, 19);
    let refused = registry.set_auto_approval(true, now_ms()).unwrap_err();
    assert!(
        refused.message.contains("20 shadow reviews"),
        "{}",
        refused.message
    );
    assert!(!registry.auto_state().approval.on);
    clean_cases(&mut registry, 1);
    let state = registry.set_auto_approval(true, now_ms()).unwrap();
    assert!(state.approval.on);
    assert_eq!(state.approval_src, Some(Source::Author));
}

/// Story 5.14 AC 2 (AD-88): the gate is judged when the writer writes, not
/// when the window showed it: a disagreement since then refuses.
#[test]
fn turning_on_is_refused_when_the_gate_broke_since_display() {
    let home = TestHome::new();
    let relay = relay(&home);
    let mut registry = open(&relay);
    clean_cases(&mut registry, 20);
    assert!(
        registry
            .writer()
            .unwrap()
            .shadow_stats(now_ms())
            .unwrap()
            .gate
    );
    let id = registry
        .insert_review(&review("refused-later", ReviewMode::Shadow, 60_000))
        .unwrap();
    registry.copy_author_decision(
        id,
        AuthorDecision::Rejected,
        Some(AuthorReason::ShouldNotRun),
        now_ms(),
    );
    assert!(registry.set_auto_approval(true, now_ms()).is_err());
    assert!(!registry.auto_state().approval.on);
}

/// Story 5.14 AC 2 (AD-88): off is always allowed, without any case.
#[test]
fn off_is_always_allowed() {
    let home = TestHome::new();
    let relay = relay(&home);
    let mut registry = open(&relay);
    let state = registry.set_auto_approval(false, now_ms()).unwrap();
    assert!(!state.approval.on);
    assert_eq!(state.approval_src, Some(Source::Author));
    assert_eq!(state.approval.reason, None);
}

/// Story 5.14 AC 2 (D-162): rule mode needs the rule offer, review mode
/// never does; the mode reads back as stored text.
#[test]
fn rule_mode_needs_the_rule_offer() {
    let home = TestHome::new();
    let relay = relay(&home);
    let mut registry = open(&relay);
    clean_cases(&mut registry, 19);
    let refused = registry
        .set_auto_mode(AutoMode::Rule, now_ms())
        .unwrap_err();
    assert!(refused.message.contains("rule mode"), "{}", refused.message);
    assert_eq!(
        registry
            .set_auto_mode(AutoMode::Review, now_ms())
            .unwrap()
            .mode,
        AutoMode::Review
    );
    clean_cases(&mut registry, 1);
    assert_eq!(
        registry
            .set_auto_mode(AutoMode::Rule, now_ms())
            .unwrap()
            .mode,
        AutoMode::Rule
    );
    drop(registry);
    let reader = open_reader(relay.officina_home(), &[]).unwrap();
    assert_eq!(reader.auto_state().unwrap().mode, AutoMode::Rule);
}

/// A live approval of request `approval_id` and the run `officina run`
/// started from it, journaled; the run's id.
fn live_run(relay: &RelayHome, registry: &mut WindowRegistry, approval_id: &str) -> String {
    registry
        .insert_review(&review(approval_id, ReviewMode::Live, 60_000))
        .unwrap();
    let run_id = format!("run-{approval_id}");
    let started = JournalEntry::RunStarted(RunStarted {
        run_id: run_id.clone(),
        project: "lab".into(),
        session_id: None,
        action: "apply".into(),
        class: ActionClass::Auto,
        params: BTreeMap::new(),
        argv: Vec::new(),
        method: None,
        url: None,
        credential: None,
        catalogue_hash: "00".into(),
        approval_id: Some(approval_id.into()),
        tree: None,
        timeout_s: Some(60),
    });
    officina_host::relay::journal::append(relay, &[started], now_ms()).unwrap();
    run_id
}

/// Story 5.14 AC 5 (AD-88): a run started from a review's approval links
/// to its row through `run_started.approval_id`; one from no review links
/// nothing.
#[test]
fn runs_link_to_their_review_through_the_approval_id() {
    let home = TestHome::new();
    let relay = relay(&home);
    let mut registry = open(&relay);
    let run_id = live_run(&relay, &mut registry, "0123456789abcdef-7");
    let shadow = registry
        .insert_review(&review("0123456789abcdef-8", ReviewMode::Shadow, 60_000))
        .unwrap();
    link_runs(&relay, &mut registry, now_ms());
    drop(registry);
    let found = rows(relay.officina_home());
    let row = |id: &str| found.iter().find(|r| r.approval_id == id).unwrap();
    assert_eq!(
        row("0123456789abcdef-7").run_id.as_deref(),
        Some(run_id.as_str())
    );
    assert_eq!(found.iter().find(|r| r.id == shadow).unwrap().run_id, None);
}

/// Story 5.14 AC 5 (AD-88, FR-9, D-141): "should not have run" stamps the
/// run's row, turns approval off with its reason, sets the mode back to
/// `review`, and is refused for a run no review approved or a second time.
#[test]
fn a_mark_turns_approval_off_and_resets_the_mode() {
    let home = TestHome::new();
    let relay = relay(&home);
    let mut registry = open(&relay);
    clean_cases(&mut registry, 20);
    registry.set_auto_approval(true, now_ms()).unwrap();
    registry.set_auto_mode(AutoMode::Rule, now_ms()).unwrap();
    let run_id = live_run(&relay, &mut registry, "0123456789abcdef-7");
    assert!(
        registry
            .mark_should_not_have_run(&run_id, now_ms())
            .is_err(),
        "not linked yet"
    );
    link_runs(&relay, &mut registry, now_ms());
    let state = registry
        .mark_should_not_have_run(&run_id, now_ms())
        .unwrap();
    assert!(!state.approval.on);
    assert_eq!(state.approval.reason, Some(AutoOffReason::ShouldNotHaveRun));
    assert_eq!(state.approval_src, Some(Source::Author));
    assert_eq!(state.mode, AutoMode::Review);
    let again = registry
        .mark_should_not_have_run(&run_id, now_ms())
        .unwrap_err();
    assert!(
        again.message.contains("already marked"),
        "{}",
        again.message
    );
    assert!(
        registry
            .mark_should_not_have_run("run-of-no-review", now_ms())
            .is_err()
    );
    drop(registry);
    let found = rows(relay.officina_home());
    assert!(
        found
            .iter()
            .any(|r| r.run_id.as_deref() == Some(run_id.as_str()) && r.marked_at_ms.is_some())
    );
}

/// Story 5.14 AC 5 (AD-88): a mark is a disagreement for 30 days, so
/// turning approval on again needs the gate to hold again.
#[test]
fn a_mark_is_a_disagreement_for_30_days() {
    let home = TestHome::new();
    let relay = relay(&home);
    let mut registry = open(&relay);
    clean_cases(&mut registry, 25);
    registry.set_auto_approval(true, now_ms()).unwrap();
    let run_id = live_run(&relay, &mut registry, "0123456789abcdef-7");
    link_runs(&relay, &mut registry, now_ms());
    registry
        .mark_should_not_have_run(&run_id, now_ms())
        .unwrap();
    let stats = registry.writer().unwrap().shadow_stats(now_ms()).unwrap();
    assert_eq!((stats.cases, stats.disagreements), (25, 1));
    assert!(registry.set_auto_approval(true, now_ms()).is_err());
    let later = now_ms() + STATS_WINDOW_MS + 1;
    let stats = registry.writer().unwrap().shadow_stats(later).unwrap();
    assert_eq!(
        stats.disagreements, 0,
        "the mark leaves the window after 30 days"
    );
}

/// Story 5.14 AC 2: each change the window makes is journaled
/// `auto_approval_set` by the collector; the registry's writer journals
/// nothing itself.
#[test]
fn the_writer_journals_nothing_itself() {
    let home = TestHome::new();
    let relay = relay(&home);
    let mut registry = open(&relay);
    registry.set_auto_approval(false, now_ms()).unwrap();
    assert!(auto_lines(&relay).is_empty());
}
