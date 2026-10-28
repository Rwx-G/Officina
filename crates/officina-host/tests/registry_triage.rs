//! The triage calls the registry records and the dry run it drives (story
//! 3.9; AD-51, AD-52, D-65, D-67, D-81, D-85, D-87, D-88), at explicit
//! `now_ms`, with each call's result made here: no process starts. The
//! runner itself, with the fake claude, is the `officina` crate's
//! `tests/triage.rs`.

mod common;

use common::registry::{DAY, Home, NOW, answer, day_file, prompt};
use officina_core::registry::confirm::{ConfirmItem, FileUnder};
use officina_core::registry::dry_run::{DryRunStop, Evaluation};
use officina_core::registry::triage_call::{
    Budget, BudgetLimit, CALL_SPACING_MS, CALLS_PER_HOUR, CallAnswer, CallUsage, TriageCase,
    failed_proposal_streak,
};
use officina_core::registry::types::{
    ModelVerdict, SettingKey, Source, TriageOutcome, TriagePurpose, TriageRefusal, Verdict,
};
use officina_host::registry::store::{Writer, open_reader};
use officina_host::registry::triage::{CallResult, DryRunRefusal, NextCall, Triage};

const CWD: &str = "C:/work/api";
const HOUR: i64 = 3_600_000;

/// A session that saw `text` as its prompt, answered and ended normally,
/// from `at` on; or, `cut`, one that ended right after a prompt.
fn session_lines(session: &str, at: i64, text: &str, cut: bool) -> Vec<String> {
    let mut lines = vec![prompt(session, at, CWD, text)];
    if !cut {
        lines.push(answer(session, at + 1_000, CWD, "Done.", None));
    }
    lines.push(format!(
        "{}\n",
        serde_json::json!({"v": 1, "ts": at + 2_000, "event": "SessionEnd", "session_id": session, "cwd": CWD, "reason": "prompt_input_exit"})
    ));
    lines
}

/// Confirms the unfiled row of `session` at `at`, filed under a new subject
/// named after it, with `verdict`; returns the proposal it answered.
fn confirm(writer: &mut Writer, home: &Home, session: &str, verdict: Verdict, at: i64) {
    let view = writer.view(None, &home.events, at).unwrap();
    let key = format!("session:{session}");
    let row = view
        .triage
        .iter()
        .find(|r| r.key == key)
        .unwrap_or_else(|| panic!("no row {key}"));
    let answers = writer
        .confirm_triage(
            &[ConfirmItem {
                key,
                shown: row.proposal,
                verdict,
                routine: false,
                file_under: Some(FileUnder::New {
                    name: session.to_owned(),
                }),
            }],
            at,
        )
        .unwrap();
    assert!(
        matches!(
            answers[0].outcome,
            officina_core::registry::confirm::ItemOutcome::Confirmed { .. }
        ),
        "{answers:?}"
    );
}

/// `sessions` ended a day before `NOW`, ingested, each confirmed with its
/// verdict a minute apart in the order given; `cut` ones were proposed by
/// rule, the others by the default.
fn confirmed(home: &Home, sessions: &[(&str, Verdict, bool)]) -> Writer {
    let start = NOW - DAY;
    let mut lines = Vec::new();
    for (i, (session, _, cut)) in sessions.iter().enumerate() {
        lines.extend(session_lines(
            session,
            start + i as i64 * 10_000,
            &format!("work of {session}"),
            *cut,
        ));
    }
    home.append(&day_file(start), &lines);
    let mut writer = home.writer(start);
    writer.ingest(&home.ctx(), NOW - HOUR).unwrap();
    for (i, (session, verdict, _)) in sessions.iter().enumerate() {
        confirm(
            &mut writer,
            home,
            session,
            *verdict,
            NOW - HOUR + i as i64 * 60_000,
        );
    }
    writer
}

/// The result of `next` at `at`: answered `verdict`, or with `outcome`.
fn result(
    next: &NextCall,
    at: i64,
    outcome: TriageOutcome,
    verdict: Option<ModelVerdict>,
) -> CallResult {
    CallResult {
        purpose: next.purpose,
        case_key: next.case_key.clone(),
        input_hash: next.case.input_hash(),
        started_at_ms: at,
        duration_ms: 800,
        outcome,
        refusal: (outcome == TriageOutcome::Refused).then_some(TriageRefusal::CmdShim),
        answer: verdict.map(|verdict| CallAnswer {
            verdict,
            reason: "a reason".into(),
        }),
        usage: if outcome == TriageOutcome::Refused {
            CallUsage::default()
        } else {
            CallUsage {
                input_tokens: Some(4_500),
                output_tokens: Some(380),
                cost_microusd: Some(1_100),
            }
        },
        detail: None,
    }
}

