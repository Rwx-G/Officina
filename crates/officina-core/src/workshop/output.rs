//! What the window keeps of a run's containers' output (AD-109, D-216,
//! D-230): the worker's stream fold, the entry point's status lines, the
//! gateway's counts and refusals, and the bytes the worker printed. Every
//! text here is worker text; [`RunOutput::texts`] is the only way out, and
//! it replaces the credential's forms first. Pure: the host's followers
//! hand it lines, and act on the events it returns.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::gateway::{GatewayLine, GatewayRefusal, MAX_LINE_HOST_CHARS};
use super::lifecycle::StopReason;
use super::run::{EntryLine, MAX_ENTRY_LINE_BYTES};
use super::scan::{Needles, replace};
use super::stream::{MAX_LINE_BYTES, StreamFacts, StreamStop, fold};
use super::vector::ContainerRole;
use crate::untrusted::Untrusted;

/// The bytes of stdout and stderr a worker may print before the window
/// stops it (AD-109's `output_cap`).
pub const OUTPUT_CAP_BYTES: u64 = 64 * 1024 * 1024;

/// How many allowed hosts are counted by name (D-216); the rest are
/// counted under [`OTHER_HOSTS`].
pub const MAX_ALLOWED_HOSTS: usize = 256;

/// The key the hosts past [`MAX_ALLOWED_HOSTS`] are counted under.
pub const OTHER_HOSTS: &str = "other";

/// How many refusals are kept while they wait (D-230's held branch);
/// past it they are counted only.
pub const MAX_HELD_REFUSALS: usize = 256;

/// How many entry point lines are kept, the first ones.
pub const MAX_ENTRY_LINES: usize = 64;

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Which of a container's pipes a line came from.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub enum Pipe {
        Stdout,
        Stderr,
    }
}

/// One line a follower read (D-216): its text, or the mark of a line past
/// the bound that the reader discarded to its newline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineRead {
    Line(String),
    Oversized,
}

/// `line` without the timestamp `docker logs --timestamps` puts first.
/// The time itself is never read: no clock is folded from a replayed line
/// (D-242, review F21).
///
/// ```
/// use officina_core::workshop::output::without_timestamp;
/// assert_eq!(without_timestamp("2026-11-02T10:00:00.123456789Z {\"a\":1}"), "{\"a\":1}");
/// assert_eq!(without_timestamp("{\"a\":1}"), "{\"a\":1}");
/// ```
pub fn without_timestamp(line: &str) -> &str {
    match line.split_once(' ') {
        Some((stamp, rest))
            if stamp.len() >= 20
                && stamp.as_bytes().get(4) == Some(&b'-')
                && stamp.as_bytes().get(10) == Some(&b'T')
                && stamp.bytes().all(|b| b.is_ascii_graphic()) =>
        {
            rest
        }
        _ => line,
    }
}

/// A refusal the gateway logged, in the order it logged them: `ordinal` is
/// its rank from the follow's start, so a replay finds the same ranks and
/// the host journals each one once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub ordinal: u32,
    /// Worker text, made printable and cut by the gateway's rule.
    pub host: String,
    pub port: u16,
    pub cause: GatewayRefusal,
}

/// What a line asks of the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputEvent {
    /// Journal this refusal once (AD-120), after replacement.
    Refused(Refusal),
    /// Stop the worker (AD-109): the stream or the output cap.
    Stop(StopReason),
}

/// The gateway's output, folded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GatewayFacts {
    pub started: bool,
    /// Per allowed host, at most [`MAX_ALLOWED_HOSTS`] names plus
    /// [`OTHER_HOSTS`].
    pub allowed: BTreeMap<String, u32>,
    pub refused: u32,
    /// The gateway reached its refusal limit.
    pub limit: bool,
    pub skipped: u64,
}

impl GatewayFacts {
    fn count_allowed(&mut self, host: &str) {
        let key = if self.allowed.contains_key(host) || self.allowed.len() < MAX_ALLOWED_HOSTS {
            host
        } else {
            OTHER_HOSTS
        };
        let count = self.allowed.entry(key.to_owned()).or_default();
        *count = count.saturating_add(1);
    }
}

/// Everything the window keeps of one run's output (AD-109).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunOutput {
    pub stream: StreamFacts,
    /// The entry point's first status lines (its stderr).
    pub entry: Vec<EntryLine>,
    /// Entry point lines skipped: not one, past the bound or the count.
    pub entry_skipped: u64,
    pub gateway: GatewayFacts,
    /// Bytes of the worker's stdout and stderr, newlines included.
    pub worker_bytes: u64,
    /// The worker's lines over [`MAX_LINE_BYTES`].
    pub oversized: u64,
}

