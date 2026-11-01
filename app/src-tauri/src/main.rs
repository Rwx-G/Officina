//! `officina-app`: the board window (AD-19). It links `officina-host` and runs
//! the collector in-process (AD-2); it never shells out to the CLI.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod taskbar;

// A build that links a mutant of AD-95 does not compile (D-148): the
// mutants exist for CI's `mutants` job, never for a window, release or not.
const _: () = assert!(!officina_core::MUTANTS_ENABLED);

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use officina_core::board::Board;
use officina_core::focus::FocusOutcome;
use officina_core::ipc::{IpcError, IpcErrorCode};
use officina_core::meeting::{AnswerResult, Delivery, MeetingView};
use officina_core::orders::card::{CardAnswer, CardChange, CardRequest, LaunchAnswer};
use officina_core::orders::orchestrator::StartAnswer;
use officina_core::orders::permission::WorkOrderPermission;
use officina_core::orders::view::OrchestratorView;
use officina_core::registry::confirm::{AuthorAction, ConfirmItem, ItemAnswer, SubjectOrigin};
use officina_core::registry::types::{AuthorReason, Settlement};
use officina_core::registry::view::{DiscardedSession, RegistryView, SubjectView};
use officina_core::relay::auto::AutoMode;
use officina_core::relay::view::RelayView;
use officina_core::workshop::image::BuildAnswer;
use officina_host::collector::Collector;
use officina_host::files::now_ms;
use officina_host::focus::{
    FocusContext, attach_command, focus_card, log_outcome, log_wrong_window,
};
use officina_host::meeting::AnswerFrom;
use officina_host::orders;
use officina_host::paths::Paths;
use officina_host::relay::authority::{self, AuthorityError};
use officina_host::relay::home::RelayHome;
use officina_host::spawn::BaseEnv;
use officina_host::watch::{AUTHOR_WAIT, AuthorAnswer, AuthorCommand, CollectorMsg, Emit, spawn};
use officina_host::workshop::build::BuildContext;
use officina_host::workshop::threads::WorkshopThreads;
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WindowEvent};

/// Event carrying each new board to the window.
const BOARD_EVENT: &str = "board://updated";
/// Event carrying each new relay view to the window (AD-35).
const RELAY_EVENT: &str = "relay://updated";
/// Event carrying each new registry view to the window (AD-54).
const REGISTRY_EVENT: &str = "registry://updated";
/// Event carrying each new Orchestrator view to the window (AD-92).
const ORCHESTRATOR_EVENT: &str = "orchestrator://updated";
/// How long closing the window waits for the briefing's reading to be
/// recorded (AD-83): the collector drops a request within
/// `AUTHOR_HEADROOM` of its deadline, so this leaves it a second to start,
/// and a busy collector costs the close two seconds at most.
const CLOSE_WAIT: Duration = Duration::from_secs(2);

struct AppState {
    board: Arc<RwLock<Option<Board>>>,
    relay: Arc<RwLock<Option<RelayView>>>,
    /// The last registry view, for `get_registry`'s first paint.
    registry: Arc<RwLock<Option<RegistryView>>>,
    /// The last Orchestrator view, for `get_orchestrator`'s first paint.
    orchestrator: Arc<RwLock<Option<OrchestratorView>>>,
    /// The relay home the collector reads, which the authority writes.
    relay_home: RelayHome,
    sender: std::sync::Mutex<std::sync::mpsc::Sender<CollectorMsg>>,
    /// The collector's current paths: a reloaded floor can move the config
    /// dir that focus and attach pass to `claude`.
    paths: Arc<RwLock<Paths>>,
    claude: Option<PathBuf>,
    /// The environment the attach console's own is built from (D-86).
    base_env: BaseEnv,
    /// The window's executable directory, read once at start from
    /// `current_exe()`: the `officina.exe` the orchestrator's MCP config
    /// runs sits there (D-144, D-167).
    exe_dir: Option<PathBuf>,
    /// Threads `workshop` and `workshop-build` (AD-98): the collector never
    /// runs `docker`, so every engine job runs on them.
    workshop: WorkshopThreads,
    /// Set while Build images runs on thread `workshop-build` (AD-99).
    build_busy: Arc<std::sync::atomic::AtomicBool>,
}

impl AppState {
    /// What focus and attach need, from the paths in use now.
    fn focus_context(&self) -> Option<FocusContext> {
        let paths = self.paths.read().ok()?;
        Some(FocusContext::new(
            &paths,
            self.claude.clone(),
            self.base_env.clone(),
        ))
    }