/// Runs the dry run of `triage` to its end from `at`, each case answered
/// by `answer_of` its prompt, one call per spacing; returns the cases'
/// prompts in the order sent.
fn run_dry_run(
    triage: &mut Triage,
    writer: &mut Writer,
    mut at: i64,
    answer_of: &dyn Fn(&TriageCase) -> (TriageOutcome, Option<ModelVerdict>),
) -> Vec<String> {
    let mut sent = Vec::new();
    while triage.running() {
        let Some(next) = triage.next_call(writer, at).unwrap() else {
            assert!(
                !triage.running(),
                "a case may start once the spacing passed"
            );
            break;
        };
        sent.push(next.case.last_prompt.clone().unwrap_or_default());
        let (outcome, verdict) = answer_of(&next.case);
        triage
            .take_result(writer, &result(&next, at, outcome, verdict), at + 900)
            .unwrap();
        at += CALL_SPACING_MS;
    }
    sent
}

fn latest(home: &Home) -> Option<Evaluation> {
    open_reader(&home.home, &[])
        .unwrap()
        .latest_evaluation()
        .unwrap()
}

/// AD-52, D-85: the dry run takes the confirmations whose proposal was the
/// default and whose input is still held, the newest first.
#[test]
fn dry_run_selects_held_default_confirmations_newest_first() {
    let home = Home::new();
    let mut writer = confirmed(
        &home,
        &[
            ("a", Verdict::ToResume, false),
            ("b", Verdict::Finished, false),
            ("cut", Verdict::ToResume, true),
            ("c", Verdict::Finished, false),
        ],
    );
    let mut triage = Triage::default();
    assert_eq!(triage.start_dry_run(&writer, NOW).unwrap(), Ok(3));
    let sent = run_dry_run(&mut triage, &mut writer, NOW, &|_| {
        (TriageOutcome::Answer, Some(ModelVerdict::Finished))
    });
    assert_eq!(sent, ["work of c", "work of b", "work of a"]);
    // The inputs expire with the excerpts they copy (D-85): past that, the
    // set is empty, by construction under 30 days old.
    let mut triage = Triage::default();
    let expiry = NOW - DAY + officina_core::registry::retention::EXCERPT_TTL_MS + 30_000;
    assert_eq!(
        triage.start_dry_run(&writer, expiry).unwrap(),
        Err(DryRunRefusal::NoCases)
    );
}

/// AD-52, D-59, D-67, D-88: one `model_evaluation` row with the counts and
/// the calls' cost, and one `triage_call` row per case.
#[test]
fn dry_run_records_one_evaluation_with_its_cost() {
    let home = Home::new();
    let mut writer = confirmed(
        &home,
        &[
            ("right-finished", Verdict::Finished, false),
            ("false-finished", Verdict::ToResume, false),
            ("unsure", Verdict::ToResume, false),
            ("timeout", Verdict::Finished, false),
        ],
    );
    let mut triage = Triage::default();
    triage.start_dry_run(&writer, NOW).unwrap().unwrap();
    run_dry_run(
        &mut triage,
        &mut writer,
        NOW,
        &|case| match case.last_prompt.as_deref().unwrap() {
            "work of right-finished" | "work of false-finished" => {
                (TriageOutcome::Answer, Some(ModelVerdict::Finished))
            }
            "work of unsure" => (TriageOutcome::Answer, Some(ModelVerdict::Unsure)),
            _ => (TriageOutcome::Timeout, None),
        },
    );
    let evaluation = latest(&home).expect("one evaluation");
    let tally = evaluation.tally;
    assert_eq!(
        (
            tally.cases,
            tally.model_correct,
            tally.default_correct,
            tally.model_false_finished,
            tally.unanswered
        ),
        (4, 1, 2, 1, 1)
    );
    assert_eq!(
        (tally.input_tokens, tally.output_tokens, tally.cost_microusd),
        (Some(4 * 4_500), Some(4 * 380), Some(4 * 1_100))
    );
    assert_eq!(triage.state().last_stop, Some(DryRunStop::Completed));
    assert!(triage.state().running.is_none());
    let calls = writer.triage_calls_since(0).unwrap();
    assert_eq!(calls.len(), 4);
    assert!(calls.iter().all(|c| c.purpose == TriagePurpose::DryRun));
}

