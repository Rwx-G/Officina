//! `auto_review` rows (AD-82, AD-86 to AD-88, D-133, D-142, D-143): one row
//! per reviewed or escalated `auto` request, the rows the cap and the budget
//! count, and the author's decision copied onto the row once he decides.
//! The window writes them (`window-authority`); doctor reads them through a
//! read-only [`Reader`].

use officina_core::registry::types::{AuthorDecision, AuthorReason, ReviewMode, ReviewOutcome};
use officina_core::relay::auto::{EscalationCause, ReviewDecision, ReviewRecord};
use officina_core::untrusted::Untrusted;
use rusqlite::Connection;

use super::read::Reader;

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
}

fn parse<T: Copy>(all: &[T], code: fn(T) -> &'static str, text: Option<String>) -> Option<T> {
    let text = text?;
    all.iter().copied().find(|v| code(*v) == text)
}

/// Every row requested, or whose call started, since `since_ms`, newest
/// first.
pub(crate) fn review_rows(conn: &Connection, since_ms: i64) -> rusqlite::Result<Vec<ReviewRow>> {
    conn.prepare_cached(
        "SELECT id, approval_id, project, action, requested_at_ms, mode, outcome, cause,
         decision, reason, rule_baseline, call_started_at_ms, cost_microusd,
         author_decision, author_reason, decided_at_ms
         FROM auto_review WHERE requested_at_ms >= ?1 OR call_started_at_ms >= ?1
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
        })
    })?
    .collect()
}

impl Reader {
    /// Every review row requested since `since_ms`, newest first.
    pub fn review_rows(&self, since_ms: i64) -> rusqlite::Result<Vec<ReviewRow>> {
        review_rows(self.conn(), since_ms)
    }
}

#[cfg(feature = "window-authority")]
mod writer {
    use std::collections::BTreeSet;

    use rusqlite::params;

    use super::*;
    use crate::registry::store::Writer;
    use crate::registry::write::RegistryError;

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

        /// Every review row requested, or whose call started, since
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
    }
}