    /// A sender for a command that runs off the main thread.
    fn collector(&self) -> Option<Sender<CollectorMsg>> {
        self.sender.lock().ok().map(|sender| sender.clone())
    }
}

/// The window's events (AD-5): what the collector thread emits goes to the
/// page, and each new board also sets the title, the taskbar overlay and,
/// for a session newly in alert, a flash of the taskbar button (AD-174).
struct WindowEvents {
    app: AppHandle,
    /// The alerted card keys of the last board emitted.
    alerted: std::sync::Mutex<BTreeSet<String>>,
}

impl Emit for WindowEvents {
    fn board(&self, board: &Board) {
        let _ = self.app.emit(BOARD_EVENT, board);
        let announce = self.alerted.lock().is_ok_and(|mut alerted| {
            let announce = taskbar::alerts_to_announce(&alerted, board);
            *alerted = taskbar::alerted_keys(board);
            announce
        });
        if let Some(window) = self.app.get_webview_window("main") {
            let _ = window.set_title(&taskbar::window_title(board));
            let size = taskbar::overlay_size(window.scale_factor().unwrap_or(1.0));
            let overlay = taskbar::overlay_for(board, size)
                .map(|badge| tauri::image::Image::new_owned(badge.rgba, badge.size, badge.size));
            let _ = window.set_overlay_icon(overlay);
            // An OS attention request that ends on focus (D-475), not page
            // motion: the reduced-motion setting does not apply to it.
            if announce {
                let _ =
                    window.request_user_attention(Some(tauri::UserAttentionType::Informational));
            }
        }
    }

    fn relay(&self, view: &RelayView) {
        let _ = self.app.emit(RELAY_EVENT, view);
    }

    fn registry(&self, view: &RegistryView) {
        let _ = self.app.emit(REGISTRY_EVENT, view);
    }

    fn orchestrator(&self, view: &OrchestratorView) {
        let _ = self.app.emit(ORCHESTRATOR_EVENT, view);
    }
}

/// Sends `command` to the collector thread, the registry's only writer
/// (AD-38), and waits at most `wait` for its answer. Past `wait`, or when
/// the collector dropped the request unrun near its deadline (D-92), the
/// window reports a timeout: the action did not run and will not.
fn ask_collector(
    sender: Option<Sender<CollectorMsg>>,
    command: AuthorCommand,
    wait: Duration,
) -> Result<AuthorAnswer, IpcError> {
    let sender = sender.ok_or_else(|| IpcError::internal("the collector is not running"))?;
    officina_host::watch::ask(&sender, command, wait)
}

/// Runs an author request off the main thread (AD-3), waiting at most
/// [`AUTHOR_WAIT`].
async fn ask(state: State<'_, AppState>, command: AuthorCommand) -> Result<AuthorAnswer, IpcError> {
    let sender = state.collector();
    tauri::async_runtime::spawn_blocking(move || ask_collector(sender, command, AUTHOR_WAIT))
        .await
        .map_err(|e| IpcError::internal(e.to_string()))?
}

/// Runs one author action; the subject it created, if any.
async fn act(state: State<'_, AppState>, action: AuthorAction) -> Result<Option<i64>, IpcError> {
    ask(state, AuthorCommand::Act(action)).await?.into_done()
}

fn card_for(state: &AppState, key: &str) -> Option<officina_core::board::Card> {
    let board = state.board.read().ok()?;
    board
        .as_ref()?
        .columns
        .all()
        .find(|c| c.key == key)
        .cloned()
}

#[tauri::command]
fn get_board(state: State<'_, AppState>) -> Option<Board> {
    state.board.read().ok().and_then(|b| b.clone())
}

/// The last relay view, for the first paint; later ones come as events.
#[tauri::command]
fn get_relay(state: State<'_, AppState>) -> Option<RelayView> {
    state.relay.read().ok().and_then(|r| r.clone())
}

/// The last registry view, for the first paint, from the cache: never
/// through the collector (D-92); later ones come as events.
#[tauri::command]
fn get_registry(state: State<'_, AppState>) -> Option<RegistryView> {
    state.registry.read().ok().and_then(|r| r.clone())
}

/// The last Orchestrator view, for the first paint, from the cache: never
/// through the collector; later ones come as events (AD-92).
#[tauri::command]
fn get_orchestrator(state: State<'_, AppState>) -> Option<OrchestratorView> {
    state.orchestrator.read().ok().and_then(|o| o.clone())
}

/// Records that the author read the briefing now, on leaving the
/// Orchestrator view (AD-83): the next briefing counts from here.
#[tauri::command]
async fn mark_briefing_read(state: State<'_, AppState>) -> Result<(), IpcError> {
    ask(state, AuthorCommand::MarkBriefingRead)
        .await?
        .into_done()
        .map(|_| ())
}

