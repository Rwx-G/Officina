//! The `PermissionRequest` router's files and output (AD-181, D-579,
//! D-580): the request `officina hook` writes while a decision only the
//! window writes is awaited, that decision, the window's liveness marker,
//! and the one output the hook may print. Pure: the host reads and writes
//! the files and keeps the clock.
//!
//! Every path but a window-written decision bound to the request ends in no
//! output, so Claude Code's own prompt appears: a fault anywhere is the
//! native prompt, never an approval. [`PromptOutput`] has no field through
//! which Claude Code could be asked to write its settings
//! (`updatedPermissions`), change the call (`updatedInput`) or stop
//! (`interrupt`).

use serde::{Deserialize, Serialize};

use crate::untrusted::Untrusted;

/// The hook event the router answers (hooks reference, `PermissionRequest`).
pub const PROMPT_EVENT: &str = "PermissionRequest";

/// `relay/prompts/`: requests and decisions (AD-181), beside AD-33's
/// `relay/approvals/`.
pub const PROMPTS_DIR: &str = "prompts";

/// The window's liveness marker under the Officina home (D-579):
/// `window/alive.json`.
pub const WINDOW_DIR: &str = "window";
/// The marker's file name in [`WINDOW_DIR`].
pub const WINDOW_MARKER_FILE: &str = "alive.json";

/// How often the window refreshes its marker's modification time (D-579).
pub const MARKER_REFRESH_MS: i64 = 5_000;
/// A marker older than this reads "no window" (D-579): three refreshes
/// missed.
pub const MARKER_STALE_MS: i64 = 3 * MARKER_REFRESH_MS;

/// The timeout the routing story registers the hook with (AD-181). Not
/// registered by this story (D-580); the wait below stays inside it, so the
/// hook ends by itself before Claude Code cancels it.
pub const PROMPT_HOOK_TIMEOUT_S: i64 = 590;
/// How long the hook waits for a decision (AD-181).
pub const PROMPT_WAIT_MS: i64 = 580_000;
const _: () = assert!(PROMPT_WAIT_MS < PROMPT_HOOK_TIMEOUT_S * 1_000);

/// How often the waiting hook looks for a decision (AD-33's stat).
pub const PROMPT_POLL_MS: u64 = 100;
/// How often the waiting hook reads the window's marker again (D-579).
pub const MARKER_RECHECK_MS: i64 = 1_000;

/// Most bytes of the canonical `tool_input` a request keeps (AD-181).
pub const INPUT_CAP_BYTES: usize = 4 * 1024;
/// Most characters of a tool name a request or a journal line keeps.
pub const TOOL_NAME_CHARS: usize = 128;
/// Most characters of a session id a request keeps: a UUID fits.
pub const SESSION_ID_CHARS: usize = 64;

/// How long request and decision files are kept before the window's
/// cleanup deletes them (AD-33's cleanup).
pub const PROMPT_FILE_TTL_MS: i64 = 24 * 60 * 60 * 1_000;

/// The one message a deny carries (AD-181): constant, so no request text
/// ever reaches Claude through it.
pub const DENY_MESSAGE: &str = "The author denied this in the Officina window.";

crate::closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What the window decided for a routed prompt: the two behaviours the
    /// hook may print.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum PromptBehavior {
        Allow,
        Deny,
    }
}

crate::closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// What the author may answer a routed prompt (AD-181): allow once,
    /// deny, or decline, which writes no decision and sends the request
    /// back to Claude Code's own prompt at once.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum PromptAnswer {
        Allow,
        Deny,
        Decline,
    }
}

impl PromptAnswer {
    /// The behaviour the hook prints for this answer; `None` for a
    /// decline, which ends the wait with no output.
    pub fn behavior(self) -> Option<PromptBehavior> {
        match self {
            PromptAnswer::Allow => Some(PromptBehavior::Allow),
            PromptAnswer::Deny => Some(PromptBehavior::Deny),
            PromptAnswer::Decline => None,
        }
    }
}

/// The kinds of file a request leaves in [`PROMPTS_DIR`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptFileKind {
    /// `<id>.json`, written by the hook.
    Request,
    /// `<id>.decided.json`, written by the window's authority only.
    Decided,
    /// `<id>.declined`, written by the window's authority only.
    Declined,
    /// `<id>.consumed.json`: the decision the hook claimed (single use).
    Consumed,
}

