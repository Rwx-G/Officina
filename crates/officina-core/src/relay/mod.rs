//! The relay (PRD v0.2, AD-21): the catalogue of closed actions a guarded
//! repository may run, its validation and canonical form, parameter values
//! and the argument vectors built from them. Pure: the host reads the files
//! and runs the programs.

pub mod approvals;
pub mod argv;
pub mod auto;
pub mod catalogue;
pub mod classes;
pub mod credential;
pub mod gate;
pub mod http;
pub mod journal;
pub mod params;
pub mod record;
pub mod run;
pub mod shell;
pub mod template;
pub mod url;
pub mod view;

#[cfg(test)]
pub(crate) mod fixtures {
    //! What the relay's unit tests share.

    use std::collections::BTreeMap;

    /// Where the unit tests' floors are read.
    pub const FLOOR_PATH: &str = "C:/path/to/officina.toml";

    /// Parameters from `(name, value)` pairs.
    pub fn params(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }
}

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// At the crate root since v0.3 (D-42); re-exported so the relay's call
/// sites keep their paths.
pub use crate::{closed_enum, sha256_hex};

/// The Officina home's directory name under the user's home, when
/// `OFFICINA_HOME` does not set another one (AD-7).
pub const HOME_DIR_NAME: &str = ".officina";
/// The relay's directory under the Officina home (AD-25).
pub const RELAY_DIR: &str = "relay";
/// The journal's directory under the Officina home (AD-25, AD-32).
pub const JOURNAL_DIR: &str = "journal";

/// Content of `relay/stopped` (AD-25, AD-34), written by the window only.
/// The file's presence stops the relay, whatever it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopFlag {
    /// When the author stopped the relay, Unix milliseconds.
    pub since_ms: i64,
}

impl StopFlag {
    /// The flag's time, when its content is a well-formed flag.
    ///
    /// ```
    /// use officina_core::relay::StopFlag;
    /// assert_eq!(StopFlag::parse(br#"{"since_ms":5}"#), Some(StopFlag { since_ms: 5 }));
    /// assert_eq!(StopFlag::parse(b""), None);
    /// ```
    pub fn parse(content: &[u8]) -> Option<StopFlag> {
        serde_json::from_slice(content).ok()
    }
}

/// SHA-256 over bytes that arrive in pieces, as `officina run` hashes an
/// output stream (AD-31).
///
/// ```
/// use officina_core::relay::{StreamHash, sha256_hex};
/// let mut hash = StreamHash::new();
/// hash.update(b"ab");
/// hash.update(b"c");
/// assert_eq!(hash.hex(), sha256_hex(b"abc"));
/// ```
#[derive(Debug, Clone, Default)]
pub struct StreamHash(Sha256);

