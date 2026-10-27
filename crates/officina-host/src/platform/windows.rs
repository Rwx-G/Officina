//! Win32 calls for the focus action (AD-15). The only module of the workspace
//! allowed `unsafe`; every block carries its safety argument. Decisions (which
//! window, which arguments) are made by pure functions in `officina_core::focus`.

#![allow(unsafe_code)] // raw Win32 bindings; each block states its invariant

use std::collections::HashMap;

use windows_sys::Win32::Foundation::{CloseHandle, HWND, INVALID_HANDLE_VALUE, LPARAM};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FLASHW_ALL, FLASHW_TIMERNOFG, FLASHWINFO, FlashWindowEx, GW_OWNER,
    GetForegroundWindow, GetWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    IsIconic, IsWindowVisible, SW_RESTORE, SetForegroundWindow, ShowWindow,
};

/// Parent pid of every running process.
fn parent_map() -> HashMap<u32, u32> {
    let mut parents = HashMap::new();
    // SAFETY: plain call; the returned handle is checked and closed below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
        return parents;
    }
    // SAFETY: PROCESSENTRY32W is plain data; zeroed is a valid bit pattern,
    // and dwSize is set as the API requires before the first call.
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = u32::try_from(std::mem::size_of::<PROCESSENTRY32W>()).unwrap_or(0);
    // SAFETY: `snapshot` is a valid snapshot handle and `entry` a properly
    // sized, writable PROCESSENTRY32W for the duration of each call.
    let mut ok = unsafe { Process32FirstW(snapshot, &mut entry) };
    while ok != 0 {
        parents.insert(entry.th32ProcessID, entry.th32ParentProcessID);
        // SAFETY: as above.
        ok = unsafe { Process32NextW(snapshot, &mut entry) };
    }
    // SAFETY: the handle came from CreateToolhelp32Snapshot and is closed once.
    unsafe { CloseHandle(snapshot) };
    parents
}

/// `pid` and its ancestors, nearest first, at most 16 hops, cycle-safe.
fn ancestors(pid: u32) -> Vec<u32> {
    let parents = parent_map();
    let mut chain = vec![pid];
    let mut current = pid;
    for _ in 0..16 {
        match parents.get(&current) {
            Some(&parent) if parent != 0 && !chain.contains(&parent) => {
                chain.push(parent);
                current = parent;
            }
            _ => break,
        }
    }
    chain
}

/// A visible, unowned, titled top-level window.
struct TopWindow {
    hwnd: HWND,
    pid: u32,
    title: String,
}

unsafe extern "system" fn collect_window(hwnd: HWND, lparam: LPARAM) -> i32 {
    // SAFETY: `lparam` is the address of the Vec passed by `top_windows`,
    // which outlives the synchronous EnumWindows call.
    let out = unsafe { &mut *(lparam as *mut Vec<TopWindow>) };
    // SAFETY: `hwnd` is a window handle supplied by EnumWindows.
    let visible = unsafe { IsWindowVisible(hwnd) } != 0;
    // SAFETY: as above.
    let owned = !unsafe { GetWindow(hwnd, GW_OWNER) }.is_null();
    if !visible || owned {
        return 1;
    }
    // SAFETY: as above.
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return 1;
    }
    let mut buf = vec![0u16; usize::try_from(len).unwrap_or(0) + 1];
    // SAFETY: `buf` holds len + 1 u16, the size passed to the call.
    let copied = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), len + 1) };
    let title = String::from_utf16_lossy(&buf[..usize::try_from(copied).unwrap_or(0)]);
    let mut pid = 0u32;
    // SAFETY: `pid` is a valid out pointer for the call.
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    out.push(TopWindow { hwnd, pid, title });
    1
}

