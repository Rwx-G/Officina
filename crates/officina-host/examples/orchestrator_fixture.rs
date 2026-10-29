//! E2E fixture tool (story 5.8, D-72's pattern): writes the work orders of
//! an Officina home whose floor with templates the fixture script wrote and
//! approved through `approve_fixture` (D-25), through the window's own
//! writer at explicit times, binds the started ones as the window binds
//! them (AD-81 step 4), records the author's last reading of the briefing,
//! then prints the `OrchestratorView` the window would build, from the same
//! functions the collector calls. Story 5.14 adds the `auto` class's
//! review rows with the author's decisions, its switches and the decision
//! lines, which `officina relay --json` then prints (the `relay-auto`
//! scenario). Never SQL from Node, never a command line
//! write path. Built only with `window-authority`, linked into neither
//! shipped binary.
//!
//! Usage: `orchestrator_fixture <officina home> <script.json>`, where the
//! file holds a [`Script`]. The board, when given, is the file `officina
//! board --json` printed for the same home.
//!
//! Exit 0, 1 when a step fails, 64 on usage.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use officina_core::agents::parse_agents;
use officina_core::board::Board;
use officina_core::orders::work_order::{SubjectChoice, WorkOrder, order_name};
use officina_core::registry::fold::AgentsSnapshot;
use officina_core::registry::types::{
    AuthorDecision, AuthorReason, LaunchOutcome, ReviewMode, ReviewOutcome, WorkOrderSource,
};
use officina_core::relay::approvals::ApprovalBy;
use officina_core::relay::auto::{AutoMode, EscalationCause, ReviewDecision};
use officina_core::relay::journal::{ApprovalEvent, JournalEntry};
use officina_core::relay::template::{ApprovedTemplate, ParamName, ProjectName, TemplateName};
use officina_host::paths::events_dir_of;
use officina_host::registry::orders::LaunchRecord;
use officina_host::registry::review::NewReview;
use officina_host::registry::store::{Writer, open_writer};
use officina_host::registry::window::WindowRegistry;
use officina_host::relay::home::{RelayHome, load_relay_floor};
use officina_host::relay::journal;
use officina_host::relay::view::{AutoRead, read_view_with};
use serde::Deserialize;

const USAGE: &str = "usage: orchestrator_fixture <officina home> <script.json>";

/// How long after its launch a started order's background session is
/// listed, inside the binding window (AD-81 step 4).
const LISTED_AFTER_MS: i64 = 30_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Script {
    /// When the view is built.
    now_ms: i64,
    /// `officina board --json`'s output for the home, when the view shows
    /// the board's sessions.
    #[serde(default)]
    board_file: Option<PathBuf>,
    #[serde(default)]
    orders: Vec<Order>,
    /// The author's last reading of the briefing, when there was one.
    #[serde(default)]
    read_at_ms: Option<i64>,
    /// Review rows of `auto` requests, with the author's decisions copied
    /// onto them (story 5.14, AD-88).
    #[serde(default)]
    reviews: Vec<SeededReview>,
    /// Auto approval turned on or off at `now_ms` through the writer,
    /// which refuses it without the gate.
    #[serde(default)]
    auto_approval: Option<bool>,
    /// Auto approval's mode set at `now_ms` through the writer.
    #[serde(default)]
    auto_mode: Option<AutoMode>,
    /// Approval decisions journaled as the window's authority writes them,
    /// for the decisions per day (AD-92).
    #[serde(default)]
    decisions: Vec<SeededDecision>,
}

/// One review row, as the window's review would have written it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeededReview {
    approval_id: String,
    project: String,
    action: String,
    requested_at_ms: i64,
    mode: ReviewMode,
    outcome: ReviewOutcome,
    #[serde(default)]
    cause: Option<EscalationCause>,
    #[serde(default)]
    decision: Option<ReviewDecision>,
    #[serde(default)]
    reason: Option<String>,
    rule_baseline: bool,
    /// The author's decision, copied a minute after the request.
    #[serde(default)]
    author: Option<AuthorDecision>,
    #[serde(default)]
    author_reason: Option<AuthorReason>,
    /// The run the approval started, linked as the window links it.
    #[serde(default)]
    run_id: Option<String>,
}

