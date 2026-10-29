//! E2E fixture tool (story 5.8, D-72's pattern): writes the work orders of
//! an Officina home whose floor with templates the fixture script wrote and
//! approved through `approve_fixture` (D-25), through the window's own
//! writer at explicit times, binds the started ones as the window binds
//! them (AD-81 step 4), records the author's last reading of the briefing,
//! then prints the `OrchestratorView` the window would build, from the same
//! functions the collector calls. A script with cards prints those instead:
//! work order cards issued and edited by core from the home's approved
//! catalogue, as the window's card store issues them (story 5.9). Story 5.14
//! adds the `auto` class's review rows with the author's decisions, its
//! switches and the decision lines, which `officina relay --json` then
//! prints (the `relay-auto` scenario). A script with a meeting (story 5.10)
//! builds a git repository with commits since the session's start and a
//! working-tree change, and prints the `MeetingView` the window builds for
//! that card, its logs read from the fake claude. Story 5.17 adds proposal
//! files, each ingested by the window's writer when received, and cards
//! issued from a seeded proposal by name. Never SQL from Node, never a
//! command line write path. Built only with `window-authority`, linked into neither
//! shipped binary.
//!
//! Usage: `orchestrator_fixture <officina home> <script.json>`, where the
//! file holds a [`Script`]. The board, when given, is the file `officina
//! board --json` printed for the same home.
//!
//! Exit 0, 1 when a step fails, 64 on usage.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use officina_core::agents::parse_agents;
use officina_core::board::Board;
use officina_core::orders::card::{
    self, CardChange, CardInputs, CardRequest, IssuedCard, WorkOrderCard, reserved_seq,
};
use officina_core::orders::proposal::{file_name, proposal_id};
use officina_core::orders::work_order::{SubjectChoice, WorkOrder, order_name};
use officina_core::registry::fold::AgentsSnapshot;
use officina_core::registry::types::{
    AuthorDecision, AuthorReason, LaunchOutcome, ReviewMode, ReviewOutcome, WorkOrderSource,
};
use officina_core::relay::approvals::ApprovalBy;
use officina_core::relay::auto::{AutoMode, EscalationCause, ReviewDecision};
use officina_core::relay::journal::{ApprovalEvent, JournalEntry};
use officina_core::relay::template::{ApprovedTemplate, ParamName, ProjectName, TemplateName};
use officina_host::collector::{Collector, LoadedFloor};
use officina_host::meeting;
use officina_host::paths::{Paths, events_dir_of, orchestrator_proposals_dir};
use officina_host::registry::orders::LaunchRecord;
use officina_host::registry::review::NewReview;
use officina_host::registry::store::{Writer, open_writer};
use officina_host::registry::window::WindowRegistry;
use officina_host::relay::home::{RelayHome, load_relay_floor};
use officina_host::relay::journal;
use officina_host::relay::view::{AutoRead, read_view_with};
use officina_host::spawn::resolve_claude;
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
    /// Cards to issue, in order; when there are any, they are printed by
    /// name instead of the view.
    #[serde(default)]
    cards: Vec<CardStep>,
    /// A meeting to print instead of the view (story 5.10).
    #[serde(default)]
    meeting: Option<MeetingScript>,
    /// Proposal files written as the orchestrator or a hand would, each
    /// ingested by the window's writer when it is received (story 5.17).
    #[serde(default)]
    proposals: Vec<SeededProposal>,
}

/// A meeting to print (story 5.10, D-181's kind `meeting`): the board's
/// card `key`, whose session `claude agents` lists in `repo`, which this
/// tool makes a git repository with `commits` and `working_tree` changes.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MeetingScript {
    key: String,
    repo: PathBuf,
    commits: Vec<SeededCommit>,
    /// Files written after the commits, tracked or not.
    #[serde(default)]
    working_tree: Vec<SeededFile>,
}

/// One commit, dated `at_ms`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeededCommit {
    at_ms: i64,
    subject: String,
    files: Vec<SeededFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeededFile {
    path: String,
    text: String,
}

