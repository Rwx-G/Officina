//! The triage model's proposals (FR-8, FR-10 as amended by amendment 8,
//! AD-40, AD-50, AD-52, D-66, D-68, D-69, D-81, D-87): whether the model is
//! on, paused or unavailable; which rows falling to the default are queued
//! for it, one case per call; which stored answers still serve; when three
//! failed calls pause it. The answer's reason is untrusted text: nothing
//! here reads it, it travels to the view untouched (AD-40).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::confirm::confirmation_input;
use super::proposal::{ModelAnswer, ModelInput, default_proposal};
use super::rates::GateState;
use super::triage_call::{CallRecord, TriageCase, failed_proposal_streak, refusal_for_binary};
use super::types::{
    ExcerptRecord, ModelVerdict, ProposalSource, SessionRecord, Sourced, TriageOutcome,
    TriageRefusal,
};
use super::verdict::Row;

/// Failed proposal calls in a row that pause the model (AD-50, D-81).
pub const PAUSE_AFTER_FAILED_CALLS: u32 = 3;

/// The value of the [`SettingKey::TriageModel`] setting (D-69): whether the
/// model is on, and the gate state that turned it off, when rules did.
///
/// [`SettingKey::TriageModel`]: super::types::SettingKey::TriageModel
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSetting {
    pub on: bool,
    #[serde(default)]
    pub reason: Option<GateState>,
}

/// The `claude` binary as a pause records it (D-66): a different stamp at
/// the next check lifts the pause.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BinaryStamp {
    pub path: String,
    pub size: u64,
    pub modified_ms: i64,
}

/// The value of the [`SettingKey::TriageModelPause`] setting (D-66, D-81):
/// paused or lifted, with the binary's stamp when it paused; the setting's
/// time is when it last changed.
///
/// [`SettingKey::TriageModelPause`]: super::types::SettingKey::TriageModelPause
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelPause {
    pub paused: bool,
    #[serde(default)]
    pub stamp: Option<BinaryStamp>,
}

/// One `model_answer` row, with the input hash of the call that gave it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredAnswer {
    pub session_id: String,
    pub verdict: ModelVerdict,
    /// Untrusted model text (AD-40): shown as text, never read by a rule.
    pub reason: String,
    pub call_id: Option<i64>,
    /// The hash of the case the call was made on; an answer whose case
    /// changed since no longer serves.
    pub input_hash: Option<String>,
    pub at_ms: i64,
}

/// The last proposal call about one case, as the queue reads it (D-87).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastCall {
    pub outcome: TriageOutcome,
    #[serde(default)]
    pub refusal: Option<TriageRefusal>,
    #[serde(default)]
    pub input_hash: Option<String>,
    pub started_at_ms: i64,
}

/// Everything the registry and the window know of the model: its setting
/// and pause with who changed them and when, whether it can be called at
/// all, the stored answers, and the last proposal call of each case key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelFacts {
    pub setting: Option<Sourced<ModelSetting>>,
    pub pause: Option<Sourced<ModelPause>>,
    /// Why the model cannot be called (FR-10 as amended: a `.cmd` or `.bat`
    /// shim, `no_excerpts`, no `claude`), `None` when it can.
    pub unavailable: Option<TriageRefusal>,
    pub answers: Vec<StoredAnswer>,
    /// By [`case_key`].
    pub last_calls: BTreeMap<String, LastCall>,
}

/// What the window knows of the model beside the registry: whether it can
/// be called, and the `claude` binary's stamp, which a pause records and a
/// change of which lifts it (D-66).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelEnv {
    pub unavailable: Option<TriageRefusal>,
    pub claude_stamp: Option<BinaryStamp>,
    /// `no_excerpts`, from the environment or the window's floor: the view
    /// shows no excerpt while it is on, before the next pass clears them.
    pub no_excerpts: bool,
}

/// Why turning the model on was refused (FR-10 as amended, AD-50, D-69):
/// nothing was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EnableRefusal {
    #[error("the triage model cannot be called: {}", .0.code())]
    Unavailable(TriageRefusal),
    #[error("the triage model cannot be turned on: its gate does not hold ({})", .0.code())]
    Gate(GateState),
}