/// The archived subjects the author may reopen, read on the collector's
/// connection (AD-54).
#[tauri::command]
async fn list_archived(state: State<'_, AppState>) -> Result<Vec<SubjectView>, IpcError> {
    ask(state, AuthorCommand::ListArchived)
        .await?
        .into_archived()
}

/// Creates a subject named `name` (D-95): from the selected sessions,
/// which give it their project and are filed under it, or in a project the
/// registry knows. Returns its id.
#[tauri::command]
async fn create_subject(
    state: State<'_, AppState>,
    name: String,
    origin: SubjectOrigin,
) -> Result<i64, IpcError> {
    act(state, AuthorAction::Create { name, origin })
        .await?
        .ok_or_else(|| IpcError::internal("no subject was created"))
}

#[tauri::command]
async fn rename_subject(
    state: State<'_, AppState>,
    subject: i64,
    name: String,
) -> Result<(), IpcError> {
    act(state, AuthorAction::Rename { subject, name })
        .await
        .map(|_| ())
}

/// Moves the sessions of `source` to `target` and archives `source`
/// (D-55).
#[tauri::command]
async fn merge_subjects(
    state: State<'_, AppState>,
    source: i64,
    target: i64,
) -> Result<(), IpcError> {
    act(state, AuthorAction::Merge { source, target })
        .await
        .map(|_| ())
}

/// Files `sessions` under `subject`, or unfiles them when it is null
/// (D-56).
#[tauri::command]
async fn file_sessions(
    state: State<'_, AppState>,
    sessions: Vec<String>,
    subject: Option<i64>,
) -> Result<(), IpcError> {
    let action = match subject {
        Some(subject) => AuthorAction::File { sessions, subject },
        None => AuthorAction::Unfile { sessions },
    };
    act(state, action).await.map(|_| ())
}

/// Archives `subject`, `routine` when it could have run alone (FR-13,
/// P-10).
#[tauri::command]
async fn archive_subject(
    state: State<'_, AppState>,
    subject: i64,
    routine: bool,
) -> Result<(), IpcError> {
    act(state, AuthorAction::Archive { subject, routine })
        .await
        .map(|_| ())
}

/// Reopens an archived subject as `to_resume` (AD-48, D-58).
#[tauri::command]
async fn reopen_subject(state: State<'_, AppState>, subject: i64) -> Result<(), IpcError> {
    act(state, AuthorAction::Reopen { subject })
        .await
        .map(|_| ())
}

/// Records the author's verdict on one ended session from the board, or
/// takes it back with `null` (story 4.15, D-536): Done moves its card to
/// Done, Discarded takes it off the board and the scene.
#[tauri::command]
async fn settle_session(
    state: State<'_, AppState>,
    session: String,
    settlement: Option<Settlement>,
) -> Result<(), IpcError> {
    act(
        state,
        AuthorAction::Settle {
            session,
            settlement,
        },
    )
    .await
    .map(|_| ())
}

/// The sessions the author discarded from the board, the latest first,
/// read on the collector's connection (story 4.15).
#[tauri::command]
async fn list_discarded(state: State<'_, AppState>) -> Result<Vec<DiscardedSession>, IpcError> {
    ask(state, AuthorCommand::ListDiscarded)
        .await?
        .into_discarded()
}

/// Confirms triage rows in one transaction (AD-49): each item carries the
/// row's key and the proposal the window showed, and is answered on its
/// own, confirmed, filed without its verdict (D-60) or refused with why.
#[tauri::command]
async fn confirm_triage(
    state: State<'_, AppState>,
    items: Vec<ConfirmItem>,
) -> Result<Vec<ItemAnswer>, IpcError> {
    ask(state, AuthorCommand::Confirm(items))
        .await?
        .into_confirmed()
}

/// Turns auto-archive on, only while its gate holds, or off (FR-12,
/// AD-50): a refusal names the gate's state, and the registry view says why
/// the toggle is disabled.
#[tauri::command]
async fn set_auto_archive(state: State<'_, AppState>, on: bool) -> Result<(), IpcError> {
    ask(state, AuthorCommand::SetAutoArchive(on))
        .await?
        .into_done()
        .map(|_| ())
}

/// Turns the triage model on, only while it can be called and its enable
/// gate holds, which also lifts a pause, or off (FR-10 as amended, AD-50,
/// D-69, D-81): a refusal says why, and the registry view says why the
/// toggle is disabled.
#[tauri::command]
async fn set_triage_model(state: State<'_, AppState>, on: bool) -> Result<(), IpcError> {
    ask(state, AuthorCommand::SetTriageModel(on))
        .await?
        .into_done()
        .map(|_| ())
}

