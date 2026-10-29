//! `officina doctor` (FR-11): reads, compares and reports. It never writes
//! under the Claude config dir nor in a project folder; what the author must
//! change is printed for them to paste. The relay's checks (FR-14) are
//! [`crate::relay::doctor`], after the v0.1 ones.

use std::fs;
use std::path::{Path, PathBuf};

use officina_core::contract::{ON_FAILURE_SINCE, check_agents, claude_version};
use officina_core::dates::{DAY_MS, dated_file_day, day_start_ms, utc_date, utc_day_text};
use officina_core::floor::{Autonomy, parse_floor};
use officina_core::quota::STALE_AFTER_MS;
use officina_core::registry::model::PAUSE_AFTER_FAILED_CALLS;
use officina_core::registry::triage_call::{
    COUNT_WINDOW_MS, CallCounts, count_calls, refusal_for_binary,
};
use officina_core::registry::types::RegistryState;
use officina_core::registry::view::RESTORE_NOTICE_MS;
use officina_core::relay::gate::gated_tools;

use crate::collector::{Collector, ContractStore};
use crate::files::{now_ms, read_recent_events};
use crate::paths::{ENV_FLOOR, Paths, env_path, events_dir_of};
use crate::registry::RegistryPaths;
use crate::registry::store::{
    self as registry_store, Location, daily_copies, onedrive_roots_from_env, quarantined,
};
use crate::relay::doctor::{self as relay_doctor, RelayDoctor};
use crate::relay::gate::GATE_EVENT;
use crate::relay::home::RelayHome;
use crate::spawn::{self, BaseEnv, ResolvedClaude, SpawnError};
use crate::writers::hook_keeps_excerpts;

/// Hook events the hook records in the event buffer (framing principle 5).
/// Any other event the hook receives is the gate's or ignored. The
/// sub-agent events (`officina_core::events::SUBAGENT_EVENTS`) are recorded
/// from v0.4's Slice 0 so that gate 1 reads real payloads (AD-178, D-518);
/// the board and the registry do not fold them.
pub const BUFFERED_EVENTS: [&str; 7] = [
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "Stop",
    "Notification",
    "SubagentStart",
    "SubagentStop",
];

/// One hook registration doctor requires: an event and, for a tool event,
/// the tools its matcher must name (none for an event without tools).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    pub event: &'static str,
    pub tools: Vec<&'static str>,
    /// The hook entry carries [`ON_FAILURE`]: a hook that cannot start,
    /// times out or crashes blocks the call instead of letting it through.
    pub fails_closed: bool,
}

/// The hook entry field and value that make Claude Code block the action
/// when the hook itself fails (hooks reference, 2.1.295 and later; D-452).
pub const ON_FAILURE: (&str, &str) = ("onFailure", "block");

/// The gate's registration (AD-26, D-18 as amended by D-171 and D-173):
/// `PreToolUse` for every shell tool of `SHELL_TOOLS` and file tool of
/// `FILE_TOOLS` and no other, so no other tool call pays for a process
/// start (E9). It fails closed (D-452): a gate whose process fails must not
/// let the call through.
pub fn gate_registration() -> Registration {
    Registration {
        event: GATE_EVENT,
        tools: gated_tools(),
        fails_closed: true,
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
            fails_closed: false,
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

/// The matcher group that registers `registration` (AD-16): one entry
/// running `officina hook`, a matcher naming its tools when it has some, and
/// [`ON_FAILURE`] on the entry of a registration that fails closed.
fn registration_group(exe: &Path, registration: &Registration) -> serde_json::Value {
    let command = format!("{} hook", quoted_exe(exe));
    let mut entry = serde_json::json!({ "type": "command", "command": command, "timeout": 5 });
    if registration.fails_closed {
        entry[ON_FAILURE.0] = ON_FAILURE.1.into();
    }
    let mut group = serde_json::json!({ "hooks": [entry] });
    if !registration.tools.is_empty() {
        group["matcher"] = registration.tools.join("|").into();
    }
    group
}

/// The `hooks` value of a settings file once `registrations` are in it
/// (D-530): `current` with every entry it holds kept as it is, plus one
/// matcher group from [`registration_group`] for each registration
/// [`missing_hook_events`] reports. For a registration with tools or one
/// that fails closed, Officina's own entries of that event are taken out
/// first, a group they leave empty going with them, so a partial matcher or
/// an entry without [`ON_FAILURE`] is replaced, never doubled. Fails when
/// `current`, or the list of an event to change, does not have the shape
/// Claude Code reads.
pub fn merged_hooks(
    current: Option<&serde_json::Value>,
    exe: &Path,
    registrations: &[Registration],
) -> Result<serde_json::Value, String> {
    let mut hooks = match current {
        None => serde_json::Map::new(),
        Some(serde_json::Value::Object(map)) => map.clone(),
        Some(_) => return Err("its \"hooks\" key is not an object".to_owned()),
    };
    for registration in registrations {
        let settings = serde_json::json!({ "hooks": serde_json::Value::Object(hooks.clone()) });
        if missing_hook_events(&settings, std::slice::from_ref(registration)).is_empty() {
            continue;
        }
        let groups = hooks
            .entry(registration.event)
            .or_insert_with(|| serde_json::Value::Array(Vec::new()));
        let serde_json::Value::Array(groups) = groups else {
            return Err(format!(
                "its \"{}\" hooks are not a list",
                registration.event
            ));
        };
        if !registration.tools.is_empty() || registration.fails_closed {
            groups.retain_mut(|group| !drop_officina_entries(group));
        }
        groups.push(registration_group(exe, registration));
    }
    Ok(serde_json::Value::Object(hooks))
}

/// Takes the entries running `officina hook` out of a matcher group; true
/// when it held some and none is left, so that the group goes too.
fn drop_officina_entries(group: &mut serde_json::Value) -> bool {
    let Some(entries) = group
        .get_mut("hooks")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return false;
    };
    let before = entries.len();
    entries.retain(|entry| {
        !entry
            .get("command")
            .and_then(serde_json::Value::as_str)
            .is_some_and(runs_officina_hook)
    });
    entries.len() < before && entries.is_empty()
}

/// The `statusLine` value that writes the quota (AD-16, D-530), from
/// `current`, a settings file's `statusLine`: its other fields kept, its
/// command teed into Officina's `quota write` (`--tee` keeps its output),
/// or kept as it is when it already runs `quota write`, which a tee into it
/// would run twice; without a command, the plain `quota write`.
pub fn statusline_value(exe: &Path, current: Option<&serde_json::Value>) -> serde_json::Value {
    let mut value = match current {
        Some(serde_json::Value::Object(map)) => map.clone(),
        _ => serde_json::Map::new(),
    };
    let command = match value
        .get("command")
        .and_then(serde_json::Value::as_str)
        .filter(|existing| !existing.trim().is_empty())
    {
        Some(existing) if runs_officina_quota_write(existing) => existing.to_owned(),
        Some(existing) => format!("{} quota write --tee | {existing}", quoted_exe(exe)),
        None => format!("{} quota write", quoted_exe(exe)),
    };
    value.entry("type").or_insert_with(|| "command".into());
    value.insert("command".to_owned(), command.into());
    serde_json::Value::Object(value)
}

/// A Claude Code settings file as doctor reads it.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingsFile {
    /// No file at that path.
    Absent,
    /// A file doctor cannot read as a JSON object, with why. The reason
    /// names a position, never the file's content.
    Unreadable(String),
    Object(serde_json::Map<String, serde_json::Value>),
}