/// Why the model may not be called (FR-10 as amended, AD-51): no `claude`,
/// a `.cmd` or `.bat` shim, or `no_excerpts`.
///
/// ```
/// use officina_core::registry::model::unavailable;
/// use officina_core::registry::types::TriageRefusal;
/// assert_eq!(unavailable(Some("C:/bin/claude.cmd"), false), Some(TriageRefusal::CmdShim));
/// assert_eq!(unavailable(Some("C:/bin/claude.exe"), true), Some(TriageRefusal::NoExcerpts));
/// assert_eq!(unavailable(Some("C:/bin/claude.exe"), false), None);
/// ```
pub fn unavailable(claude: Option<&str>, no_excerpts: bool) -> Option<TriageRefusal> {
    let Some(claude) = claude else {
        return Some(TriageRefusal::ClaudeNotFound);
    };
    refusal_for_binary(claude).or(no_excerpts.then_some(TriageRefusal::NoExcerpts))
}

/// The key of a proposal case in `triage_call.case_key`: the session the
/// row is anchored on, which `model_answer` is keyed by too.
pub fn case_key(session_id: &str) -> String {
    format!("session:{session_id}")
}

/// The case a proposal call sends for `session` (AD-52): what a
/// confirmation of its row would keep, so the dry run measures what
/// production runs; `None` without an excerpt, since there is nothing to
/// judge.
pub fn case_of(session: &SessionRecord, excerpt: Option<&ExcerptRecord>) -> Option<TriageCase> {
    confirmation_input(session, excerpt).map(|input| TriageCase::from(&input))
}

impl ModelFacts {
    /// True when the author, or nothing since, left the model on.
    pub fn on(&self) -> bool {
        self.setting.as_ref().is_some_and(|s| s.value.on)
    }

    /// True while three failed calls keep it paused (D-81).
    pub fn paused(&self) -> bool {
        self.pause.as_ref().is_some_and(|p| p.value.paused)
    }

    /// True when its stored answers take the default's place (FR-8): on
    /// and available, paused or not.
    pub fn serves(&self) -> bool {
        self.on() && self.unavailable.is_none()
    }

    /// True when rows falling to the default are queued (AD-52): on,
    /// available and not paused.
    pub fn queues(&self) -> bool {
        self.serves() && !self.paused()
    }

    /// When the model last turned itself off (D-69): turning it on again
    /// needs a dry run newer than this.
    pub fn self_disabled_at(&self) -> Option<i64> {
        self.setting
            .as_ref()
            .filter(|s| !s.value.on && s.value.reason.is_some())
            .map(|s| s.at_ms)
    }

    /// Since when failed calls count toward the pause and a case whose
    /// call failed may be sent again: the latest turning on, or lifting of
    /// a pause (D-81). A blip never locks a row out once the author
    /// re-enables.
    pub fn since_ms(&self) -> i64 {
        let enabled = self
            .setting
            .as_ref()
            .filter(|s| s.value.on)
            .map_or(i64::MIN, |s| s.at_ms);
        let lifted = self
            .pause
            .as_ref()
            .filter(|p| !p.value.paused)
            .map_or(i64::MIN, |p| p.at_ms);
        enabled.max(lifted)
    }

    /// The model's part of the proposals (AD-52): its stored answers while
    /// it serves, each only while the case it answered is the session's
    /// case now.
    pub fn input(&self, sessions: &[SessionRecord], excerpts: &[ExcerptRecord]) -> ModelInput {
        if !self.serves() {
            return ModelInput::default();
        }
        let answers = self
            .answers
            .iter()
            .filter(|answer| {
                let Some(session) = sessions.iter().find(|s| s.session_id == answer.session_id)
                else {
                    return false;
                };
                let excerpt = excerpts.iter().find(|e| e.session_id == answer.session_id);
                let current = case_of(session, excerpt).map(|case| case.input_hash());
                current.is_some() && current == answer.input_hash
            })
            .map(|answer| {
                (
                    answer.session_id.clone(),
                    ModelAnswer {
                        verdict: answer.verdict,
                        call_id: answer.call_id,
                    },
                )
            })
            .collect();
        ModelInput {
            enabled: true,
            answers,
        }
    }
}