/// The texts of a run's output with the credential's forms replaced
/// (D-230), the only shape they leave the window's memory in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputTexts {
    pub assistant_tail: Option<Untrusted<String>>,
    pub allowed_hosts: Vec<(Untrusted<String>, u32)>,
    pub entry_details: Vec<Untrusted<String>>,
    pub result_subtype: Option<Untrusted<String>>,
}

fn stop_of(stop: StreamStop) -> StopReason {
    match stop {
        StreamStop::UnexpectedTool => StopReason::UnexpectedTool,
        StreamStop::HookFailed => StopReason::HookFailed,
        StreamStop::AuthFailed => StopReason::AuthFailed,
    }
}

impl RunOutput {
    /// Forgets what `role`'s follower gave, before it reads again from the
    /// start (AD-109): the facts are rebuilt from nothing.
    pub fn restart(&mut self, role: ContainerRole) {
        match role {
            ContainerRole::Gateway => self.gateway = GatewayFacts::default(),
            _ => {
                self.stream = StreamFacts::default();
                self.entry.clear();
                self.entry_skipped = 0;
                self.worker_bytes = 0;
                self.oversized = 0;
            }
        }
    }

    /// Folds one line of `role`'s `pipe` (AD-109, D-216). A worker's stdout
    /// is its stream, its stderr the entry point's lines; a gateway's lines
    /// are [`GatewayLine`]s on either pipe. Deterministic, so a replay from
    /// the start returns the same events again.
    pub fn observe(
        &mut self,
        role: ContainerRole,
        pipe: Pipe,
        read: &LineRead,
    ) -> Vec<OutputEvent> {
        match role {
            ContainerRole::Gateway => self.observe_gateway(read).into_iter().collect(),
            _ => self.observe_worker(pipe, read),
        }
    }

    fn observe_worker(&mut self, pipe: Pipe, read: &LineRead) -> Vec<OutputEvent> {
        let mut events = Vec::new();
        let before = self.stream.stop;
        match read {
            LineRead::Oversized => {
                self.worker_bytes = self.worker_bytes.saturating_add(MAX_LINE_BYTES as u64 + 1);
                self.oversized += 1;
                if pipe == Pipe::Stdout {
                    self.stream.lines += 1;
                    self.stream.skipped += 1;
                } else {
                    self.entry_skipped += 1;
                }
            }
            LineRead::Line(line) => {
                self.worker_bytes = self.worker_bytes.saturating_add(line.len() as u64 + 1);
                match pipe {
                    Pipe::Stdout => fold(&mut self.stream, line),
                    Pipe::Stderr => match EntryLine::parse(line) {
                        Some(entry) if self.entry.len() < MAX_ENTRY_LINES => self.entry.push(entry),
                        _ => self.entry_skipped += 1,
                    },
                }
            }
        }
        if before.is_none()
            && let Some(stop) = self.stream.stop
        {
            events.push(OutputEvent::Stop(stop_of(stop)));
        }
        if self.worker_bytes >= OUTPUT_CAP_BYTES {
            events.push(OutputEvent::Stop(StopReason::OutputCap));
        }
        events
    }

    fn observe_gateway(&mut self, read: &LineRead) -> Option<OutputEvent> {
        let LineRead::Line(line) = read else {
            self.gateway.skipped += 1;
            return None;
        };
        let parsed = (line.len() <= MAX_ENTRY_LINE_BYTES)
            .then(|| serde_json::from_str::<GatewayLine>(line).ok())
            .flatten();
        match parsed {
            Some(GatewayLine::Started { .. }) => self.gateway.started = true,
            Some(GatewayLine::Allowed { host, .. }) => {
                self.gateway
                    .count_allowed(&super::gateway::printable(&host));
            }
            Some(GatewayLine::Refused {
                host, port, cause, ..
            }) => {
                self.gateway.refused = self.gateway.refused.saturating_add(1);
                return Some(OutputEvent::Refused(Refusal {
                    ordinal: self.gateway.refused,
                    host: super::gateway::printable(&host),
                    port,
                    cause,
                }));
            }
            Some(GatewayLine::Limit {}) => self.gateway.limit = true,
            None => self.gateway.skipped += 1,
        }
        None
    }

