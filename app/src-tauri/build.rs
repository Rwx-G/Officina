/// The window's own commands, declared so the capability file gates them
/// (AD-19): a command missing from `capabilities/default.json` cannot be
/// called by the page, whatever script runs in it.
const COMMANDS: &[&str] = &[
    "get_board",
    "get_relay",
    "refresh",
    "focus",
    "report_wrong_window",
    "copy_attach_command",
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
    "confirm_triage",
    "evaluate_triage_model",
    "set_triage_model",
    "set_auto_archive",
    "set_auto_approval",
    "set_auto_mode",
    "mark_should_not_have_run",
    "settle_session",
    "list_discarded",
    "issue_card",
    "edit_card",
    "set_work_order_permission",
    "issue_card_from_proposal",
    "dismiss_proposal",
    "launch_work_order",
    "get_orchestrator",
    "mark_briefing_read",
    "start_orchestrator",
    "open_meeting",
    "send_answer",
    "build_images",
    "run_battery",
    "open_review",
    "approve_sensitive",
    "import_package",
    "discard_package",
    "stop_workshop",
    "resume_workshop",
    "remove_run",
    "get_workshop",
    "prepare_login",
    "check_login",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run the Tauri build script");
    // tauri-build links its manifest into the binaries only; an example
    // without the Common Controls v6 dependency stops at start on Windows
    // (STATUS_ENTRYPOINT_NOT_FOUND).
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-examples=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-examples=/MANIFESTDEPENDENCY:type='win32' \
             name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
             processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
}
