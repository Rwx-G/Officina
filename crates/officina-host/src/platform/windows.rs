//! Win32 calls for the focus action (AD-15), for the relay's process tree
//! (AD-30, AD-31), for the credentials of HTTP actions (AD-36), for the
//! registry's location check (AD-39) and for the orchestrator's liveness and
//! console (AD-89). The only
//! module of the workspace allowed `unsafe`; every
//! block carries its safety argument. Decisions (which window, which
//! arguments) are made by pure functions in `officina_core`.

#![allow(unsafe_code)] // raw Win32 bindings; each block states its invariant

use std::collections::HashMap;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::path::PathBuf;

use windows_sys::Win32::Foundation::{
    CloseHandle, FILETIME, HANDLE, HWND, INVALID_HANDLE_VALUE, LPARAM,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};

// windows-sys declares CreateJobObjectW only with its `Win32_Security`
// feature, for the type of the security attributes pointer. Officina always
// passes null there, so the one function is declared here rather than
// enabling a feature the architecture addendum does not list.
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateJobObjectW(attributes: *const std::ffi::c_void, name: *const u16) -> HANDLE;
}
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows_sys::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, GetCurrentProcess, GetExitCodeProcess, GetProcessTimes,
    OpenProcess, PROCESS_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, STARTUPINFOW,
};

/// A job object: every process assigned to it, and every process those
/// start afterwards, ends together when the job is terminated.
#[derive(Debug)]
pub struct Job(HANDLE);