/// D-81: a dry run runs while the model is disabled, and while it is
/// paused.
#[test]
fn dry_run_runs_while_disabled_or_paused() {
    let home = Home::new();
    let mut writer = confirmed(&home, &[("a", Verdict::ToResume, false)]);
    writer
        .set_setting(SettingKey::TriageModel, "false", Source::Author, NOW)
        .unwrap();
    writer
        .set_setting(
            SettingKey::TriageModelPause,
            r#"{"reason":"three failed calls"}"#,
            Source::Rules,
            NOW,
        )
        .unwrap();
    let mut triage = Triage::default();
    assert_eq!(triage.start_dry_run(&writer, NOW).unwrap(), Ok(1));
    run_dry_run(&mut triage, &mut writer, NOW, &|_| {
        (TriageOutcome::Answer, Some(ModelVerdict::ToResume))
    });
    assert_eq!(latest(&home).unwrap().tally.model_correct, 1);
}

/// D-81: a dry run's failed calls are dry-run rows; the streak the pause
/// reads counts proposal calls only.
#[test]
fn dry_run_failures_do_not_count_toward_the_pause() {
    let home = Home::new();
    let mut writer = confirmed(
        &home,
        &[
            ("a", Verdict::ToResume, false),
            ("b", Verdict::ToResume, false),
            ("c", Verdict::ToResume, false),
            ("d", Verdict::ToResume, false),
        ],
    );
    let mut triage = Triage::default();
    triage.start_dry_run(&writer, NOW).unwrap().unwrap();
    run_dry_run(&mut triage, &mut writer, NOW, &|_| {
        (TriageOutcome::Invalid, None)
    });
    let calls = writer.triage_calls_since(0).unwrap();
    assert_eq!(calls.len(), 4);
    assert!(calls.iter().all(|c| c.outcome == TriageOutcome::Invalid));
    assert_eq!(failed_proposal_streak(&calls), 0);
    assert_eq!(latest(&home).unwrap().tally.unanswered, 4);
}

/// AD-52, D-67: the budget spent stops the dry run, and what ran is
/// recorded.
#[test]
fn dry_run_stops_when_the_budget_is_spent() {
    let home = Home::new();
    let mut writer = confirmed(
        &home,
        &[
            ("a", Verdict::ToResume, false),
            ("b", Verdict::ToResume, false),
            ("c", Verdict::ToResume, false),
        ],
    );
    // An hour already holding all its calls but one.
    let spent = NextCall {
        purpose: TriagePurpose::DryRun,
        case_key: "confirmation:0".into(),
        case: TriageCase::default(),
    };
    for i in 0..CALLS_PER_HOUR as i64 - 1 {
        let at = NOW - HOUR + 60_000 + i * 50_000;
        writer
            .record_call(&result(
                &spent,
                at,
                TriageOutcome::Answer,
                Some(ModelVerdict::Unsure),
            ))
            .unwrap();
    }
    let mut triage = Triage::default();
    triage.start_dry_run(&writer, NOW).unwrap().unwrap();
    let ran = run_dry_run(&mut triage, &mut writer, NOW, &|_| {
        (TriageOutcome::Answer, Some(ModelVerdict::ToResume))
    });
    assert_eq!(ran.len(), 1, "one call left in the hour");
    assert_eq!(triage.state().last_stop, Some(DryRunStop::BudgetSpent));
    let evaluation = latest(&home).unwrap();
    assert_eq!(
        (evaluation.tally.cases, evaluation.tally.model_correct),
        (1, 1)
    );
}

/// D-67: a second dry run while one runs is refused, and the first goes
/// on.
#[test]
fn second_dry_run_is_refused_while_one_runs() {
    let home = Home::new();
    let mut writer = confirmed(&home, &[("a", Verdict::ToResume, false)]);
    let mut triage = Triage::default();
    assert_eq!(triage.start_dry_run(&writer, NOW).unwrap(), Ok(1));
    assert_eq!(
        triage.start_dry_run(&writer, NOW).unwrap(),
        Err(DryRunRefusal::Running)
    );
    let progress = triage.state().running.unwrap();
    assert_eq!((progress.done, progress.total), (0, 1));
    run_dry_run(&mut triage, &mut writer, NOW, &|_| {
        (TriageOutcome::Answer, Some(ModelVerdict::ToResume))
    });
    assert_eq!(latest(&home).unwrap().tally.cases, 1);
    assert_eq!(triage.start_dry_run(&writer, NOW + DAY).unwrap(), Ok(1));
}

