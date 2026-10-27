//! The two commands Claude Code runs: the hook (FR-3) and the quota writer
//! (FR-5). Both are silent and never fail the caller.

use std::fs;

use officina_core::events::reduce_payload;
use officina_core::floor::requests_no_excerpts;
use officina_core::quota::{from_statusline, parse_quota_file};

use crate::files::{append_line, atomic_write, event_file_for};
use crate::paths::{Paths, excerpts_disabled_by_env};

/// A quota file younger than this is not rewritten when its windows are
/// unchanged: the statusline runs on every render (AD-10).
pub const QUOTA_REWRITE_AFTER_S: i64 = 60;

/// What `record_hook` did, for tests and diagnostics. The hook command itself
/// ignores it and always exits 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookOutcome {
    Appended,
    NotAHookPayload,
    WriteFailed(String),
}

/// Whether the hook keeps prompt and assistant excerpts. Not when
/// `OFFICINA_NO_EXCERPTS` is set, nor when the floor in `paths` sets
/// `no_excerpts` (the hook resolves it through [`Paths::from_env`]:
/// `OFFICINA_FLOOR`, else `<officina home>/floor.toml`; a floor given only as
/// `--floor` to the window is not seen). A floor that cannot be read keeps
/// them: the hook never fails on its own configuration.
pub fn hook_keeps_excerpts(paths: &Paths) -> bool {
    if excerpts_disabled_by_env() {
        return false;
    }
    !paths
        .floor_file
        .as_deref()
        .and_then(|floor| fs::read_to_string(floor).ok())
        .is_some_and(|content| requests_no_excerpts(&content))
}

/// Reduces one hook payload and appends it to today's event file.
/// `claude_pid` is the hook's `CLAUDE_PID` (E14), recorded on the line.
pub fn record_hook(
    paths: &Paths,
    stdin: &[u8],
    now_ms: i64,
    with_excerpts: bool,
    claude_pid: Option<u32>,
) -> HookOutcome {
    let Ok(payload) = serde_json::from_slice::<serde_json::Value>(stdin) else {
        return HookOutcome::NotAHookPayload;
    };
    let Some(mut line) = reduce_payload(&payload, now_ms, with_excerpts) else {
        return HookOutcome::NotAHookPayload;
    };
    line.claude_pid = claude_pid;
    match append_line(
        &event_file_for(&paths.events_dir(), now_ms),
        &line.to_bounded_line(),
    ) {
        Ok(()) => HookOutcome::Appended,
        Err(e) => HookOutcome::WriteFailed(e.to_string()),
    }
}

/// What `record_quota` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaOutcome {
    Written,
    Unchanged,
    NoRateLimits,
    NotJson,
    WriteFailed(String),
}

/// Writes the account's quota file from a statusline payload.
pub fn record_quota(paths: &Paths, stdin: &[u8], now_s: i64) -> QuotaOutcome {
    let Ok(payload) = serde_json::from_slice::<serde_json::Value>(stdin) else {
        return QuotaOutcome::NotJson;
    };
    let Some(file) = from_statusline(&payload, &paths.account_key(), now_s) else {
        return QuotaOutcome::NoRateLimits;
    };
    let target = paths.quota_file();
    let previous = fs::read_to_string(&target)
        .ok()
        .and_then(|c| parse_quota_file(&c));
    if previous
        .as_ref()
        .is_some_and(|p| p.same_windows(&file) && now_s - p.written_at < QUOTA_REWRITE_AFTER_S)
    {
        return QuotaOutcome::Unchanged;
    }
    match serde_json::to_vec(&file)
        .map_err(|e| e.to_string())
        .and_then(|bytes| atomic_write(&target, &bytes).map_err(|e| e.to_string()))
    {
        Ok(()) => QuotaOutcome::Written,
        Err(e) => QuotaOutcome::WriteFailed(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::read_recent_events;
    use std::path::PathBuf;

    fn paths(home: &std::path::Path) -> Paths {
        Paths {
            officina_home: home.to_path_buf(),
            claude_config_dir: PathBuf::from("unused"),
            claude_config_override: None,
            floor_file: None,
        }
    }

    #[test]
    fn hook_appends_and_ignores_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        let now = 1_793_102_400_000;
        assert_eq!(
            record_hook(
                &p,
                br#"{"hook_event_name":"Stop","session_id":"s"}"#,
                now,
                true,
                Some(4242),
            ),
            HookOutcome::Appended
        );
        assert_eq!(
            record_hook(&p, b"not json", now, true, None),
            HookOutcome::NotAHookPayload
        );
        assert_eq!(
            record_hook(&p, b"{}", now, true, None),
            HookOutcome::NotAHookPayload
        );
        let (events, corrupt) = read_recent_events(&p.events_dir(), now);
        assert_eq!(events[0].claude_pid, Some(4242));
        assert_eq!((events.len(), corrupt), (1, 0));
    }

    #[test]
    fn quota_skips_unchanged_recent_writes() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        let payload = br#"{"rate_limits":{"five_hour":{"used_percentage":10,"resets_at":5}}}"#;
        assert_eq!(record_quota(&p, payload, 1000), QuotaOutcome::Written);
        assert_eq!(record_quota(&p, payload, 1030), QuotaOutcome::Unchanged);
        assert_eq!(record_quota(&p, payload, 1061), QuotaOutcome::Written);
        let changed = br#"{"rate_limits":{"five_hour":{"used_percentage":11,"resets_at":5}}}"#;
        assert_eq!(record_quota(&p, changed, 1062), QuotaOutcome::Written);
        assert_eq!(record_quota(&p, b"{}", 1063), QuotaOutcome::NoRateLimits);
        assert_eq!(record_quota(&p, b"x", 1063), QuotaOutcome::NotJson);
        assert!(p.quota_file().ends_with("default.json"));
    }
}
