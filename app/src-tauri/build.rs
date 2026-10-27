/// The window's own commands, declared so the capability file gates them
/// (AD-19): a command missing from `capabilities/default.json` cannot be
/// called by the page, whatever script runs in it.
const COMMANDS: &[&str] = &[
    "get_board",
    "refresh",
    "focus",
    "report_wrong_window",
    "copy_attach_command",
    "approve_catalogue",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run the Tauri build script");
}
