//! `officina doctor` (FR-11): reads, compares and reports. It never writes
//! under the Claude config dir nor in a project folder; what the author must
//! change is printed for them to paste. The relay's checks (FR-14) are
//! [`crate::relay::doctor`], after the v0.1 ones.

use std::fs;
use std::path::Path;

use officina_core::contract::check_agents;
use officina_core::floor::{Autonomy, parse_floor};
use officina_core::quota::STALE_AFTER_MS;
use officina_core::relay::gate::gated_tools;

use crate::claude;
use crate::collector::Collector;
use crate::files::{now_ms, read_recent_events};
use crate::paths::{ENV_FLOOR, env_path};
use crate::relay::doctor::{self as relay_doctor, RelayDoctor};
use crate::relay::gate::GATE_EVENT;
use crate::relay::home::RelayHome;

/// Hook events the hook records in the event buffer (framing principle 5).
/// Any other event the hook receives is the gate's or ignored.
pub const BUFFERED_EVENTS: [&str; 5] = [
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "Stop",
    "Notification",
];

/// One hook registration doctor requires: an event and, for a tool event,
/// the tools its matcher must name (none for an event without tools).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    pub event: &'static str,
    pub tools: Vec<&'static str>,
}

/// The gate's registration (AD-26, D-18 as amended by D-171 and D-173):
/// `PreToolUse` for every shell tool of `SHELL_TOOLS` and file tool of
/// `FILE_TOOLS` and no other, so no other tool call pays for a process
/// start (E9).
pub fn gate_registration() -> Registration {
    Registration {
        event: GATE_EVENT,
        tools: gated_tools(),
    }
}

/// The registrations doctor requires: every buffered event, plus the gate's
/// when the relay guards a project (D-18), so workstations without one pay
/// nothing on each shell tool call.
pub fn registrations(gate: bool) -> Vec<Registration> {
    BUFFERED_EVENTS
        .into_iter()
        .map(|event| Registration {
            event,
            tools: Vec::new(),
        })
        .chain(gate.then(gate_registration))
        .collect()
}

/// True when the relay guards at least one project: in its floor (phase 1)
/// or in the approved snapshot, either being enough (D-14).
pub fn gate_required(home: &RelayHome) -> bool {
    let approved = home.read_approved();
    let in_snapshot = approved.approved().is_some_and(|a| {
        a.canonical
            .projects
            .values()
            .any(|p| p.autonomy == Autonomy::Guarded)
    });
    let in_floor = fs::read_to_string(home.relay_floor(&approved))
        .ok()
        .and_then(|text| parse_floor(&text).ok())
        .is_some_and(|floor| {
            floor
                .projects
                .iter()
                .any(|p| floor.autonomy_of(p) == Autonomy::Guarded)
        });
    in_snapshot || in_floor
}

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