/// Turns auto approval on, only while its gate holds when the registry
/// writes, or off (AD-88): a refusal says why, and the Relay view offers
/// the toggle only while the gate holds. Each change is journaled
/// `auto_approval_set`.
#[tauri::command]
async fn set_auto_approval(state: State<'_, AppState>, on: bool) -> Result<(), IpcError> {
    ask(state, AuthorCommand::SetAutoApproval(on))
        .await?
        .into_done()
        .map(|_| ())
}

/// Sets auto approval's mode (D-162): `rule` only while the rule offer
/// holds when the registry writes, `review` always.
#[tauri::command]
async fn set_auto_mode(state: State<'_, AppState>, mode: AutoMode) -> Result<(), IpcError> {
    ask(state, AuthorCommand::SetAutoMode(mode))
        .await?
        .into_done()
        .map(|_| ())
}

/// Marks an auto-approved run of the last 7 days "should not have run"
/// (AD-88, FR-9): auto approval turns off and the mode returns to
/// `review`.
#[tauri::command]
async fn mark_should_not_have_run(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<(), IpcError> {
    ask(state, AuthorCommand::MarkShouldNotHaveRun(run_id))
        .await?
        .into_done()
        .map(|_| ())
}

/// Queues a dry run of the triage model on the author's past
/// confirmations (AD-52, D-92): answered once queued, with how many cases
/// it holds; its progress and its result come in the registry view, never
/// as a proposal.
#[tauri::command]
async fn evaluate_triage_model(state: State<'_, AppState>) -> Result<u32, IpcError> {
    ask(state, AuthorCommand::Evaluate).await?.into_evaluating()
}

/// Issues a work order card for `request` (AD-78): the card, with the
/// argument vector exactly as it would run, or the refusal.
#[tauri::command]
async fn issue_card(
    state: State<'_, AppState>,
    request: CardRequest,
) -> Result<CardAnswer, IpcError> {
    ask(state, AuthorCommand::IssueCard(request))
        .await?
        .into_card()
}

/// A new card in place of `card_id` with one value, from its list, or the
/// subject changed (AD-78); the old card is superseded.
#[tauri::command]
async fn edit_card(
    state: State<'_, AppState>,
    card_id: String,
    change: CardChange,
) -> Result<CardAnswer, IpcError> {
    ask(state, AuthorCommand::EditCard { card_id, change })
        .await?
        .into_card()
}

/// Switches the work orders of card `card_id`'s project to `permission`
/// (story 5.24, D-983, D-985) and answers the card issued again with it.
/// The page sends Officina's default on a pointer click only (D-986).
#[tauri::command]
async fn set_work_order_permission(
    state: State<'_, AppState>,
    card_id: String,
    permission: WorkOrderPermission,
) -> Result<CardAnswer, IpcError> {
    ask(
        state,
        AuthorCommand::SetWorkOrderPermission {
            card_id,
            permission,
        },
    )
    .await?
    .into_card()
}

/// Issues a work order card from the pending proposal `proposal_id`
/// (AD-91, D-152): its stored values through the same validation as the
/// author's, under a "proposal" label; or why it opens none.
#[tauri::command]
async fn issue_card_from_proposal(
    state: State<'_, AppState>,
    proposal_id: String,
) -> Result<CardAnswer, IpcError> {
    ask(state, AuthorCommand::IssueCardFromProposal { proposal_id })
        .await?
        .into_card()
}

/// Dismisses the pending or invalid proposal `proposal_id` (FR-12).
#[tauri::command]
async fn dismiss_proposal(state: State<'_, AppState>, proposal_id: String) -> Result<(), IpcError> {
    ask(state, AuthorCommand::DismissProposal { proposal_id })
        .await?
        .into_done()
        .map(|_| ())
}

/// Launches card `card_id`, the prompt shown hashing to `prompt_sha256`
/// (AD-78, AD-81): the checks and the record on the collector thread, the
/// spawn on this command's blocking thread, never the collector's (AD-3).
#[tauri::command]
async fn launch_work_order(
    state: State<'_, AppState>,
    card_id: String,
    prompt_sha256: String,
) -> Result<LaunchAnswer, IpcError> {
    let sender = state
        .collector()
        .ok_or_else(|| IpcError::internal("the collector is not running"))?;
    tauri::async_runtime::spawn_blocking(move || {
        orders::launch_card(&sender, &card_id, &prompt_sha256)
    })
    .await
    .map_err(|e| IpcError::internal(e.to_string()))?
}

/// Starts the orchestrator in a console of its own, or brings the live one
/// to the front (AD-89, AD-92): the decision and the record on the
/// collector thread, the spawn on this command's blocking thread (AD-3).
#[tauri::command]
async fn start_orchestrator(state: State<'_, AppState>) -> Result<StartAnswer, IpcError> {
    let sender = state
        .collector()
        .ok_or_else(|| IpcError::internal("the collector is not running"))?;
    let exe_dir = state
        .exe_dir
        .clone()
        .ok_or_else(|| IpcError::internal("the window's executable directory is unknown"))?;
    tauri::async_runtime::spawn_blocking(move || {
        orders::orchestrator::start_orchestrator(&sender, &exe_dir)
    })
    .await
    .map_err(|e| IpcError::internal(e.to_string()))?
}

/// Build images (AD-99, AD-100): a pointer click in the window (D-28,
/// story 6.20's button). The collector grants it; thread `workshop-build`
/// checks the engine and the files, then builds; the answer says whether
/// the builds were queued or why not.
#[tauri::command]
async fn build_images(state: State<'_, AppState>) -> Result<BuildAnswer, IpcError> {
    let sender = state
        .collector()
        .ok_or_else(|| IpcError::internal("the collector is not running"))?;
    let thread = state.workshop.build.clone();
    let context = BuildContext {
        exe_dir: state.exe_dir.clone(),
        linux_override: std::env::var_os(officina_host::workshop::ENV_LINUX_BIN_DIR),
        docker: officina_host::workshop::engine::resolve(),
        reader: Arc::new(officina_host::workshop::proxy::WinHttpReader),
        busy: Arc::clone(&state.build_busy),
        now_ms,
    };
    tauri::async_runtime::spawn_blocking(move || {
        officina_host::workshop::build::build_images(&sender, &thread, context, Box::new(|| {}))
    })
    .await
    .map_err(|e| IpcError::internal(e.to_string()))?
}

/// The meeting of `key` (AD-84, D-138): a card key, `sid:<session id>`
/// or `subject:<id>`. The collector answers what it holds; the event
/// lines, the journal, `git` and `claude logs` are read on this command's
/// blocking thread, never the collector's (AD-3).
#[tauri::command]
async fn open_meeting(state: State<'_, AppState>, key: String) -> Result<MeetingView, IpcError> {
    let sender = state
        .collector()
        .ok_or_else(|| IpcError::internal("the collector is not running"))?;
    tauri::async_runtime::spawn_blocking(move || {
        officina_host::meeting::open_meeting(&sender, &key)
    })
    .await
    .map_err(|e| IpcError::internal(e.to_string()))?
}

/// Sends the author's answer to the meeting of `key` (AD-85), or the
/// answer proposal `proposal_id` (AD-182), never both: a proposal carries
/// no text, the collector reading the stored one (D-575). The collector
/// checks the delivery the panel showed against the latest board and
/// grants the start; the console starts on this command's blocking thread;
/// a resume is answered once the session turned busy or its window passed.
#[tauri::command]
async fn send_answer(
    state: State<'_, AppState>,
    key: String,
    expected: Delivery,
    text: Option<String>,
    proposal_id: Option<String>,
) -> Result<AnswerResult, IpcError> {
    let from = match (text, proposal_id) {
        (Some(text), None) => AnswerFrom::Author(text),
        (None, Some(id)) => AnswerFrom::Proposal(id),
        _ => {
            return Err(IpcError::new(
                IpcErrorCode::Refused,
                "an answer carries either the author's text or a proposal's id",
            ));
        }
    };
    let sender = state
        .collector()
        .ok_or_else(|| IpcError::internal("the collector is not running"))?;
    tauri::async_runtime::spawn_blocking(move || {
        officina_host::meeting::send_answer(&sender, &key, expected, from)
    })
    .await
    .map_err(|e| IpcError::internal(e.to_string()))?
}

#[tauri::command]
fn refresh(state: State<'_, AppState>) {
    if let Ok(sender) = state.sender.lock() {
        let _ = sender.send(CollectorMsg::Refresh);
    }
}

#[tauri::command]
async fn focus(state: State<'_, AppState>, key: String) -> Result<FocusOutcome, IpcError> {
    let Some(card) = card_for(&state, &key) else {
        return Ok(FocusOutcome::Failed(
            "this session is no longer on the board".into(),
        ));
    };
    let Some(ctx) = state.focus_context() else {
        return Ok(FocusOutcome::Failed(
            "the window's state is unavailable".into(),
        ));
    };
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        let outcome = focus_card(&card, &ctx);
        log_outcome(&ctx.state_dir, &card, &outcome, now_ms());
        outcome
    })
    .await
    .map_err(|e| IpcError::internal(e.to_string()))?;
    Ok(outcome)
}