impl SettingsFile {
    pub fn read(path: &Path) -> SettingsFile {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return SettingsFile::Absent,
            Err(e) => return SettingsFile::Unreadable(e.to_string()),
        };
        match serde_json::from_str(text.trim_start_matches('\u{feff}')) {
            Ok(serde_json::Value::Object(map)) => SettingsFile::Object(map),
            Ok(_) => SettingsFile::Unreadable("not a JSON object".to_owned()),
            Err(e) => SettingsFile::Unreadable(e.to_string()),
        }
    }

    /// The value of `key`, when the file is an object that has it.
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        match self {
            SettingsFile::Object(map) => map.get(key),
            SettingsFile::Absent | SettingsFile::Unreadable(_) => None,
        }
    }
}

/// What to do so that `key` of the settings file `file` holds `value`
/// (D-530): the sentence saying where, and the block to paste, which never
/// holds another key of the file, since those can hold environment values
/// or tokens. The whole file when it does not exist; else the key alone,
/// to add or to replace whole. No block for a file doctor cannot read: one
/// that does not start from the file's content could drop part of it.
pub fn key_paste(
    file: &Path,
    settings: &SettingsFile,
    key: &str,
    value: &serde_json::Value,
) -> (String, Option<String>) {
    let file_name = file.display();
    let pretty =
        |value: &serde_json::Value| serde_json::to_string_pretty(value).unwrap_or_default();
    let fragment = || format!("\"{key}\": {}", pretty(value));
    match settings {
        SettingsFile::Absent => (
            format!(
                "Create {file_name} with this content (if doctor prints another block for the same file, put both keys in the one object)"
            ),
            Some(pretty(&serde_json::json!({ key: value }))),
        ),
        SettingsFile::Object(map) if map.contains_key(key) => (
            format!("Replace the whole \"{key}\" key of {file_name} with this one"),
            Some(fragment()),
        ),
        SettingsFile::Object(_) => (
            format!("Add this \"{key}\" key to {file_name}, beside its other keys"),
            Some(fragment()),
        ),
        SettingsFile::Unreadable(why) => (
            format!(
                "{file_name} cannot be read as a JSON object ({why}): fix it and run doctor again; no block is printed, since one that does not start from the file's content could drop part of it"
            ),
            None,
        ),
    }
}

/// A block doctor prints and also writes under the Officina home (D-534):
/// a terminal can cut a long line, a command's path included, when it is
/// copied, and the file keeps every line whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasteFile {
    pub path: PathBuf,
    pub text: String,
}

/// The file under `dir` holding the block for `key` of `target`'s settings.
pub fn paste_file_path(dir: &Path, target: &str, key: &str) -> PathBuf {
    dir.join(format!("{target}-{key}.json"))
}

/// How a project folder's blocks are named: `project-`, its folder name
/// with letters, digits, `.`, `_` and `-` kept, then the start of its whole
/// path's hash, so two folders of one name never share a file.
pub fn project_target(folder: &Path) -> String {
    let name: String = folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let hash = officina_core::sha256_hex(folder.to_string_lossy().as_bytes());
    format!("project-{name}-{}", &hash[..8])
}

/// One file's part of a paste, the sentence then the block under it, and
/// the file at `path` that holds the same block.
fn saved_section(
    (instruction, block): (String, Option<String>),
    path: PathBuf,
) -> (String, Option<PasteFile>) {
    match block {
        Some(block) => (
            format!(
                "{instruction} (also in {}, its lines whole):\n{block}",
                path.display()
            ),
            Some(PasteFile { path, text: block }),
        ),
        None => (format!("{instruction}."), None),
    }
}

/// Writes this run's blocks into `dir`, the `.json` files an earlier run
/// left there removed first, so the folder never holds a block doctor no
/// longer prints. `dir` is under the Officina home, never under a Claude
/// Code configuration. A warning when that fails: the printed blocks still
/// hold everything.
pub fn write_paste_files(dir: &Path, files: &[PasteFile]) -> Option<Check> {
    let written = (|| -> std::io::Result<()> {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|x| x == "json") {
                    fs::remove_file(entry.path())?;
                }
            }
        }
        if files.is_empty() {
            return Ok(());
        }
        fs::create_dir_all(dir)?;
        for file in files {
            fs::write(&file.path, format!("{}\n", file.text))?;
        }
        Ok(())
    })();
    written.err().map(|e| {
        check(
            Level::Warn,
            "paste files",
            format!(
                "cannot write the blocks to {}: {e}; copy them from this output",
                dir.display()
            ),
        )
    })
}

/// What of `registrations` the hooks in `settings` leave out: an event none
/// of whose matcher groups runs `officina hook` (a command whose first word
/// is the `officina` binary and whose second is `hook`), or, for an event
/// with tools, the tools no such group's matcher names, written
/// `Event (Tool, Tool)` when only some are left out. For a registration that
/// fails closed, an `officina hook` entry of that event without
/// [`ON_FAILURE`] is reported too, as `Event fail-closed (onFailure
/// "block")` (D-452). Other hooks in the same file are ignored (E2: hooks
/// merge).
pub fn missing_hook_events(
    settings: &serde_json::Value,
    registrations: &[Registration],
) -> Vec<String> {
    registrations
        .iter()
        .flat_map(|registration| {
            let ours: Vec<&serde_json::Value> = settings
                .get("hooks")
                .and_then(|h| h.get(registration.event))
                .and_then(|m| m.as_array())
                .into_iter()
                .flatten()
                .filter(|group| officina_entries(group).next().is_some())
                .collect();
            let fails_open = registration.fails_closed
                && ours
                    .iter()
                    .flat_map(|group| officina_entries(group))
                    .any(|entry| {
                        entry.get(ON_FAILURE.0).and_then(|v| v.as_str()) != Some(ON_FAILURE.1)
                    });
            let fail_open_entry = fails_open.then(|| {
                format!(
                    "{} fail-closed ({}: \"{}\")",
                    registration.event, ON_FAILURE.0, ON_FAILURE.1
                )
            });
            [missing_registration(registration, &ours), fail_open_entry]
                .into_iter()
                .flatten()
        })
        .collect()
}

/// The entries of a matcher group that run `officina hook`.
fn officina_entries(group: &serde_json::Value) -> impl Iterator<Item = &serde_json::Value> {
    group
        .get("hooks")
        .and_then(|h| h.as_array())
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry
                .get("command")
                .and_then(|c| c.as_str())
                .is_some_and(runs_officina_hook)
        })
}

