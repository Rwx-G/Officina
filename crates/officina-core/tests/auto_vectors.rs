//! Replays the hand-written vector set of the `auto` review (AD-86 to
//! AD-88, story 5.13): the precheck, the prompt's data line, a call's
//! output and the rule baseline; and refuses a set that leaves a cause
//! without a case.

use std::collections::{BTreeMap, BTreeSet};

use officina_core::registry::triage_call::{NONCE_LEN, PromptError};
use officina_core::registry::types::ReviewOutcome;
use officina_core::relay::approvals::{
    ApprovalContext, ApprovalRequest, KeyInputs, request_id, request_key,
};
use officina_core::relay::auto::{
    CheckStatus, EscalationCause, GATE_CASES, Precheck, PrecheckFacts, RULE_DIFF_PERCENT,
    ReviewContext, ReviewJob, ReviewRecord, StatsRecord, data_line, decide, output_error_cause,
    precheck, precheck_rule, prompt, read_output, rule_baseline, rule_decision, shadow_stats,
};
use officina_core::relay::catalogue::{ApprovedCatalogue, load};
use officina_core::relay::journal::{
    JournalEntry, JournalLine, RunFinished, RunOutcome, RunStarted, StreamSummary, TreeFingerprint,
};
use officina_core::relay::template::ModelAlias;
use officina_core::untrusted::Untrusted;
use serde_json::Value;

const VECTORS: &str = include_str!("../../../tests/vectors/auto.json");
const FLOOR_PATH: &str = "C:/path/to/officina.toml";
const NONCE: &str = "0f1e2d3c4b5a6978";

struct Set {
    raw: Value,
    approved: ApprovedCatalogue,
    now_ms: i64,
}

fn load_set() -> Set {
    let raw: Value = serde_json::from_str(VECTORS).expect("the auto vector set parses");
    assert_eq!(raw["schema"], 1);
    let relay = load(raw["floor"].as_str().unwrap(), FLOOR_PATH).unwrap();
    assert!(
        relay.catalogue.issues.is_empty(),
        "{:?}",
        relay.catalogue.issues
    );
    let now_ms = raw["now_ms"].as_i64().unwrap();
    Set {
        approved: ApprovedCatalogue::approving(&relay, FLOOR_PATH, now_ms - 3_600_000),
        raw,
        now_ms,
    }
}

fn dirs() -> BTreeMap<String, String> {
    BTreeMap::from([("lab".to_owned(), "C:/path/to/Lab".to_owned())])
}

const SESSION: &str = "00000000-0000-0000-0000-000000000001";

/// The request `officina run <action> group=lab-web` writes, its command
/// rebuilt from the catalogue unless `argv` overrides it.
fn request(
    set: &Set,
    action: &str,
    session: Option<&str>,
    argv: Option<Vec<String>>,
) -> (String, ApprovalRequest) {
    let params = BTreeMap::from([("group".to_owned(), "lab-web".to_owned())]);
    let check_run_id = (action == "apply").then(|| "19a2b3c4d5e-1f".to_owned());
    let key = request_key(&KeyInputs {
        project: "lab",
        action,
        params: &params,
        catalogue_hash: &set.approved.hash,
        check_run_id: check_run_id.as_deref(),
        tree: None,
    });
    let program = "C:/path/to/tool.exe".to_owned();
    let argv = argv.unwrap_or_else(|| match action {
        "apply" => vec![program, "--apply".into(), "lab-web".into()],
        _ => vec![program, "lab-web".into()],
    });
    let created_at_ms = set.now_ms - 60_000;
    (
        request_id(&key, created_at_ms),
        ApprovalRequest {
            request_key: key,
            created_at_ms,
            project: "lab".to_owned(),
            action: action.to_owned(),
            params,
            argv,
            http: None,
            catalogue_hash: set.approved.hash.clone(),
            session_id: session.map(str::to_owned),
            check_run_id,
            tree: None,
        },
    )
}

