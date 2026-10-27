//! `officina doctor` (FR-11): reads, compares and reports. It never writes
//! under the Claude config dir; what the author must change is printed for
//! them to paste.

use std::fs;
use std::path::Path;

use officina_core::contract::check_agents;
use officina_core::quota::STALE_AFTER_MS;

use crate::claude;
use crate::collector::Collector;
use crate::files::{now_ms, read_recent_events};

/// Hook events Officina registers (framing principle 5).
pub const HOOK_EVENTS: [&str; 5] = [
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "Stop",
    "Notification",
];

/// Severity of one doctor line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    /// Works, but something is degraded or not yet observed.
    Warn,
    /// Officina cannot do its job until this is fixed.
    Fail,
}

/// One doctor finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub level: Level,
    pub title: String,
    pub detail: String,
    /// Text to paste, when the fix is a configuration change.
    pub paste: Option<String>,
}

fn check(level: Level, title: &str, detail: impl Into<String>) -> Check {
    Check {
        level,
        title: title.to_owned(),
        detail: detail.into(),
        paste: None,
    }
}

/// The command line a hook or statusline uses to reach this executable:
/// absolute path, forward slashes, quoted, which works under both `cmd` and
/// the bash Claude Code uses on Windows.
pub fn quoted_exe(exe: &Path) -> String {
    format!("\"{}\"", exe.to_string_lossy().replace('\\', "/"))
}

/// The hooks block to paste into user settings (AD-16).
pub fn hooks_block(exe: &Path) -> String {
    let command = format!("{} hook", quoted_exe(exe));
    let mut hooks = serde_json::Map::new();
    for event in HOOK_EVENTS {
        hooks.insert(
            event.to_owned(),
            serde_json::json!([{ "hooks": [{ "type": "command", "command": command, "timeout": 5 }] }]),
        );
    }
    serde_json::to_string_pretty(&serde_json::json!({ "hooks": hooks })).unwrap_or_default()
}

/// The statusline wiring to paste (AD-16): `--tee` keeps the existing line.
pub fn statusline_snippet(exe: &Path, existing_command: Option<&str>) -> String {
    let command = match existing_command {
        Some(existing) => format!("{} quota write --tee | {existing}", quoted_exe(exe)),
        None => format!("{} quota write", quoted_exe(exe)),
    };
    serde_json::to_string_pretty(&serde_json::json!({
        "statusLine": { "type": "command", "command": command }
    }))
    .unwrap_or_default()
}