#[tauri::command]
fn report_wrong_window(state: State<'_, AppState>, key: String) {
    if let Some(ctx) = state.focus_context() {
        log_wrong_window(&ctx.state_dir, &key, now_ms());
    }
}

#[tauri::command]
fn copy_attach_command(state: State<'_, AppState>, key: String) -> Option<String> {
    let ctx = state.focus_context()?;
    card_for(&state, &key).and_then(|c| attach_command(&c, &ctx))
}

/// Runs an authority call off the main thread (AD-3), then asks the
/// collector for a relay view at once, whether the call succeeded or not
/// (AD-35). Every failure reaches the window as one [`IpcError`].
async fn with_authority<T: Send + 'static>(
    state: State<'_, AppState>,
    call: impl FnOnce(&RelayHome) -> Result<T, AuthorityError> + Send + 'static,
) -> Result<T, IpcError> {
    let home = state.relay_home.clone();
    let collector = state.collector();
    tauri::async_runtime::spawn_blocking(move || {
        let result = call(&home);
        if let Some(sender) = collector {
            let _ = sender.send(CollectorMsg::RelayChanged);
        }
        result.map_err(|e| e.ipc())
    })
    .await
    .map_err(|e| IpcError::internal(e.to_string()))?
}

/// Approves the relay's catalogue when `hash` is still the current one
/// (AD-24, D-8). Returns the approved hash.
#[tauri::command]
async fn approve_catalogue(state: State<'_, AppState>, hash: String) -> Result<String, IpcError> {
    with_authority(state, move |home| {
        authority::approve_catalogue(home, &hash).map(|approved| approved.hash)
    })
    .await
}