fn top_windows() -> Vec<TopWindow> {
    let mut windows: Vec<TopWindow> = Vec::new();
    // SAFETY: the callback only dereferences `lparam` as the Vec above, which
    // lives until EnumWindows returns; EnumWindows does not retain it.
    unsafe {
        EnumWindows(
            Some(collect_window),
            &mut windows as *mut Vec<TopWindow> as LPARAM,
        )
    };
    windows
}

/// Titles of the windows owned by the nearest ancestor of `pid` that owns any.
/// Returns the handles alongside, in the same order.
fn session_windows(pid: u32) -> Vec<(HWND, String)> {
    let windows = top_windows();
    for ancestor in ancestors(pid) {
        let owned: Vec<(HWND, String)> = windows
            .iter()
            .filter(|w| w.pid == ancestor)
            .map(|w| (w.hwnd, w.title.clone()))
            .collect();
        if !owned.is_empty() {
            return owned;
        }
    }
    Vec::new()
}

/// Titles of the windows a focus on `pid` would choose from. Exposed for the
/// manual checklist and for diagnostics.
pub fn window_titles_for(pid: u32) -> Vec<String> {
    session_windows(pid).into_iter().map(|(_, t)| t).collect()
}

/// Brings the window of an interactive session to the foreground.
pub fn focus_session_window(pid: u32, names: &[String]) -> officina_core::focus::FocusOutcome {
    use officina_core::focus::{FocusOutcome, WindowChoice, choose_window};
    let windows = session_windows(pid);
    let titles: Vec<String> = windows.iter().map(|(_, t)| t.clone()).collect();
    match choose_window(&titles, names) {
        WindowChoice::None => FocusOutcome::NoWindow,
        WindowChoice::Candidates(list) => FocusOutcome::Candidates(list),
        WindowChoice::One(index) => {
            let hwnd = windows[index].0;
            // SAFETY: `hwnd` came from EnumWindows moments ago; a window
            // closed since makes these calls fail harmlessly.
            unsafe {
                if IsIconic(hwnd) != 0 {
                    ShowWindow(hwnd, SW_RESTORE);
                }
                if SetForegroundWindow(hwnd) != 0 || GetForegroundWindow() == hwnd {
                    return FocusOutcome::Focused;
                }
                let flash = FLASHWINFO {
                    cbSize: u32::try_from(std::mem::size_of::<FLASHWINFO>()).unwrap_or(0),
                    hwnd,
                    dwFlags: FLASHW_ALL | FLASHW_TIMERNOFG,
                    uCount: 3,
                    dwTimeout: 0,
                };
                FlashWindowEx(&flash);
            }
            FocusOutcome::Refused
        }
    }
}

/// Spawns `claude attach <id>` in a new console (FR-10). The child is not
/// tracked: Officina owns no session process.
pub fn open_attach_console(
    claude: &std::path::Path,
    id: &str,
    config_dir: Option<&std::path::Path>,
) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    let mut cmd = std::process::Command::new(claude);
    cmd.arg("attach").arg(id).creation_flags(CREATE_NEW_CONSOLE);
    // `attach` needs no working directory. Running it from the session's
    // folder would let `cmd.exe` (behind a `claude.cmd` shim) pick up a
    // planted `node.cmd` there; the binary's own folder is not the session's
    // to write, and the variable stops `cmd.exe` searching the current
    // directory at all.
    if let Some(dir) = claude.parent().filter(|d| !d.as_os_str().is_empty()) {
        cmd.current_dir(dir);
    }
    cmd.env("NoDefaultCurrentDirectoryInExePath", "1");
    if let Some(dir) = config_dir {
        cmd.env(crate::paths::ENV_CLAUDE_CONFIG_DIR, dir);
    }
    cmd.spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_process_has_ancestors() {
        let chain = ancestors(std::process::id());
        assert_eq!(chain[0], std::process::id());
        assert!(chain.len() >= 2, "the test runner has a parent");
    }

    #[test]
    fn window_enumeration_does_not_crash_and_titles_are_non_empty() {
        assert!(top_windows().iter().all(|w| !w.title.is_empty()));
    }
}
