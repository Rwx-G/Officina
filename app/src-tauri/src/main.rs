//! `officina-app`: the board window (AD-19). It links `officina-host` and runs
//! the collector in-process (AD-2); it never shells out to the CLI.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::mpsc::{RecvTimeoutError, Sender};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use officina_core::board::Board;
use officina_core::focus::FocusOutcome;
use officina_core::ipc::{IpcError, IpcErrorCode};
use officina_core::registry::confirm::{AuthorAction, SubjectOrigin};
use officina_core::registry::view::{RegistryView, SubjectView};
use officina_core::relay::view::RelayView;
use officina_host::collector::Collector;
use officina_host::files::now_ms;
use officina_host::focus::{
    FocusContext, attach_command, focus_card, log_outcome, log_wrong_window,
};
use officina_host::paths::Paths;
use officina_host::relay::authority::{self, AuthorityError};
use officina_host::relay::home::RelayHome;
use officina_host::watch::{
    AUTHOR_WAIT, AuthorAnswer, AuthorCommand, AuthorRequest, CollectorMsg, Emit, spawn,
};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

/// Event carrying each new board to the window.
const BOARD_EVENT: &str = "board://updated";
/// Event carrying each new relay view to the window (AD-35).
const RELAY_EVENT: &str = "relay://updated";
/// Event carrying each new registry view to the window (AD-54).
const REGISTRY_EVENT: &str = "registry://updated";

struct AppState {
    board: Arc<RwLock<Option<Board>>>,
    relay: Arc<RwLock<Option<RelayView>>>,
    /// The last registry view, for `get_registry`'s first paint.
    registry: Arc<RwLock<Option<RegistryView>>>,
    /// The relay home the collector reads, which the authority writes.
    relay_home: RelayHome,
    sender: std::sync::Mutex<std::sync::mpsc::Sender<CollectorMsg>>,
    /// The collector's current paths: a reloaded floor can move the config
    /// dir that focus and attach pass to `claude`.
    paths: Arc<RwLock<Paths>>,
    claude: Option<PathBuf>,
}

impl AppState {
    /// What focus and attach need, from the paths in use now.
    fn focus_context(&self) -> Option<FocusContext> {
        let paths = self.paths.read().ok()?;
        Some(FocusContext::new(&paths, self.claude.clone()))
    }

    /// A sender for a command that runs off the main thread.
    fn collector(&self) -> Option<Sender<CollectorMsg>> {
        self.sender.lock().ok().map(|sender| sender.clone())
    }
}

/// The window's events (AD-5): what the collector thread emits goes to the
/// page, and each new board also sets the title and the taskbar overlay.
struct WindowEvents(AppHandle);

impl Emit for WindowEvents {
    fn board(&self, board: &Board) {
        let _ = self.0.emit(BOARD_EVENT, board);
        if let Some(window) = self.0.get_webview_window("main") {
            let _ = window.set_title(&window_title(board));
            let overlay = (board.waiting_count > 0).then(waiting_dot);
            let _ = window.set_overlay_icon(overlay);
        }
    }

    fn relay(&self, view: &RelayView) {
        let _ = self.0.emit(RELAY_EVENT, view);
    }

    fn registry(&self, view: &RegistryView) {
        let _ = self.0.emit(REGISTRY_EVENT, view);
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
    let not_run = || {
        IpcError::new(
            IpcErrorCode::Timeout,
            format!(
                "the registry did not answer within {} ms; nothing was changed",
                wait.as_millis()
            ),
        )
    };
    let sender = sender.ok_or_else(|| IpcError::internal("the collector is not running"))?;
    let (request, answer) = AuthorRequest::new(command, wait);
    sender
        .send(CollectorMsg::Author(request))
        .map_err(|_| IpcError::internal("the collector has stopped"))?;
    match answer.recv_timeout(wait) {
        Ok(reply) => reply,
        Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => Err(not_run()),
    }
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
    match ask(state, AuthorCommand::Act(action)).await? {
        AuthorAnswer::Done { created } => Ok(created),
        AuthorAnswer::Archived(_) => {
            Err(IpcError::internal("the collector answered another request"))
        }
    }
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

/// The archived subjects the author may reopen, read on the collector's
/// connection (AD-54).
#[tauri::command]
async fn list_archived(state: State<'_, AppState>) -> Result<Vec<SubjectView>, IpcError> {
    match ask(state, AuthorCommand::ListArchived).await? {
        AuthorAnswer::Archived(listed) => Ok(listed),
        AuthorAnswer::Done { .. } => {
            Err(IpcError::internal("the collector answered another request"))
        }
    }
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

/// Approves or rejects a `human` action's request (FR-9, AD-35); the
/// authority refuses one that is unknown, decided, expired or does not
/// rebuild from the approved catalogue (D-27).
#[tauri::command]
async fn decide_approval(
    state: State<'_, AppState>,
    id: String,
    approve: bool,
) -> Result<(), IpcError> {
    with_authority(state, move |home| {
        authority::decide_approval(home, &id, approve)
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

/// The taskbar overlay: an amber dot with a dark rim, drawn in code so no
/// asset can drift from the palette. Visible with the window behind an editor.
fn waiting_dot() -> tauri::image::Image<'static> {
    const SIZE: u32 = 16;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    let center = (SIZE as f32 - 1.0) / 2.0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let d = ((x as f32 - center).powi(2) + (y as f32 - center).powi(2)).sqrt();
            let pixel: [u8; 4] = if d <= 5.5 {
                [0xF5, 0xB7, 0x31, 0xFF]
            } else if d <= 7.5 {
                [0x2A, 0x24, 0x1C, 0xFF]
            } else {
                [0, 0, 0, 0]
            };
            rgba.extend_from_slice(&pixel);
        }
    }
    tauri::image::Image::new_owned(rgba, SIZE, SIZE)
}

fn window_title(board: &Board) -> String {
    match board.waiting_count {
        0 => "Officina".to_owned(),
        n => format!("Officina ({n} waiting)"),
    }
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
            let collector = Collector::new(floor_arg().as_deref())?;
            let claude = collector.claude().map(std::path::Path::to_path_buf);
            let relay_home = collector.relay().clone();
            // From now on, a decision the window did not make shows (D-173).
            if let Err(e) = authority::ensure_record(&relay_home) {
                eprintln!("officina: the decision record could not be started: {e}");
            }
            let watch = spawn(collector, WindowEvents(app.handle().clone()))?;
            app.manage(AppState {
                board: watch.board,
                relay: watch.relay,
                registry: watch.registry,
                relay_home,
                sender: std::sync::Mutex::new(watch.sender),
                paths: watch.paths,
                claude,
            });
            Ok(())
        })
        .on_window_event(|window, event| {
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
            reopen_subject
        ])
        .run(tauri::generate_context!())
        .expect("failed to run the Officina window");
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
        for command in [
            "get_relay",
            "approve_catalogue",
            "decide_approval",
            "set_relay_stopped",
            "acknowledge_anomaly",
            "get_registry",
            "list_archived",
            "create_subject",
            "rename_subject",
            "merge_subjects",
            "file_sessions",
            "archive_subject",
            "reopen_subject",
        ] {
            assert!(declared.contains(command), "{command}");
        }
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
        assert_eq!(super::window_title(&board), "Officina (3 waiting)");
        let none = build_board(&BoardInput::default(), 10);
        assert_eq!(super::window_title(&none), "Officina");
    }
}