/// AD-52, D-65: the budget is read from the rows, so a new writer, as a
/// restarted window opens, holds it.
#[test]
fn budget_holds_across_a_restart() {
    let home = Home::new();
    let mut writer = home.writer(NOW);
    let call = NextCall {
        purpose: TriagePurpose::DryRun,
        case_key: "confirmation:1".into(),
        case: TriageCase::default(),
    };
    for i in 0..CALLS_PER_HOUR as i64 {
        writer
            .record_call(&result(
                &call,
                NOW - 50 * 60_000 + i * 30_000,
                TriageOutcome::Timeout,
                None,
            ))
            .unwrap();
    }
    drop(writer);
    let reopened = home.writer(NOW);
    assert!(matches!(
        reopened.budget(NOW).unwrap(),
        Budget::Spent {
            limit: BudgetLimit::Hour,
            ..
        }
    ));
    assert_eq!(reopened.budget(NOW + HOUR).unwrap(), Budget::Open);
}

/// AD-52: calls, tokens and cost today and over 30 days reach the view.
#[test]
fn counts_cover_today_and_30_days() {
    let home = Home::new();
    let mut writer = home.writer(NOW);
    let call = NextCall {
        purpose: TriagePurpose::DryRun,
        case_key: "confirmation:1".into(),
        case: TriageCase::default(),
    };
    for (at, outcome) in [
        (NOW - HOUR, TriageOutcome::Answer),
        (NOW - 2 * HOUR, TriageOutcome::Failed),
        (NOW - 10 * DAY, TriageOutcome::Answer),
        (NOW - 31 * DAY, TriageOutcome::Answer),
    ] {
        writer
            .record_call(&result(&call, at, outcome, None))
            .unwrap();
    }
    let view = writer.view(None, &home.events, NOW).unwrap();
    let calls = &view.model.calls;
    assert_eq!(
        (
            calls.today.calls,
            calls.today.unanswered,
            calls.today.cost_microusd
        ),
        (2, 1, 2 * 1_100)
    );
    assert_eq!(
        (calls.last_30_days.calls, calls.last_30_days.input_tokens),
        (3, 3 * 4_500)
    );
    assert!(!view.model.enabled, "the model stays disabled (story 3.11)");
}

/// D-65: a refusal is one row, counted with its cause, that spends no
/// budget.
#[test]
fn refusals_spend_no_budget_and_are_counted() {
    let home = Home::new();
    let mut writer = home.writer(NOW);
    let call = NextCall {
        purpose: TriagePurpose::DryRun,
        case_key: "confirmation:1".into(),
        case: TriageCase::default(),
    };
    writer
        .record_call(&result(&call, NOW - 1_000, TriageOutcome::Refused, None))
        .unwrap();
    assert_eq!(writer.budget(NOW).unwrap(), Budget::Open);
    let view = writer.view(None, &home.events, NOW).unwrap();
    assert_eq!(
        (view.model.calls.today.calls, view.model.calls.today.refused),
        (0, 1)
    );
    assert_eq!(view.model.calls.refusals.len(), 1);
    assert_eq!(view.model.calls.refusals[0].cause, TriageRefusal::CmdShim);
    // A dry run ends on its first refusal: what refused it holds for every
    // case, and nothing ran, so no evaluation is recorded.
    let mut writer = confirmed(
        &home,
        &[
            ("a", Verdict::ToResume, false),
            ("b", Verdict::ToResume, false),
        ],
    );
    let mut triage = Triage::default();
    triage.start_dry_run(&writer, NOW).unwrap().unwrap();
    let next = triage.next_call(&mut writer, NOW).unwrap().unwrap();
    triage
        .take_result(
            &mut writer,
            &result(&next, NOW, TriageOutcome::Refused, None),
            NOW,
        )
        .unwrap();
    assert!(!triage.running());
    assert_eq!(triage.state().last_stop, Some(DryRunStop::Refused));
    assert!(latest(&home).is_none());
    assert_eq!(writer.budget(NOW + 1).unwrap(), Budget::Open);
}

