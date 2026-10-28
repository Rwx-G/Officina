//! The dry run (AD-52, D-59, D-67, D-85, D-88): the disabled model scored
//! on the author's own past confirmations, never shown as a proposal. The
//! cases are the confirmations whose proposal was the default and whose
//! input is still held, newest first, at most [`DRY_RUN_MAX_CASES`]; each
//! is one call; the tally compares the model's verdict and the default's
//! with the author's, and is all a dry run records.

use serde::{Deserialize, Serialize};

use super::confirm::ConfirmationInput;
use super::proposal::{Proposal, default_proposal};
use super::triage_call::{CallUsage, TriageCase};
use super::types::{ModelVerdict, Verdict};

/// The most cases one dry run runs (AD-52).
pub const DRY_RUN_MAX_CASES: usize = 50;

/// A confirmation as the dry run reads it: the proposal it answered, the
/// author's verdict, and the input it keeps, while it keeps one (D-85).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluableConfirmation {
    pub id: i64,
    pub confirmed_at_ms: i64,
    pub proposal: Proposal,
    pub verdict: Verdict,
    pub input: Option<ConfirmationInput>,
}

/// One case of a dry run: the confirmation it comes from, the author's
/// verdict, and what is sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DryRunCase {
    pub confirmation_id: i64,
    pub verdict: Verdict,
    pub case: TriageCase,
    /// When the input it copies is due to go (D-85): a case still queued
    /// then is never sent.
    pub expires_at_ms: i64,
}

impl DryRunCase {
    /// The case's key in `triage_call.case_key`.
    pub fn key(&self) -> String {
        format!("confirmation:{}", self.confirmation_id)
    }
}

/// The cases of a dry run at `now_ms` (AD-52, D-85): the confirmations
/// whose proposal was the default and whose input has not expired, the
/// newest first, at most [`DRY_RUN_MAX_CASES`].
///
/// ```
/// use officina_core::registry::dry_run::dry_run_cases;
/// assert!(dry_run_cases(&[], 1_000).is_empty());
/// ```
pub fn dry_run_cases(confirmations: &[EvaluableConfirmation], now_ms: i64) -> Vec<DryRunCase> {
    let default = default_proposal();
    let mut held: Vec<&EvaluableConfirmation> = confirmations
        .iter()
        .filter(|c| c.proposal == default)
        .filter(|c| c.input.as_ref().is_some_and(|i| i.expires_at_ms > now_ms))
        .collect();
    held.sort_by(|a, b| {
        b.confirmed_at_ms
            .cmp(&a.confirmed_at_ms)
            .then_with(|| b.id.cmp(&a.id))
    });
    held.into_iter()
        .take(DRY_RUN_MAX_CASES)
        .filter_map(|c| {
            let input = c.input.as_ref()?;
            Some(DryRunCase {
                confirmation_id: c.id,
                verdict: c.verdict,
                case: TriageCase::from(input),
                expires_at_ms: input.expires_at_ms,
            })
        })
        .collect()
}

/// What a dry run counts (AD-52, D-59, D-67, D-88): every case it ran,
/// the model's and the default's correct verdicts, the model's `finished`
/// the author reversed, the cases with no answer, and what the calls cost.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Tally {
    pub cases: u32,
    pub model_correct: u32,
    pub default_correct: u32,
    pub model_false_finished: u32,
    pub unanswered: u32,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub input_tokens: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub output_tokens: Option<i64>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub cost_microusd: Option<i64>,
}

fn add(sum: Option<i64>, more: Option<i64>) -> Option<i64> {
    match (sum, more) {
        (None, None) => None,
        (sum, more) => Some(sum.unwrap_or(0) + more.unwrap_or(0)),
    }
}

impl Tally {
    /// Counts one case the author decided `truth`, which the model answered
    /// `answer` (`None` when the call gave none): `unsure` is never correct
    /// (D-59), an unanswered case is not correct for the model (D-67), and
    /// the default's verdict is scored on every case.
    ///
    /// ```
    /// use officina_core::registry::dry_run::Tally;
    /// use officina_core::registry::triage_call::CallUsage;
    /// use officina_core::registry::types::{ModelVerdict, Verdict};
    /// let mut tally = Tally::default();
    /// tally.add(Verdict::Finished, Some(ModelVerdict::Finished), &CallUsage::default());
    /// tally.add(Verdict::ToResume, None, &CallUsage::default());
    /// assert_eq!((tally.cases, tally.model_correct, tally.default_correct, tally.unanswered), (2, 1, 1, 1));
    /// ```
    pub fn add(&mut self, truth: Verdict, answer: Option<ModelVerdict>, usage: &CallUsage) {
        self.cases += 1;
        let as_verdict = |proposed: ModelVerdict| match proposed {
            ModelVerdict::Finished => Some(Verdict::Finished),
            ModelVerdict::ToResume => Some(Verdict::ToResume),
            ModelVerdict::Unsure => None,
        };
        if as_verdict(default_proposal().verdict) == Some(truth) {
            self.default_correct += 1;
        }
        match answer {
            None => self.unanswered += 1,
            Some(proposed) => {
                if as_verdict(proposed) == Some(truth) {
                    self.model_correct += 1;
                }
                if proposed == ModelVerdict::Finished && truth != Verdict::Finished {
                    self.model_false_finished += 1;
                }
            }
        }
        self.input_tokens = add(self.input_tokens, usage.input_tokens);
        self.output_tokens = add(self.output_tokens, usage.output_tokens);
        self.cost_microusd = add(self.cost_microusd, usage.cost_microusd);
    }
}