impl PromptFileKind {
    fn suffix(self) -> &'static str {
        match self {
            PromptFileKind::Request => ".json",
            PromptFileKind::Decided => ".decided.json",
            PromptFileKind::Declined => ".declined",
            PromptFileKind::Consumed => ".consumed.json",
        }
    }
}

/// The name of `id`'s file of `kind`.
///
/// ```
/// use officina_core::relay::prompts::{PromptFileKind, prompt_file_name};
/// assert_eq!(prompt_file_name("0a-1", PromptFileKind::Decided), "0a-1.decided.json");
/// ```
pub fn prompt_file_name(id: &str, kind: PromptFileKind) -> String {
    format!("{id}{}", kind.suffix())
}

/// The id and kind a file name of [`PROMPTS_DIR`] stands for; `None` for
/// any other name, a temporary file's included.
///
/// ```
/// use officina_core::relay::prompts::{PromptFileKind, parse_prompt_file_name};
/// assert_eq!(parse_prompt_file_name("0a-1.json"), Some(("0a-1", PromptFileKind::Request)));
/// assert_eq!(parse_prompt_file_name("0a-1.declined"), Some(("0a-1", PromptFileKind::Declined)));
/// assert_eq!(parse_prompt_file_name("0a-1.json.42.tmp"), None);
/// ```
pub fn parse_prompt_file_name(name: &str) -> Option<(&str, PromptFileKind)> {
    [
        PromptFileKind::Request,
        PromptFileKind::Decided,
        PromptFileKind::Declined,
        PromptFileKind::Consumed,
    ]
    .into_iter()
    .find_map(|kind| {
        name.strip_suffix(kind.suffix())
            .filter(|id| is_prompt_id(id))
            .map(|id| (id, kind))
    })
}

/// True when `id` has the shape the hook gives ids: lowercase hex digits
/// and hyphens, 1 to 64 characters. Anything else (a separator, a dot) is
/// refused before it reaches a path.
///
/// ```
/// use officina_core::relay::prompts::is_prompt_id;
/// assert!(is_prompt_id("0123abcd-1793102400000"));
/// assert!(!is_prompt_id("../relay/approved"));
/// assert!(!is_prompt_id(""));
/// ```
pub fn is_prompt_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || b == b'-')
}

/// A request's id: the start of a hash over what makes it unique, then its
/// time, so two hooks never share one and the names sort by time.
///
/// ```
/// use officina_core::relay::prompts::{is_prompt_id, prompt_id};
/// let id = prompt_id("s", "00", 42, 1_793_102_400_000);
/// assert!(is_prompt_id(&id) && id.ends_with("-1793102400000"));
/// assert_ne!(id, prompt_id("s", "00", 43, 1_793_102_400_000));
/// ```
pub fn prompt_id(session_id: &str, input_sha256: &str, pid: u32, now_ms: i64) -> String {
    let unique = crate::sha256_hex(format!("{session_id}\n{input_sha256}\n{pid}").as_bytes());
    format!("{}-{now_ms}", &unique[..16])
}

/// `value` as canonical JSON: object keys sorted by their bytes at every
/// depth, no whitespace, so one input hashes the same whatever order or
/// spacing Claude Code wrote it in.
///
/// ```
/// use officina_core::relay::prompts::canonical_json;
/// let value = serde_json::json!({"b": [1, {"d": 2, "c": "x"}], "a": null});
/// assert_eq!(canonical_json(&value), r#"{"a":null,"b":[1,{"c":"x","d":2}]}"#);
/// ```
pub fn canonical_json(value: &serde_json::Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&scalar_text(&serde_json::Value::String(key.clone())));
                out.push(':');
                write_canonical(&map[key], out);
            }
            out.push('}');
        }
        serde_json::Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        scalar => out.push_str(&scalar_text(scalar)),
    }
}

fn scalar_text(value: &serde_json::Value) -> String {
    // A string, number, bool or null always serializes.
    serde_json::to_string(value).unwrap_or_default()
}

/// SHA-256 of the canonical `tool_input`, lowercase hex: what a decision is
/// bound to.
pub fn input_sha256(tool_input: &serde_json::Value) -> String {
    crate::sha256_hex(canonical_json(tool_input).as_bytes())
}