impl StreamHash {
    pub fn new() -> StreamHash {
        StreamHash(Sha256::new())
    }

    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    /// The hash of every byte so far, lowercase hex. The stream can go on.
    pub fn hex(&self) -> String {
        crate::hex(&self.0.clone().finalize())
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use serde::Serialize;

    use super::approvals::{ApprovalBy, ApprovalState};
    use super::auto::{AutoMode, AutoOffReason, EscalationCause, ReviewDecision};
    use super::catalogue::{
        ActionClass, ForwardedValues, IssueKind, IssueTable, NoticeKind, Runner,
    };
    use super::gate::DenyKind;
    use super::http::HttpMethod;
    use super::journal::{
        AnswerOutcome, AnswerSource, GateFault, JournalKind, RefusalReason, RunOutcome,
    };
    use super::params::ValueRefusal;
    use super::record::{AnomalyKind, Verdict};
    use super::template::{ModelAlias, TemplateUnavailable};
    use super::url::UrlIssue;
    use super::view::Tone;
    use crate::floor::{Autonomy, Priority};
    use crate::ipc::IpcErrorCode;
    use crate::orders::work_order::WorkOrderRefusal;
    use crate::scene::{AlertCause, Zone};

    fn assert_codes<T: Serialize + Copy + Debug>(all: &[T], code: fn(T) -> &'static str) {
        for &variant in all {
            let wire = serde_json::to_value(variant).unwrap();
            assert_eq!(wire.as_str(), Some(code(variant)), "{variant:?}");
        }
    }

    /// `code()` is spelled at compile time from the `rename_all` literal
    /// serde reads; this replays serde on every variant of every enum built
    /// with it. The count below fails when an enum gains `code()` in a file
    /// of this crate without a line here.
    #[test]
    fn code_equals_the_serde_name_for_every_variant() {
        let checked: [fn(); 33] = [
            || assert_codes(EscalationCause::ALL, EscalationCause::code),
            || assert_codes(AutoMode::ALL, AutoMode::code),
            || assert_codes(AutoOffReason::ALL, AutoOffReason::code),
            || assert_codes(ApprovalBy::ALL, ApprovalBy::code),
            || assert_codes(ApprovalState::ALL, ApprovalState::code),
            || assert_codes(ReviewDecision::ALL, ReviewDecision::code),
            || assert_codes(ActionClass::ALL, ActionClass::code),
            || assert_codes(IssueTable::ALL, IssueTable::code),
            || assert_codes(ModelAlias::ALL, ModelAlias::code),
            || assert_codes(TemplateUnavailable::ALL, TemplateUnavailable::code),
            || assert_codes(WorkOrderRefusal::ALL, WorkOrderRefusal::code),
            || assert_codes(Autonomy::ALL, Autonomy::code),
            || assert_codes(Priority::ALL, Priority::code),
            || assert_codes(Zone::ALL, Zone::code),
            || assert_codes(AlertCause::ALL, AlertCause::code),
            || assert_codes(ForwardedValues::ALL, ForwardedValues::code),
            || assert_codes(IssueKind::ALL, IssueKind::code),
            || assert_codes(NoticeKind::ALL, NoticeKind::code),
            || assert_codes(Runner::ALL, Runner::code),
            || assert_codes(DenyKind::ALL, DenyKind::code),
            || assert_codes(HttpMethod::ALL, HttpMethod::code),
            || assert_codes(GateFault::ALL, GateFault::code),
            || assert_codes(JournalKind::ALL, JournalKind::code),
            || assert_codes(AnswerOutcome::ALL, AnswerOutcome::code),
            || assert_codes(AnswerSource::ALL, AnswerSource::code),
            || assert_codes(RefusalReason::ALL, RefusalReason::code),
            || assert_codes(RunOutcome::ALL, RunOutcome::code),
            || assert_codes(ValueRefusal::ALL, ValueRefusal::code),
            || assert_codes(AnomalyKind::ALL, AnomalyKind::code),
            || assert_codes(Verdict::ALL, Verdict::code),
            || assert_codes(UrlIssue::ALL, UrlIssue::code),
            || assert_codes(Tone::ALL, Tone::code),
            || assert_codes(IpcErrorCode::ALL, IpcErrorCode::code),
        ];
        for check in checked {
            check();
        }
        let sources = [
            include_str!("../floor.rs"),
            include_str!("../board.rs"),
            include_str!("../focus.rs"),
            include_str!("../scene.rs"),
            include_str!("../ipc.rs"),
            include_str!("../orders/work_order.rs"),
            include_str!("approvals.rs"),
            include_str!("argv.rs"),
            include_str!("auto.rs"),
            include_str!("catalogue.rs"),
            include_str!("classes.rs"),
            include_str!("credential.rs"),
            include_str!("gate.rs"),
            include_str!("http.rs"),
            include_str!("journal.rs"),
            include_str!("params.rs"),
            include_str!("record.rs"),
            include_str!("run.rs"),
            include_str!("shell.rs"),
            include_str!("template.rs"),
            include_str!("url.rs"),
            include_str!("view.rs"),
        ];
        let with_code: usize = sources
            .iter()
            .map(|source| {
                let lines: Vec<&str> = source.lines().map(str::trim).collect();
                lines
                    .windows(2)
                    .filter(|pair| {
                        pair[0] == "closed_enum! {" && pair[1].starts_with("#[serde(rename_all")
                    })
                    .count()
            })
            .sum();
        assert_eq!(checked.len(), with_code);
    }
}