impl Job {
    /// A new, unnamed job. With `kill_on_close`, closing its last handle
    /// ends every process in it: when the process holding the handle dies,
    /// killed or not, its tree dies too (AD-31).
    pub fn new(kill_on_close: bool) -> io::Result<Job> {
        // SAFETY: both arguments may be null (default security, no name);
        // the returned handle is checked and owned by the Job, which closes
        // it once on drop.
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = Job(handle);
        if kill_on_close {
            // SAFETY: the struct is plain data, for which all zeroes is a
            // valid value (no limits set).
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let size = u32::try_from(std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
                .unwrap_or(u32::MAX);
            // SAFETY: `job.0` is a valid job handle, and the pointer and size
            // describe `limits`, which lives for the call.
            let ok = unsafe {
                SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    std::ptr::from_ref(&limits).cast(),
                    size,
                )
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(job)
    }

    /// Puts this process in the job, so every child it starts afterwards is
    /// in it from its first instruction: no race between a spawn and its
    /// assignment. A process already in a job nests (Windows 8 and later).
    pub fn assign_current_process(&self) -> io::Result<()> {
        // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no
        // closing; `self.0` is a valid job handle.
        let ok = unsafe { AssignProcessToJobObject(self.0, GetCurrentProcess()) };
        if ok == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    /// Puts a started child in the job. Whatever the child started before
    /// this call is not in it.
    pub fn assign(&self, child: &std::process::Child) -> io::Result<()> {
        // SAFETY: the child's handle stays valid while `child` is borrowed;
        // `self.0` is a valid job handle.
        let ok = unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle()) };
        if ok == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    /// Ends every process in the job with `exit_code`, the caller included
    /// when it is in the job.
    pub fn terminate(&self, exit_code: u32) -> io::Result<()> {
        // SAFETY: `self.0` is a valid job handle.
        let ok = unsafe { TerminateJobObject(self.0, exit_code) };
        if ok == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: the handle came from CreateJobObjectW and is closed once.
        unsafe { CloseHandle(self.0) };
    }
}

/// The Windows system directory (`C:\Windows\System32`), from the system
/// itself: never `PATH` or `%SystemRoot%`, which a session can set (AD-30).
pub fn system_directory() -> io::Result<PathBuf> {
    let mut buf = vec![0u16; 260];
    loop {
        let capacity = u32::try_from(buf.len()).unwrap_or(u32::MAX);
        // SAFETY: `buf` holds `capacity` u16, the size passed to the call,
        // which writes at most that many.
        let len = unsafe { GetSystemDirectoryW(buf.as_mut_ptr(), capacity) };
        let len = usize::try_from(len).unwrap_or(0);
        if len == 0 {
            return Err(io::Error::last_os_error());
        }
        if len < buf.len() {
            buf.truncate(len);
            return Ok(PathBuf::from(String::from_utf16_lossy(&buf)));
        }
        // Too small: `len` is the size needed, terminator included.
        buf.resize(len, 0);
    }
}

/// True when the drive whose root is `root` (`C:\`) is a network drive:
/// `GetDriveTypeW` answers `DRIVE_REMOTE` (AD-39). The registry is never
/// opened there.
pub fn is_remote_drive(root: &str) -> bool {
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
    // windows-sys declares DRIVE_REMOTE under its `Win32_System_
    // WindowsProgramming` feature, which the addendum does not list; the
    // value is part of GetDriveTypeW's documented contract.
    const DRIVE_REMOTE: u32 = 4;
    let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 string that outlives the
    // call, which only reads it.
    unsafe { GetDriveTypeW(wide.as_ptr()) == DRIVE_REMOTE }
}

// windows-sys declares OpenProcessToken only with its `Win32_Security`
// feature, for the type of the access mask; the one function is declared
// here, as CreateJobObjectW is above.
#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenProcessToken(process: HANDLE, access: u32, token: *mut HANDLE) -> i32;
}

/// The environment Windows builds from the registry for this process's
/// user, as Explorer gives a new process (`CreateEnvironmentBlock`, not
/// inheriting): the machine and user variables, `Path` the machine's then
/// the user's, every value expanded. Nothing comes from this process's own
/// environment, which a session sets (D-173).
pub fn registry_environment() -> io::Result<Vec<(String, String)>> {
    use windows_sys::Win32::System::Environment::{
        CreateEnvironmentBlock, DestroyEnvironmentBlock,
    };
    const TOKEN_QUERY: u32 = 0x0008;
    const TOKEN_DUPLICATE: u32 = 0x0002;
    const TOKEN_IMPERSONATE: u32 = 0x0004;
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no
    // closing; `token` is a valid out pointer, set only on success.
    let opened = unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY | TOKEN_DUPLICATE | TOKEN_IMPERSONATE,
            &mut token,
        )
    };
    if opened == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut block: *mut std::ffi::c_void = std::ptr::null_mut();
    // SAFETY: `token` is a valid token handle; `block` is a valid out
    // pointer, set only on success; FALSE asks for no inherited variable.
    let created = unsafe { CreateEnvironmentBlock(&mut block, token, 0) };
    let error = io::Error::last_os_error();
    // SAFETY: the handle came from OpenProcessToken and is closed once.
    unsafe { CloseHandle(token) };
    if created == 0 {
        return Err(error);
    }
    let mut vars = Vec::new();
    let mut entry = block.cast::<u16>().cast_const();
    loop {
        // SAFETY: the block is a run of NUL-terminated UTF-16 strings ended
        // by an empty one; reading stops at each terminator, so every read
        // lies inside it.
        let len = (0..).take_while(|i| unsafe { *entry.add(*i) } != 0).count();
        if len == 0 {
            break;
        }
        // SAFETY: `len` units before the terminator were just read.
        let text = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(entry, len) });
        // Entries such as `=C:=C:\` are the per-drive folders, not variables.
        if let Some((name, value)) = text.split_once('=').filter(|(name, _)| !name.is_empty()) {
            vars.push((name.to_owned(), value.to_owned()));
        }
        // SAFETY: the next entry starts after this one's terminator.
        entry = unsafe { entry.add(len + 1) };
    }
    // SAFETY: the block came from CreateEnvironmentBlock and is freed once;
    // nothing reads it afterwards.
    unsafe { DestroyEnvironmentBlock(block) };
    Ok(vars)
}

/// A registry hive the relay reads (D-173).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hive {
    /// `HKEY_LOCAL_MACHINE`: only an administrator writes it.
    LocalMachine,
    /// `HKEY_CURRENT_USER`: the user's, so a session's too.
    CurrentUser,
}