/// One `model_evaluation` row: a finished dry run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Evaluation {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub at_ms: i64,
    pub tally: Tally,
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// How the last dry run of this window ended (D-67).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum DryRunStop {
        /// Every case ran.
        Completed,
        /// The budget was spent: what ran is recorded.
        BudgetSpent,
        /// A call was refused before it started: the cause is listed with
        /// the refusals, and what ran is recorded.
        Refused,
    }
}

/// A dry run while it runs (D-92): how far it got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DryRunProgress {
    pub done: u32,
    pub total: u32,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub started_at_ms: i64,
}

/// What the window's collector knows of dry runs and the registry does not
/// hold: the one running, and how the last one ended.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DryRunState {
    pub running: Option<DryRunProgress>,
    pub last_stop: Option<DryRunStop>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::types::{ProposalSource, ReasonCode};

    const NOW: i64 = 1_793_102_400_000;

    fn confirmation(
        id: i64,
        at: i64,
        proposal: Proposal,
        expires: Option<i64>,
    ) -> EvaluableConfirmation {
        EvaluableConfirmation {
            id,
            confirmed_at_ms: at,
            proposal,
            verdict: Verdict::Finished,
            input: expires.map(|expires_at_ms| ConfirmationInput {
                prompt: Some(format!("prompt {id}")),
                answer_head: None,
                answer_tail: None,
                end_reason: Some("prompt_input_exit".into()),
                cut_mid_turn: false,
                expires_at_ms,
            }),
        }
    }

    #[test]
    fn cases_are_held_default_confirmations_newest_first() {
        let state = Proposal {
            verdict: ModelVerdict::Finished,
            source: ProposalSource::State,
            reason: ReasonCode::StateDone,
            call_id: None,
        };
        let held = |id, at| confirmation(id, at, default_proposal(), Some(NOW + 1));
        let mut all: Vec<EvaluableConfirmation> = (0..DRY_RUN_MAX_CASES as i64 + 5)
            .map(|i| held(i + 10, NOW - 1_000 + i))
            .collect();
        all.push(confirmation(1, NOW, state, Some(NOW + 1)));
        all.push(confirmation(2, NOW, default_proposal(), Some(NOW)));
        all.push(confirmation(3, NOW, default_proposal(), None));
        let cases = dry_run_cases(&all, NOW);
        assert_eq!(cases.len(), DRY_RUN_MAX_CASES);
        assert_eq!(
            cases[0].confirmation_id,
            DRY_RUN_MAX_CASES as i64 + 14,
            "newest first"
        );
        assert!(cases.iter().all(|c| c.confirmation_id >= 10));
        assert_eq!(cases[0].case.last_prompt.as_deref(), Some("prompt 64"));
        assert_eq!(cases[0].key(), "confirmation:64");
    }

    #[test]
    fn unsure_is_never_correct_and_a_false_finished_is_counted() {
        let mut tally = Tally::default();
        let usage = CallUsage {
            input_tokens: Some(4_500),
            output_tokens: None,
            cost_microusd: Some(1_100),
        };
        tally.add(Verdict::ToResume, Some(ModelVerdict::Unsure), &usage);
        tally.add(Verdict::ToResume, Some(ModelVerdict::Finished), &usage);
        tally.add(
            Verdict::Finished,
            Some(ModelVerdict::Finished),
            &CallUsage::default(),
        );
        assert_eq!(
            (
                tally.cases,
                tally.model_correct,
                tally.default_correct,
                tally.model_false_finished,
                tally.unanswered
            ),
            (3, 1, 2, 1, 0)
        );
        assert_eq!(
            (tally.input_tokens, tally.output_tokens, tally.cost_microusd),
            (Some(9_000), None, Some(2_200))
        );
    }
}
