//! `officina-app`: the board window (AD-19). It links `officina-host` and runs
//! the collector in-process (AD-2); it never shells out to the CLI.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use officina_core::board::Board;
use officina_core::focus::FocusOutcome;
use officina_host::collector::Collector;
use officina_host::files::now_ms;
use officina_host::focus::{
    FocusContext, attach_command, focus_card, log_outcome, log_wrong_window,
};
use officina_host::watch::{CollectorMsg, spawn};
use tauri::{Emitter, Manager, State, WindowEvent};

/// Event carrying each new board to the window.
const BOARD_EVENT: &str = "board://updated";

struct AppState {
    board: Arc<RwLock<Option<Board>>>,
    sender: std::sync::Mutex<std::sync::mpsc::Sender<CollectorMsg>>,
    focus: FocusContext,
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

#[tauri::command]
fn refresh(state: State<'_, AppState>) {
    if let Ok(sender) = state.sender.lock() {
        let _ = sender.send(CollectorMsg::Refresh);
    }
}

#[tauri::command]
async fn focus(state: State<'_, AppState>, key: String) -> Result<FocusOutcome, String> {
    let Some(card) = card_for(&state, &key) else {
        return Ok(FocusOutcome::Failed(
            "this session is no longer on the board".into(),
        ));
    };
    let ctx = state.focus.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        let outcome = focus_card(&card, &ctx);
        log_outcome(&ctx.state_dir, &card, &outcome, now_ms());
        outcome
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(outcome)
}

#[tauri::command]
fn report_wrong_window(state: State<'_, AppState>, key: String) {
    log_wrong_window(&state.focus.state_dir, &key, now_ms());
}

#[tauri::command]
fn copy_attach_command(state: State<'_, AppState>, key: String) -> Option<String> {
    card_for(&state, &key).and_then(|c| attach_command(&c, &state.focus))
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
            let focus = FocusContext::new(&collector.paths, collector.claude.clone());
            let handle = app.handle().clone();
            let watch = spawn(collector, move |board| {
                let _ = handle.emit(BOARD_EVENT, board);
                if let Some(window) = handle.get_webview_window("main") {
                    let _ = window.set_title(&window_title(board));
                    let overlay = (board.waiting_count > 0).then(waiting_dot);
                    let _ = window.set_overlay_icon(overlay);
                }
            })?;
            app.manage(AppState {
                board: watch.board,
                sender: std::sync::Mutex::new(watch.sender),
                focus,
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
            refresh,
            focus,
            report_wrong_window,
            copy_attach_command
        ])
        .run(tauri::generate_context!())
        .expect("failed to run the Officina window");
}
