//! E2E fixture tool (D-72): plays the window's registry writer on an
//! Officina home whose event files the fixture script wrote, at an explicit
//! `now_ms`, then exits, so `officina subjects --json` and `officina board
//! --json` print the fixtures with the window closed (D-82). Never SQL from
//! Node, never a command line write path. Built only with
//! `window-authority`, linked into neither shipped binary.
//!
//! Usage: `registry_fixture <officina home> <steps.json>`, where the file
//! holds `{"now_ms": <ms>, "steps": [...]}` and each step is one variant of
//! [`Step`], written in snake case, as documented on the variant.
//!
//! Exit 0, 1 when a step fails, 64 on usage.

use std::path::Path;
use std::process::ExitCode;

use officina_core::agents::parse_agents;
use officina_core::registry::confirm::{AuthorAction, FileUnder, ItemOutcome};
use officina_core::registry::fold::AgentsSnapshot;
use officina_core::registry::triage_call::{CALL_SPACING_MS, CallAnswer, CallUsage, TriageCase};
use officina_core::registry::types::{
    ModelVerdict, TriageOutcome, TriagePurpose, TriageRefusal, Verdict,
};
use officina_host::collector::record_hook_fields;
use officina_host::registry::RegistryPaths;
use officina_host::registry::ingest::IngestContext;
use officina_host::registry::store::{Writer, open_writer};
use officina_host::registry::triage::{CallResult, Triage};
use serde::Deserialize;

const USAGE: &str = "usage: registry_fixture <officina home> <steps.json>";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Script {
    now_ms: i64,
    steps: Vec<Step>,
}

/// One step of a fixture script.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Step {
    /// `"ingest"`: one ingestion pass over the home's event files.
    Ingest,
    /// `{"agents": [<claude agents records>]}`: one successful session
    /// list.
    Agents(Vec<serde_json::Value>),
    /// `{"author": <AuthorAction>}`: one author action, which must succeed.
    Author(AuthorAction),
    /// `"copy"`: the daily copy, as the hourly pass takes it.
    Copy,
    /// `"damage"`: the database overwritten with bytes that are no
    /// database, then opened again, so the window's restore runs from the
    /// copy (D-93).
    Damage,
    /// `{"hook_fields": "<claude version>"}`: one ingestion pass, whose hook
    /// sightings are folded into that version's record of
    /// `state/contract.json`, as the window folds them (AD-56); the record
    /// must exist already (`officina board --json` starts it).
    HookFields(String),
    /// `{"at": <ms>}`: the steps after it run at that time instead of
    /// `now_ms`.
    At(i64),
    /// `{"confirm": {"key", "verdict", "routine", "file_under"}}`: the
    /// author confirms the row of that key with the proposal the view shows
    /// now, which must be confirmed (AD-49).
    Confirm(Confirmation),
    /// `{"dry_run": [<answer>, ...]}`: a dry run started and driven through
    /// the window's triage state, each case answered in turn as the list
    /// says (an answer [`call_result`] reads), one call per spacing, no
    /// process started (AD-52).
    DryRun(Vec<String>),
    /// `{"refused_call": "<refusal>"}`: one call refused for that cause, as
    /// the runner records it (D-65).
    RefusedCall(TriageRefusal),
    /// `{"auto_archive": true | false}`: the author turns auto-archive on,
    /// which must be accepted, or off (FR-12).
    AutoArchive(bool),
    /// `"auto_archive_pass"`: one auto-archive pass, as the collector runs
    /// it after a view build (D-70).
    AutoArchivePass,
    /// `{"triage_model": true | false}`: the author turns the triage model
    /// on, which must be accepted, or off (FR-10 as amended).
    TriageModel(bool),
    /// `{"proposals": [{"answer": <answer>, "reason": "..."}, ...]}`: the
    /// rows queued for the model taken in turn through the window's triage
    /// state, each call ended as the list says (an answer [`call_result`]
    /// reads; the reason, untrusted text, on an answer), one call per
    /// spacing, no process started (AD-52).
    Proposals(Vec<ProposalAnswer>),
}

/// How one proposal call ends: an answer with its reason, or an outcome
/// without one.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposalAnswer {
    answer: String,
    #[serde(default)]
    reason: Option<String>,
}