/// What [`missing_hook_events`] reports for `registration` whose groups
/// running `officina hook` are `ours`: the event, or the tools left out.
fn missing_registration(
    registration: &Registration,
    ours: &[&serde_json::Value],
) -> Option<String> {
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

/// True when a stage of the pipeline `command` runs `officina quota write`
/// (the `officina` binary, then `quota`, then `write`), whatever its options.
pub fn runs_officina_quota_write(command: &str) -> bool {
    command.split('|').any(|stage| {
        let Some(words) = command_words(stage) else {
            return false;
        };
        let [program, quota, write, ..] = words.as_slice() else {
            return false;
        };
        let file_name = program.rsplit(['/', '\\']).next().unwrap_or_default();
        (file_name.eq_ignore_ascii_case("officina")
            || file_name.eq_ignore_ascii_case("officina.exe"))
            && quota == "quota"
            && write == "write"
    })
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

/// Runs every check. `exe` is the path of the running `officina` binary.
pub fn run(collector: &Collector, exe: &Path) -> Vec<Check> {
    let now = now_ms();
    let paths = collector.paths();
    let user_settings = paths.claude_config_dir.join("settings.json");
    let relay = RelayHome::new(paths.officina_home.clone(), env_path(ENV_FLOOR));
    let gate = gate_required(&relay);
    let required = registrations(gate);
    let (mut checks, version) = check_claude(collector);
    let doctor_dir = paths.doctor_dir();
    let (hooks, hooks_file) = check_hooks(&user_settings, exe, &required, &doctor_dir);
    checks.push(hooks);
    if let Some(version) = version.as_deref().filter(|_| gate) {
        checks.push(check_gate_failure_mode(version));
    }
    checks.extend(check_hook_activity(&paths.events_dir(), now));
    let (quota, quota_files) = check_quota(collector, &user_settings, exe, now, &doctor_dir);
    checks.push(quota);
    let files: Vec<PasteFile> = hooks_file.into_iter().chain(quota_files).collect();
    checks.extend(write_paste_files(&doctor_dir, &files));
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

/// What a `claude` that is not a native executable costs (AD-94 part A,
/// FR-15, D-562): every use Officina has for it is refused, and the attach
/// command can still be copied. Replaces v0.3's shim line (D-436).
pub fn refused_claude_line(claude: &Path) -> String {
    format!(
        "claude resolves to {}, which is not a native claude.exe, and Officina runs no other: launches, answers, reviews, the orchestrator, attach, the board's refresh and the triage call are refused; a background session's attach command can still be copied; install the native claude.exe or set OFFICINA_CLAUDE_BIN to it",
        claude.display()
    )
}

/// `claude --version`, then `claude agents --json` against the contract;
/// the version, when it was read, for the hook fields of the registry
/// section. A `claude` Officina refuses to run fails with one line.
fn check_claude(collector: &Collector) -> (Vec<Check>, Option<String>) {
    let Some(bin) = collector.resolved_claude() else {
        return (
            vec![check(
                Level::Fail,
                "claude",
                SpawnError::NotFound.to_string(),
            )],
            None,
        );
    };
    if !bin.is_native() {
        return (
            vec![check(
                Level::Fail,
                "claude",
                refused_claude_line(bin.path()),
            )],
            None,
        );
    }
    let config_dir = collector.paths().claude_config_override.as_deref();
    let base = collector.base_env();
    match spawn::version(bin, config_dir, base) {
        Ok(v) => (
            vec![
                check(
                    Level::Ok,
                    "claude",
                    format!("{v} at {}", bin.path().display()),
                ),
                check_agents_output(bin, config_dir, base),
            ],
            Some(v),
        ),
        Err(e) => (
            vec![
                check(Level::Fail, "claude", e.to_string()),
                check_agents_output(bin, config_dir, base),
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

fn check_agents_output(bin: &ResolvedClaude, config_dir: Option<&Path>, base: &BaseEnv) -> Check {
    const TITLE: &str = "claude agents --json";
    let out = match spawn::agents_json(bin, config_dir, base) {
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

/// Every required registration in the user settings, else the file's whole
/// `hooks` key with them merged in, to paste over it (D-530), and the file
/// under `doctor_dir` that holds it (D-534).
fn check_hooks(
    user_settings: &Path,
    exe: &Path,
    required: &[Registration],
    doctor_dir: &Path,
) -> (Check, Option<PasteFile>) {
    let settings = SettingsFile::read(user_settings);
    let current = settings.get("hooks");
    let missing = missing_hook_events(&serde_json::json!({ "hooks": current }), required);
    if missing.is_empty() {
        let ok = check(
            Level::Ok,
            "hooks",
            format!("registered in {}", user_settings.display()),
        );
        return (ok, None);
    }
    let (instruction, paste) = match merged_hooks(current, exe, required) {
        Ok(hooks) => key_paste(user_settings, &settings, "hooks", &hooks),
        Err(why) => (
            format!(
                "{} cannot be merged: {why}; fix it and run doctor again, no block is printed",
                user_settings.display()
            ),
            None,
        ),
    };
    let kept = if current.is_some() && paste.is_some() {
        " (every hook already there is kept in it)"
    } else {
        ""
    };
    let file = paste.as_ref().map(|block| PasteFile {
        path: paste_file_path(doctor_dir, "user", "hooks"),
        text: block.clone(),
    });
    let never_edits = match &file {
        Some(file) => format!(
            "; Officina never edits the file. The block is also in {}, its lines whole:",
            file.path.display()
        ),
        None => ".".to_owned(),
    };
    let line = Check {
        level: Level::Fail,
        title: "hooks".into(),
        detail: format!(
            "not registered for {} in {}. {instruction}{kept}{never_edits}",
            missing.join(", "),
            user_settings.display()
        ),
        paste,
    };
    (line, file)
}

/// Whether the installed Claude Code, which printed `version`, honours the
/// gate entry's [`ON_FAILURE`] (D-452). Older, the entry is not honoured and
/// a gate hook that cannot start, times out or crashes lets the call
/// through: a failure, as a hooks block without the field is. A version
/// doctor cannot read warns.
pub fn check_gate_failure_mode(version: &str) -> Check {
    const TITLE: &str = "gate failure mode";
    let [major, minor, patch] = ON_FAILURE_SINCE;
    let since = format!("{major}.{minor}.{patch}");
    match claude_version(version) {
        Some(found) if found >= ON_FAILURE_SINCE => check(
            Level::Ok,
            TITLE,
            format!(
                "{version} blocks a gate call when the hook fails ({}: \"{}\", {since} or later)",
                ON_FAILURE.0, ON_FAILURE.1
            ),
        ),
        Some(_) => check(
            Level::Fail,
            TITLE,
            format!(
                "{version} predates {since}, which added {}: \"{}\": a gate hook that cannot start, times out or crashes lets the call through. Update Claude Code",
                ON_FAILURE.0, ON_FAILURE.1
            ),
        ),
        None => check(
            Level::Warn,
            TITLE,
            format!(
                "cannot read a version in \"{version}\": {}: \"{}\" needs Claude Code {since} or later",
                ON_FAILURE.0, ON_FAILURE.1
            ),
        ),
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

/// The command of a settings file's `statusLine`, if it sets one.
fn statusline_command(status_line: &serde_json::Value) -> Option<&str> {
    status_line.get("command")?.as_str()
}

/// A project folder whose own settings set a statusline that does not run
/// Officina's `quota write`: in a session opened there it wins over the
/// user's (Claude Code's settings precedence), so the quota file is not
/// written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatuslineOverride {
    pub folder: PathBuf,
    /// The settings file that sets it: `settings.local.json` wins over
    /// `settings.json`.
    pub file: PathBuf,
    /// Its whole `statusLine` value.
    pub status_line: serde_json::Value,
}

impl StatuslineOverride {
    /// The folder's personal settings file, where the wiring goes: never
    /// the committed `settings.json`.
    pub fn local_settings(&self) -> PathBuf {
        self.folder.join(".claude").join("settings.local.json")
    }
}

/// The overrides among `folders`: each folder's `.claude/settings.local.json`
/// statusline, else its `.claude/settings.json` one, when it does not run
/// Officina's `quota write`. Read-only.
pub fn statusline_overrides(folders: &[PathBuf]) -> Vec<StatuslineOverride> {
    folders
        .iter()
        .filter_map(|folder| {
            let claude = folder.join(".claude");
            ["settings.local.json", "settings.json"]
                .into_iter()
                .map(|name| claude.join(name))
                .find_map(|file| {
                    let status_line = SettingsFile::read(&file)
                        .get("statusLine")
                        .filter(|value| statusline_command(value).is_some())
                        .cloned()?;
                    Some((file, status_line))
                })
                .filter(|(_, status_line)| {
                    !statusline_command(status_line).is_some_and(runs_officina_quota_write)
                })
                .map(|(file, status_line)| StatuslineOverride {
                    folder: folder.clone(),
                    file,
                    status_line,
                })
        })
        .collect()
}

/// The folders of the sessions the hook saw recently, from the `cwd` of the
/// event buffer's lines, existing folders only, sorted.
fn recent_session_folders(events_dir: &Path, now: i64) -> Vec<PathBuf> {
    let (events, _) = read_recent_events(events_dir, now);
    let folders: std::collections::BTreeSet<PathBuf> = events
        .iter()
        .filter_map(|e| e.cwd.as_deref())
        .map(PathBuf::from)
        .filter(|folder| folder.is_dir())
        .collect();
    folders.into_iter().collect()
}

/// A fresh quota file for this floor's account; else why it is stale and,
/// for each settings file to change, its whole `statusLine` value wired to
/// paste over it (D-530). A user statusline that already runs Officina's
/// `quota write` is in place: the cause is then a project's own statusline,
/// which wins in sessions opened in its folder, so the folders of the
/// sessions the hook saw are checked for one, and each gets its value for
/// its `settings.local.json`. Each block is also in a file under
/// `doctor_dir` (D-534).
fn check_quota(
    collector: &Collector,
    user_settings: &Path,
    exe: &Path,
    now: i64,
    doctor_dir: &Path,
) -> (Check, Vec<PasteFile>) {
    let found = collector.read_quota();
    if let Some(q) = found
        .as_ref()
        .filter(|q| now - q.written_at * 1000 <= STALE_AFTER_MS)
    {
        let ok = check(
            Level::Ok,
            "quota",
            format!(
                "account {} updated {} min ago",
                q.account_key,
                (now / 1000 - q.written_at) / 60
            ),
        );
        return (ok, Vec::new());
    }
    let state = format!(
        "{} for account {}",
        if found.is_some() { "stale" } else { "no data" },
        collector.paths().account_key()
    );
    let user = SettingsFile::read(user_settings);
    let user_wired = user
        .get("statusLine")
        .and_then(statusline_command)
        .is_some_and(runs_officina_quota_write);
    let overrides = statusline_overrides(&recent_session_folders(
        &collector.paths().events_dir(),
        now,
    ));
    let user_section = || {
        saved_section(
            key_paste(
                user_settings,
                &user,
                "statusLine",
                &statusline_value(exe, user.get("statusLine")),
            ),
            paste_file_path(doctor_dir, "user", "statusLine"),
        )
    };
    let (folder_texts, folder_files): (Vec<String>, Vec<Option<PasteFile>>) = overrides
        .iter()
        .map(|o| {
            let local = o.local_settings();
            saved_section(
                key_paste(
                    &local,
                    &SettingsFile::read(&local),
                    "statusLine",
                    &statusline_value(exe, Some(&o.status_line)),
                ),
                paste_file_path(doctor_dir, &project_target(&o.folder), "statusLine"),
            )
        })
        .unzip();
    let per_folder = folder_texts.join("\n");
    let mut files: Vec<PasteFile> = Vec::new();
    let folders = overrides
        .iter()
        .map(|o| o.file.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let (detail, paste) = match (user_wired, overrides.is_empty()) {
        (true, true) => (
            format!(
                "{state}. The user statusline already runs officina quota write and no folder of a recent session sets its own: the file is written while a session shows its statusline, so open one, and check that the statusline's officina is this one ({})",
                quoted_exe(exe)
            ),
            None,
        ),
        (true, false) => {
            files.extend(folder_files.into_iter().flatten());
            (
                format!(
                    "{state}. The user statusline already runs officina quota write, but these project settings set their own statusline, which wins in sessions opened there: {folders}. Wire it in each folder's .claude/settings.local.json, never a committed file (Officina never edits them):"
                ),
                Some(per_folder),
            )
        }
        (false, true) => {
            let (text, file) = user_section();
            files.extend(file);
            (
                format!(
                    "{state}. Wire officina quota write into the user statusline (Officina never edits the file):"
                ),
                Some(text),
            )
        }
        (false, false) => {
            let (text, file) = user_section();
            files.extend(file);
            files.extend(folder_files.into_iter().flatten());
            (
                format!(
                    "{state}. Wire officina quota write into the user statusline and, in the folders whose project settings set their own ({folders}), into their .claude/settings.local.json, never a committed file (Officina never edits them):"
                ),
                Some(format!("{text}\n{per_folder}")),
            )
        }
    };
    let line = Check {
        level: Level::Warn,
        title: "quota".into(),
        detail,
        paste,
    };
    (line, files)
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
                check(
                    Level::Ok,
                    "registry copies",
                    format!(
                        "{} daily copies, the newest from {} ({age} days old)",
                        copies.len(),
                        utc_day_text(day_start_ms(newest.day))
                    ),
                )
            }
        });
    }
    let waiting = fs::read_dir(events_dir_of(officina_home))
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
        checks.push(check(
            if recent { Level::Warn } else { Level::Ok },
            "registry restore",
            format!(
                "restored from {} on {}: author actions taken after that copy are lost, events were ingested again",
                notice.copy,
                utc_day_text(notice.restored_at_ms)
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
/// the refusal every call meets while `claude` resolves to anything but a
/// native executable (AD-51, D-562). Read-only; the registry is never
/// created.
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
                "claude resolves to {}, which is not a native claude.exe: every triage call is refused before it starts; install the native claude.exe or set OFFICINA_CLAUDE_BIN to it",
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
        (
            Level::Warn,
            format!(
                "paused since {}: its last {PAUSE_AFTER_FAILED_CALLS} proposal calls failed in a row; turn it on again in the Triage panel to lift the pause, or a new claude lifts it",
                utc_day_text(since)
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
                "; today {}; last {} days {}",
                counted(&view.today),
                COUNT_WINDOW_MS / DAY_MS,
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
        detail.push_str(&format!(
            "; latest dry run {}: {} cases, model right {}, default right {}",
            utc_day_text(latest.at_ms),
            latest.tally.cases,
            latest.tally.model_correct,
            latest.tally.default_correct
        ));
    }
    checks.push(check(level, "triage model", detail));
    checks
}

/// The floor file, if any, loaded without error.
fn check_floor(collector: &Collector) -> Check {
    let view_issues = || {
        collector
            .floor()
            .view_issues
            .iter()
            .map(|issue| format!("line {}: `{}`: {}", issue.line, issue.key, issue.message))
            .collect::<Vec<_>>()
            .join("; ")
    };
    if let Some(mut warning) = no_excerpts_unseen_by_the_hook(collector) {
        if !collector.floor().view_issues.is_empty() {
            warning.detail = format!("{}; {}", warning.detail, view_issues());
        }
        return warning;
    }
    match (&collector.floor().path, &collector.floor().error) {
        (None, _) => check(
            Level::Ok,
            "floor",
            "none: projects are derived from each session's folder",
        ),
        (Some(p), None) if collector.floor().view_issues.is_empty() => check(
            Level::Ok,
            "floor",
            format!(
                "{} ({} projects)",
                p.display(),
                collector.floor().projects.len()
            ),
        ),
        // A view key with a bad value keeps its default: the board works,
        // and the author is told where to look (AD-168).
        (Some(p), None) => check(
            Level::Warn,
            "floor",
            format!(
                "{} ({} projects); {}",
                p.display(),
                collector.floor().projects.len(),
                view_issues()
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

    /// The whole settings file doctor prints when the user settings do not
    /// exist.
    fn hooks_block(exe: &Path, registrations: &[Registration]) -> String {
        serde_json::to_string_pretty(&serde_json::json!({
            "hooks": merged_hooks(None, exe, registrations).unwrap()
        }))
        .unwrap()
    }

    /// Does what the author does with one block doctor printed for `file`:
    /// writes the whole file, or sets the one key it holds.
    fn apply_paste(file: &Path, block: &str) {
        if block.starts_with('{') {
            fs::write(file, block).unwrap();
            return;
        }
        let SettingsFile::Object(mut settings) = SettingsFile::read(file) else {
            panic!("a key block for a file that is not an object");
        };
        let (key, value) = block.split_once(": ").unwrap();
        settings.insert(
            serde_json::from_str(key).unwrap(),
            serde_json::from_str(value).unwrap(),
        );
        fs::write(file, serde_json::Value::Object(settings).to_string()).unwrap();
    }

    /// The groups of `event` in `hooks` whose entries run `officina hook`,
    /// counted per entry.
    fn officina_entry_count(hooks: &serde_json::Value, event: &str) -> usize {
        hooks[event]
            .as_array()
            .into_iter()
            .flatten()
            .map(|group| officina_entries(group).count())
            .sum()
    }

    fn foreign_hooks() -> serde_json::Value {
        serde_json::json!({
            "PostToolUse": [{"matcher": "Edit|Write", "hooks": [
                {"type": "command", "command": "npx prettier --write", "timeout": 30, "onFailure": "block"}]}],
            "Stop": [{"hooks": [{"type": "command", "command": "python notify.py"}]}],
            GATE_EVENT: [{"matcher": "Bash", "hooks": [
                {"type": "command", "command": "python guard.py", "onFailure": "block"}]}]
        })
    }

    /// D-530: every foreign event, group, command, matcher and `onFailure`
    /// is in the merged value as it was, in its place, and Officina's
    /// registrations come after them, the gate's next to a foreign
    /// `PreToolUse` group; the result registers everything.
    #[test]
    fn merged_hooks_keep_every_foreign_entry() {
        let exe = PathBuf::from("/opt/officina");
        let foreign = foreign_hooks();
        for gate in [false, true] {
            let required = registrations(gate);
            let merged = merged_hooks(Some(&foreign), &exe, &required).unwrap();
            for (event, groups) in foreign.as_object().unwrap() {
                let groups = groups.as_array().unwrap();
                assert_eq!(
                    &merged[event].as_array().unwrap()[..groups.len()],
                    groups.as_slice(),
                    "{event}"
                );
            }
            assert!(
                missing_hook_events(&serde_json::json!({ "hooks": &merged }), &required).is_empty(),
                "{merged:#}"
            );
            for registration in &required {
                assert_eq!(
                    officina_entry_count(&merged, registration.event),
                    1,
                    "{}",
                    registration.event
                );
            }
            if gate {
                let pre_tool_use = merged[GATE_EVENT].as_array().unwrap();
                assert_eq!(pre_tool_use.len(), 2, "{merged:#}");
                assert_eq!(pre_tool_use[1]["matcher"], gated_tools_matcher());
                assert_eq!(pre_tool_use[1]["hooks"][0][ON_FAILURE.0], ON_FAILURE.1);
            }
        }
    }

    /// D-530: a merged value merges to itself, and doctor on the file that
    /// holds it reports the hooks registered and prints nothing.
    #[test]
    fn merging_twice_changes_nothing_and_doctor_then_prints_nothing() {
        let exe = PathBuf::from("/opt/officina");
        let required = registrations(true);
        let once = merged_hooks(Some(&foreign_hooks()), &exe, &required).unwrap();
        assert_eq!(merged_hooks(Some(&once), &exe, &required).unwrap(), once);

        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        fs::write(
            &file,
            serde_json::json!({"model": "opus", "hooks": foreign_hooks()}).to_string(),
        )
        .unwrap();
        let line = check_hooks(&file, &exe, &required, Path::new("doctor")).0;
        assert_eq!(line.level, Level::Fail);
        assert!(
            line.detail.contains(&format!(
                "Replace the whole \"hooks\" key of {}",
                file.display()
            )),
            "{}",
            line.detail
        );
        apply_paste(&file, &line.paste.unwrap());
        let line = check_hooks(&file, &exe, &required, Path::new("doctor")).0;
        assert_eq!((line.level, line.paste), (Level::Ok, None));
        let SettingsFile::Object(settings) = SettingsFile::read(&file) else {
            panic!("the pasted file is an object");
        };
        assert_eq!(settings["model"], "opus");
    }

    /// D-530: Officina's own gate entry with a partial matcher or without
    /// `onFailure`, in a group it shares with a foreign hook, is replaced:
    /// the foreign entry stays in its group with its matcher, and the gate
    /// runs once. An Officina-only group it empties goes.
    #[test]
    fn a_partial_gate_entry_is_replaced_never_doubled() {
        let exe = PathBuf::from("/opt/officina");
        let gate = [gate_registration()];
        let guard = serde_json::json!({"type": "command", "command": "python guard.py"});
        let hooks = serde_json::json!({GATE_EVENT: [
            {"matcher": "Bash", "hooks": [guard.clone(), {"type": "command", "command": "officina.exe hook"}]},
            {"matcher": gated_tools_matcher(), "hooks": [{"type": "command", "command": "officina.exe hook"}]}
        ]});
        let merged = merged_hooks(Some(&hooks), &exe, &gate).unwrap();
        assert_eq!(
            merged[GATE_EVENT],
            serde_json::json!([
                {"matcher": "Bash", "hooks": [guard]},
                registration_group(&exe, &gate[0])
            ])
        );
    }

    /// D-530: the printed block never holds another key of the settings
    /// file, whose values can be environment values or tokens.
    #[test]
    fn no_other_key_of_a_settings_file_is_printed() {
        const SECRET: &str = "sk-not-a-real-token";
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            officina_home: dir.path().join("officina"),
            claude_config_dir: dir.path().join("config"),
            claude_config_override: None,
            floor_file: None,
        };
        fs::create_dir_all(&paths.claude_config_dir).unwrap();
        let user = paths.claude_config_dir.join("settings.json");
        let others = serde_json::json!({
            "env": {"ANTHROPIC_API_KEY": SECRET},
            "permissions": {"allow": [format!("Bash(curl -H {SECRET})")]},
            "apiKeyHelper": format!("echo {SECRET}")
        });
        let mut settings = others.clone();
        settings["statusLine"] =
            serde_json::json!({"type": "command", "command": "python s.py", "padding": 1});
        settings["hooks"] = foreign_hooks();
        fs::write(&user, settings.to_string()).unwrap();
        let repo = session_folder(
            &paths,
            dir.path(),
            "repo",
            &[("settings.json", "python p.py")],
        );
        fs::write(
            repo.join(".claude").join("settings.local.json"),
            others.to_string(),
        )
        .unwrap();
        let exe = PathBuf::from("/opt/officina");
        let collector = Collector::from_parts(paths.clone(), LoadedFloor::default(), None);
        let hooks = check_hooks(&user, &exe, &registrations(true), Path::new("doctor")).0;
        let quota = check_quota(&collector, &user, &exe, now_ms(), Path::new("doctor")).0;
        for line in [&hooks, &quota] {
            let text = format!(
                "{}\n{}",
                line.detail,
                line.paste.as_deref().unwrap_or_default()
            );
            for key in others.as_object().unwrap().keys() {
                assert!(!text.contains(&format!("\"{key}\"")), "{key}: {text}");
            }
            assert!(!text.contains(SECRET), "{text}");
        }
        let paste = quota.paste.unwrap();
        assert!(paste.contains("\"padding\": 1"), "{paste}");
        assert!(
            paste.contains(&format!(
                "Add this \"statusLine\" key to {}",
                repo.join(".claude").join("settings.local.json").display()
            )),
            "{paste}"
        );
    }

    /// D-530: a settings file doctor cannot read gets no merged block, only
    /// the reason; nor does a `hooks` key of the wrong shape.
    #[test]
    fn an_unreadable_settings_file_gets_no_block() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let exe = PathBuf::from("/opt/officina");
        for text in [
            "{\"hooks\": {",
            "[]",
            "{\"hooks\": []}",
            "{\"hooks\": {\"Stop\": {}}}",
        ] {
            fs::write(&file, text).unwrap();
            let line = check_hooks(&file, &exe, &registrations(false), Path::new("doctor")).0;
            assert_eq!((line.level, &line.paste), (Level::Fail, &None), "{text}");
            assert!(
                line.detail.contains("no block is printed"),
                "{text}: {}",
                line.detail
            );
            let (instruction, block) = key_paste(
                &file,
                &SettingsFile::read(&file),
                "statusLine",
                &statusline_value(&exe, None),
            );
            assert_eq!(
                block.is_none(),
                matches!(SettingsFile::read(&file), SettingsFile::Unreadable(_)),
                "{text}: {instruction}"
            );
        }
    }

    /// D-530: a file that does not exist gets its whole content, the key
    /// alone; one that exists gets the key to add or to replace.
    #[test]
    fn the_block_is_the_whole_file_only_when_there_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.local.json");
        let value = serde_json::json!({"type": "command", "command": "x"});
        let paste = || key_paste(&file, &SettingsFile::read(&file), "statusLine", &value);
        let (instruction, block) = paste();
        assert!(instruction.starts_with("Create "), "{instruction}");
        let whole: serde_json::Value = serde_json::from_str(&block.unwrap()).unwrap();
        assert_eq!(whole, serde_json::json!({ "statusLine": value }));

        fs::write(&file, "\u{feff}{\"model\": \"opus\"}").unwrap();
        let (instruction, block) = paste();
        assert!(
            instruction.starts_with("Add this \"statusLine\" key"),
            "{instruction}"
        );
        assert!(block.unwrap().starts_with("\"statusLine\": {"));

        fs::write(&file, "{\"statusLine\": {}}").unwrap();
        let (instruction, block) = paste();
        assert!(
            instruction.starts_with("Replace the whole \"statusLine\" key"),
            "{instruction}"
        );
        apply_paste(&file, &block.unwrap());
        assert_eq!(SettingsFile::read(&file).get("statusLine"), Some(&value));
    }

    /// Story 5.2 AC 12 (AD-94 part A, D-562): a `claude` that is not a
    /// native executable fails doctor with one line naming every refused
    /// use and the attach command's copy; nothing is started for it.
    #[test]
    fn a_non_exe_claude_fails_with_every_refused_use() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            officina_home: dir.path().join("officina"),
            claude_config_dir: dir.path().join("config"),
            claude_config_override: None,
            floor_file: None,
        };
        let ran = dir.path().join("ran.txt");
        for name in ["claude.cmd", "claude.bat", "claude.com"] {
            let shim = dir.path().join(name);
            std::fs::write(&shim, format!("@echo ran> \"{}\"\r\n", ran.display())).unwrap();
            let collector =
                Collector::from_parts(paths.clone(), LoadedFloor::default(), Some(shim.clone()));
            let (checks, version) = check_claude(&collector);
            assert_eq!(version, None);
            let [line] = checks.as_slice() else {
                panic!("{name}: one line, {checks:?}");
            };
            assert_eq!((line.title.as_str(), line.level), ("claude", Level::Fail));
            assert_eq!(line.detail, refused_claude_line(&shim));
            for refused in [
                "launches",
                "answers",
                "reviews",
                "the orchestrator",
                "attach",
                "the board's refresh",
                "the triage call",
                "attach command can still be copied",
            ] {
                assert!(line.detail.contains(refused), "{name}: {refused}");
            }
        }
        assert!(!ran.exists(), "a shim ran");
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
        let collector = Collector::from_parts(paths.clone(), floor(elsewhere.clone()), None);
        let line = check_floor(&collector);
        assert_eq!(line.level, Level::Warn, "{line:?}");
        assert!(line.detail.contains("OFFICINA_FLOOR"), "{line:?}");
        // A view key's bad value is reported beside the excerpts warning,
        // never hidden by it (AD-168).
        let with_issue = LoadedFloor {
            view_issues: vec![officina_core::floor::ViewIssue {
                key: "priority",
                line: 7,
                message: "a bad value".into(),
            }],
            ..floor(elsewhere.clone())
        };
        let line = check_floor(&Collector::from_parts(paths.clone(), with_issue, None));
        assert_eq!(line.level, Level::Warn, "{line:?}");
        assert!(line.detail.contains("OFFICINA_FLOOR"), "{line:?}");
        assert!(line.detail.contains("line 7: `priority`"), "{line:?}");
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

    /// AD-178 gate 1, D-518: the sub-agent events are buffered, so the
    /// hooks block registers them and their payloads reach the buffer.
    #[test]
    fn subagent_events_are_buffered() {
        for event in officina_core::events::SUBAGENT_EVENTS {
            assert!(BUFFERED_EVENTS.contains(&event), "{event}");
            assert!(
                registrations(false).iter().any(|r| r.event == event),
                "{event}"
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
            let mut group = serde_json::json!({"hooks": [
                {"type": "command", "command": "officina.exe hook", "onFailure": "block"}
            ]});
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
            {"matcher": gated_tools_matcher(), "hooks": [{"type": "command", "command": "officina hook", "onFailure": "block"}]}
        ]}});
        assert!(missing_hook_events(&other_hook_beside, &gate).is_empty());
    }

    /// Story 2.17 AC 1, D-452: the printed gate entry fails closed, and only
    /// the gate's; the buffered events keep failing open.
    #[test]
    fn only_the_gate_entry_fails_closed() {
        let required = registrations(true);
        let block: serde_json::Value =
            serde_json::from_str(&hooks_block(&PathBuf::from("/opt/officina"), &required)).unwrap();
        for registration in &required {
            let entry = &block["hooks"][registration.event][0]["hooks"][0];
            let expected = registration.fails_closed.then_some(ON_FAILURE.1);
            assert_eq!(
                entry[ON_FAILURE.0].as_str(),
                expected,
                "{}",
                registration.event
            );
            assert_eq!(registration.fails_closed, registration.event == GATE_EVENT);
        }
    }

    /// Story 2.17 AC 2, D-452: an installed gate entry without
    /// `onFailure: "block"`, or with another value, is reported, also when a
    /// second group carries it, and the printed block clears it; the field
    /// on another tool's hook or on a buffered event changes nothing.
    #[test]
    fn a_gate_entry_that_fails_open_is_reported() {
        let gate = [gate_registration()];
        let fail_closed = format!("{GATE_EVENT} fail-closed (onFailure: \"block\")");
        let entry = |on_failure: Option<&str>| {
            let mut entry = serde_json::json!({"type": "command", "command": "officina.exe hook"});
            if let Some(value) = on_failure {
                entry[ON_FAILURE.0] = value.into();
            }
            entry
        };
        let settings =
            |groups: Vec<serde_json::Value>| serde_json::json!({"hooks": {GATE_EVENT: groups}});
        let group = |entries: Vec<serde_json::Value>| serde_json::json!({"matcher": gated_tools_matcher(), "hooks": entries});
        for open in [None, Some("continue"), Some("Block"), Some("")] {
            assert_eq!(
                missing_hook_events(&settings(vec![group(vec![entry(open)])]), &gate),
                std::slice::from_ref(&fail_closed),
                "{open:?}"
            );
        }
        let one_of_two = settings(vec![
            group(vec![entry(Some("block"))]),
            group(vec![entry(None)]),
        ]);
        assert_eq!(
            missing_hook_events(&one_of_two, &gate),
            std::slice::from_ref(&fail_closed)
        );
        let foreign_guard = serde_json::json!({"type": "command", "command": "python guard.py"});
        let beside = settings(vec![group(vec![foreign_guard, entry(Some("block"))])]);
        assert!(missing_hook_events(&beside, &gate).is_empty());

        let names = gated_tools();
        let partial = settings(vec![
            serde_json::json!({"matcher": names[0], "hooks": [entry(None)]}),
        ]);
        assert_eq!(
            missing_hook_events(&partial, &gate),
            [
                format!("{GATE_EVENT} ({})", names[1..].join(", ")),
                fail_closed.clone()
            ]
        );

        let buffered = registrations(false);
        let without: serde_json::Value =
            serde_json::from_str(&hooks_block(&PathBuf::from("/opt/officina"), &buffered)).unwrap();
        assert!(missing_hook_events(&without, &buffered).is_empty());

        let dir = tempfile::tempdir().unwrap();
        let settings_file = dir.path().join("settings.json");
        fs::write(
            &settings_file,
            settings(vec![group(vec![entry(None)])]).to_string(),
        )
        .unwrap();
        let exe = PathBuf::from("/opt/officina");
        let line = check_hooks(&settings_file, &exe, &gate, Path::new("doctor")).0;
        assert_eq!(line.level, Level::Fail, "{line:?}");
        assert!(line.detail.contains(&fail_closed), "{line:?}");
        apply_paste(&settings_file, &line.paste.unwrap());
        assert_eq!(
            check_hooks(&settings_file, &exe, &gate, Path::new("doctor"))
                .0
                .level,
            Level::Ok
        );
    }

    /// Story 2.17 AC 3, D-452: a Claude Code older than the first version
    /// honouring `onFailure` fails, that version and later pass, a version
    /// doctor cannot read warns.
    #[test]
    fn a_claude_code_without_on_failure_fails() {
        let [major, minor, patch] = ON_FAILURE_SINCE;
        let level = |version: String| check_gate_failure_mode(&version).level;
        assert_eq!(
            level(format!("{major}.{minor}.{patch} (Claude Code)")),
            Level::Ok
        );
        assert_eq!(
            level(format!("{major}.{minor}.{} (Claude Code)", patch + 17)),
            Level::Ok
        );
        assert_eq!(
            level(format!("{major}.{}.0 (Claude Code)", minor + 1)),
            Level::Ok
        );
        assert_eq!(
            level(format!("{major}.{minor}.{} (Claude Code)", patch - 1)),
            Level::Fail
        );
        assert_eq!(
            level(format!("{major}.{}.999 (Claude Code)", minor - 1)),
            Level::Fail
        );
        assert_eq!(level("Claude Code".to_owned()), Level::Warn);
        let old = check_gate_failure_mode("2.1.294 (Claude Code)");
        assert!(old.detail.contains("lets the call through"), "{old:?}");
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
        let group = |matcher: &str| serde_json::json!({"matcher": matcher, "hooks": [{"type": "command", "command": "officina.exe hook", "onFailure": "block"}]});
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

    /// A registry of this build's schema at `version` under `home`,
    /// written without the window's writer, which this test build may not
    /// hold.
    fn registry_at(home: &Path, version: i64) -> rusqlite::Connection {
        let paths = RegistryPaths::new(home);
        fs::create_dir_all(paths.dir()).unwrap();
        let conn = rusqlite::Connection::open(paths.database()).unwrap();
        for migration in crate::registry::schema::MIGRATIONS {
            conn.execute_batch(migration).unwrap();
        }
        conn.pragma_update(None, "user_version", version).unwrap();
        conn
    }

    fn registry_line<'a>(checks: &'a [Check], title: &str) -> &'a Check {
        checks
            .iter()
            .find(|c| c.title == title)
            .unwrap_or_else(|| panic!("no {title} line in {checks:?}"))
    }

    /// Story 3.9 AC 11 (AD-51, AD-53): a `claude` that is not a native
    /// executable is reported as refusing every triage call; a native one
    /// is not.
    #[test]
    fn cmd_shim_is_reported_as_refusing_triage() {
        let dir = tempfile::tempdir().unwrap();
        for shim in ["claude.cmd", "claude.Bat", "claude.com"] {
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

    /// D-530: the statusline value tees into the existing command and keeps
    /// the existing value's other fields.
    #[test]
    fn statusline_value_tees_into_the_existing_command() {
        let exe = PathBuf::from("/opt/officina");
        let existing =
            serde_json::json!({"type": "command", "command": "python statusline.py", "padding": 2});
        assert_eq!(
            statusline_value(&exe, Some(&existing)),
            serde_json::json!({"type": "command", "command": "\"/opt/officina\" quota write --tee | python statusline.py", "padding": 2})
        );
        for none in [None, Some(serde_json::json!({"command": " "}))] {
            assert_eq!(
                statusline_value(&exe, none.as_ref()),
                serde_json::json!({"type": "command", "command": "\"/opt/officina\" quota write"})
            );
        }
    }

    /// A recent session's folder, its hook line in the buffer, and its
    /// project settings.
    fn session_folder(
        paths: &Paths,
        root: &Path,
        name: &str,
        settings: &[(&str, &str)],
    ) -> PathBuf {
        let folder = root.join(name);
        std::fs::create_dir_all(folder.join(".claude")).unwrap();
        for (file, command) in settings {
            std::fs::write(
                folder.join(".claude").join(file),
                serde_json::json!({"statusLine": {"type": "command", "command": command}})
                    .to_string(),
            )
            .unwrap();
        }
        let line = serde_json::json!({"v": 1, "ts": now_ms(), "event": "UserPromptSubmit",
            "session_id": name, "cwd": folder.to_string_lossy()});
        std::fs::create_dir_all(paths.events_dir()).unwrap();
        append_line(
            &event_file_for(&paths.events_dir(), now_ms()),
            &format!("{line}\n"),
        )
        .unwrap();
        folder
    }

    /// The owner's case: the user statusline runs Officina, but a project's
    /// own wins in sessions opened there; doctor names the folder and gives
    /// it the snippet teeing into its command, and ignores a folder whose
    /// local settings already run Officina.
    #[test]
    fn a_stale_quota_names_the_project_statuslines_that_win() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            officina_home: dir.path().join("officina"),
            claude_config_dir: dir.path().join("config"),
            claude_config_override: None,
            floor_file: None,
        };
        std::fs::create_dir_all(&paths.claude_config_dir).unwrap();
        let exe = PathBuf::from("C:/Program Files/Officina/officina.exe");
        let user = paths.claude_config_dir.join("settings.json");
        std::fs::write(
            &user,
            serde_json::json!({"statusLine": {"command": format!("{} quota write", quoted_exe(&exe))}}).to_string(),
        )
        .unwrap();
        let repo = session_folder(
            &paths,
            dir.path(),
            "repo",
            &[(
                "settings.json",
                "python \"$CLAUDE_PROJECT_DIR/.claude/statusline.py\"",
            )],
        );
        session_folder(
            &paths,
            dir.path(),
            "wired",
            &[
                ("settings.json", "python s.py"),
                (
                    "settings.local.json",
                    "officina quota write --tee | python s.py",
                ),
            ],
        );
        session_folder(&paths, dir.path(), "plain", &[]);
        let collector = Collector::from_parts(paths.clone(), LoadedFloor::default(), None);
        let line = check_quota(&collector, &user, &exe, now_ms(), Path::new("doctor")).0;
        assert_eq!(line.level, Level::Warn);
        assert!(
            line.detail.contains("already runs officina quota write"),
            "{}",
            line.detail
        );
        assert!(
            line.detail.contains(
                &repo
                    .join(".claude")
                    .join("settings.json")
                    .display()
                    .to_string()
            ),
            "{}",
            line.detail
        );
        assert!(!line.detail.contains("wired"), "{}", line.detail);
        let paste = line.paste.expect("a snippet for the folder");
        assert!(
            paste.contains(
                &repo
                    .join(".claude")
                    .join("settings.local.json")
                    .display()
                    .to_string()
            ),
            "{paste}"
        );
        assert!(paste.contains("quota write --tee | python"), "{paste}");
        assert!(
            paste.contains(&format!(
                "Create {} with this content",
                repo.join(".claude").join("settings.local.json").display()
            )),
            "{paste}"
        );
        assert!(
            !paste.contains("quota write --tee | \\\"C:/Program Files"),
            "{paste}"
        );

        // No override: the user wiring is in place, nothing to paste.
        let quiet = tempfile::tempdir().unwrap();
        let paths = Paths {
            officina_home: quiet.path().join("officina"),
            ..paths
        };
        let collector = Collector::from_parts(paths, LoadedFloor::default(), None);
        let line = check_quota(&collector, &user, &exe, now_ms(), Path::new("doctor")).0;
        assert!(line.paste.is_none(), "{line:?}");
        assert!(
            line.detail.contains("no folder of a recent session"),
            "{}",
            line.detail
        );
    }

    /// An existing statusline that already runs Officina's `quota write` is
    /// kept as it is, never teed into it again.
    #[test]
    fn statusline_value_never_tees_into_officina_itself() {
        let exe = PathBuf::from("C:/Program Files/Officina/officina.exe");
        for existing in [
            "\"C:/Program Files/Officina/officina.exe\" quota write",
            "\"C:/Program Files/Officina/officina.exe\" quota write --tee | python s.py",
            "/opt/officina quota write",
        ] {
            let current = serde_json::json!({"type": "command", "command": existing});
            assert_eq!(statusline_value(&exe, Some(&current)), current);
        }
    }

    #[test]
    fn officina_quota_write_is_recognised_in_a_pipeline() {
        for runs in [
            "\"C:/x/officina.exe\" quota write",
            "officina quota write --tee | python s.py",
            "python s.py | /opt/OFFICINA quota write",
        ] {
            assert!(runs_officina_quota_write(runs), "{runs}");
        }
        for other in [
            "python statusline.py",
            "officina quota",
            "officina-app quota write",
            "\"unterminated quota write",
            "officina doctor | quota write",
        ] {
            assert!(!runs_officina_quota_write(other), "{other}");
        }
    }

    /// Every file under `dir` with its bytes, sorted.
    fn files_under(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(next) = stack.pop() {
            for entry in fs::read_dir(&next).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let bytes = fs::read(&path).unwrap();
                    out.push((path, bytes));
                }
            }
        }
        out.sort();
        out
    }

    /// D-534: each block doctor prints is also written, whole, to a file
    /// under the Officina home that its line names: the hooks key as
    /// printed, the user statusline, and a project's complete
    /// `settings.local.json` when it has none. Nothing is written under
    /// the Claude config dir or in the project folder.
    #[test]
    fn each_block_is_written_whole_to_the_file_its_line_names() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            officina_home: dir.path().join("officina"),
            claude_config_dir: dir.path().join("config"),
            claude_config_override: None,
            floor_file: None,
        };
        fs::create_dir_all(&paths.claude_config_dir).unwrap();
        let user = paths.claude_config_dir.join("settings.json");
        fs::write(
            &user,
            serde_json::json!({"statusLine": {"type": "command", "command": "python s.py"}, "hooks": foreign_hooks()})
                .to_string(),
        )
        .unwrap();
        let repo = session_folder(
            &paths,
            dir.path(),
            "repo",
            &[("settings.json", "python p.py")],
        );
        let config_before = files_under(&paths.claude_config_dir);
        let repo_before = files_under(&repo);
        let exe = PathBuf::from("C:/Program Files/Officina/officina.exe");
        let collector = Collector::from_parts(paths.clone(), LoadedFloor::default(), None);
        let checks = run(&collector, &exe);
        let line = |title: &str| checks.iter().find(|c| c.title == title).unwrap();
        let doctor_dir = paths.doctor_dir();

        let hooks_file = paste_file_path(&doctor_dir, "user", "hooks");
        let hooks = line("hooks");
        assert!(
            hooks.detail.contains(&hooks_file.display().to_string()),
            "{}",
            hooks.detail
        );
        assert_eq!(
            fs::read_to_string(&hooks_file).unwrap(),
            format!("{}\n", hooks.paste.as_deref().unwrap())
        );

        let quota = line("quota");
        let paste = quota.paste.as_deref().unwrap();
        let user_file = paste_file_path(&doctor_dir, "user", "statusLine");
        let repo_file = paste_file_path(&doctor_dir, &project_target(&repo), "statusLine");
        for file in [&user_file, &repo_file] {
            assert!(paste.contains(&file.display().to_string()), "{paste}");
        }
        let user_block = fs::read_to_string(&user_file).unwrap();
        assert!(user_block.starts_with("\"statusLine\": {"), "{user_block}");
        assert!(paste.contains(user_block.trim_end()), "{paste}");
        let whole: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&repo_file).unwrap()).unwrap();
        assert_eq!(
            whole.as_object().unwrap().keys().collect::<Vec<_>>(),
            ["statusLine"]
        );
        assert!(
            whole["statusLine"]["command"]
                .as_str()
                .unwrap()
                .ends_with("| python p.py")
        );

        assert_eq!(files_under(&paths.claude_config_dir), config_before);
        assert_eq!(files_under(&repo), repo_before);

        // A run that prints no block leaves no file behind; other files stay.
        fs::write(doctor_dir.join("notes.txt"), "mine").unwrap();
        assert_eq!(write_paste_files(&doctor_dir, &[]), None);
        let left: Vec<PathBuf> = files_under(&doctor_dir)
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        assert_eq!(left, [doctor_dir.join("notes.txt")]);
    }

    /// D-534: two project folders of one name never share a file, and a
    /// folder name is kept to characters every file system takes.
    #[test]
    fn project_files_are_named_after_the_folder_and_its_path() {
        let a = project_target(Path::new("C:/work/a/repo"));
        let b = project_target(Path::new("C:/work/b/repo"));
        assert_ne!(a, b);
        for target in [&a, &b] {
            assert!(target.starts_with("project-repo-"), "{target}");
        }
        let odd = project_target(Path::new("C:/work/my repo: v2*"));
        assert!(odd.starts_with("project-my_repo__v2_-"), "{odd}");
    }

    /// D-534: a folder doctor cannot write warns, and the run goes on.
    #[test]
    fn an_unwritable_paste_folder_warns() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("doctor");
        fs::write(&blocker, "a file where the folder goes").unwrap();
        let file = PasteFile {
            path: paste_file_path(&blocker, "user", "hooks"),
            text: "\"hooks\": {}".to_owned(),
        };
        let warning = write_paste_files(&blocker, &[file]).unwrap();
        assert_eq!(
            (warning.title.as_str(), warning.level),
            ("paste files", Level::Warn)
        );
    }
}