    /// The output's texts with every form of the credential replaced
    /// (D-230). The caller holds the texts while it has no needles.
    pub fn texts(&self, needles: &Needles) -> OutputTexts {
        let clean = |text: &str| Untrusted::new(replace(text, needles));
        OutputTexts {
            assistant_tail: self
                .stream
                .assistant_tail
                .as_ref()
                .map(|tail| tail.mapped(|t| replace(t, needles))),
            allowed_hosts: self
                .gateway
                .allowed
                .iter()
                .map(|(host, count)| (clean(host), *count))
                .collect(),
            entry_details: self
                .entry
                .iter()
                .filter_map(|line| line.detail.as_deref())
                .map(clean)
                .collect(),
            result_subtype: self
                .stream
                .result
                .as_ref()
                .and_then(|r| r.subtype.as_ref())
                .map(|s| s.mapped(|t| replace(t, needles))),
        }
    }
}

/// `refusal`'s host with the credential's forms replaced (D-230), cut to
/// the gateway's bound again: a replacement can lengthen it.
pub fn clean_host(host: &str, needles: &Needles) -> String {
    replace(host, needles)
        .chars()
        .take(MAX_LINE_HOST_CHARS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str) -> LineRead {
        LineRead::Line(text.to_owned())
    }

    /// D-216: a leading `*.` entry admits unbounded names; the counts keep
    /// [`MAX_ALLOWED_HOSTS`] of them, the rest under [`OTHER_HOSTS`].
    #[test]
    fn allowed_hosts_are_bounded() {
        let mut output = RunOutput::default();
        for n in 0..MAX_ALLOWED_HOSTS + 10 {
            let allowed = GatewayLine::Allowed {
                host: format!("h{n}.example.com"),
                port: 443,
                ech: false,
            };
            output.observe(
                ContainerRole::Gateway,
                Pipe::Stdout,
                &line(&allowed.to_line()),
            );
        }
        assert_eq!(output.gateway.allowed.len(), MAX_ALLOWED_HOSTS + 1);
        assert_eq!(output.gateway.allowed[OTHER_HOSTS], 10);
    }

    /// AD-109: the output cap stops the worker once its bytes pass it.
    #[test]
    fn the_output_cap_stops() {
        let mut output = RunOutput {
            worker_bytes: OUTPUT_CAP_BYTES - 2,
            ..RunOutput::default()
        };
        let events = output.observe(ContainerRole::Worker, Pipe::Stderr, &line("x"));
        assert_eq!(events, vec![OutputEvent::Stop(StopReason::OutputCap)]);
    }

    /// AD-109: a replay after [`RunOutput::restart`] gives the same
    /// refusal ranks, so the host journals each refusal once.
    #[test]
    fn a_replay_ranks_refusals_the_same() {
        let refused =
            GatewayLine::refused("x.example.com", 443, GatewayRefusal::NotAllowed, None).to_line();
        let mut output = RunOutput::default();
        let first: Vec<OutputEvent> = (0..2)
            .flat_map(|_| output.observe(ContainerRole::Gateway, Pipe::Stdout, &line(&refused)))
            .collect();
        output.restart(ContainerRole::Gateway);
        let again: Vec<OutputEvent> = (0..2)
            .flat_map(|_| output.observe(ContainerRole::Gateway, Pipe::Stdout, &line(&refused)))
            .collect();
        assert_eq!(first, again);
        assert!(matches!(&first[1], OutputEvent::Refused(r) if r.ordinal == 2));
    }

    /// D-230: no text leaves with the credential in it.
    #[test]
    fn texts_are_replaced() {
        let value = "a-seeded-value-0123456789abcdef";
        let needles = crate::workshop::scan::needles(value.as_bytes());
        let mut output = RunOutput::default();
        let allowed = GatewayLine::Allowed {
            host: format!("{value}.example.com"),
            port: 443,
            ech: false,
        };
        output.observe(
            ContainerRole::Gateway,
            Pipe::Stdout,
            &line(&allowed.to_line()),
        );
        let said = serde_json::json!({"type": "assistant", "message": {"content": [{"type": "text", "text": value}]}});
        output.observe(
            ContainerRole::Worker,
            Pipe::Stdout,
            &line(&said.to_string()),
        );
        let texts = serde_json::to_string(&output.texts(&needles)).unwrap();
        assert!(!texts.contains(value), "{texts}");
        assert!(texts.contains(crate::workshop::scan::REPLACEMENT));
        assert!(!clean_host(&format!("{value}.example.com"), &needles).contains(value));
    }
}