/// Approves or rejects a `human` or `auto` action's request (FR-9, AD-35,
/// AD-86); rejecting an `auto` one takes its reason. The authority refuses
/// a request that is unknown, decided, expired, being reviewed or does not
/// rebuild from the approved catalogue (D-27, D-157), and a reason that
/// does not fit its class.
#[tauri::command]
async fn decide_approval(
    state: State<'_, AppState>,
    id: String,
    approve: bool,
    reason: Option<AuthorReason>,
) -> Result<(), IpcError> {
    with_authority(state, move |home| {
        authority::decide_approval(home, &id, approve, reason)
    })
    .await
}

/// Stops or resumes the relay (FR-11, AD-34, D-26). Returns whether the
/// state changed.
#[tauri::command]
async fn set_relay_stopped(state: State<'_, AppState>, stopped: bool) -> Result<bool, IpcError> {
    with_authority(state, move |home| {
        authority::set_relay_stopped(home, stopped)
    })
    .await
}

/// Acknowledges an anomaly the Relay view lists (D-173): a decision the
/// window did not make, which doctor then no longer fails on.
#[tauri::command]
async fn acknowledge_anomaly(state: State<'_, AppState>, key: String) -> Result<(), IpcError> {
    with_authority(state, move |home| {
        authority::acknowledge_anomaly(home, &key)
    })
    .await
}