/// True when a case may be sent under the last proposal call about it
/// (D-87): never sent; sent on other inputs; or its call on these inputs
/// failed (`failed`, `timeout`, `invalid`) before `since_ms`, the latest
/// enable or lift. An answer is held, and a refusal is never sent again on
/// the same inputs.
///
/// ```
/// use officina_core::registry::model::{LastCall, may_send};
/// use officina_core::registry::types::TriageOutcome;
/// let refused = LastCall { outcome: TriageOutcome::Refused, refusal: None, input_hash: Some("h".into()), started_at_ms: 5 };
/// assert!(!may_send(Some(&refused), "h", 10));
/// assert!(may_send(Some(&refused), "other", 10));
/// assert!(may_send(None, "h", 10));
/// ```
pub fn may_send(last: Option<&LastCall>, input_hash: &str, since_ms: i64) -> bool {
    let Some(last) = last else {
        return true;
    };
    if last.input_hash.as_deref() != Some(input_hash) {
        return true;
    }
    match last.outcome {
        TriageOutcome::Answer | TriageOutcome::Refused => false,
        TriageOutcome::Failed | TriageOutcome::Timeout | TriageOutcome::Invalid => {
            last.started_at_ms < since_ms
        }
    }
}

/// True when the proposal calls made since `since_ms` end with
/// [`PAUSE_AFTER_FAILED_CALLS`] failed ones in a row (AD-50, D-81):
/// refusals and dry runs neither count nor break the streak.
///
/// ```
/// use officina_core::registry::model::pause_due;
/// assert!(!pause_due(&[], 0));
/// ```
pub fn pause_due(calls: &[CallRecord], since_ms: i64) -> bool {
    let recent: Vec<CallRecord> = calls
        .iter()
        .filter(|c| c.started_at_ms >= since_ms)
        .cloned()
        .collect();
    failed_proposal_streak(&recent) >= PAUSE_AFTER_FAILED_CALLS
}

/// What a row shows of the model (AD-52): nothing, "model pending", its
/// proposal with the untrusted reason, or the outcome of a call that gave
/// none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowModel {
    None,
    /// Queued: the row keeps the default until the answer comes.
    Pending,
    /// The model's proposal serves; its reason, untrusted text.
    Answered {
        reason: String,
    },
    /// The last call on the row's current inputs gave no answer: the
    /// default stays, with why.
    NoAnswer {
        outcome: TriageOutcome,
        refusal: Option<TriageRefusal>,
        at_ms: i64,
    },
}

/// The session a row is about and its case now, when it has one.
fn row_case<'a>(
    row: &Row,
    sessions: &'a [SessionRecord],
    excerpts: &[ExcerptRecord],
) -> Option<(&'a SessionRecord, TriageCase)> {
    let id = row.session_id.as_deref()?;
    let session = sessions.iter().find(|s| s.session_id == id)?;
    let excerpt = excerpts.iter().find(|e| e.session_id == id);
    Some((session, case_of(session, excerpt)?))
}

/// What `row` shows of the model, the rows computed with
/// [`ModelFacts::input`] (AD-52, D-87): only the default's place is offered
/// to the model, so a row a rule settled shows nothing (FR-8).
pub fn row_model(
    row: &Row,
    facts: &ModelFacts,
    sessions: &[SessionRecord],
    excerpts: &[ExcerptRecord],
) -> RowModel {
    if !facts.on() {
        return RowModel::None;
    }
    if facts.serves() && row.proposal.source == ProposalSource::Model {
        let reason = row.session_id.as_deref().and_then(|id| {
            facts
                .answers
                .iter()
                .find(|a| a.session_id == id && a.call_id == row.proposal.call_id)
        });
        return reason.map_or(RowModel::None, |a| RowModel::Answered {
            reason: a.reason.clone(),
        });
    }
    if row.proposal != default_proposal() {
        return RowModel::None;
    }
    let Some((session, case)) = row_case(row, sessions, excerpts) else {
        return RowModel::None;
    };
    let hash = case.input_hash();
    let last = facts.last_calls.get(&case_key(&session.session_id));
    if facts.queues() && may_send(last, &hash, facts.since_ms()) {
        return RowModel::Pending;
    }
    match last {
        Some(last)
            if last.input_hash.as_deref() == Some(hash.as_str())
                && last.outcome != TriageOutcome::Answer =>
        {
            RowModel::NoAnswer {
                outcome: last.outcome,
                refusal: last.refusal,
                at_ms: last.started_at_ms,
            }
        }
        _ => RowModel::None,
    }
}

