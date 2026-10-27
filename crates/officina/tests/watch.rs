//! The window's collector thread end to end (story 1.5, AD-3 to AD-5): first
//! board, re-emission on a hook event, no re-emission when nothing changed.
//!
//! The fake claude is given no fixture here (it then lists no session):
//! classification of real fixtures is covered by the `board` CLI tests, and
//! passing a fixture would need a process-wide environment variable.

mod common;

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use officina_core::board::Board;
use officina_host::collector::{Collector, LoadedFloor};
use officina_host::paths::Paths;
use officina_host::watch::{CollectorMsg, spawn};
use officina_host::writers::record_hook;

#[test]
fn emits_first_board_then_only_on_change() {
    let env = common::TestEnv::new();
    let paths = Paths {
        officina_home: env.officina_home(),
        claude_config_dir: env.claude_config(),
        claude_config_override: None,
        floor_file: None,
    };
    let collector = Collector::from_parts(
        paths.clone(),
        LoadedFloor::default(),
        Some(PathBuf::from(common::FAKE_CLAUDE)),
    );
    let (tx, rx) = mpsc::channel::<Board>();
    let handle = spawn(collector, move |b| {
        let _ = tx.send(b.clone());
    })
    .unwrap();

    let first = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("first board");
    assert!(first.stale.is_none());
    assert!(handle.board.read().unwrap().is_some());

    assert!(
        rx.recv_timeout(Duration::from_millis(1600)).is_err(),
        "an unchanged board must not be emitted again"
    );

    let now = officina_host::files::now_ms();
    record_hook(
        &paths,
        br#"{"hook_event_name":"UserPromptSubmit","session_id":"s-gone","cwd":"C:/work/api","prompt":"x"}"#,
        now - 120_000,
        true,
    );
    record_hook(
        &paths,
        br#"{"hook_event_name":"SessionEnd","session_id":"s-gone","reason":"prompt_input_exit"}"#,
        now - 60_000,
        true,
    );
    let next = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("board after hook event");
    assert_eq!(next.columns.to_resume.len(), 1);
    assert_eq!(next.columns.to_resume[0].last_prompt.as_deref(), Some("x"));

    handle.sender.send(CollectorMsg::Shutdown).unwrap();
    handle.thread.join().unwrap();
}
