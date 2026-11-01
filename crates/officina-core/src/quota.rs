//! The quota file contract (AD-8): what `officina quota write` extracts from a
//! statusline payload, and how the board reads it back.
//!
//! Rate limits belong to an account, not to a session, so there is one file per
//! Claude config dir and the last writer wins.

use serde::{Deserialize, Serialize};

/// Version of the quota file schema.
pub const QUOTA_SCHEMA: u32 = 1;

/// Quota data older than this is displayed as stale.
pub const STALE_AFTER_MS: i64 = 30 * 60 * 1000;

/// The statusline's name of the five-hour window, kept as the file's key.
pub const FIVE_HOUR: &str = "five_hour";

/// The statusline's name of the weekly window, kept as the file's key.
pub const SEVEN_DAY: &str = "seven_day";

/// Account key used when no config dir override applies.
pub const DEFAULT_ACCOUNT_KEY: &str = "default";

/// One rate-limit window as Claude Code reports it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct QuotaWindow {
    /// 0 to 100.
    pub used_percentage: f64,
    /// Unix seconds.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub resets_at: i64,
}

/// Content of `<officina home>/quota/<account key>.json`. A window the
/// statusline did not carry is absent from the file, never `null`: Claude
/// Code sends each window only while the account has it, and the window
/// shows a gauge only when the file carries it (D-1042).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuotaFile {
    pub schema: u32,
    pub account_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Unix seconds, like the statusline payload.
    pub written_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_hour: Option<QuotaWindow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seven_day: Option<QuotaWindow>,
}

impl QuotaFile {
    /// True when both windows carry the same values as `other`'s. Used by the
    /// writer to skip rewriting an unchanged file on every statusline tick.
    pub fn same_windows(&self, other: &QuotaFile) -> bool {
        self.five_hour == other.five_hour && self.seven_day == other.seven_day
    }

    /// The windows the file carries, by their statusline names, five-hour
    /// first, as doctor names them.
    pub fn window_names(&self) -> Vec<&'static str> {
        [
            (FIVE_HOUR, self.five_hour.is_some()),
            (SEVEN_DAY, self.seven_day.is_some()),
        ]
        .into_iter()
        .filter_map(|(name, present)| present.then_some(name))
        .collect()
    }
}

fn window(payload: &serde_json::Value, name: &str) -> Option<QuotaWindow> {
    let w = payload.get("rate_limits")?.get(name)?;
    let used = w.get("used_percentage")?.as_f64()?;
    let resets_at = w.get("resets_at")?.as_i64()?;
    Some(QuotaWindow {
        used_percentage: used.clamp(0.0, 100.0),
        resets_at,
    })
}

/// Builds the quota file from a statusline payload. Returns `None` when the
/// payload carries no rate limit at all (API-key accounts, or before the first
/// API response): writing an empty file would erase good data from another
/// session of the same account.
///
/// ```
/// let payload = serde_json::json!({
///     "session_id": "s1",
///     "rate_limits": {"five_hour": {"used_percentage": 42.5, "resets_at": 1800000000}},
/// });
/// let file = officina_core::quota::from_statusline(&payload, "default", 1_000).unwrap();
/// assert_eq!(file.five_hour.unwrap().used_percentage, 42.5);
/// assert!(file.seven_day.is_none());
/// ```
pub fn from_statusline(
    payload: &serde_json::Value,
    account_key: &str,
    now_s: i64,
) -> Option<QuotaFile> {
    let five_hour = window(payload, FIVE_HOUR);
    let seven_day = window(payload, SEVEN_DAY);
    if five_hour.is_none() && seven_day.is_none() {
        return None;
    }
    Some(QuotaFile {
        schema: QUOTA_SCHEMA,
        account_key: account_key.to_owned(),
        session_id: payload
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(str::to_owned),
        written_at: now_s,
        five_hour,
        seven_day,
    })
}

/// Parses a quota file, refusing other schema versions.
pub fn parse_quota_file(content: &str) -> Option<QuotaFile> {
    serde_json::from_str::<QuotaFile>(content)
        .ok()
        .filter(|f| f.schema == QUOTA_SCHEMA)
}

