//! The review of `auto` requests end to end (story 5.13, AD-86 to AD-88,
//! PRD section 7's Auto fail-closed row): real requests made by
//! `officina run`, reviewed by the window's collector thread through the
//! fake claude's `review-*` modes (D-582), each outcome one `auto_review`
//! row; every precheck cause escalating with no call; the budget and the
//! cap counted from the rows; blind shadow writing no approval; and the
//! session's answers never reaching the review; and (story 5.14) live and
//! rule modes, approving through the window's authority. Windows only, like
//! `run.rs`: the floor's program is the fake tool's Windows path.

#![cfg(windows)]

mod common;

use std::time::Duration;

use common::auto::{Auto, Options, PATIENCE, SESSION, stop};
use officina_core::registry::types::{ReviewMode, ReviewOutcome};
use officina_core::relay::approvals::{ApprovalState, DecideRefusal};
use officina_core::relay::auto::{AutoMode, EscalationCause, ReviewDecision};
use officina_core::relay::journal::JournalEntry;
use officina_host::registry::review::ReviewRow;
use officina_host::relay::authority::{AuthorityError, decide_approval};

/// One mode of the fake: its name, the author's prompt, how long it hangs,
/// and the row's outcome, decision and cause.
type ModeCase = (
    &'static str,
    String,
    u64,
    ReviewOutcome,
    Option<ReviewDecision>,
    EscalationCause,
);

/// AC 8: each answer of the fake review gives its row: the outcome, the
/// decision kept, and the cause shadow mode records.
#[test]
fn each_review_mode_records_its_outcome() {
    let token = |decision: &str| format!("Apply the playbook. FAKE-REVIEW:{decision}");
    let cases: [ModeCase; 7] = [
        (
            "review",
            token("approve"),
            0,
            ReviewOutcome::Answered,
            Some(ReviewDecision::Approve),
            EscalationCause::AutoApprovalOff,
        ),
        (
            "review",
            token("reject"),
            0,
            ReviewOutcome::Answered,
            Some(ReviewDecision::Reject),
            EscalationCause::ReviewRejected,
        ),
        (
            "review",
            token("escalate"),
            0,
            ReviewOutcome::Answered,
            Some(ReviewDecision::Escalate),
            EscalationCause::ReviewEscalated,
        ),
        (
            "review-garbage",
            token("approve"),
            0,
            ReviewOutcome::Invalid,
            None,
            EscalationCause::InvalidAnswer,
        ),
        (
            "review-extra-field",
            token("approve"),
            0,
            ReviewOutcome::Invalid,
            None,
            EscalationCause::InvalidAnswer,
        ),
        (
            "review-exit1",
            token("approve"),
            0,
            ReviewOutcome::Failed,
            None,
            EscalationCause::CallFailed,
        ),
        (
            "review-hang",
            token("approve"),
            30_000,
            ReviewOutcome::Timeout,
            None,
            EscalationCause::CallTimeout,
        ),
    ];
    for (mode, prompt, hang_ms, outcome, decision, cause) in cases {
        let options = Options {
            mode,
            prompt,
            hang_ms,
            ..Options::default()
        };
        let auto = Auto::new(&options);
        let id = auto.request("lab-web", Some(SESSION));
        let fake = auto.fake(&options, "bin");
        let (handle, _emitted) =
            auto.window_with(auto.collector(Some(fake), Duration::from_secs(2)));
        let row = auto.row_of(&id);
        stop(handle);
        assert_eq!(row.outcome, outcome, "{mode}");
        assert_eq!(row.decision, decision, "{mode}");
        assert_eq!(row.cause, Some(cause), "{mode}");
        assert!(row.call_started_at_ms.is_some(), "{mode}: a call ran");
        assert_eq!(auto.calls().len(), 1, "{mode}: one call");
        let lines: Vec<JournalEntry> = auto
            .relay
            .env
            .journal()
            .into_iter()
            .map(|l| l.entry)
            .collect();
        assert!(
            lines.iter().any(|e| matches!(e, JournalEntry::AutoReview(r) if r.approval_id == id && r.outcome == outcome)),
            "{mode}: the auto_review line"
        );
        assert!(
            lines.iter().any(|e| matches!(e, JournalEntry::AutoEscalated(a) if a.approval_id == id && a.cause == cause)),
            "{mode}: the auto_escalated line"
        );
    }
}

