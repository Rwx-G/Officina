//! The two commands Claude Code runs: the hook (FR-3) and the quota writer
//! (FR-5). Both are silent and never fail the caller.

use std::fs;

use officina_core::events::reduce_payload;
use officina_core::quota::{from_statusline, parse_quota_file};

use crate::files::{append_line, atomic_write, event_file_for};
use crate::paths::Paths;

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

/// Reduces one hook payload and appends it to today's event file.
pub fn record_hook(paths: &Paths, stdin: &[u8], now_ms: i64, with_excerpts: bool) -> HookOutcome {
    let Ok(payload) = serde_json::from_slice::<serde_json::Value>(stdin) else {
        return HookOutcome::NotAHookPayload;
    };
    let Some(line) = reduce_payload(&payload, now_ms, with_excerpts) else {
        return HookOutcome::NotAHookPayload;
    };
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
                true
            ),
            HookOutcome::Appended
        );
        assert_eq!(
            record_hook(&p, b"not json", now, true),
            HookOutcome::NotAHookPayload
        );
        assert_eq!(
            record_hook(&p, b"{}", now, true),
            HookOutcome::NotAHookPayload
        );
        let (events, corrupt) = read_recent_events(&p.events_dir(), now);
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