/// The `REG_SZ` value `value` of `key` under `hive`, from the registry's
/// 64-bit view; `None` when the key or value is missing, or of another type.
pub fn registry_string(hive: Hive, key: &str, value: &str) -> Option<String> {
    use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW,
    };
    let root = match hive {
        Hive::LocalMachine => HKEY_LOCAL_MACHINE,
        Hive::CurrentUser => HKEY_CURRENT_USER,
    };
    let key: Vec<u16> = key.encode_utf16().chain(std::iter::once(0)).collect();
    let value: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let mut buf = vec![0u16; 260];
    loop {
        let mut bytes = u32::try_from(buf.len() * 2).unwrap_or(u32::MAX);
        // SAFETY: `key` and `value` are NUL-terminated UTF-16 strings that
        // outlive the call; `buf` holds `bytes` bytes, the size passed, and
        // the call writes at most that many; the type pointer may be null.
        let status = unsafe {
            RegGetValueW(
                root,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        match status {
            ERROR_SUCCESS => {
                let units = usize::try_from(bytes).unwrap_or(0) / 2;
                buf.truncate(units.min(buf.len()));
                let end = buf.iter().position(|unit| *unit == 0).unwrap_or(buf.len());
                return Some(String::from_utf16_lossy(&buf[..end]));
            }
            // Too small: `bytes` is the size needed, which a value changed
            // meanwhile can still outgrow, so the loop asks again.
            ERROR_MORE_DATA => {
                let needed = usize::try_from(bytes).unwrap_or(0) / 2 + 1;
                if needed <= buf.len() {
                    return None;
                }
                buf.resize(needed, 0);
            }
            _ => return None,
        }
    }
}

/// Windows' proxy configuration for the current user, as
/// `WinHttpGetIEProxyConfigForCurrentUser` returns it (D-611).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WinHttpIeConfig {
    pub auto_detect: bool,
    pub pac_url: Option<String>,
    pub proxy: Option<String>,
    pub bypass: Option<String>,
}

/// A WinHTTP string the caller owns: copied out, then released with
/// `GlobalFree`, as the API documents for these fields.
fn take_global_string(text: windows_sys::core::PWSTR) -> Option<String> {
    if text.is_null() {
        return None;
    }
    // SAFETY: `text` is a non-null, NUL-terminated UTF-16 string WinHTTP
    // allocated for the caller; it is read up to its terminator, then freed
    // exactly once with `GlobalFree` and never used again.
    unsafe {
        let mut len = 0usize;
        while *text.add(len) != 0 {
            len += 1;
        }
        let value = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
        windows_sys::Win32::Foundation::GlobalFree(text.cast());
        Some(value).filter(|v| !v.is_empty())
    }
}

/// Reads the current user's proxy configuration with
/// `WinHttpGetIEProxyConfigForCurrentUser` (D-611); read-only, its strings
/// freed on every path.
pub fn winhttp_ie_config() -> io::Result<WinHttpIeConfig> {
    use windows_sys::Win32::Networking::WinHttp::{
        WINHTTP_CURRENT_USER_IE_PROXY_CONFIG, WinHttpGetIEProxyConfigForCurrentUser,
    };
    let mut config = WINHTTP_CURRENT_USER_IE_PROXY_CONFIG::default();
    // SAFETY: `config` is a valid, zeroed out structure the call fills;
    // every string it sets is taken and freed below, once.
    let ok = unsafe { WinHttpGetIEProxyConfigForCurrentUser(&mut config) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(WinHttpIeConfig {
        auto_detect: config.fAutoDetect != 0,
        pac_url: take_global_string(config.lpszAutoConfigUrl),
        proxy: take_global_string(config.lpszProxy),
        bypass: take_global_string(config.lpszProxyBypass),
    })
}

/// What `WinHttpGetProxyForUrl` gave for one URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WinHttpProxyAnswer {
    /// `WINHTTP_ACCESS_TYPE_NO_PROXY`: direct.
    Direct,
    /// `WINHTTP_ACCESS_TYPE_NAMED_PROXY` with its proxy list, as WinHTTP
    /// writes it.
    Named(String),
    /// Automatic detection found no PAC file
    /// (`ERROR_WINHTTP_AUTODETECTION_FAILED`).
    NoPac,
    /// The PAC file could not be fetched or evaluated, with the error.
    Failed(u32),
}

/// A WinHTTP session handle, closed when dropped, on every path.
struct WinHttpSession(*mut core::ffi::c_void);

impl Drop for WinHttpSession {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the handle came from `WinHttpOpen`, is non-null and is
            // closed exactly once, here.
            unsafe {
                windows_sys::Win32::Networking::WinHttp::WinHttpCloseHandle(self.0);
            }
        }
    }
}