/// The rows a case's facts name, or `calls_in_last_hour` calls spread over
/// the last hour.
fn rows(set: &Set, facts: &Value) -> Vec<ReviewRecord> {
    if let Some(calls) = facts.get("calls_in_last_hour").and_then(Value::as_i64) {
        return (0..calls)
            .map(|i| ReviewRecord {
                project: "lab".into(),
                action: "check".into(),
                requested_at_ms: set.now_ms - 60_000 * (i + 1) / 2,
                outcome: ReviewOutcome::Answered,
                decision: None,
                call_started_at_ms: Some(set.now_ms - 60_000 * (i + 1) / 2),
            })
            .collect();
    }
    facts
        .get("rows")
        .map(|rows| serde_json::from_value(rows.clone()).expect("rows parse"))
        .unwrap_or_default()
}

/// The facts of a case: the defaults that let a request be reviewed, each
/// overridden by the case.
fn facts<'a>(set: &Set, raw: &'a Value, rows: &'a [ReviewRecord]) -> PrecheckFacts<'a> {
    let text = |key: &str, default: Option<&'a str>| match raw.get(key) {
        Some(Value::Null) => None,
        Some(value) => value.as_str(),
        None => default,
    };
    PrecheckFacts {
        claude: text("claude", Some("C:/bin/claude.exe")),
        workdir_empty: raw
            .get("workdir_empty")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        no_excerpts: raw
            .get("no_excerpts")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        five_hour_used: match raw.get("five_hour_used") {
            Some(value) => value.as_f64(),
            None => Some(10.0),
        },
        rows,
        review_model: match raw.get("review_model") {
            Some(Value::Null) => None,
            Some(value) => value.as_str().and_then(ModelAlias::from_code),
            None => Some(ModelAlias::Haiku),
        },
        now_ms: set.now_ms,
    }
}

fn outcome_code(found: &Precheck) -> String {
    match found {
        Precheck::NotAuto => "not_auto".to_owned(),
        Precheck::Escalate(cause) => cause.code().to_owned(),
        Precheck::Review(_) => "review".to_owned(),
    }
}

fn cases<'a>(set: &'a Set, section: &str) -> &'a Vec<Value> {
    set.raw[section].as_array().expect("a section")
}

#[test]
fn precheck_cases_match() {
    let set = load_set();
    let dirs = dirs();
    let ctx = ApprovalContext {
        approved: Some(&set.approved),
        project_dirs: &dirs,
        system_dir: "C:/Windows/System32",
    };
    for case in cases(&set, "precheck") {
        let name = case["name"].as_str().unwrap();
        let session = match case.get("session_id") {
            Some(Value::Null) => None,
            Some(value) => value.as_str(),
            None => Some(SESSION),
        };
        let argv = case
            .get("argv")
            .map(|argv| serde_json::from_value(argv.clone()).unwrap());
        let (id, request) = request(&set, case["action"].as_str().unwrap(), session, argv);
        let rows = rows(&set, &case["facts"]);
        let found = precheck(&ctx, &id, &request, &facts(&set, &case["facts"], &rows));
        assert_eq!(outcome_code(&found), case["expect"], "case {name}");
    }
}

