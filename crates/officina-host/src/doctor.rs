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

/// Events whose hooks in `settings` do not run `officina hook`: a command
/// whose first word is the `officina` binary and whose second is `hook`.
pub fn missing_hook_events(settings: &serde_json::Value) -> Vec<&'static str> {
    HOOK_EVENTS
        .into_iter()
        .filter(|event| {
            let mut commands = settings
                .get("hooks")
                .and_then(|h| h.get(event))
                .and_then(|m| m.as_array())
                .into_iter()
                .flatten()
                .filter_map(|m| m.get("hooks").and_then(|h| h.as_array()))
                .flatten()
                .filter_map(|h| h.get("command").and_then(|c| c.as_str()));
            !commands.any(runs_officina_hook)
        })
        .collect()
}

/// True when `command` starts the Officina hook: its first word, once
/// unquoted, is a path whose file name is `officina` or `officina.exe`
/// (any case, as on Windows), and its second word is exactly `hook`, the only
/// spelling the binary accepts. Later words are ignored, as the binary
/// ignores them.
fn runs_officina_hook(command: &str) -> bool {
    let Some(words) = command_words(command) else {
        return false;
    };
    let [program, subcommand, ..] = words.as_slice() else {
        return false;
    };
    let file_name = program.rsplit(['/', '\\']).next().unwrap_or_default();
    subcommand == "hook"
        && (file_name.eq_ignore_ascii_case("officina")
            || file_name.eq_ignore_ascii_case("officina.exe"))
}

/// Splits a command line into words as a POSIX shell does for plain words:
/// whitespace separates, single and double quotes group and are removed.
/// Backslashes are kept: in the commands doctor prints and Windows users
/// write, they are path separators, not escapes. `None` on an unterminated
/// quote, which no shell would run.
fn command_words(command: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut current: Option<String> = None;
    let mut quote: Option<char> = None;
    for c in command.chars() {
        match quote {
            Some(open) if c == open => quote = None,
            Some(_) => current.get_or_insert_default().push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                current.get_or_insert_default();
            }
            None if c.is_whitespace() => words.extend(current.take()),
            None => current.get_or_insert_default().push(c),
        }
    }
    if quote.is_some() {
        return None;
    }
    words.extend(current);
    Some(words)
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// Runs every check. `exe` is the path of the running `officina` binary.
pub fn run(collector: &Collector, exe: &Path) -> Vec<Check> {
    let now = now_ms();
    let paths = &collector.paths;
    let user_settings = paths.claude_config_dir.join("settings.json");
    let mut checks = check_claude(collector);
    checks.push(check_hooks(&user_settings, exe));
    checks.extend(check_hook_activity(&paths.events_dir(), now));
    checks.push(check_quota(collector, &user_settings, exe, now));
    checks.push(check_officina_home(&paths.officina_home));
    checks.push(check_floor(collector));
    checks
}

/// `claude --version`, then `claude agents --json` against the contract.
fn check_claude(collector: &Collector) -> Vec<Check> {
    let Some(bin) = &collector.claude else {
        return vec![check(
            Level::Fail,
            "claude",
            claude::ClaudeError::NotFound.to_string(),
        )];
    };
    let config_dir = collector.paths.claude_config_override.as_deref();
    let version = match claude::version(bin, config_dir) {
        Ok(v) => check(Level::Ok, "claude", format!("{v} at {}", bin.display())),
        Err(e) => check(Level::Fail, "claude", e.to_string()),
    };
    vec![version, check_agents_output(bin, config_dir)]
}

fn check_agents_output(bin: &Path, config_dir: Option<&Path>) -> Check {
    const TITLE: &str = "claude agents --json";
    let out = match claude::agents_json(bin, config_dir) {
        Ok(out) => out,
        Err(e) => return check(Level::Fail, TITLE, e.to_string()),
    };
    let entries = match serde_json::from_str::<Vec<serde_json::Value>>(out.trim()) {
        Ok(entries) => entries,
        Err(e) => return check(Level::Fail, TITLE, format!("not a JSON array: {e}")),
    };
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
    check(level, TITLE, detail)
}

/// Every hook event registered in the user settings, else the block to paste.
fn check_hooks(user_settings: &Path, exe: &Path) -> Check {
    let settings = read_json(user_settings).unwrap_or(serde_json::Value::Null);
    let missing = missing_hook_events(&settings);
    if missing.is_empty() {
        return check(
            Level::Ok,
            "hooks",
            format!("registered in {}", user_settings.display()),
        );
    }
    Check {
        level: Level::Fail,
        title: "hooks".into(),
        detail: format!(
            "not registered for {} in {}. Merge this block into that file (Officina never edits it):",
            missing.join(", "),
            user_settings.display()
        ),
        paste: Some(hooks_block(exe)),
    }
}

/// The last hook event of the day, plus unparsable lines when there are any.
fn check_hook_activity(events_dir: &Path, now: i64) -> Vec<Check> {
    let (events, corrupt) = read_recent_events(events_dir, now);
    let activity = match events.iter().map(|e| e.ts).max() {
        Some(last) => check(
            Level::Ok,
            "hook activity",
            format!(
                "last event {} min ago, {} in 24 h",
                (now - last) / 60_000,
                events.len()
            ),
        ),
        None => check(
            Level::Warn,
            "hook activity",
            "no hook event in the last 24 h: start or prompt a session after registering the hooks",
        ),
    };
    let mut checks = vec![activity];
    if corrupt > 0 {
        checks.push(check(
            Level::Warn,
            "event buffer",
            format!("{corrupt} unparsable lines in the last 24 h"),
        ));
    }
    checks
}

