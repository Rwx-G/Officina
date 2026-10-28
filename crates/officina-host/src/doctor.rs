//! `officina doctor` (FR-11): reads, compares and reports. It never writes
//! under the Claude config dir nor in a project folder; what the author must
//! change is printed for them to paste. The relay's checks (FR-14) are
//! [`crate::relay::doctor`], after the v0.1 ones.

use std::fs;
use std::path::{Path, PathBuf};

use officina_core::contract::check_agents;
use officina_core::dates::{DAY_MS, dated_file_day, day_start_ms, utc_date};
use officina_core::floor::{Autonomy, parse_floor};
use officina_core::quota::STALE_AFTER_MS;
use officina_core::registry::model::PAUSE_AFTER_FAILED_CALLS;
use officina_core::registry::triage_call::{
    COUNT_WINDOW_MS, CallCounts, count_calls, refusal_for_binary,
};
use officina_core::registry::types::RegistryState;
use officina_core::registry::view::RESTORE_NOTICE_MS;
use officina_core::relay::gate::gated_tools;

use crate::claude;
use crate::collector::{Collector, ContractStore};
use crate::files::{now_ms, read_recent_events};
use crate::paths::{ENV_FLOOR, Paths, env_path};
use crate::registry::RegistryPaths;
use crate::registry::store::{
    self as registry_store, Location, daily_copies, onedrive_roots_from_env, quarantined,
};
use crate::relay::doctor::{self as relay_doctor, RelayDoctor};
use crate::relay::gate::GATE_EVENT;
use crate::relay::home::RelayHome;
use crate::writers::hook_keeps_excerpts;

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
    let paths = collector.paths();
    let user_settings = paths.claude_config_dir.join("settings.json");
    let relay = RelayHome::new(paths.officina_home.clone(), env_path(ENV_FLOOR));
    let required = registrations(gate_required(&relay));
    let (mut checks, version) = check_claude(collector);
    checks.push(check_hooks(&user_settings, exe, &required));
    checks.extend(check_hook_activity(&paths.events_dir(), now));
    checks.push(check_quota(collector, &user_settings, exe, now));
    checks.push(check_officina_home(&paths.officina_home));
    let onedrive_roots = onedrive_roots_from_env();
    let mut registry = check_registry(&paths.officina_home, &onedrive_roots, now);
    if let Some(version) = &version {
        registry.push(check_hook_fields(&collector.contract_file(), version));
    }
    registry.extend(check_triage(
        collector.claude(),
        &paths.officina_home,
        &onedrive_roots,
        now,
    ));
    checks.extend(registry_section(registry));
    checks.push(check_floor(collector));
    let system_dir = crate::relay::home::system_dir();
    checks.extend(relay_doctor::checks(&RelayDoctor {
        home: &relay,
        doctor_floor: collector.floor().path.as_deref(),
        system_dir: system_dir.as_deref(),
        now_ms: now,
    }));
    checks
}

/// The titles of the registry section (story 3.12 AC 1, AD-53), in the
/// order doctor prints them: the registry's own checks (stories 3.1, 3.2),
/// then the canary's hook fields (3.8), then the triage model's (3.9,
/// 3.11).
pub const REGISTRY_SECTION: &[&str] = &[
    "registry location",
    "registry",
    "registry integrity",
    "registry copies",
    "event files",
    "event files rewritten",
    "registry quarantine",
    "registry restore",
    "hook fields",
    "triage call",
    "triage model",
];

/// The registry section's checks in [`REGISTRY_SECTION`]'s order: whichever
/// check emits a line, it lands at its place, so the section reads the same
/// on every run.
fn registry_section(mut checks: Vec<Check>) -> Vec<Check> {
    let place = |check: &Check| {
        REGISTRY_SECTION
            .iter()
            .position(|title| *title == check.title)
            .unwrap_or(REGISTRY_SECTION.len())
    };
    checks.sort_by_key(place);
    checks
}