/// AD-87's order: with every precheck cause holding at once, the first in
/// `EscalationCause::ALL` wins; lift it and the next one does, through the
/// last.
#[test]
fn causes_are_checked_in_their_order() {
    let set = load_set();
    let dirs = dirs();
    let ctx = ApprovalContext {
        approved: Some(&set.approved),
        project_dirs: &dirs,
        system_dir: "C:/Windows/System32",
    };
    let order: Vec<EscalationCause> = EscalationCause::ALL
        .iter()
        .copied()
        .filter(|c| c.before_any_call())
        .collect();
    let failing = |cause: EscalationCause| -> (&'static str, Value) {
        match cause {
            EscalationCause::ClaudeNotFound => ("claude", Value::Null),
            EscalationCause::ClaudeShim => ("claude", "C:/npm/claude.cmd".into()),
            EscalationCause::WorkdirNotEmpty => ("workdir_empty", false.into()),
            EscalationCause::NoSessionId => ("session_id", Value::Null),
            EscalationCause::NoExcerpts => ("no_excerpts", true.into()),
            EscalationCause::QuotaHigh => ("five_hour_used", 95.0.into()),
            EscalationCause::HourlyCap => ("cap", true.into()),
            EscalationCause::BudgetSpent => ("calls_in_last_hour", 60.into()),
            EscalationCause::ReviewConfigInvalid => ("review_model", Value::Null),
            other => panic!("{other:?} is not a precheck cause"),
        }
    };
    for (at, expected) in order.iter().enumerate() {
        // Every cause from `at` on holds; the shim and not-found causes
        // share a key, so the earlier one wins it.
        let mut raw = serde_json::Map::new();
        let mut session = Some(SESSION);
        let mut cap_rows = Vec::new();
        for cause in order[at..].iter().rev() {
            let (key, value) = failing(*cause);
            match key {
                "session_id" => session = None,
                "cap" => {
                    cap_rows = (0..2)
                        .map(|_| ReviewRecord {
                            project: "lab".into(),
                            action: "apply".into(),
                            requested_at_ms: set.now_ms - 1_000,
                            outcome: ReviewOutcome::NoCall,
                            decision: Some(officina_core::relay::auto::ReviewDecision::Approve),
                            call_started_at_ms: None,
                        })
                        .collect();
                }
                _ => {
                    raw.insert(key.to_owned(), value);
                }
            }
        }
        let raw = Value::Object(raw);
        let mut rows = rows(&set, &raw);
        rows.extend(cap_rows);
        let (id, request) = request(&set, "apply", session, None);
        let found = precheck(&ctx, &id, &request, &facts(&set, &raw, &rows));
        assert_eq!(found, Precheck::Escalate(*expected), "from {expected:?} on");
    }
}

/// A job of the set's `auto` request, through the precheck.
fn job(set: &Set) -> ReviewJob {
    let dirs = dirs();
    let ctx = ApprovalContext {
        approved: Some(&set.approved),
        project_dirs: &dirs,
        system_dir: "C:/Windows/System32",
    };
    let (id, request) = request(set, "apply", Some(SESSION), None);
    let rows = Vec::new();
    match precheck(&ctx, &id, &request, &facts(set, &Value::Null, &rows)) {
        Precheck::Review(job) => *job,
        other => panic!("{other:?}"),
    }
}

#[test]
fn prompt_cases_match() {
    let set = load_set();
    let job = job(&set);
    assert_eq!(NONCE.len(), NONCE_LEN);
    for case in cases(&set, "prompt") {
        let name = case["name"].as_str().unwrap();
        let context = ReviewContext {
            subject: Some("lab upkeep".into()),
            last_prompt: Some(Untrusted::new(
                case["last_prompt"].as_str().unwrap().to_owned(),
            )),
            outcomes: Vec::new(),
            check: CheckStatus {
                name: "check".into(),
                satisfied: true,
                ran_at_ms: None,
            },
        };
        let built = prompt(&job, &context, NONCE);
        if case["refused"] == true {
            assert_eq!(built, Err(PromptError::NonceInInput), "case {name}");
            continue;
        }
        assert!(built.is_ok(), "case {name}");
        let line = data_line(&job, &context);
        assert!(!line.contains('\n'), "case {name}: one line");
        for needle in case["data_contains"].as_array().unwrap() {
            assert!(
                line.contains(needle.as_str().unwrap()),
                "case {name}: {needle} in {line}"
            );
        }
        for needle in case["data_lacks"].as_array().unwrap() {
            assert!(
                !line.contains(needle.as_str().unwrap()),
                "case {name}: {needle} in {line}"
            );
        }
    }
}