/// Runs `git <args>` in `dir` with a fixed identity, both dates `at_ms`.
#[expect(
    clippy::disallowed_methods,
    reason = "the fixture builds its repository with git outside host::git"
)]
fn git_in(git: &Path, dir: &Path, args: &[&str], at_ms: i64) -> Result<(), String> {
    let date = format!("{} +0000", at_ms / 1000);
    let out = std::process::Command::new(git)
        .args([
            "-c",
            "user.name=user01",
            "-c",
            "user.email=user@example.com",
            "-c",
            "core.autocrlf=false",
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgSign=false",
        ])
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_DATE", &date)
        .env("GIT_COMMITTER_DATE", &date)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

fn write_file(repo: &Path, file: &SeededFile) -> Result<(), String> {
    let path = repo.join(&file.path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, &file.text).map_err(|e| e.to_string())
}

/// Builds the repository of `meeting`, then the meeting of its key as the
/// window builds it: the collector's half on a collector that listed the
/// sessions through the `claude` of `OFFICINA_CLAUDE_BIN`, then the reads
/// and the view (`meeting::grant`, `meeting::gather`).
fn print_meeting(
    home: &Path,
    board: Option<&Board>,
    meeting: &MeetingScript,
    now_ms: i64,
) -> Result<String, String> {
    let git = officina_host::relay::tree::resolve_git().ok_or("no git installed")?;
    std::fs::create_dir_all(&meeting.repo).map_err(|e| e.to_string())?;
    let first = meeting.commits.first().map_or(now_ms, |c| c.at_ms);
    git_in(&git, &meeting.repo, &["init", "-q"], first)?;
    for commit in &meeting.commits {
        for file in &commit.files {
            write_file(&meeting.repo, file)?;
            git_in(&git, &meeting.repo, &["add", &file.path], commit.at_ms)?;
        }
        git_in(
            &git,
            &meeting.repo,
            &["commit", "-q", "-m", &commit.subject],
            commit.at_ms,
        )?;
    }
    for file in &meeting.working_tree {
        write_file(&meeting.repo, file)?;
    }
    let paths = Paths {
        officina_home: home.to_path_buf(),
        claude_config_dir: home.join("claude"),
        claude_config_override: None,
        floor_file: None,
    };
    let claude = resolve_claude().map(|c| c.path().to_path_buf());
    let mut collector = Collector::from_parts(paths, LoadedFloor::default(), claude);
    if !collector.refresh_agents(now_ms) {
        return Err("claude agents did not answer".to_owned());
    }
    // The version decides whether a live answer resumes (D-139).
    collector.refresh_contract(now_ms);
    let mut registry = WindowRegistry::open(home, &[], now_ms);
    let grant =
        meeting::grant(&collector, &mut registry, board, &meeting.key).map_err(|e| e.message)?;
    let view = meeting::gather(&grant, now_ms);
    serde_json::to_string_pretty(&view).map_err(|e| e.to_string())
}

/// One file of `orchestrator/proposals/`, named for its fixture: a file as
/// `officina mcp` writes it (`file`), or bytes that hold no proposal
/// (`raw`), written at `written_at_ms` and ingested at `received_at_ms`;
/// dismissed by the author when `dismissed`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeededProposal {
    name: String,
    written_at_ms: i64,
    received_at_ms: i64,
    #[serde(default)]
    file: Option<serde_json::Value>,
    #[serde(default)]
    raw: Option<String>,
    #[serde(default)]
    dismissed: bool,
}

/// The fixed id of the seeded proposal at `index`, so a card step names
/// the same proposal on every run.
fn seeded_id(proposal: &SeededProposal, index: usize) -> String {
    proposal_id(proposal.written_at_ms, index as u64 + 1)
}