/// A row the author confirms, as the window sends it, the proposal aside:
/// the fixture takes the one the view shows.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Confirmation {
    key: String,
    verdict: Verdict,
    #[serde(default)]
    routine: bool,
    #[serde(default)]
    file_under: Option<FileUnder>,
}

/// One call's result for a case of `purpose`, answered `answer` with
/// `reason` at `at_ms`.
fn call_result(
    purpose: TriagePurpose,
    case_key: String,
    case: &TriageCase,
    answer: &str,
    reason: &str,
    at_ms: i64,
) -> Result<CallResult, String> {
    let (outcome, verdict) = match answer {
        "timeout" => (TriageOutcome::Timeout, None),
        "invalid" => (TriageOutcome::Invalid, None),
        "failed" => (TriageOutcome::Failed, None),
        verdict => (
            TriageOutcome::Answer,
            Some(
                ModelVerdict::from_code(verdict)
                    .ok_or_else(|| format!("unknown answer {verdict}"))?,
            ),
        ),
    };
    Ok(CallResult {
        purpose,
        case_key,
        input_hash: case.input_hash(),
        started_at_ms: at_ms,
        duration_ms: 2_400,
        outcome,
        refusal: None,
        answer: verdict.map(|verdict| CallAnswer {
            verdict,
            reason: reason.to_owned(),
        }),
        usage: CallUsage {
            input_tokens: Some(4_500),
            output_tokens: Some(380),
            cost_microusd: Some(1_100),
        },
        detail: None,
    })
}

fn writer(home: &Path, now_ms: i64) -> Result<Writer, String> {
    open_writer(home, &[], now_ms)
        .writer()
        .ok_or_else(|| "the registry did not open writable".to_owned())
}

