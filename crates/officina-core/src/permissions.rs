//! Claude Code's permission modes as doctor reads and proposes them (story
//! 5.20, AD-180, D-490, D-573, D-574): the documented modes, the
//! sessions seen in each mode, and what the auto-mode block holds beyond
//! the author's own entries. Officina sets no mode: it counts, and prints
//! a block the author may paste.

use std::collections::{BTreeMap, BTreeSet};

/// The permission modes Claude Code documents (permission-modes page;
/// research notes E2 and E85). Doctor names any other value it sees as
/// unknown.
pub const PERMISSION_MODES: &[&str] = &[
    "default",
    "acceptEdits",
    "plan",
    "auto",
    "dontAsk",
    "bypassPermissions",
];

/// The mode the block switches `permissions.defaultMode` to.
pub const AUTO_MODE: &str = "auto";

/// The `autoMode.environment` doctor proposes when the user settings hold
/// no `autoMode` key: Claude Code's defaults, then two placeholders the
/// author replaces with his own trusted places (D-574). Never a real name.
pub const AUTO_MODE_ENVIRONMENT: &[&str] =
    &["$defaults", "<trusted repository>", "<lab host group>"];

/// Gate 8's addition to `permissions.allow` (D-573): research notes E85
/// recorded a pass in case a, so `officina run` needs no rule and none is
/// added.
pub const GATE_8_ALLOW_RULES: &[&str] = &[];

/// Gate 8's addition to `autoMode.allow` (D-573): none, case c was not
/// needed (research notes E85).
pub const GATE_8_AUTO_MODE_ALLOW: &[&str] = &[];

/// The keys of `permissions` whose value Claude Code reads as a list of
/// strings (settings page); doctor refuses to merge into any other shape.
const PERMISSIONS_LISTS: &[&str] = &["allow", "deny", "ask", "additionalDirectories"];

/// The keys of `autoMode` whose value is a list of strings (auto-mode
/// configuration page).
const AUTO_MODE_LISTS: &[&str] = &["environment", "allow", "soft_deny", "hard_deny"];

/// `current` as an object whose `lists` hold strings only, else why not.
fn checked_object(
    key: &str,
    current: Option<&serde_json::Value>,
    lists: &[&str],
) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let map = match current {
        None => return Ok(serde_json::Map::new()),
        Some(serde_json::Value::Object(map)) => map.clone(),
        Some(_) => return Err(format!("its \"{key}\" key is not an object")),
    };
    for list in lists {
        match map.get(*list) {
            None => {}
            Some(serde_json::Value::Array(items)) if items.iter().all(|i| i.is_string()) => {}
            Some(_) => return Err(format!("its \"{key}.{list}\" is not a list of strings")),
        }
    }
    Ok(map)
}

/// Appends `added` to the list `list` of `map`, an entry already there
/// kept once, after the author's own entries.
fn append_entries(
    map: &mut serde_json::Map<String, serde_json::Value>,
    list: &str,
    added: &[&str],
) {
    if added.is_empty() {
        return;
    }
    let entries = map
        .entry(list)
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    if let serde_json::Value::Array(items) = entries {
        for entry in added {
            if !items.iter().any(|item| item == entry) {
                items.push((*entry).into());
            }
        }
    }
}

/// The whole `permissions` key the author may paste to switch to auto mode
/// (D-574): `current` with every entry kept, `defaultMode` set to
/// [`AUTO_MODE`] and gate 8's rules added; an error naming the part that
/// is not the documented shape.
///
/// ```
/// use officina_core::permissions::permissions_block;
/// let current = serde_json::json!({"allow": ["Bash(git status)"], "defaultMode": "bypassPermissions"});
/// let block = permissions_block(Some(&current)).unwrap();
/// assert_eq!(block, serde_json::json!({"allow": ["Bash(git status)"], "defaultMode": "auto"}));
/// assert!(permissions_block(Some(&serde_json::json!(["x"]))).is_err());
/// ```
pub fn permissions_block(current: Option<&serde_json::Value>) -> Result<serde_json::Value, String> {
    let mut map = checked_object("permissions", current, PERMISSIONS_LISTS)?;
    match map.get("defaultMode") {
        None | Some(serde_json::Value::String(_)) => {}
        Some(_) => return Err("its \"permissions.defaultMode\" is not a string".to_owned()),
    }
    map.insert("defaultMode".to_owned(), AUTO_MODE.into());
    append_entries(&mut map, "allow", GATE_8_ALLOW_RULES);
    Ok(serde_json::Value::Object(map))
}