/// Asks WinHTTP for the proxy of `url` under the PAC file `pac_url` or
/// automatic detection, as Windows itself evaluates it (D-611): a session
/// without a proxy of its own, each step bounded by `timeout_ms`, the
/// answer's strings freed and the session closed on every path.
pub fn winhttp_proxy_for_url(
    url: &str,
    auto_detect: bool,
    pac_url: Option<&str>,
    timeout_ms: i32,
) -> WinHttpProxyAnswer {
    use windows_sys::Win32::Networking::WinHttp::{
        ERROR_WINHTTP_AUTODETECTION_FAILED, WINHTTP_ACCESS_TYPE_NAMED_PROXY,
        WINHTTP_ACCESS_TYPE_NO_PROXY, WINHTTP_AUTO_DETECT_TYPE_DHCP,
        WINHTTP_AUTO_DETECT_TYPE_DNS_A, WINHTTP_AUTOPROXY_AUTO_DETECT,
        WINHTTP_AUTOPROXY_CONFIG_URL, WINHTTP_AUTOPROXY_OPTIONS, WINHTTP_PROXY_INFO,
        WinHttpGetProxyForUrl, WinHttpOpen, WinHttpSetTimeouts,
    };
    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(std::iter::once(0)).collect() };
    let agent = wide("Officina");
    // SAFETY: `agent` is a NUL-terminated UTF-16 string that outlives the
    // call; the proxy and bypass names may be null for this access type.
    let session = WinHttpSession(unsafe {
        WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_NO_PROXY,
            std::ptr::null(),
            std::ptr::null(),
            0,
        )
    });
    if session.0.is_null() {
        return WinHttpProxyAnswer::Failed(
            io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32
        );
    }
    // SAFETY: `session.0` is the open session handle; the call only sets
    // its timeouts.
    unsafe {
        WinHttpSetTimeouts(session.0, timeout_ms, timeout_ms, timeout_ms, timeout_ms);
    }
    let url = wide(url);
    let pac = pac_url.map(wide);
    let mut options = WINHTTP_AUTOPROXY_OPTIONS::default();
    match &pac {
        Some(pac) => {
            options.dwFlags = WINHTTP_AUTOPROXY_CONFIG_URL;
            options.lpszAutoConfigUrl = pac.as_ptr();
        }
        None if auto_detect => {
            options.dwFlags = WINHTTP_AUTOPROXY_AUTO_DETECT;
            options.dwAutoDetectFlags =
                WINHTTP_AUTO_DETECT_TYPE_DHCP | WINHTTP_AUTO_DETECT_TYPE_DNS_A;
        }
        None => return WinHttpProxyAnswer::NoPac,
    }
    options.fAutoLogonIfChallenged = 1;
    let mut info = WINHTTP_PROXY_INFO::default();
    // SAFETY: the session is open; `url` and the PAC URL `options` points at
    // are NUL-terminated UTF-16 strings that outlive the call; `info` is a
    // zeroed structure whose strings, when set, are taken and freed below.
    let ok = unsafe { WinHttpGetProxyForUrl(session.0, url.as_ptr(), &mut options, &mut info) };
    if ok == 0 {
        let code = io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32;
        return if code == ERROR_WINHTTP_AUTODETECTION_FAILED {
            WinHttpProxyAnswer::NoPac
        } else {
            WinHttpProxyAnswer::Failed(code)
        };
    }
    let proxy = take_global_string(info.lpszProxy);
    let _ = take_global_string(info.lpszProxyBypass);
    match (info.dwAccessType, proxy) {
        (WINHTTP_ACCESS_TYPE_NAMED_PROXY, Some(list)) => WinHttpProxyAnswer::Named(list),
        _ => WinHttpProxyAnswer::Direct,
    }
}