fn run(home: &Path, script: Script) -> Result<(), String> {
    let mut now_ms = script.now_ms;
    let events = home.join("events");
    let mut writer = writer(home, now_ms)?;
    let mut snapshot: Option<AgentsSnapshot> = None;
    let ctx = IngestContext {
        events_dir: &events,
        officina_home: home,
        projects: &[],
        no_excerpts: false,
    };
    for (index, step) in script.steps.into_iter().enumerate() {
        let failed = |error: String| format!("step {index}: {error}");
        match step {
            Step::Ingest => {
                writer
                    .ingest(&ctx, now_ms)
                    .map_err(|e| failed(e.to_string()))?;
            }
            Step::HookFields(version) => {
                let report = writer
                    .ingest(&ctx, now_ms)
                    .map_err(|e| failed(e.to_string()))?;
                let contract = home.join("state").join("contract.json");
                record_hook_fields(&contract, &version, &report.hook_sightings)
                    .ok_or_else(|| failed(format!("no contract record for {version}")))?;
            }
            Step::Agents(records) => {
                let text = serde_json::to_string(&records).map_err(|e| failed(e.to_string()))?;
                let records = parse_agents(&text).map_err(|e| failed(e.to_string()))?;
                writer
                    .apply_agents(&mut snapshot, &records, &[], now_ms)
                    .map_err(|e| failed(e.to_string()))?;
            }
            Step::Author(action) => {
                let created = writer
                    .author(&action, now_ms)
                    .map_err(|e| failed(e.to_string()))?;
                if let Some(id) = created {
                    println!("created subject {id}");
                }
            }
            Step::Copy => {
                writer
                    .copy_if_due(now_ms)
                    .map_err(|e| failed(e.to_string()))?;
            }
            Step::At(at) => now_ms = at,
            Step::Confirm(item) => {
                let view = writer
                    .view(None, &events, now_ms)
                    .map_err(|e| failed(e.to_string()))?;
                let row = view
                    .triage
                    .iter()
                    .find(|row| row.key == item.key)
                    .ok_or_else(|| failed(format!("no row {}", item.key)))?;
                let answers = writer
                    .confirm_triage(
                        &[officina_core::registry::confirm::ConfirmItem {
                            key: item.key.clone(),
                            shown: row.proposal,
                            verdict: item.verdict,
                            routine: item.routine,
                            file_under: item.file_under,
                        }],
                        now_ms,
                    )
                    .map_err(|e| failed(e.to_string()))?;
                if !matches!(answers[0].outcome, ItemOutcome::Confirmed { .. }) {
                    return Err(failed(format!("{} not confirmed: {answers:?}", item.key)));
                }
            }
            Step::DryRun(answers) => {
                let mut triage = Triage::default();
                triage
                    .start_dry_run(&writer, now_ms)
                    .map_err(|e| failed(e.to_string()))?
                    .map_err(|e| failed(e.to_string()))?;
                let mut at = now_ms;
                for answer in &answers {
                    let next = triage
                        .next_call(&mut writer, at)
                        .map_err(|e| failed(e.to_string()))?
                        .ok_or_else(|| failed("the dry run has fewer cases than answers".into()))?;
                    let result = call_result(
                        TriagePurpose::DryRun,
                        next.case_key.clone(),
                        &next.case,
                        answer,
                        "fixture",
                        at,
                    )
                    .map_err(failed)?;
                    triage
                        .take_result(&mut writer, &result, at + 2_400)
                        .map_err(|e| failed(e.to_string()))?;
                    at += CALL_SPACING_MS;
                }
                if triage.running() {
                    return Err(failed("the dry run has more cases than answers".into()));
                }
            }
            Step::RefusedCall(cause) => {
                let result = CallResult {
                    purpose: TriagePurpose::DryRun,
                    case_key: "confirmation:0".into(),
                    input_hash: TriageCase::default().input_hash(),
                    started_at_ms: now_ms,
                    duration_ms: 0,
                    outcome: TriageOutcome::Refused,
                    refusal: Some(cause),
                    answer: None,
                    usage: CallUsage::default(),
                    detail: None,
                };
                writer
                    .record_call(&result)
                    .map_err(|e| failed(e.to_string()))?;
            }
            Step::AutoArchive(on) => {
                writer
                    .set_auto_archive(on, now_ms)
                    .map_err(|e| failed(e.to_string()))?
                    .map_err(|state| failed(format!("auto-archive refused: {}", state.code())))?;
            }
            Step::AutoArchivePass => {
                let run = writer
                    .auto_archive(now_ms)
                    .map_err(|e| failed(e.to_string()))?;
                println!(
                    "auto-archived {:?}, turned off: {:?}",
                    run.archived, run.turned_off
                );
            }
            Step::TriageModel(on) => {
                writer
                    .set_triage_model(on, now_ms)
                    .map_err(|e| failed(e.to_string()))?
                    .map_err(|refusal| failed(refusal.to_string()))?;
            }
            Step::Proposals(answers) => {
                let mut triage = Triage::default();
                let mut at = now_ms;
                for answer in &answers {
                    let next = triage
                        .next_call(&mut writer, at)
                        .map_err(|e| failed(e.to_string()))?
                        .filter(|next| next.purpose == TriagePurpose::Proposal)
                        .ok_or_else(|| failed("fewer rows queued than answers".into()))?;
                    let result = call_result(
                        TriagePurpose::Proposal,
                        next.case_key.clone(),
                        &next.case,
                        &answer.answer,
                        answer.reason.as_deref().unwrap_or("fixture"),
                        at,
                    )
                    .map_err(failed)?;
                    triage
                        .take_result(&mut writer, &result, at + 2_400)
                        .map_err(|e| failed(e.to_string()))?;
                    at += CALL_SPACING_MS;
                }
            }
            Step::Damage => {
                drop(writer);
                let database = RegistryPaths::new(home).database();
                std::fs::write(&database, "not a database, ".repeat(300))
                    .map_err(|e| failed(e.to_string()))?;
                writer = self::writer(home, now_ms).map_err(failed)?;
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [home, steps] = args.as_slice() else {
        eprintln!("{USAGE}");
        return ExitCode::from(64);
    };
    let script = match std::fs::read_to_string(steps)
        .map_err(|e| e.to_string())
        .and_then(|text| serde_json::from_str::<Script>(&text).map_err(|e| e.to_string()))
    {
        Ok(script) => script,
        Err(error) => {
            eprintln!("registry_fixture: {steps}: {error}");
            return ExitCode::from(64);
        }
    };
    match run(Path::new(home), script) {
        Ok(()) => {
            println!("done");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("registry_fixture: {error}");
            ExitCode::FAILURE
        }
    }
}
