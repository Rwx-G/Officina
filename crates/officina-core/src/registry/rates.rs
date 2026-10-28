//! Measured error and the gates it opens (FR-11, FR-12 as amended by
//! amendments 9 and 10, AD-50, D-59, D-69, D-77): per proposal source, how
//! often its `finished` was flipped, how often any of its proposals was,
//! and its `unsure` apart; the auto-archive gate, the model enable gate and
//! the model self-disable test, each with its state. Read from the
//! confirmations; an auto confirmation counts once 7 days old, or once its
//! subject was reopened within them, which is a flip. The counts and rates
//! travel in the view (AD-5), each gate with the count it needs, so the
//! panel and the switches read the same numbers (AD-38).

use serde::{Deserialize, Serialize};

use super::dry_run::Evaluation;
use super::proposal::Proposal;
use super::types::{ModelVerdict, ProposalSource, Verdict};

const DAY_MS: i64 = 86_400_000;

/// Each rate rests on a source's last this many counted confirmations.
pub const RATE_WINDOW: usize = 50;

/// An auto confirmation counts once this old, as kept unless its subject
/// was reopened within it (AD-50).
pub const AUTO_COUNT_AFTER_MS: i64 = 7 * DAY_MS;

/// The auto-archive gate needs this many `state` `finished` cases (FR-12).
pub const AUTO_ARCHIVE_MIN_CASES: u32 = 50;

/// ... and fewer flips than this share of them, in percent (FR-12): with
/// [`AUTO_ARCHIVE_MIN_CASES`] cases, none (amendment 10).
pub const AUTO_ARCHIVE_FLIP_PERCENT: u32 = 2;

/// The model enable gate needs a dry run this recent (AD-50).
pub const MODEL_GATE_MAX_AGE_MS: i64 = 7 * DAY_MS;

/// ... on at least this many cases (AD-50).
pub const MODEL_GATE_MIN_CASES: u32 = 30;

/// The self-disable test judges the model once it has this many counted
/// confirmations among its last [`RATE_WINDOW`] (AD-50).
pub const SELF_DISABLE_MIN_CASES: u32 = 20;

/// A confirmation as the rates read it (AD-40, AD-49): the proposal it
/// answered, the verdict given, whether automation gave it, and when its
/// subject was reopened after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerdictRecord {
    pub id: i64,
    pub subject_id: i64,
    pub proposal: Proposal,
    pub verdict: Verdict,
    /// Given by auto-archive, not by the author (FR-12).
    pub auto: bool,
    pub confirmed_at_ms: i64,
    pub reopened_at_ms: Option<i64>,
}

/// How one confirmation counts at `now_ms`, when it counts yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Counted {
    flipped: bool,
    unsure: bool,
}

/// How `record` counts at `now_ms` (AD-50, D-59): an author's confirmation
/// at once, flipped when its verdict is not the one proposed, `unsure`
/// apart and never a flip; an auto confirmation once its subject was
/// reopened within [`AUTO_COUNT_AFTER_MS`], as a flip, or once that old,
/// as kept.
fn counted(record: &VerdictRecord, now_ms: i64) -> Option<Counted> {
    if record.auto {
        let reopened_within = record
            .reopened_at_ms
            .is_some_and(|at| at - record.confirmed_at_ms < AUTO_COUNT_AFTER_MS);
        let old_enough = now_ms - record.confirmed_at_ms >= AUTO_COUNT_AFTER_MS;
        return (reopened_within || old_enough).then_some(Counted {
            flipped: reopened_within,
            unsure: false,
        });
    }
    Some(match record.proposal.definite() {
        None => Counted {
            flipped: false,
            unsure: true,
        },
        Some(proposed) => Counted {
            flipped: proposed != record.verdict,
            unsure: false,
        },
    })
}

/// A count of cases and the flips among them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FlipCount {
    pub cases: u32,
    pub flipped: u32,
}

/// One proposal source's measured error (FR-11 as amended by amendment 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SourceRate {
    pub source: ProposalSource,
    /// Among its last [`RATE_WINDOW`] counted `finished` proposals: `None`
    /// when it has none ("no finished proposal yet").
    pub finished: Option<FlipCount>,
    /// Among its last [`RATE_WINDOW`] counted proposals of any verdict.
    pub all: FlipCount,
    /// Of those, the `unsure` ones, never flips (D-59).
    pub unsure: u32,
}

