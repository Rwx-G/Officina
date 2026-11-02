//! The escape battery's vector set (story 6.10 AC 1 and 4, AD-117):
//! `tests/vectors/battery.json`'s `attempts` section is rebuilt from
//! `battery::ATTEMPTS` and must equal the file; its `judge` section names
//! edits of a passing run derived from the constant, with the judgement
//! pinned. `OFFICINA_WRITE_VECTORS=1` writes both for a reviewed change.

use serde_json::Value;

use officina_core::workshop::battery::{
    ATTEMPTS, Applies, AttemptLine, AttemptOutcome, BatteryFamily, BatteryLine, BatteryOutcome,
    Control, ControlLine, ControlStatus, GATEWAY_B_EXIT, JudgeInput, Judged, attempts_for, judge,
};
use officina_core::workshop::floor::CredentialKind;
use officina_core::workshop::gateway::GatewayRefusal;

const VECTORS: &str = include_str!("../../../tests/vectors/battery.json");

#[expect(
    clippy::disallowed_methods,
    reason = "a test's write mode for a reviewed change of the golden file"
)]
fn writing() -> bool {
    std::env::var("OFFICINA_WRITE_VECTORS").is_ok_and(|v| v == "1")
}

#[expect(
    clippy::disallowed_methods,
    reason = "a test's write mode for a reviewed change of the golden file"
)]
fn write(all: &Value) {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/vectors/battery.json"
    );
    std::fs::write(path, serde_json::to_string_pretty(all).unwrap() + "\n").unwrap();
}

fn vectors() -> Value {
    serde_json::from_str(VECTORS).expect("battery.json parses")
}

/// A passing run of `kind`: every control `ok`, every attempt `refused`,
/// each gateway-line attempt's cause logged in order, gateway B exiting 3.
fn passing(kind: CredentialKind, docker_desktop: bool) -> JudgeInput {
    let mut lines: Vec<BatteryLine> = BatteryFamily::ALL
        .iter()
        .filter_map(|family| {
            family.control().map(|control| {
                BatteryLine::Control(ControlLine {
                    family: *family,
                    control,
                    status: ControlStatus::Ok,
                })
            })
        })
        .collect();
    let mut refusals = Vec::new();
    for phase in [1, 2] {
        for attempt in attempts_for(kind, phase) {
            lines.push(BatteryLine::Attempt(AttemptLine {
                family: attempt.family,
                attempt: attempt.code.to_owned(),
                expected: attempt.expected,
                outcome: AttemptOutcome::Refused,
                detail: None,
            }));
            refusals.extend(attempt.expected);
        }
    }
    JudgeInput {
        kind,
        lines,
        refusals,
        gateway_b_exit: Some(GATEWAY_B_EXIT),
        docker_desktop,
    }
}

fn set_outcome(input: &mut JudgeInput, code: &str, outcome: AttemptOutcome) {
    for line in &mut input.lines {
        if let BatteryLine::Attempt(a) = line
            && a.attempt == code
        {
            a.outcome = outcome;
        }
    }
}

fn fail_control(input: &mut JudgeInput, family: BatteryFamily) {
    for line in &mut input.lines {
        if let BatteryLine::Control(c) = line
            && c.family == family
        {
            c.status = ControlStatus::Failed;
        }
    }
}

/// A `judge` case's input: the passing run edited as the case says.
fn case_input(case: &Value) -> JudgeInput {
    let kind = CredentialKind::from_code(case["kind"].as_str().unwrap()).unwrap();
    let mut input = passing(kind, case["docker_desktop"].as_bool().unwrap());
    for edit in case["outcomes"].as_object().into_iter().flatten() {
        let outcome = AttemptOutcome::from_code(edit.1.as_str().unwrap()).unwrap();
        set_outcome(&mut input, edit.0, outcome);
    }
    for family in case["failed_controls"].as_array().into_iter().flatten() {
        fail_control(
            &mut input,
            BatteryFamily::from_code(family.as_str().unwrap()).unwrap(),
        );
    }
    for cause in case["dropped_refusals"].as_array().into_iter().flatten() {
        let cause = GatewayRefusal::from_code(cause.as_str().unwrap()).unwrap();
        if let Some(at) = input.refusals.iter().position(|r| *r == cause) {
            input.refusals.remove(at);
        }
    }
    if !case["gateway_b_exit"].is_null() {
        input.gateway_b_exit = case["gateway_b_exit"]
            .as_i64()
            .map(|c| i32::try_from(c).unwrap());
    }
    input
}