/// A fresh quota file for this floor's account, else the statusline wiring.
fn check_quota(collector: &Collector, user_settings: &Path, exe: &Path, now: i64) -> Check {
    let found = collector.read_quota();
    if let Some(q) = found
        .as_ref()
        .filter(|q| now - q.written_at * 1000 <= STALE_AFTER_MS)
    {
        return check(
            Level::Ok,
            "quota",
            format!(
                "account {} updated {} min ago",
                q.account_key,
                (now / 1000 - q.written_at) / 60
            ),
        );
    }
    let existing = read_json(user_settings).and_then(|s| {
        s.get("statusLine")?
            .get("command")?
            .as_str()
            .map(str::to_owned)
    });
    Check {
        level: Level::Warn,
        title: "quota".into(),
        detail: format!(
            "{} for account {}. Wire one personal statusline (user settings or .claude/settings.local.json, never a committed file):",
            if found.is_some() { "stale" } else { "no data" },
            collector.paths.account_key()
        ),
        paste: Some(statusline_snippet(exe, existing.as_deref())),
    }
}

/// The Officina home exists and takes a write.
fn check_officina_home(home: &Path) -> Check {
    let probe = home.join(".doctor-probe");
    match fs::create_dir_all(home).and_then(|()| fs::write(&probe, b"ok")) {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
            check(Level::Ok, "officina home", home.display().to_string())
        }
        Err(e) => check(
            Level::Fail,
            "officina home",
            format!("{} is not writable: {e}", home.display()),
        ),
    }
}

/// The floor file, if any, loaded without error.
fn check_floor(collector: &Collector) -> Check {
    match (&collector.floor.path, &collector.floor.error) {
        (None, _) => check(
            Level::Ok,
            "floor",
            "none: projects are derived from each session's folder",
        ),
        (Some(p), None) => check(
            Level::Ok,
            "floor",
            format!(
                "{} ({} projects)",
                p.display(),
                collector.floor.projects.len()
            ),
        ),
        (Some(_), Some(e)) => check(Level::Fail, "floor", e.clone()),
    }
}

/// Exit code of `officina doctor`: 1 when any check failed.
pub fn exit_code(checks: &[Check]) -> i32 {
    i32::from(checks.iter().any(|c| c.level == Level::Fail))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::LoadedFloor;
    use crate::files::{append_line, event_file_for};
    use crate::paths::Paths;
    use std::path::PathBuf;

    fn summary(checks: &[Check]) -> Vec<(&str, Level, bool)> {
        checks
            .iter()
            .map(|c| (c.title.as_str(), c.level, c.paste.is_some()))
            .collect()
    }

    #[test]
    fn run_reports_every_check_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            officina_home: dir.path().join("officina"),
            claude_config_dir: dir.path().join("config"),
            claude_config_override: None,
            floor_file: None,
        };
        let today = event_file_for(&paths.events_dir(), now_ms());
        let line = format!(
            "{{\"v\":1,\"ts\":{},\"event\":\"Stop\",\"session_id\":\"s\"}}\n",
            now_ms()
        );
        append_line(&today, &line).unwrap();
        append_line(&today, "garbage\n").unwrap();
        let exe = PathBuf::from("/opt/officina");

        let collector = Collector::from_parts(paths.clone(), LoadedFloor::default(), None);
        assert_eq!(
            summary(&run(&collector, &exe)),
            [
                ("claude", Level::Fail, false),
                ("hooks", Level::Fail, true),
                ("hook activity", Level::Ok, false),
                ("event buffer", Level::Warn, false),
                ("quota", Level::Warn, true),
                ("officina home", Level::Ok, false),
                ("floor", Level::Ok, false),
            ]
        );

        let broken_floor = LoadedFloor {
            path: Some(dir.path().join("officina.toml")),
            error: Some("invalid floor file".into()),
            ..Default::default()
        };
        let collector = Collector::from_parts(paths, broken_floor, None);
        let checks = run(&collector, &exe);
        let floor = checks.last().unwrap();
        assert_eq!((floor.title.as_str(), floor.level), ("floor", Level::Fail));
        assert_eq!(floor.detail, "invalid floor file");
    }

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
    fn hook_commands_are_recognised_by_their_words() {
        for registered in [
            "officina hook",
            "officina.exe hook",
            r"C:\Tools\Officina.EXE hook",
            "\"C:/Program Files/Officina/officina.exe\" hook",
            "'/opt/my tools/officina' hook",
            "\"C:/Program Files\"/Officina/officina.exe hook",
            "officina.exe \"hook\"",
            "  officina.exe   hook  ",
            "officina.exe hook --unexpected",
        ] {
            assert!(runs_officina_hook(registered), "{registered}");
        }
        for near_miss in [
            "officina-hooker hook",
            "officina.exe hooker",
            "officina.exe Hook",
            "echo officina hook",
            "python officina.py hook",
            "notofficina.exe hook",
            "officina.exe.bak hook",
            "C:/Program Files/Officina/officina.exe hook",
            "\"C:/Program Files/Officina/officina.exe hook\"",
            "\"C:/Program Files/Officina/officina.exe hook",
            "officina.exe doctor",
            "officina.exe",
            "",
        ] {
            assert!(!runs_officina_hook(near_miss), "{near_miss}");
        }
    }

    #[test]
    fn statusline_snippet_tees_into_the_existing_command() {
        let exe = PathBuf::from("/opt/officina");
        let with = statusline_snippet(&exe, Some("python statusline.py"));
        assert!(with.contains("\\\"/opt/officina\\\" quota write --tee | python statusline.py"));
        assert!(statusline_snippet(&exe, None).contains("quota write\""));
    }
}
