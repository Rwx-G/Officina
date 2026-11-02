//! A worker's `stream-json` output folded into facts (AD-109, FR-8): what
//! the window keeps of each line, permissively parsed, every text
//! untrusted, and the three signs that stop a run sooner. No line can keep
//! a run going or mark it done: any process of the container can print
//! one (R50).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::launch::WORKER_DISALLOWED_TOOLS;
use crate::untrusted::Untrusted;

/// The fields the fold reads, as `<type>[/<subtype>].<path>` (AD-12's
/// canary constants, from the gate 3, 4, 5 and 6 entries and D-1101).
pub const STREAM_FIELDS: &[&str] = &[
    "system/init.model",
    "system/init.tools",
    "system/init.mcp_servers",
    "system/hook_response.hook_event",
    "system/hook_response.exit_code",
    "system/api_retry.error",
    "result.subtype",
    "result.is_error",
    "result.total_cost_usd",
    "result.num_turns",
    "result.duration_ms",
    "result.session_id",
    "result.api_error_status",
    "result.terminal_reason",
    "assistant.message.content",
    "assistant.error",
];

/// The longest line folded; a longer one is counted and skipped.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;

/// How much assistant text is kept, its end (AD-109).
pub const ASSISTANT_TAIL_BYTES: usize = 4 * 1024;

/// How many `authentication_failed` retries in a row stop a run (AD-109).
pub const AUTH_FAILURES_IN_A_ROW: u32 = 3;

/// The exit codes of a hook whose program could not run (the gate 5
/// entry): not found, not executable.
pub const HOOK_FAILED_EXITS: [i64; 2] = [126, 127];

/// The retry error that counts toward `auth_failed`.
pub const AUTH_ERROR: &str = "authentication_failed";

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// A sign in the stream that stops a run sooner (AD-109).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum StreamStop {
        /// The first `system/init` lists a tool the worker never runs.
        UnexpectedTool,
        /// A hook's program could not run (exit 126 or 127).
        HookFailed,
        /// The credential was refused (AD-109's run of retries, D-1101's
        /// shapes).
        AuthFailed,
    }
}

/// The first `system/init`'s facts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitFacts {
    pub model: Option<Untrusted<String>>,
    pub tools: Vec<Untrusted<String>>,
    pub mcp_servers: Vec<Untrusted<String>>,
}

/// One hook event's counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookCounts {
    pub ran: u32,
    pub failed: u32,
}

/// The last `result`'s facts, display only.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResultFacts {
    pub subtype: Option<Untrusted<String>>,
    pub is_error: Option<bool>,
    pub cost_usd: Option<f64>,
    pub turns: Option<u64>,
    pub duration_ms: Option<u64>,
    pub session_id: Option<Untrusted<String>>,
    pub api_error_status: Option<u64>,
    pub terminal_reason: Option<Untrusted<String>>,
}

/// What the window keeps of a worker's stream (AD-109).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StreamFacts {
    pub lines: u64,
    /// Lines over [`MAX_LINE_BYTES`] or not JSON objects.
    pub skipped: u64,
    pub init: Option<InitFacts>,
    /// Per hook event (worker text, bounded by the map's own use).
    pub hooks: BTreeMap<String, HookCounts>,
    /// `system/api_retry` errors by category.
    pub retries: BTreeMap<String, u32>,
    /// The current run of consecutive `authentication_failed` retries.
    pub auth_run: u32,
    pub result: Option<ResultFacts>,
    pub assistant_tail: Option<Untrusted<String>>,
    /// The first sign that stops the run, if any.
    pub stop: Option<StreamStop>,
    /// The tail's text, which only the fold reads to append to it.
    #[serde(skip)]
    tail: String,
}

fn text(value: &Value) -> Option<Untrusted<String>> {
    value.as_str().map(|s| Untrusted::new(s.to_owned()))
}

fn names(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_str().or_else(|| item["name"].as_str()))
        .map(str::to_owned)
        .collect()
}