/// The blob of the generic credential `target` in the Windows Credential
/// Manager (`CredReadW`, `CRED_TYPE_GENERIC`), copied out and freed with
/// `CredFree`; `None` when no such credential exists (AD-36). The bytes are
/// the secret: the caller decodes them and never prints them.
pub fn read_generic_credential(target: &str) -> io::Result<Option<Vec<u8>>> {
    use windows_sys::Win32::Foundation::ERROR_NOT_FOUND;
    use windows_sys::Win32::Security::Credentials::{
        CRED_TYPE_GENERIC, CREDENTIALW, CredFree, CredReadW,
    };
    let wide: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
    let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: `wide` is a NUL-terminated UTF-16 string that outlives the
    // call; `credential` is a valid out pointer, set only on success.
    let ok = unsafe { CredReadW(wide.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) };
    if ok == 0 {
        let error = io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(code) if code == ERROR_NOT_FOUND as i32 => Ok(None),
            _ => Err(error),
        };
    }
    // SAFETY: on success `credential` points to a CREDENTIALW the system
    // allocated; its blob pointer is valid for `CredentialBlobSize` bytes
    // (or null with size 0). The bytes are copied before the free below.
    let blob = unsafe {
        let record = &*credential;
        let size = usize::try_from(record.CredentialBlobSize).unwrap_or(0);
        if record.CredentialBlob.is_null() || size == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(record.CredentialBlob, size).to_vec()
        }
    };
    // SAFETY: `credential` came from a successful CredReadW and is freed
    // once, here; nothing reads it afterwards.
    unsafe { CredFree(credential.cast()) };
    Ok(Some(blob))
}

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

/// Creation time of a process in 100 ns ticks, `None` when it cannot be read.
fn creation_time(pid: u32) -> Option<u64> {
    process_state(pid).map(|(created, _)| created)
}

/// What `GetExitCodeProcess` answers for a process that has not exited
/// (`STILL_ACTIVE`, 259); windows-sys declares it under a feature the
/// addendum does not list.
const STILL_ACTIVE: u32 = 259;

/// The creation time of process `pid` in 100 ns ticks, and whether it has
/// not exited yet; `None` when it cannot be opened.
fn process_state(pid: u32) -> Option<(u64, bool)> {
    // SAFETY: plain call; the handle is checked and closed below.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
    // SAFETY: `handle` is a valid process handle with query rights and the
    // four out pointers are writable FILETIMEs for the duration of the call.
    let timed =
        unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) };
    let mut code = 0u32;
    // SAFETY: `handle` is a valid process handle with query rights and
    // `code` is a writable u32 for the call.
    let coded = unsafe { GetExitCodeProcess(handle, &mut code) };
    // SAFETY: the handle came from OpenProcess and is closed once.
    unsafe { CloseHandle(handle) };
    (timed != 0).then(|| {
        (
            (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime),
            coded != 0 && code == STILL_ACTIVE,
        )
    })
}

/// True when process `pid` still runs and was created at
/// `created_at_ms` (Unix milliseconds, as `orchestrator_start` stores it,
/// D-144): Windows reuses process ids, and a pid given to another process
/// since has a later creation time (AD-89).
pub fn process_alive(pid: u32, created_at_ms: i64) -> bool {
    process_state(pid).is_some_and(|(created, running)| {
        running && officina_core::dates::filetime_ms(created) == created_at_ms
    })
}

/// `pid` and its ancestors, nearest first, at most 16 hops, cycle-safe.
///
/// Windows reuses process ids: when a parent exits, its id can be given to an
/// unrelated process, which the snapshot then reports as the parent. A real
/// parent is always older than its child, so the walk stops at the first
/// "parent" created after the process it supposedly started (AD-15 step 1).
fn ancestors(pid: u32) -> Vec<u32> {
    let parents = parent_map();
    let mut chain = vec![pid];
    let mut current = pid;
    let mut current_created = creation_time(pid);
    for _ in 0..16 {
        let Some(&parent) = parents.get(&current) else {
            break;
        };
        if parent == 0 || chain.contains(&parent) {
            break;
        }
        let parent_created = creation_time(parent);
        if let (Some(child), Some(candidate)) = (current_created, parent_created)
            && candidate > child
        {
            break;
        }
        chain.push(parent);
        current = parent;
        current_created = parent_created;
    }
    chain
}

/// The start stamp of process `pid` in 100 ns ticks while it runs; `None`
/// once it has exited, or when it cannot be opened (D-960).
pub fn running_since(pid: u32) -> Option<u64> {
    process_state(pid).and_then(|(created, running)| running.then_some(created))
}