/// Events whose hooks in `settings` do not call `officina ... hook`.
pub fn missing_hook_events(settings: &serde_json::Value) -> Vec<&'static str> {
    HOOK_EVENTS
        .into_iter()
        .filter(|event| {
            let commands = settings
                .get("hooks")
                .and_then(|h| h.get(event))
                .and_then(|m| m.as_array())
                .into_iter()
                .flatten()
                .filter_map(|m| m.get("hooks").and_then(|h| h.as_array()))
                .flatten()
                .filter_map(|h| h.get("command").and_then(|c| c.as_str()));
            !commands
                .map(str::to_lowercase)
                .any(|c| c.contains("officina") && c.trim_end().ends_with(" hook"))
        })
        .collect()
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// Runs every check. `exe` is the path of the running `officina` binary.
pub fn run(collector: &Collector, exe: &Path) -> Vec<Check> {
    let now = now_ms();
    let paths = &collector.paths;
    let config_dir = paths.claude_config_override.as_deref();
    let mut checks = Vec::new();

    match &collector.claude {
        None => checks.push(check(
            Level::Fail,
            "claude",
            claude::ClaudeError::NotFound.to_string(),
        )),
        Some(bin) => {
            match claude::version(bin, config_dir) {
                Ok(v) => checks.push(check(
                    Level::Ok,
                    "claude",
                    format!("{v} at {}", bin.display()),
                )),
                Err(e) => checks.push(check(Level::Fail, "claude", e.to_string())),
            }
            match claude::agents_json(bin, config_dir) {
                Ok(out) => match serde_json::from_str::<Vec<serde_json::Value>>(out.trim()) {
                    Ok(entries) => {
                        let c = check_agents(&entries);
                        let level = if c.held() { Level::Ok } else { Level::Fail };
                        let mut detail = format!("{} sessions listed", entries.len());
                        if !c.missing.is_empty() {
                            detail.push_str(&format!("; missing fields: {}", c.missing.join(", ")));
                        }
                        if !c.not_observed.is_empty() {
                            detail.push_str(&format!(
                                "; not observed yet: {}",
                                c.not_observed.join(", ")
                            ));
                        }
                        checks.push(check(level, "claude agents --json", detail));
                    }
                    Err(e) => checks.push(check(
                        Level::Fail,
                        "claude agents --json",
                        format!("not a JSON array: {e}"),
                    )),
                },
                Err(e) => checks.push(check(Level::Fail, "claude agents --json", e.to_string())),
            }
        }
    }

    let user_settings = paths.claude_config_dir.join("settings.json");
    let settings = read_json(&user_settings).unwrap_or(serde_json::Value::Null);
    let missing = missing_hook_events(&settings);
    if missing.is_empty() {
        checks.push(check(
            Level::Ok,
            "hooks",
            format!("registered in {}", user_settings.display()),
        ));
    } else {
        checks.push(Check {
            level: Level::Fail,
            title: "hooks".into(),
            detail: format!(
                "not registered for {} in {}. Merge this block into that file (Officina never edits it):",
                missing.join(", "),
                user_settings.display()
            ),
            paste: Some(hooks_block(exe)),
        });
    }

    let (events, corrupt) = read_recent_events(&paths.events_dir(), now);
    if let Some(last) = events.iter().map(|e| e.ts).max() {
        let minutes = (now - last) / 60_000;
        checks.push(check(
            Level::Ok,
            "hook activity",
            format!("last event {minutes} min ago, {} in 24 h", events.len()),
        ));
    } else {
        checks.push(check(
            Level::Warn,
            "hook activity",
            "no hook event in the last 24 h: start or prompt a session after registering the hooks",
        ));
    }
    if corrupt > 0 {
        checks.push(check(
            Level::Warn,
            "event buffer",
            format!("{corrupt} unparsable lines in the last 24 h"),
        ));
    }

    match collector.read_quota() {
        Some(q) if now - q.written_at * 1000 <= STALE_AFTER_MS => checks.push(check(
            Level::Ok,
            "quota",
            format!(
                "account {} updated {} min ago",
                q.account_key,
                (now / 1000 - q.written_at) / 60
            ),
        )),
        found => {
            let existing = read_json(&user_settings).and_then(|s| {
                s.get("statusLine")?
                    .get("command")?
                    .as_str()
                    .map(str::to_owned)
            });
            checks.push(Check {
                level: Level::Warn,
                title: "quota".into(),
                detail: format!(
                    "{} for account {}. Wire one personal statusline (user settings or .claude/settings.local.json, never a committed file):",
                    if found.is_some() { "stale" } else { "no data" },
                    paths.account_key()
                ),
                paste: Some(statusline_snippet(exe, existing.as_deref())),
            });
        }
    }

    let probe = paths.officina_home.join(".doctor-probe");
    match fs::create_dir_all(&paths.officina_home).and_then(|()| fs::write(&probe, b"ok")) {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
            checks.push(check(
                Level::Ok,
                "officina home",
                paths.officina_home.display().to_string(),
            ));
        }
        Err(e) => checks.push(check(
            Level::Fail,
            "officina home",
            format!("{} is not writable: {e}", paths.officina_home.display()),
        )),
    }

    match (&collector.floor.path, &collector.floor.error) {
        (None, _) => checks.push(check(
            Level::Ok,
            "floor",
            "none: projects are derived from each session's folder",
        )),
        (Some(p), None) => checks.push(check(
            Level::Ok,
            "floor",
            format!(
                "{} ({} projects)",
                p.display(),
                collector.floor.projects.len()
            ),
        )),
        (Some(_), Some(e)) => checks.push(check(Level::Fail, "floor", e.clone())),
    }

    checks
}

/// Exit code of `officina doctor`: 1 when any check failed.
pub fn exit_code(checks: &[Check]) -> i32 {
    i32::from(checks.iter().any(|c| c.level == Level::Fail))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn hooks_block_registers_every_event_and_is_recognised() {
        let exe = PathBuf::from(r"C:\Program Files\Officina\officina.exe");
        let block: serde_json::Value = serde_json::from_str(&hooks_block(&exe)).unwrap();
        assert!(missing_hook_events(&block).is_empty());
        let cmd = block["hooks"]["Stop"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert_eq!(cmd, "\"C:/Program Files/Officina/officina.exe\" hook");
    }

    #[test]
    fn partially_registered_hooks_are_reported() {
        let settings = serde_json::json!({"hooks": {
            "Stop": [{"hooks": [{"type": "command", "command": "python x.py"},
                                 {"type": "command", "command": "officina.exe hook"}]}],
            "SessionStart": [{"hooks": [{"type": "command", "command": "officina.exe doctor"}]}]
        }});
        assert_eq!(
            missing_hook_events(&settings),
            vec![
                "SessionStart",
                "SessionEnd",
                "UserPromptSubmit",
                "Notification"
            ]
        );
        assert_eq!(
            missing_hook_events(&serde_json::Value::Null).len(),
            HOOK_EVENTS.len()
        );
    }

    #[test]
    fn statusline_snippet_tees_into_the_existing_command() {
        let exe = PathBuf::from("/opt/officina");
        let with = statusline_snippet(&exe, Some("python statusline.py"));
        assert!(with.contains("\\\"/opt/officina\\\" quota write --tee | python statusline.py"));
        assert!(statusline_snippet(&exe, None).contains("quota write\""));
    }
}
