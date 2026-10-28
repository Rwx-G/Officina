//! Hook events: the payload Claude Code hands to `officina hook`, reduced to
//! the fields the board needs, and the JSON Lines format of the event buffer.

use serde::{Deserialize, Serialize};

use crate::contract::absent_hook_fields;

/// Version of the event buffer line format. Bumped on any incompatible change;
/// readers skip lines carrying another version.
pub const EVENT_LINE_VERSION: u32 = 1;

/// Excerpts of prompts and assistant messages are cut to this many characters.
/// The board shows one line of each; anything longer is exposure without use.
pub const EXCERPT_CHARS: usize = 280;

/// Documented values of `SessionEnd.reason` (research notes E2).
pub const SESSION_END_REASONS: [&str; 5] =
    ["clear", "resume", "logout", "prompt_input_exit", "other"];

/// Documented values of `SessionStart.source` (research notes E2).
pub const SESSION_START_SOURCES: [&str; 5] = ["startup", "resume", "clear", "compact", "fork"];

/// The events that describe a sub-agent of a session rather than the
/// session itself (AD-176): their lines carry the sub-agent's id and type,
/// never its texts or transcript paths, and the board and the registry do
/// not fold them into the session's own state.
pub const SUBAGENT_EVENTS: [&str; 2] = ["SubagentStart", "SubagentStop"];

/// `agent_id` and `agent_type` are cut to this many characters (AD-176).
pub const AGENT_FIELD_CHARS: usize = 64;

/// True for a line of [`SUBAGENT_EVENTS`].
pub fn is_subagent_event(event: &str) -> bool {
    SUBAGENT_EVENTS.contains(&event)
}

/// One line of the event buffer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventLine {
    pub v: u32,
    /// Unix milliseconds at which `officina hook` received the payload.
    pub ts: i64,
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notification_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_message: Option<String>,
    /// The end of `last_assistant_message` when [`last_message`] does not
    /// hold it whole (AD-45): an optional field of version 1, which older
    /// readers ignore.
    ///
    /// [`last_message`]: EventLine::last_message
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_message_tail: Option<String>,
    /// Id of the `claude` process, from the hook's `CLAUDE_PID` (E14): ties a
    /// session id that `/clear` replaced to the live card of its process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_pid: Option<u32>,
    /// The sub-agent of a [`SUBAGENT_EVENTS`] line, cut to
    /// [`AGENT_FIELD_CHARS`] (AD-176): an optional field of version 1,
    /// which older readers ignore.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// The sub-agent's type, as [`agent_id`](EventLine::agent_id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    /// The fields of the event's [`HOOK_FIELDS`] entry the payload lacked
    /// (AD-45, AD-56): an optional field of version 1, omitted when none
    /// is missing, which older readers ignore.
    ///
    /// [`HOOK_FIELDS`]: crate::contract::HOOK_FIELDS
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub absent: Vec<String>,
    /// Set when excerpts were dropped to keep the line under [`MAX_LINE_BYTES`].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

/// Upper bound of one serialized line, newline included. One bounded write per
/// hook call is what keeps concurrent appends from interleaving (AD-10).
pub const MAX_LINE_BYTES: usize = 4096;

impl EventLine {
    /// Serializes the line with its trailing newline, dropping excerpts when
    /// the result would exceed [`MAX_LINE_BYTES`]: the answer's tail first,
    /// since the board shows the head (AD-45), then the other excerpts. The
    /// fixed fields are short identifiers, so the line always fits once
    /// excerpts are gone, except for a pathological `cwd`, which is then
    /// dropped as well.
    pub fn to_bounded_line(&self) -> String {
        let render = |line: &EventLine| {
            let mut text = serde_json::to_string(line).unwrap_or_default();
            text.push('\n');
            text
        };
        let full = render(self);
        if full.len() <= MAX_LINE_BYTES {
            return full;
        }
        let mut reduced = self.clone();
        reduced.last_message_tail = None;
        reduced.truncated = true;
        let without_tail = render(&reduced);
        if without_tail.len() <= MAX_LINE_BYTES {
            return without_tail;
        }
        reduced.prompt = None;
        reduced.last_message = None;
        reduced.message = None;
        reduced.truncated = true;
        let without_excerpts = render(&reduced);
        if without_excerpts.len() <= MAX_LINE_BYTES {
            return without_excerpts;
        }
        reduced.cwd = None;
        render(&reduced)
    }
}