/// `claude --version`, then `claude agents --json` against the contract;
/// the version, when it was read, for the hook fields of the registry
/// section.
fn check_claude(collector: &Collector) -> (Vec<Check>, Option<String>) {
    let Some(bin) = collector.claude() else {
        return (
            vec![check(
                Level::Fail,
                "claude",
                claude::ClaudeError::NotFound.to_string(),
            )],
            None,
        );
    };
    let config_dir = collector.paths().claude_config_override.as_deref();
    match claude::version(bin, config_dir) {
        Ok(v) => (
            vec![
                check(Level::Ok, "claude", format!("{v} at {}", bin.display())),
                check_agents_output(bin, config_dir),
            ],
            Some(v),
        ),
        Err(e) => (
            vec![
                check(Level::Fail, "claude", e.to_string()),
                check_agents_output(bin, config_dir),
            ],
            None,
        ),
    }
}

/// The hook payload fields of `HOOK_FIELDS` the running version left out,
/// and the buffered events none of its lines carried yet, from the record
/// the window keeps in `state/contract.json` (AD-56, D-71). A missing field
/// warns (story 3.12 AC 1): the board keeps working, and the registry
/// records the session without it.
fn check_hook_fields(contract_file: &Path, version: &str) -> Check {
    const TITLE: &str = "hook fields";
    let store = ContractStore::read(contract_file);
    let Some(record) = store.versions.get(version) else {
        return check(
            Level::Ok,
            TITLE,
            format!("not checked yet for {version}: the window checks them as hook events arrive"),
        );
    };
    let (level, mut detail) = if record.hook_missing.is_empty() {
        (
            Level::Ok,
            "every field of HOOK_FIELDS present since this version was first seen".to_owned(),
        )
    } else {
        (
            Level::Warn,
            format!("missing: {}", record.hook_missing.join(", ")),
        )
    };
    if !record.hook_not_observed.is_empty() {
        detail.push_str(&format!(
            "; not observed yet: {}",
            record.hook_not_observed.join(", ")
        ));
    }
    check(level, TITLE, detail)
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
            collector.paths().account_key()
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

/// More event files than this waiting for ingestion or deletion is a
/// warning (AD-44, R21): normally today's, yesterday's and the two before
/// them, until both kept copies hold them.
pub const WAITING_EVENT_FILES: usize = 7;

/// The registry (FR-1 as amended, AD-53's doctor line, D-47, D-91): where
/// it lies, its version, `quick_check`, its daily copies, a quarantine and
/// the last restore. Doctor opens it read-only and never creates it; on a
/// network folder it does not open it at all.
pub fn check_registry(officina_home: &Path, onedrive_roots: &[PathBuf], now: i64) -> Vec<Check> {
    let paths = RegistryPaths::new(officina_home);
    let dir = paths.dir().display().to_string();
    let mut checks = Vec::new();
    match registry_store::locate(officina_home, onedrive_roots) {
        Location::Network => {
            return vec![check(
                Level::Fail,
                "registry location",
                format!(
                    "{dir} is on a network folder, where SQLite's WAL does not work: the registry is not opened there; set OFFICINA_HOME to a local folder (the board keeps working)"
                ),
            )];
        }
        Location::Synced { exclude } => checks.push(check(
            Level::Warn,
            "registry location",
            format!(
                "{dir} is in a folder a sync client manages, which may write an older copy back: exclude {} from the client",
                exclude.display()
            ),
        )),
        Location::Local => checks.push(check(Level::Ok, "registry location", dir.clone())),
    }
    let opened = registry_store::open_reader(officina_home, onedrive_roots);
    let state = match &opened {
        Ok(reader) => match reader.user_version() {
            Ok(version) => check(Level::Ok, "registry", format!("schema version {version}")),
            Err(e) => check(Level::Fail, "registry", format!("cannot be read: {e}")),
        },
        Err(failure) => {
            let detail = failure.detail.as_deref().unwrap_or_default();
            match failure.state {
                RegistryState::Absent => check(
                    Level::Ok,
                    "registry",
                    "absent: the window creates it when it starts",
                ),
                RegistryState::Older => check(
                    Level::Warn,
                    "registry",
                    "written by an older Officina: the window copies it, then migrates it, when it starts",
                ),
                RegistryState::Newer => check(
                    Level::Warn,
                    "registry",
                    "written by a newer Officina: the window opens it read-only, ingests nothing and deletes nothing",
                ),
                RegistryState::Unavailable => {
                    check(Level::Fail, "registry", format!("cannot be read: {detail}"))
                }
                RegistryState::Ready | RegistryState::Network => {
                    check(Level::Fail, "registry", format!("not read: {detail}"))
                }
            }
        }
    };
    checks.push(state);
    if let Ok(reader) = &opened {
        checks.push(match reader.quick_check() {
            Ok(findings) if registry_store::passed(&findings) => {
                check(Level::Ok, "registry integrity", "quick_check passed")
            }
            Ok(findings) => check(
                Level::Fail,
                "registry integrity",
                format!(
                    "quick_check failed: {}; the window sets it aside and restores the newest valid copy when it starts",
                    findings.join("; ")
                ),
            ),
            Err(e) => check(Level::Fail, "registry integrity", e.to_string()),
        });
        let copies = daily_copies(&paths);
        checks.push(match copies.first() {
            None => check(
                Level::Warn,
                "registry copies",
                "no daily copy yet: the window takes one each day it runs",
            ),
            Some(newest) => {
                let age = (day_start_ms(utc_date(now)) - day_start_ms(newest.day)) / DAY_MS;
                let (y, m, d) = newest.day;
                check(
                    Level::Ok,
                    "registry copies",
                    format!(
                        "{} daily copies, the newest from {y:04}-{m:02}-{d:02} ({age} days old)",
                        copies.len()
                    ),
                )
            }
        });
    }
    let waiting = fs::read_dir(officina_home.join("events"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| dated_file_day(&entry.file_name().to_string_lossy()).is_some())
        .count();
    if waiting > WAITING_EVENT_FILES {
        checks.push(check(
            Level::Warn,
            "event files",
            format!(
                "{waiting} event files wait for ingestion or deletion: the window has not run with a usable registry for days; nothing is lost, they are ingested when it does"
            ),
        ));
    }
    let rewritten: Vec<String> = opened
        .as_ref()
        .ok()
        .and_then(|reader| reader.event_files().ok())
        .into_iter()
        .flatten()
        .filter(|row| row.rewrites > 0)
        .map(|row| format!("{} ({} times)", row.name, row.rewrites))
        .collect();
    if !rewritten.is_empty() {
        checks.push(check(
            Level::Warn,
            "event files rewritten",
            format!(
                "found shorter than what was ingested, and read again from their start: {}; something other than the hook writes them",
                rewritten.join(", ")
            ),
        ));
    }
    let set_aside = quarantined(&paths);
    if !set_aside.is_empty() {
        checks.push(check(
            Level::Warn,
            "registry quarantine",
            format!(
                "{} damaged registries set aside under {}",
                set_aside.len(),
                paths.quarantine_dir().display()
            ),
        ));
    }
    if let Some(notice) = opened.as_ref().ok().and_then(|r| r.restore_notice()) {
        let recent = now - notice.restored_at_ms < RESTORE_NOTICE_MS;
        let (y, m, d) = utc_date(notice.restored_at_ms);
        checks.push(check(
            if recent { Level::Warn } else { Level::Ok },
            "registry restore",
            format!(
                "restored from {} on {y:04}-{m:02}-{d:02}: author actions taken after that copy are lost, events were ingested again",
                notice.copy
            ),
        ));
    }
    checks
}

/// Calls, tokens and cost, as doctor prints them.
fn counted(counts: &CallCounts) -> String {
    format!(
        "{} calls ({} without an answer), {} refused, {} input and {} output tokens, USD {:.4}",
        counts.calls,
        counts.unanswered,
        counts.refused,
        counts.input_tokens,
        counts.output_tokens,
        counts.cost_microusd as f64 / 1_000_000.0
    )
}

/// The triage model (AD-53): whether it proposes or is paused, the calls
/// and their cost today and over the count window, the latest dry run, and
/// the refusal every call meets while `claude` resolves to a `.cmd` or
/// `.bat` shim (AD-51). Read-only; the registry is never created.
pub fn check_triage(
    claude: Option<&Path>,
    officina_home: &Path,
    onedrive_roots: &[PathBuf],
    now: i64,
) -> Vec<Check> {
    let mut checks = Vec::new();
    if let Some(shim) = claude.filter(|bin| refusal_for_binary(&bin.to_string_lossy()).is_some()) {
        checks.push(check(
            Level::Warn,
            "triage call",
            format!(
                "claude resolves to {}, a shim only cmd.exe runs: every triage call is refused before it starts; install the native claude.exe or set OFFICINA_CLAUDE_BIN to it",
                shim.display()
            ),
        ));
    }
    let Ok(reader) = registry_store::open_reader(officina_home, onedrive_roots) else {
        checks.push(check(
            Level::Ok,
            "triage model",
            "disabled; no registry to count its calls in",
        ));
        return checks;
    };
    let facts = match reader.model_facts(None) {
        Ok(facts) => facts,
        Err(error) => {
            checks.push(check(
                Level::Fail,
                "triage model",
                format!("cannot be read: {error}"),
            ));
            return checks;
        }
    };
    let (level, state) = if facts.on() && facts.paused() {
        let since = facts.pause.as_ref().map_or(0, |p| p.at_ms);
        let (y, m, d) = utc_date(since);
        (
            Level::Warn,
            format!(
                "paused since {y:04}-{m:02}-{d:02}: its last {PAUSE_AFTER_FAILED_CALLS} proposal calls failed in a row; turn it on again in the Triage panel to lift the pause, or a new claude lifts it"
            ),
        )
    } else if facts.on() {
        (
            Level::Ok,
            "enabled: it proposes a verdict where no rule settled one".to_owned(),
        )
    } else if let Some(reason) = facts.setting.as_ref().and_then(|s| s.value.reason) {
        (
            Level::Ok,
            format!(
                "disabled: it turned itself off ({}); a newer winning dry run is needed to turn it on again",
                reason.code()
            ),
        )
    } else {
        (
            Level::Ok,
            "disabled: it proposes nothing; a dry run scores it on your past confirmations"
                .to_owned(),
        )
    };
    let calls = reader
        .triage_calls_since(now - COUNT_WINDOW_MS)
        .map(|calls| count_calls(&calls, now));
    let mut detail = state;
    match calls {
        Ok(view) => {
            detail.push_str(&format!(
                "; today {}; last 30 days {}",
                counted(&view.today),
                counted(&view.last_30_days)
            ));
            for refusal in &view.refusals {
                detail.push_str(&format!(
                    "; refused {} times for {}",
                    refusal.count,
                    refusal.cause.code()
                ));
            }
        }
        Err(e) => detail.push_str(&format!("; its calls cannot be read: {e}")),
    }
    if let Ok(Some(latest)) = reader.latest_evaluation() {
        let (y, m, d) = utc_date(latest.at_ms);
        detail.push_str(&format!(
            "; latest dry run {y:04}-{m:02}-{d:02}: {} cases, model right {}, default right {}",
            latest.tally.cases, latest.tally.model_correct, latest.tally.default_correct
        ));
    }
    checks.push(check(level, "triage model", detail));
    checks
}

/// The floor file, if any, loaded without error.
fn check_floor(collector: &Collector) -> Check {
    if let Some(warning) = no_excerpts_unseen_by_the_hook(collector) {
        return warning;
    }
    match (&collector.floor().path, &collector.floor().error) {
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
                collector.floor().projects.len()
            ),
        ),
        (Some(_), Some(e)) => check(Level::Fail, "floor", e.clone()),
    }
}