/// Writes and ingests the script's proposals through the window's writer,
/// against the home's approved snapshot, then expires what is due at
/// `now_ms`.
fn seed_proposals(
    writer: &mut Writer,
    home: &Path,
    relay: &RelayHome,
    script: &Script,
) -> Result<(), String> {
    if script.proposals.is_empty() {
        return Ok(());
    }
    let dir = orchestrator_proposals_dir(home);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let catalogue = relay
        .read_approved()
        .approved()
        .map(|approved| approved.canonical.clone());
    for (index, proposal) in script.proposals.iter().enumerate() {
        let id = seeded_id(proposal, index);
        let bytes = match (&proposal.file, &proposal.raw) {
            (Some(file), None) => serde_json::to_vec(file).map_err(|e| e.to_string())?,
            (None, Some(raw)) => raw.as_bytes().to_vec(),
            _ => {
                return Err(format!(
                    "{}: a file or raw bytes, one of them",
                    proposal.name
                ));
            }
        };
        std::fs::write(dir.join(file_name(&id)), bytes).map_err(|e| e.to_string())?;
        writer
            .ingest_proposals(&dir, catalogue.as_ref(), proposal.received_at_ms)
            .map_err(|e| e.to_string())?;
        if proposal.dismissed
            && !writer
                .dismiss_proposal(&id, proposal.received_at_ms + 60_000)
                .map_err(|e| e.to_string())?
        {
            return Err(format!("{}: not dismissed", proposal.name));
        }
    }
    writer
        .expire_proposals(script.now_ms)
        .map_err(|e| e.to_string())?;
    Ok(())
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

/// One card, named for its fixture: issued for a request, by an edit of a
/// card an earlier step named, or from the seeded proposal of that name.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CardStep {
    name: String,
    #[serde(default)]
    request: Option<CardRequest>,
    #[serde(default)]
    edit: Option<CardEdit>,
    #[serde(default)]
    proposal: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CardEdit {
    of: String,
    change: CardChange,
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

/// Issues the script's cards through core's `card::issue` and `card::edit`
/// with the approved catalogue, the registry's subjects and `seq`, and the
/// board's quota, as the window's store does; the ids are fixed, so a card
/// fixture names the same card on every run.
fn issue_cards(
    home: &Path,
    relay: &RelayHome,
    board: Option<&Board>,
    script: &Script,
    now_ms: i64,
) -> Result<BTreeMap<String, WorkOrderCard>, String> {
    let steps = &script.cards;
    let state = relay.read_approved();
    let approved = state
        .approved()
        .ok_or("the home's catalogue is not approved")?
        .clone();
    let current =
        load_relay_floor(&relay.relay_floor(&state)).map_err(|e| format!("relay floor: {e}"))?;
    let mut registry = WindowRegistry::open(home, &[], now_ms);
    let mut issued: BTreeMap<String, IssuedCard> = BTreeMap::new();
    for (index, step) in steps.iter().enumerate() {
        let id = format!("{:032x}", index + 1);
        if let Some(name) = &step.proposal {
            let (index, seeded) = script
                .proposals
                .iter()
                .enumerate()
                .find(|(_, p)| p.name == *name)
                .ok_or_else(|| format!("{}: no proposal named {name}", step.name))?;
            let order = registry
                .proposal_order(&seeded_id(seeded, index), now_ms)
                .map_err(|e| format!("{}: {}", step.name, e.message))?
                .map_err(|refusal| format!("{}: {refusal:?}", step.name))?;
            let (subjects, highest) = registry
                .card_reads(&order.template)
                .map_err(|e| format!("{}: {}", step.name, e.message))?;
            let held = issued
                .values()
                .filter(|card| card.card().template == order.template)
                .map(IssuedCard::seq);
            let inputs = CardInputs {
                approved: &approved,
                current: &current,
                subjects: &subjects,
                quota: board.map(|b| &b.quota),
                config_dir: None,
                now_ms,
            };
            let card = card::issue_from_proposal(&id, reserved_seq(highest, held), &order, &inputs)
                .map_err(|refusal| format!("{}: refused {refusal:?}", step.name))?;
            issued.insert(step.name.clone(), card);
            continue;
        }
        let previous = match &step.edit {
            Some(edit) => Some(
                issued
                    .get(&edit.of)
                    .ok_or_else(|| format!("{}: no card named {}", step.name, edit.of))?,
            ),
            None => None,
        };
        let template = match (&step.request, previous) {
            (Some(request), None) => request.template.clone(),
            (None, Some(previous)) => previous.card().template.clone(),
            _ => return Err(format!("{}: a request or an edit, one of them", step.name)),
        };
        let (subjects, highest) = registry
            .card_reads(&template)
            .map_err(|e| format!("{}: {}", step.name, e.message))?;
        let inputs = CardInputs {
            approved: &approved,
            current: &current,
            subjects: &subjects,
            quota: board.map(|b| &b.quota),
            config_dir: None,
            now_ms,
        };
        let answer = match (&step.request, previous, &step.edit) {
            (Some(request), _, _) => {
                let held = issued
                    .values()
                    .filter(|card| card.card().template == template)
                    .map(IssuedCard::seq);
                card::issue(&id, reserved_seq(highest, held), request, &inputs)
            }
            (None, Some(previous), Some(edit)) => card::edit(previous, &edit.change, &id, &inputs),
            _ => return Err(format!("{}: a request or an edit, one of them", step.name)),
        };
        let card = answer.map_err(|refusal| format!("{}: refused {refusal:?}", step.name))?;
        issued.insert(step.name.clone(), card);
    }
    Ok(issued
        .into_iter()
        .map(|(name, issued)| (name, issued.card().clone()))
        .collect())
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
        // A script of cards names proposals an earlier run seeded.
        if script.cards.is_empty() {
            seed_proposals(&mut writer, home, &relay, script)?;
        }
    }
    let board: Option<Board> = match &script.board_file {
        Some(path) => Some(
            serde_json::from_str(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
                .map_err(|e| format!("board: {e}"))?,
        ),
        None => None,
    };
    if !script.cards.is_empty() {
        let cards = issue_cards(home, &relay, board.as_ref(), script, script.now_ms)?;
        return serde_json::to_string_pretty(&cards).map_err(|e| e.to_string());
    }
    if let Some(meeting) = &script.meeting {
        return print_meeting(home, board.as_ref(), meeting, script.now_ms);
    }
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