fn untrusted(names: Vec<String>) -> Vec<Untrusted<String>> {
    names.into_iter().map(Untrusted::new).collect()
}

/// Keeps the end of `tail` plus `more` within [`ASSISTANT_TAIL_BYTES`],
/// cut on a character boundary.
fn appended(tail: &str, more: &str) -> String {
    let mut all = format!("{tail}{more}");
    if all.len() > ASSISTANT_TAIL_BYTES {
        let mut start = all.len() - ASSISTANT_TAIL_BYTES;
        while !all.is_char_boundary(start) {
            start += 1;
        }
        all = all[start..].to_owned();
    }
    all
}

impl StreamFacts {
    fn stop_with(&mut self, stop: StreamStop) {
        self.stop.get_or_insert(stop);
    }
}

/// Folds one stdout line of the worker into `facts` (AD-109): permissive,
/// deterministic, so a replay from the start rebuilds the same facts.
pub fn fold(facts: &mut StreamFacts, line: &str) {
    facts.lines += 1;
    if line.len() > MAX_LINE_BYTES {
        facts.skipped += 1;
        return;
    }
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        facts.skipped += 1;
        return;
    };
    if !value.is_object() {
        facts.skipped += 1;
        return;
    }
    match (value["type"].as_str(), value["subtype"].as_str()) {
        (Some("system"), Some("init")) => {
            if facts.init.is_some() {
                return;
            }
            let tools = names(&value["tools"]);
            if tools
                .iter()
                .any(|t| WORKER_DISALLOWED_TOOLS.contains(&t.as_str()))
            {
                facts.stop_with(StreamStop::UnexpectedTool);
            }
            facts.init = Some(InitFacts {
                model: text(&value["model"]),
                tools: untrusted(tools),
                mcp_servers: untrusted(names(&value["mcp_servers"])),
            });
        }
        (Some("system"), Some("hook_response")) => {
            let event = value["hook_event"].as_str().unwrap_or("unknown").to_owned();
            let failed = value["exit_code"]
                .as_i64()
                .is_some_and(|code| HOOK_FAILED_EXITS.contains(&code));
            let counts = facts.hooks.entry(event).or_default();
            counts.ran = counts.ran.saturating_add(1);
            if failed {
                counts.failed = counts.failed.saturating_add(1);
                facts.stop_with(StreamStop::HookFailed);
            }
        }
        (Some("system"), Some("api_retry")) => {
            let error = value["error"].as_str().unwrap_or("unknown").to_owned();
            if error == AUTH_ERROR {
                facts.auth_run = facts.auth_run.saturating_add(1);
                if facts.auth_run >= AUTH_FAILURES_IN_A_ROW {
                    facts.stop_with(StreamStop::AuthFailed);
                }
            } else {
                facts.auth_run = 0;
            }
            let count = facts.retries.entry(error).or_default();
            *count = count.saturating_add(1);
        }
        (Some("assistant"), _) => {
            facts.auth_run = 0;
            if value["error"].as_str() == Some(AUTH_ERROR) {
                facts.stop_with(StreamStop::AuthFailed);
            }
            let said: String = value["message"]["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|part| part["type"] == "text")
                .filter_map(|part| part["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            if !said.is_empty() {
                facts.tail = appended(&facts.tail, &said);
                facts.assistant_tail = Some(Untrusted::new(facts.tail.clone()));
            }
        }
        (Some("result"), _) => {
            let status = value["api_error_status"].as_u64();
            if matches!(status, Some(401 | 403)) {
                facts.stop_with(StreamStop::AuthFailed);
            }
            facts.result = Some(ResultFacts {
                subtype: text(&value["subtype"]),
                is_error: value["is_error"].as_bool(),
                cost_usd: value["total_cost_usd"].as_f64(),
                turns: value["num_turns"].as_u64(),
                duration_ms: value["duration_ms"].as_u64(),
                session_id: text(&value["session_id"]),
                api_error_status: status,
                terminal_reason: text(&value["terminal_reason"]),
            });
        }
        _ => {}
    }
}