/// The source's counted confirmations at `now_ms`, the latest first.
fn counted_of(
    records: &[VerdictRecord],
    source: ProposalSource,
    now_ms: i64,
) -> Vec<(VerdictRecord, Counted)> {
    let mut found: Vec<(VerdictRecord, Counted)> = records
        .iter()
        .filter(|r| r.proposal.source == source)
        .filter_map(|r| Some((*r, counted(r, now_ms)?)))
        .collect();
    found.sort_by(|(a, _), (b, _)| {
        b.confirmed_at_ms
            .cmp(&a.confirmed_at_ms)
            .then_with(|| b.id.cmp(&a.id))
    });
    found
}

fn tally<'a>(counted: impl Iterator<Item = &'a (VerdictRecord, Counted)>) -> (FlipCount, u32) {
    let mut count = FlipCount::default();
    let mut unsure = 0;
    for (_, c) in counted.take(RATE_WINDOW) {
        count.cases += 1;
        count.flipped += u32::from(c.flipped);
        unsure += u32::from(c.unsure);
    }
    (count, unsure)
}

/// The measured error of `source` at `now_ms`.
pub fn source_rate(records: &[VerdictRecord], source: ProposalSource, now_ms: i64) -> SourceRate {
    let counted = counted_of(records, source, now_ms);
    let (finished, _) = tally(
        counted
            .iter()
            .filter(|(r, _)| r.proposal.verdict == ModelVerdict::Finished),
    );
    let (all, unsure) = tally(counted.iter());
    SourceRate {
        source,
        finished: (finished.cases > 0).then_some(finished),
        all,
        unsure,
    }
}

/// Every source's measured error at `now_ms`, in [`ProposalSource::ALL`]'s
/// order.
///
/// ```
/// use officina_core::registry::rates::rates;
/// use officina_core::registry::types::ProposalSource;
/// let none = rates(&[], 1_000);
/// assert_eq!(none.len(), ProposalSource::ALL.len());
/// assert!(none.iter().all(|r| r.finished.is_none()));
/// ```
pub fn rates(records: &[VerdictRecord], now_ms: i64) -> Vec<SourceRate> {
    ProposalSource::ALL
        .iter()
        .map(|source| source_rate(records, *source, now_ms))
        .collect()
}

closed_enum! {
    #[serde(rename_all = "snake_case")]
    /// Where a gate stands, and why (AD-50, D-69).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
    pub enum GateState {
        /// The gate holds: the switch may be on, or stays on.
        Holds,
        /// Fewer cases than the gate needs.
        NotEnoughCases,
        /// Too many of the cases were flipped.
        TooManyFlips,
        /// No dry run was recorded.
        NoDryRun,
        /// The latest dry run is too old.
        DryRunTooOld,
        /// The latest dry run ran too few cases.
        DryRunTooSmall,
        /// The latest dry run did not score more correct verdicts than the
        /// default.
        ModelNotBetter,
        /// The model turned itself off after the latest dry run: a newer
        /// one must win before it is turned on again (D-69).
        NeedsNewerDryRun,
        /// Over its recent confirmed proposals, the model was right less
        /// often than `to_resume` would have been.
        WorseThanDefault,
    }
}

/// A gate's state with the numbers it rests on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Gate {
    pub state: GateState,
    /// The cases it read.
    pub cases: u32,
    /// The cases it needs.
    pub needed: u32,
}

/// The three gates (AD-50).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Gates {
    pub auto_archive: Gate,
    pub model_enable: Gate,
    pub model_self_disable: Gate,
}

/// The auto-archive gate (FR-12, AD-50): the `state` source's counted
/// `finished` proposals, at least [`AUTO_ARCHIVE_MIN_CASES`], flipped less
/// often than [`AUTO_ARCHIVE_FLIP_PERCENT`] of them.
pub fn auto_archive_gate(records: &[VerdictRecord], now_ms: i64) -> Gate {
    let finished = source_rate(records, ProposalSource::State, now_ms)
        .finished
        .unwrap_or_default();
    let state = if finished.cases < AUTO_ARCHIVE_MIN_CASES {
        GateState::NotEnoughCases
    } else if finished.flipped * 100 >= AUTO_ARCHIVE_FLIP_PERCENT * finished.cases {
        GateState::TooManyFlips
    } else {
        GateState::Holds
    };
    Gate {
        state,
        cases: finished.cases,
        needed: AUTO_ARCHIVE_MIN_CASES,
    }
}

