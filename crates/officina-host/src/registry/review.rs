//! `auto_review` rows (AD-82, AD-86 to AD-88, D-133, D-142, D-143): one row
//! per reviewed or escalated `auto` request, the rows the cap and the budget
//! count, and the author's decision copied onto the row once he decides;
//! the `auto_approval` and `auto_mode` settings, turned on only while the
//! shadow statistics hold, and the "should not have run" mark that turns
//! approval off. The window writes them (`window-authority`); doctor and
//! `officina relay --json` read them through a read-only [`Reader`].

use officina_core::registry::types::{
    AuthorDecision, AuthorReason, ReviewMode, ReviewOutcome, SettingKey, Source,
};
use officina_core::relay::auto::{
    AutoApprovalSetting, AutoMode, EscalationCause, ReviewDecision, ReviewRecord,
};
use officina_core::relay::view::AutoRow;
use officina_core::untrusted::Untrusted;
use rusqlite::Connection;

use super::read::{Reader, setting_of};

/// One review row as the window writes it (AD-82): the request, the mode,
/// how the review ended, and what the call cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewReview {
    pub approval_id: String,
    pub request_key: String,
    pub project: String,
    pub action: String,
    pub requested_at_ms: i64,
    pub mode: ReviewMode,
    pub outcome: ReviewOutcome,
    pub cause: Option<EscalationCause>,
    pub decision: Option<ReviewDecision>,
    /// The review's reason: model text, a parse record's plain string here
    /// (D-130), read back as `Untrusted` ([`ReviewRow::reason`]).
    pub reason: Option<String>,
    pub rule_baseline: bool,
    pub call_started_at_ms: Option<i64>,
    pub duration_ms: Option<i64>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cost_microusd: Option<i64>,
}

/// A row whose request the author has not decided yet, as the copy of his
/// decision reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndecidedReview {
    pub id: i64,
    pub approval_id: String,
}

/// One row as the window's views and doctor read it back (AD-88).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewRow {
    pub id: i64,
    pub approval_id: String,
    pub project: String,
    pub action: String,
    pub requested_at_ms: i64,
    pub mode: ReviewMode,
    pub outcome: ReviewOutcome,
    /// A cause no variant of this build parses reads `None` (D-133).
    pub cause: Option<EscalationCause>,
    pub decision: Option<ReviewDecision>,
    pub reason: Option<Untrusted<String>>,
    pub rule_baseline: bool,
    pub call_started_at_ms: Option<i64>,
    pub cost_microusd: Option<i64>,
    pub author_decision: Option<AuthorDecision>,
    pub author_reason: Option<AuthorReason>,
    pub decided_at_ms: Option<i64>,
    /// The run its approval started, linked through
    /// `run_started.approval_id` (AD-88).
    pub run_id: Option<String>,
    /// When the author marked that run "should not have run".
    pub marked_at_ms: Option<i64>,
}

impl ReviewRow {
    /// The row as the cap and the budget count it.
    pub fn record(&self) -> ReviewRecord {
        ReviewRecord {
            project: self.project.clone(),
            action: self.action.clone(),
            requested_at_ms: self.requested_at_ms,
            outcome: self.outcome,
            decision: self.decision,
            call_started_at_ms: self.call_started_at_ms,
        }
    }

    /// The row as the relay view reads it; its reason only when
    /// `with_reason`: `officina relay --json`, which every session can run,
    /// never carries one (D-143, review F17).
    pub fn auto_row(&self, with_reason: bool) -> AutoRow {
        AutoRow {
            approval_id: self.approval_id.clone(),
            project: self.project.clone(),
            action: self.action.clone(),
            requested_at_ms: self.requested_at_ms,
            mode: self.mode,
            outcome: self.outcome,
            cause: self.cause,
            decision: self.decision,
            reason: self.reason.clone().filter(|_| with_reason),
            rule_baseline: self.rule_baseline,
            call_started_at_ms: self.call_started_at_ms,
            author_decision: self.author_decision,
            author_reason: self.author_reason,
            run_id: self.run_id.clone(),
            marked_at_ms: self.marked_at_ms,
        }
    }
}

fn parse<T: Copy>(all: &[T], code: fn(T) -> &'static str, text: Option<String>) -> Option<T> {
    let text = text?;
    all.iter().copied().find(|v| code(*v) == text)
}