/// `pid` and its ancestors with their start stamps, nearest first, by
/// [`ancestors`]'s walk (D-960). Empty when `pid`'s own stamp cannot be
/// read, so the chain always starts with `pid`; an ancestor whose stamp
/// cannot be read is left out, since nothing could match it.
pub fn ancestry(pid: u32) -> Vec<(u32, u64)> {
    let chain = ancestors(pid);
    if creation_time(pid).is_none() {
        return Vec::new();
    }
    chain
        .into_iter()
        .filter_map(|process| creation_time(process).map(|started| (process, started)))
        .collect()
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

/// Restores `hwnd` if it is minimised and brings it to the foreground;
/// when Windows refuses the change, flashes it.
fn bring_to_front(hwnd: HWND) -> officina_core::focus::FocusOutcome {
    // SAFETY: `hwnd` came from EnumWindows moments ago; a window closed
    // since makes these calls fail harmlessly.
    unsafe {
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        if SetForegroundWindow(hwnd) != 0 || GetForegroundWindow() == hwnd {
            return officina_core::focus::FocusOutcome::Focused;
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
    officina_core::focus::FocusOutcome::Refused
}

/// Brings the window of an interactive session to the foreground.
pub fn focus_session_window(pid: u32, names: &[String]) -> officina_core::focus::FocusOutcome {
    use officina_core::focus::{FocusOutcome, WindowChoice, choose_window};
    use officina_core::untrusted::Untrusted;
    let windows = session_windows(pid);
    let titles: Vec<String> = windows.iter().map(|(_, t)| t.clone()).collect();
    match choose_window(&titles, names) {
        WindowChoice::None => FocusOutcome::NoWindow,
        WindowChoice::Candidates(list) => {
            FocusOutcome::Candidates(list.into_iter().map(Untrusted::new).collect())
        }
        WindowChoice::One(index) => bring_to_front(windows[index].0),
    }
}

/// The indexes of the titles that hold `name` as a whole word (AD-89):
/// `-`, `_` and `.` belong to a name, so a longer name holding it is no
/// match.
fn titled(titles: &[&str], name: &str) -> Vec<usize> {
    titles
        .iter()
        .enumerate()
        .filter(|(_, title)| officina_core::focus::title_contains_word(title, name))
        .map(|(index, _)| index)
        .collect()
}

/// Brings to the front the one top-level window whose title holds `name`
/// as a whole word, whoever owns it: `--name` titles the orchestrator's
/// console, or its Windows Terminal tab (AD-89). Several are candidates,
/// none is [`NoWindow`](officina_core::focus::FocusOutcome::NoWindow).
pub fn focus_window_by_title(name: &str) -> officina_core::focus::FocusOutcome {
    use officina_core::focus::FocusOutcome;
    use officina_core::untrusted::Untrusted;
    let windows = top_windows();
    let titles: Vec<&str> = windows.iter().map(|w| w.title.as_str()).collect();
    match titled(&titles, name).as_slice() {
        [] => FocusOutcome::NoWindow,
        [one] => bring_to_front(windows[*one].hwnd),
        several => FocusOutcome::Candidates(
            several
                .iter()
                .map(|index| Untrusted::new(windows[*index].title.clone()))
                .collect(),
        ),
    }
}

// windows-sys declares CreateProcessW only with its `Win32_Security`
// feature, for the type of the two security attributes pointers. Officina
// always passes null there, so the one function is declared here, as
// CreateJobObjectW is above.
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateProcessW(
        application: *const u16,
        command_line: *mut u16,
        process_attributes: *const std::ffi::c_void,
        thread_attributes: *const std::ffi::c_void,
        inherit_handles: i32,
        creation_flags: u32,
        environment: *const std::ffi::c_void,
        current_directory: *const u16,
        startup_info: *const STARTUPINFOW,
        process_information: *mut PROCESS_INFORMATION,
    ) -> i32;
}

/// `text` as a NUL-terminated UTF-16 string.
fn wide(text: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    text.encode_wide().chain(std::iter::once(0)).collect()
}

/// Appends `arg` to `line` quoted as the Microsoft C runtime parses a
/// command line back into arguments, so each element stays exactly one
/// argument (E13): backslashes are doubled only before a quote.
fn push_quoted(line: &mut String, arg: &str) {
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '\x0b', '"']) {
        line.push_str(arg);
        return;
    }
    line.push('"');
    let mut backslashes = 0usize;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                line.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                line.push('"');
                backslashes = 0;
            }
            other => {
                line.extend(std::iter::repeat_n('\\', backslashes));
                line.push(other);
                backslashes = 0;
            }
        }
    }
    line.extend(std::iter::repeat_n('\\', backslashes * 2));
    line.push('"');
}