fn judged(case: &Value) -> Value {
    let judgement = judge(&case_input(case));
    serde_json::json!({ "outcome": judgement.outcome, "failures": judgement.failures })
}

#[test]
fn the_attempts_section_is_the_constant() {
    let mut all = vectors();
    let rebuilt = serde_json::to_value(ATTEMPTS).unwrap();
    if writing() {
        all["attempts"] = rebuilt;
        write(&all);
        return;
    }
    assert_eq!(all["attempts"], rebuilt, "battery.json is stale");
}

#[test]
fn judge_cases_match() {
    let mut all = vectors();
    let cases = all["judge"].as_array().expect("a judge section").clone();
    assert!(!cases.is_empty());
    if writing() {
        all["judge"] = Value::Array(
            cases
                .into_iter()
                .map(|mut case| {
                    case["expect"] = judged(&case);
                    case
                })
                .collect(),
        );
        write(&all);
        return;
    }
    for case in &cases {
        assert_eq!(judged(case), case["expect"], "{}", case["name"]);
    }
}

fn planned(case: &Value) -> Value {
    use officina_core::workshop::battery::{
        DECOY_PORT, DecoyAddresses, PHASE_2_DEADLINE_S, PHASE_2_LIMIT, PHASE_2_MARGIN_S, Plan,
        expected_env, parse_plan, plan,
    };
    use officina_core::workshop::vector::GATEWAY_PORT;
    let strings = |v: &Value| -> Vec<String> {
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s.as_str().map(str::to_owned))
            .collect()
    };
    let input = &case["input"];
    let decoy = &input["decoy"];
    let built = Plan {
        gateway: input["gateway"].as_str().unwrap().to_owned(),
        gateway_port: GATEWAY_PORT,
        decoy: DecoyAddresses {
            own: strings(&decoy["own"]),
            host_docker_internal: strings(&decoy["host_docker_internal"]),
            gateway_docker_internal: strings(&decoy["gateway_docker_internal"]),
        },
        decoy_port: DECOY_PORT,
        mount: input["mount"].as_str().unwrap().to_owned(),
        login_mount: input["login_mount"].as_str().map(str::to_owned),
        expected_env: expected_env(
            &strings(&input["vector_env"]),
            &strings(&input["image_env"]),
        ),
        pids_limit: u32::try_from(input["pids_limit"].as_u64().unwrap()).unwrap(),
        memory_mib: u32::try_from(input["memory_mib"].as_u64().unwrap()).unwrap(),
        wait_s: PHASE_2_DEADLINE_S + PHASE_2_MARGIN_S,
        bad_requests: PHASE_2_LIMIT,
    };
    let args = plan(&built);
    assert_eq!(
        parse_plan(&args).as_ref(),
        Some(&built),
        "the plan parses back"
    );
    serde_json::json!(args)
}

#[test]
fn plan_cases_match() {
    let mut all = vectors();
    let cases = all["plan"].as_array().expect("a plan section").clone();
    assert!(!cases.is_empty());
    if writing() {
        all["plan"] = Value::Array(
            cases
                .into_iter()
                .map(|mut case| {
                    case["expect"] = planned(&case);
                    case
                })
                .collect(),
        );
        write(&all);
        return;
    }
    for case in &cases {
        assert_eq!(planned(case), case["expect"], "{}", case["name"]);
    }
}

#[test]
fn every_family_has_an_attempt() {
    for family in BatteryFamily::ALL {
        assert!(
            ATTEMPTS.iter().any(|a| a.family == *family),
            "{family:?} has no attempt"
        );
    }
}

#[test]
fn every_family_judged_from_a_failure_has_a_control() {
    for family in BatteryFamily::ALL {
        let from_failure = ATTEMPTS
            .iter()
            .any(|a| a.family == *family && a.judged != Judged::Fact);
        if from_failure {
            assert!(family.control().is_some(), "{family:?} has no control");
        }
    }
    for control in Control::ALL {
        assert!(
            BatteryFamily::ALL
                .iter()
                .any(|f| f.control() == Some(*control)),
            "{control:?} guards no family"
        );
    }
}

#[test]
fn every_attempt_has_a_case() {
    for kind in CredentialKind::ALL {
        for docker_desktop in [true, false] {
            assert_eq!(
                judge(&passing(*kind, docker_desktop)).outcome,
                BatteryOutcome::Passed
            );
        }
        for phase in [1, 2] {
            for attempt in attempts_for(*kind, phase) {
                let mut input = passing(*kind, true);
                set_outcome(&mut input, attempt.code, AttemptOutcome::Escaped);
                let judgement = judge(&input);
                assert_eq!(
                    judgement.outcome,
                    BatteryOutcome::Failed,
                    "{}",
                    attempt.code
                );
                assert_eq!(judgement.failures.len(), 1, "{}", attempt.code);
            }
        }
    }
}