pub(crate) fn check(level: Level, title: &str, detail: impl Into<String>) -> Check {
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

/// The hooks block to paste into user settings (AD-16), one entry per
/// registration, with a matcher naming its tools when it has some.
pub fn hooks_block(exe: &Path, registrations: &[Registration]) -> String {
    let command = format!("{} hook", quoted_exe(exe));
    let mut hooks = serde_json::Map::new();
    for registration in registrations {
        let mut group = serde_json::json!({
            "hooks": [{ "type": "command", "command": command, "timeout": 5 }]
        });
        if !registration.tools.is_empty() {
            group["matcher"] = registration.tools.join("|").into();
        }
        hooks.insert(
            registration.event.to_owned(),
            serde_json::Value::Array(vec![group]),
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

/// What of `registrations` the hooks in `settings` leave out: an event none
/// of whose matcher groups runs `officina hook` (a command whose first word
/// is the `officina` binary and whose second is `hook`), or, for an event
/// with tools, the tools no such group's matcher names, written
/// `Event (Tool, Tool)` when only some are left out. Other hooks in the same
/// file are ignored (E2: hooks merge).
pub fn missing_hook_events(
    settings: &serde_json::Value,
    registrations: &[Registration],
) -> Vec<String> {
    registrations
        .iter()
        .filter_map(|registration| {
            let ours: Vec<&serde_json::Value> = settings
                .get("hooks")
                .and_then(|h| h.get(registration.event))
                .and_then(|m| m.as_array())
                .into_iter()
                .flatten()
                .filter(|group| {
                    group
                        .get("hooks")
                        .and_then(|h| h.as_array())
                        .into_iter()
                        .flatten()
                        .filter_map(|h| h.get("command").and_then(|c| c.as_str()))
                        .any(runs_officina_hook)
                })
                .collect();
            if registration.tools.is_empty() {
                return ours.is_empty().then(|| registration.event.to_owned());
            }
            let left_out: Vec<&str> = registration
                .tools
                .iter()
                .copied()
                .filter(|tool| {
                    !ours.iter().any(|group| {
                        group
                            .get("matcher")
                            .and_then(|m| m.as_str())
                            .is_some_and(|matcher| matcher_names(matcher, tool))
                    })
                })
                .collect();
            match left_out.len() {
                0 => None,
                n if n == registration.tools.len() => Some(registration.event.to_owned()),
                _ => Some(format!("{} ({})", registration.event, left_out.join(", "))),
            }
        })
        .collect()
}

/// True when a hook matcher names `tool`: the tool itself or one of its
/// `|` alternatives. A missing, empty or `*` matcher runs the hook on every
/// tool call, which works but puts a process start on each (E9), so it does
/// not count: doctor prints the block with the matcher instead.
fn matcher_names(matcher: &str, tool: &str) -> bool {
    matcher
        .split('|')
        .any(|alternative| alternative.trim() == tool)
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
    let relay = RelayHome::new(paths.officina_home.clone(), env_path(ENV_FLOOR));
    let required = registrations(gate_required(&relay));
    let mut checks = check_claude(collector);
    checks.push(check_hooks(&user_settings, exe, &required));
    checks.extend(check_hook_activity(&paths.events_dir(), now));
    checks.push(check_quota(collector, &user_settings, exe, now));
    checks.push(check_officina_home(&paths.officina_home));
    checks.push(check_floor(collector));
    let system_dir = crate::relay::home::system_dir();
    checks.extend(relay_doctor::checks(&RelayDoctor {
        home: &relay,
        doctor_floor: collector.floor.path.as_deref(),
        system_dir: system_dir.as_deref(),
        now_ms: now,
    }));
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

/// Every required registration in the user settings, else the block to
/// paste.
fn check_hooks(user_settings: &Path, exe: &Path, required: &[Registration]) -> Check {
    let settings = read_json(user_settings).unwrap_or(serde_json::Value::Null);
    let missing = missing_hook_events(&settings, required);
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
        paste: Some(hooks_block(exe, required)),
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
    use officina_core::relay::gate::gated_tools_matcher;
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
        for gate in [false, true] {
            let required = registrations(gate);
            let block: serde_json::Value =
                serde_json::from_str(&hooks_block(&exe, &required)).unwrap();
            assert!(missing_hook_events(&block, &required).is_empty());
            for registration in &required {
                let cmd = block["hooks"][registration.event][0]["hooks"][0]["command"]
                    .as_str()
                    .unwrap();
                assert_eq!(cmd, "\"C:/Program Files/Officina/officina.exe\" hook");
            }
        }
    }

    #[test]
    fn partially_registered_hooks_are_reported() {
        let settings = serde_json::json!({"hooks": {
            "Stop": [{"hooks": [{"type": "command", "command": "python x.py"},
                                 {"type": "command", "command": "officina.exe hook"}]}],
            "SessionStart": [{"hooks": [{"type": "command", "command": "officina.exe doctor"}]}]
        }});
        let required = registrations(false);
        let expected: Vec<&str> = BUFFERED_EVENTS
            .into_iter()
            .filter(|event| *event != "Stop")
            .collect();
        assert_eq!(missing_hook_events(&settings, &required), expected);
        assert_eq!(
            missing_hook_events(&serde_json::Value::Null, &required).len(),
            BUFFERED_EVENTS.len()
        );
    }

    fn relay_home_with(floor: Option<&str>) -> (tempfile::TempDir, RelayHome) {
        let dir = tempfile::tempdir().unwrap();
        let home = RelayHome::new(dir.path().join("officina"), None);
        if let Some(text) = floor {
            fs::create_dir_all(home.officina_home()).unwrap();
            fs::write(home.default_floor(), text).unwrap();
        }
        (dir, home)
    }

    #[test]
    fn block_includes_pre_tool_use_only_with_a_guarded_project() {
        let exe = PathBuf::from("/opt/officina");
        let guarded = "schema = 2\n[[project]]\nname = \"lab\"\npath = \"Lab\"\n";
        let free = "schema = 2\n[[project]]\nname = \"lab\"\npath = \"Lab\"\nautonomy = \"free\"\n";
        let v0_1 = "schema = 1\n[[project]]\nname = \"lab\"\npath = \"Lab\"\n";
        for (floor, expected) in [
            (None, false),
            (Some(free), false),
            (Some(v0_1), false),
            (Some("not toml [["), false),
            (Some(guarded), true),
        ] {
            let (_dir, home) = relay_home_with(floor);
            assert_eq!(gate_required(&home), expected, "{floor:?}");
            let block: serde_json::Value =
                serde_json::from_str(&hooks_block(&exe, &registrations(expected))).unwrap();
            let pre_tool_use = &block["hooks"][GATE_EVENT];
            assert_eq!(!pre_tool_use.is_null(), expected, "{floor:?}");
            if expected {
                assert_eq!(pre_tool_use[0]["matcher"], gated_tools_matcher());
            }
        }
    }

    #[test]
    fn pre_tool_use_without_the_matcher_is_missing() {
        let with_matcher = |matcher: Option<&str>| {
            let mut group =
                serde_json::json!({"hooks": [{"type": "command", "command": "officina.exe hook"}]});
            if let Some(m) = matcher {
                group["matcher"] = m.into();
            }
            serde_json::json!({"hooks": {GATE_EVENT: [group]}})
        };
        let gate = [gate_registration()];
        for missing in [
            None,
            Some(""),
            Some("*"),
            Some("Read"),
            Some("Bashful"),
            Some("bash"),
        ] {
            assert_eq!(
                missing_hook_events(&with_matcher(missing), &gate),
                [GATE_EVENT],
                "{missing:?}"
            );
        }
        let names = gated_tools();
        let reversed: Vec<&str> = names.iter().rev().copied().collect();
        for present in [
            gated_tools_matcher(),
            format!("Read|{}", reversed.join("|")),
            format!("Glob | {}", names.join(" | ")),
        ] {
            assert!(
                missing_hook_events(&with_matcher(Some(&present)), &gate).is_empty(),
                "{present}"
            );
        }
        let other_hook_beside = serde_json::json!({"hooks": {GATE_EVENT: [
            {"matcher": gated_tools_matcher(), "hooks": [{"type": "command", "command": "python guard.py"}]},
            {"matcher": gated_tools_matcher(), "hooks": [{"type": "command", "command": "officina hook"}]}
        ]}});
        assert!(missing_hook_events(&other_hook_beside, &gate).is_empty());
    }

    /// D-171, D-173: the printed matcher names every shell and file tool of
    /// the constants, each once, and the printed block counts as registered.
    #[test]
    fn block_matcher_names_every_gated_tool() {
        let required = registrations(true);
        let block: serde_json::Value =
            serde_json::from_str(&hooks_block(&PathBuf::from("/opt/officina"), &required)).unwrap();
        let matcher = block["hooks"][GATE_EVENT][0]["matcher"].as_str().unwrap();
        let mut named: Vec<&str> = matcher.split('|').collect();
        let mut expected: Vec<&str> = officina_core::relay::gate::SHELL_TOOLS
            .iter()
            .map(|(name, _)| *name)
            .chain(
                officina_core::relay::gate::FILE_TOOLS
                    .iter()
                    .map(|(name, _)| *name),
            )
            .collect();
        named.sort_unstable();
        expected.sort_unstable();
        assert_eq!(named, expected);
        assert!(missing_hook_events(&block, &required).is_empty());
    }

    /// D-171: a matcher a v0.2 workstation merged earlier, naming one shell
    /// tool, leaves the others out, and doctor names them; two groups that
    /// share the tools between them cover them all.
    #[test]
    fn a_matcher_naming_bash_only_is_missing_powershell_and_monitor() {
        let gate = [gate_registration()];
        let group = |matcher: &str| serde_json::json!({"matcher": matcher, "hooks": [{"type": "command", "command": "officina.exe hook"}]});
        let names = gated_tools();
        let (first, others) = names.split_first().unwrap();
        let only_first = serde_json::json!({"hooks": {GATE_EVENT: [group(first)]}});
        assert_eq!(
            missing_hook_events(&only_first, &gate),
            [format!("{GATE_EVENT} ({})", others.join(", "))]
        );
        let split =
            serde_json::json!({"hooks": {GATE_EVENT: [group(first), group(&others.join("|"))]}});
        assert!(missing_hook_events(&split, &gate).is_empty());
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