/// One `approval_given` or `approval_rejected` line.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeededDecision {
    at_ms: i64,
    approval_id: String,
    approve: bool,
    by: ApprovalBy,
}

/// Writes the review rows, the auto settings and the decision lines of
/// `script` (story 5.14).
fn seed_auto(writer: &mut Writer, relay: &RelayHome, script: &Script) -> Result<(), String> {
    for review in &script.reviews {
        let called = review.outcome != ReviewOutcome::NoCall;
        let id = writer
            .insert_review(&NewReview {
                approval_id: review.approval_id.clone(),
                request_key: format!("key-{}", review.approval_id),
                project: review.project.clone(),
                action: review.action.clone(),
                requested_at_ms: review.requested_at_ms,
                mode: review.mode,
                outcome: review.outcome,
                cause: review.cause,
                decision: review.decision,
                reason: review.reason.clone(),
                rule_baseline: review.rule_baseline,
                call_started_at_ms: called.then_some(review.requested_at_ms + 1_000),
                duration_ms: called.then_some(2_400),
                input_tokens: None,
                output_tokens: None,
                cost_microusd: None,
            })
            .map_err(|e| e.to_string())?;
        if let Some(author) = review.author {
            writer
                .copy_author_decision(
                    id,
                    author,
                    review.author_reason,
                    review.requested_at_ms + 60_000,
                )
                .map_err(|e| e.to_string())?;
        }
        if let Some(run_id) = &review.run_id {
            writer
                .link_runs(&[(review.approval_id.clone(), run_id.clone())])
                .map_err(|e| e.to_string())?;
        }
    }
    if let Some(on) = script.auto_approval {
        writer
            .set_auto_approval(on, script.now_ms)
            .map_err(|e| e.to_string())?
            .map_err(|refusal| refusal.to_string())?;
    }
    if let Some(mode) = script.auto_mode {
        writer
            .set_auto_mode(mode, script.now_ms)
            .map_err(|e| e.to_string())?
            .map_err(|refusal| refusal.to_string())?;
    }
    for decision in &script.decisions {
        let event = ApprovalEvent {
            approval_id: decision.approval_id.clone(),
            request_key: format!("key-{}", decision.approval_id),
            project: None,
            action: None,
            dry_run: None,
            by: Some(decision.by),
            reason: None,
        };
        let line = if decision.approve {
            JournalEntry::ApprovalGiven(event)
        } else {
            JournalEntry::ApprovalRejected(event)
        };
        journal::append(relay, &[line], decision.at_ms).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// One work order, launched from a card of the approved catalogue.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Order {
    card: String,
    template: String,
    project: String,
    values: Vec<(String, String)>,
    seq: u32,
    subject: SubjectChoice,
    launched_at_ms: i64,
    /// `launching` leaves the row as the launch recorded it.
    outcome: LaunchOutcome,
    #[serde(default)]
    exit_code: Option<i32>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    printed: Option<String>,
    /// The background session that takes the order's name, listed
    /// [`LISTED_AFTER_MS`] after the launch.
    #[serde(default)]
    bind: Option<Bind>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bind {
    bg_id: String,
    session_id: String,
}

/// The order `order` validates to against the home's approved catalogue
/// (AD-77), as a card of the composer would.
fn validate(relay: &RelayHome, order: &Order) -> Result<WorkOrder, String> {
    let state = relay.read_approved();
    let approved = state
        .approved()
        .ok_or("the home's catalogue is not approved")?
        .clone();
    let current =
        load_relay_floor(&relay.relay_floor(&state)).map_err(|e| format!("relay floor: {e}"))?;
    let name = TemplateName::parse(&order.template).ok_or("a template name")?;
    let template = ApprovedTemplate::new(&approved, &current, &name)
        .map_err(|e| format!("template {}: {e:?}", order.template))?;
    let project = ProjectName::parse(&order.project).ok_or("a project name")?;
    let values: Vec<(ParamName, &str)> = order
        .values
        .iter()
        .map(|(name, value)| {
            ParamName::parse(name)
                .map(|name| (name, value.as_str()))
                .ok_or_else(|| format!("parameter name {name}"))
        })
        .collect::<Result<_, _>>()?;
    WorkOrder::validate(&template, &project, &values)
        .map_err(|e| format!("order {}: {e:?}", order.card))
}

/// Records `order` as a launch, its outcome and its binding.
fn record(
    writer: &mut Writer,
    snapshot: &mut Option<AgentsSnapshot>,
    validated: &WorkOrder,
    order: &Order,
) -> Result<(), String> {
    writer
        .insert_work_order(
            &LaunchRecord {
                order: validated,
                card_id: &order.card,
                seq: order.seq,
                source: WorkOrderSource::Composer,
                subject: &order.subject,
                proposal_id: None,
            },
            order.launched_at_ms,
        )
        .map_err(|e| e.to_string())?
        .map_err(|_| format!("{}: the name is taken", order.card))?;
    if order.outcome != LaunchOutcome::Launching {
        writer
            .record_launch_outcome(
                &order.card,
                order.outcome,
                order.exit_code,
                order.error.as_deref(),
                order.printed.as_deref(),
                order.launched_at_ms + 1_000,
            )
            .map_err(|e| e.to_string())?;
    }
    if let Some(bind) = &order.bind {
        let listed = order.launched_at_ms + LISTED_AFTER_MS;
        let entry = serde_json::json!([{
            "id": bind.bg_id,
            "sessionId": bind.session_id,
            "name": order_name(validated.template(), order.seq),
            "cwd": validated.project_folder(),
            "kind": "background",
            "pid": 4900,
            "startedAt": order.launched_at_ms + 1_000,
            "status": "busy",
            "state": "working",
        }]);
        let records = parse_agents(&entry.to_string()).map_err(|e| e.to_string())?;
        writer
            .apply_agents(snapshot, &records, &[], listed)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn run(home: &Path, script: &Script) -> Result<String, String> {
    let relay = RelayHome::new(home, None);
    {
        let mut writer = open_writer(home, &[], script.now_ms)
            .writer()
            .ok_or("the registry cannot be written")?;
        let mut snapshot: Option<AgentsSnapshot> = None;
        for order in &script.orders {
            let validated = validate(&relay, order)?;
            record(&mut writer, &mut snapshot, &validated, order)?;
        }
        if let Some(at) = script.read_at_ms {
            writer.mark_briefing_read(at).map_err(|e| e.to_string())?;
        }
        seed_auto(&mut writer, &relay, script)?;
    }
    let board: Option<Board> = match &script.board_file {
        Some(path) => Some(
            serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
                .map_err(|e| format!("board: {e}"))?,
        ),
        None => None,
    };
    let auto = AutoRead::read_only(home, script.now_ms);
    let relay_view =
        read_view_with(&relay, script.now_ms, None, &auto).map_err(|e| e.to_string())?;
    let mut registry = WindowRegistry::open(home, &[], script.now_ms);
    let registry_view = registry.view(board.as_ref(), &events_dir_of(home), script.now_ms);
    let view = registry.orchestrator_view(
        board.as_ref(),
        Some(&relay_view),
        Some(&registry_view),
        script.now_ms,
    );
    serde_json::to_string_pretty(&view).map_err(|e| e.to_string())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [home, script] = args.as_slice() else {
        eprintln!("{USAGE}");
        return ExitCode::from(64);
    };
    let script: Script = match std::fs::read_to_string(script)
        .map_err(|e| e.to_string())
        .and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string()))
    {
        Ok(script) => script,
        Err(error) => {
            eprintln!("orchestrator_fixture: {error}");
            return ExitCode::from(64);
        }
    };
    match run(Path::new(home), &script) {
        Ok(view) => {
            println!("{view}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("orchestrator_fixture: {error}");
            ExitCode::FAILURE
        }
    }
}