/// The whole `autoMode` key the author may paste (D-574): `current` kept
/// whole, gate 8's sentences added; without one, an `environment` of
/// [`AUTO_MODE_ENVIRONMENT`]. True in the second place when the
/// placeholders were proposed, so the line above the block explains them.
///
/// ```
/// use officina_core::permissions::{AUTO_MODE_ENVIRONMENT, auto_mode_block};
/// let (block, proposed) = auto_mode_block(None).unwrap();
/// assert!(proposed);
/// assert_eq!(block["environment"], serde_json::json!(AUTO_MODE_ENVIRONMENT));
/// ```
pub fn auto_mode_block(
    current: Option<&serde_json::Value>,
) -> Result<(serde_json::Value, bool), String> {
    let mut map = checked_object("autoMode", current, AUTO_MODE_LISTS)?;
    let proposed = current.is_none();
    if proposed {
        map.insert(
            "environment".to_owned(),
            serde_json::json!(AUTO_MODE_ENVIRONMENT),
        );
    }
    append_entries(&mut map, "allow", GATE_8_AUTO_MODE_ALLOW);
    Ok((serde_json::Value::Object(map), proposed))
}

/// The kind a session ran as, for the permission line: from the registry's
/// session record, unknown without one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SeenKind {
    Interactive,
    Background,
    Unknown,
}

impl SeenKind {
    pub fn text(self) -> &'static str {
        match self {
            SeenKind::Interactive => "interactive",
            SeenKind::Background => "background",
            SeenKind::Unknown => "unknown",
        }
    }
}

/// Doctor's "permission mode" line (D-574): for each mode seen in `lines`
/// (session id, mode), the number of distinct sessions per kind, a mode
/// outside [`PERMISSION_MODES`] named as unknown; `None` when no line
/// carries a mode. A session seen in two modes counts in each.
///
/// ```
/// use officina_core::permissions::{SeenKind, permission_line};
/// let line = permission_line(
///     &[("s1", "bypassPermissions"), ("s1", "bypassPermissions"), ("s2", "auto")],
///     |session| if session == "s1" { SeenKind::Interactive } else { SeenKind::Unknown },
/// );
/// assert_eq!(
///     line.as_deref(),
///     Some("auto 1 session (1 unknown); bypassPermissions 1 session (1 interactive)")
/// );
/// ```
pub fn permission_line(
    lines: &[(&str, &str)],
    kind_of: impl Fn(&str) -> SeenKind,
) -> Option<String> {
    let mut sessions: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (session, mode) in lines {
        sessions.entry(mode).or_default().insert(session);
    }
    if sessions.is_empty() {
        return None;
    }
    let mut modes: Vec<&str> = sessions.keys().copied().collect();
    // The documented modes in the page's order, then the others by name.
    modes.sort_by_key(|mode| {
        PERMISSION_MODES
            .iter()
            .position(|known| known == mode)
            .unwrap_or(PERMISSION_MODES.len())
    });
    let parts: Vec<String> = modes
        .iter()
        .map(|mode| {
            let ids = &sessions[mode];
            let mut kinds: BTreeMap<SeenKind, usize> = BTreeMap::new();
            for id in ids {
                *kinds.entry(kind_of(id)).or_default() += 1;
            }
            let split: Vec<String> = kinds
                .iter()
                .map(|(kind, n)| format!("{n} {}", kind.text()))
                .collect();
            let name = if PERMISSION_MODES.contains(mode) {
                (*mode).to_owned()
            } else {
                format!("unknown mode {mode:?}")
            };
            format!(
                "{name} {} {} ({})",
                ids.len(),
                if ids.len() == 1 {
                    "session"
                } else {
                    "sessions"
                },
                split.join(", ")
            )
        })
        .collect();
    Some(parts.join("; "))
}