#[test]
fn login_attempts_are_played_on_a_login_floor_only() {
    let login = judge(&passing(CredentialKind::Login, true)).totals.attempts;
    let token = judge(&passing(CredentialKind::OauthToken, true))
        .totals
        .attempts;
    assert!(login > token);
}

#[test]
fn one_escape_fails() {
    let mut input = passing(CredentialKind::OauthToken, true);
    set_outcome(&mut input, ATTEMPTS[0].code, AttemptOutcome::Escaped);
    let judgement = judge(&input);
    assert_eq!(judgement.outcome, BatteryOutcome::Failed);
    assert_eq!(judgement.totals.escaped, 1);
}

#[test]
fn one_inconclusive_fails() {
    let mut input = passing(CredentialKind::OauthToken, true);
    let code = ATTEMPTS.last().unwrap().code;
    input
        .lines
        .retain(|l| !matches!(l, BatteryLine::Attempt(a) if a.attempt == code));
    let judgement = judge(&input);
    assert_eq!(judgement.outcome, BatteryOutcome::Failed);
    assert_eq!(judgement.totals.inconclusive, 1, "a missing line");
}

#[test]
fn a_failed_control_is_inconclusive() {
    for family in BatteryFamily::ALL.iter().filter(|f| f.control().is_some()) {
        let mut input = passing(CredentialKind::OauthToken, true);
        fail_control(&mut input, *family);
        let judgement = judge(&input);
        assert_eq!(judgement.outcome, BatteryOutcome::Failed, "{family:?}");
        let family_attempts = judgement
            .attempts
            .iter()
            .filter(|a| a.family == *family)
            .count();
        assert!(family_attempts > 0);
        assert!(
            judgement
                .attempts
                .iter()
                .filter(|a| a.family == *family)
                .all(|a| a.outcome == AttemptOutcome::Inconclusive
                    || ATTEMPTS
                        .iter()
                        .any(|x| x.code == a.attempt && x.judged == Judged::Fact)),
            "{family:?}"
        );
    }
}

#[test]
fn an_unmatched_gateway_attempt_fails() {
    for attempt in ATTEMPTS.iter().filter(|a| a.expected.is_some()) {
        let mut input = passing(CredentialKind::OauthToken, true);
        let cause = attempt.expected.unwrap();
        input.refusals.retain(|r| *r != cause);
        assert_eq!(
            judge(&input).outcome,
            BatteryOutcome::Failed,
            "{}",
            attempt.code
        );
    }
    let mut reordered = passing(CredentialKind::OauthToken, true);
    reordered.refusals.reverse();
    assert_eq!(
        judge(&reordered).outcome,
        BatteryOutcome::Failed,
        "in order"
    );
}

#[test]
fn gateway_b_not_exiting_3_fails() {
    for exit in [None, Some(0), Some(1), Some(GATEWAY_B_EXIT + 1)] {
        let mut input = passing(CredentialKind::OauthToken, true);
        input.gateway_b_exit = exit;
        assert_eq!(judge(&input).outcome, BatteryOutcome::Failed, "{exit:?}");
    }
}

#[test]
fn not_applicable_fails_on_docker_desktop() {
    for attempt in ATTEMPTS.iter().filter(|a| a.docker_desktop_only) {
        let mut on = passing(CredentialKind::OauthToken, true);
        set_outcome(&mut on, attempt.code, AttemptOutcome::NotApplicable);
        assert_eq!(
            judge(&on).outcome,
            BatteryOutcome::Failed,
            "{}",
            attempt.code
        );
        let mut off = passing(CredentialKind::OauthToken, false);
        set_outcome(&mut off, attempt.code, AttemptOutcome::NotApplicable);
        assert_eq!(
            judge(&off).outcome,
            BatteryOutcome::Passed,
            "{}",
            attempt.code
        );
    }
    for attempt in ATTEMPTS
        .iter()
        .filter(|a| !a.docker_desktop_only && a.applies == Applies::Always)
    {
        let mut off = passing(CredentialKind::OauthToken, false);
        set_outcome(&mut off, attempt.code, AttemptOutcome::NotApplicable);
        assert_eq!(
            judge(&off).outcome,
            BatteryOutcome::Failed,
            "{}",
            attempt.code
        );
    }
}
