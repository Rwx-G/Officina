//! Where a verdict proposal comes from (FR-8, AD-49, D-59): the first
//! source that matches wins, in this order: Claude Code's background
//! `state`, Officina's `rules` (a turn cut mid-way), the triage `model` when
//! it is enabled and has answered, else the default, `to_resume` by rule.
//! Each proposal carries a [`ReasonCode`], never prose, and is computed
//! again on every view build from the session's facts: nothing here is
//! stored (AD-40).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::agents::LifecycleState;

use super::types::{ModelVerdict, ProposalSource, ReasonCode, SessionRecord, Verdict};

/// A verdict proposal with where it came from and why (glossary,
/// Proposal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Proposal {
    pub verdict: ModelVerdict,
    pub source: ProposalSource,
    pub reason: ReasonCode,
    /// The triage call that answered a model proposal, which a confirmation
    /// stores (D-63, D-88); `None` for every other source.
    pub call_id: Option<i64>,
}

impl Proposal {
    /// The verdict confirming the proposal as shown applies: none for
    /// `unsure`, which waits for the author's choice (D-59).
    ///
    /// ```
    /// use officina_core::registry::proposal::empty_subject;
    /// use officina_core::registry::types::Verdict;
    /// assert_eq!(empty_subject().definite(), Some(Verdict::Finished));
    /// ```
    pub fn definite(&self) -> Option<Verdict> {
        match self.verdict {
            ModelVerdict::Finished => Some(Verdict::Finished),
            ModelVerdict::ToResume => Some(Verdict::ToResume),
            ModelVerdict::Unsure => None,
        }
    }
}

/// The triage model's part, a pure input (AD-52): whether it is enabled,
/// and what it answered for each session, by session id. Its runner and
/// queue fill it (stories 3.9, 3.11); until then it is disabled.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelInput {
    pub enabled: bool,
    #[serde(default)]
    pub answers: BTreeMap<String, ModelAnswer>,
}

/// A stored model answer as a proposal reads it: the verdict, and the call
/// that gave it (`model_answer.call_id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelAnswer {
    pub verdict: ModelVerdict,
    #[serde(default)]
    pub call_id: Option<i64>,
}