/// The model enable gate (FR-10 as amended by amendment 8, AD-50, D-69):
/// the latest dry run under [`MODEL_GATE_MAX_AGE_MS`] old, on at least
/// [`MODEL_GATE_MIN_CASES`] cases, scoring more correct verdicts than the
/// default, and newer than the model's last self-disable, when it turned
/// itself off at `self_disabled_at`.
pub fn model_enable_gate(
    latest: Option<&Evaluation>,
    self_disabled_at: Option<i64>,
    now_ms: i64,
) -> Gate {
    let cases = latest.map_or(0, |e| e.tally.cases);
    let state = match latest {
        None => GateState::NoDryRun,
        Some(e) if self_disabled_at.is_some_and(|off| e.at_ms <= off) => {
            GateState::NeedsNewerDryRun
        }
        Some(e) if now_ms - e.at_ms >= MODEL_GATE_MAX_AGE_MS => GateState::DryRunTooOld,
        Some(e) if e.tally.cases < MODEL_GATE_MIN_CASES => GateState::DryRunTooSmall,
        Some(e) if e.tally.model_correct <= e.tally.default_correct => GateState::ModelNotBetter,
        Some(_) => GateState::Holds,
    };
    Gate {
        state,
        cases,
        needed: MODEL_GATE_MIN_CASES,
    }
}

/// The model self-disable test (FR-10 as amended, AD-50): over the
/// model's last [`RATE_WINDOW`] counted proposals, once there are at least
/// [`SELF_DISABLE_MIN_CASES`], it fails when the model was right less often
/// than `to_resume` would have been; `unsure` is never right (D-59).
pub fn model_self_disable_gate(records: &[VerdictRecord], now_ms: i64) -> Gate {
    let counted = counted_of(records, ProposalSource::Model, now_ms);
    let window: Vec<&VerdictRecord> = counted.iter().take(RATE_WINDOW).map(|(r, _)| r).collect();
    let cases = u32::try_from(window.len()).unwrap_or(u32::MAX);
    let model_right = window
        .iter()
        .filter(|r| r.proposal.definite() == Some(r.verdict))
        .count();
    let default_right = window
        .iter()
        .filter(|r| r.verdict == Verdict::ToResume)
        .count();
    let state = if cases >= SELF_DISABLE_MIN_CASES && model_right < default_right {
        GateState::WorseThanDefault
    } else {
        GateState::Holds
    };
    Gate {
        state,
        cases,
        needed: SELF_DISABLE_MIN_CASES,
    }
}