/// What a call's stdout leads to with auto approval off: the row's
/// outcome, the decision kept, and the cause.
fn output_of(stdout: &str) -> (ReviewOutcome, Option<String>, EscalationCause) {
    match read_output(stdout.as_bytes()).0 {
        Ok(answer) => (
            ReviewOutcome::Answered,
            Some(answer.decision.code().to_owned()),
            decide(answer.decision, false).expect("approval off escalates every answer"),
        ),
        Err(error) => {
            let (outcome, cause) = output_error_cause(error);
            (outcome, None, cause)
        }
    }
}

#[test]
fn output_cases_match() {
    let set = load_set();
    for case in cases(&set, "output") {
        let name = case["name"].as_str().unwrap();
        let (outcome, decision, cause) = output_of(case["stdout"].as_str().unwrap());
        assert_eq!(outcome.code(), case["outcome"], "case {name}");
        assert_eq!(
            decision.as_deref(),
            case["decision"].as_str(),
            "case {name}"
        );
        assert_eq!(cause.code(), case["cause"], "case {name}");
    }
}

fn fingerprint(text: &str) -> TreeFingerprint {
    TreeFingerprint {
        sha256: officina_core::sha256_hex(text.as_bytes()),
        files: 1,
        git: None,
    }
}

#[test]
fn baseline_cases_match() {
    let set = load_set();
    let job = job(&set);
    let stream = StreamSummary {
        bytes: 0,
        sha256: officina_core::sha256_hex(b""),
    };
    for case in cases(&set, "baseline") {
        let name = case["name"].as_str().unwrap();
        let mut lines = Vec::new();
        for run in case["runs"].as_array().unwrap() {
            let (action, run_id, ts, tree, code) = (
                run[0].as_str().unwrap(),
                run[1].as_str().unwrap(),
                run[2].as_i64().unwrap(),
                run[3].as_str().unwrap(),
                run[4].as_i64().unwrap(),
            );
            lines.push(JournalLine::new(
                ts,
                JournalEntry::RunStarted(RunStarted {
                    run_id: run_id.to_owned(),
                    project: "lab".to_owned(),
                    session_id: None,
                    action: action.to_owned(),
                    class: officina_core::relay::catalogue::ActionClass::Free,
                    params: BTreeMap::from([("group".to_owned(), "lab-web".to_owned())]),
                    argv: Vec::new(),
                    method: None,
                    url: None,
                    credential: None,
                    catalogue_hash: set.approved.hash.clone(),
                    approval_id: None,
                    tree: Some(fingerprint(tree)),
                    timeout_s: Some(60),
                }),
            ));
            lines.push(JournalLine::new(
                ts + 1_000,
                JournalEntry::RunFinished(RunFinished {
                    body_broke_off: false,
                    run_id: run_id.to_owned(),
                    outcome: RunOutcome::Exited,
                    exit_code: Some(code),
                    duration_ms: 1_000,
                    stdout: Some(stream.clone()),
                    stderr: Some(stream.clone()),
                    dropped_bytes: 0,
                    status: None,
                    body_bytes: None,
                    body_sha256: None,
                    detail: None,
                }),
            ));
        }
        let tree = fingerprint(case["tree"].as_str().unwrap());
        assert_eq!(
            rule_baseline(&job, &lines, Some(&tree), set.now_ms),
            case["expect"].as_bool().unwrap(),
            "case {name}"
        );
    }
}