/// AC 5 (D-143): a review that answers approve in shadow writes no
/// approval file and no decision marker; the card says `auto_approval_off`.
#[test]
fn an_approve_in_shadow_writes_no_approval_file() {
    let options = Options {
        prompt: "Apply it. FAKE-REVIEW:approve".to_owned(),
        ..Options::default()
    };
    let auto = Auto::new(&options);
    let id = auto.request("lab-web", Some(SESSION));
    let (handle, emitted) = auto.window(&options);
    let row = auto.row_of(&id);
    assert_eq!(row.decision, Some(ReviewDecision::Approve));
    let board = loop {
        let board = emitted
            .boards
            .recv_timeout(common::auto::PATIENCE)
            .expect("a board");
        if board
            .approvals
            .iter()
            .any(|a| a.id == id && a.escalation.is_some())
        {
            break board;
        }
    };
    stop(handle);
    let card = board.approvals.iter().find(|a| a.id == id).unwrap();
    assert_eq!(card.escalation, Some(EscalationCause::AutoApprovalOff));
    let dir = auto.relay.env.relay_home().approvals_dir();
    let names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names.iter().all(|n| n.ends_with(".request.json")),
        "{names:?}"
    );
}

/// AC 1 and 8: each precheck cause escalates with its row and no call.
#[test]
fn each_precheck_cause_escalates_without_a_call() {
    let plain = Options::default();
    for cause in EscalationCause::ALL
        .iter()
        .copied()
        .filter(|c| c.before_any_call())
    {
        let options = Options {
            no_excerpts: cause == EscalationCause::NoExcerpts,
            review_table: if cause == EscalationCause::ReviewConfigInvalid {
                "[auto_review]\nmodel = \"gpt\"\n"
            } else {
                ""
            },
            cap: if cause == EscalationCause::HourlyCap {
                1
            } else {
                6
            },
            ..plain.clone()
        };
        let auto = Auto::new(&options);
        match cause {
            EscalationCause::HourlyCap => auto.seed_rows(1, 60_000, Some(ReviewDecision::Approve)),
            EscalationCause::BudgetSpent => auto.seed_rows(60, 60_000, None),
            EscalationCause::QuotaHigh => {
                let payload = serde_json::json!({"session_id": SESSION, "rate_limits": {
                    "five_hour": {"used_percentage": 95, "resets_at": common::now_ms() / 1000 + 3600}}});
                officina_host::writers::record_quota(
                    &auto.paths(),
                    payload.to_string().as_bytes(),
                    common::now_ms() / 1000,
                );
            }
            EscalationCause::WorkdirNotEmpty => {
                let workdir =
                    officina_host::relay::review::workdir(&auto.relay.env.officina_home());
                std::fs::create_dir_all(&workdir).unwrap();
                std::fs::write(workdir.join("CLAUDE.md"), "planted").unwrap();
            }
            _ => {}
        }
        let session = (cause != EscalationCause::NoSessionId).then_some(SESSION);
        let id = auto.request("lab-web", session);
        let claude = match cause {
            EscalationCause::ClaudeNotFound => None,
            EscalationCause::ClaudeShim => {
                let fake = auto.fake(&options, "bin");
                let shim = fake.with_file_name("claude.cmd");
                std::fs::copy(&fake, &shim).unwrap();
                Some(shim)
            }
            _ => Some(auto.fake(&options, "bin")),
        };
        let (handle, _emitted) = auto.window_with(auto.collector(claude, Duration::from_secs(10)));
        let row = auto.row_of(&id);
        stop(handle);
        assert_eq!(row.cause, Some(cause), "{cause:?}");
        assert_eq!(row.outcome, ReviewOutcome::NoCall, "{cause:?}");
        assert_eq!(row.call_started_at_ms, None, "{cause:?}");
        assert!(auto.calls().is_empty(), "{cause:?}: no call");
    }
}