/// Every row requested, whose call started, or marked since `since_ms`,
/// newest first.
pub(crate) fn review_rows(conn: &Connection, since_ms: i64) -> rusqlite::Result<Vec<ReviewRow>> {
    conn.prepare_cached(
        "SELECT id, approval_id, project, action, requested_at_ms, mode, outcome, cause,
         decision, reason, rule_baseline, call_started_at_ms, cost_microusd,
         author_decision, author_reason, decided_at_ms, run_id, marked_at_ms
         FROM auto_review
         WHERE requested_at_ms >= ?1 OR call_started_at_ms >= ?1 OR marked_at_ms >= ?1
         ORDER BY requested_at_ms DESC, id DESC",
    )?
    .query_map([since_ms], |row| {
        let mode: String = row.get(5)?;
        let outcome: String = row.get(6)?;
        Ok(ReviewRow {
            id: row.get(0)?,
            approval_id: row.get(1)?,
            project: row.get(2)?,
            action: row.get(3)?,
            requested_at_ms: row.get(4)?,
            mode: parse(ReviewMode::ALL, ReviewMode::code, Some(mode))
                .unwrap_or(ReviewMode::Shadow),
            outcome: parse(ReviewOutcome::ALL, ReviewOutcome::code, Some(outcome))
                .unwrap_or(ReviewOutcome::NoCall),
            cause: parse(EscalationCause::ALL, EscalationCause::code, row.get(7)?),
            decision: parse(ReviewDecision::ALL, ReviewDecision::code, row.get(8)?),
            reason: row.get::<_, Option<String>>(9)?.map(Untrusted::new),
            rule_baseline: row.get(10)?,
            call_started_at_ms: row.get(11)?,
            cost_microusd: row.get(12)?,
            author_decision: parse(AuthorDecision::ALL, AuthorDecision::code, row.get(13)?),
            author_reason: parse(AuthorReason::ALL, AuthorReason::code, row.get(14)?),
            decided_at_ms: row.get(15)?,
            run_id: row.get(16)?,
            marked_at_ms: row.get(17)?,
        })
    })?
    .collect()
}

/// Auto approval and its mode as the registry holds them (AD-88, D-162).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AutoState {
    pub approval: AutoApprovalSetting,
    /// Who last set approval; `None` before anyone did.
    pub approval_src: Option<Source>,
    /// [`AutoMode::Review`] until the author chose another.
    pub mode: AutoMode,
}

impl AutoState {
    /// True while a review or the rule may approve.
    pub fn approval_on(&self) -> bool {
        self.approval.on
    }
}

/// The settings `auto_approval` and `auto_mode`; a value this build cannot
/// read counts as absent: approval off, mode review.
pub(crate) fn auto_state(conn: &Connection) -> rusqlite::Result<AutoState> {
    let approval = setting_of::<AutoApprovalSetting>(conn, SettingKey::AutoApproval)?;
    let mode = setting_of::<AutoMode>(conn, SettingKey::AutoMode)?;
    Ok(AutoState {
        approval: approval.as_ref().map(|s| s.value).unwrap_or_default(),
        approval_src: approval.map(|s| s.src),
        mode: mode.map(|s| s.value).unwrap_or_default(),
    })
}

impl Reader {
    /// Every review row requested, whose call started, or marked since
    /// `since_ms`, newest first.
    pub fn review_rows(&self, since_ms: i64) -> rusqlite::Result<Vec<ReviewRow>> {
        review_rows(self.conn(), since_ms)
    }

    /// Auto approval and its mode.
    pub fn auto_state(&self) -> rusqlite::Result<AutoState> {
        auto_state(self.conn())
    }
}

#[cfg(feature = "window-authority")]
mod writer {
    use std::collections::BTreeSet;

    use officina_core::relay::auto::{
        AutoOffReason, MARK_WINDOW_MS, MarkRefusal, STATS_WINDOW_MS, ShadowStats, ToggleRefusal,
        shadow_stats,
    };
    use rusqlite::{OptionalExtension, TransactionBehavior, params};

    use super::*;
    use crate::registry::store::Writer;
    use crate::registry::write::{RegistryError, setting_json, write_setting};

    /// The shadow statistics on `conn` at `now_ms` (AD-88).
    fn stats_on(conn: &Connection, now_ms: i64) -> rusqlite::Result<ShadowStats> {
        let rows: Vec<_> = review_rows(conn, now_ms - STATS_WINDOW_MS)?
            .iter()
            .map(|row| row.auto_row(false).stats())
            .collect();
        Ok(shadow_stats(&rows, now_ms))
    }

