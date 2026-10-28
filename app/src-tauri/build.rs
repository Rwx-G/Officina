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
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run the Tauri build script");
}