/// AC 8 (D-142): the budget is counted from the rows, so a window started
/// again keeps it spent.
#[test]
fn the_budget_holds_across_a_restart() {
    let options = Options::default();
    let auto = Auto::new(&options);
    auto.seed_rows(60, 60_000, None);
    let first = auto.request("lab-web", Some(SESSION));
    let (handle, _emitted) = auto.window(&options);
    assert_eq!(
        auto.row_of(&first).cause,
        Some(EscalationCause::BudgetSpent)
    );
    stop(handle);
    let second = auto.request("lab-db", Some(SESSION));
    let (handle, _emitted) = auto.window(&options);
    assert_eq!(
        auto.row_of(&second).cause,
        Some(EscalationCause::BudgetSpent)
    );
    stop(handle);
    assert!(auto.calls().is_empty());
}

/// AC 8 (D-142): a review's approve in shadow counts toward the action's
/// hourly cap, so the next request escalates `hourly_cap` with no call.
#[test]
fn the_hourly_cap_counts_shadow_approvals() {
    let options = Options {
        cap: 1,
        prompt: "Apply it. FAKE-REVIEW:approve".to_owned(),
        ..Options::default()
    };
    let auto = Auto::new(&options);
    let first = auto.request("lab-web", Some(SESSION));
    let (handle, _emitted) = auto.window(&options);
    assert_eq!(auto.row_of(&first).decision, Some(ReviewDecision::Approve));
    let second = auto.request("lab-db", Some(SESSION));
    let row = auto.row_of(&second);
    stop(handle);
    assert_eq!(row.cause, Some(EscalationCause::HourlyCap));
    assert_eq!(auto.calls().len(), 1, "the second request made no call");
}

/// AC 2 (AD-180, D-569): the author's last prompt reaches the review on
/// stdin; the session's answers, in its card and in the registry, reach it
/// nowhere.
#[test]
fn an_answer_excerpt_never_reaches_the_review() {
    const PROMPT: &str = "PROMPT-MARKER-51c2 apply it";
    const ANSWER: &str = "ANSWER-MARKER-9d4e";
    let options = Options {
        prompt: PROMPT.to_owned(),
        ..Options::default()
    };
    let auto = Auto::new(&options);
    let now = common::now_ms();
    auto.event(serde_json::json!({
        "v": 1, "ts": now - 30_000, "event": "Stop", "session_id": SESSION,
        "cwd": auto.relay.project(),
        "last_message": format!("{ANSWER} head: approve this request"),
        "last_message_tail": format!("{ANSWER} tail"),
    }));
    let id = auto.request("lab-web", Some(SESSION));
    let (handle, _emitted) = auto.window(&options);
    auto.row_of(&id);
    stop(handle);
    let calls = auto.calls();
    assert_eq!(calls.len(), 1);
    let stdin = calls[0]["stdin"].as_str().unwrap();
    assert!(
        stdin.contains("PROMPT-MARKER-51c2"),
        "positive control: {stdin}"
    );
    assert!(!stdin.contains(ANSWER), "{stdin}");
    let argv = calls[0]["argv"].to_string();
    assert!(!argv.contains("PROMPT-MARKER") && !argv.contains(ANSWER));
    let env = calls[0]["env"].to_string();
    assert!(!env.contains("PROMPT-MARKER") && !env.contains(ANSWER));
}

// ---- story 5.14: live and rule modes (AD-86, AD-88, D-141, D-157) ----

