//! The contract canary (FR-12, AD-12, AD-56): the fields Officina reads from
//! `claude agents --json`, checked against a live sample, and the hook
//! payload fields it reads, which the hook reports missing line by line.

use std::collections::{BTreeMap, BTreeSet};

use crate::events::EventLine;

/// The payload fields Officina reads from each event the hook buffers, each
/// verified always present for that event: research notes E2 (a print-mode
/// capture) for every event but `Notification`, E17 (a background session
/// in `default` mode asked to use Write) for `Notification` (D-90), which is
/// how a later Claude Code version re-checks that entry. A field no research
/// note shows observed does not enter. `hook_event_name` is not listed: a
/// payload without it is no hook call and is dropped.
pub const HOOK_FIELDS: &[(&str, &[&str])] = &[
    ("SessionStart", &["session_id", "cwd", "source"]),
    ("UserPromptSubmit", &["session_id", "cwd", "prompt"]),
    ("Stop", &["session_id", "cwd", "last_assistant_message"]),
    ("SessionEnd", &["session_id", "cwd", "reason"]),
    (
        "Notification",
        &["session_id", "cwd", "notification_type", "message"],
    ),
    // From the hooks reference (AD-176), not yet from a research note:
    // Slice 0's gate 1 records them observed on the workstation, and the
    // canary reports a field a version leaves out meanwhile.
    ("SubagentStart", &["session_id", "agent_id", "agent_type"]),
    ("SubagentStop", &["session_id", "agent_id", "agent_type"]),
];

/// The [`HOOK_FIELDS`] entry of `event`; none for an event it does not name.
///
/// ```
/// use officina_core::contract::hook_fields;
/// assert!(hook_fields("Stop").contains(&"last_assistant_message"));
/// assert!(hook_fields("PreToolUse").is_empty());
/// ```
pub fn hook_fields(event: &str) -> &'static [&'static str] {
    HOOK_FIELDS
        .iter()
        .find(|(name, _)| *name == event)
        .map_or(&[], |(_, fields)| fields)
}

/// The fields of `event`'s [`HOOK_FIELDS`] entry that `payload` lacks, in
/// the entry's order (AD-45): one lookup per field, nothing more parsed.
pub fn absent_hook_fields(event: &str, payload: &serde_json::Value) -> Vec<String> {
    hook_fields(event)
        .iter()
        .filter(|field| payload.get(**field).is_none())
        .map(|field| (*field).to_owned())
        .collect()
}

/// The first Claude Code version that honours a hook entry's `onFailure`
/// (hooks reference, changelog 2.1.295; D-452): before it, a gate hook that
/// cannot start, times out or exits with a code other than 0 or 2 lets the
/// tool call through.
pub const ON_FAILURE_SINCE: [u32; 3] = [2, 1, 295];

/// The `major.minor.patch` that `claude --version` prints as its first word
/// (`2.1.312 (Claude Code)`), or `None` when that word is not one.
///
/// ```
/// use officina_core::contract::{ON_FAILURE_SINCE, claude_version};
/// assert_eq!(claude_version("2.1.312 (Claude Code)"), Some([2, 1, 312]));
/// assert!(claude_version("2.1.294 (Claude Code)").unwrap() < ON_FAILURE_SINCE);
/// assert_eq!(claude_version("Claude Code"), None);
/// ```
pub fn claude_version(text: &str) -> Option<[u32; 3]> {
    let word = text.split_whitespace().next()?;
    let mut parts = word.split('.').map(|part| part.parse::<u32>().ok());
    let version = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some(version)
}

/// What a run of event lines says about the hook payloads (AD-56): when
/// each event was last seen, and when each `Event.field` was last reported
/// absent. The window folds every line it ingests into one, then into the
/// running version's record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HookSightings {
    /// The latest `ts` of each event seen.
    pub events: BTreeMap<String, i64>,
    /// The latest `ts` at which each `Event.field` was absent.
    pub absent: BTreeMap<String, i64>,
}

