//! The use metrics of PRD section 7 (the Use, Auto and Delivery share
//! rows, P-25, D-139, D-142), counted per UTC day from the relay journal:
//! the executable form of the PRD's "how measured", so the numbers the QA
//! report gives come from here, never from a hand count.

use std::collections::BTreeMap;

use crate::dates::utc_day_text;
use crate::registry::types::LaunchOutcome;
use crate::relay::approvals::ApprovalBy;
use crate::relay::journal::{AnswerOutcome, AnswerSource, JournalEntry, JournalLine};

/// A count split by who wrote the answer (PRD section 9 item 3, AD-182):
/// the author's composer, or an answer proposal he sent with a click.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BySource {
    pub author: u32,
    pub proposal: u32,
}

impl BySource {
    fn add(&mut self, source: AnswerSource) {
        match source {
            AnswerSource::Author => self.author += 1,
            AnswerSource::Proposal => self.proposal += 1,
        }
    }

    /// Both sources together.
    pub fn total(&self) -> u32 {
        self.author + self.proposal
    }
}

/// What one UTC day of the journal holds for the use metrics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DayUse {
    /// `YYYY-MM-DD`, UTC.
    pub day: String,
    /// `work_order_launched` lines whose `claude --bg` started.
    pub work_orders_started: u32,
    /// Answers that left the meeting: a console started (a resume, or
    /// `claude attach` after the copy) or the text copied alone. The
    /// Delivery share's denominator (D-139): an answer delivered by copy
    /// and attach, or copied only, counts here and can never confirm.
    pub answers_sent: BySource,
    /// A resume's second line: the session was seen busy in the window.
    pub answers_confirmed: BySource,
    /// The author's `approval_given` and `approval_rejected` lines, on
    /// `human` and `auto` requests alike (`by` absent or `author`, so the
    /// lines written before v0.5 count): the Auto metric (D-142).
    pub author_decisions: u32,
    /// The same lines given by a review or by the rule, counted apart.
    pub auto_decisions: u32,
}

impl DayUse {
    /// The Use metric's day: at least one work order started or one answer
    /// sent.
    pub fn used(&self) -> bool {
        self.work_orders_started > 0 || self.answers_sent.total() > 0
    }
}