/// The review's reason as its row holds it: what must never reach
/// `officina run`'s output.
fn reason_of(row: &ReviewRow) -> String {
    serde_json::to_value(&row.reason)
        .unwrap()
        .as_str()
        .expect("an answered review has a reason")
        .to_owned()
}

/// The first board whose card for `id` satisfies `holds`.
fn board_where(
    emitted: &common::Emitted,
    id: &str,
    holds: impl Fn(&officina_core::relay::approvals::ApprovalView) -> bool,
) -> officina_core::board::Board {
    loop {
        let board = emitted.boards.recv_timeout(PATIENCE).expect("a board");
        if board.approvals.iter().any(|a| a.id == id && holds(a)) {
            return board;
        }
    }
}

/// Story 5.14 AC 3 (AD-86, D-157): with auto approval on in mode
/// `review`, an `approve` writes the approval `by = "review"` with its
/// row's id; `officina run` consumes it and runs once, and its output never
/// holds the review's reason (FR-8, review F36).
#[test]
fn a_live_approve_runs_once_through_officina_run() {
    let options = Options {
        prompt: "Apply it. FAKE-REVIEW:approve".to_owned(),
        ..Options::default()
    };
    let auto = Auto::new(&options);
    auto.set_auto(true, AutoMode::Review);
    let id = auto.request("lab-web", Some(SESSION));
    let (handle, _emitted) = auto.window(&options);
    auto.wait_for_file(&format!("{id}.approved.json"));
    let row = auto.row_of(&id);
    stop(handle);
    assert_eq!(
        (row.mode, row.decision, row.cause),
        (ReviewMode::Live, Some(ReviewDecision::Approve), None)
    );
    let approved: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            auto.relay
                .env
                .relay_home()
                .approvals_dir()
                .join(format!("{id}.approved.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(approved["by"], "review");
    assert_eq!(approved["review_id"], row.id.to_string());
    assert!(
        !auto
            .approval_files()
            .iter()
            .any(|n| n.ends_with(".reviewing")),
        "the mark is gone once the review answered"
    );
    let ran = auto.run(&["apply", "group=lab-web"], Some(SESSION));
    assert_eq!(ran.status.code(), Some(0), "{ran:?}");
    let reason = reason_of(&row);
    for stream in [&ran.stdout, &ran.stderr] {
        assert!(!String::from_utf8_lossy(stream).contains(&reason));
    }
    let started: Vec<_> = auto
        .relay
        .env
        .journal()
        .into_iter()
        .filter_map(|line| match line.entry {
            JournalEntry::RunStarted(s) if s.action == "apply" => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].approval_id.as_deref(), Some(id.as_str()));
    let again = auto.run(&["apply", "group=lab-web", "--wait", "1"], Some(SESSION));
    assert_eq!(again.status.code(), Some(126), "used once: a new request");
}

/// Story 5.14 AC 3 (D-143): in live mode a review's `reject` or
/// `escalate` makes the request a Needs you card naming its cause, with no
/// approval.
#[test]
fn a_live_reject_or_escalate_becomes_a_needs_you_card_with_its_cause() {
    for (token, cause) in [
        ("reject", EscalationCause::ReviewRejected),
        ("escalate", EscalationCause::ReviewEscalated),
    ] {
        let options = Options {
            prompt: format!("Apply it. FAKE-REVIEW:{token}"),
            ..Options::default()
        };
        let auto = Auto::new(&options);
        auto.set_auto(true, AutoMode::Review);
        let id = auto.request("lab-web", Some(SESSION));
        let (handle, emitted) = auto.window(&options);
        let board = board_where(&emitted, &id, |a| a.escalation.is_some());
        stop(handle);
        let card = board.approvals.iter().find(|a| a.id == id).unwrap();
        assert_eq!(card.escalation, Some(cause), "{token}");
        assert_eq!(card.state, ApprovalState::Pending, "{token}");
        assert!(board.waiting_count >= 1, "{token}");
        assert_eq!(auto.row_of(&id).mode, ReviewMode::Live, "{token}");
        assert!(
            auto.approval_files()
                .iter()
                .all(|n| n.ends_with(".request.json")),
            "{token}: {:?}",
            auto.approval_files()
        );
    }
}

/// Story 5.14 AC 3 (AD-92, D-157): while its live review runs, a request
/// is `reviewing`: no buttons to press, not counted as waiting, and the
/// author cannot decide it.
#[test]
fn a_reviewing_request_is_not_counted_as_waiting() {
    let options = Options {
        mode: "review-hang",
        hang_ms: 60_000,
        ..Options::default()
    };
    let auto = Auto::new(&options);
    auto.set_auto(true, AutoMode::Review);
    let id = auto.request("lab-web", Some(SESSION));
    let fake = auto.fake(&options, "bin");
    let (handle, emitted) = auto.window_with(auto.collector(Some(fake), Duration::from_secs(20)));
    let board = board_where(&emitted, &id, |a| a.state == ApprovalState::Reviewing);
    let refused = decide_approval(&auto.relay.env.relay_home(), &id, true, None);
    stop(handle);
    let card = board.approvals.iter().find(|a| a.id == id).unwrap();
    assert_eq!(card.escalation, None);
    assert_eq!(board.waiting_count, 0);
    assert!(matches!(
        refused,
        Err(AuthorityError::Decide(DecideRefusal::Reviewing(_)))
    ));
}

/// Story 5.14 AC 3: a request decided before its turn in the queue is
/// skipped: no call, no row, and its `reviewing` mark lifted.
#[test]
fn a_request_decided_before_its_turn_is_skipped() {
    let options = Options {
        mode: "review-hang",
        hang_ms: 4_000,
        ..Options::default()
    };
    let auto = Auto::new(&options);
    auto.set_auto(true, AutoMode::Review);
    let first = auto.request("lab-web", Some(SESSION));
    let second = auto.request("lab-db", Some(SESSION));
    let fake = auto.fake(&options, "bin");
    let (handle, _emitted) = auto.window_with(auto.collector(Some(fake), Duration::from_secs(2)));
    auto.wait_for_file(&format!("{second}.reviewing"));
    // Another decider claimed it while the first call ran (D-157).
    let home = auto.relay.env.relay_home();
    assert!(officina_host::relay::approvals::claim_decision(&home, &second).unwrap());
    assert_eq!(auto.row_of(&first).outcome, ReviewOutcome::Timeout);
    let deadline = std::time::Instant::now() + PATIENCE;
    while auto
        .approval_files()
        .contains(&format!("{second}.reviewing"))
    {
        assert!(std::time::Instant::now() < deadline, "the mark stays");
        std::thread::sleep(Duration::from_millis(50));
    }
    stop(handle);
    assert!(auto.rows().iter().all(|r| r.approval_id != second));
    assert_eq!(auto.calls().len(), 1, "the skipped request made no call");
}

/// Story 5.14 AC 4 (D-141): in rule mode no review call runs at all.
#[test]
fn rule_mode_makes_no_call() {
    let options = Options {
        prompt: "Apply it. FAKE-REVIEW:approve".to_owned(),
        ..Options::default()
    };
    let auto = Auto::new(&options);
    auto.set_auto(true, AutoMode::Rule);
    let id = auto.request("lab-web", Some(SESSION));
    let (handle, _emitted) = auto.window(&options);
    let row = auto.row_of(&id);
    stop(handle);
    assert_eq!(
        (row.mode, row.outcome),
        (ReviewMode::Rule, ReviewOutcome::NoCall)
    );
    assert_eq!(row.call_started_at_ms, None);
    assert!(auto.calls().is_empty());
}

/// Story 5.14 AC 4 (D-141, D-162): a false baseline (no earlier run of
/// the action) escalates `rule_baseline_false`; once a run succeeded and
/// its check holds on the tree, the next request is approved `by = "rule"`
/// with a row deciding `approve`.
#[test]
fn rule_mode_approves_by_rule_and_escalates_a_false_baseline() {
    let options = Options::default();
    let auto = Auto::new(&options);
    auto.set_auto(true, AutoMode::Rule);
    let first = auto.request("lab-web", Some(SESSION));
    let (handle, _emitted) = auto.window(&options);
    let row = auto.row_of(&first);
    assert_eq!(
        (row.decision, row.cause, row.rule_baseline),
        (
            Some(ReviewDecision::Escalate),
            Some(EscalationCause::RuleBaselineFalse),
            false
        )
    );
    decide_approval(&auto.relay.env.relay_home(), &first, true, None).unwrap();
    let ran = auto.run(&["apply", "group=lab-web"], Some(SESSION));
    assert_eq!(ran.status.code(), Some(0), "{ran:?}");
    // The next request is approved by the rule while `officina run` waits,
    // and runs.
    let known = auto.request_ids();
    let check = auto.run(&["check", "group=lab-db"], None);
    assert_eq!(check.status.code(), Some(0), "{check:?}");
    let waited = auto.run(&["apply", "group=lab-db", "--wait", "20"], Some(SESSION));
    assert_eq!(waited.status.code(), Some(0), "{waited:?}");
    let second = auto
        .request_ids()
        .into_iter()
        .find(|id| !known.contains(id))
        .expect("a new request");
    let row = auto.row_of(&second);
    stop(handle);
    assert_eq!(
        (
            row.mode,
            row.outcome,
            row.decision,
            row.cause,
            row.rule_baseline
        ),
        (
            ReviewMode::Rule,
            ReviewOutcome::NoCall,
            Some(ReviewDecision::Approve),
            None,
            true
        )
    );
    let approved: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            auto.relay
                .env
                .relay_home()
                .approvals_dir()
                .join(format!("{second}.consumed.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(approved["by"], "rule");
    assert!(auto.calls().is_empty());
}

/// Story 5.14 AC 4 (D-162): rule rows count toward the hourly cap as
/// review rows do.
#[test]
fn rule_approvals_count_toward_the_hourly_cap() {
    let options = Options {
        cap: 1,
        ..Options::default()
    };
    let auto = Auto::new(&options);
    auto.set_auto(true, AutoMode::Rule);
    auto.seed_rows_in(ReviewMode::Rule, 1, 60_000, Some(ReviewDecision::Approve));
    let id = auto.request("lab-web", Some(SESSION));
    let (handle, _emitted) = auto.window(&options);
    let row = auto.row_of(&id);
    stop(handle);
    assert_eq!(row.cause, Some(EscalationCause::HourlyCap));
    assert_eq!(row.mode, ReviewMode::Rule);
}

/// Story 5.14 AC 3 (FR-8, review F36): in shadow mode too, `officina run`
/// never prints the review's reason, whatever it decides.
#[test]
fn run_output_never_holds_the_review_reason() {
    let options = Options {
        prompt: "Apply it. FAKE-REVIEW:reject".to_owned(),
        ..Options::default()
    };
    let auto = Auto::new(&options);
    let id = auto.request("lab-web", Some(SESSION));
    let (handle, _emitted) = auto.window(&options);
    let row = auto.row_of(&id);
    stop(handle);
    let reason = reason_of(&row);
    let waited = auto.run(&["apply", "group=lab-web", "--wait", "1"], Some(SESSION));
    decide_approval(
        &auto.relay.env.relay_home(),
        &id,
        false,
        Some(officina_core::registry::types::AuthorReason::NotNow),
    )
    .unwrap();
    let refused = auto.run(&["apply", "group=lab-web"], Some(SESSION));
    assert_ne!(refused.status.code(), Some(0), "{refused:?}");
    for output in [&waited, &refused] {
        for stream in [&output.stdout, &output.stderr] {
            assert!(
                !String::from_utf8_lossy(stream).contains(&reason),
                "{output:?}"
            );
        }
    }
}