fn floor_arg() -> Option<PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    let i = args.iter().position(|a| a == "--floor")?;
    args.get(i + 1).map(PathBuf::from)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            // Every `claude` the window starts has its environment built
            // from the window's own, read once here (D-86), by its kind's
            // rule (D-561).
            let collector = Collector::new(floor_arg().as_deref())?;
            let claude = collector.claude().map(std::path::Path::to_path_buf);
            let base_env = collector.base_env().clone();
            let relay_home = collector.relay().clone();
            let officina_home = collector.paths().officina_home.clone();
            // D-167: read once here; tests pass a folder of their own.
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf));
            // From now on, a decision the window did not make shows (D-173).
            if let Err(e) = authority::ensure_record(&relay_home) {
                eprintln!("officina: the decision record could not be started: {e}");
            }
            let events = WindowEvents {
                app: app.handle().clone(),
                alerted: std::sync::Mutex::new(BTreeSet::new()),
            };
            let watch = spawn(collector, events)?;
            let workshop = WorkshopThreads::spawn()?;
            workshop.read_engine_at_start(
                officina_host::workshop::engine::resolve(),
                base_env.vars().to_vec(),
                officina_home,
                watch.sender.clone(),
            );
            app.manage(AppState {
                board: watch.board,
                relay: watch.relay,
                registry: watch.registry,
                orchestrator: watch.orchestrator,
                relay_home,
                sender: std::sync::Mutex::new(watch.sender),
                paths: watch.paths,
                claude,
                base_env,
                exe_dir,
                workshop,
                build_busy: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window is a reading of the briefing (AD-83).
            if let WindowEvent::CloseRequested { .. } = event {
                if let Some(state) = window.try_state::<AppState>() {
                    let _ = ask_collector(
                        state.collector(),
                        AuthorCommand::MarkBriefingRead,
                        CLOSE_WAIT,
                    );
                }
                return;
            }
            let visible = match event {
                WindowEvent::Resized(_) => !window.is_minimized().unwrap_or(false),
                WindowEvent::Focused(true) => true,
                _ => return,
            };
            if let Some(state) = window.try_state::<AppState>()
                && let Ok(sender) = state.sender.lock()
            {
                let _ = sender.send(CollectorMsg::Visibility(visible));
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_board,
            get_relay,
            refresh,
            focus,
            report_wrong_window,
            copy_attach_command,
            approve_catalogue,
            decide_approval,
            set_relay_stopped,
            acknowledge_anomaly,
            get_registry,
            list_archived,
            create_subject,
            rename_subject,
            merge_subjects,
            file_sessions,
            archive_subject,
            reopen_subject,
            confirm_triage,
            evaluate_triage_model,
            set_triage_model,
            set_auto_archive,
            set_auto_approval,
            set_auto_mode,
            mark_should_not_have_run,
            settle_session,
            list_discarded,
            issue_card,
            edit_card,
            set_work_order_permission,
            issue_card_from_proposal,
            dismiss_proposal,
            launch_work_order,
            get_orchestrator,
            mark_briefing_read,
            start_orchestrator,
            open_meeting,
            send_answer,
            build_images
        ])
        .build(tauri::generate_context!())
        .expect("failed to build the Officina window")
        .run(|app, event| {
            // A clean exit leaves no liveness marker (D-579): the collector
            // thread ends with the process, before its own removal runs.
            if let RunEvent::Exit = event
                && let Some(state) = app.try_state::<AppState>()
            {
                officina_host::relay::prompts::remove_marker(&state.relay_home);
            }
        });
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    /// The quoted names between `start` and the next `end` in `text`.
    fn quoted_names(text: &str, start: &str, end: &str) -> BTreeSet<String> {
        let from = text.find(start).expect(start) + start.len();
        let list = &text[from..from + text[from..].find(end).expect(end)];
        list.split('"')
            .skip(1)
            .step_by(2)
            .map(str::to_owned)
            .collect()
    }

    /// The words of `tauri::generate_handler![...]` in this file.
    fn handled_commands() -> BTreeSet<String> {
        let text = include_str!("main.rs");
        let start = "generate_handler![";
        let from = text.find(start).unwrap() + start.len();
        let list = &text[from..from + text[from..].find(']').unwrap()];
        list.split(',')
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .collect()
    }

    /// AD-19, AD-35: a command declared in `build.rs` but not allowed in the
    /// capability file cannot be called by the page, and an allowed one that
    /// is not declared or not handled is a typo. Each list is read from its
    /// file, so a command added to one place only fails here.
    #[test]
    fn every_declared_command_is_allowed() {
        let declared = quoted_names(
            include_str!("../build.rs"),
            "const COMMANDS: &[&str] = &[",
            "];",
        );
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        let allowed: BTreeSet<String> = capability["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|p| p.as_str())
            .filter_map(|p| p.strip_prefix("allow-"))
            .map(|p| p.replace('-', "_"))
            .collect();
        assert!(!declared.is_empty());
        assert_eq!(
            allowed, declared,
            "capabilities/default.json against build.rs"
        );
        assert_eq!(
            handled_commands(),
            declared,
            "generate_handler! against build.rs"
        );
        // The frontend's calls, read from `ipc.ts`, the only module that
        // invokes a command: each is declared, and each declared command
        // is called, so neither list can be edited alone.
        let invoked = invoked_commands(include_str!("../../src/lib/ipc.ts"));
        assert!(!invoked.is_empty());
        assert_eq!(invoked, declared, "ipc.ts invocations against build.rs");
    }

    /// The command names `ipc.ts` passes to `invoke`, typed or not.
    fn invoked_commands(source: &str) -> BTreeSet<String> {
        source
            .split("invoke")
            .skip(1)
            .filter_map(|rest| {
                let rest = rest.strip_prefix('<').map_or(rest, |typed| {
                    typed.split_once(">(").map_or("", |(_, call)| call)
                });
                let name = rest
                    .strip_prefix("(\"")
                    .or_else(|| rest.strip_prefix('"'))?;
                name.split_once('"').map(|(name, _)| name.to_owned())
            })
            .collect()
    }

    /// AD-38, D-92: a command waits for the collector at most the time it
    /// is given, then reports a timeout; the request it sent carries that
    /// deadline, so the collector drops it unrun.
    #[test]
    fn author_reply_times_out() {
        use officina_core::ipc::IpcErrorCode;
        use officina_host::watch::{AuthorCommand, CollectorMsg};
        use std::time::{Duration, Instant};

        let (sender, collector) = std::sync::mpsc::channel();
        let started = Instant::now();
        let error = super::ask_collector(
            Some(sender),
            AuthorCommand::ListArchived,
            Duration::from_millis(50),
        )
        .unwrap_err();
        assert_eq!(error.code, IpcErrorCode::Timeout, "{error:?}");
        // Security audit Info: a request that started in time may still be
        // applied after the window gave up, so it never says nothing was.
        assert!(error.message.contains("the next view shows"), "{error:?}");
        let waited = started.elapsed();
        assert!(
            waited >= Duration::from_millis(50) && waited < Duration::from_secs(5),
            "{waited:?}"
        );
        match collector.try_recv() {
            Ok(CollectorMsg::Author(request)) => assert!(request.deadline <= Instant::now()),
            other => panic!("{other:?}"),
        }
        // A collector that answers nothing and lets the request go is a
        // timeout too, not a fault.
        let (sender, collector) = std::sync::mpsc::channel();
        drop(collector);
        let gone = super::ask_collector(
            Some(sender),
            AuthorCommand::ListArchived,
            Duration::from_millis(50),
        );
        assert_eq!(gone.unwrap_err().code, IpcErrorCode::Internal);
    }

    /// The directives of a CSP string, by name.
    fn directives(csp: &str) -> std::collections::BTreeMap<&str, Vec<&str>> {
        csp.split(';')
            .filter_map(|directive| {
                let mut words = directive.split_whitespace();
                Some((words.next()?, words.collect()))
            })
            .collect()
    }

    /// Story 2.14 AC 10: the built window allows no inline style (the
    /// Playwright suite runs under this very string and sees no violation);
    /// only Vite's dev server, which injects styles from script, keeps it.
    #[test]
    fn csp_allows_no_inline_style_in_the_built_window() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let security = &config["app"]["security"];
        let built = directives(security["csp"].as_str().unwrap());
        assert_eq!(built["style-src"], ["'self'"]);
        assert_eq!(built["default-src"], ["'self'"]);
        assert!(
            built
                .values()
                .flatten()
                .all(|source| !source.contains("unsafe")),
            "{built:?}"
        );
        let dev = directives(security["devCsp"].as_str().unwrap());
        assert_eq!(dev["style-src"], ["'self'", "'unsafe-inline'"]);
    }

    /// AD-35: the title, and with it the taskbar overlay, count the valid
    /// approval requests beside the sessions that wait.
    #[test]
    fn window_title_counts_approvals() {
        use officina_core::agents::parse_agents;
        use officina_core::board::{BoardInput, build_board};
        use officina_core::relay::approvals::ApprovalView;

        let approval = |id: &str, valid: bool| ApprovalView {
            id: id.to_owned(),
            project: "lab".to_owned(),
            action: "restart".to_owned(),
            class: officina_core::relay::catalogue::ActionClass::Human,
            state: officina_core::relay::approvals::ApprovalState::Pending,
            escalation: None,
            description: String::new(),
            argv: Vec::new(),
            http: None,
            valid,
            invalid_reason: (!valid).then(|| "forged".to_owned()),
            session_id: None,
            session_name: None,
            requested_at_ms: 1,
            expires_at_ms: 2,
            dry_run: None,
            binding: None,
            bound_files: None,
            files_unchanged: None,
        };
        let input = BoardInput {
            agents: Some(
                parse_agents(r#"[{"pid":1,"sessionId":"s","kind":"interactive","status":"waiting","startedAt":1}]"#)
                    .unwrap(),
            ),
            approvals: vec![
                approval("0123456789abcdef-1", true),
                approval("0123456789abcdef-2", true),
                approval("0123456789abcdef-3", false),
            ],
            ..Default::default()
        };
        let board = build_board(&input, 10);
        assert_eq!(super::taskbar::window_title(&board), "Officina (3 waiting)");
        let none = build_board(&BoardInput::default(), 10);
        assert_eq!(super::taskbar::window_title(&none), "Officina");
    }
}