/// One proposal case to send (AD-52): the session, its key and what is
/// sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedCase {
    pub session_id: String,
    pub case_key: String,
    pub case: TriageCase,
}

/// The rows queued for the model, in the rows' order (the latest first),
/// one case per row (AD-52): those [`row_model`] says are pending.
pub fn queue(
    rows: &[Row],
    facts: &ModelFacts,
    sessions: &[SessionRecord],
    excerpts: &[ExcerptRecord],
) -> Vec<QueuedCase> {
    if !facts.queues() {
        return Vec::new();
    }
    rows.iter()
        .filter(|row| row_model(row, facts, sessions, excerpts) == RowModel::Pending)
        .filter_map(|row| {
            let (session, case) = row_case(row, sessions, excerpts)?;
            Some(QueuedCase {
                session_id: session.session_id.clone(),
                case_key: case_key(&session.session_id),
                case,
            })
        })
        .collect()
}

/// True when an answer to the call on `input_hash` about `session_id` may
/// be stored (AD-52, D-68): a row still offers the default's place to the
/// model for that session, and its case is still the one sent. An answer
/// that arrives after the row was confirmed, filed elsewhere, settled by a
/// rule or changed is dropped, its call recorded.
pub fn answer_still_wanted(
    rows: &[Row],
    session_id: &str,
    input_hash: &str,
    sessions: &[SessionRecord],
    excerpts: &[ExcerptRecord],
) -> bool {
    rows.iter()
        .filter(|row| row.session_id.as_deref() == Some(session_id))
        .filter(|row| {
            row.proposal == default_proposal() || row.proposal.source == ProposalSource::Model
        })
        .any(|row| {
            row_case(row, sessions, excerpts)
                .is_some_and(|(_, case)| case.input_hash() == input_hash)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::proposal::ModelInput;
    use crate::registry::types::{
        HookEnd, ReasonCode, Source, SubjectRecord, SubjectStatus, TriagePurpose,
    };
    use crate::registry::verdict::{VerdictInput, rows};

    fn proposal_call(outcome: TriageOutcome, started_at_ms: i64) -> CallRecord {
        CallRecord {
            purpose: TriagePurpose::Proposal,
            started_at_ms,
            outcome,
            refusal: None,
            input_tokens: None,
            output_tokens: None,
            cost_microusd: None,
        }
    }

    fn hook<T>(value: T, at_ms: i64) -> Option<Sourced<T>> {
        Some(Sourced {
            value,
            src: Source::Hook,
            at_ms,
        })
    }

    fn session(id: &str, prompt: i64) -> SessionRecord {
        SessionRecord {
            session_id: id.to_owned(),
            project: "api".to_owned(),
            first_seen_ms: 1,
            last_seen_ms: prompt + 10,
            listed: hook(1, 1),
            last_prompt_at: hook(prompt, prompt),
            last_stop_at: hook(prompt + 1, prompt + 1),
            ended: hook(
                HookEnd {
                    reason: Some("prompt_input_exit".to_owned()),
                    claude_pid: None,
                },
                prompt + 10,
            ),
            ..SessionRecord::default()
        }
    }

    fn excerpt(id: &str, prompt: &str, at: i64) -> ExcerptRecord {
        ExcerptRecord {
            session_id: id.to_owned(),
            prompt: Some(prompt.to_owned()),
            prompt_at_ms: Some(at),
            ..ExcerptRecord::default()
        }
    }

    fn on(at_ms: i64) -> Option<Sourced<ModelSetting>> {
        Some(Sourced {
            value: ModelSetting {
                on: true,
                reason: None,
            },
            src: Source::Author,
            at_ms,
        })
    }

    fn rows_of(sessions: &[SessionRecord], model: &ModelInput) -> Vec<Row> {
        let subjects: [SubjectRecord; 0] = [];
        rows(&VerdictInput {
            subjects: &subjects,
            sessions,
            confirmations: &[],
            model,
        })
    }

    /// AD-52, FR-8: an unfiled row falling to the default is pending while
    /// the model is on, available and not paused; it is not off, paused,
    /// unavailable or without an excerpt.
    #[test]
    fn default_rows_are_pending_only_while_the_model_queues() {
        let sessions = [session("a", 100), session("b", 200)];
        let excerpts = [excerpt("a", "Ship it", 100)];
        let facts = ModelFacts {
            setting: on(50),
            ..ModelFacts::default()
        };
        let found = rows_of(&sessions, &facts.input(&sessions, &excerpts));
        let state = |facts: &ModelFacts, key: &str| {
            let row = found.iter().find(|r| r.key == key).unwrap();
            row_model(row, facts, &sessions, &excerpts)
        };
        assert_eq!(state(&facts, "session:a"), RowModel::Pending);
        assert_eq!(
            state(&facts, "session:b"),
            RowModel::None,
            "no excerpt to judge"
        );
        let queued = queue(&found, &facts, &sessions, &excerpts);
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].case_key, "session:a");
        for blocked in [
            ModelFacts::default(),
            ModelFacts {
                unavailable: Some(TriageRefusal::CmdShim),
                ..facts.clone()
            },
            ModelFacts {
                pause: Some(Sourced {
                    value: ModelPause {
                        paused: true,
                        stamp: None,
                    },
                    src: Source::Rules,
                    at_ms: 60,
                }),
                ..facts.clone()
            },
        ] {
            assert_ne!(state(&blocked, "session:a"), RowModel::Pending);
            assert!(queue(&found, &blocked, &sessions, &excerpts).is_empty());
        }
    }

    /// AD-52: an answer serves while its case is the session's case; a
    /// changed case drops it and queues the row again.
    #[test]
    fn an_answer_serves_only_on_the_case_it_answered() {
        let sessions = [session("a", 100)];
        let excerpts = [excerpt("a", "Ship it", 100)];
        let hash = case_of(&sessions[0], Some(&excerpts[0]))
            .unwrap()
            .input_hash();
        let answer = |input_hash: &str| StoredAnswer {
            session_id: "a".into(),
            verdict: ModelVerdict::Finished,
            reason: "<b>done</b>".into(),
            call_id: Some(9),
            input_hash: Some(input_hash.into()),
            at_ms: 120,
        };
        let facts = ModelFacts {
            setting: on(50),
            answers: vec![answer(&hash)],
            last_calls: [(
                case_key("a"),
                LastCall {
                    outcome: TriageOutcome::Answer,
                    refusal: None,
                    input_hash: Some(hash.clone()),
                    started_at_ms: 110,
                },
            )]
            .into(),
            ..ModelFacts::default()
        };
        let found = rows_of(&sessions, &facts.input(&sessions, &excerpts));
        assert_eq!(
            (found[0].proposal.source, found[0].proposal.reason),
            (ProposalSource::Model, ReasonCode::Model)
        );
        assert_eq!(
            row_model(&found[0], &facts, &sessions, &excerpts),
            RowModel::Answered {
                reason: "<b>done</b>".into()
            }
        );
        let mut stale = ModelFacts {
            answers: vec![answer("another case")],
            ..facts.clone()
        };
        for last in stale.last_calls.values_mut() {
            last.input_hash = Some("another case".into());
        }
        let found = rows_of(&sessions, &stale.input(&sessions, &excerpts));
        assert_eq!(found[0].proposal, default_proposal());
        assert_eq!(
            row_model(&found[0], &stale, &sessions, &excerpts),
            RowModel::Pending
        );
        // Disabled, an answer never serves.
        let off = ModelFacts {
            setting: None,
            ..facts
        };
        assert_eq!(off.input(&sessions, &excerpts), ModelInput::default());
    }

    /// D-87, D-81: a refused case waits for new inputs; a failed one waits
    /// for the next enable or lift; an answered one is held.
    #[test]
    fn a_case_is_sent_again_only_on_new_inputs_or_after_a_lift() {
        let last = |outcome, at| LastCall {
            outcome,
            refusal: None,
            input_hash: Some("h".into()),
            started_at_ms: at,
        };
        for outcome in TriageOutcome::ALL {
            assert!(may_send(Some(&last(*outcome, 10)), "new", 0), "{outcome:?}");
        }
        assert!(!may_send(Some(&last(TriageOutcome::Answer, 10)), "h", 20));
        assert!(!may_send(Some(&last(TriageOutcome::Refused, 10)), "h", 20));
        for failed in [
            TriageOutcome::Failed,
            TriageOutcome::Timeout,
            TriageOutcome::Invalid,
        ] {
            assert!(!may_send(Some(&last(failed, 10)), "h", 5));
            assert!(may_send(Some(&last(failed, 10)), "h", 20));
        }
    }

    /// D-81: three failed proposal calls in a row pause the model; an
    /// answer breaks the streak; calls before the latest enable or lift do
    /// not count.
    #[test]
    fn three_failed_calls_in_a_row_are_due_a_pause() {
        let calls = [
            proposal_call(TriageOutcome::Answer, 1),
            proposal_call(TriageOutcome::Failed, 2),
            proposal_call(TriageOutcome::Timeout, 3),
        ];
        assert!(!pause_due(&calls, 0));
        let mut more = calls.to_vec();
        more.push(proposal_call(TriageOutcome::Invalid, 4));
        assert!(pause_due(&more, 0));
        assert!(!pause_due(&more, 3), "the streak since the lift is two");
        let mut refused = more.clone();
        refused.push(CallRecord {
            refusal: Some(TriageRefusal::WorkdirNotEmpty),
            ..proposal_call(TriageOutcome::Refused, 5)
        });
        assert!(
            pause_due(&refused, 0),
            "a refusal neither counts nor breaks"
        );
    }

    /// D-69: a rules turn-off is a self-disable; the author's is not.
    #[test]
    fn self_disable_time_is_read_from_a_rules_turn_off() {
        let off = |reason| ModelFacts {
            setting: Some(Sourced {
                value: ModelSetting { on: false, reason },
                src: Source::Rules,
                at_ms: 70,
            }),
            ..ModelFacts::default()
        };
        assert_eq!(
            off(Some(GateState::WorseThanDefault)).self_disabled_at(),
            Some(70)
        );
        assert_eq!(off(None).self_disabled_at(), None);
    }

    /// AD-52, D-68: an answer arriving after its row was confirmed, or for
    /// a case that changed, is not wanted; a row a rule settled is not
    /// either.
    #[test]
    fn a_late_or_changed_answer_is_not_wanted() {
        let sessions = [session("a", 100)];
        let excerpts = [excerpt("a", "Ship it", 100)];
        let hash = case_of(&sessions[0], Some(&excerpts[0]))
            .unwrap()
            .input_hash();
        let found = rows_of(&sessions, &ModelInput::default());
        assert!(answer_still_wanted(
            &found, "a", &hash, &sessions, &excerpts
        ));
        assert!(!answer_still_wanted(
            &found, "a", "older", &sessions, &excerpts
        ));
        assert!(!answer_still_wanted(&[], "a", &hash, &sessions, &excerpts));
        let mut cut = session("a", 100);
        cut.last_stop_at = None;
        let cut = [cut];
        let settled = rows_of(&cut, &ModelInput::default());
        let cut_hash = case_of(&cut[0], Some(&excerpts[0])).unwrap().input_hash();
        assert!(!answer_still_wanted(
            &settled, "a", &cut_hash, &cut, &excerpts
        ));
        // A subject row anchored on the session is wanted too.
        let subject = SubjectRecord {
            id: 1,
            project: "api".into(),
            name: "s".into(),
            status: Sourced {
                value: SubjectStatus::Active,
                src: Source::Author,
                at_ms: 1,
            },
            created_at_ms: 1,
            routine: false,
            merged_into: None,
        };
        let mut filed = session("a", 100);
        filed.subject = Some(Sourced {
            value: Some(1),
            src: Source::Author,
            at_ms: 1,
        });
        let filed = [filed];
        let subject_rows = rows(&VerdictInput {
            subjects: &[subject],
            sessions: &filed,
            confirmations: &[],
            model: &ModelInput::default(),
        });
        let filed_hash = case_of(&filed[0], Some(&excerpts[0])).unwrap().input_hash();
        assert!(answer_still_wanted(
            &subject_rows,
            "a",
            &filed_hash,
            &filed,
            &excerpts
        ));
    }
}