/// D-87: a case whose last call was refused is not queued again until its
/// inputs change, also after a restart.
#[test]
fn a_refused_row_is_not_queued_again_until_its_inputs_change() {
    let home = Home::new();
    let mut writer = home.writer(NOW);
    let case = TriageCase {
        last_prompt: Some("rotate the logs".into()),
        end: Some("logout".into()),
        ..TriageCase::default()
    };
    let key = "session:s1";
    assert!(writer.may_queue(key, &case).unwrap(), "never called");
    let refused = NextCall {
        purpose: TriagePurpose::Proposal,
        case_key: key.into(),
        case: case.clone(),
    };
    writer
        .record_call(&result(&refused, NOW, TriageOutcome::Refused, None))
        .unwrap();
    assert!(!writer.may_queue(key, &case).unwrap());
    drop(writer);
    let writer = home.writer(NOW);
    assert!(
        !writer.may_queue(key, &case).unwrap(),
        "held across a restart"
    );
    let changed = TriageCase {
        answer_end: Some("...and compressed.".into()),
        ..case.clone()
    };
    assert!(writer.may_queue(key, &changed).unwrap(), "new inputs");
    assert!(
        writer.may_queue("session:s2", &case).unwrap(),
        "another row"
    );
}

/// D-85: a case whose input expires while it waits in a running dry run is
/// never sent; the run ends with what it sent.
#[test]
fn a_case_whose_input_expired_while_queued_is_not_sent() {
    use officina_core::registry::retention::EXCERPT_TTL_MS;
    let home = Home::new();
    let mut writer = confirmed(
        &home,
        &[
            ("old", Verdict::ToResume, false),
            ("new", Verdict::ToResume, false),
        ],
    );
    // Both inputs expire from their session's first excerpt, ten seconds
    // apart; the run reaches the older case after its expiry.
    let start = NOW - DAY;
    let older_expiry = start + EXCERPT_TTL_MS;
    let mut triage = Triage::default();
    assert_eq!(
        triage.start_dry_run(&writer, older_expiry - 1).unwrap(),
        Ok(2)
    );
    let first = triage
        .next_call(&mut writer, older_expiry - 1)
        .unwrap()
        .unwrap();
    assert_eq!(first.case.last_prompt.as_deref(), Some("work of new"));
    triage
        .take_result(
            &mut writer,
            &result(
                &first,
                older_expiry - 1,
                TriageOutcome::Answer,
                Some(ModelVerdict::ToResume),
            ),
            older_expiry,
        )
        .unwrap();
    assert_eq!(triage.state().running.map(|p| p.total), Some(2));
    assert_eq!(
        triage
            .next_call(&mut writer, older_expiry + CALL_SPACING_MS)
            .unwrap(),
        None
    );
    assert!(!triage.running());
    assert_eq!(triage.state().last_stop, Some(DryRunStop::Completed));
    assert_eq!(
        writer.triage_calls_since(0).unwrap().len(),
        1,
        "the expired case was not sent"
    );
    assert_eq!(latest(&home).unwrap().tally.cases, 1);
}

/// Quality audit H1: when the `triage` thread is gone with a case out, the
/// dry run ends with what ran recorded instead of waiting forever, and no
/// other dry run is accepted.
#[test]
fn a_gone_worker_ends_the_dry_run_and_refuses_another() {
    let home = Home::new();
    let mut writer = confirmed(
        &home,
        &[
            ("first", Verdict::Finished, false),
            ("second", Verdict::ToResume, false),
        ],
    );
    let mut triage = Triage::default();
    triage.start_dry_run(&writer, NOW).unwrap().unwrap();
    let first = triage.next_call(&mut writer, NOW).unwrap().unwrap();
    triage
        .take_result(
            &mut writer,
            &result(
                &first,
                NOW,
                TriageOutcome::Answer,
                Some(ModelVerdict::Finished),
            ),
            NOW + 900,
        )
        .unwrap();
    let at = NOW + CALL_SPACING_MS;
    assert!(
        triage.next_call(&mut writer, at).unwrap().is_some(),
        "a case out"
    );
    assert!(triage.running());
    triage.worker_gone(&mut writer, at + 1).unwrap();
    assert!(!triage.running());
    assert_eq!(triage.state().last_stop, Some(DryRunStop::WorkerGone));
    assert_eq!(latest(&home).expect("what ran is recorded").tally.cases, 1);
    assert_eq!(
        triage.start_dry_run(&writer, at + 2).unwrap(),
        Err(DryRunRefusal::NoWorker)
    );
    assert!(triage.next_call(&mut writer, at + DAY).unwrap().is_none());
}