/// What one raw line of an event file holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedLine {
    /// A line of this format version.
    Event(Box<EventLine>),
    /// An empty line, as a hook killed mid-line leaves before the next one.
    Blank,
    /// A well-formed line of another format version: skipped, not corrupt.
    OtherVersion,
    /// Anything else, a line that is not UTF-8 included.
    Corrupt,
}

/// Reads one line of an event file, without its `\n` (AD-10). Lines are
/// decoded one by one, so a bad byte costs one line, never the file.
///
/// ```
/// use officina_core::events::{ParsedLine, parse_line};
/// assert!(matches!(parse_line(br#"{"v":1,"ts":5,"event":"Stop"}"#), ParsedLine::Event(_)));
/// assert_eq!(parse_line(br#"{"v":2,"ts":5,"new":true}"#), ParsedLine::OtherVersion);
/// assert_eq!(parse_line(b"garbage"), ParsedLine::Corrupt);
/// assert_eq!(parse_line(b"\r"), ParsedLine::Blank);
/// ```
pub fn parse_line(raw: &[u8]) -> ParsedLine {
    let Ok(line) = std::str::from_utf8(raw) else {
        return ParsedLine::Corrupt;
    };
    let line = line.trim_end_matches('\r');
    if line.trim().is_empty() {
        return ParsedLine::Blank;
    }
    match serde_json::from_str::<EventLine>(line) {
        Ok(event) if event.v == EVENT_LINE_VERSION => ParsedLine::Event(Box::new(event)),
        Ok(_) => ParsedLine::OtherVersion,
        Err(_) => {
            let other_version = serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .and_then(|v| v.get("v").and_then(serde_json::Value::as_u64))
                .is_some_and(|v| v != u64::from(EVENT_LINE_VERSION));
            if other_version {
                ParsedLine::OtherVersion
            } else {
                ParsedLine::Corrupt
            }
        }
    }
}