/// Story 5.14 AC 4 (D-141): rule mode's precheck, then the rule's
/// decision on the case's baseline.
#[test]
fn rule_cases_match() {
    let set = load_set();
    let dirs = dirs();
    let ctx = ApprovalContext {
        approved: Some(&set.approved),
        project_dirs: &dirs,
        system_dir: "C:/Windows/System32",
    };
    for case in cases(&set, "rule") {
        let name = case["name"].as_str().unwrap();
        let session = match case.get("session_id") {
            Some(Value::Null) => None,
            Some(value) => value.as_str(),
            None => Some(SESSION),
        };
        let (id, request) = request(&set, case["action"].as_str().unwrap(), session, None);
        let rows = rows(&set, &case["facts"]);
        let found = match precheck_rule(&ctx, &id, &request, &rows, set.now_ms) {
            Precheck::Review(_) => match rule_decision(case["baseline"].as_bool().unwrap()) {
                (decision, None) => decision.code().to_owned(),
                (_, Some(cause)) => cause.code().to_owned(),
            },
            other => outcome_code(&other),
        };
        assert_eq!(found, case["expect"], "case {name}");
    }
}

/// Story 5.14 AC 1 (AD-88, D-143): the shadow statistics of each case's
/// rows, and the 3/n bound of its cases.
#[test]
fn stats_cases_match() {
    let set = load_set();
    let defaults = serde_json::json!({
        "requested_at_ms": set.now_ms - 60_000,
        "mode": "shadow",
        "outcome": "answered",
        "decision": "approve",
        "rule_baseline": true,
        "author_decision": "approved",
        "author_reason": null,
        "marked_at_ms": null,
    });
    for case in cases(&set, "stats") {
        let name = case["name"].as_str().unwrap();
        let mut rows: Vec<StatsRecord> = Vec::new();
        for spec in case["rows"].as_array().unwrap() {
            let mut row = defaults.clone();
            for (key, value) in spec.as_object().unwrap() {
                if key != "n" {
                    row[key] = value.clone();
                }
            }
            let record: StatsRecord = serde_json::from_value(row).expect("a stats row parses");
            let n = spec.get("n").and_then(Value::as_u64).unwrap_or(1);
            rows.extend((0..n).map(|_| record.clone()));
        }
        let stats = shadow_stats(&rows, set.now_ms);
        let expect = &case["expect"];
        assert_eq!(
            (
                u64::from(stats.cases),
                u64::from(stats.disagreements),
                u64::from(stats.rule_diff),
                stats.gate,
                stats.rule_offer
            ),
            (
                expect["cases"].as_u64().unwrap(),
                expect["disagreements"].as_u64().unwrap(),
                expect["rule_diff"].as_u64().unwrap(),
                expect["gate"].as_bool().unwrap(),
                expect["rule_offer"].as_bool().unwrap()
            ),
            "case {name}"
        );
        assert_eq!(
            stats.false_approve_bound_percent,
            Some(300.0 / f64::from(stats.cases)),
            "case {name}"
        );
        assert_eq!(
            (stats.gate_cases, stats.rule_diff_percent),
            (GATE_CASES, RULE_DIFF_PERCENT)
        );
    }
}

/// Every cause has a case: the precheck's in the precheck section, a
/// call's in the output section, the rule's in the rule section, and the
/// timeout and the authority's refusal named by the set (the host tests
/// replay those two: `watch.rs` and `authority.rs`).
#[test]
fn every_cause_has_a_case() {
    let set = load_set();
    let mut seen: BTreeSet<String> = ["precheck", "rule"]
        .iter()
        .flat_map(|section| cases(&set, section))
        .filter_map(|case| case["expect"].as_str().map(str::to_owned))
        .collect();
    seen.extend(
        cases(&set, "output")
            .iter()
            .filter_map(|case| case["cause"].as_str().map(str::to_owned)),
    );
    seen.insert(set.raw["timeout_cause"].as_str().unwrap().to_owned());
    seen.insert(set.raw["refused_cause"].as_str().unwrap().to_owned());
    for cause in EscalationCause::ALL {
        assert!(seen.contains(cause.code()), "no case for {cause:?}");
    }
    for section in ["precheck", "prompt", "output", "baseline", "rule", "stats"] {
        assert!(!cases(&set, section).is_empty(), "{section}");
    }
}