/// The first `max_bytes` bytes of `text`, cut back to a character
/// boundary.
///
/// ```
/// use officina_core::relay::prompts::cap_on_char_boundary;
/// assert_eq!(cap_on_char_boundary("aé", 2), "a");
/// assert_eq!(cap_on_char_boundary("aé", 3), "aé");
/// ```
pub fn cap_on_char_boundary(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// A `PermissionRequest` waiting for the window (AD-181), as
/// `relay/prompts/<id>.json` holds it. A field the payload does not carry
/// is absent (SD-5 may find one missing on Windows); a decision binds to
/// the session id and the input hash only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptRequest {
    pub id: String,
    pub session_id: String,
    /// `CLAUDE_PID` from the hook's environment (E14), when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// SHA-256 of the canonical `tool_input`, the whole of it.
    pub input_sha256: String,
    /// The canonical `tool_input` capped at [`INPUT_CAP_BYTES`]: shown as
    /// text only, never parsed into anything that runs.
    pub tool_input: Untrusted<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    pub requested_at_ms: i64,
    /// When the hook stops waiting; a decision after it is refused.
    pub deadline_ms: i64,
}

impl PromptRequest {
    /// The request a `PermissionRequest` payload makes at `now_ms`, waited
    /// for up to `wait_ms`; `None` without a session id, which nothing
    /// could be bound to. A missing `tool_input` is hashed as `null`.
    pub fn from_payload(
        payload: &serde_json::Value,
        claude_pid: Option<u32>,
        pid: u32,
        now_ms: i64,
        wait_ms: i64,
    ) -> Option<PromptRequest> {
        let text = |key: &str, chars: usize| {
            payload
                .get(key)
                .and_then(serde_json::Value::as_str)
                .filter(|v| !v.is_empty())
                .map(|v| v.chars().take(chars).collect::<String>())
        };
        let session_id = text("session_id", SESSION_ID_CHARS)?;
        let input = payload
            .get("tool_input")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let canonical = canonical_json(&input);
        let input_sha256 = crate::sha256_hex(canonical.as_bytes());
        Some(PromptRequest {
            id: prompt_id(&session_id, &input_sha256, pid, now_ms),
            session_id,
            claude_pid,
            tool_name: text("tool_name", TOOL_NAME_CHARS),
            input_sha256,
            tool_input: Untrusted::new(
                cap_on_char_boundary(&canonical, INPUT_CAP_BYTES).to_owned(),
            ),
            permission_mode: text("permission_mode", crate::events::PERMISSION_MODE_CHARS),
            requested_at_ms: now_ms,
            deadline_ms: now_ms.saturating_add(wait_ms),
        })
    }

    /// True when `at_ms` is past the request's deadline.
    pub fn expired(&self, at_ms: i64) -> bool {
        at_ms > self.deadline_ms
    }
}

/// The window's decision on one request (AD-181), as
/// `relay/prompts/<id>.decided.json` holds it: bound to the request's
/// session and input hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptDecision {
    pub id: String,
    pub session_id: String,
    pub input_sha256: String,
    pub behavior: PromptBehavior,
    pub decided_at_ms: i64,
}

impl PromptDecision {
    /// The decision `behavior` on `request`, at `now_ms`.
    pub fn on(request: &PromptRequest, behavior: PromptBehavior, now_ms: i64) -> PromptDecision {
        PromptDecision {
            id: request.id.clone(),
            session_id: request.session_id.clone(),
            input_sha256: request.input_sha256.clone(),
            behavior,
            decided_at_ms: now_ms,
        }
    }

    /// True when this decision answers `request`: the same id, session and
    /// input hash, made by its deadline and taken by it too.
    pub fn answers(&self, request: &PromptRequest, now_ms: i64) -> bool {
        self.id == request.id
            && self.session_id == request.session_id
            && self.input_sha256 == request.input_sha256
            && !request.expired(self.decided_at_ms)
            && !request.expired(now_ms)
    }
}