/// The three gates at `now_ms`; `self_disabled_at` is when the model last
/// turned itself off, if it did.
pub fn gates(
    records: &[VerdictRecord],
    latest: Option<&Evaluation>,
    self_disabled_at: Option<i64>,
    now_ms: i64,
) -> Gates {
    Gates {
        auto_archive: auto_archive_gate(records, now_ms),
        model_enable: model_enable_gate(latest, self_disabled_at, now_ms),
        model_self_disable: model_self_disable_gate(records, now_ms),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::confirm::wake;
    use crate::registry::dry_run::Tally;
    use crate::registry::types::{
        HookEnd, ReasonCode, SessionRecord, Source, Sourced, SubjectRecord, SubjectStatus,
    };

    const NOW: i64 = 1_793_102_400_000;

    fn proposal(source: ProposalSource, verdict: ModelVerdict) -> Proposal {
        let reason = match (source, verdict) {
            (ProposalSource::State, ModelVerdict::Finished) => ReasonCode::StateDone,
            (ProposalSource::State, _) => ReasonCode::StateFailed,
            (ProposalSource::Model, _) => ReasonCode::Model,
            (ProposalSource::Rules, ModelVerdict::Finished) => ReasonCode::EmptySubject,
            (ProposalSource::Rules, _) => ReasonCode::DefaultToResume,
        };
        Proposal {
            verdict,
            source,
            reason,
            call_id: None,
        }
    }

    fn record(
        id: i64,
        source: ProposalSource,
        proposed: ModelVerdict,
        verdict: Verdict,
        at: i64,
    ) -> VerdictRecord {
        VerdictRecord {
            id,
            subject_id: id,
            proposal: proposal(source, proposed),
            verdict,
            auto: false,
            confirmed_at_ms: at,
            reopened_at_ms: None,
        }
    }

    fn state_finished(id: i64, verdict: Verdict, at: i64) -> VerdictRecord {
        record(
            id,
            ProposalSource::State,
            ModelVerdict::Finished,
            verdict,
            at,
        )
    }

    #[test]
    fn last_50_per_source() {
        // 60 kept `state` `finished` proposals, the oldest ten flipped: the
        // window holds the newest 50, none flipped.
        let mut records: Vec<VerdictRecord> = (0..60)
            .map(|i| {
                let verdict = if i < 10 {
                    Verdict::ToResume
                } else {
                    Verdict::Finished
                };
                state_finished(i, verdict, NOW - 1_000_000 + i * 1_000)
            })
            .collect();
        records.push(record(
            100,
            ProposalSource::Rules,
            ModelVerdict::ToResume,
            Verdict::Finished,
            NOW,
        ));
        let state = source_rate(&records, ProposalSource::State, NOW);
        assert_eq!(
            state.finished,
            Some(FlipCount {
                cases: 50,
                flipped: 0
            })
        );
        assert_eq!(state.all.cases, RATE_WINDOW as u32);
        let rules = source_rate(&records, ProposalSource::Rules, NOW);
        assert_eq!(
            rules.all,
            FlipCount {
                cases: 1,
                flipped: 1
            }
        );
        assert_eq!(auto_archive_gate(&records, NOW).state, GateState::Holds);
        // One more flip among the newest: the gate closes until 50 newer
        // unflipped cases exist (amendment 10).
        records.push(state_finished(200, Verdict::ToResume, NOW + 1));
        let gate = auto_archive_gate(&records, NOW + 1);
        assert_eq!((gate.state, gate.cases), (GateState::TooManyFlips, 50));
        assert_eq!(
            auto_archive_gate(&records[..40], NOW).state,
            GateState::NotEnoughCases
        );
    }

    #[test]
    fn no_finished_proposal_is_stated() {
        let records = [record(
            1,
            ProposalSource::Rules,
            ModelVerdict::ToResume,
            Verdict::ToResume,
            NOW,
        )];
        let all = rates(&records, NOW);
        let rules = all
            .iter()
            .find(|r| r.source == ProposalSource::Rules)
            .unwrap();
        assert_eq!(rules.finished, None, "no finished proposal yet");
        assert_eq!(rules.all.cases, 1);
        assert_eq!(
            all.iter().map(|r| r.source).collect::<Vec<_>>(),
            ProposalSource::ALL
        );
    }

    #[test]
    fn auto_confirmation_counts_after_7_days() {
        let auto = VerdictRecord {
            auto: true,
            ..state_finished(1, Verdict::Finished, NOW)
        };
        let at = |now| source_rate(&[auto], ProposalSource::State, now).finished;
        assert_eq!(at(NOW + AUTO_COUNT_AFTER_MS - 1), None, "not counted yet");
        assert_eq!(
            at(NOW + AUTO_COUNT_AFTER_MS),
            Some(FlipCount {
                cases: 1,
                flipped: 0
            })
        );
    }

    #[test]
    fn author_reopen_within_7_days_is_a_flip() {
        let reopened = |after| VerdictRecord {
            auto: true,
            reopened_at_ms: Some(NOW + after),
            ..state_finished(1, Verdict::Finished, NOW)
        };
        let finished = |record| {
            source_rate(
                &[record],
                ProposalSource::State,
                NOW + 2 * AUTO_COUNT_AFTER_MS,
            )
            .finished
        };
        assert_eq!(
            finished(reopened(AUTO_COUNT_AFTER_MS - 1)),
            Some(FlipCount {
                cases: 1,
                flipped: 1
            })
        );
        assert_eq!(
            finished(reopened(AUTO_COUNT_AFTER_MS)),
            Some(FlipCount {
                cases: 1,
                flipped: 0
            }),
            "reopened after its 7 days: kept"
        );
        // A flip counts at once, before the 7 days are over.
        assert_eq!(
            source_rate(&[reopened(60_000)], ProposalSource::State, NOW + 60_000).finished,
            Some(FlipCount {
                cases: 1,
                flipped: 1
            })
        );
    }

    /// D-77: a session of an auto-archived subject alive again stamps the
    /// reopen through `wake`, which the rate reads as a flip.
    #[test]
    fn alive_again_within_7_days_is_a_flip() {
        let subject = SubjectRecord {
            id: 1,
            project: "ops".into(),
            name: "nightly report".into(),
            status: Sourced {
                value: SubjectStatus::Archived,
                src: Source::Author,
                at_ms: NOW,
            },
            created_at_ms: NOW - 1_000,
            routine: false,
            merged_into: None,
        };
        let before = SessionRecord {
            session_id: "bg".into(),
            subject: Some(Sourced {
                value: Some(1),
                src: Source::Author,
                at_ms: NOW - 1_000,
            }),
            ended: Some(Sourced {
                value: HookEnd {
                    reason: Some("prompt_input_exit".into()),
                    claude_pid: None,
                },
                src: Source::Hook,
                at_ms: NOW - 500,
            }),
            ..SessionRecord::default()
        };
        let after = SessionRecord {
            last_prompt_at: Some(Sourced {
                value: NOW + 3_600_000,
                src: Source::Hook,
                at_ms: NOW + 3_600_000,
            }),
            ..before.clone()
        };
        let woken = wake(&before, &after, &[subject]).expect("woken");
        assert!(woken.stamp_reopen);
        let auto = VerdictRecord {
            auto: true,
            reopened_at_ms: woken.stamp_reopen.then_some(NOW + 3_600_000),
            ..state_finished(1, Verdict::Finished, NOW)
        };
        assert_eq!(
            source_rate(&[auto], ProposalSource::State, NOW + 3_600_000).finished,
            Some(FlipCount {
                cases: 1,
                flipped: 1
            })
        );
    }

    #[test]
    fn unsure_is_counted_apart() {
        let records = [
            record(
                1,
                ProposalSource::Model,
                ModelVerdict::Unsure,
                Verdict::Finished,
                NOW,
            ),
            record(
                2,
                ProposalSource::Model,
                ModelVerdict::Unsure,
                Verdict::ToResume,
                NOW,
            ),
            record(
                3,
                ProposalSource::Model,
                ModelVerdict::Finished,
                Verdict::ToResume,
                NOW,
            ),
        ];
        let model = source_rate(&records, ProposalSource::Model, NOW);
        assert_eq!(model.unsure, 2);
        assert_eq!(
            model.all,
            FlipCount {
                cases: 3,
                flipped: 1
            },
            "unsure is no flip"
        );
        assert_eq!(
            model.finished,
            Some(FlipCount {
                cases: 1,
                flipped: 1
            })
        );
    }

    #[test]
    fn model_gates_read_the_dry_run_and_the_model_rows() {
        let evaluation = |at_ms, cases, model_correct, default_correct| Evaluation {
            at_ms,
            tally: Tally {
                cases,
                model_correct,
                default_correct,
                ..Tally::default()
            },
        };
        let state = |e: Option<&Evaluation>| model_enable_gate(e, None, NOW).state;
        assert_eq!(state(None), GateState::NoDryRun);
        assert_eq!(
            state(Some(&evaluation(NOW - MODEL_GATE_MAX_AGE_MS, 40, 30, 20))),
            GateState::DryRunTooOld
        );
        assert_eq!(
            state(Some(&evaluation(NOW, MODEL_GATE_MIN_CASES - 1, 29, 1))),
            GateState::DryRunTooSmall
        );
        assert_eq!(
            state(Some(&evaluation(NOW, 40, 20, 20))),
            GateState::ModelNotBetter
        );
        assert_eq!(state(Some(&evaluation(NOW, 40, 21, 20))), GateState::Holds);
        // D-69: after a self-disable, the winning dry run must be newer.
        let winning = evaluation(NOW - 1_000, 40, 21, 20);
        assert_eq!(
            model_enable_gate(Some(&winning), Some(NOW - 1_000), NOW).state,
            GateState::NeedsNewerDryRun
        );
        assert_eq!(
            model_enable_gate(Some(&winning), Some(NOW - 2_000), NOW).state,
            GateState::Holds
        );

        // Self-disable: right less often than `to_resume` would have been,
        // judged from 20 cases on.
        let worse: Vec<VerdictRecord> = (0..SELF_DISABLE_MIN_CASES as i64)
            .map(|i| {
                record(
                    i,
                    ProposalSource::Model,
                    ModelVerdict::Finished,
                    Verdict::ToResume,
                    NOW - i,
                )
            })
            .collect();
        assert_eq!(
            model_self_disable_gate(&worse, NOW).state,
            GateState::WorseThanDefault
        );
        assert_eq!(
            model_self_disable_gate(&worse[1..], NOW).state,
            GateState::Holds,
            "too few to judge: kept"
        );
    }
}