    /// Writes approval `on` with `reason` and, when given, `mode`, source
    /// `author`.
    fn write_state(
        conn: &Connection,
        on: bool,
        reason: Option<AutoOffReason>,
        mode: Option<AutoMode>,
        now_ms: i64,
    ) -> rusqlite::Result<()> {
        let approval = setting_json(&AutoApprovalSetting { on, reason });
        write_setting(
            conn,
            SettingKey::AutoApproval,
            &approval,
            Source::Author,
            now_ms,
        )?;
        if let Some(mode) = mode {
            write_setting(
                conn,
                SettingKey::AutoMode,
                &setting_json(&mode),
                Source::Author,
                now_ms,
            )?;
        }
        Ok(())
    }

    impl Writer {
        /// Inserts one review row and returns its id.
        pub fn insert_review(&mut self, review: &NewReview) -> Result<i64, RegistryError> {
            self.conn
                .prepare_cached(
                    "INSERT INTO auto_review (approval_id, request_key, project, action,
                     requested_at_ms, mode, outcome, cause, decision, reason, rule_baseline,
                     call_started_at_ms, duration_ms, input_tokens, output_tokens, cost_microusd)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                )?
                .execute(params![
                    review.approval_id,
                    review.request_key,
                    review.project,
                    review.action,
                    review.requested_at_ms,
                    review.mode.code(),
                    review.outcome.code(),
                    review.cause.map(EscalationCause::code),
                    review.decision.map(ReviewDecision::code),
                    review.reason,
                    review.rule_baseline,
                    review.call_started_at_ms,
                    review.duration_ms,
                    review.input_tokens,
                    review.output_tokens,
                    review.cost_microusd,
                ])?;
            Ok(self.conn.last_insert_rowid())
        }

        /// Sets the cause of row `id`: an approval the authority refused
        /// escalates with it (D-734).
        pub fn set_review_cause(
            &mut self,
            id: i64,
            cause: EscalationCause,
        ) -> Result<(), RegistryError> {
            self.conn
                .prepare_cached("UPDATE auto_review SET cause = ?2 WHERE id = ?1")?
                .execute(params![id, cause.code()])?;
            Ok(())
        }