/// Cuts `text` to at most `max_chars` characters on a character boundary,
/// marking the cut with an ellipsis.
pub fn excerpt(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

/// The last `max_chars` characters of `text` on a character boundary, led
/// by an ellipsis, when `text` is longer than `max_chars`; `None` when
/// [`excerpt`] of the same length holds it whole (AD-45).
///
/// ```
/// use officina_core::events::tail_excerpt;
/// assert_eq!(tail_excerpt("abcdef", 3).as_deref(), Some("…def"));
/// assert_eq!(tail_excerpt("abc", 3), None);
/// ```
pub fn tail_excerpt(text: &str, max_chars: usize) -> Option<String> {
    let count = text.chars().count();
    (count > max_chars).then(|| {
        let tail: String = text.chars().skip(count - max_chars).collect();
        format!("…{tail}")
    })
}

fn text_field(payload: &serde_json::Value, key: &str) -> Option<String> {
    payload.get(key)?.as_str().map(str::to_owned)
}

/// Reduces a raw hook payload to an [`EventLine`]. Returns `None` when the
/// payload carries no `hook_event_name`: such input is not a hook call and is
/// dropped. `with_excerpts` false omits prompt and message text entirely.
///
/// ```
/// let payload = serde_json::json!({
///     "hook_event_name": "UserPromptSubmit",
///     "session_id": "s1",
///     "cwd": "C:\\repo",
///     "prompt": "fix the build",
/// });
/// let line = officina_core::events::reduce_payload(&payload, 1_000, true).unwrap();
/// assert_eq!(line.prompt.as_deref(), Some("fix the build"));
/// ```
pub fn reduce_payload(
    payload: &serde_json::Value,
    received_at_ms: i64,
    with_excerpts: bool,
) -> Option<EventLine> {
    let event = text_field(payload, "hook_event_name")?;
    // A sub-agent's answer and transcripts are its own, never the session's
    // (AD-176): only its id and type are kept.
    let subagent = is_subagent_event(&event);
    let with_excerpts = with_excerpts && !subagent;
    let agent_field = |key: &str| {
        if subagent {
            text_field(payload, key).map(|t| t.chars().take(AGENT_FIELD_CHARS).collect())
        } else {
            None
        }
    };
    let cut = |key: &str| {
        if with_excerpts {
            text_field(payload, key).map(|t| excerpt(&t, EXCERPT_CHARS))
        } else {
            None
        }
    };
    Some(EventLine {
        v: EVENT_LINE_VERSION,
        ts: received_at_ms,
        session_id: text_field(payload, "session_id"),
        cwd: text_field(payload, "cwd"),
        source: text_field(payload, "source"),
        reason: text_field(payload, "reason"),
        notification_type: text_field(payload, "notification_type"),
        // Notification messages are Claude Code's own wording ("Claude needs
        // your permission to use Bash"), not the author's text, so they stay
        // even when excerpts are off (AD-8).
        message: text_field(payload, "message").map(|t| excerpt(&t, EXCERPT_CHARS)),
        prompt: cut("prompt"),
        last_message: cut("last_assistant_message"),
        last_message_tail: if with_excerpts {
            text_field(payload, "last_assistant_message")
                .and_then(|t| tail_excerpt(&t, EXCERPT_CHARS))
        } else {
            None
        },
        claude_pid: None,
        agent_id: agent_field("agent_id"),
        agent_type: agent_field("agent_type"),
        absent: absent_hook_fields(&event, payload),
        truncated: false,
        event,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn excerpt_respects_character_boundaries() {
        assert_eq!(excerpt("héllo", 10), "héllo");
        assert_eq!(excerpt("héllo", 2), "hé…");
        assert_eq!(excerpt("", 3), "");
    }

    #[test]
    fn stop_payload_keeps_last_message_excerpt_only() {
        let long = "x".repeat(EXCERPT_CHARS + 50);
        let payload = json!({
            "hook_event_name": "Stop",
            "session_id": "s",
            "last_assistant_message": long,
            "background_tasks": [],
            "transcript_path": "/path/to/transcript.jsonl",
        });
        let line = reduce_payload(&payload, 5, true).unwrap();
        assert_eq!(
            line.last_message.as_ref().unwrap().chars().count(),
            EXCERPT_CHARS + 1
        );
        let serialized = serde_json::to_string(&line).unwrap();
        assert!(!serialized.contains("transcript"));
    }

    #[test]
    fn excerpts_can_be_disabled() {
        let payload = json!({"hook_event_name": "UserPromptSubmit", "prompt": "secret paste"});
        let line = reduce_payload(&payload, 5, false).unwrap();
        assert_eq!(line.prompt, None);
    }

    #[test]
    fn payload_without_event_name_is_dropped() {
        assert_eq!(reduce_payload(&json!({"session_id": "s"}), 5, true), None);
        assert_eq!(reduce_payload(&json!("text"), 5, true), None);
    }

    #[test]
    fn oversized_line_drops_excerpts_and_says_so() {
        let wide = "é".repeat(EXCERPT_CHARS);
        let payload = json!({
            "hook_event_name": "Notification",
            "session_id": "s",
            "cwd": "C:/repo",
            "prompt": wide, "message": wide, "last_assistant_message": wide,
        });
        let mut line = reduce_payload(&payload, 1, true).unwrap();
        line.cwd = Some("d".repeat(3000));
        let text = line.to_bounded_line();
        assert!(text.len() <= MAX_LINE_BYTES && text.ends_with('\n'));
        let back: EventLine = serde_json::from_str(text.trim_end()).unwrap();
        assert!(back.truncated);
        assert_eq!(back.prompt, None);
        assert_eq!(back.session_id.as_deref(), Some("s"));

        let small = reduce_payload(&payload, 1, false)
            .unwrap()
            .to_bounded_line();
        assert!(!small.contains("truncated"));
    }

    fn stop_with(message: &str) -> EventLine {
        let payload = json!({
            "hook_event_name": "Stop",
            "session_id": "s",
            "last_assistant_message": message,
        });
        reduce_payload(&payload, 5, true).unwrap()
    }

    #[test]
    fn tail_is_the_last_280_characters_on_a_boundary() {
        let message: String = "é𝄞a".repeat(EXCERPT_CHARS);
        let line = stop_with(&message);
        let tail = line.last_message_tail.unwrap();
        assert!(tail.starts_with('…'));
        let kept: String = tail.chars().skip(1).collect();
        assert_eq!(kept.chars().count(), EXCERPT_CHARS);
        assert!(message.ends_with(&kept));
        assert_eq!(line.last_message, Some(excerpt(&message, EXCERPT_CHARS)));
    }

    #[test]
    fn tail_is_absent_when_the_head_holds_the_message() {
        let whole = "x".repeat(EXCERPT_CHARS);
        let line = stop_with(&whole);
        assert_eq!(line.last_message.as_deref(), Some(whole.as_str()));
        assert_eq!(line.last_message_tail, None);
        assert!(!line.to_bounded_line().contains("last_message_tail"));
        let one_more = format!("{whole}y");
        assert_eq!(
            stop_with(&one_more).last_message_tail,
            Some(format!("…{}y", "x".repeat(EXCERPT_CHARS - 1)))
        );
    }

    #[test]
    fn oversized_line_drops_the_tail_first() {
        // Four bytes each: the four excerpts together exceed the bound,
        // three of them do not.
        let wide = "𝄞".repeat(EXCERPT_CHARS + 1);
        let payload = json!({
            "hook_event_name": "Stop",
            "session_id": "s",
            "prompt": wide, "message": wide, "last_assistant_message": wide,
        });
        let line = reduce_payload(&payload, 1, true).unwrap();
        assert!(line.last_message_tail.is_some());
        let text = line.to_bounded_line();
        assert!(text.len() <= MAX_LINE_BYTES);
        let back: EventLine = serde_json::from_str(text.trim_end()).unwrap();
        assert!(back.truncated);
        assert_eq!(back.last_message_tail, None);
        assert_eq!(back.last_message, line.last_message);
        assert_eq!(back.prompt, line.prompt);
        assert_eq!(back.message, line.message);
    }

    /// The payload of `event` holding every field of its `HOOK_FIELDS`
    /// entry, redacted values.
    fn complete(event: &str) -> serde_json::Value {
        let mut payload = json!({"hook_event_name": event});
        for field in crate::contract::hook_fields(event) {
            payload[*field] = json!(format!("redacted {field}"));
        }
        payload
    }

    /// AD-45, AD-56: each field of the entry missing from the payload is
    /// listed, in the entry's order, on a version 1 line.
    #[test]
    fn absent_lists_the_missing_hook_fields() {
        for (event, fields) in crate::contract::HOOK_FIELDS {
            for field in *fields {
                let mut payload = complete(event);
                payload.as_object_mut().unwrap().remove(*field);
                let line = reduce_payload(&payload, 5, true).unwrap();
                assert_eq!(line.absent, [(*field).to_owned()], "{event}");
                assert_eq!(line.v, EVENT_LINE_VERSION);
                let text = line.to_bounded_line();
                let back: EventLine = serde_json::from_str(text.trim_end()).unwrap();
                assert_eq!(back.absent, line.absent, "{event}");
            }
            let bare = reduce_payload(&json!({"hook_event_name": event}), 5, false).unwrap();
            assert_eq!(bare.absent.len(), fields.len(), "{event}");
        }
    }

    #[test]
    fn absent_is_omitted_when_empty() {
        for (event, _) in crate::contract::HOOK_FIELDS {
            let line = reduce_payload(&complete(event), 5, true).unwrap();
            assert!(line.absent.is_empty(), "{event}");
            assert!(!line.to_bounded_line().contains("absent"), "{event}");
        }
        let other = reduce_payload(&json!({"hook_event_name": "PreCompact"}), 5, true).unwrap();
        assert!(
            other.absent.is_empty(),
            "an event with no entry lists nothing"
        );
        let old: EventLine =
            serde_json::from_str(r#"{"v":1,"ts":5,"event":"Stop","session_id":"s"}"#).unwrap();
        assert!(
            old.absent.is_empty(),
            "a line written before reads as complete"
        );
    }

    /// AD-176: a sub-agent line keeps the sub-agent's id and type, each cut
    /// to `AGENT_FIELD_CHARS` on a character boundary.
    #[test]
    fn subagent_fields_are_kept_and_capped() {
        let long: String = "é".repeat(AGENT_FIELD_CHARS + 5);
        for event in SUBAGENT_EVENTS {
            let payload = json!({
                "hook_event_name": event,
                "session_id": "lead",
                "agent_id": long,
                "agent_type": "Explore",
            });
            let line = reduce_payload(&payload, 5, true).unwrap();
            let id = line.agent_id.as_deref().unwrap();
            assert_eq!(id.chars().count(), AGENT_FIELD_CHARS, "{event}");
            assert!(long.starts_with(id), "{event}");
            assert_eq!(line.agent_type.as_deref(), Some("Explore"), "{event}");
            assert_eq!(line.session_id.as_deref(), Some("lead"), "{event}");
            let back: EventLine = serde_json::from_str(line.to_bounded_line().trim_end()).unwrap();
            assert_eq!(back, line, "{event}");
        }
        let other = reduce_payload(
            &json!({"hook_event_name": "Stop", "agent_id": "a", "agent_type": "t"}),
            5,
            true,
        )
        .unwrap();
        assert_eq!((other.agent_id, other.agent_type), (None, None));
    }

    /// AD-176: a sub-agent's answer and its transcript paths never reach
    /// the buffer, whatever the excerpt setting.
    #[test]
    fn subagent_texts_and_paths_are_never_kept() {
        for event in SUBAGENT_EVENTS {
            let payload = json!({
                "hook_event_name": event,
                "session_id": "lead",
                "agent_id": "a1",
                "agent_type": "Explore",
                "last_assistant_message": "secret answer",
                "prompt": "secret prompt",
                "agent_transcript_path": "/path/to/subagents/agent.jsonl",
                "transcript_path": "/path/to/session.jsonl",
            });
            let text = reduce_payload(&payload, 5, true).unwrap().to_bounded_line();
            for kept_out in ["secret", "transcript", "/path/to"] {
                assert!(!text.contains(kept_out), "{event}: {text}");
            }
        }
    }

    #[test]
    fn line_with_the_new_field_parses_as_version_1() {
        let with_tail = r#"{"v":1,"ts":5,"event":"Stop","session_id":"s","last_message":"head…","last_message_tail":"…tail"}"#;
        let line: EventLine = serde_json::from_str(with_tail).unwrap();
        assert_eq!(line.v, EVENT_LINE_VERSION);
        assert_eq!(line.last_message_tail.as_deref(), Some("…tail"));
        let without: EventLine =
            serde_json::from_str(r#"{"v":1,"ts":5,"event":"Stop","session_id":"s"}"#).unwrap();
        assert_eq!(without.last_message_tail, None);
        let no_excerpts = reduce_payload(
            &json!({"hook_event_name": "Stop", "last_assistant_message": "y".repeat(400)}),
            5,
            false,
        )
        .unwrap();
        assert_eq!(
            (no_excerpts.last_message, no_excerpts.last_message_tail),
            (None, None)
        );
    }
}