/// The command line of `program` with `args`: the program quoted whole (a
/// path holds no quote), then each argument by [`push_quoted`].
fn command_line(program: &std::path::Path, args: &[String]) -> String {
    let mut line = format!("\"{}\"", program.display());
    for arg in args {
        line.push(' ');
        push_quoted(&mut line, arg);
    }
    line
}

/// A Unicode environment block holding exactly `env`, sorted by name
/// ignoring case as Windows expects.
fn environment_block(env: &[(String, String)]) -> Vec<u16> {
    let mut sorted: Vec<&(String, String)> = env.iter().collect();
    sorted.sort_by_key(|(name, _)| name.to_uppercase());
    let mut block: Vec<u16> = Vec::new();
    for (name, value) in sorted {
        block.extend(format!("{name}={value}").encode_utf16());
        block.push(0);
    }
    block.push(0);
    block
}

/// Starts `program` with `args` in a console of its own (`flags` holds
/// `CREATE_NEW_CONSOLE`), in `cwd`, with exactly `env`, and leaves it to
/// the author: Officina owns no session process (FR-10). It inherits no
/// handle and is given no standard handles, so it reads and writes the
/// new console whatever the window's own standard handles are (review
/// F11); `std::process::Command` always passes the caller's. Returns its
/// pid and creation time.
pub fn start_in_new_console(
    program: &std::path::Path,
    args: &[String],
    cwd: Option<&std::path::Path>,
    env: &[(String, String)],
    flags: u32,
) -> io::Result<(u32, Option<u64>)> {
    let application = wide(program.as_os_str());
    let mut line: Vec<u16> = command_line(program, args)
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let block = environment_block(env);
    let directory = cwd.map(|dir| wide(dir.as_os_str()));
    let startup = STARTUPINFOW {
        cb: u32::try_from(std::mem::size_of::<STARTUPINFOW>()).unwrap_or(0),
        ..STARTUPINFOW::default()
    };
    let mut info = PROCESS_INFORMATION::default();
    // SAFETY: every pointer is to a NUL-terminated buffer, or the doubly
    // NUL-terminated environment block, that lives for the call; the
    // command line is writable, as CreateProcessW requires; both security
    // attribute pointers are null (defaults); `startup` is sized by its
    // `cb`, and `info` is a writable out structure.
    let ok = unsafe {
        CreateProcessW(
            application.as_ptr(),
            line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            flags | CREATE_UNICODE_ENVIRONMENT,
            block.as_ptr().cast(),
            directory.as_ref().map_or(std::ptr::null(), |d| d.as_ptr()),
            &startup,
            &mut info,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
    // SAFETY: `info.hProcess` is the new process's handle, with every
    // right; the four out pointers are writable FILETIMEs for the call.
    let timed = unsafe {
        GetProcessTimes(
            info.hProcess,
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    };
    // SAFETY: both handles came from CreateProcessW and are closed once;
    // closing them does not end the process.
    unsafe {
        CloseHandle(info.hThread);
        CloseHandle(info.hProcess);
    }
    let created = (timed != 0)
        .then(|| (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime));
    Ok((info.dwProcessId, created))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Story 6.25 AC 1: WinHTTP reads the current user's configuration
    /// (whatever it holds), and asks for a URL's proxy without a PAC file
    /// or detection only when one applies.
    #[test]
    fn reads_the_current_users_proxy_config() {
        let config = winhttp_ie_config().expect("WinHTTP answers");
        for text in [&config.pac_url, &config.proxy, &config.bypass]
            .into_iter()
            .flatten()
        {
            assert!(!text.is_empty() && !text.contains('\0'));
        }
        assert_eq!(
            winhttp_proxy_for_url("https://api.anthropic.com/", false, None, 1000),
            WinHttpProxyAnswer::NoPac
        );
        assert!(matches!(
            winhttp_proxy_for_url(
                "https://api.anthropic.com/",
                false,
                Some("http://127.0.0.1:9/officina-test.pac"),
                1000
            ),
            WinHttpProxyAnswer::Failed(_)
        ));
    }

    /// Story 6.3 AC 1: the HKLM read the engine and git resolutions use
    /// reads a value every Windows has.
    #[test]
    fn reads_a_known_hklm_string() {
        let name = registry_string(
            Hive::LocalMachine,
            "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion",
            "ProductName",
        )
        .expect("ProductName is set");
        assert!(name.starts_with("Windows"), "{name}");
        assert_eq!(
            registry_string(Hive::LocalMachine, "SOFTWARE\\Officina-Missing-Key", "X"),
            None
        );
    }

    #[test]
    fn own_process_has_ancestors() {
        let chain = ancestors(std::process::id());
        assert_eq!(chain[0], std::process::id());
        assert!(chain.len() >= 2, "the test runner has a parent");
        let times: Vec<u64> = chain.iter().filter_map(|p| creation_time(*p)).collect();
        assert!(
            times.windows(2).all(|w| w[1] <= w[0]),
            "every ancestor is older than its child"
        );
    }

    /// E13: each argument comes back whole from the C runtime's parse;
    /// `crates/officina/tests/spawn.rs` replays a console kind through a
    /// real child.
    #[test]
    fn quoting_keeps_each_argument_whole() {
        // A trailing backslash is doubled only where a closing quote
        // follows it; elsewhere backslashes are literal.
        let args: Vec<String> = [
            "plain",
            "",
            "two words",
            "say \"hi\"",
            "C:\\dir\\",
            "C:\\my dir\\",
            "a\tb\nc",
        ]
        .map(str::to_owned)
        .to_vec();
        let line = command_line(std::path::Path::new("C:\\Program Files\\claude.exe"), &args);
        assert_eq!(
            line,
            "\"C:\\Program Files\\claude.exe\" plain \"\" \"two words\" \"say \\\"hi\\\"\" C:\\dir\\ \"C:\\my dir\\\\\" \"a\tb\nc\""
        );
        let block = environment_block(&[("b".into(), "2".into()), ("A".into(), "1".into())]);
        assert_eq!(String::from_utf16_lossy(&block), "A=1\0b=2\0\0");
    }

    #[test]
    fn window_enumeration_does_not_crash_and_titles_are_non_empty() {
        assert!(top_windows().iter().all(|w| !w.title.is_empty()));
    }

    /// Story 5.16 AC 4 (AD-89): the orchestrator's console is found by its
    /// name as a whole word; a longer name holding it is another session.
    #[test]
    fn title_match_is_a_whole_word() {
        use officina_core::spawn::ORCHESTRATOR_NAME;
        let titles = [
            "officina-orchestrator",
            "Claude Code - officina-orchestrator",
            "OFFICINA-ORCHESTRATOR: idle",
            "my-officina-orchestrator",
            "officina-orchestrator-2",
            "officina orchestrator",
            "officina-orchestrator.log - Notepad",
        ];
        assert_eq!(titled(&titles, ORCHESTRATOR_NAME), [0, 1, 2]);
        assert!(titled(&[], ORCHESTRATOR_NAME).is_empty());
        // No window holds a name nobody uses: nothing is brought forward.
        assert_eq!(
            focus_window_by_title("officina-test-no-window-7f3a91"),
            officina_core::focus::FocusOutcome::NoWindow
        );
    }

    /// Story 5.16 AC 4 (AD-89, D-144): a pid is alive only with the
    /// creation time it was kept with, and only until it exits.
    #[test]
    fn process_alive_refuses_a_reused_pid() {
        let own = std::process::id();
        let created = officina_core::dates::filetime_ms(creation_time(own).unwrap());
        assert!(process_alive(own, created));
        assert!(!process_alive(own, created + 1), "another creation time");
        assert!(!process_alive(own, created - 1), "another creation time");
        #[expect(
            clippy::disallowed_methods,
            reason = "the test starts a short-lived process to watch it exit"
        )]
        let mut child = std::process::Command::new(system_directory().unwrap().join("ping.exe"))
            .args(["-n", "2", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        let born = officina_core::dates::filetime_ms(creation_time(pid).unwrap());
        assert!(process_alive(pid, born), "it runs");
        child.wait().unwrap();
        assert!(!process_alive(pid, born), "it exited");
        assert!(!process_alive(0, 0));
    }
}