/// The only output the hook prints on `PermissionRequest` (hooks
/// reference, PermissionRequest decision control): `hookEventName` and
/// `decision.behavior`, plus the constant [`DENY_MESSAGE`] on a deny.
/// Built only by [`PromptOutput::of`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptOutput {
    hook_specific_output: HookSpecificOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct HookSpecificOutput {
    hook_event_name: &'static str,
    decision: OutputDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct OutputDecision {
    behavior: PromptBehavior,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'static str>,
}

impl PromptOutput {
    /// The output for `behavior`.
    ///
    /// ```
    /// use officina_core::relay::prompts::{PromptBehavior, PromptOutput};
    /// assert_eq!(
    ///     PromptOutput::of(PromptBehavior::Allow).to_json(),
    ///     r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}"#
    /// );
    /// ```
    pub fn of(behavior: PromptBehavior) -> PromptOutput {
        PromptOutput {
            hook_specific_output: HookSpecificOutput {
                hook_event_name: PROMPT_EVENT,
                decision: OutputDecision {
                    behavior,
                    message: (behavior == PromptBehavior::Deny).then_some(DENY_MESSAGE),
                },
            },
        }
    }

    /// The output as one line of JSON.
    pub fn to_json(&self) -> String {
        // Static strings and a unit enum always serialize.
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// The window's liveness marker (D-579), `window/alive.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowMarker {
    pub pid: u32,
    pub started_at_ms: i64,
    /// False until the routing story ships and the owner turns routing on
    /// (D-579, D-580).
    pub routes_prompts: bool,
}

/// True when a window that routes prompts is running (D-579): the marker
/// `content` parses, says it routes, and its modification time is within
/// [`MARKER_STALE_MS`] of `now_ms`, either way, so a clock set back does
/// not keep a dead marker fresh. A missing marker (`content` or
/// `modified_ms` absent) is no window.
pub fn window_routes(content: Option<&[u8]>, modified_ms: Option<i64>, now_ms: i64) -> bool {
    let (Some(content), Some(modified_ms)) = (content, modified_ms) else {
        return false;
    };
    let fresh = now_ms.abs_diff(modified_ms) <= MARKER_STALE_MS.unsigned_abs();
    fresh
        && serde_json::from_slice::<WindowMarker>(content).is_ok_and(|marker| marker.routes_prompts)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_793_102_400_000;

    fn marker(routes: bool) -> Vec<u8> {
        serde_json::to_vec(&WindowMarker {
            pid: 7,
            started_at_ms: NOW - 60_000,
            routes_prompts: routes,
        })
        .unwrap()
    }

    fn request() -> PromptRequest {
        PromptRequest::from_payload(
            &serde_json::json!({
                "session_id": "0f1e2d3c-4b5a-4978-8796-a5b4c3d2e1f0",
                "tool_name": "Bash",
                "tool_input": {"command": "rm -rf build", "description": "Clean"},
            }),
            None,
            42,
            NOW,
            PROMPT_WAIT_MS,
        )
        .unwrap()
    }

    /// Every property name the output holds, at every depth, as a dotted
    /// path.
    fn paths(value: &serde_json::Value, prefix: &str, out: &mut Vec<String>) {
        if let serde_json::Value::Object(map) = value {
            for (key, inner) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                out.push(path.clone());
                paths(inner, &path, out);
            }
        }
    }

    /// Story 5.22 AC 1 (AD-181, D-580): an allow holds the event name and
    /// the behaviour only; a deny adds the constant message; no output
    /// holds `updatedPermissions`, `updatedInput` or `interrupt` at any
    /// depth.
    #[test]
    fn the_output_holds_only_behavior_and_the_constant_message() {
        for behavior in PromptBehavior::ALL {
            let value: serde_json::Value =
                serde_json::from_str(&PromptOutput::of(*behavior).to_json()).unwrap();
            let decision = &value["hookSpecificOutput"]["decision"];
            assert_eq!(value["hookSpecificOutput"]["hookEventName"], PROMPT_EVENT);
            assert_eq!(decision["behavior"], behavior.code());
            match behavior {
                PromptBehavior::Allow => assert!(decision.get("message").is_none()),
                PromptBehavior::Deny => assert_eq!(decision["message"], DENY_MESSAGE),
            }
            let mut names = Vec::new();
            paths(&value, "", &mut names);
            for forbidden in ["updatedPermissions", "updatedInput", "interrupt"] {
                assert!(
                    !names.iter().any(|n| n.ends_with(forbidden)),
                    "{forbidden} in {names:?}"
                );
            }
        }
    }

    /// Story 5.22 AC 1: one input hashes the same whatever its key order
    /// and spacing; another value hashes otherwise.
    #[test]
    fn the_input_hash_is_canonical() {
        let a: serde_json::Value =
            serde_json::from_str(r#"{"command":"ls","options":{"b":1,"a":[true,null]}}"#).unwrap();
        let b: serde_json::Value = serde_json::from_str(
            "{ \"options\" : { \"a\" : [ true , null ], \"b\" : 1 },\n \"command\" : \"ls\" }",
        )
        .unwrap();
        assert_eq!(input_sha256(&a), input_sha256(&b));
        assert_eq!(
            canonical_json(&b),
            r#"{"command":"ls","options":{"a":[true,null],"b":1}}"#
        );
        let c: serde_json::Value = serde_json::from_str(r#"{"command":"ls -a"}"#).unwrap();
        assert_ne!(input_sha256(&a), input_sha256(&c));
        assert_eq!(
            input_sha256(&a),
            crate::sha256_hex(canonical_json(&a).as_bytes())
        );
    }

    /// Story 5.22 AC 1: the kept input stops at [`INPUT_CAP_BYTES`] on a
    /// character boundary, while the hash covers the whole input.
    #[test]
    fn the_input_is_capped_on_a_character_boundary() {
        // Each `é` is two bytes; the cap falls inside one.
        let command = format!("x{}", "é".repeat(INPUT_CAP_BYTES));
        let input = serde_json::json!({ "command": command });
        let request = PromptRequest::from_payload(
            &serde_json::json!({"session_id": "s", "tool_input": input}),
            None,
            1,
            NOW,
            PROMPT_WAIT_MS,
        )
        .unwrap();
        let kept = serde_json::to_value(&request).unwrap()["tool_input"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(kept.len() <= INPUT_CAP_BYTES && kept.len() > INPUT_CAP_BYTES - 4);
        assert!(canonical_json(&input).starts_with(&kept));
        assert_eq!(request.input_sha256, input_sha256(&input));
    }

    /// Story 5.22 AC 2 (D-579): a missing marker, one past
    /// [`MARKER_STALE_MS`] either way, one saying it does not route and one
    /// that does not parse all read "no window".
    #[test]
    fn a_missing_stale_or_non_routing_marker_is_no_window() {
        let routing = marker(true);
        assert!(!window_routes(None, None, NOW));
        assert!(!window_routes(Some(&routing), None, NOW));
        assert!(!window_routes(None, Some(NOW), NOW));
        assert!(!window_routes(
            Some(&routing),
            Some(NOW - MARKER_STALE_MS - 1),
            NOW
        ));
        assert!(!window_routes(
            Some(&routing),
            Some(NOW + MARKER_STALE_MS + 1),
            NOW
        ));
        assert!(!window_routes(Some(&marker(false)), Some(NOW), NOW));
        assert!(!window_routes(Some(b"{\"pid\":7"), Some(NOW), NOW));
    }

    /// Story 5.22 AC 2 (D-579): a routing marker refreshed within
    /// [`MARKER_STALE_MS`] is a window, at the bound included.
    #[test]
    fn a_fresh_routing_marker_is_a_window() {
        let routing = marker(true);
        assert!(window_routes(Some(&routing), Some(NOW), NOW));
        assert!(window_routes(
            Some(&routing),
            Some(NOW - MARKER_STALE_MS),
            NOW
        ));
        assert!(window_routes(
            Some(&routing),
            Some(NOW - MARKER_REFRESH_MS),
            NOW
        ));
    }

    /// AC 4's binding (AD-181): a decision answers its own request only,
    /// by its deadline.
    #[test]
    fn a_decision_answers_its_own_request_by_its_deadline() {
        let request = request();
        let decision = PromptDecision::on(&request, PromptBehavior::Allow, NOW + 1);
        assert!(decision.answers(&request, NOW + 2));
        assert!(!decision.answers(&request, request.deadline_ms + 1));
        let late = PromptDecision::on(&request, PromptBehavior::Allow, request.deadline_ms + 1);
        assert!(!late.answers(&request, request.deadline_ms));
        let mut other_session = decision.clone();
        other_session.session_id = "another".into();
        assert!(!other_session.answers(&request, NOW + 2));
        let mut other_hash = decision.clone();
        other_hash.input_sha256 = "00".repeat(32);
        assert!(!other_hash.answers(&request, NOW + 2));
        let mut other_id = decision;
        other_id.id = "0-1".into();
        assert!(!other_id.answers(&request, NOW + 2));
    }

    #[test]
    fn answers_map_to_behaviours_and_ids_are_safe_names() {
        assert_eq!(
            PromptAnswer::ALL
                .iter()
                .map(|a| a.behavior())
                .collect::<Vec<_>>(),
            [
                Some(PromptBehavior::Allow),
                Some(PromptBehavior::Deny),
                None
            ]
        );
        let request = request();
        assert!(is_prompt_id(&request.id));
        for bad in ["A0", "0/1", "0.json", "0\\1", &"0".repeat(65)] {
            assert!(!is_prompt_id(bad), "{bad}");
        }
    }
}