/// The warning for a floor that sets `no_excerpts` the hook does not read
/// (security audit L3): the hook finds its floor through `OFFICINA_FLOOR`,
/// else `<officina home>/floor.toml`, never through `--floor`, so a floor
/// given only on a command line leaves excerpts in the event files.
fn no_excerpts_unseen_by_the_hook(collector: &Collector) -> Option<Check> {
    let floor = collector.floor();
    let path = floor.path.as_deref()?;
    if !floor.file.as_ref().is_some_and(|f| f.no_excerpts) {
        return None;
    }
    let default_floor = collector.paths().officina_home.join("floor.toml");
    let hook_paths = Paths {
        floor_file: env_path(ENV_FLOOR)
            .or_else(|| default_floor.is_file().then_some(default_floor)),
        ..collector.paths().clone()
    };
    hook_keeps_excerpts(&hook_paths).then(|| {
        check(
            Level::Warn,
            "floor",
            format!(
                "{} sets no_excerpts, but the hook does not read it and keeps writing excerpts to the event files: it reads OFFICINA_FLOOR, else {}; set OFFICINA_FLOOR to this file, or OFFICINA_NO_EXCERPTS=1",
                path.display(),
                collector.paths().officina_home.join("floor.toml").display()
            ),
        )
    })
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
                ("registry location", Level::Ok, false),
                ("registry", Level::Ok, false),
                ("triage model", Level::Ok, false),
                ("floor", Level::Ok, false),
            ]
        );
        assert!(
            !RegistryPaths::new(&paths.officina_home).dir().exists(),
            "doctor never creates the registry"
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

    /// Security audit L3: a floor that sets `no_excerpts` the hook cannot
    /// find warns; the same floor at the place the hook reads does not.
    #[test]
    fn a_no_excerpts_floor_the_hook_cannot_see_warns() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            officina_home: dir.path().join("officina"),
            claude_config_dir: dir.path().join("config"),
            claude_config_override: None,
            floor_file: None,
        };
        std::fs::create_dir_all(&paths.officina_home).unwrap();
        let text = "schema = 1\nno_excerpts = true\n";
        let elsewhere = dir.path().join("officina.toml");
        std::fs::write(&elsewhere, text).unwrap();
        let floor = |path: PathBuf| LoadedFloor {
            path: Some(path),
            file: Some(parse_floor(text).unwrap()),
            ..Default::default()
        };
        let collector = Collector::from_parts(paths.clone(), floor(elsewhere), None);
        let line = check_floor(&collector);
        assert_eq!(line.level, Level::Warn, "{line:?}");
        assert!(line.detail.contains("OFFICINA_FLOOR"), "{line:?}");
        let default_floor = paths.officina_home.join("floor.toml");
        std::fs::write(&default_floor, text).unwrap();
        let collector = Collector::from_parts(paths, floor(default_floor), None);
        assert_eq!(check_floor(&collector).level, Level::Ok);
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

    /// AD-56, D-44: the canary checks the payload of every event the hook
    /// buffers, and names no event it does not.
    #[test]
    fn every_buffered_event_has_hook_fields() {
        use officina_core::contract::{HOOK_FIELDS, hook_fields};
        for event in BUFFERED_EVENTS {
            assert!(
                !hook_fields(event).is_empty(),
                "{event} has no HOOK_FIELDS entry"
            );
        }
        for (event, _) in HOOK_FIELDS {
            assert!(
                BUFFERED_EVENTS.contains(event),
                "{event} is not a buffered event"
            );
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

    const NOW: i64 = 1_793_102_400_000;
    const DAY: i64 = DAY_MS;

    /// A registry of schema 1 at `version` under `home`, written without the
    /// window's writer, which this test build may not hold.
    fn registry_at(home: &Path, version: i64) -> rusqlite::Connection {
        let paths = RegistryPaths::new(home);
        fs::create_dir_all(paths.dir()).unwrap();
        let conn = rusqlite::Connection::open(paths.database()).unwrap();
        conn.execute_batch(crate::registry::schema::MIGRATIONS[0])
            .unwrap();
        conn.pragma_update(None, "user_version", version).unwrap();
        conn
    }

    fn registry_line<'a>(checks: &'a [Check], title: &str) -> &'a Check {
        checks
            .iter()
            .find(|c| c.title == title)
            .unwrap_or_else(|| panic!("no {title} line in {checks:?}"))
    }

    /// Story 3.9 AC 11 (AD-51, AD-53): a `claude` that resolves to a
    /// `.cmd` or `.bat` shim is reported as refusing every triage call; a
    /// native one is not.
    #[test]
    fn cmd_shim_is_reported_as_refusing_triage() {
        let dir = tempfile::tempdir().unwrap();
        for shim in ["claude.cmd", "claude.Bat"] {
            let checks = check_triage(Some(&dir.path().join(shim)), dir.path(), &[], NOW);
            let line = registry_line(&checks, "triage call");
            assert_eq!(line.level, Level::Warn, "{shim}");
            assert!(line.detail.contains("refused"), "{}", line.detail);
        }
        let native = check_triage(Some(&dir.path().join("claude.exe")), dir.path(), &[], NOW);
        assert!(native.iter().all(|c| c.title != "triage call"));
        assert!(
            !RegistryPaths::new(dir.path()).dir().exists(),
            "never created"
        );
    }

    /// Story 3.9 AC 11 (AD-52, AD-53): the model's state and its calls,
    /// today and over 30 days, with their cost and the refusals' causes.
    #[test]
    fn model_state_and_counts_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let conn = registry_at(dir.path(), crate::registry::schema::SCHEMA_VERSION);
        for (at, outcome, refusal, cost) in [
            (NOW - 60_000, "answer", None, Some(1_100)),
            (NOW - 120_000, "timeout", None, Some(0)),
            (NOW - 180_000, "refused", Some("cmd_shim"), None),
            (NOW - 10 * DAY, "answer", None, Some(2_000)),
        ] {
            conn.execute(
                "INSERT INTO triage_call (purpose, started_at_ms, outcome, refusal, input_tokens, output_tokens, cost_microusd)
                 VALUES ('dry_run', ?1, ?2, ?3, 4500, 380, ?4)",
                rusqlite::params![at, outcome, refusal, cost],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO model_evaluation (at_ms, cases, model_correct, default_correct, model_false_finished, unanswered)
             VALUES (?1, 30, 21, 18, 2, 1)",
            [NOW - DAY],
        )
        .unwrap();
        let checks = check_triage(None, dir.path(), &[], NOW);
        let line = registry_line(&checks, "triage model");
        assert_eq!(line.level, Level::Ok);
        for part in [
            "disabled",
            "today 2 calls (1 without an answer), 1 refused",
            "USD 0.0011",
            "last 30 days 3 calls",
            "USD 0.0031",
            "refused 1 times for cmd_shim",
            "30 cases, model right 21, default right 18",
        ] {
            assert!(line.detail.contains(part), "{part}: {}", line.detail);
        }
        conn.execute(
            "INSERT INTO setting (key, value, src, at_ms) VALUES ('triage_model', '{\"on\":true}', 'author', ?1)",
            [NOW],
        )
        .unwrap();
        let on = check_triage(None, dir.path(), &[], NOW);
        let line = registry_line(&on, "triage model");
        assert_eq!(line.level, Level::Ok);
        assert!(line.detail.starts_with("enabled"), "{}", line.detail);
        conn.execute(
            "INSERT INTO setting (key, value, src, at_ms) VALUES ('triage_model_pause', '{\"paused\":true}', 'rules', ?1)",
            [NOW],
        )
        .unwrap();
        let paused = check_triage(None, dir.path(), &[], NOW);
        let line = registry_line(&paused, "triage model");
        assert_eq!(line.level, Level::Warn, "a paused model warns");
        assert!(
            line.detail.starts_with("paused since") && line.detail.contains("turn it on again"),
            "{}",
            line.detail
        );
        conn.execute(
            "UPDATE setting SET value = '{\"on\":false,\"reason\":\"worse_than_default\"}', src = 'rules'
             WHERE key = 'triage_model'",
            [],
        )
        .unwrap();
        let off = check_triage(None, dir.path(), &[], NOW);
        let line = registry_line(&off, "triage model");
        assert_eq!(line.level, Level::Ok);
        assert!(
            line.detail
                .contains("turned itself off (worse_than_default)"),
            "{}",
            line.detail
        );
    }

    #[test]
    fn registry_in_a_synced_folder_warns_and_names_it() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".SynologyWorkingDirectory"), b"").unwrap();
        let home = dir.path().join("officina");
        let checks = check_registry(&home, &[], NOW);
        let line = registry_line(&checks, "registry location");
        assert_eq!(line.level, Level::Warn);
        assert!(
            line.detail
                .contains(&RegistryPaths::new(&home).dir().display().to_string()),
            "{}",
            line.detail
        );
        assert!(!RegistryPaths::new(&home).dir().exists());

        let onedrive = tempfile::tempdir().unwrap();
        let checks = check_registry(
            &onedrive.path().join("officina"),
            &[onedrive.path().to_path_buf()],
            NOW,
        );
        assert_eq!(
            registry_line(&checks, "registry location").level,
            Level::Warn
        );

        let network = check_registry(
            Path::new(r"\\localhost\officina-missing-share\home"),
            &[],
            NOW,
        );
        assert_eq!(
            summary(&network),
            [("registry location", Level::Fail, false)]
        );
    }

    #[test]
    fn newer_registry_warns() {
        let dir = tempfile::tempdir().unwrap();
        drop(registry_at(
            dir.path(),
            crate::registry::schema::SCHEMA_VERSION + 1,
        ));
        let checks = check_registry(dir.path(), &[], NOW);
        let line = registry_line(&checks, "registry");
        assert_eq!(line.level, Level::Warn);
        assert!(line.detail.contains("newer"), "{}", line.detail);
        assert!(checks.iter().all(|c| c.title != "registry integrity"));

        rusqlite::Connection::open(RegistryPaths::new(dir.path()).database())
            .unwrap()
            .pragma_update(
                None,
                "user_version",
                crate::registry::schema::SCHEMA_VERSION,
            )
            .unwrap();
        let checks = check_registry(dir.path(), &[], NOW);
        assert_eq!(registry_line(&checks, "registry").level, Level::Ok);
    }

    #[test]
    fn failed_quick_check_fails() {
        let dir = tempfile::tempdir().unwrap();
        drop(registry_at(
            dir.path(),
            crate::registry::schema::SCHEMA_VERSION,
        ));
        let db = RegistryPaths::new(dir.path()).database();
        let mut bytes = fs::read(&db).unwrap();
        let page = usize::from(u16::from_be_bytes([bytes[16], bytes[17]]));
        bytes[page..2 * page].fill(0xFF);
        fs::write(&db, &bytes).unwrap();
        let checks = check_registry(dir.path(), &[], NOW);
        assert_eq!(registry_line(&checks, "registry").level, Level::Ok);
        assert_eq!(
            registry_line(&checks, "registry integrity").level,
            Level::Fail
        );
        assert_eq!(exit_code(&checks), 1);
        assert_eq!(fs::read(&db).unwrap(), bytes, "doctor changed nothing");
    }

    /// AD-44, R21: event files pile up while the window does not run with a
    /// usable registry; doctor warns past the threshold, whatever the
    /// registry's state.
    #[test]
    fn more_than_seven_waiting_event_files_warn() {
        let dir = tempfile::tempdir().unwrap();
        let events = dir.path().join("events");
        fs::create_dir_all(&events).unwrap();
        fs::write(events.join("notes.txt"), b"not an event file").unwrap();
        let waiting = |checks: &[Check]| checks.iter().any(|c| c.title == "event files");
        for back in 0..WAITING_EVENT_FILES as i64 {
            fs::write(
                events.join(officina_core::dates::event_file_name(NOW - back * DAY)),
                b"",
            )
            .unwrap();
        }
        assert!(
            !waiting(&check_registry(dir.path(), &[], NOW)),
            "at the threshold"
        );
        fs::write(
            events.join(officina_core::dates::event_file_name(NOW - 30 * DAY)),
            b"",
        )
        .unwrap();
        let checks = check_registry(dir.path(), &[], NOW);
        let line = registry_line(&checks, "event files");
        assert_eq!(line.level, Level::Warn);
        assert!(
            line.detail
                .starts_with(&format!("{} event files wait", WAITING_EVENT_FILES + 1)),
            "{}",
            line.detail
        );
    }

    /// D-84: a file found shorter than its offset was read again; doctor
    /// names it.
    #[test]
    fn rewritten_event_files_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let conn = registry_at(dir.path(), crate::registry::schema::SCHEMA_VERSION);
        conn.execute(
            "INSERT INTO event_file (name, ingested_bytes, corrupt_lines, rewrites) VALUES ('2026-10-27.jsonl', 10, 0, 2), ('2026-10-26.jsonl', 99, 0, 0)",
            [],
        )
        .unwrap();
        drop(conn);
        let checks = check_registry(dir.path(), &[], NOW);
        let line = registry_line(&checks, "event files rewritten");
        assert_eq!(line.level, Level::Warn);
        assert!(
            line.detail.contains("2026-10-27.jsonl (2 times)"),
            "{}",
            line.detail
        );
        assert!(!line.detail.contains("2026-10-26"), "{}", line.detail);
    }

    #[test]
    fn copies_quarantine_and_restore_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let paths = RegistryPaths::new(dir.path());
        let conn = registry_at(dir.path(), crate::registry::schema::SCHEMA_VERSION);
        let notice =
            serde_json::json!({"copy": "registry-2026-10-25.db", "restored_at_ms": NOW - DAY});
        conn.execute(
            "INSERT INTO setting (key, value, src, at_ms) VALUES ('restore_notice', ?1, 'rules', ?2)",
            rusqlite::params![notice.to_string(), NOW - DAY],
        )
        .unwrap();
        drop(conn);
        let checks = check_registry(dir.path(), &[], NOW);
        assert_eq!(
            registry_line(&checks, "registry copies").level,
            Level::Warn,
            "no copy yet"
        );
        assert!(checks.iter().all(|c| c.title != "registry quarantine"));

        fs::create_dir_all(paths.backup_dir()).unwrap();
        for name in [
            "registry-2026-10-25.db",
            "registry-2026-10-26.db",
            "pre-v2-2026-10-20.db",
        ] {
            fs::write(paths.backup_dir().join(name), b"").unwrap();
        }
        fs::create_dir_all(paths.quarantine_dir().join((NOW - DAY).to_string())).unwrap();
        let checks = check_registry(dir.path(), &[], NOW);
        let copies = registry_line(&checks, "registry copies");
        assert_eq!(copies.level, Level::Ok);
        assert!(
            copies.detail.starts_with("2 daily copies")
                && copies.detail.contains("2026-10-26 (1 days old)"),
            "{}",
            copies.detail
        );
        let quarantine = registry_line(&checks, "registry quarantine");
        assert_eq!(quarantine.level, Level::Warn);
        assert!(
            quarantine.detail.starts_with("1 damaged"),
            "{}",
            quarantine.detail
        );
        let restore = registry_line(&checks, "registry restore");
        assert_eq!(restore.level, Level::Warn);
        assert!(
            restore.detail.contains("registry-2026-10-25.db")
                && restore.detail.contains("2026-10-26"),
            "{}",
            restore.detail
        );
        let later = check_registry(dir.path(), &[], NOW + 7 * DAY);
        assert_eq!(registry_line(&later, "registry restore").level, Level::Ok);
    }

    #[test]
    fn statusline_snippet_tees_into_the_existing_command() {
        let exe = PathBuf::from("/opt/officina");
        let with = statusline_snippet(&exe, Some("python statusline.py"));
        assert!(with.contains("\\\"/opt/officina\\\" quota write --tee | python statusline.py"));
        assert!(statusline_snippet(&exe, None).contains("quota write\""));
    }
}