        /// The approval ids that have a row among the requests made since
        /// `since_ms`: one review per request.
        pub fn reviewed_requests(&self, since_ms: i64) -> Result<BTreeSet<String>, RegistryError> {
            let ids = self
                .conn
                .prepare_cached("SELECT approval_id FROM auto_review WHERE requested_at_ms >= ?1")?
                .query_map([since_ms], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(ids)
        }

        /// Every review row requested, whose call started, or marked since
        /// `since_ms`, newest first.
        pub fn review_rows(&self, since_ms: i64) -> Result<Vec<ReviewRow>, RegistryError> {
            Ok(review_rows(&self.conn, since_ms)?)
        }

        /// The rows whose request the author has not decided, requested
        /// since `since_ms`.
        pub fn undecided_reviews(
            &self,
            since_ms: i64,
        ) -> Result<Vec<UndecidedReview>, RegistryError> {
            let rows = self
                .conn
                .prepare_cached(
                    "SELECT id, approval_id FROM auto_review
                     WHERE author_decision IS NULL AND requested_at_ms >= ?1 ORDER BY id",
                )?
                .query_map([since_ms], |row| {
                    Ok(UndecidedReview {
                        id: row.get(0)?,
                        approval_id: row.get(1)?,
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
            Ok(rows)
        }

        /// Copies the author's decision onto row `id` (AD-88): approve, or
        /// reject with its reason, at `decided_at_ms`. A row already decided
        /// is left as it is; returns whether it was written.
        pub fn copy_author_decision(
            &mut self,
            id: i64,
            decision: AuthorDecision,
            reason: Option<AuthorReason>,
            decided_at_ms: i64,
        ) -> Result<bool, RegistryError> {
            Ok(self
                .conn
                .prepare_cached(
                    "UPDATE auto_review
                     SET author_decision = ?2, author_reason = ?3, decided_at_ms = ?4
                     WHERE id = ?1 AND author_decision IS NULL",
                )?
                .execute(params![
                    id,
                    decision.code(),
                    reason.map(AuthorReason::code),
                    decided_at_ms
                ])?
                == 1)
        }

        /// The approval ids a review or the rule approved since `since_ms`
        /// whose run is not linked yet (AD-88).
        pub fn unlinked_approvals(&self, since_ms: i64) -> Result<BTreeSet<String>, RegistryError> {
            let ids = self
                .conn
                .prepare_cached(
                    "SELECT approval_id FROM auto_review
                     WHERE requested_at_ms >= ?1 AND run_id IS NULL AND mode IN ('live', 'rule')
                     AND decision = 'approve' AND cause IS NULL",
                )?
                .query_map([since_ms], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(ids)
        }

        /// Links each `(approval_id, run_id)` started by `officina run` to
        /// the approval's row, unless it is linked already (AD-88);
        /// returns how many rows it linked.
        pub fn link_runs(&mut self, runs: &[(String, String)]) -> Result<usize, RegistryError> {
            let tx = self
                .conn
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            let mut linked = 0;
            for (approval_id, run_id) in runs {
                linked += tx
                    .prepare_cached(
                        "UPDATE auto_review SET run_id = ?2
                         WHERE approval_id = ?1 AND run_id IS NULL AND decision = 'approve'
                         AND mode IN ('live', 'rule') AND cause IS NULL",
                    )?
                    .execute(params![approval_id, run_id])?;
            }
            tx.commit()?;
            Ok(linked)
        }

        /// Auto approval and its mode.
        pub fn auto_state(&self) -> Result<AutoState, RegistryError> {
            Ok(auto_state(&self.conn)?)
        }

        /// The shadow statistics at `now_ms` (AD-88).
        pub fn shadow_stats(&self, now_ms: i64) -> Result<ShadowStats, RegistryError> {
            Ok(stats_on(&self.conn, now_ms)?)
        }

        /// Turns auto approval on or off at `now_ms`, source `author`
        /// (AD-88, D-69's pattern): on is refused unless the approval gate
        /// holds when it writes, judged in the write's transaction; off is
        /// always allowed. Returns the state written.
        pub fn set_auto_approval(
            &mut self,
            on: bool,
            now_ms: i64,
        ) -> Result<Result<AutoState, ToggleRefusal>, RegistryError> {
            let tx = self
                .conn
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            if on && !stats_on(&tx, now_ms)?.gate {
                return Ok(Err(ToggleRefusal::GateDoesNotHold));
            }
            write_state(&tx, on, None, None, now_ms)?;
            let state = auto_state(&tx)?;
            tx.commit()?;
            Ok(Ok(state))
        }

        /// Sets the mode at `now_ms`, source `author` (D-162): `rule` is
        /// refused unless the rule offer holds when it writes; `review` is
        /// always allowed. Returns the state written.
        pub fn set_auto_mode(
            &mut self,
            mode: AutoMode,
            now_ms: i64,
        ) -> Result<Result<AutoState, ToggleRefusal>, RegistryError> {
            let tx = self
                .conn
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            if mode == AutoMode::Rule && !stats_on(&tx, now_ms)?.rule_offer {
                return Ok(Err(ToggleRefusal::NoRuleOffer));
            }
            write_setting(
                &tx,
                SettingKey::AutoMode,
                &setting_json(&mode),
                Source::Author,
                now_ms,
            )?;
            let state = auto_state(&tx)?;
            tx.commit()?;
            Ok(Ok(state))
        }

        /// Marks run `run_id` "should not have run" at `now_ms` (AD-88,
        /// FR-9, D-141): only a run a review or the rule approved in the
        /// last [`MARK_WINDOW_MS`], once; stamps its row, which counts as a
        /// disagreement for 30 days, turns auto approval off with the
        /// reason and sets the mode back to `review`, in one transaction.
        /// Returns the state written.
        pub fn mark_should_not_have_run(
            &mut self,
            run_id: &str,
            now_ms: i64,
        ) -> Result<Result<AutoState, MarkRefusal>, RegistryError> {
            let tx = self
                .conn
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            let found: Option<(i64, Option<i64>)> = tx
                .prepare_cached(
                    "SELECT id, marked_at_ms FROM auto_review
                     WHERE run_id = ?1 AND mode IN ('live', 'rule') AND decision = 'approve'
                     AND cause IS NULL AND requested_at_ms >= ?2
                     ORDER BY id DESC LIMIT 1",
                )?
                .query_row(params![run_id, now_ms - MARK_WINDOW_MS], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })
                .optional()?;
            let id = match found {
                None => return Ok(Err(MarkRefusal::NotAutoApproved(run_id.to_owned()))),
                Some((_, Some(_))) => {
                    return Ok(Err(MarkRefusal::AlreadyMarked(run_id.to_owned())));
                }
                Some((id, None)) => id,
            };
            tx.prepare_cached("UPDATE auto_review SET marked_at_ms = ?2 WHERE id = ?1")?
                .execute(params![id, now_ms])?;
            write_state(
                &tx,
                false,
                Some(AutoOffReason::ShouldNotHaveRun),
                Some(AutoMode::Review),
                now_ms,
            )?;
            let state = auto_state(&tx)?;
            tx.commit()?;
            Ok(Ok(state))
        }
    }
}