/// Derives the account key: [`DEFAULT_ACCOUNT_KEY`] when no config dir
/// override applies, else `cfg-` plus a hash of the normalised path.
///
/// FNV-1a is enough here: the key only has to be stable and collision-free
/// across the handful of config dirs on one workstation; it protects nothing.
/// The public contract is the `officina quota write` command, not this hash.
pub fn account_key(config_dir_override: Option<&str>) -> String {
    match config_dir_override {
        None => DEFAULT_ACCOUNT_KEY.to_owned(),
        Some(dir) => format!("cfg-{:016x}", fnv1a(&normalise_path(dir))),
    }
}

/// Lowercase, forward slashes, no trailing slash: two spellings of one Windows
/// path give the same result.
pub fn normalise_path(path: &str) -> String {
    path.trim_end_matches(['\\', '/'])
        .replace('\\', "/")
        .to_lowercase()
}

/// FNV-1a 64-bit hash.
pub fn fnv1a(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn both_windows_are_extracted_and_clamped() {
        let payload = json!({"rate_limits": {
            "five_hour": {"used_percentage": 120, "resets_at": 10},
            "seven_day": {"used_percentage": 41.0, "resets_at": 20}}});
        let file = from_statusline(&payload, "k", 7).unwrap();
        assert_eq!(file.five_hour.unwrap().used_percentage, 100.0);
        assert_eq!(file.seven_day.unwrap().resets_at, 20);
        assert_eq!(file.written_at, 7);
    }

    #[test]
    fn a_window_the_statusline_leaves_out_is_absent_from_the_file() {
        let payload = json!({"rate_limits": {
            "seven_day": {"used_percentage": 41.0, "resets_at": 20}}});
        let file = from_statusline(&payload, "k", 7).unwrap();
        assert_eq!(file.five_hour, None);
        assert_eq!(file.window_names(), [SEVEN_DAY]);
        let text = serde_json::to_string(&file).unwrap();
        assert!(!text.contains(FIVE_HOUR), "{text}");
        // An older file with both windows still reads both.
        let both = from_statusline(
            &json!({"rate_limits": {
                "five_hour": {"used_percentage": 1.0, "resets_at": 2},
                "seven_day": {"used_percentage": 3.0, "resets_at": 4}}}),
            "k",
            5,
        )
        .unwrap();
        let read = parse_quota_file(&serde_json::to_string(&both).unwrap()).unwrap();
        assert_eq!(read.window_names(), [FIVE_HOUR, SEVEN_DAY]);
    }

    #[test]
    fn payload_without_rate_limits_writes_nothing() {
        assert_eq!(from_statusline(&json!({"session_id": "s"}), "k", 1), None);
        assert_eq!(
            from_statusline(
                &json!({"rate_limits": {"five_hour": {"used_percentage": "x"}}}),
                "k",
                1
            ),
            None
        );
    }

    #[test]
    fn round_trip_and_schema_gate() {
        let file = from_statusline(
            &json!({"rate_limits": {"five_hour": {"used_percentage": 1.5, "resets_at": 2}}}),
            "k",
            3,
        )
        .unwrap();
        let text = serde_json::to_string(&file).unwrap();
        assert_eq!(parse_quota_file(&text), Some(file.clone()));
        assert_eq!(
            parse_quota_file(&text.replace("\"schema\":1", "\"schema\":2")),
            None
        );
        let mut later = file.clone();
        later.written_at = 99;
        assert!(file.same_windows(&later));
    }

    #[test]
    fn account_key_normalises_case_and_separators() {
        let a = account_key(Some(r"C:\Users\x\.claude"));
        assert_eq!(a, account_key(Some("c:/users/x/.claude/")));
        assert_ne!(a, account_key(Some(r"C:\Users\x\.claude-work")));
        assert!(a.starts_with("cfg-") && a.len() == 20);
        assert_eq!(account_key(None), DEFAULT_ACCOUNT_KEY);
    }
}