/// The use metrics per UTC day, oldest first; a day none of `lines` counts
/// in is absent.
pub fn per_day(lines: &[JournalLine]) -> Vec<DayUse> {
    fn of(days: &mut BTreeMap<String, DayUse>, ts: i64) -> &mut DayUse {
        let key = utc_day_text(ts);
        days.entry(key.clone()).or_insert_with(|| DayUse {
            day: key,
            ..DayUse::default()
        })
    }
    let mut days: BTreeMap<String, DayUse> = BTreeMap::new();
    for line in lines {
        match &line.entry {
            JournalEntry::WorkOrderLaunched(launched)
                if launched.outcome == LaunchOutcome::Started =>
            {
                of(&mut days, line.ts).work_orders_started += 1;
            }
            JournalEntry::MeetingAnswer(answer) => match answer.outcome {
                AnswerOutcome::Spawned | AnswerOutcome::Copied => {
                    of(&mut days, line.ts).answers_sent.add(answer.source);
                }
                AnswerOutcome::Confirmed => {
                    of(&mut days, line.ts).answers_confirmed.add(answer.source)
                }
                AnswerOutcome::SpawnFailed
                | AnswerOutcome::Refused
                | AnswerOutcome::NotConfirmed => {}
            },
            JournalEntry::ApprovalGiven(event) | JournalEntry::ApprovalRejected(event) => {
                match event.by.unwrap_or(ApprovalBy::Author) {
                    ApprovalBy::Author => of(&mut days, line.ts).author_decisions += 1,
                    ApprovalBy::Review | ApprovalBy::Rule => {
                        of(&mut days, line.ts).auto_decisions += 1
                    }
                }
            }
            _ => {}
        }
    }
    days.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dates::DAY_MS;
    use crate::meeting::Delivery;
    use crate::relay::journal::{ApprovalEvent, MeetingAnswer, WorkOrderLaunched};

    /// 2026-10-29T00:00:00Z.
    const DAY: i64 = 1_793_232_000_000;

    fn launched(outcome: LaunchOutcome) -> JournalEntry {
        JournalEntry::WorkOrderLaunched(WorkOrderLaunched {
            order_id: "c1".into(),
            name: "probe-notes-1".into(),
            template: "probe-notes".into(),
            project: "lab".into(),
            values: BTreeMap::new(),
            argv: Vec::new(),
            prompt_sha256: "0".repeat(64),
            catalogue_hash: "0".repeat(64),
            outcome,
            exit_code: None,
        })
    }

    fn answer(delivery: Delivery, outcome: AnswerOutcome, source: AnswerSource) -> JournalEntry {
        JournalEntry::MeetingAnswer(MeetingAnswer {
            session_id: Some("s".into()),
            delivery,
            answer_sha256: "0".repeat(64),
            outcome,
            source,
            proposal_id: (source == AnswerSource::Proposal).then(|| "p1".to_owned()),
        })
    }

    fn decision(by: Option<ApprovalBy>, given: bool) -> JournalEntry {
        let event = ApprovalEvent {
            approval_id: "a".into(),
            request_key: "k".into(),
            project: Some("lab".into()),
            action: Some("apply".into()),
            dry_run: None,
            by,
            reason: None,
        };
        if given {
            JournalEntry::ApprovalGiven(event)
        } else {
            JournalEntry::ApprovalRejected(event)
        }
    }

    fn at(ts: i64, entry: JournalEntry) -> JournalLine {
        JournalLine::new(ts, entry)
    }

    #[test]
    fn days_are_utc() {
        assert_eq!(utc_day_text(DAY), "2026-10-29");
        let lines = [
            at(DAY - 1, launched(LaunchOutcome::Started)),
            at(DAY, launched(LaunchOutcome::Started)),
            at(DAY + DAY_MS - 1, launched(LaunchOutcome::Started)),
            at(DAY + DAY_MS, launched(LaunchOutcome::Started)),
            // Only a started order counts.
            at(DAY, launched(LaunchOutcome::Failed)),
            at(DAY, launched(LaunchOutcome::UntrustedWorkspace)),
        ];
        let days = per_day(&lines);
        let counts: Vec<(&str, u32)> = days
            .iter()
            .map(|d| (d.day.as_str(), d.work_orders_started))
            .collect();
        assert_eq!(
            counts,
            [("2026-10-28", 1), ("2026-10-29", 2), ("2026-10-30", 1)]
        );
        assert!(days.iter().all(DayUse::used));
        assert!(per_day(&[at(DAY, launched(LaunchOutcome::Failed))]).is_empty());
    }

    #[test]
    fn author_decisions_exclude_review_and_rule() {
        let lines = [
            // A line written before v0.5 carries no `by`: the author's.
            at(DAY, decision(None, true)),
            at(DAY, decision(Some(ApprovalBy::Author), false)),
            at(DAY, decision(Some(ApprovalBy::Review), true)),
            at(DAY, decision(Some(ApprovalBy::Rule), true)),
            at(DAY, decision(Some(ApprovalBy::Review), false)),
        ];
        let days = per_day(&lines);
        assert_eq!(days.len(), 1);
        assert_eq!(days[0].author_decisions, 2);
        assert_eq!(days[0].auto_decisions, 3);
        assert!(!days[0].used(), "a decision is not a day of use");
    }

    #[test]
    fn delivery_share_counts_copy_and_attach_in_the_denominator() {
        let author = AnswerSource::Author;
        let lines = [
            at(
                DAY,
                answer(Delivery::ResumeLive, AnswerOutcome::Spawned, author),
            ),
            at(
                DAY,
                answer(Delivery::ResumeLive, AnswerOutcome::Confirmed, author),
            ),
            at(
                DAY,
                answer(Delivery::ResumeEnded, AnswerOutcome::Spawned, author),
            ),
            at(
                DAY,
                answer(Delivery::ResumeEnded, AnswerOutcome::NotConfirmed, author),
            ),
            at(
                DAY,
                answer(Delivery::CopyAndAttach, AnswerOutcome::Spawned, author),
            ),
            at(
                DAY,
                answer(Delivery::CopyOnly, AnswerOutcome::Copied, author),
            ),
            // Nothing left the meeting: neither sent nor confirmed.
            at(
                DAY,
                answer(Delivery::ResumeLive, AnswerOutcome::Refused, author),
            ),
            at(
                DAY,
                answer(Delivery::ResumeLive, AnswerOutcome::SpawnFailed, author),
            ),
        ];
        let days = per_day(&lines);
        assert_eq!(days.len(), 1);
        assert_eq!(days[0].answers_sent.author, 4);
        assert_eq!(days[0].answers_confirmed.author, 1);
        assert!(days[0].used());
    }

    #[test]
    fn proposals_are_counted_apart_from_the_authors_answers() {
        let lines = [
            at(
                DAY,
                answer(
                    Delivery::ResumeLive,
                    AnswerOutcome::Spawned,
                    AnswerSource::Author,
                ),
            ),
            at(
                DAY,
                answer(
                    Delivery::ResumeLive,
                    AnswerOutcome::Spawned,
                    AnswerSource::Proposal,
                ),
            ),
            at(
                DAY,
                answer(
                    Delivery::ResumeLive,
                    AnswerOutcome::Confirmed,
                    AnswerSource::Proposal,
                ),
            ),
            at(
                DAY,
                answer(
                    Delivery::CopyOnly,
                    AnswerOutcome::Copied,
                    AnswerSource::Proposal,
                ),
            ),
        ];
        let days = per_day(&lines);
        assert_eq!(
            days[0].answers_sent,
            BySource {
                author: 1,
                proposal: 2
            }
        );
        assert_eq!(
            days[0].answers_confirmed,
            BySource {
                author: 0,
                proposal: 1
            }
        );
    }
}