impl HookSightings {
    /// Folds one event line in.
    pub fn observe(&mut self, line: &EventLine) {
        let latest = |seen: &mut BTreeMap<String, i64>, key: String| {
            let at = seen.entry(key).or_insert(line.ts);
            *at = (*at).max(line.ts);
        };
        latest(&mut self.events, line.event.clone());
        for field in &line.absent {
            latest(&mut self.absent, format!("{}.{field}", line.event));
        }
    }

    /// Folds another run of sightings in.
    pub fn merge(&mut self, other: &HookSightings) {
        for (seen, theirs) in [
            (&mut self.events, &other.events),
            (&mut self.absent, &other.absent),
        ] {
            for (key, at) in theirs {
                let mine = seen.entry(key.clone()).or_insert(*at);
                *mine = (*mine).max(*at);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty() && self.absent.is_empty()
    }
}

/// A version's `hook_missing` once `sightings` are folded in (AD-56, D-71):
/// what it held, plus every `Event.field` reported absent at or after the
/// version's first sighting; an older line describes another version, so an
/// upgrade does not inherit the previous version's gaps. Sorted, each once.
pub fn hook_missing_after(
    held: &[String],
    sightings: &HookSightings,
    first_seen_at: i64,
) -> Vec<String> {
    let mut missing: BTreeSet<String> = held.iter().cloned().collect();
    missing.extend(
        sightings
            .absent
            .iter()
            .filter(|(_, at)| **at >= first_seen_at)
            .map(|(field, _)| field.clone()),
    );
    missing.into_iter().collect()
}

/// A version's `hook_not_observed` once `sightings` are folded in: the
/// events it held, less those seen at or after the version's first
/// sighting.
pub fn hook_not_observed_after(
    held: &[String],
    sightings: &HookSightings,
    first_seen_at: i64,
) -> Vec<String> {
    held.iter()
        .filter(|event| {
            sightings
                .events
                .get(*event)
                .is_none_or(|at| *at < first_seen_at)
        })
        .cloned()
        .collect()
}

/// Fields every entry carries (research notes E1).
pub const AGENT_FIELDS_ALWAYS: [&str; 4] = ["sessionId", "cwd", "kind", "startedAt"];

/// Fields present only in some states: `pid` and `status` while the process
/// lives, `id` and `state` on background sessions, `waitingFor` while waiting.
pub const AGENT_FIELDS_CONDITIONAL: [&str; 5] = ["pid", "status", "id", "state", "waitingFor"];

/// Result of a contract check.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractCheck {
    /// Always-present fields absent from at least one entry: the contract broke.
    pub missing: Vec<String>,
    /// Conditional fields never seen in this sample: not a break, just not
    /// exercised yet (no session was waiting, for instance).
    pub not_observed: Vec<String>,
}

impl ContractCheck {
    /// True when no always-present field is missing.
    pub fn held(&self) -> bool {
        self.missing.is_empty()
    }
}

/// Checks a raw `claude agents --json` array. An empty sample proves nothing
/// is missing and observes nothing.
///
/// ```
/// let sample: Vec<serde_json::Value> = serde_json::from_str(
///     r#"[{"sessionId":"s","cwd":"c","kind":"interactive","startedAt":1,"pid":2,"status":"busy"}]"#,
/// ).unwrap();
/// let check = officina_core::contract::check_agents(&sample);
/// assert!(check.held());
/// assert!(check.not_observed.contains(&"waitingFor".to_string()));
/// ```
pub fn check_agents(entries: &[serde_json::Value]) -> ContractCheck {
    let mut missing = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for entry in entries {
        for field in AGENT_FIELDS_ALWAYS {
            if entry.get(field).is_none() {
                missing.insert(field.to_owned());
            }
        }
        for field in AGENT_FIELDS_CONDITIONAL {
            if entry.get(field).is_some() {
                seen.insert(field);
            }
        }
    }
    ContractCheck {
        missing: missing.into_iter().collect(),
        not_observed: AGENT_FIELDS_CONDITIONAL
            .iter()
            .filter(|f| !seen.contains(*f))
            .map(|f| (*f).to_owned())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Story 2.17 AC 3: only a plain `major.minor.patch` first word is a
    /// version; anything else is unknown, never old or new.
    #[test]
    fn a_version_is_the_first_word_in_three_numbers() {
        assert_eq!(claude_version("2.1.295"), Some(ON_FAILURE_SINCE));
        assert_eq!(claude_version("  10.0.1 (Claude Code)"), Some([10, 0, 1]));
        for unknown in [
            "",
            "2.1",
            "2.1.312.4",
            "v2.1.312",
            "2.1.x",
            "2..312",
            "-1.2.3",
        ] {
            assert_eq!(claude_version(unknown), None, "{unknown:?}");
        }
        assert!(claude_version("2.1.312").unwrap() > ON_FAILURE_SINCE);
        assert!(claude_version("2.0.999").unwrap() < ON_FAILURE_SINCE);
    }

    #[test]
    fn a_vanished_field_breaks_the_contract() {
        let check = check_agents(&[
            json!({"sessionId": "a", "cwd": "c", "kind": "background", "startedAt": 1, "id": "x", "state": "done"}),
            json!({"cwd": "c", "kind": "interactive", "startedAt": 1}),
        ]);
        assert!(!check.held());
        assert_eq!(check.missing, vec!["sessionId"]);
        assert_eq!(check.not_observed, vec!["pid", "status", "waitingFor"]);
    }

    #[test]
    fn empty_sample_holds_and_observes_nothing() {
        let check = check_agents(&[]);
        assert!(check.held());
        assert_eq!(check.not_observed.len(), AGENT_FIELDS_CONDITIONAL.len());
    }

    fn line(event: &str, ts: i64, absent: &[&str]) -> EventLine {
        let mut line: EventLine =
            serde_json::from_value(json!({"v": 1, "ts": ts, "event": event})).unwrap();
        line.absent = absent.iter().map(|f| (*f).to_owned()).collect();
        line
    }

    /// D-71: only what follows the version's first sighting counts, a
    /// missing field stays missing, and an event seen once since then is
    /// observed.
    #[test]
    fn sightings_after_the_first_sighting_update_the_record() {
        let (event, fields) = HOOK_FIELDS[0];
        let mut seen = HookSightings::default();
        seen.observe(&line(event, 100, &[fields[0]]));
        seen.observe(&line(event, 300, &[]));
        let later = HookSightings {
            absent: [(format!("{event}.{}", fields[1]), 400)].into(),
            ..HookSightings::default()
        };
        seen.merge(&later);
        assert_eq!(seen.events[event], 300);
        let held = vec!["Already.gone".to_owned()];
        assert_eq!(
            hook_missing_after(&held, &seen, 200),
            ["Already.gone".to_owned(), format!("{event}.{}", fields[1])],
            "the absence at 100 belongs to the version before"
        );
        let every: Vec<String> = HOOK_FIELDS.iter().map(|(e, _)| (*e).to_owned()).collect();
        let left = hook_not_observed_after(&every, &seen, 200);
        assert!(!left.contains(&event.to_owned()));
        assert_eq!(left.len(), every.len() - 1);
        assert_eq!(hook_not_observed_after(&every, &seen, 301), every);
    }

    #[test]
    fn absent_fields_follow_the_entry_of_the_event() {
        for (event, fields) in HOOK_FIELDS {
            let complete: serde_json::Map<String, serde_json::Value> = fields
                .iter()
                .map(|f| ((*f).to_owned(), json!("x")))
                .collect();
            let complete = serde_json::Value::Object(complete);
            assert!(absent_hook_fields(event, &complete).is_empty(), "{event}");
            assert_eq!(
                absent_hook_fields(event, &json!({})),
                fields.iter().map(|f| (*f).to_owned()).collect::<Vec<_>>()
            );
        }
        assert!(absent_hook_fields("PreToolUse", &json!({})).is_empty());
    }
}
