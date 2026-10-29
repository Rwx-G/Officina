//! Platform calls: Win32 on Windows, the reference platform until v1, and
//! what the session binding reads of `/proc` on Linux (D-960).

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(windows)]
pub mod windows;