/// True when the last `UserPromptSubmit` came after the last `Stop`, or
/// no `Stop` followed a prompt at all: the session ended inside a turn.
/// Read from the facts story 3.3 records, never from a transcript.
pub fn cut_mid_turn(session: &SessionRecord) -> bool {
    match (&session.last_prompt_at, &session.last_stop_at) {
        (Some(prompt), Some(stop)) => prompt.value > stop.value,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

/// The background `state` rule: `done` proposes `finished`, `failed` and
/// `stopped` propose `to_resume` (E1: a `done` session is alive, idle and
/// ended for triage).
fn from_state(session: &SessionRecord) -> Option<Proposal> {
    if !session.is_background() {
        return None;
    }
    let (verdict, reason) = match session.state.as_ref()?.value {
        LifecycleState::Done => (ModelVerdict::Finished, ReasonCode::StateDone),
        LifecycleState::Failed => (ModelVerdict::ToResume, ReasonCode::StateFailed),
        LifecycleState::Stopped => (ModelVerdict::ToResume, ReasonCode::StateStopped),
        _ => return None,
    };
    Some(Proposal {
        verdict,
        source: ProposalSource::State,
        reason,
        call_id: None,
    })
}

/// The proposal for a row anchored on `session` (AD-49): first match wins.
///
/// ```
/// use officina_core::registry::proposal::{ModelInput, propose};
/// use officina_core::registry::types::{ProposalSource, ReasonCode, SessionRecord};
/// let quiet = SessionRecord { session_id: "s".into(), ..SessionRecord::default() };
/// let proposal = propose(&quiet, &ModelInput::default());
/// assert_eq!((proposal.source, proposal.reason), (ProposalSource::Rules, ReasonCode::DefaultToResume));
/// ```
pub fn propose(session: &SessionRecord, model: &ModelInput) -> Proposal {
    if let Some(state) = from_state(session) {
        return state;
    }
    if cut_mid_turn(session) {
        return Proposal {
            verdict: ModelVerdict::ToResume,
            source: ProposalSource::Rules,
            reason: ReasonCode::CutMidTurn,
            call_id: None,
        };
    }
    if model.enabled
        && let Some(answer) = model.answers.get(&session.session_id)
    {
        return Proposal {
            verdict: answer.verdict,
            source: ProposalSource::Model,
            reason: ReasonCode::Model,
            call_id: answer.call_id,
        };
    }
    Proposal {
        verdict: ModelVerdict::ToResume,
        source: ProposalSource::Rules,
        reason: ReasonCode::DefaultToResume,
        call_id: None,
    }
}

/// The proposal of an "empty subject: archive?" row (D-76): `finished`, by
/// rule, the one `rules` proposal of `finished`.
pub fn empty_subject() -> Proposal {
    Proposal {
        verdict: ModelVerdict::Finished,
        source: ProposalSource::Rules,
        reason: ReasonCode::EmptySubject,
        call_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::SessionKind;
    use crate::registry::types::{HookEnd, Source, Sourced};

    fn sourced<T>(value: T, at_ms: i64) -> Option<Sourced<T>> {
        Some(Sourced {
            value,
            src: Source::Hook,
            at_ms,
        })
    }

    fn session() -> SessionRecord {
        SessionRecord {
            session_id: "s".to_owned(),
            last_prompt_at: sourced(100, 100),
            last_stop_at: sourced(200, 200),
            ended: sourced(
                HookEnd {
                    reason: Some("prompt_input_exit".to_owned()),
                    claude_pid: None,
                },
                300,
            ),
            ..SessionRecord::default()
        }
    }

    fn background(state: LifecycleState) -> SessionRecord {
        SessionRecord {
            kind: sourced(SessionKind::Background, 50),
            state: sourced(state, 400),
            ..session()
        }
    }

    fn model(enabled: bool, verdict: ModelVerdict) -> ModelInput {
        ModelInput {
            enabled,
            answers: [(
                "s".to_owned(),
                ModelAnswer {
                    verdict,
                    call_id: Some(7),
                },
            )]
            .into(),
        }
    }

    fn of(proposal: Proposal) -> (ModelVerdict, ProposalSource, ReasonCode) {
        (proposal.verdict, proposal.source, proposal.reason)
    }

    #[test]
    fn first_match_wins() {
        let answered = model(true, ModelVerdict::Finished);
        // `state` beats a cut turn and the model.
        let mut cut_background = background(LifecycleState::Failed);
        cut_background.last_stop_at = None;
        assert_eq!(
            of(propose(&cut_background, &answered)),
            (
                ModelVerdict::ToResume,
                ProposalSource::State,
                ReasonCode::StateFailed
            )
        );
        assert_eq!(
            of(propose(&background(LifecycleState::Done), &answered)),
            (
                ModelVerdict::Finished,
                ProposalSource::State,
                ReasonCode::StateDone
            )
        );
        assert_eq!(
            of(propose(&background(LifecycleState::Stopped), &answered)),
            (
                ModelVerdict::ToResume,
                ProposalSource::State,
                ReasonCode::StateStopped
            )
        );
        // A cut turn beats the model.
        let mut cut = session();
        cut.last_prompt_at = sourced(250, 250);
        assert_eq!(
            of(propose(&cut, &answered)),
            (
                ModelVerdict::ToResume,
                ProposalSource::Rules,
                ReasonCode::CutMidTurn
            )
        );
        cut.last_stop_at = None;
        assert!(cut_mid_turn(&cut), "a prompt no Stop followed");
        // The model beats the default.
        assert_eq!(
            of(propose(&session(), &answered)),
            (
                ModelVerdict::Finished,
                ProposalSource::Model,
                ReasonCode::Model
            )
        );
    }

    /// A normal exit after the turn ended proposes nothing by itself: the
    /// default applies.
    #[test]
    fn prompt_input_exit_after_stop_falls_to_the_default() {
        assert!(!cut_mid_turn(&session()));
        assert_eq!(
            of(propose(&session(), &ModelInput::default())),
            (
                ModelVerdict::ToResume,
                ProposalSource::Rules,
                ReasonCode::DefaultToResume
            )
        );
    }

    /// The model takes the default's place only: disabled, or without an
    /// answer for this session, the default stands; `unsure` is allowed and
    /// carries no definite verdict (D-59).
    #[test]
    fn model_answer_replaces_only_the_default() {
        let disabled = model(false, ModelVerdict::Finished);
        assert_eq!(
            propose(&session(), &disabled).reason,
            ReasonCode::DefaultToResume
        );
        let other = ModelInput {
            enabled: true,
            answers: [(
                "other".to_owned(),
                ModelAnswer {
                    verdict: ModelVerdict::Finished,
                    call_id: None,
                },
            )]
            .into(),
        };
        assert_eq!(
            propose(&session(), &other).reason,
            ReasonCode::DefaultToResume
        );
        let unsure = propose(&session(), &model(true, ModelVerdict::Unsure));
        assert_eq!(
            of(unsure),
            (
                ModelVerdict::Unsure,
                ProposalSource::Model,
                ReasonCode::Model
            )
        );
        assert_eq!(unsure.definite(), None);
        assert_eq!(
            unsure.call_id,
            Some(7),
            "the call that answered travels with the proposal (D-63)"
        );
        assert_eq!(propose(&session(), &disabled).call_id, None);
        assert_eq!(
            propose(&session(), &model(true, ModelVerdict::ToResume)).definite(),
            Some(Verdict::ToResume)
        );
    }

    /// AD-40: a proposal is a function of the facts in, so a changed fact
    /// changes it at the next view build, with no stored proposal to
    /// update.
    #[test]
    fn proposals_are_recomputed_not_stored() {
        let mut session = background(LifecycleState::Done);
        let first = propose(&session, &ModelInput::default());
        assert_eq!(first, propose(&session, &ModelInput::default()));
        session.state = sourced(LifecycleState::Failed, 500);
        assert_eq!(
            propose(&session, &ModelInput::default()).reason,
            ReasonCode::StateFailed
        );
        session.kind = sourced(SessionKind::Interactive, 600);
        assert_eq!(
            propose(&session, &ModelInput::default()).reason,
            ReasonCode::DefaultToResume
        );
    }
}
